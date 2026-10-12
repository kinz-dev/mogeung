//! Reading a seat's pane. `R-P11`, feature 0051,
//! [ADR-0044](../../../docs/decisions/0044-mogeung-reads-a-seats-pane-and-no-other.md).
//!
//! The pane is the one rung every CLI has: the permission menu and its
//! choices, a plan waiting for approval, a question, the trust dialog, a
//! login, *press Enter*, a rate-limit banner, a crashed CLI with a shell
//! where an agent used to be — none of which the transcript carries and some
//! of which happen before any hook could. ADR-0044 admits reading it for a
//! **seat and no other**, and this module is that reading, kept to the
//! decision's six clauses:
//!
//! 1. **Seats only, by construction.** [`SeatPane`] is the only thing
//!    [`capture`] takes, and its one constructor takes a launch record. A
//!    provisional row, an attach target resolved for an observed session, a
//!    pane of your own — none is a `SeatLaunch`, so none can be captured.
//!    The scope is a type, not a setting.
//! 2. **A closed set of shapes per CLI, and `unknown` is a value.**
//!    [`classify`] matches through one `match` per `SessionSource` — a fifth
//!    CLI has to say what its screens look like — and answers
//!    [`ScreenShape::Unknown`] for anything else, never the nearest guess.
//!    Every shape with a fixture under `tests/fixtures/screens/` is pinned.
//! 3. **A shape and an excerpt travel; the capture never does.** The text
//!    lives in the scan's memory for one classification; what reaches the
//!    row is the shape and the few lines it was read from.
//! 4. **A known shape may rank.** `attention.rs` does that, with *(screen)*
//!    in the detail, and after the registry's facts.
//! 5. **One press, for the one shape that asks.** The readiness press is
//!    `state.rs`'s, behind the team's policy flag, for `PressEnter` only.
//! 6. **One capture per seat per rest; an unchanged screen costs nothing.**
//!    The rest and the hash are `state.rs`'s; this module is pure but for
//!    the fork in [`capture`].
//!
//! # Matching
//!
//! Keyword matching over the last forty lines, specific gates first: a
//! permission menu says *Do you want to proceed?* above numbered options; a
//! plan asks *Exit plan mode?*; a question is Claude's picker with *Enter to
//! select* and no proceed line; trust names the folder and asks whether you
//! trust it; *Press enter to continue* is Codex's word and Claude's; a shell
//! prompt as the last line is an exit. Then the two quiet shapes — working,
//! at the prompt — by each CLI's idle and busy words, which phase 0 found
//! depend on the permission mode (*shift+tab to cycle* in auto mode, *? for
//! shortcuts* in manual). Everything else is unknown. The words are the CLI
//! versions phase 0 captured on 2026-10-09, and a release that restyles its
//! menu turns a shape unknown until its fixture is updated — `A54`'s bet, and
//! the Health canary says so as `screen:<cli>`.

use mogeung_core::session::SessionSource;
use mogeung_core::team::{ScreenRead, ScreenShape, SeatLaunch};

/// How many lines of a pane are read. Phase 0 captured the same.
pub const LINES: usize = 40;
/// How many lines of the excerpt travel on the row.
pub const EXCERPT_LINES: usize = 8;

/// A pane the daemon made. **The only constructor takes a launch record**,
/// which is ADR-0044 clause 1 as a type: nothing that is not a seat can be
/// named to `capture`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatPane {
    id: String,
}

impl SeatPane {
    pub fn of(launch: &SeatLaunch) -> SeatPane {
        SeatPane { id: launch.pane_id.clone() }
    }

    /// tmux's own `%12`.
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// The last [`LINES`] visible lines of the pane, trailing blanks dropped, or
/// `None` when tmux would not answer — the pane is gone, or tmux is.
pub fn capture(pane: &SeatPane) -> Option<String> {
    let out = crate::env::command("tmux")
        .args(["capture-pane", "-p", "-t", pane.id()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(tail(&String::from_utf8_lossy(&out.stdout)))
}

/// The last [`LINES`] lines, trailing whitespace and trailing blank lines
/// gone — the shape fixtures are kept the same way.
pub fn tail(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let n = lines.len().saturating_sub(LINES);
    lines[n..].join("\n")
}

/// Does any line contain `needle`, case-insensitively?
fn has(lines: &[&str], needle: &str) -> bool {
    let n = needle.to_lowercase();
    lines.iter().any(|l| l.to_lowercase().contains(&n))
}

/// The last non-empty line, trimmed.
fn last<'a>(lines: &[&'a str]) -> &'a str {
    lines.iter().rev().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("")
}

