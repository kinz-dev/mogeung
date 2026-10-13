//! Team definitions: kept, checked and mirrored. `R-P3`, feature 0051.
//!
//! A team is the second kind of thing this daemon stores that it could not
//! recompute — the first is a note, and
//! [ADR-0015](../../../docs/decisions/0015-markdown-is-the-truth.md) applies
//! to both: daemon-owned, in the store, served over the wire to any client,
//! and **mirrored one way** as markdown under `~/.mogeung/teams/` so the user's
//! writing is never trapped in a database only mogeung can open.
//! [ADR-0043](../../../docs/decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md)
//! clause 8 says so for a definition in terms, and says the opposite for a
//! launch record, which is derived state and is not mirrored.
//!
//! # The mirror is never read back
//!
//! As for notes: editing the `.md` changes nothing, and the daemon does not
//! watch the directory. The window is where a team is edited (*"UI come
//! first for the team config"*, 2026-10-09); a file in and a file out are
//! `R-P12`'s import and export, and an OpenRig `rig.yaml` arrives through that
//! door as other people's configuration, never as this format.
//!
//! # What is checked here, and what is not
//!
//! [`validate`] refuses a definition that could not be *addressed*: a seat
//! with no name, two seats with one name, a wake aimed at a seat the team
//! does not have, a cap of zero. It does **not** check that the repository
//! exists or that the CLI is installed — a definition may be written before
//! either is true on this machine, and the launcher (`R-P4`) is where those
//! refusals belong, at the moment they would matter and in words that say
//! which it was.

use anyhow::{bail, Result};
use mogeung_core::session::SessionSource;
use mogeung_core::team::{Effort, Seat, Team};
use std::path::{Path, PathBuf};

/// Where the mirror lives.
pub fn mirror_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("teams")
}

/// A fresh id: milliseconds and a little entropy, the way a note's is.
pub fn new_id() -> String {
    crate::notes::new_id()
}

/// A filesystem-safe stem from the team's name. Browsability only: the id is
/// the identity, and the slug may change when the team is renamed.
pub fn slug(name: &str) -> String {
    let s = crate::notes::slug(name);
    if s == "note" {
        "team".to_string()
    } else {
        s
    }
}

/// A seat name is an address and a directory name: lower-case ASCII letters,
/// digits and dashes, starting with a letter.
pub fn valid_seat_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    name.len() <= 32 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Why a seat's thinking level cannot be handed to its CLI, or `None`.
/// `R-P16`. The window's `firstProblem` says the same words.
pub fn effort_problem(s: &Seat) -> Option<String> {
    let e = s.effort?;
    let taken = Effort::taken_by(s.cli);
    if taken.is_empty() {
        return Some(format!(
            "seat `{}`: {} takes no thinking level at launch — leave it at the CLI's default",
            s.name,
            s.cli.label()
        ));
    }
    if !taken.contains(&e) {
        return Some(format!("seat `{}`: {} has no `{}` thinking level", s.name, s.cli.label(), e.as_str()));
    }
    if s.cli == SessionSource::OpenCode && s.model.as_deref().is_none_or(|m| m.trim().is_empty()) {
        return Some(format!(
            "seat `{}`: opencode honours a thinking level only on a model it is told — name the model too",
            s.name
        ));
    }
    None
}

