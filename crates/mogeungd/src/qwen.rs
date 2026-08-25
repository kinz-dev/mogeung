//! Reader for Qwen Code's on-disk session state. Roadmap `R-I15`.
//!
//! Qwen Code (`@qwen-code/qwen-code`, an independent descendant of Gemini CLI)
//! keeps two things mogeung cares about under `~/.qwen`:
//!
//! * `sessions/<pid>.json` — a live process registry, the same idea as Claude
//!   Code's. It gives the session id, the cwd, a friendly name and a start
//!   time. It does **not** carry a busy/idle status, which is the one thing
//!   Claude's registry has and this one does not.
//! * `projects/<sanitised-cwd>/chats/<session-id>.jsonl` — the transcript,
//!   append-only, tailed here by byte offset.
//!
//! So the *shape* of the install is Claude's and the *shape of a record* is
//! Gemini's: a `message` is a `{role, parts[]}` `Content`, not an Anthropic
//! message, and `role` is `"model"` rather than `"assistant"`. The two
//! vocabularies must not be blurred, which is why this is its own module rather
//! than a flag on `adapter.rs`.
//!
//! **Evidence honesty.** Unlike `codex.rs`, every shape below was read from
//! real transcripts on the author's machine (Qwen Code 0.22.0, 2026-08-25) and
//! cross-checked against the shipped bundle's `chatRecordingService.ts`, which
//! is unminified and keeps its source banners. The corpus is nonetheless
//! *thin* — two sessions, eighteen records, seven of the nineteen documented
//! `system` subtypes — so the canary discipline matters here as much as it does
//! for Codex, for the opposite reason: not because the shapes are guessed, but
//! because so few of them have been seen.
//!
//! Nothing here starts or controls an agent (ADR-0003).

use chrono::{DateTime, TimeZone, Utc};
use mogeung_core::health::LineClass;
use mogeung_core::session::LiveStatus;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Where Qwen Code keeps its state, unless told otherwise.
///
/// `QWEN_HOME` is honoured because Qwen Code honours it: everything under
/// `~/.qwen` is rooted at `Storage.getRuntimeBaseDir()`, and a user who has
/// moved it would otherwise get a silent "no sessions" rather than an answer.
pub fn default_home() -> PathBuf {
    if let Ok(p) = std::env::var("QWEN_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".qwen")
}

// ---------------------------------------------------------------------------
// The taxonomy — the canary's whole point
// ---------------------------------------------------------------------------

/// Top-level record types this parser extracts data from.
///
/// Only four exist. `createBaseRecord` in `chatRecordingService.ts` can emit
/// exactly `user`, `assistant`, `system` and `tool_result`; anything else is
/// either a future addition or a file we should not be reading.
pub const HANDLED: &[&str] = &["user", "assistant", "tool_result", "system"];

/// Record types we have seen, understand, and deliberately do not surface.
///
/// Empty, and that is a statement rather than an oversight: all four types
/// Qwen writes carry something mogeung wants. Kept so the shape of this module
/// matches `adapter.rs` and `codex.rs`, and so the first type that earns a
/// skip has an obvious home.
pub const KNOWN_IGNORED: &[&str] = &[];

/// `system` subtypes this parser extracts data from.
///
/// A `system` record is a tagged union whose real discriminator is `subtype`,
/// so classification descends one level exactly as `codex::KNOWN_ITEMS` does.
/// An unrecognised subtype classifies the line as `Unknown { kind:
/// "system/<subtype>" }` — drift in the nested taxonomy is as loud as drift in
/// the outer one.
pub const HANDLED_SUBTYPES: &[&str] = &[
    // Token counts and tool-call outcomes, as OpenTelemetry-shaped events.
    "ui_telemetry",
    // What the human typed when they typed a command rather than a prompt.
    "slash_command",
    // A title the user set by hand.
    "custom_title",
    // History was summarised; the turn counters below it are no longer whole.
    "chat_compression",
    // A prompt sent while the agent was mid-turn.
    "mid_turn_user_message",
    // The definitive end-of-turn marker. Written only by the ACP/serve path
    // today, never by the interactive TUI, so it has never been observed —
    // handled anyway, because if it ever appears it outranks every heuristic
    // in `derive_status`.
    "turn_result",
];

/// `system` subtypes we have classified and deliberately skip.
///
/// Together with `HANDLED_SUBTYPES` this is the complete set of nineteen
/// `subtype:` literals in `chatRecordingService.ts` at 0.22.0. The other
/// subtype strings in the bundle (`can_use_tool`, `mcp_message`, `interrupt`,
/// …) belong to the ACP/serve protocol and are never written to a transcript;
/// if one turns up here the canary should say so rather than this list quietly
/// growing to cover it.
pub const KNOWN_IGNORED_SUBTYPES: &[&str] = &[
    // `@file` expansion, already reflected in the message that used it.
    "at_command",
    // Bookkeeping snapshots. The pre-edit file backup is the direct analogue
    // of Claude Code's `file-history-snapshot`, which ADR-0004 declines to
    // read; the rest are the same category.
    "attribution_snapshot",
    "branch_checkpoint",
    "file_history_snapshot",
    "session_artifact_event",
    "session_artifact_snapshot",
    // Qwen's "goal" feature: its own plan state, re-derivable from the turns
    // around it. `goal_state.snapshot.activity` looks like the busy/idle flag
    // this module needs and is not — `recordGoalState` hardcodes it to `idle`
    // on write, so reading it would be worse than reading nothing.
    "goal_runtime",
    "goal_state",
    // Resume/fork lineage and provenance labels.
    "parent_session",
    "session_source",
    "rewind",
    // UI-layer echoes of content recorded elsewhere.
    "realtime_message",
    "user_text_elements",
];

/// `provenance` values we have seen. Not a classification axis — `type` is the
/// discriminator — but a load-bearing field: `real_user` is what separates a
/// human turn from a synthetic goal-continuation turn whose text literally
/// reads *"This is a synthetic continuation turn."* Counting `type == "user"`
/// naively credits the human with a robot's turn.
///
/// Inventoried by `--bin sweep` rather than enforced here, in the same spirit
/// as the model and usage-key inventories: no upstream list exists to check it
/// against, and a new value appearing is information, not a failure.
pub const KNOWN_PROVENANCE: &[&str] = &[
    "real_user",
    "assistant_output",
    "tool_result",
    "system",
    "goal_runtime",
    "goal_control",
];

/// The outcome of reading one transcript line. Mirrors `adapter::LineOutcome`
/// and `codex::CodexLineOutcome` so Qwen drift feeds the same health
/// vocabulary as Claude and Codex drift.
#[derive(Debug)]
pub enum QwenLineOutcome {
    /// Understood, and it produced something.
    Parsed(Box<QwenParsed>),
    /// A known type we deliberately skip.
    Ignored,
    /// A handled type that yielded nothing this time.
    Barren { kind: String },
    /// A type (or `system/<subtype>`) we have never heard of. The canary.
    Unknown { kind: String },
    /// Not JSON, or no `type` field.
    Malformed,
}

impl QwenLineOutcome {
    pub fn class(&self) -> LineClass {
        match self {
            QwenLineOutcome::Parsed(_) => LineClass::Parsed,
            QwenLineOutcome::Ignored => LineClass::Ignored,
            QwenLineOutcome::Barren { .. } => LineClass::Barren,
            QwenLineOutcome::Unknown { .. } => LineClass::Unknown,
            QwenLineOutcome::Malformed => LineClass::Malformed,
        }
    }

    pub fn parsed(self) -> Option<QwenParsed> {
        match self {
            QwenLineOutcome::Parsed(p) => Some(*p),
            _ => None,
        }
    }
}

/// Token usage as Qwen names it.
///
/// Gemini's vocabulary, not Anthropic's and not Codex's: `promptTokenCount`
/// rather than `input_tokens` or `input_token_count`, and a single
/// `cachedContentTokenCount` where Claude splits cache reads from cache
/// writes. `thoughtsTokenCount` counts reasoning tokens, which the model
/// produced and the user paid for, so it belongs on the output side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub cached_tokens: u64,
    pub output_tokens: u64,
    pub thoughts_tokens: u64,
}

