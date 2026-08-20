//! Reader for Claude Code's on-disk session transcripts.
//!
//! Claude Code writes every session to `~/.claude/projects/<slug>/<id>.jsonl`
//! for its own resume/history purposes. mogeung tails those files. Nothing here
//! starts or controls an agent — that is the whole point of the observer model.
//!
//! The parser is deliberately forgiving: unknown event types and unexpected
//! shapes are ignored, never fatal. This is an undocumented internal format on
//! a tool that ships constantly, so a schema change must degrade the transcript
//! rather than break the watcher.

use chrono::{DateTime, Utc};
use mogeung_core::health::LineClass;
use mogeung_core::transcript::EventKind;
use mogeung_core::verify::VerifyKind;
use serde_json::Value;

/// Transcript event types we have seen, understand the purpose of, and
/// deliberately do not surface.
///
/// This list is the whole point of roadmap `R-A1`. Before it existed, an
/// unrecognised type and a type we had chosen to skip were both `None`, so a
/// format change was indistinguishable from normal operation. Adding an entry
/// here is now a deliberate act: it says *we looked at this and decided it
/// carries nothing we need*, and it silences an alert that would otherwise
/// fire.
///
/// Verified against the 52-transcript corpus on the author's machine
/// (2026-07-25, Claude Code 2.1.219/2.1.220). `queue-operation`, `pr-link` and
/// `frame-link` were found only because the canary flagged them.
pub const KNOWN_IGNORED: &[&str] = &[
    // Session settings chatter.
    "mode",
    "permission-mode",
    // Pre-edit backup bookkeeping. See ADR-0004 for why we do not read it.
    "file-history-snapshot",
    // Pasted/attached content, already reflected in the message that used it.
    "attachment",
    // Turn timing and meta notices.
    "system",
    // Queued follow-up prompts, before they become real turns.
    "queue-operation",
    // Links the CLI records after opening a PR or a frame.
    "pr-link",
    "frame-link",
    // The web/desktop bridge's own bookkeeping: a session id, the bridge's id
    // for it, and a sequence number. No content, no tokens, no tool use — the
    // same category as the two above. Found 2026-08-07 by re-running the
    // classification over the corpus, which had grown to 315 transcripts and
    // sprouted a fourteenth type since the last sweep.
    "bridge-session",
    // Three keys — `type`, `sessionId`, `atis` — and `atis` is the **empty
    // string in all 380 of them**, across 7 transcripts of the 274 swept on
    // 2026-08-20. So this is not "a field we chose not to read": there is
    // nothing in it to read. If a non-empty one ever appears it will not be
    // loud, which is the honest cost of ignoring a type by its name rather
    // than by its shape — noted here so the next sweep can look.
    "atis-latch",
];

/// Types this parser extracts data from.
///
/// Kept explicit so that a line of a handled type which yields nothing can be
/// reported as [`LineClass::Barren`] rather than silently dropped — a spike
/// there means a shape we depend on has moved.
pub const HANDLED: &[&str] = &[
    "assistant",
    "user",
    "ai-title",
    // A second writer of the title, found 2026-08-20. See the parse arm.
    "agent-name",
    "last-prompt",
    "file-history-delta",
];

/// The outcome of reading one transcript line.
#[derive(Debug)]
pub enum LineOutcome {
    /// Understood, and it produced something.
    Parsed(Box<Parsed>),
    /// A known type we deliberately skip.
    Ignored,
    /// A handled type that yielded nothing this time.
    Barren { event_type: String },
    /// A type we have never heard of. The canary.
    Unknown { event_type: String },
    /// Not JSON, or no `type` field.
    Malformed,
}

impl LineOutcome {
    pub fn class(&self) -> LineClass {
        match self {
            LineOutcome::Parsed(_) => LineClass::Parsed,
            LineOutcome::Ignored => LineClass::Ignored,
            LineOutcome::Barren { .. } => LineClass::Barren,
            LineOutcome::Unknown { .. } => LineClass::Unknown,
            LineOutcome::Malformed => LineClass::Malformed,
        }
    }

    pub fn parsed(self) -> Option<Parsed> {
        match self {
            LineOutcome::Parsed(p) => Some(*p),
            _ => None,
        }
    }
}