/// Refuse what could not be addressed or counted. The reason names the field.
pub fn validate(team: &Team) -> Result<()> {
    if team.name.trim().is_empty() {
        bail!("a team needs a name");
    }
    if team.name.chars().count() > 64 {
        bail!("a team's name is at most 64 characters");
    }
    if !team.repo.trim().is_empty() && !Path::new(&team.repo).is_absolute() {
        bail!("the team's repository must be an absolute path, not `{}`", team.repo);
    }
    if team.policy.hourly_cap == 0 {
        bail!("the hourly cap must be at least one — pause the team to stop every automatic send");
    }
    if team.policy.hop_budget > 100 {
        bail!("a hop budget over a hundred is a budget in name only");
    }
    if team.policy.wind_down.trim().is_empty() {
        bail!("the wind-down message is empty — a seat asked for nothing writes nothing");
    }
    if !(1..=60).contains(&team.policy.wind_down_minutes) {
        bail!("a wind-down waits between one and sixty minutes, not {}", team.policy.wind_down_minutes);
    }
    let mut seen = std::collections::HashSet::new();
    for s in &team.seats {
        if !valid_seat_name(&s.name) {
            bail!(
                "seat name `{}` must be lower-case letters, digits and dashes, starting with a letter",
                s.name
            );
        }
        if !seen.insert(s.name.as_str()) {
            bail!("two seats are named `{}`", s.name);
        }
        if let Some(problem) = effort_problem(s) {
            bail!("{problem}");
        }
    }
    for w in &team.policy.wakes {
        if team.seat(&w.on_seat).is_none() {
            bail!("a wake watches `{}`, which is not a seat of this team", w.on_seat);
        }
        if team.seat(&w.to).is_none() {
            bail!("a wake addresses `{}`, which is not a seat of this team", w.to);
        }
        if w.on_seat == w.to {
            bail!("a wake on `{}` would send to itself", w.to);
        }
        if w.text.trim().is_empty() {
            bail!("a wake from `{}` to `{}` has no text to send", w.on_seat, w.to);
        }
    }
    for d in &team.policy.watchdogs {
        if d.silent_minutes == 0 {
            bail!("a watchdog needs at least one silent minute");
        }
        if let Some(seat) = &d.seat {
            if team.seat(seat).is_none() {
                bail!("a watchdog names `{seat}`, which is not a seat of this team");
            }
        }
        if d.text.trim().is_empty() {
            bail!("a watchdog has no text to send");
        }
    }
    Ok(())
}

fn mirror_path(dir: &Path, t: &Team) -> PathBuf {
    dir.join(format!("{}-{}.md", t.id, slug(&t.name)))
}

/// The markdown a team mirrors to. Pure, so a test can read it without a
/// directory; the front matter says what it is and the sections say what a
/// seat will be started with, in the order the launcher applies them.
pub fn render(t: &Team) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    out.push_str("---\n");
    let _ = writeln!(out, "id: {}", t.id);
    let _ = writeln!(out, "name: {}", t.name);
    if !t.repo.is_empty() {
        let _ = writeln!(out, "repo: {}", t.repo);
    }
    let _ = writeln!(out, "paused: {}", t.paused);
    let _ = writeln!(out, "updated: {}", t.updated);
    out.push_str("---\n\n");
    let _ = writeln!(out, "# Team: {}\n", t.name);
    let _ = writeln!(out, "Default permission mode: `{}`\n", t.permission.label());
    out.push_str("## Guidance — every seat gets this\n\n");
    out.push_str(if t.guidance.trim().is_empty() { "(none)\n" } else { t.guidance.trim_end() });
    out.push_str("\n\n## Policy\n\n");
    let p = &t.policy;
    let _ = writeln!(out, "- hop budget: {} (0 = every message through you)", p.hop_budget);
    let _ = writeln!(out, "- automatic sends per hour: {}", p.hourly_cap);
    let _ = writeln!(out, "- handover: {:?}", p.handover);
    let _ = writeln!(out, "- readiness press: {}", if p.readiness_press { "on" } else { "off" });
    let _ = writeln!(out, "- wind down: {} minute(s) to answer; the message: {:?}", p.wind_down_minutes, p.wind_down);
    if !p.wakes.is_empty() {
        out.push_str("\n### Wakes — declared text\n\n");
        for w in &p.wakes {
            let _ = writeln!(out, "- on `{}` {:?} → `{}`: {:?}", w.on_seat, w.on, w.to, w.text);
        }
    }
    if !p.watchdogs.is_empty() {
        out.push_str("\n### Watchdogs — declared text\n\n");
        for d in &p.watchdogs {
            let _ = writeln!(
                out,
                "- {} silent {} min{} → {:?}",
                d.seat.as_deref().map(|s| format!("`{s}`")).unwrap_or_else(|| "any seat".into()),
                d.silent_minutes,
                if d.while_busy { " while busy" } else { "" },
                d.text
            );
        }
    }
    out.push_str("\n## Seats\n");
    for s in &t.seats {
        let _ = writeln!(out, "\n### {}\n", s.name);
        if !s.label.trim().is_empty() {
            let _ = writeln!(out, "- label: {}", s.label);
        }
        let _ = writeln!(out, "- cli: {}", s.cli.label());
        if let Some(m) = &s.model {
            let _ = writeln!(out, "- model: {m}");
        }
        if let Some(e) = s.effort {
            let _ = writeln!(out, "- thinking: {}", e.as_str());
        }
        let _ = writeln!(out, "- works in: {:?}", s.works_in);
        let _ = writeln!(
            out,
            "- permission: {}{}",
            t.permission_for(s).label(),
            if s.permission.is_none() { " (the team's)" } else { "" }
        );
        if s.own_home {
            out.push_str("- own home: yes (a blank seat, ADR-0043 clause 5)\n");
        }
        if !s.guidance.trim().is_empty() {
            let _ = writeln!(out, "\n{}", s.guidance.trim_end());
        }
    }
    out
}