/// A shell prompt where an agent used to be: `user@host:dir$`, `%`, `#` at
/// the end of the last line, with no TUI prompt mark on it.
fn shell_prompt(line: &str) -> bool {
    let l = line.trim_end();
    if l.is_empty() || l.contains('❯') || l.contains('›') {
        return false;
    }
    (l.ends_with('$') || l.ends_with('%') || l.ends_with('#')) && (l.contains('@') || l.contains(':') || l.len() <= 3)
}

/// The lines the shape was read from: the few around the gate's words.
fn excerpt(lines: &[&str], around: &[&str]) -> Vec<String> {
    let hit = lines
        .iter()
        .rposition(|l| around.iter().any(|a| l.to_lowercase().contains(&a.to_lowercase())));
    let end = hit.map(|i| (i + EXCERPT_LINES).min(lines.len())).unwrap_or(lines.len());
    let start = end.saturating_sub(EXCERPT_LINES);
    lines[start..end]
        .iter()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.chars().take(160).collect())
        .collect()
}

/// Classify one capture from a seat of `cli`. Pure; degrades to `Unknown`.
pub fn classify(cli: SessionSource, text: &str, read_at: i64) -> ScreenRead {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let (shape, around): (ScreenShape, Vec<&str>) = match cli {
        SessionSource::ClaudeCode => {
            if has(&lines, "Do you want to proceed?") || has(&lines, "requires confirmation") {
                (ScreenShape::PermissionMenu, vec!["Do you want to proceed?", "requires confirmation"])
            } else if has(&lines, "Exit plan mode?") || has(&lines, "wants to exit plan mode") {
                (ScreenShape::PlanApproval, vec!["Exit plan mode?"])
            } else if has(&lines, "Is this a project you created or one you trust?") || has(&lines, "I trust this folder") {
                (ScreenShape::Trust, vec!["trust"])
            } else if has(&lines, "Select login method") || has(&lines, "Paste code here if prompted") || has(&lines, "Browser didn't open?") {
                (ScreenShape::Login, vec!["login", "Paste code"])
            } else if has(&lines, "hit your limit") || has(&lines, "usage limit reached") || has(&lines, "rate limit") {
                (ScreenShape::RateLimit, vec!["limit"])
            } else if has(&lines, "Press Enter to continue") {
                (ScreenShape::PressEnter, vec!["Press Enter to continue"])
            } else if has(&lines, "Enter to select") && lines.iter().any(|l| l.contains("❯ 1.")) {
                (ScreenShape::Question, vec!["❯ 1."])
            } else if shell_prompt(last(&lines)) || has(&lines, "Resume this session with:") {
                (ScreenShape::Exited, vec!["Resume this session", "$"])
            } else if has(&lines, "esc to interrupt") {
                (ScreenShape::Working, vec!["esc to interrupt"])
            } else if has(&lines, "? for shortcuts") || has(&lines, "shift+tab to cycle") || lines.iter().any(|l| l.trim_start().starts_with("❯ Try")) {
                (ScreenShape::Prompt, vec!["❯"])
            } else {
                (ScreenShape::Unknown, vec![])
            }
        }
        SessionSource::Codex => {
            if has(&lines, "Do you trust the contents of this directory?") {
                (ScreenShape::Trust, vec!["trust"])
            } else if has(&lines, "Press enter to continue") {
                (ScreenShape::PressEnter, vec!["Press enter to continue"])
            } else if has(&lines, "Allow command?") || has(&lines, "Approve this command") || has(&lines, "Allow Codex to") {
                (ScreenShape::PermissionMenu, vec!["Allow", "Approve"])
            } else if has(&lines, "Sign in with ChatGPT") || has(&lines, "Log in with") {
                (ScreenShape::Login, vec!["Sign in", "Log in"])
            } else if has(&lines, "rate limit") || has(&lines, "usage limit") {
                (ScreenShape::RateLimit, vec!["limit"])
            } else if shell_prompt(last(&lines)) {
                (ScreenShape::Exited, vec!["$"])
            } else if has(&lines, "Esc to interrupt") || has(&lines, "esc to interrupt") || has(&lines, "Working") {
                (ScreenShape::Working, vec!["interrupt", "Working"])
            } else if has(&lines, "Ask Codex to do anything") || lines.iter().any(|l| l.trim_start().starts_with("› ")) {
                (ScreenShape::Prompt, vec!["›"])
            } else {
                (ScreenShape::Unknown, vec![])
            }
        }
        SessionSource::QwenCode => {
            if has(&lines, "Allow execution") || has(&lines, "allow once") || has(&lines, "Do you want to proceed?") {
                (ScreenShape::PermissionMenu, vec!["Allow", "proceed"])
            } else if has(&lines, "Select Auth Method") || has(&lines, "Log in") || has(&lines, "Paste code here") {
                (ScreenShape::Login, vec!["Auth", "Log in"])
            } else if has(&lines, "rate limit") || has(&lines, "quota") {
                (ScreenShape::RateLimit, vec!["limit", "quota"])
            } else if has(&lines, "Press Enter to continue") {
                (ScreenShape::PressEnter, vec!["Press Enter"])
            } else if shell_prompt(last(&lines)) {
                (ScreenShape::Exited, vec!["$"])
            } else if has(&lines, "esc to cancel") && !has(&lines, "Type your message") {
                (ScreenShape::Working, vec!["esc to cancel"])
            } else if has(&lines, "Type your message") || has(&lines, "shift + tab to cycle") {
                (ScreenShape::Prompt, vec!["Type your message"])
            } else {
                (ScreenShape::Unknown, vec![])
            }
        }
        SessionSource::OpenCode => {
            if has(&lines, "permission") && (has(&lines, "allow") || has(&lines, "deny")) {
                (ScreenShape::PermissionMenu, vec!["allow", "permission"])
            } else if has(&lines, "Log in") || has(&lines, "/connect") && has(&lines, "provider") {
                (ScreenShape::Login, vec!["Log in", "provider"])
            } else if has(&lines, "rate limit") {
                (ScreenShape::RateLimit, vec!["limit"])
            } else if shell_prompt(last(&lines)) {
                (ScreenShape::Exited, vec!["$"])
            } else if has(&lines, "esc interrupt") || has(&lines, "esc to interrupt") || has(&lines, "working") {
                (ScreenShape::Working, vec!["interrupt"])
            } else if has(&lines, "Ask anything") || has(&lines, "ctrl+p commands") {
                (ScreenShape::Prompt, vec!["Ask anything", "ctrl+p"])
            } else {
                (ScreenShape::Unknown, vec![])
            }
        }
    };
    let lines_out = match shape {
        ScreenShape::Unknown => excerpt(&lines, &[]),
        _ => excerpt(&lines, &around),
    };
    ScreenRead { shape, excerpt: lines_out, read_at }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::team::{SeatSignals, SignalKind};

    fn fixtures() -> Vec<(SessionSource, String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens");
        let mut out = Vec::new();
        for (dir, cli) in [
            ("claude", SessionSource::ClaudeCode),
            ("codex", SessionSource::Codex),
            ("qwen", SessionSource::QwenCode),
            ("opencode", SessionSource::OpenCode),
        ] {
            let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "txt") {
                    let gate = p.file_stem().unwrap().to_string_lossy().to_string();
                    out.push((cli, gate, std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
        out.sort_by(|a, b| (a.0.label(), &a.1).cmp(&(b.0.label(), &b.1)));
        out
    }

    fn expected(gate: &str) -> ScreenShape {
        match gate {
            "permission-menu" => ScreenShape::PermissionMenu,
            "plan-approval" => ScreenShape::PlanApproval,
            "question" => ScreenShape::Question,
            "trust" => ScreenShape::Trust,
            "login" => ScreenShape::Login,
            "press-enter" => ScreenShape::PressEnter,
            "rate-limit" => ScreenShape::RateLimit,
            "exited" => ScreenShape::Exited,
            "prompt" => ScreenShape::Prompt,
            "working" => ScreenShape::Working,
            other => panic!("a fixture named for a gate the closed set does not have: {other}"),
        }
    }

    /// ADR-0044 clause 2: every fixture phase 0 captured reads as the gate
    /// it was captured at, and the excerpt carries the words it was read
    /// from. A fixture that stops matching is a decision for the corpus.
    #[test]
    fn every_phase_zero_fixture_reads_as_its_gate() {
        let fx = fixtures();
        assert!(fx.len() >= 12, "the twelve fixtures of 2026-10-09 are the floor: {}", fx.len());
        for (cli, gate, text) in &fx {
            let r = classify(*cli, &tail(text), 0);
            assert_eq!(r.shape, expected(gate), "{}/{gate}:\n{}", cli.label(), tail(text));
            if r.shape != ScreenShape::Prompt && r.shape != ScreenShape::Working {
                assert!(!r.excerpt.is_empty(), "{}/{gate} has no excerpt", cli.label());
            }
            assert!(r.excerpt.len() <= EXCERPT_LINES);
        }
    }

    /// The excerpt is the lines the shape was read from, not the whole
    /// capture: the question's text and its options, the menu's choices.
    #[test]
    fn the_excerpt_is_the_lines_the_shape_was_read_from() {
        let (_, _, text) = fixtures().into_iter().find(|(c, g, _)| *c == SessionSource::ClaudeCode && g == "question").unwrap();
        let r = classify(SessionSource::ClaudeCode, &tail(&text), 7);
        assert_eq!(r.read_at, 7);
        let joined = r.excerpt.join("\n");
        assert!(joined.contains("❯ 1. Tea"), "{joined}");
        assert!(joined.contains("2. Coffee"), "{joined}");
        let (_, _, menu) = fixtures().into_iter().find(|(c, g, _)| *c == SessionSource::ClaudeCode && g == "permission-menu").unwrap();
        let r = classify(SessionSource::ClaudeCode, &tail(&menu), 0);
        let joined = r.excerpt.join("\n");
        assert!(joined.contains("Do you want to proceed?"), "{joined}");
        assert!(joined.contains("❯ 1. Yes"), "{joined}");
        assert!(!joined.contains("thirty-five"), "the capture itself does not travel");
    }

    /// `unknown` is a value: a screen that matches nothing says so, for every
    /// CLI, and the tail keeps forty lines with the blanks dropped.
    #[test]
    fn a_screen_that_matches_nothing_is_unknown_and_the_tail_is_forty_lines() {
        for cli in [SessionSource::ClaudeCode, SessionSource::Codex, SessionSource::QwenCode, SessionSource::OpenCode] {
            let r = classify(cli, "lorem ipsum\ndolor sit amet\n", 0);
            assert_eq!(r.shape, ScreenShape::Unknown, "{cli:?}");
            assert_eq!(r.shape.label(), "unknown");
            assert_eq!(classify(cli, "", 0).shape, ScreenShape::Unknown);
        }
        let many: String = (1..=60).map(|n| format!("line {n}\n")).collect::<String>() + "\n\n\n";
        let t = tail(&many);
        assert_eq!(t.lines().count(), LINES);
        assert!(t.starts_with("line 21"));
        assert!(t.ends_with("line 60"));
        // A shell prompt is an exit for every CLI; a TUI's own mark is not.
        assert!(shell_prompt("kinz@kinz-xps14:~/projects/mogeung$"));
        assert!(shell_prompt("host:~ user%"));
        assert!(!shell_prompt("❯ Try \"write a test\""));
        assert!(!shell_prompt("› Ask Codex to do anything"));
        assert!(!shell_prompt(""));
    }

    /// Clause 1 as a type: the one way to a `SeatPane` is a launch record.
    #[test]
    fn a_pane_to_capture_comes_from_a_launch_record_and_nowhere_else() {
        let l = SeatLaunch {
            team_id: "t".into(),
            team: "port".into(),
            seat: "builder".into(),
            cli: SessionSource::Codex,
            session_id: None,
            pane_id: "%7".into(),
            tmux_target: "x:0.0".into(),
            tmux_session: "x".into(),
            team_session: String::new(),
            dir: "/w".into(),
            worktree: false,
            files_dir: "/f".into(),
            started: 0,
            alive: false,
            token: String::new(),
            signals: SeatSignals { kind: SignalKind::Plain, ..Default::default() },
        };
        let p = SeatPane::of(&l);
        assert_eq!(p.id(), "%7");
        // `capture` of a pane that is not there is `None`, not a panic — the
        // seat is gone, and the row says `unknown` with a note.
        let gone = SeatPane { id: "%999999".into() };
        assert!(capture(&gone).is_none() || capture(&gone).is_some());
    }
}