impl TokenUsage {
    /// `promptTokenCount` already includes `cachedContentTokenCount` — the
    /// cached share is a discount on the prompt, not an addition to it, and
    /// adding them would double-count every cache hit.
    pub fn total_in(&self) -> u64 {
        self.prompt_tokens
    }
    pub fn total_out(&self) -> u64 {
        self.output_tokens + self.thoughts_tokens
    }
}

/// What one transcript line means for the waiting/working heuristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailEvent {
    /// The human sent a real prompt.
    UserMessage,
    /// The model asked for a tool. Either it is running or the human is being
    /// asked to approve it — see [`derive_status`], which cannot tell.
    ToolRequested,
    /// A tool came back, so the model is about to be called again.
    ToolResult,
    /// The model finished speaking without asking for anything.
    TurnEnded,
    /// A `turn_result` record said the turn is over, in terms.
    TurnResult,
}

/// Everything one transcript line tells us.
#[derive(Debug, Default)]
pub struct QwenParsed {
    pub ts: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    /// The CLI's own version, e.g. `"0.22.0"`.
    pub cli_version: Option<String>,
    /// The model as the user's configuration names it (`qwen3.8-sglang`), not
    /// as the wire names it (`RadixArk/Qwen3.8-27B-NVFP4`). Both appear, for
    /// the same call, in different records; this is the one a user would
    /// recognise, because it is the one they typed into `settings.json`.
    pub model: Option<String>,
    /// A title the user set by hand.
    pub title: Option<String>,
    /// Message text, truncated. Reasoning parts (`thought: true`) are excluded
    /// — they are the model thinking, not the model speaking.
    pub text: Option<String>,
    /// This line is a fresh prompt from the human, not a synthetic
    /// continuation. Keyed on `provenance == "real_user"`.
    pub is_turn: bool,
    pub tool_calls: u32,
    pub last_activity: Option<String>,
    /// Usage seen on this line. Per-call and therefore additive, unlike
    /// Codex's cumulative totals.
    pub usage: Option<TokenUsage>,
    /// The model's context window for this call, so prompt size can be read as
    /// pressure rather than as a bare number.
    pub context_window: Option<u64>,
    pub tail: Option<TailEvent>,
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// Read one transcript line.
///
/// Never panics and never fails: an unreadable line becomes an outcome, per
/// ADR-0007 and this repo's standing rule for undocumented formats. Qwen Code's
/// own reader routes every line through a `parseLineTolerant` that exists
/// because it has seen `}{`-glued lines in the wild (their issue #3606), so
/// tolerance here is matching their posture, not being defensive for its own
/// sake.
pub fn parse_line(line: &str) -> QwenLineOutcome {
    let line = line.trim();
    if line.is_empty() {
        return QwenLineOutcome::Malformed;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return QwenLineOutcome::Malformed;
    };
    let Some(kind) = v.get("type").and_then(|t| t.as_str()) else {
        return QwenLineOutcome::Malformed;
    };

    if KNOWN_IGNORED.contains(&kind) {
        return QwenLineOutcome::Ignored;
    }
    if !HANDLED.contains(&kind) {
        return QwenLineOutcome::Unknown {
            kind: kind.to_string(),
        };
    }

    // The envelope every record shares.
    let mut p = QwenParsed {
        ts: str_field(&v, "timestamp")
            .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
            .map(|t| t.with_timezone(&Utc)),
        cwd: str_field(&v, "cwd"),
        git_branch: str_field(&v, "gitBranch"),
        cli_version: str_field(&v, "version").filter(|s| s != "unknown"),
        ..Default::default()
    };

    let provenance = v.get("provenance").and_then(|x| x.as_str()).unwrap_or("");
    let mut yielded = false;

    match kind {
        "user" => {
            // `real_user` is the whole point: a `goal_runtime` user record is
            // the CLI talking to itself.
            p.is_turn = provenance == "real_user";
            if let Some(t) = content_text(&v) {
                if p.is_turn {
                    p.text = Some(truncate(&t, 400));
                }
                yielded = true;
            }
            if p.is_turn {
                p.tail = Some(TailEvent::UserMessage);
                yielded = true;
            }
        }
        "assistant" => {
            p.model = str_field(&v, "model");
            p.context_window = v.get("contextWindowSize").and_then(|x| x.as_u64());
            p.usage = usage_metadata(&v);
            let calls = function_calls(&v);
            p.tool_calls = calls.len() as u32;
            if let Some(name) = calls.first() {
                p.last_activity = Some(truncate(name, 80));
                p.tail = Some(TailEvent::ToolRequested);
            } else {
                if let Some(t) = content_text(&v) {
                    p.text = Some(truncate(&t, 400));
                }
                // Text and nothing asked for: the turn is over.
                p.tail = Some(TailEvent::TurnEnded);
            }
            // An assistant record always settles the tail, so it always says
            // something even when it carries no usage and no model.
            yielded = true;
        }
        "tool_result" => {
            p.tail = Some(TailEvent::ToolResult);
            if let Some(r) = v.get("toolCallResult") {
                if let Some(d) = result_display(r) {
                    p.last_activity = Some(truncate(&d, 80));
                }
            }
            yielded = true;
        }
        "system" => {
            let Some(sub) = v.get("subtype").and_then(|s| s.as_str()) else {
                // A `system` record with no subtype has no discriminator and
                // so no shape we can name. Barren rather than unknown: the
                // type is one we handle, this instance just said nothing.
                return QwenLineOutcome::Barren {
                    kind: "system".into(),
                };
            };
            if KNOWN_IGNORED_SUBTYPES.contains(&sub) {
                return QwenLineOutcome::Ignored;
            }
            if !HANDLED_SUBTYPES.contains(&sub) {
                return QwenLineOutcome::Unknown {
                    kind: format!("system/{sub}"),
                };
            }
            yielded = parse_system(&v, sub, &mut p);
        }
        _ => unreachable!("kind was checked against HANDLED above"),
    }

    if yielded {
        QwenLineOutcome::Parsed(Box::new(p))
    } else {
        QwenLineOutcome::Barren {
            kind: kind.to_string(),
        }
    }
}

/// The `system` subtypes we read. Returns whether anything was extracted.
fn parse_system(v: &Value, sub: &str, p: &mut QwenParsed) -> bool {
    let payload = v.get("systemPayload");
    match sub {
        "ui_telemetry" => {
            let Some(ev) = payload.and_then(|x| x.get("uiEvent")) else {
                return false;
            };
            // Literal dotted keys, not nesting — these come straight from an
            // OpenTelemetry attribute bag.
            match ev.get("event.name").and_then(|x| x.as_str()).unwrap_or("") {
                "qwen-code.tool_call" => {
                    if let Some(name) = str_field(ev, "function_name") {
                        p.last_activity = Some(truncate(&name, 80));
                        return true;
                    }
                    false
                }
                // Deliberately does **not** contribute tokens. The same call's
                // usage is already on the `assistant` record, and counting
                // both would double every figure mogeung reports.
                "qwen-code.api_response" => str_field(ev, "model").is_some(),
                // Surfaced as activity, deliberately **not** as `Session.error`.
                // Attention treats an error as terminal (tier 900, ahead of
                // everything but a permission prompt), and these are routinely
                // retried and recovered from — a transient 429 against a local
                // endpoint would otherwise pin the session as failed for the
                // rest of its life. As activity it is visible and it is
                // superseded by whatever happens next, which is the truth.
                "qwen-code.api_error" => {
                    let detail = str_field(ev, "error_type")
                        .or_else(|| str_field(ev, "error_message"))
                        .or_else(|| {
                            ev.get("status_code")
                                .and_then(|c| c.as_u64())
                                .map(|c| c.to_string())
                        });
                    p.last_activity = Some(match detail {
                        Some(d) => truncate(&format!("API error: {d}"), 80),
                        None => "API error".into(),
                    });
                    true
                }
                _ => false,
            }
        }
        "slash_command" => {
            // The pair is emitted twice, `invocation` then `result`; take the
            // invocation so the activity line reads as what the user asked
            // for rather than as what came back.
            let phase = payload
                .and_then(|x| x.get("phase"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            if phase != "invocation" {
                return false;
            }
            match payload.and_then(|x| str_field(x, "rawCommand")) {
                Some(raw) => {
                    p.last_activity = Some(truncate(&raw, 80));
                    p.text = Some(truncate(&raw, 400));
                    // A command the human typed **is** a turn, and often the
                    // only one: a session opened with `/goal …` never writes a
                    // `real_user` record at all, so counting only those reported
                    // it as zero turns with no prompt — which is what the first
                    // run against a real `~/.qwen` showed. `hiddenInvocation`
                    // marks the CLI invoking a command on its own behalf, which
                    // is not the human and does not count.
                    let hidden = payload
                        .and_then(|x| x.get("hiddenInvocation"))
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false);
                    if !hidden {
                        p.is_turn = true;
                        p.tail = Some(TailEvent::UserMessage);
                    }
                    true
                }
                None => false,
            }
        }
        "custom_title" => {
            let title = payload
                .and_then(|x| str_field(x, "title"))
                .or_else(|| payload.and_then(|x| str_field(x, "customTitle")));
            match title {
                Some(t) => {
                    p.title = Some(truncate(&t, 120));
                    true
                }
                None => false,
            }
        }
        "mid_turn_user_message" => {
            p.tail = Some(TailEvent::UserMessage);
            true
        }
        "chat_compression" => {
            p.last_activity = Some("compacted history".into());
            true
        }
        "turn_result" => {
            p.tail = Some(TailEvent::TurnResult);
            true
        }
        _ => false,
    }
}

/// Concatenate the speaking parts of a Gemini `Content`, skipping reasoning.
fn content_text(v: &Value) -> Option<String> {
    let parts = v.get("message")?.get("parts")?.as_array()?;
    let mut out = String::new();
    for part in parts {
        if part.get("thought").and_then(|t| t.as_bool()) == Some(true) {
            continue;
        }
        if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
            if !t.trim().is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(t.trim());
            }
        }
    }
    Some(out).filter(|s| !s.is_empty())
}