/// Write one team to the mirror, replacing whatever it was called before.
///
/// Best effort by design, as for a note: the team is already in the store
/// when this runs, and a directory that cannot be written must not cost the
/// user the definition it was meant to protect.
pub fn mirror_in(dir: &Path, t: &Team) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let _ = unmirror_in(dir, &t.id);
    std::fs::write(mirror_path(dir, t), render(t))?;
    Ok(())
}

/// Remove every mirror file for this id, whatever it was last called.
pub fn unmirror_in(dir: &Path, id: &str) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let prefix = format!("{id}-");
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.ends_with(".md") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    Ok(())
}

/// Shown beside a seat before it is booted: what signals this CLI gives a
/// seat without touching the user's home, and what needs the opt-in home.
/// Static, because it is a property of the CLI (ADR-0043's table), and in
/// core's vocabulary so the window can say the same thing.
pub fn signals_for(seat: &Seat) -> &'static str {
    use mogeung_core::session::SessionSource::*;
    match (seat.cli, seat.own_home) {
        (ClaudeCode, _) => "hooks via --settings, a status line, the screen — none touch your home",
        (OpenCode, _) => "hooks via a plugin file mogeung owns, named in its inline config; the screen",
        (QwenCode, true) => "hooks in the seat's own home, the screen",
        (QwenCode, false) => "the screen only — hooks need a home of this seat's own (ADR-0043 clause 5)",
        // `R-P5` built Claude's, Qwen's and opencode's; Codex's hooks live in
        // `config.toml` beside trust hashes, and writing them into a home of
        // mogeung's own is a format nobody here has read yet.
        (Codex, _) => {
            "the screen only — Codex's hooks live in its config with trust hashes, not yet written even in a home of its own"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::session::SessionSource;
    use mogeung_core::team::{Policy, SeatEvent, Wake, Watchdog};

    fn team() -> Team {
        Team {
            id: "t1".into(),
            name: "port openrig".into(),
            repo: "/home/you/mogeung".into(),
            guidance: "Read the spec first.".into(),
            permission: mogeung_core::team::PermissionMode::AcceptEdits,
            policy: Policy::default(),
            seats: vec![
                Seat::new("planner", SessionSource::ClaudeCode),
                Seat::new("builder", SessionSource::Codex),
            ],
            paused: false,
            created: 1,
            updated: 2,
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mogeung-teams-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// `R-P16`: a level the CLI cannot be handed is refused in words, at
    /// save, rather than dropped at boot.
    #[test]
    fn a_thinking_level_its_cli_cannot_take_is_refused_by_name() {
        let mut t = team();
        let mut editor = Seat::new("editor", SessionSource::ClaudeCode);
        editor.model = Some("opus".into());
        editor.effort = Some(Effort::High);
        let mut translator = Seat::new("translator", SessionSource::ClaudeCode);
        translator.model = Some("claude-sonnet-5".into());
        translator.effort = Some(Effort::Medium);
        let report = Seat::new("report", SessionSource::OpenCode);
        t.seats = vec![editor, translator, report];
        validate(&t).expect("the asked-for team");

        t.seats[0].effort = Some(Effort::Ultra);
        let why = validate(&t).unwrap_err().to_string();
        assert!(why.contains("`editor`") && why.contains("ultra"), "{why}");
        t.seats[0].effort = Some(Effort::High);

        let mut qwen = Seat::new("tester", SessionSource::QwenCode);
        qwen.effort = Some(Effort::Low);
        t.seats.push(qwen);
        let why = validate(&t).unwrap_err().to_string();
        assert!(why.contains("takes no thinking level"), "{why}");
        t.seats.pop();

        t.seats[2].effort = Some(Effort::High);
        let why = validate(&t).unwrap_err().to_string();
        assert!(why.contains("name the model too"), "{why}");
        t.seats[2].model = Some("anthropic/claude-opus-5-5".into());
        validate(&t).expect("opencode with a model and a level");
    }

    #[test]
    fn a_seat_name_is_an_address() {
        assert!(valid_seat_name("reviewer"));
        assert!(valid_seat_name("builder-2"));
        assert!(!valid_seat_name(""));
        assert!(!valid_seat_name("Reviewer"));
        assert!(!valid_seat_name("2nd"));
        assert!(!valid_seat_name("a b"));
        assert!(!valid_seat_name("../x"));
    }

    #[test]
    fn a_definition_that_cannot_be_addressed_is_refused_by_name() {
        let mut t = team();
        t.seats.push(Seat::new("builder", SessionSource::QwenCode));
        assert!(validate(&t).unwrap_err().to_string().contains("two seats are named `builder`"));

        let mut t = team();
        t.policy.wakes.push(Wake {
            on_seat: "builder".into(),
            on: SeatEvent::Stop,
            to: "reviewer".into(),
            text: "go".into(),
        });
        assert!(validate(&t).unwrap_err().to_string().contains("`reviewer`"));

        let mut t = team();
        t.policy.wakes.push(Wake {
            on_seat: "builder".into(),
            on: SeatEvent::Stop,
            to: "builder".into(),
            text: "go".into(),
        });
        assert!(validate(&t).unwrap_err().to_string().contains("itself"));

        let mut t = team();
        t.policy.hourly_cap = 0;
        assert!(validate(&t).unwrap_err().to_string().contains("hourly cap"));

        let mut t = team();
        t.repo = "relative/path".into();
        assert!(validate(&t).unwrap_err().to_string().contains("absolute"));

        let mut t = team();
        t.policy.watchdogs.push(Watchdog {
            seat: None,
            silent_minutes: 0,
            while_busy: true,
            text: "status?".into(),
        });
        assert!(validate(&t).unwrap_err().to_string().contains("silent minute"));

        assert!(validate(&team()).is_ok());
        // A team with no seats yet is a team being written, and is kept.
        let mut t = team();
        t.seats.clear();
        assert!(validate(&t).is_ok());
    }

    /// The mirror shows the two layers apart and the inherited mode as the
    /// team's, so a reader of the file sees what a seat is started with.
    #[test]
    fn the_mirror_says_what_each_seat_gets() {
        let mut t = team();
        t.seats[1].permission = Some(mogeung_core::team::PermissionMode::Plan);
        t.seats[1].guidance = "Build only.".into();
        t.policy.wakes.push(Wake {
            on_seat: "builder".into(),
            on: SeatEvent::Stop,
            to: "planner".into(),
            text: "builder finished".into(),
        });
        let md = render(&t);
        assert!(md.contains("name: port openrig"), "{md}");
        assert!(md.contains("Default permission mode: `acceptEdits`"), "{md}");
        assert!(md.contains("Read the spec first."), "{md}");
        assert!(md.contains("- permission: acceptEdits (the team's)"), "{md}");
        assert!(md.contains("- permission: plan\n"), "{md}");
        assert!(md.contains("Build only."), "{md}");
        assert!(md.contains("on `builder` Stop → `planner`: \"builder finished\""), "{md}");
        assert!(md.contains("- hop budget: 3"), "{md}");
        assert!(md.contains("- automatic sends per hour: 60"), "{md}");
    }

    #[test]
    fn re_mirroring_leaves_exactly_one_file_per_team() {
        let dir = scratch("rename");
        let mut t = team();
        mirror_in(&dir, &t).unwrap();
        t.name = "renamed team".into();
        mirror_in(&dir, &t).unwrap();
        let files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].starts_with("t1-renamed-team"), "{files:?}");
        unmirror_in(&dir, "t1").unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_signals_a_seat_gets_follow_the_cli_and_the_home() {
        let mut s = Seat::new("b", SessionSource::QwenCode);
        assert!(signals_for(&s).contains("screen only"));
        s.own_home = true;
        assert!(signals_for(&s).contains("own home"));
        assert!(signals_for(&Seat::new("p", SessionSource::ClaudeCode)).contains("--settings"));
        assert!(signals_for(&Seat::new("o", SessionSource::OpenCode)).contains("plugin"));
        // `R-P5`: a Codex seat is plain whatever its home, and the sentence
        // says why rather than promising hooks nobody has written.
        let mut c = Seat::new("c", SessionSource::Codex);
        assert!(signals_for(&c).contains("screen only"));
        c.own_home = true;
        assert!(signals_for(&c).contains("screen only") && signals_for(&c).contains("not yet"));
    }
}