/// Everything one transcript line tells us.
#[derive(Debug, Default)]
pub struct Parsed {
    pub events: Vec<EventKind>,
    pub ts: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub version: Option<String>,
    /// Claude Code's own generated title for the conversation.
    pub title: Option<String>,
    pub last_prompt: Option<String>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// This line represents a fresh prompt from the human.
    pub is_turn: bool,
    pub tool_calls: u32,
    pub last_activity: Option<String>,
    /// Files this line shows the session touching.
    pub touched: Vec<String>,
    pub error: Option<String>,
    /// Emitted by a subagent rather than the main conversation.
    pub sidechain: bool,
    /// This line is the CLI's synthetic "you've hit your session limit"
    /// message. `R-G1`.
    pub limit_hit: bool,
    /// The reset phrase from that message, verbatim prose (e.g. `8pm
    /// (Europe/London)`), when it carried one.
    pub limit_resets: Option<String>,
    /// Bash commands on this line that look like verification —
    /// `(tool_use_id, kind, clipped command)`. `R-E1`.
    pub verify_cmds: Vec<(String, VerifyKind, String)>,
    /// "Tests pass"-shaped assertions in this line's assistant prose —
    /// `(kind, clipped sentence)`. `R-E3`.
    pub claims: Vec<(VerifyKind, String)>,
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
}

/// Collapse a value onto one line. Tool summaries and activity strings are
/// rendered in single-line contexts, and a heredoc in a bash command would
/// otherwise wreck the layout.
fn one_line(s: &str, n: usize) -> String {
    let flat: String = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    truncate(&flat, n)
}

fn str_at<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str())
}

/// Human-readable one-liner for a tool call.
fn tool_summary(name: &str, input: &Value) -> String {
    let get = |k: &str| input.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let s = match name {
        "Bash" => {
            let cmd = get("command");
            if cmd.is_empty() {
                get("description").to_string()
            } else {
                cmd.to_string()
            }
        }
        "Read" | "Write" | "Edit" | "NotebookEdit" => get("file_path").to_string(),
        "Glob" | "Grep" => {
            let p = get("pattern");
            let path = get("path");
            if path.is_empty() {
                p.to_string()
            } else {
                format!("{p}  in {path}")
            }
        }
        "WebFetch" => get("url").to_string(),
        "WebSearch" => get("query").to_string(),
        "Task" | "Agent" => get("description").to_string(),
        "TaskCreate" | "TaskUpdate" => get("subject").to_string(),
        "Skill" => get("skill").to_string(),
        _ => input
            .as_object()
            .and_then(|o| {
                o.iter()
                    .find(|(_, v)| v.as_str().map(|s| s.len() < 200).unwrap_or(false))
                    .and_then(|(_, v)| v.as_str())
            })
            .unwrap_or("")
            .to_string(),
    };
    one_line(&s, 160)
}

/// Classify a shell command as verification, if it is one. `R-E1`.
///
/// Honest patterns over a tokenised command, nothing cleverer: `cd x && cargo
/// test` is a test run, `grep test foo.rs` is not. Chained commands
/// (`&&`/`;`/`||`) classify per segment and the strongest kind wins —
/// `cargo build && cargo test` is a test run.
pub fn verify_kind(command: &str) -> Option<VerifyKind> {
    command
        .split(|c| c == ';' || c == '\n')
        .flat_map(|s| s.split("&&"))
        .flat_map(|s| s.split("||"))
        .filter_map(segment_kind)
        .max()
}

