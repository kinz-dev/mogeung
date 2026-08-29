//! Why was this line changed, and is the answer actually in the transcript?
//! `R-O4`, and [A36](../../../docs/product/assumptions.md)'s test before it.
//!
//! `R-O4`'s claim is that **nothing else can do this**: a review tool has the
//! diff, an IDE has the code, and only mogeung has the conversation that wrote
//! the line sitting beside it. The claim is about the product. `A36` is the bet
//! underneath it and it is a different sentence — *the rationale for a change is
//! in the transcript that produced it, **and is reachable from the line***.
//!
//! **The structure is not the bet.** `R-F2` links a file to the sessions that
//! touched it, `R-F9` and [`crate::insight::turns_near`] link a moment to its
//! turns, and all of it works today. The bet is that the turns near a line
//! contain the *why*, and the reason to doubt it was written down before it
//! could be discovered: an assistant narrates **what it did** far more often
//! than why, and the why is usually in the human's prompt several turns
//! earlier. That would make this a retrieval pointing at the wrong end of the
//! conversation — a fixable design error wearing an assumption's clothes.
//!
//! So this module holds **two** retrievals and no opinion about which wins.
//! `--bin why` asks the same question through both and prints where the answers
//! came from. Whichever one the corpus picks is the one `R-O4`'s panel gets,
//! and it will be this code rather than a second copy of it — `R-O3` bought
//! that rule at the price of a harness that graded a prompt which never
//! shipped.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::insight::{assistant_text, human_prompt, parse_ts, str_at, stream_lines, tool_use_blocks};

/// The tools that write a file. `Read` is not one of them: a line that was
/// read is not a line that was changed, and including it would answer *why*
/// from a turn that changed nothing.
pub const EDIT_TOOLS: [&str; 3] = ["Edit", "Write", "NotebookEdit"];

/// The most of one turn a model is shown.
///
/// Larger than [`crate::insight::turns_near`]'s preview by design: 200
/// characters is enough to *recognise* a turn in a list and not enough to
/// contain a reason. The tail is what is kept when a turn is cut, because a
/// prompt's ask is usually at its end and an assistant's conclusion always is.
pub const TURN_CHARS: usize = 1200;

/// The whole ask's budget. A conversation is unbounded and a prompt is not —
/// `R-O3` learnt this against a 280-file diff that answered nothing after 78
/// seconds, and a long session is the same shape of failure.
pub const TOTAL_CHARS: usize = 12_000;

/// One moment where a session's tool wrote a file.
#[derive(Debug, Clone)]
pub struct EditMoment {
    /// 1-based line in the transcript — the anchor everything else is relative to.
    pub line: u64,
    pub timestamp: DateTime<Utc>,
    /// The path as the tool named it, absolute as the agent saw it.
    pub path: String,
    pub tool: String,
}

/// One turn, as the model is shown it.
#[derive(Debug, Clone)]
pub struct Turn {
    pub line: u64,
    /// `user` or `assistant`.
    pub role: String,
    pub text: String,
    /// What opens the Transcript at this moment. A transcript line number is
    /// not a location any client can use; a timestamp is `R-F9`'s own route.
    pub timestamp: DateTime<Utc>,
}

/// Which retrieval produced a set of turns. `A36`'s two candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `R-F9`'s: the turns nearest the moment in **time**, either side of it.
    /// What the machinery does today, and what `R-O4` would inherit by
    /// default.
    Nearest,
    /// The human prompt at or before the edit, and everything between it and
    /// the edit. The shape `A36`'s stated doubt implies is the right one.
    LeadingUp,
}

impl Shape {
    pub fn label(self) -> &'static str {
        match self {
            Shape::Nearest => "nearest-in-time",
            Shape::LeadingUp => "leading-up",
        }
    }
}