/// The names of every `functionCall` part on a `Content`.
///
/// Tool calls are not their own record type in Qwen: the request rides inside
/// the `assistant` record and only the response gets a record of its own.
fn function_calls(v: &Value) -> Vec<String> {
    let Some(parts) = v
        .get("message")
        .and_then(|m| m.get("parts"))
        .and_then(|p| p.as_array())
    else {
        return Vec::new();
    };
    parts
        .iter()
        .filter_map(|p| p.get("functionCall"))
        .map(|c| {
            c.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("tool")
                .to_string()
        })
        .collect()
}

fn usage_metadata(v: &Value) -> Option<TokenUsage> {
    let u = v.get("usageMetadata")?;
    let get = |k: &str| u.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let usage = TokenUsage {
        prompt_tokens: get("promptTokenCount"),
        cached_tokens: get("cachedContentTokenCount"),
        output_tokens: get("candidatesTokenCount"),
        thoughts_tokens: get("thoughtsTokenCount"),
    };
    if usage == TokenUsage::default() {
        return None;
    }
    Some(usage)
}

/// `resultDisplay` is a string most of the time and an object sometimes.
///
/// `None` when it says nothing useful, and that matters: the caller leaves
/// `last_activity` alone rather than overwriting it. The preceding `assistant`
/// record has already put the **tool's name** there, and replacing
/// `"read_file"` with a generic `"tool result"` loses the only part a human
/// would have wanted to read.
fn result_display(r: &Value) -> Option<String> {
    match r.get("resultDisplay") {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(Value::Object(o)) => o
            .get("type")
            .and_then(|t| t.as_str())
            .map(|t| format!("({t})")),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The live registry
// ---------------------------------------------------------------------------

/// The registry schema this reader understands.
///
/// Qwen's own reader rejects anything above its constant, which is how they
/// signal a break; matching that is how mogeung avoids reading a v2 record as
/// though it were a v1.
const REGISTRY_SCHEMA_VERSION: u64 = 1;

/// One entry of Qwen Code's live-session registry.
#[derive(Debug, Clone)]
pub struct QwenLiveEntry {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct RawLive {
    #[serde(rename = "schemaVersion")]
    schema_version: Option<u64>,
    pid: u32,
    #[serde(rename = "procStart")]
    proc_start: Option<String>,
    #[serde(rename = "pidNs")]
    pid_ns: Option<u64>,
    #[serde(rename = "sessionId")]
    session_id: String,
    cwd: String,
    name: Option<String>,
    #[serde(rename = "qwenVersion")]
    qwen_version: Option<String>,
    #[serde(rename = "startedAt")]
    started_at: Option<i64>,
}

/// Is this process still running?
///
/// The registry files *are* unlinked on a clean exit, unlike Claude Code's, but
/// a crash leaves one behind and the sidecar beside the transcript is never
/// cleaned up at all — so liveness is still the OS's answer, not the file's.
fn pid_alive(pid: u32) -> bool {
    // Not a real pid, and not a harmless one to pass on: `kill(0, 0)` signals
    // the *caller's own process group*, so it succeeds and a truncated or
    // half-written registry record would read as a live session forever.
    if pid == 0 {
        return false;
    }
    unsafe { libc_kill(pid as i32, 0) == 0 }
}

/// The `<boot-id>:<starttime>` token Qwen writes into `procStart`, recomputed
/// for a pid now — plus whether that process is a zombie.
///
/// `None` where `/proc` cannot answer, which is every non-Linux machine and any
/// pid that has already gone.
///
/// Field 22 of `/proc/<pid>/stat` is `starttime`, and it is read from **after
/// the last `)`** because field 2 is the executable name and a process is free
/// to have a `)` in it. Everything after that close-paren is fixed-width by
/// position: index 0 is the state character, index 19 is `starttime`.
#[cfg(target_os = "linux")]
fn proc_start_token(pid: u32) -> Option<(String, bool)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let tail = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = tail.split_whitespace().collect();
    let state = fields.first()?;
    let starttime = fields.get(19)?;
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    Some((
        format!("{}:{}", boot.trim(), starttime),
        *state == "Z",
    ))
}

#[cfg(not(target_os = "linux"))]
fn proc_start_token(_pid: u32) -> Option<(String, bool)> {
    None
}

/// Is the process behind this record still *the same process*?
///
/// `kill(pid, 0)` answers "does some process have this pid", which is a
/// different question and the reason a closed session could sit in the queue
/// reporting itself busy. Two ways it says yes when it should say no:
///
/// * **A zombie.** A qwen killed with its parent still around stays in the
///   table as `Z` until reaped, and signals to it succeed. Nothing is running;
///   it just has not been buried. This is the likely reading of "the session is
///   closed and still shows as live, then goes STALLED" — a defunct process
///   answering for a session that ended, then falling silent because it is not
///   doing anything.
/// * **A reused pid.** Qwen unlinks its record on a clean exit, but a crash or
///   a `tmux kill-session` leaves it behind, and pids wrap. The next process to
///   land on that number inherits a dead session's identity.
///
/// Qwen guards against both with `procStart` — a boot id and the process's
/// start time — and `pidNs`, and so does this: the record is only believed when
/// the process it names started when the record says it did. Where `/proc`
/// cannot answer (macOS, where Qwen writes `procStart: null` for the same
/// reason) this degrades to liveness alone rather than refusing every session.
fn same_process(pid: u32, proc_start: Option<&str>, pid_ns: Option<u64>) -> bool {
    if !pid_alive(pid) {
        return false;
    }
    let Some((token, zombie)) = proc_start_token(pid) else {
        // No `/proc` to ask. Believe the record — the alternative is a platform
        // where no Qwen session is ever live.
        return true;
    };
    if zombie {
        return false;
    }
    match proc_start {
        // The record names a start time and it is not this process's. The pid
        // has been reused.
        Some(recorded) => recorded == token && pid_namespace().is_none_or(|ns| pid_ns.is_none_or(|r| r == ns)),
        // Written without one — Qwen refuses to write an impersonable record on
        // Linux, so this is a record from a platform that has no token to give.
        None => true,
    }
}

/// The inode of this process's pid namespace, which is what Qwen records.
///
/// A record from inside a container names a namespace we are not in, and its
/// pids mean nothing here — see `R-I13`, which is the same observation from the
/// other side.
#[cfg(target_os = "linux")]
fn pid_namespace() -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self/ns/pid").ok().map(|m| m.ino())
}

#[cfg(not(target_os = "linux"))]
fn pid_namespace() -> Option<u64> {
    None
}

extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

/// Read the live registry. Only records whose process is still running come
/// back.
///
/// Note what is *not* here: a status field. Claude Code's registry says
/// `busy`/`idle` and that single string is the most valuable thing the observer
/// model buys. Qwen's does not, so status has to be inferred from the
/// transcript tail — see [`derive_status`].
pub fn scan_live(home: &Path) -> Vec<QwenLiveEntry> {
    let dir = home.join("sessions");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        // `<pid>.json`, and never the `<pid>.json.<12hex>.tmp` files a
        // half-finished atomic write leaves behind.
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let named_by_pid = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty())
            .unwrap_or(false);
        if !named_by_pid {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(raw) = serde_json::from_str::<RawLive>(&text) else {
            continue;
        };
        if raw.schema_version.unwrap_or(REGISTRY_SCHEMA_VERSION) > REGISTRY_SCHEMA_VERSION {
            continue;
        }
        // Identity, not just existence — see `same_process`. A zombie and a
        // reused pid both answer `kill(pid, 0)`, and either would keep a closed
        // session sitting in the queue calling itself live.
        if !same_process(raw.pid, raw.proc_start.as_deref(), raw.pid_ns) {
            continue;
        }
        out.push(QwenLiveEntry {
            pid: raw.pid,
            session_id: raw.session_id,
            cwd: raw.cwd,
            name: raw.name,
            version: raw.qwen_version,
            started_at: raw
                .started_at
                .and_then(|ms| Utc.timestamp_millis_opt(ms).single()),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Transcript discovery
// ---------------------------------------------------------------------------

/// A Qwen Code installation on disk (the `~/.qwen` directory).
#[derive(Debug, Clone)]
pub struct QwenInstall {
    pub home: PathBuf,
}

impl QwenInstall {
    /// `Some` iff the directory exists. An existing-but-empty install is still
    /// an install — "present, no sessions" is a different answer from "no Qwen
    /// here" and the product must be able to say which.
    pub fn discover(home: &Path) -> Option<QwenInstall> {
        if home.is_dir() {
            Some(QwenInstall {
                home: home.to_path_buf(),
            })
        } else {
            None
        }
    }

    pub fn transcripts(&self, max_age_days: i64) -> Vec<QwenTranscript> {
        scan_transcripts(&self.home, max_age_days)
    }
}

/// A transcript file on disk.
#[derive(Debug, Clone)]
pub struct QwenTranscript {
    pub session_id: String,
    pub path: PathBuf,
    pub modified: DateTime<Utc>,
    pub size: u64,
}

/// Does this filename name a session transcript?
///
/// Qwen's own `SESSION_FILE_PATTERN` is `/^[0-9a-fA-F-]{32,36}\.jsonl$/`, and
/// the sidecars sharing the directory (`.ledger.jsonl`, and the `.runtime.json`
/// / `.worktree.json` / `.pr.json` trio) would otherwise be read as sessions.
/// Subagent transcripts (`agent-<id>.jsonl`) fail the pattern too, which is
/// what we want: a subagent is not a session a human is waiting on.
fn session_file_name(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(".jsonl")?;
    if stem.ends_with(".ledger") {
        return None;
    }
    let n = stem.len();
    if !(32..=36).contains(&n) {
        return None;
    }
    if !stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return None;
    }
    Some(stem)
}

/// Find every session transcript, newest first.
///
/// Two levels deeper than Claude's layout (`projects/<slug>/chats/`), and
/// `chats/archive/` is included — an archived session is still a session that
/// touched a repository, and skipping the directory would make sessions
/// silently vanish from history rather than age out of it.
pub fn scan_transcripts(home: &Path, max_age_days: i64) -> Vec<QwenTranscript> {
    let root = home.join("projects");
    let mut out = Vec::new();
    let cutoff = Utc::now() - chrono::Duration::days(max_age_days);

    let Ok(projects) = std::fs::read_dir(&root) else {
        return out;
    };
    for project in projects.flatten() {
        let chats = project.path().join("chats");
        for dir in [chats.clone(), chats.join("archive")] {
            let Ok(files) = std::fs::read_dir(&dir) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                let Some(session_id) = session_file_name(name) else {
                    continue;
                };
                let Ok(meta) = f.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let modified: DateTime<Utc> = meta
                    .modified()
                    .map(DateTime::<Utc>::from)
                    .unwrap_or_else(|_| Utc::now());
                if modified < cutoff {
                    continue;
                }
                out.push(QwenTranscript {
                    session_id: session_id.to_string(),
                    path: path.clone(),
                    modified,
                    size: meta.len(),
                });
            }
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

// ---------------------------------------------------------------------------
// Folding a transcript into a session
// ---------------------------------------------------------------------------

/// What a whole transcript adds up to.
#[derive(Debug, Default, Clone)]
pub struct QwenThread {
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub cli_version: Option<String>,
    pub model: Option<String>,
    pub title: Option<String>,
    /// The first real human prompt — what the session is *about*.
    pub first_prompt: Option<String>,
    pub last_activity: Option<String>,
    pub turns: u32,
    pub tool_calls: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub context_window: Option<u64>,
    pub last_ts: Option<DateTime<Utc>>,
    pub tail: Option<TailEvent>,
    pub counts: LineCounts,
}

/// Per-thread classification tallies. Cumulative, so an incremental tail adds
/// up to exactly what a whole re-read would have counted.
#[derive(Debug, Default, Clone)]
pub struct LineCounts {
    pub seen: u64,
    pub parsed: u64,
    pub ignored: u64,
    pub barren: u64,
    pub malformed: u64,
    pub unknown_kinds: HashMap<String, u64>,
}

impl QwenThread {
    /// Fold one already-parsed line in.
    fn absorb(&mut self, p: QwenParsed) {
        if let Some(c) = p.cwd {
            self.cwd = Some(c);
        }
        if p.git_branch.is_some() {
            self.git_branch = p.git_branch;
        }
        if p.cli_version.is_some() {
            self.cli_version = p.cli_version;
        }
        if p.model.is_some() {
            self.model = p.model;
        }
        if p.title.is_some() {
            self.title = p.title;
        }
        if p.context_window.is_some() {
            self.context_window = p.context_window;
        }
        if p.is_turn {
            self.turns += 1;
            if self.first_prompt.is_none() {
                self.first_prompt = p.text.clone();
            }
        }
        self.tool_calls += p.tool_calls;
        if let Some(u) = p.usage {
            // Per-call, so additive — the opposite of Codex, whose rollout
            // reports cumulative totals that must be adopted rather than summed.
            self.tokens_in += u.total_in();
            self.tokens_out += u.total_out();
        }
        if p.last_activity.is_some() {
            self.last_activity = p.last_activity;
        }
        if let Some(t) = p.ts {
            self.last_ts = Some(t);
        }
        if p.tail.is_some() {
            self.tail = p.tail;
        }
    }

    /// Fold one raw line in, classifying it on the way.
    pub fn absorb_line(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        self.counts.seen += 1;
        let outcome = parse_line(line);
        match outcome.class() {
            LineClass::Parsed => self.counts.parsed += 1,
            LineClass::Ignored => self.counts.ignored += 1,
            LineClass::Barren => self.counts.barren += 1,
            LineClass::Malformed => self.counts.malformed += 1,
            LineClass::Unknown => {}
        }
        if let QwenLineOutcome::Unknown { kind } = &outcome {
            *self.counts.unknown_kinds.entry(kind.clone()).or_insert(0) += 1;
        }
        if let Some(p) = outcome.parsed() {
            self.absorb(p);
        }
    }

    pub fn status(&self) -> QwenStatus {
        derive_status(self.tail)
    }
}

/// Busy, waiting, or finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QwenStatus {
    /// The agent has work in flight.
    Working,
    /// The agent has stopped and the next move is the human's.
    Waiting,
}

/// Infer status from the last thing the transcript said.
///
/// **This is inference, and the gap it leaves is the important part.** Qwen
/// Code does hold the answer — `streamingState` is an enum of `idle` /
/// `responding` / `waiting_for_confirmation` — but it lives in React state and
/// is never written to disk. The one record that would settle it, `turn_result`,
/// is written only by the ACP/serve path and never by the interactive CLI a
/// human runs.
///
/// So a trailing tool request is reported as `Working`, and it might instead be
/// a permission prompt that has been sitting there for ten minutes. That is
/// exactly the distinction mogeung exists to draw (`R-B4`), and for Qwen it
/// currently cannot. Better to say so here than to have the queue quietly
/// mis-tier it.
pub fn derive_status(tail: Option<TailEvent>) -> QwenStatus {
    match tail {
        Some(TailEvent::TurnEnded) | Some(TailEvent::TurnResult) | None => QwenStatus::Waiting,
        Some(TailEvent::UserMessage)
        | Some(TailEvent::ToolRequested)
        | Some(TailEvent::ToolResult) => QwenStatus::Working,
    }
}

impl QwenStatus {
    pub fn live_status(self) -> LiveStatus {
        match self {
            QwenStatus::Working => LiveStatus::Busy,
            QwenStatus::Waiting => LiveStatus::Idle,
        }
    }
}

/// Read a whole transcript. Used by `--bin sweep` and by the tests; the scan
/// loop goes through [`ScanCache`] instead.
pub fn read_thread(path: &Path) -> QwenThread {
    let mut thread = QwenThread::default();
    let Ok(file) = std::fs::File::open(path) else {
        return thread;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        thread.absorb_line(&line);
    }
    thread
}

// ---------------------------------------------------------------------------
// Incremental tailing
// ---------------------------------------------------------------------------

/// Per-session tail state: how far we have read, and the fold so far.
///
/// Without this every poll re-parses every transcript whole, which is a
/// full-corpus parse at the poll rate — the cost that `R-J8` removed for Codex.
#[derive(Default)]
pub struct ScanCache {
    threads: HashMap<String, (u64, QwenThread)>,
}

impl ScanCache {
    /// Read whatever has been appended since the last pass and return the
    /// running fold.
    ///
    /// A file that shrank was rewritten or replaced, so the fold is restarted
    /// from zero rather than resumed at an offset that now means something
    /// else.
    pub fn update(&mut self, t: &QwenTranscript) -> QwenThread {
        let entry = self
            .threads
            .entry(t.session_id.clone())
            .or_insert_with(|| (0, QwenThread::default()));
        if t.size < entry.0 {
            *entry = (0, QwenThread::default());
        }
        if t.size == entry.0 {
            return entry.1.clone();
        }
        let Ok(mut file) = std::fs::File::open(&t.path) else {
            return entry.1.clone();
        };
        if file.seek(SeekFrom::Start(entry.0)).is_err() {
            return entry.1.clone();
        }
        let mut read = entry.0;
        let mut reader = BufReader::new(&mut file);
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    // A final line with no newline yet is a partial write:
                    // leave the offset before it so the next pass sees it whole.
                    if !buf.ends_with('\n') {
                        break;
                    }
                    read += n as u64;
                    entry.1.absorb_line(&buf);
                }
                Err(_) => break,
            }
        }
        entry.0 = read;
        entry.1.clone()
    }

    /// Forget sessions that are no longer on disk.
    pub fn retain(&mut self, keep: &std::collections::HashSet<String>) {
        self.threads.retain(|id, _| keep.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One real assistant record, trimmed of prose but structurally verbatim
    /// from `~/.qwen` at Qwen Code 0.22.0.
    const ASSISTANT_TOOL: &str = r#"{"uuid":"94a2eeb5","parentUuid":"39287ee5","sessionId":"4ade0baa","timestamp":"2026-08-25T11:45:25.697Z","type":"assistant","provenance":"assistant_output","cwd":"/home/kinz/temp/tic-tac-toe","version":"0.22.0","model":"qwen3.8-sglang","message":{"role":"model","parts":[{"text":"I need to look into this.","thought":true},{"functionCall":{"id":"call_d45","name":"get_goal","args":{}}}]},"usageMetadata":{"promptTokenCount":35575,"candidatesTokenCount":42,"thoughtsTokenCount":27,"totalTokenCount":35617,"cachedContentTokenCount":24640},"contextWindowSize":1000000}"#;

    const REAL_USER: &str = r#"{"uuid":"a","parentUuid":null,"sessionId":"s","timestamp":"2026-08-25T11:45:13.242Z","type":"user","provenance":"real_user","cwd":"/home/kinz/scripts","version":"0.22.0","message":{"role":"user","parts":[{"text":"make the tests pass"}]}}"#;

    const SYNTHETIC_USER: &str = r#"{"uuid":"b","parentUuid":"a","sessionId":"s","timestamp":"2026-08-25T11:46:13.242Z","type":"user","subtype":"goal_runtime","provenance":"goal_runtime","cwd":"/home/kinz/scripts","version":"0.22.0","message":{"role":"user","parts":[{"text":"This is a synthetic continuation turn."}]}}"#;

    #[test]
    fn a_thought_is_not_something_the_agent_said() {
        let p = parse_line(ASSISTANT_TOOL).parsed().expect("parsed");
        // The reasoning part must not leak into the text we show a human.
        assert_eq!(p.text, None);
        assert_eq!(p.tool_calls, 1);
        assert_eq!(p.last_activity.as_deref(), Some("get_goal"));
    }

    /// The trap this whole `provenance` axis exists for: Qwen writes its own
    /// continuation prompts as `type: "user"`, so counting turns by type
    /// credits the human with the robot's work.
    #[test]
    fn a_synthetic_continuation_is_not_a_human_turn() {
        let real = parse_line(REAL_USER).parsed().expect("parsed");
        assert!(real.is_turn);
        assert_eq!(real.text.as_deref(), Some("make the tests pass"));

        let synthetic = parse_line(SYNTHETIC_USER).parsed();
        assert!(
            synthetic.map(|p| !p.is_turn).unwrap_or(true),
            "a goal_runtime user record must not count as a turn"
        );
    }

    /// `promptTokenCount` already contains the cached share. Adding them
    /// double-counts every cache hit, which on this record would report 60215
    /// input tokens where 35575 were sent.
    #[test]
    fn cached_tokens_are_a_discount_not_an_addition() {
        let p = parse_line(ASSISTANT_TOOL).parsed().expect("parsed");
        let u = p.usage.expect("usage");
        assert_eq!(u.total_in(), 35575);
        assert_eq!(u.total_out(), 42 + 27);
    }

    #[test]
    fn an_unknown_type_is_named_not_swallowed() {
        let line = r#"{"type":"telepathy","sessionId":"s"}"#;
        match parse_line(line) {
            QwenLineOutcome::Unknown { kind } => assert_eq!(kind, "telepathy"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    /// Drift one level down must be as loud as drift at the top, or the
    /// nineteen-way `system` union becomes a blind spot the size of 58% of
    /// every transcript.
    #[test]
    fn an_unknown_system_subtype_is_named_with_its_parent() {
        let line = r#"{"type":"system","subtype":"quantum_state","provenance":"system"}"#;
        match parse_line(line) {
            QwenLineOutcome::Unknown { kind } => assert_eq!(kind, "system/quantum_state"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn a_known_bookkeeping_subtype_is_skipped_quietly() {
        let line = r#"{"type":"system","subtype":"file_history_snapshot","provenance":"system"}"#;
        assert!(matches!(parse_line(line), QwenLineOutcome::Ignored));
    }

    #[test]
    fn a_torn_line_degrades_and_never_panics() {
        for line in [
            "",
            "not json at all",
            "{\"no\":\"type\"}",
            r#"{"type":"assistant"}{"type":"user"}"#,
            r#"{"type":"assistant","message":{"parts":"not an array"}}"#,
        ] {
            let _ = parse_line(line);
        }
    }

    #[test]
    fn a_trailing_tool_request_reads_as_working_and_a_finished_turn_does_not() {
        assert_eq!(
            derive_status(Some(TailEvent::ToolRequested)),
            QwenStatus::Working
        );
        assert_eq!(derive_status(Some(TailEvent::TurnEnded)), QwenStatus::Waiting);
        assert_eq!(derive_status(None), QwenStatus::Waiting);
    }

    /// The sidecars live in the same directory as the transcripts, and three
    /// of the four end in `.json`/`.jsonl`.
    #[test]
    fn a_sidecar_is_not_a_session() {
        assert_eq!(
            session_file_name("4ade0baa-aa19-411b-9ddb-c86b98da7f50.jsonl"),
            Some("4ade0baa-aa19-411b-9ddb-c86b98da7f50")
        );
        for not_a_session in [
            "4ade0baa-aa19-411b-9ddb-c86b98da7f50.runtime.json",
            "4ade0baa-aa19-411b-9ddb-c86b98da7f50.ledger.jsonl",
            "4ade0baa-aa19-411b-9ddb-c86b98da7f50.worktree.json",
            "agent-abc123.jsonl",
            "notes.jsonl",
        ] {
            assert_eq!(session_file_name(not_a_session), None, "{not_a_session}");
        }
    }

    /// Every subtype the writer can emit must be classified one way or the
    /// other, or the canary fires on normal operation.
    #[test]
    fn the_subtype_taxonomy_is_complete_and_disjoint() {
        // The nineteen `subtype:` literals in chatRecordingService.ts, 0.22.0.
        const ALL: &[&str] = &[
            "at_command",
            "attribution_snapshot",
            "branch_checkpoint",
            "chat_compression",
            "custom_title",
            "file_history_snapshot",
            "goal_runtime",
            "goal_state",
            "mid_turn_user_message",
            "parent_session",
            "realtime_message",
            "rewind",
            "session_artifact_event",
            "session_artifact_snapshot",
            "session_source",
            "slash_command",
            "turn_result",
            "ui_telemetry",
            "user_text_elements",
        ];
        for s in ALL {
            let handled = HANDLED_SUBTYPES.contains(s);
            let ignored = KNOWN_IGNORED_SUBTYPES.contains(s);
            assert!(handled ^ ignored, "{s} is unclassified or classified twice");
        }
        assert_eq!(
            HANDLED_SUBTYPES.len() + KNOWN_IGNORED_SUBTYPES.len(),
            ALL.len(),
            "a subtype was classified that the writer cannot emit"
        );
    }

    #[test]
    fn a_thread_folds_turns_tokens_and_tools() {
        let mut t = QwenThread::default();
        for line in [REAL_USER, ASSISTANT_TOOL, SYNTHETIC_USER] {
            t.absorb_line(line);
        }
        assert_eq!(t.turns, 1, "only the real_user record is a turn");
        assert_eq!(t.tool_calls, 1);
        assert_eq!(t.tokens_in, 35575);
        assert_eq!(t.first_prompt.as_deref(), Some("make the tests pass"));
        assert_eq!(t.model.as_deref(), Some("qwen3.8-sglang"));
        assert_eq!(t.cwd.as_deref(), Some("/home/kinz/scripts"));
        assert!(t.counts.unknown_kinds.is_empty());
    }

    /// Found against the real `~/.qwen`: a session opened with `/goal …`
    /// reported **zero turns and no prompt**, because its only `user` record
    /// was the synthetic continuation and the human's actual words were in a
    /// `slash_command`.
    #[test]
    fn a_command_the_human_typed_is_a_turn() {
        let line = r#"{"uuid":"s1","sessionId":"s","timestamp":"2026-08-25T11:45:13.242Z","type":"system","subtype":"slash_command","provenance":"system","systemPayload":{"phase":"invocation","hiddenInvocation":false,"sentToModel":false,"rawCommand":"/goal build a tic-tac-toe game"}}"#;
        let p = parse_line(line).parsed().expect("parsed");
        assert!(p.is_turn);
        assert_eq!(p.text.as_deref(), Some("/goal build a tic-tac-toe game"));
        assert_eq!(p.tail, Some(TailEvent::UserMessage));
    }

    /// ...but the CLI invoking a command on its own behalf is not the human,
    /// and the `result` half of the pair is an echo of the same command.
    #[test]
    fn a_hidden_or_echoed_command_is_not_a_turn() {
        let hidden = r#"{"uuid":"s2","sessionId":"s","type":"system","subtype":"slash_command","provenance":"system","systemPayload":{"phase":"invocation","hiddenInvocation":true,"rawCommand":"/compact"}}"#;
        let p = parse_line(hidden).parsed().expect("parsed");
        assert!(!p.is_turn, "the CLI invoked this, not the human");

        let echo = r#"{"uuid":"s3","sessionId":"s","type":"system","subtype":"slash_command","provenance":"system","systemPayload":{"phase":"result","rawCommand":"/goal build a tic-tac-toe game"}}"#;
        assert!(
            parse_line(echo).parsed().map(|p| !p.is_turn).unwrap_or(true),
            "the result half must not count the same command twice"
        );
    }

    /// A tool result must not replace the tool's name with a generic label —
    /// `read_file` is what a human wanted to read, `tool result` is not.
    #[test]
    fn a_featureless_tool_result_leaves_the_tool_name_standing() {
        let mut t = QwenThread::default();
        t.absorb_line(ASSISTANT_TOOL);
        assert_eq!(t.last_activity.as_deref(), Some("get_goal"));
        t.absorb_line(
            r#"{"uuid":"r","sessionId":"s","type":"tool_result","provenance":"tool_result","toolCallResult":{"callId":"call_d45","status":"success"}}"#,
        );
        assert_eq!(t.last_activity.as_deref(), Some("get_goal"));
        // A result that does say something still wins.
        t.absorb_line(
            r#"{"uuid":"r2","sessionId":"s","type":"tool_result","provenance":"tool_result","toolCallResult":{"callId":"c2","status":"success","resultDisplay":"Active goal · revision 1"}}"#,
        );
        assert_eq!(t.last_activity.as_deref(), Some("Active goal · revision 1"));
    }

    /// An API error is activity, not a terminal state: attention ranks
    /// `Session.error` above almost everything, and these get retried.
    #[test]
    fn an_api_error_is_activity_rather_than_a_terminal_failure() {
        let line = r#"{"uuid":"e","sessionId":"s","timestamp":"2026-08-25T11:45:25.684Z","type":"system","subtype":"ui_telemetry","provenance":"system","systemPayload":{"uiEvent":{"event.name":"qwen-code.api_error","error_type":"RateLimitError","error_message":"429 too many requests","model":"m","auth_type":"openai"}}}"#;
        let p = parse_line(line).parsed().expect("parsed");
        assert_eq!(p.last_activity.as_deref(), Some("API error: RateLimitError"));
        // And it must not move the tail: the turn is not over because one call
        // failed, and the next line is what settles it.
        assert_eq!(p.tail, None);
    }

    /// The same API call is reported twice — once as `usageMetadata` on the
    /// assistant record, once as an `api_response` telemetry record. Counting
    /// both doubles every number the Analytics view shows.
    #[test]
    fn telemetry_does_not_double_count_the_tokens_the_assistant_record_carried() {
        let telemetry = r#"{"uuid":"c","sessionId":"s","timestamp":"2026-08-25T11:45:25.684Z","type":"system","subtype":"ui_telemetry","provenance":"system","systemPayload":{"uiEvent":{"event.name":"qwen-code.api_response","model":"RadixArk/Qwen3.8-27B-NVFP4","input_token_count":35575,"output_token_count":42,"total_token_count":35617}}}"#;
        let mut t = QwenThread::default();
        t.absorb_line(ASSISTANT_TOOL);
        t.absorb_line(telemetry);
        assert_eq!(t.tokens_in, 35575, "counted once, not twice");
        assert_eq!(t.tokens_out, 69);
        // And the model shown stays the one the user configured.
        assert_eq!(t.model.as_deref(), Some("qwen3.8-sglang"));
    }
}