fn segment_kind(seg: &str) -> Option<VerifyKind> {
    // Strip leading env assignments and unexciting wrappers.
    let words: Vec<&str> = seg
        .split_whitespace()
        .skip_while(|w| w.contains('=') || *w == "sudo" || *w == "env" || *w == "time")
        .collect();
    let (first, rest) = words.split_first()?;
    let first = first.rsplit('/').next().unwrap_or(first);
    let has = |w: &str| rest.iter().any(|r| *r == w);

    Some(match first {
        "pytest" | "jest" | "vitest" | "rspec" | "phpunit" | "ctest" | "gotestsum" => {
            VerifyKind::Test
        }
        "mypy" | "pyright" | "tsc" => VerifyKind::Typecheck,
        "eslint" | "ruff" | "flake8" | "golangci-lint" | "shellcheck" => VerifyKind::Lint,
        "nyc" | "gcov" | "tarpaulin" => VerifyKind::Coverage,
        "cargo" => match rest.first().copied() {
            Some("test" | "nextest") => VerifyKind::Test,
            Some("clippy") => VerifyKind::Lint,
            Some("check") => VerifyKind::Typecheck,
            Some("llvm-cov" | "tarpaulin") => VerifyKind::Coverage,
            Some("build") => VerifyKind::Build,
            _ => return None,
        },
        "go" => match rest.first().copied() {
            Some("test") => VerifyKind::Test,
            Some("vet") => VerifyKind::Lint,
            Some("build") => VerifyKind::Build,
            _ => return None,
        },
        "npm" | "yarn" | "pnpm" | "bun" => {
            let sub = rest.iter().find(|w| **w != "run")?;
            if sub.contains("test") {
                VerifyKind::Test
            } else if *sub == "build" {
                VerifyKind::Build
            } else if sub.contains("lint") {
                VerifyKind::Lint
            } else if sub.contains("typecheck") || sub.contains("tsc") {
                VerifyKind::Typecheck
            } else {
                return None;
            }
        }
        "make" => {
            if has("test") || has("check") {
                VerifyKind::Test
            } else {
                VerifyKind::Build
            }
        }
        "mvn" | "gradle" | "gradlew" | "dotnet" => {
            if has("test") {
                VerifyKind::Test
            } else if has("package") || has("compile") || has("build") || has("install") {
                VerifyKind::Build
            } else {
                return None;
            }
        }
        "python" | "python3" => {
            if rest.windows(2).any(|w| w == ["-m", "pytest"] || w == ["-m", "unittest"]) {
                VerifyKind::Test
            } else {
                return None;
            }
        }
        // `./scripts/run-tests.sh`, `./test.sh` — a script that says test.
        f if f.ends_with(".sh") && f.contains("test") => VerifyKind::Test,
        _ if rest.iter().any(|w| *w == "--coverage") => VerifyKind::Coverage,
        _ => return None,
    })
}

/// Success-shaped claim phrases. Failure reports ("tests fail") are not
/// claims that need checking — the agent is already admitting it.
const CLAIM_PATTERNS: &[(&str, VerifyKind)] = &[
    ("tests pass", VerifyKind::Test),
    ("tests passed", VerifyKind::Test),
    ("test passes", VerifyKind::Test),
    ("tests are passing", VerifyKind::Test),
    ("test suite passes", VerifyKind::Test),
    ("all tests green", VerifyKind::Test),
    ("tests are green", VerifyKind::Test),
    ("0 failed", VerifyKind::Test),
    ("build succeeds", VerifyKind::Build),
    ("build succeeded", VerifyKind::Build),
    ("builds successfully", VerifyKind::Build),
    ("build passes", VerifyKind::Build),
    ("compiles cleanly", VerifyKind::Build),
    ("compilation succeeded", VerifyKind::Build),
    ("typecheck passes", VerifyKind::Typecheck),
    ("type checks pass", VerifyKind::Typecheck),
    ("no type errors", VerifyKind::Typecheck),
    ("clippy is clean", VerifyKind::Lint),
    ("lint passes", VerifyKind::Lint),
    ("no lint errors", VerifyKind::Lint),
];

/// Extract success-shaped verification claims from assistant prose. `R-E3`.
fn extract_claims(text: &str, out: &mut Vec<(VerifyKind, String)>) {
    let lower = text.to_lowercase();
    for (pat, kind) in CLAIM_PATTERNS {
        let Some(pos) = lower.find(pat) else { continue };
        // Clip the sentence around the match, on char boundaries.
        let start = text[..pos]
            .rfind(['.', '!', '\n'])
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = text[pos..]
            .find(['.', '!', '\n'])
            .map(|i| pos + i + 1)
            .unwrap_or(text.len());
        let sentence = one_line(&text[start..end], 200);
        if !out.iter().any(|(_, s)| *s == sentence) {
            out.push((*kind, sentence));
        }
    }
}