/// Every moment a transcript wrote a file, oldest first.
///
/// `path_suffix` narrows to one file — matched as a **suffix**, because the
/// tool records the path the agent saw and the caller has the one git shows,
/// which differ by the worktree root. That is `R-F2`'s existing match rule and
/// its known weakness (`A8`) travels with it: two worktrees ending in the same
/// path cannot be told apart here.
pub fn edit_moments(transcript: &Path, path_suffix: Option<&str>) -> Vec<EditMoment> {
    let mut out = Vec::new();
    stream_lines(transcript, |line_no, line| {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return true };
        let Some(ts) = parse_ts(&v) else { return true };
        for b in tool_use_blocks(&v) {
            let Some(tool) = str_at(b, "name") else { continue };
            if !EDIT_TOOLS.contains(&tool) {
                continue;
            }
            let Some(path) = b.get("input").and_then(|i| str_at(i, "file_path")) else { continue };
            if let Some(want) = path_suffix {
                if !(path == want || path.ends_with(want)) {
                    continue;
                }
            }
            out.push(EditMoment {
                line: line_no,
                timestamp: ts,
                path: path.to_string(),
                tool: tool.to_string(),
            });
        }
        true
    });
    out
}

/// The turns a question about `at` is answered from, oldest first.
///
/// Both shapes return the turns in **transcript order** rather than in
/// relevance order, because a conversation read out of order is a different
/// conversation — and the line numbers travel so an answer can cite them and a
/// reader can open the Transcript there.
pub fn turns_for(transcript: &Path, at: &EditMoment, shape: Shape, k: usize) -> Vec<Turn> {
    let all = collect_turns(transcript);
    match shape {
        Shape::Nearest => {
            let mut by_time: Vec<(i64, usize)> = all
                .iter()
                .enumerate()
                .map(|(i, t)| ((t.1.timestamp() - at.timestamp.timestamp()).abs(), i))
                .collect();
            by_time.sort_by_key(|(d, i)| (*d, *i));
            let mut idx: Vec<usize> = by_time.into_iter().take(k).map(|(_, i)| i).collect();
            idx.sort_unstable();
            idx.into_iter().map(|i| all[i].0.clone()).collect()
        }
        Shape::LeadingUp => {
            // Everything before the edit, nearest last — then the tail, cut at
            // the human prompt that starts it if one is in reach. A window
            // that begins mid-answer starts the story after the ask.
            let before: Vec<&(Turn, DateTime<Utc>)> =
                all.iter().filter(|(t, _)| t.line < at.line).collect();
            let start = before
                .iter()
                .rposition(|(t, _)| t.role == "user")
                .map(|p| p.saturating_sub(k.saturating_sub(1)).max(before.len().saturating_sub(k)))
                .unwrap_or_else(|| before.len().saturating_sub(k));
            let mut out: Vec<Turn> = before[start.min(before.len())..]
                .iter()
                .map(|(t, _)| t.clone())
                .collect();
            // The prompt itself, if the window slid past it. Without this the
            // shape is only *the turns before*, which is the other shape with
            // extra steps.
            if !out.iter().any(|t| t.role == "user") {
                if let Some((p, _)) = before.iter().rev().find(|(t, _)| t.role == "user") {
                    out.insert(0, p.clone());
                }
            }
            out
        }
    }
}

/// Every human prompt and assistant text line of a transcript, in order.
fn collect_turns(transcript: &Path) -> Vec<(Turn, DateTime<Utc>)> {
    let mut out = Vec::new();
    stream_lines(transcript, |line_no, line| {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return true };
        let Some(ts) = parse_ts(&v) else { return true };
        let (role, text) = if let Some(p) = human_prompt(&v) {
            ("user", p)
        } else if let Some(t) = assistant_text(&v) {
            ("assistant", t)
        } else {
            return true;
        };
        out.push((
            Turn {
                line: line_no,
                role: role.to_string(),
                text: tail(&text, TURN_CHARS),
                timestamp: ts,
            },
            ts,
        ));
        true
    });
    out
}

/// The last `n` characters, on a char boundary, marked when something was cut.
fn tail(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.to_string();
    }
    let kept: String = s.chars().skip(count - n).collect();
    format!("…{kept}")
}