/// Tools whose use means a file on disk changed.
fn touched_path(name: &str, input: &Value) -> Option<String> {
    match name {
        "Write" | "Edit" | "NotebookEdit" => {
            str_at(input, "file_path").map(str::to_string)
        }
        _ => None,
    }
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    }
}

/// Classify and read one line of a `.jsonl` transcript.
///
/// Never panics and never fails: an unreadable line becomes an outcome, because
/// the formats are undocumented and a schema change must degrade the board
/// rather than stop the watcher.
pub fn parse_line(line: &str) -> LineOutcome {
    let line = line.trim();
    if line.is_empty() {
        // The tailer already drops blank lines; this is belt and braces, and a
        // blank line is not evidence of a format change.
        return LineOutcome::Ignored;
    }
    if !line.starts_with('{') {
        return LineOutcome::Malformed;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return LineOutcome::Malformed;
    };
    let Some(ty) = str_at(&v, "type") else {
        return LineOutcome::Malformed;
    };
    let ty = ty.to_string();

    if KNOWN_IGNORED.contains(&ty.as_str()) {
        return LineOutcome::Ignored;
    }
    if !HANDLED.contains(&ty.as_str()) {
        // The canary. We have never seen this type, so we cannot know what we
        // are missing — which is exactly why it must be said out loud.
        return LineOutcome::Unknown { event_type: ty };
    }

    match extract(&v, &ty) {
        Some(p) => LineOutcome::Parsed(Box::new(p)),
        None => LineOutcome::Barren { event_type: ty },
    }
}