/// The question, with the citation contract that makes an answer checkable.
///
/// Two rules carry the weight. **Cite or say you cannot**: an answer with no
/// line numbers is an answer from the code alone, and `R-O4` renders that
/// differently rather than passing it off as provenance. And **the words NOT IN
/// THESE TURNS are an acceptable answer** — `A4`'s mitigation, because a model
/// summarising a transcript shape the parser silently dropped will otherwise
/// describe a session confidently and wrongly.
pub fn prompt(question: &str, path: &str, hunk_header: Option<&str>, turns: &[Turn]) -> String {
    let where_ = match hunk_header {
        Some(h) => format!("`{path}` (the reader is looking at {h})"),
        None => format!("`{path}`"),
    };
    let mut s = format!(
        "Below are turns from the conversation in which a coding agent changed {where_}.\n\
         Each turn is labelled with the transcript line it came from.\n\n\
         Answer this question from these turns alone: {question}\n\n\
         Answer in exactly this form and nothing else:\n\n\
         REASON: <one short paragraph, or exactly NOT IN THESE TURNS>\n\
         CITES: <the line numbers you used, comma separated. Empty if none>\n\n\
         Do not answer from what you know about code in general — only from what is \
         said below. If these turns do not say why the change was made, answer NOT IN \
         THESE TURNS. An invented reason is worse than no answer.\n\n",
    );
    let mut spent = 0usize;
    for t in turns {
        if spent >= TOTAL_CHARS {
            s.push_str("… earlier turns omitted\n");
            break;
        }
        let body = if t.text.chars().count() + spent > TOTAL_CHARS {
            tail(&t.text, TOTAL_CHARS - spent)
        } else {
            t.text.clone()
        };
        spent += body.chars().count();
        s.push_str(&format!("--- line {} ({})\n{}\n\n", t.line, t.role, body));
    }
    s
}

/// The question, when **no transcript covers the file** and the diff is all
/// there is. `R-O4`'s labelled fallback.
///
/// A separate prompt rather than the same one with an empty turn list, because
/// the two answers are different in kind and the model should know which it is
/// being asked for: one is *what were they trying to do*, the other is *what
/// does this change do*. Passing the second off as the first is the failure the
/// `basis` label exists to prevent.
pub fn prompt_from_code(question: &str, path: &str, header: &str, lines: &[String]) -> String {
    let mut s = format!(
        "Below is a hunk of a diff to `{path}` ({header}).\n\n\
         **No conversation covering this file was found**, so answer from the diff \
         alone: {question}\n\n\
         Answer in one short paragraph and nothing else. Do not guess at anyone's \
         intent — you have not been shown it. Say what the change does, and say when \
         the diff does not tell you why.\n\n```diff\n"
    );
    let mut spent = 0usize;
    for l in lines {
        if spent >= TOTAL_CHARS {
            s.push_str("… rest of the hunk omitted\n");
            break;
        }
        spent += l.chars().count();
        s.push_str(l);
        s.push('\n');
    }
    s.push_str("```\n");
    s
}

/// What the model said, read back.
#[derive(Debug, Clone, Default)]
pub struct Answer {
    /// The reason, or empty when it said there was none.
    pub reason: String,
    /// The transcript lines it used, in the order it named them.
    pub cites: Vec<u64>,
    /// It said the turns do not contain the reason. Not a failure — this is the
    /// answer `A36` is most interested in.
    pub no_reason: bool,
    /// The reply carried no `REASON:` label at all.
    ///
    /// Kept as its own flag rather than folded into the reason, because the
    /// first corpus run found a reply that was neither prose nor an answer:
    /// llmproxy's own routing classification, echoed into the response body
    /// (`R3_LOCAL <parameter name="reason">…`). Counting that as *a reason
    /// found* would inflate exactly the number `A36` turns on.
    pub unformed: bool,
}

/// Read the answer back out of whatever the model wrote.
///
/// Forgiving about shape and strict about the one thing that matters: a
/// citation naming a line the prompt did not carry is **dropped**, because an
/// answer that cites a turn the reader cannot open is worse than an uncited
/// one — it looks like provenance and is not.
pub fn parse_answer(text: &str, shown: &[Turn]) -> Answer {
    let mut a = Answer::default();
    for line in text.lines() {
        let t = line.trim().trim_start_matches(['-', '*', '#', ' ']);
        if let Some(rest) = strip_label(t, "REASON") {
            a.reason = rest.trim().to_string();
        } else if let Some(rest) = strip_label(t, "CITES") {
            a.cites = rest
                .split(|c: char| !c.is_ascii_digit())
                .filter(|p| !p.is_empty())
                .filter_map(|p| p.parse::<u64>().ok())
                .filter(|n| shown.iter().any(|t| t.line == *n))
                .collect();
            a.cites.dedup();
        }
    }
    // A model that ignored the form entirely still said something; the whole
    // reply is the reason, because dropping it would report *no answer* for a
    // run that had one.
    if a.reason.is_empty() && !text.trim().is_empty() && !text.contains("NOT IN THESE TURNS") {
        a.reason = text.trim().to_string();
        a.unformed = true;
    }
    if a.reason.to_ascii_uppercase().contains("NOT IN THESE TURNS")
        || text.contains("NOT IN THESE TURNS") && a.reason.is_empty()
    {
        a.no_reason = true;
        a.reason.clear();
    }
    a
}

fn strip_label<'a>(line: &'a str, label: &str) -> Option<&'a str> {
    let lower = line.to_ascii_uppercase();
    let want = format!("{label}:");
    lower.starts_with(&want).then(|| &line[want.len()..])
}

/// Does this answer rest on the assistant's own narration alone?
///
/// The rule `--bin why` bought: nearest-in-time answers cited 1 human turn
/// against 12 of the assistant's, and *the file was changed because the
/// assistant then wrote the file* is a sentence rather than a rationale. An
/// answer with **no** citations is not narration — it is uncited, which the
/// `basis` label says separately.
pub fn is_narration<'a>(roles: impl IntoIterator<Item = &'a str>) -> bool {
    let mut any = false;
    for r in roles {
        any = true;
        if r == "user" {
            return false;
        }
    }
    any
}