/// Pull what we can out of a line whose type we handle.
///
/// `None` means the type is one we read but this line gave us nothing — a
/// normal occurrence in small numbers (a `last-prompt` with no prompt), and a
/// signal that a shape has moved if it becomes common.
fn extract(v: &Value, ty: &str) -> Option<Parsed> {
    let mut out = Parsed {
        ts: str_at(&v, "timestamp")
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc)),
        cwd: str_at(&v, "cwd").map(str::to_string),
        git_branch: str_at(&v, "gitBranch")
            .filter(|b| !b.is_empty() && *b != "HEAD")
            .map(str::to_string),
        version: str_at(&v, "version").map(str::to_string),
        sidechain: v
            .get("isSidechain")
            .and_then(|s| s.as_bool())
            .unwrap_or(false),
        ..Default::default()
    };

    match ty {
        // Claude Code's generated conversation title — the best label we get.
        "ai-title" => {
            out.title = str_at(&v, "aiTitle").map(str::to_string);
        }

        // The same title, under a second name. Appeared 2026-08-20, 249 lines
        // across 3 of the 274 transcripts on this machine.
        //
        // **Read rather than ignored, and the redundancy is the reason.** Every
        // value it carries is already an `ai-title` in the same file, `ai-title`
        // wrote it first in all six cases, and it never disagrees with the title
        // in force — so today this arm cannot change a single session's name.
        // What it buys is that the title survives `ai-title` being renamed or
        // retired, which is not a hypothetical in a private format that grew two
        // new types in a fortnight: two writers of one field is cheaper than
        // discovering, from a queue full of untitled sessions, that the one we
        // read went away.
        //
        // It cannot smuggle in a *subagent's* name, which is what the type's
        // spelling suggests it might: its `sessionId` was the transcript's own
        // in every line swept.
        "agent-name" => {
            out.title = str_at(&v, "agentName").map(str::to_string);
        }

        "last-prompt" => {
            out.last_prompt = str_at(&v, "lastPrompt").map(|s| truncate(s, 400));
        }

        // Records a file the session is tracking edits to.
        "file-history-delta" => {
            if let Some(p) = str_at(&v, "trackingPath") {
                out.touched.push(p.to_string());
            }
        }

        "user" => {
            let content = v.get("message").and_then(|m| m.get("content"))?;
            match content {
                // A plain string is always a human prompt.
                Value::String(s) => {
                    if !s.trim().is_empty() {
                        out.is_turn = true;
                        out.last_prompt = Some(truncate(s, 400));
                        out.events.push(EventKind::UserPrompt {
                            text: truncate(s, 4000),
                        });
                    }
                }
                Value::Array(blocks) => {
                    let mut prompt_parts = Vec::new();
                    for b in blocks {
                        match b.get("type").and_then(|t| t.as_str()) {
                            Some("tool_result") => {
                                out.events.push(EventKind::ToolResult {
                                    tool_use_id: str_at(b, "tool_use_id")
                                        .unwrap_or("")
                                        .to_string(),
                                    is_error: b
                                        .get("is_error")
                                        .and_then(|e| e.as_bool())
                                        .unwrap_or(false),
                                    preview: truncate(
                                        &text_of(b.get("content").unwrap_or(&Value::Null)),
                                        400,
                                    ),
                                });
                            }
                            Some("text") => {
                                if let Some(t) = str_at(b, "text") {
                                    prompt_parts.push(t.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    let joined = prompt_parts.join("\n");
                    if !joined.trim().is_empty() {
                        out.is_turn = true;
                        out.last_prompt = Some(truncate(&joined, 400));
                        out.events.push(EventKind::UserPrompt {
                            text: truncate(&joined, 4000),
                        });
                    }
                }
                _ => {}
            }
        }

        "assistant" => {
            let msg = v.get("message")?;
            // The CLI reports a hit rate limit as a synthetic assistant
            // message, not an event type — verified against the real corpus,
            // 2026-07-29. `message.model == "<synthetic>"` is the signature;
            // the text is prose ("You've hit your session limit · resets
            // 8pm (Europe/London)").
            if str_at(msg, "model") == Some("<synthetic>") {
                let text = text_of(msg.get("content").unwrap_or(&Value::Null));
                let lower = text.to_lowercase();
                if lower.contains("limit") {
                    out.limit_hit = true;
                    out.limit_resets = text
                        .split_once("resets")
                        .map(|(_, r)| truncate(r.trim_start_matches([' ', ':']), 80))
                        .filter(|r| !r.is_empty());
                }
            }
            if let Some(usage) = msg.get("usage") {
                let n = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
                out.tokens_in = n("input_tokens")
                    + n("cache_read_input_tokens")
                    + n("cache_creation_input_tokens");
                out.tokens_out = n("output_tokens");
            }
            // API errors are marked on the message itself.
            if v.get("isApiErrorMessage")
                .and_then(|e| e.as_bool())
                .unwrap_or(false)
            {
                let detail = text_of(msg.get("content").unwrap_or(&Value::Null));
                out.error = Some(truncate(&detail, 300));
            }
            if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
                out.error = Some(truncate(&text_of(e), 300));
            }

            for block in msg.get("content").and_then(|c| c.as_array())? {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = str_at(block, "text") {
                            if !t.trim().is_empty() {
                                // Subagent prose stays out: its claims are
                                // about subtasks, not the session's work.
                                if !out.sidechain {
                                    extract_claims(t, &mut out.claims);
                                }
                                out.events.push(EventKind::AssistantText {
                                    text: truncate(t, 8000),
                                });
                            }
                        }
                    }
                    Some("thinking") => {
                        if let Some(t) = str_at(block, "thinking") {
                            if !t.trim().is_empty() {
                                out.events.push(EventKind::Thinking {
                                    text: truncate(t, 4000),
                                });
                            }
                        }
                    }
                    Some("tool_use") => {
                        let name = str_at(block, "name").unwrap_or("tool").to_string();
                        let empty = Value::Object(Default::default());
                        let input = block.get("input").unwrap_or(&empty);
                        let summary = tool_summary(&name, input);
                        if name == "Bash" && !out.sidechain {
                            if let Some(cmd) = str_at(input, "command") {
                                if let Some(kind) = verify_kind(cmd) {
                                    out.verify_cmds.push((
                                        str_at(block, "id").unwrap_or("").to_string(),
                                        kind,
                                        one_line(cmd, 160),
                                    ));
                                }
                            }
                        }
                        if let Some(p) = touched_path(&name, input) {
                            out.touched.push(p);
                        }
                        out.tool_calls += 1;
                        // Subagent chatter should not masquerade as the main
                        // session's current activity.
                        if !out.sidechain {
                            out.last_activity = Some(if summary.is_empty() {
                                name.clone()
                            } else {
                                format!("{name}: {summary}")
                            });
                        }
                        out.events.push(EventKind::ToolUse {
                            tool_use_id: str_at(block, "id").unwrap_or("").to_string(),
                            name,
                            summary,
                        });
                    }
                    _ => {}
                }
            }
        }

        // Unreachable: `parse_line` has already sorted every type into
        // handled, known-ignored or unknown. Kept so that adding a name to
        // `HANDLED` without writing an arm degrades to "barren" rather than
        // failing to compile into a silent lie.
        _ => return None,
    }

    if out.events.is_empty()
        && out.title.is_none()
        && out.last_prompt.is_none()
        && out.touched.is_empty()
        && out.error.is_none()
        && out.tokens_out == 0
        && !out.limit_hit
    {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unwrap a line we expect to yield data.
    fn parsed(line: &str) -> Parsed {
        match parse_line(line) {
            LineOutcome::Parsed(p) => *p,
            other => panic!("expected a parsed line, got {other:?}"),
        }
    }

    fn class(line: &str) -> LineClass {
        parse_line(line).class()
    }

    #[test]
    fn known_bookkeeping_is_ignored_quietly() {
        assert_eq!(class(""), LineClass::Ignored);
        for ty in KNOWN_IGNORED {
            let line = format!(r#"{{"type":"{ty}"}}"#);
            assert_eq!(
                class(&line),
                LineClass::Ignored,
                "{ty} is in KNOWN_IGNORED but did not classify as ignored"
            );
        }
    }

    /// The canary. This is the distinction the whole feature exists to draw:
    /// an unclassified type must never look like ordinary skipped bookkeeping.
    #[test]
    fn an_unheard_of_type_is_not_confused_with_bookkeeping() {
        match parse_line(r#"{"type":"warp-drive","x":1}"#) {
            LineOutcome::Unknown { event_type } => assert_eq!(event_type, "warp-drive"),
            other => panic!("a future event type must be flagged, got {other:?}"),
        }
    }

    /// Regression: these exist in the real corpus and were being swallowed by
    /// a catch-all. The first three were found *by* the canary; `bridge-session`
    /// was found on 2026-08-07 by re-running the sweep by hand, which is the
    /// same check the canary makes at runtime and is worth doing when no daemon
    /// has been up long enough to make it.
    #[test]
    fn types_found_in_the_real_corpus_are_all_classified() {
        for ty in ["queue-operation", "pr-link", "frame-link", "bridge-session", "atis-latch"] {
            let line = format!(r#"{{"type":"{ty}","sessionId":"s"}}"#);
            assert_eq!(
                class(&line),
                LineClass::Ignored,
                "{ty} occurs in real transcripts and must be classified, not unknown"
            );
        }
    }

    /// `agent-name` is the session's title under a second name, found by the
    /// 2026-08-20 sweep. It is **read**, not ignored: every value it carries is
    /// already an `ai-title` today, and reading both is what keeps a title on
    /// the queue if the CLI ever retires the name we depend on.
    #[test]
    fn a_second_writer_of_the_title_is_read_like_the_first() {
        let line = r#"{"type":"agent-name","agentName":"the migration","sessionId":"s"}"#;
        assert_eq!(parsed(line).title.as_deref(), Some("the migration"));
    }

    /// A handled type that yields nothing is `Barren`, never `Ignored` — the
    /// distinction that would tell us the shape moved under the name.
    #[test]
    fn a_nameless_agent_name_is_barren_rather_than_quietly_skipped() {
        assert_eq!(class(r#"{"type":"agent-name","sessionId":"s"}"#), LineClass::Barren);
    }

    #[test]
    fn unreadable_lines_are_malformed_not_silently_dropped() {
        assert_eq!(class("not json"), LineClass::Malformed);
        assert_eq!(class("{ truncated"), LineClass::Malformed);
        // Valid JSON, but nothing tells us what it is.
        assert_eq!(class(r#"{"sessionId":"s"}"#), LineClass::Malformed);
    }

    /// A handled type that yields nothing is its own category: normal in small
    /// numbers, and evidence a shape moved if it becomes common.
    #[test]
    fn a_handled_type_yielding_nothing_is_barren() {
        // Seen 42× in the real corpus: a last-prompt with no lastPrompt.
        match parse_line(r#"{"type":"last-prompt","leafUuid":"u","sessionId":"s"}"#) {
            LineOutcome::Barren { event_type } => assert_eq!(event_type, "last-prompt"),
            other => panic!("expected barren, got {other:?}"),
        }
        assert_eq!(class(r#"{"type":"assistant"}"#), LineClass::Barren);
    }

    #[test]
    fn multiline_commands_stay_on_one_line() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"python3 - <<'PY'\nimport os\nprint(1)\nPY"}}]}}"#;
        let p = parsed(l);
        let activity = p.last_activity.unwrap();
        assert!(!activity.contains('\n'), "activity leaked a newline: {activity:?}");
        assert!(activity.starts_with("Bash: python3 - <<'PY' import os"));
    }

    #[test]
    fn reads_the_generated_title() {
        let p = parsed(r#"{"type":"ai-title","aiTitle":"Fix the retry loop"}"#);
        assert_eq!(p.title.as_deref(), Some("Fix the retry loop"));
    }

    #[test]
    fn a_string_user_message_is_a_turn() {
        let l = r#"{"type":"user","timestamp":"2026-07-17T14:24:21.804Z","cwd":"/w","gitBranch":"main","message":{"role":"user","content":"do the thing"}}"#;
        let p = parsed(l);
        assert!(p.is_turn);
        assert_eq!(p.cwd.as_deref(), Some("/w"));
        assert_eq!(p.git_branch.as_deref(), Some("main"));
        assert!(p.ts.is_some());
    }

    #[test]
    fn a_tool_result_carrier_is_not_a_turn() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":false,"content":"ok"}]}}"#;
        let p = parsed(l);
        assert!(!p.is_turn, "tool results must not be counted as human turns");
        assert!(matches!(p.events[0], EventKind::ToolResult { .. }));
    }

    #[test]
    fn detached_head_is_not_treated_as_a_branch() {
        let l = r#"{"type":"user","gitBranch":"HEAD","message":{"role":"user","content":"x"}}"#;
        assert!(parsed(l).git_branch.is_none());
    }

    #[test]
    fn write_and_edit_register_as_touched_files() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"/w/src/a.rs"}}]}}"#;
        let p = parsed(l);
        assert_eq!(p.touched, vec!["/w/src/a.rs"]);
        assert_eq!(p.tool_calls, 1);
        assert_eq!(p.last_activity.as_deref(), Some("Edit: /w/src/a.rs"));
    }

    #[test]
    fn reads_but_does_not_count_as_a_file_change() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"/w/src/a.rs"}}]}}"#;
        assert!(parsed(l).touched.is_empty());
    }

    #[test]
    fn subagent_activity_does_not_become_the_session_headline() {
        let l = r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#;
        let p = parsed(l);
        assert!(p.sidechain);
        assert!(p.last_activity.is_none());
    }

    #[test]
    fn api_errors_surface_as_errors() {
        let l = r#"{"type":"assistant","isApiErrorMessage":true,"message":{"content":[{"type":"text","text":"overloaded_error"}]}}"#;
        let p = parsed(l);
        assert!(p.error.unwrap().contains("overloaded"));
    }

    /// R-G1. The real shape, taken from the corpus (2026-07-06 hits): a
    /// synthetic assistant message, not an event type.
    #[test]
    fn a_synthetic_limit_message_is_a_limit_hit() {
        let l = r#"{"type":"assistant","timestamp":"2026-07-06T17:14:01.090Z","message":{"model":"<synthetic>","role":"assistant","type":"message","content":[{"type":"text","text":"You've hit your session limit · resets 8pm (Europe/London)"}],"usage":{"input_tokens":0,"output_tokens":0}}}"#;
        let p = parsed(l);
        assert!(p.limit_hit);
        assert_eq!(p.limit_resets.as_deref(), Some("8pm (Europe/London)"));
        assert!(p.error.is_none(), "a limit is not a failure");
    }

    /// A synthetic message that is not about limits must not cry limit.
    #[test]
    fn other_synthetic_messages_are_not_limit_hits() {
        let l = r#"{"type":"assistant","message":{"model":"<synthetic>","content":[{"type":"text","text":"Conversation compacted"}]}}"#;
        let p = parsed(l);
        assert!(!p.limit_hit);
    }

    /// A line type nobody has ever seen must reach the canary, not a guess.
    ///
    /// `rate_limit_event` was handled here for a while on the strength of
    /// `R-G1`'s premise — and that premise was wrong: zero exist across 235
    /// transcripts (A20). Handling a shape we have never observed costs the
    /// alert that [`LineClass::Unknown`] exists to raise, which is the one
    /// mechanism built for a format that moved. The guess is gone; if such a
    /// line ever appears, the canary says so loudly.
    #[test]
    fn an_unseen_line_type_reaches_the_canary_rather_than_a_guessed_arm() {
        let l = r#"{"type":"rate_limit_event","window":"5h","utilization":0.97}"#;
        assert!(
            !HANDLED.contains(&"rate_limit_event"),
            "a type nobody has observed must not be pre-handled"
        );
        match parse_line(l) {
            LineOutcome::Unknown { event_type } => assert_eq!(event_type, "rate_limit_event"),
            other => panic!("expected the canary to fire, got {other:?}"),
        }
    }

    /// R-E1. The classifier must recognise real verification and refuse
    /// lookalikes — `grep test` is not a test run.
    #[test]
    fn verification_commands_classify_and_lookalikes_do_not() {
        use VerifyKind::*;
        let cases = [
            ("cargo test --workspace", Some(Test)),
            ("cd /w && cargo test", Some(Test)),
            ("cargo build --release && cargo test", Some(Test)),
            ("RUST_LOG=debug cargo check", Some(Typecheck)),
            ("cargo clippy -- -D warnings", Some(Lint)),
            ("npm run test:unit", Some(Test)),
            ("yarn build", Some(Build)),
            ("python3 -m pytest tests/", Some(Test)),
            ("make test", Some(Test)),
            ("make", Some(Build)),
            ("./scripts/run-tests.sh", Some(Test)),
            ("tsc --noEmit", Some(Typecheck)),
            ("cargo llvm-cov --lcov", Some(Coverage)),
            // The lookalikes.
            ("grep test src/main.rs", None),
            ("cat tests/fixtures/corpus.jsonl", None),
            ("git log --grep 'cargo test'", None),
            ("ls", None),
        ];
        for (cmd, want) in cases {
            assert_eq!(verify_kind(cmd), want, "misclassified: {cmd}");
        }
    }

    /// R-E1/R-E3. A verifying Bash call and a success claim both surface.
    #[test]
    fn verify_commands_and_claims_are_extracted() {
        let l = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done. All tests pass now."},{"type":"tool_use","id":"toolu_v1","name":"Bash","input":{"command":"cargo test"}}]}}"#;
        let p = parsed(l);
        assert_eq!(p.verify_cmds.len(), 1);
        assert_eq!(p.verify_cmds[0].0, "toolu_v1");
        assert_eq!(p.verify_cmds[0].1, VerifyKind::Test);
        assert_eq!(p.claims.len(), 1);
        assert!(p.claims[0].1.contains("tests pass"));
    }

    /// Subagent claims are about subtasks, not the session's work.
    #[test]
    fn sidechain_claims_and_commands_stay_out() {
        let l = r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"tests pass"},{"type":"tool_use","id":"t","name":"Bash","input":{"command":"cargo test"}}]}}"#;
        let p = parsed(l);
        assert!(p.claims.is_empty());
        assert!(p.verify_cmds.is_empty());
    }

    #[test]
    fn usage_totals_are_read() {
        let l = r#"{"type":"assistant","message":{"content":[],"usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":5}}}"#;
        let p = parsed(l);
        assert_eq!(p.tokens_in, 15);
        assert_eq!(p.tokens_out, 20);
    }
}