/// Which side of the conversation an answer leant on. `A36`'s whole question.
pub fn cited_roles(a: &Answer, shown: &[Turn]) -> (usize, usize) {
    let mut user = 0;
    let mut assistant = 0;
    for c in &a.cites {
        match shown.iter().find(|t| t.line == *c).map(|t| t.role.as_str()) {
            Some("user") => user += 1,
            Some(_) => assistant += 1,
            None => {}
        }
    }
    (user, assistant)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("mogeung-why-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).expect("scratch");
            Scratch(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn turn(line: u64, role: &str, text: &str) -> Turn {
        Turn {
            line,
            role: role.to_string(),
            text: text.to_string(),
            timestamp: DateTime::parse_from_rfc3339("2026-08-01T10:00:00Z")
                .expect("ts")
                .with_timezone(&Utc),
        }
    }

    fn write(path: &Path, lines: &[String]) {
        let mut f = std::fs::File::create(path).expect("create");
        for l in lines {
            writeln!(f, "{l}").expect("write");
        }
    }

    fn user(ts: &str, text: &str) -> String {
        format!(r#"{{"type":"user","timestamp":"{ts}","message":{{"role":"user","content":"{text}"}}}}"#)
    }
    fn says(ts: &str, text: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"content":[{{"type":"text","text":"{text}"}}]}}}}"#
        )
    }
    fn edits(ts: &str, path: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"content":[{{"type":"tool_use","id":"t","name":"Edit","input":{{"file_path":"{path}"}}}}]}}}}"#
        )
    }

    fn corpus(dir: &Path) -> std::path::PathBuf {
        let p = dir.join("t.jsonl");
        write(
            &p,
            &[
                user("2026-08-01T10:00:00Z", "the retries hammer the API. Back them off"),
                says("2026-08-01T10:00:10Z", "I will add exponential backoff"),
                edits("2026-08-01T10:00:20Z", "/w/src/retry.rs"),
                says("2026-08-01T10:00:30Z", "Done. Added a backoff helper"),
                // A second, unrelated stretch of conversation, later in time.
                user("2026-08-01T11:00:00Z", "now bump the version"),
                edits("2026-08-01T11:00:10Z", "/w/Cargo.toml"),
            ],
        );
        p
    }

    #[test]
    fn only_the_tools_that_write_are_moments() {
        let d = Scratch::new("moments");
        let p = corpus(&d.0);
        let all = edit_moments(&p, None);
        assert_eq!(all.len(), 2, "two edits, and no Read among them");
        assert_eq!(all[0].line, 3);
        assert_eq!(all[0].tool, "Edit");

        // Suffix matching, because the tool records the path the agent saw and
        // the caller has the one git shows. `R-F2`'s rule, reused.
        let one = edit_moments(&p, Some("src/retry.rs"));
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].path, "/w/src/retry.rs");
    }

    /// The test that would fail if `A36`'s doubt were designed away rather than
    /// measured: the two shapes have to be able to disagree, or the harness
    /// compares a thing with itself.
    #[test]
    fn the_two_shapes_can_return_different_turns() {
        let d = Scratch::new("shapes");
        let p = corpus(&d.0);
        let at = edit_moments(&p, Some("src/retry.rs")).remove(0);

        let leading = turns_for(&p, &at, Shape::LeadingUp, 2);
        assert_eq!(leading.first().map(|t| t.role.as_str()), Some("user"));
        assert!(
            leading[0].text.contains("hammer the API"),
            "the ask that drove the edit is what leading-up is for"
        );

        // Nearest in time reaches *forwards* as happily as backwards, which is
        // the whole of the doubt: the assistant's own summary is closer to the
        // edit than the prompt that caused it.
        let nearest = turns_for(&p, &at, Shape::Nearest, 2);
        assert!(nearest.iter().any(|t| t.text.contains("Done.")));
    }

    #[test]
    fn a_citation_the_reader_cannot_open_is_dropped() {
        let shown = vec![turn(1, "user", "back off the retries"), turn(2, "assistant", "adding backoff")];
        let a = parse_answer("REASON: the API was being hammered\nCITES: 1, 2, 9999", &shown);
        assert_eq!(a.cites, vec![1, 2], "9999 was never shown; it is not provenance");
        assert!(!a.no_reason);
        assert_eq!(cited_roles(&a, &shown), (1, 1));
    }

    #[test]
    fn not_in_these_turns_is_an_answer_and_not_a_failure() {
        let shown = vec![turn(1, "assistant", "done")];
        let a = parse_answer("REASON: NOT IN THESE TURNS\nCITES:", &shown);
        assert!(a.no_reason);
        assert!(a.reason.is_empty());
        assert!(a.cites.is_empty());
    }

    /// A model that ignores the form has still said something, and reporting
    /// *no answer* for a run that had one is how a harness lies.
    #[test]
    fn a_reply_that_ignores_the_form_is_still_a_reason() {
        let shown = vec![turn(1, "user", "x")];
        let a = parse_answer("They wanted the retries backed off.", &shown);
        assert_eq!(a.reason, "They wanted the retries backed off.");
        assert!(a.cites.is_empty(), "an uncited answer is not provenance");
        // …and it is marked, because the first corpus run caught a reply that
        // was neither prose nor an answer — a proxy's routing classification,
        // echoed into the body.
        assert!(a.unformed);
    }

    #[test]
    fn narration_is_every_citation_being_the_agent_talking_about_itself() {
        assert!(is_narration(["assistant", "assistant"]));
        assert!(!is_narration(["assistant", "user"]), "one human turn is a rationale");
        assert!(!is_narration([]), "no citations is uncited, which is a different label");
    }

    #[test]
    fn the_prompt_is_bounded_by_the_whole_conversation() {
        let long = "x".repeat(TOTAL_CHARS);
        let turns: Vec<Turn> = (1..=6).map(|i| turn(i, "assistant", &long)).collect();
        let p = prompt("why?", "src/a.rs", None, &turns);
        assert!(p.chars().count() < TOTAL_CHARS * 2, "a bound that is not a bound is a 78-second failure");
        assert!(p.contains("earlier turns omitted"), "what was cut has to be visible as a cut");
    }
}
