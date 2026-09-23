//! Reader for opencode's session database. `R-I25`, feature 0048.
//!
//! opencode — `opencode-ai` on npm, a compiled Bun binary — keeps every
//! session in **one SQLite database**, `~/.local/share/opencode/opencode.db`,
//! in WAL mode. That is the fourth CLI and the first whose sessions are not
//! files at all: there is nothing to tail. Codex's index is the nearest
//! precedent, so this reads the way `codex.rs` reads `state_5.sqlite` —
//! strictly read-only, `SELECT *` mapped by column name, a missing column
//! reported rather than fatal.
//!
//! **Everything here was read from real rows** on the author's machine
//! (opencode 1.18.32, 2026-09-23), including rows captured *while in flight*
//! by polling the database during a turn: a turn not yet complete, a tool
//! mid-run, a tool blocked on a permission prompt, the `question` tool waiting
//! on a human, a subagent's child session, a hang-up mid-tool. Feature 0048
//! lists what was seen and what is still inference.
//!
//! Three tables carry what the queue needs:
//!
//! - `session` — one row per session, with cwd (`directory`), title, CLI
//!   version, **running token totals** and a `parent_id` for subagents.
//! - `message` — one row per message; `data` is JSON with `role`,
//!   `time.completed`, `finish` and `error`. **Each step of a turn is its own
//!   assistant message**: a step that ends in a tool call finishes with
//!   `"tool-calls"` and the next step is a new row.
//! - `part` — the pieces of a message: `text`, `reasoning`, `tool`, `patch`,
//!   `step-start`/`step-finish`, and a few more. A tool part carries
//!   `state.status` — `pending`, `running`, `completed`, `error`.
//!
//! And one more tells us *what changed*: `event_sequence`, one row per session
//! whose `seq` rises on every update the session receives — part updates
//! included. So a scan reads that small table and re-reads only the sessions
//! whose number moved. Nothing is re-read for a session nobody is using.
//!
//! **The distinction Qwen cannot draw, opencode can.** A tool blocked on a
//! permission prompt is `running` with **no `metadata`**; the moment it is
//! approved and actually starts, `metadata` appears — `{"output": ""}` for a
//! shell command, before it has printed anything. Seen for `bash` and `edit`
//! by approving the prompt from outside and watching the row change. The
//! `question` tool is the same shape with the question itself in its input.
//! [`derive_status`] reads both, and says which tools the rule is believed
//! for.
//!
//! What is **not** on disk is liveness. opencode keeps no per-process registry
//! — `locks/` stays empty while one runs — so a session is live when an
//! `opencode` process is running in its directory. [`scan_processes`] and
//! [`match_processes`] are that, and their limits are written beside them.
//!
//! Nothing here starts, steers or stops opencode ([ADR-0003]). Nothing here
//! writes to its database: the connection is opened `SQLITE_OPEN_READ_ONLY`,
//! which also guarantees no `-wal`/`-shm` side file is created. The same
//! database holds opencode's `account` and `credential` tables; they are never
//! queried.
//!
//! [ADR-0003]: ../../../docs/decisions/0003-observe-do-not-spawn.md

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use mogeung_core::LiveStatus;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

/// Where opencode keeps its data: `$XDG_DATA_HOME/opencode`, else
/// `~/.local/share/opencode` — on macOS too, because opencode follows XDG
/// there rather than `~/Library`.
pub fn default_home() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(x).join("opencode");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".local/share/opencode")
}

/// opencode's home **beside an injected Claude home**, for the reason
/// `AgentHomes::beside` gives: a test that builds a fake `~/.claude` must get
/// a fake opencode beside it, never the developer's real sessions.
///
/// opencode's home is not a dot-directory in `$HOME` the way the other three
/// are, so "sibling" means `<home>/.local/share/opencode`. `XDG_DATA_HOME` is
/// honoured only when the Claude home *is* the real one — an injected root
/// must not be overruled by the machine's environment.
pub fn home_beside(claude_home: &Path) -> PathBuf {
    let real = std::env::var_os("HOME").map(PathBuf::from);
    match claude_home.parent() {
        Some(parent) if Some(parent) != real.as_deref() => parent.join(".local/share/opencode"),
        _ => default_home(),
    }
}

// --- classification ----------------------------------------------------------

/// Part types this reader takes something from.
pub const HANDLED_PARTS: &[&str] = &[
    // The prompt, and the agent's words.
    "text",
    // Shown as "thinking" while it is the latest thing.
    "reasoning",
    // Tool calls: counted, named in the activity line, and the evidence for
    // an approval or a question.
    "tool",
    // The files an edit touched, by absolute path — what attributes a shared
    // worktree's diff to the session that made it, and what collision
    // detection needs to know *who* is editing a file.
    "patch",
];

/// Part types seen or defined, understood, and deliberately not read.
pub const KNOWN_IGNORED_PARTS: &[&str] = &[
    // Step boundaries. `step-finish` repeats the message's own tokens and
    // `finish`, and the session row already totals the tokens.
    "step-start",
    "step-finish",
    // A worktree snapshot hash; the git observer keeps its own base.
    "snapshot",
    // Attachments and `@agent` mentions, already reflected in the text.
    "file",
    "agent",
    // A subagent's task in the parent, and bookkeeping for compaction and
    // provider retries. **Not yet seen in a real row** — defined by the
    // writer, classified here so their first appearance is not a false alarm.
    "subtask",
    "compaction",
    "retry",
];

/// The states a tool part moves through. `pending` (input not yet parsed) →
/// `running` → `completed` or `error`. All four seen.
pub const TOOL_STATUSES: &[&str] = &["pending", "running", "completed", "error"];

/// Message roles. Both seen.
pub const ROLES: &[&str] = &["user", "assistant"];

/// Columns of `session` this reader depends on. The rest are read if present.
const EXPECTED_SESSION_COLUMNS: &[&str] = &["id", "directory", "title", "time_created", "time_updated"];

/// How is one part classified? `None` for a known part, `Some(kind)` for the
/// canary — `part/<type>` or `tool/<status>`. Shared with `--bin sweep`, which
/// must never carry its own copy of these lists.
pub fn classify_part(v: &Value) -> Option<String> {
    let Some(ty) = v.get("type").and_then(Value::as_str) else {
        return Some("part/<no type>".into());
    };
    if !HANDLED_PARTS.contains(&ty) && !KNOWN_IGNORED_PARTS.contains(&ty) {
        return Some(format!("part/{ty}"));
    }
    if ty == "tool" {
        let status = v.pointer("/state/status").and_then(Value::as_str).unwrap_or("<none>");
        if !TOOL_STATUSES.contains(&status) {
            return Some(format!("tool/{status}"));
        }
    }
    None
}

/// The same for a message: `message/<role>` when the role is unknown.
pub fn classify_message(v: &Value) -> Option<String> {
    match v.get("role").and_then(Value::as_str) {
        Some(r) if ROLES.contains(&r) => None,
        Some(r) => Some(format!("message/{r}")),
        None => Some("message/<no role>".into()),
    }
}

// --- rows, folded ---------------------------------------------------------------

/// What one `message` row says, reduced to what status and counts need.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OcMessage {
    pub role: String,
    pub created_ms: i64,
    pub completed: bool,
    /// `"stop"`, `"tool-calls"`, `"length"`… — `"tool-calls"` means another
    /// step follows in a new message.
    pub finish: Option<String>,
    /// `error.name`, and its message when there is one.
    pub error: Option<(String, String)>,
    /// This message's own tokens, in and out, on the session row's terms.
    pub tokens: (u64, u64),
}

pub fn parse_message(v: &Value) -> OcMessage {
    let error = v.get("error").filter(|e| !e.is_null()).map(|e| {
        let name = e.get("name").and_then(Value::as_str).unwrap_or("error").to_string();
        let msg = e
            .pointer("/data/message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        (name, msg)
    });
    let n = |ptr: &str| v.pointer(ptr).and_then(Value::as_u64).unwrap_or(0);
    let tokens = (
        n("/tokens/input") + n("/tokens/cache/read") + n("/tokens/cache/write"),
        n("/tokens/output") + n("/tokens/reasoning"),
    );
    OcMessage {
        tokens,
        role: v.get("role").and_then(Value::as_str).unwrap_or_default().to_string(),
        created_ms: v.pointer("/time/created").and_then(Value::as_i64).unwrap_or(0),
        completed: v.pointer("/time/completed").is_some_and(|c| !c.is_null()),
        finish: v.get("finish").and_then(Value::as_str).map(str::to_string),
        error,
    }
}

/// What one `part` row says.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OcPart {
    pub message_id: String,
    pub kind: String,
    /// Words, for a `text` part that a human or the agent wrote. Synthetic
    /// text — file contents opencode injects — is left out.
    pub text: Option<String>,
    pub tool: Option<String>,
    pub status: Option<String>,
    /// Whether `state.metadata` is present. **The approval tell**: absent
    /// while a tool waits on a prompt, present from the moment it runs.
    pub has_metadata: bool,
    pub started_ms: Option<i64>,
    /// `state.title` if the tool gave one, else a one-line summary of its
    /// input — the command, the file, the pattern.
    pub summary: Option<String>,
    /// The first question, for the `question` tool.
    pub question: Option<String>,
    /// Files this part says were written: a `patch` part's list, or the path
    /// a completed `edit`/`write` wrote.
    pub files: Vec<String>,
}

/// The first line worth reading, cut to `n` characters. A code fence is
/// skipped: a reply that opens with one said nothing in its first line, and
/// the live test showed exactly that — an activity line reading "```".
fn one_line(s: &str, n: usize) -> String {
    let line = s
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("```"))
        .unwrap_or("");
    if line.chars().count() <= n {
        line.to_string()
    } else {
        let cut: String = line.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// The most telling field of a tool's input, in the order a person would
/// want it: what it ran, what it touched, what it looked for.
fn input_summary(input: &Value) -> Option<String> {
    for key in ["command", "filePath", "path", "pattern", "url", "description", "query"] {
        if let Some(s) = input.get(key).and_then(Value::as_str) {
            if !s.trim().is_empty() {
                return Some(one_line(s, 90));
            }
        }
    }
    None
}

pub fn parse_part(message_id: &str, v: &Value) -> OcPart {
    let kind = v.get("type").and_then(Value::as_str).unwrap_or_default().to_string();
    let mut p = OcPart { message_id: message_id.to_string(), kind, ..Default::default() };
    match p.kind.as_str() {
        "patch" => {
            p.files = v
                .get("files")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default();
        }
        "text" => {
            let synthetic = v.get("synthetic").and_then(Value::as_bool).unwrap_or(false);
            if !synthetic {
                p.text = v.get("text").and_then(Value::as_str).map(str::to_string);
            }
        }
        "tool" => {
            let st = v.get("state").cloned().unwrap_or(Value::Null);
            p.tool = v.get("tool").and_then(Value::as_str).map(str::to_string);
            p.status = st.get("status").and_then(Value::as_str).map(str::to_string);
            p.has_metadata = st.get("metadata").is_some_and(|m| !m.is_null());
            p.started_ms = st.pointer("/time/start").and_then(Value::as_i64);
            let input = st.get("input").cloned().unwrap_or(Value::Null);
            p.summary = st
                .get("title")
                .and_then(Value::as_str)
                .filter(|t| !t.trim().is_empty())
                .map(|t| one_line(t, 90))
                .or_else(|| input_summary(&input));
            // A write that happened — `completed`, not a prompt still asking.
            let writes = matches!(p.tool.as_deref(), Some("edit" | "write" | "multiedit"));
            if writes && p.status.as_deref() == Some("completed") {
                if let Some(f) = input.get("filePath").and_then(Value::as_str) {
                    p.files.push(f.to_string());
                }
            }
            if p.tool.as_deref() == Some("question") {
                p.question = input
                    .pointer("/questions/0/question")
                    .and_then(Value::as_str)
                    .map(|q| one_line(q, 120));
            }
        }
        _ => {}
    }
    p
}

// --- status ---------------------------------------------------------------------

/// Tools for which "running with no metadata" means *waiting on a prompt*.
///
/// `bash` and `edit` were watched through the whole cycle — prompt shown,
/// approved from outside, row changed. `write` and `apply_patch` sit behind
/// the same `edit` permission and finish in milliseconds once allowed, so a
/// long metadata-less `running` for them can only be the prompt. Tools that
/// are merely *slow* without metadata — `webfetch`, a `read` of a huge file —
/// are deliberately **not** listed: for them the same shape means working,
/// and calling it a prompt would be a false alarm at the highest tier.
pub const APPROVAL_TOOLS: &[&str] = &["bash", "edit", "write", "apply_patch", "patch", "multiedit"];

/// How long a tool may sit `running` without metadata before it is read as
/// a prompt. Allowed tools gain metadata within a few hundred milliseconds of
/// starting; the prompt holds until a human answers.
pub const APPROVAL_GRACE_MS: i64 = 2_500;

/// Where a session's turn stands, from its rows alone.
#[derive(Debug, Clone, PartialEq)]
pub enum OcStatus {
    /// No messages yet.
    Empty,
    /// A turn is in flight: a step open, a tool running, or the next step not
    /// yet begun after a tool call.
    Working,
    /// The turn ended — stopped, interrupted, or failed. Your move.
    Finished,
    /// The `question` tool is waiting for your answer.
    Asking { question: Option<String> },
    /// A tool is blocked on a permission prompt.
    Approval { part_id: String, tool: String, summary: String, since_ms: i64 },
}

impl OcStatus {
    pub fn live_status(&self) -> LiveStatus {
        match self {
            OcStatus::Working | OcStatus::Empty => LiveStatus::Busy,
            OcStatus::Finished | OcStatus::Asking { .. } | OcStatus::Approval { .. } => LiveStatus::Idle,
        }
    }
}

/// Read the turn's state off the latest message and its parts.
///
/// **The order of the checks is the argument.** A question or a prompt is
/// checked before "is the step complete", because a step blocked on either
/// is *not* complete — it is open and waiting, and the difference between
/// that and working is the whole reason to read the tool parts at all.
pub fn derive_status(messages: &BTreeMap<String, OcMessage>, parts: &BTreeMap<String, OcPart>, now_ms: i64) -> OcStatus {
    // Ids are time-ordered (`msg_<hex time>…`), and a later step is always a
    // later id. `created_ms` breaks the rare tie a clock step could cause.
    let Some((last_id, last)) = messages.iter().max_by(|a, b| {
        a.1.created_ms.cmp(&b.1.created_ms).then_with(|| a.0.cmp(b.0))
    }) else {
        return OcStatus::Empty;
    };
    if last.role != "assistant" {
        // Your prompt is in and no reply has begun: working, from your side.
        return OcStatus::Working;
    }
    if last.error.is_some() {
        // Interrupted (`MessageAbortedError`) or failed: either way the turn
        // is over and the next move is yours.
        return OcStatus::Finished;
    }
    for (part_id, p) in parts.iter().filter(|(_, p)| p.message_id == *last_id && p.kind == "tool") {
        if p.status.as_deref() != Some("running") || p.has_metadata {
            continue;
        }
        let tool = p.tool.clone().unwrap_or_default();
        if tool == "question" {
            return OcStatus::Asking { question: p.question.clone() };
        }
        let since = p.started_ms.unwrap_or(now_ms);
        if APPROVAL_TOOLS.contains(&tool.as_str()) && now_ms - since >= APPROVAL_GRACE_MS {
            return OcStatus::Approval {
                part_id: part_id.clone(),
                tool,
                summary: p.summary.clone().unwrap_or_default(),
                since_ms: since,
            };
        }
    }
    match last.finish.as_deref() {
        // Set a moment before `time.completed` is: reading `finish` alone
        // avoids a flicker back to working at the end of every turn.
        Some(f) if f != "tool-calls" => OcStatus::Finished,
        _ => OcStatus::Working,
    }
}

// --- one session ------------------------------------------------------------------

/// One session's rows, folded and kept between scans.
#[derive(Debug, Clone, Default)]
pub struct OcThread {
    pub messages: BTreeMap<String, OcMessage>,
    pub parts: BTreeMap<String, OcPart>,
    /// Highest `time_updated` read; the next read asks for rows at or after it.
    watermark: i64,
    /// `event_sequence.seq` when last read.
    seq: Option<i64>,
    /// Unrecognised rows, by row id, so a re-read at the watermark does not
    /// count one twice.
    unknown: HashMap<String, String>,
}

impl OcThread {
    fn absorb_message(&mut self, id: String, v: &Value) {
        match classify_message(v) {
            Some(kind) => {
                self.unknown.insert(id.clone(), kind);
            }
            None => {
                self.unknown.remove(&id);
            }
        }
        self.messages.insert(id, parse_message(v));
    }

    fn absorb_part(&mut self, id: String, message_id: &str, v: &Value) {
        match classify_part(v) {
            Some(kind) => {
                self.unknown.insert(id.clone(), kind);
            }
            None => {
                self.unknown.remove(&id);
            }
        }
        self.parts.insert(id, parse_part(message_id, v));
    }

    fn parts_of<'a>(&'a self, message_id: &'a str) -> impl Iterator<Item = &'a OcPart> + 'a {
        self.parts.values().filter(move |p| p.message_id == message_id)
    }

    /// A turn is a message **you** typed: a user message with words in it.
    pub fn turns(&self) -> u32 {
        self.messages
            .iter()
            .filter(|(id, m)| m.role == "user" && self.parts_of(id).any(|p| p.text.as_deref().is_some_and(|t| !t.trim().is_empty())))
            .count() as u32
    }

    pub fn tool_calls(&self) -> u32 {
        self.parts.values().filter(|p| p.kind == "tool").count() as u32
    }

    /// Your latest prompt.
    pub fn last_prompt(&self) -> Option<String> {
        let (id, _) = self
            .messages
            .iter()
            .filter(|(_, m)| m.role == "user")
            .max_by_key(|(id, m)| (m.created_ms, (*id).clone()))?;
        self.parts_of(id).filter_map(|p| p.text.as_deref()).map(|t| one_line(t, 200)).find(|t| !t.is_empty())
    }

    /// What the agent is doing, or last did, in a few words.
    pub fn last_activity(&self, status: &OcStatus) -> Option<String> {
        match status {
            OcStatus::Asking { question } => {
                return Some(format!("asks: {}", question.clone().unwrap_or_else(|| "a question".into())));
            }
            OcStatus::Approval { tool, summary, .. } => {
                return Some(if summary.is_empty() { tool.clone() } else { format!("{tool}: {summary}") });
            }
            _ => {}
        }
        let (id, last) = self
            .messages
            .iter()
            .filter(|(_, m)| m.role == "assistant")
            .max_by_key(|(id, m)| (m.created_ms, (*id).clone()))?;
        if let Some((name, msg)) = &last.error {
            return Some(if name == "MessageAbortedError" {
                "interrupted".to_string()
            } else if msg.is_empty() {
                format!("error: {name}")
            } else {
                format!("error: {}", one_line(msg, 120))
            });
        }
        let mut parts: Vec<(&String, &OcPart)> = self.parts.iter().filter(|(_, p)| p.message_id == *id).collect();
        parts.sort_by(|a, b| a.0.cmp(b.0));
        for (_, p) in parts.iter().rev() {
            match p.kind.as_str() {
                "tool" => {
                    let tool = p.tool.clone().unwrap_or_else(|| "tool".into());
                    return Some(match &p.summary {
                        Some(s) => format!("{tool}: {s}"),
                        None => tool,
                    });
                }
                "text" => {
                    if let Some(t) = p.text.as_deref().map(|t| one_line(t, 120)).filter(|t| !t.is_empty()) {
                        return Some(t);
                    }
                }
                "reasoning" => return Some("thinking".into()),
                _ => {}
            }
        }
        None
    }

    /// Every file this session wrote, once each, with when — in the order it
    /// first wrote them. A `patch` part carries no time of its own, so it
    /// takes its message's.
    pub fn touched(&self) -> Vec<(String, i64)> {
        let mut out: Vec<(String, i64)> = Vec::new();
        for p in self.parts.values() {
            let at = p
                .started_ms
                .or_else(|| self.messages.get(&p.message_id).map(|m| m.created_ms))
                .unwrap_or(0);
            for f in &p.files {
                match out.iter_mut().find(|(path, _)| path == f) {
                    Some(seen) => seen.1 = seen.1.min(at),
                    None => out.push((f.clone(), at)),
                }
            }
        }
        out.sort_by_key(|(_, at)| *at);
        out
    }

    /// Sum of the assistant messages' own token counts — the fallback for a
    /// session row without its token columns.
    fn message_tokens(&self) -> (u64, u64) {
        self.messages
            .values()
            .fold((0, 0), |(i, o), m| (i + m.tokens.0, o + m.tokens.1))
    }
}

/// One session as the queue needs it, computed at scan time.
#[derive(Debug, Clone, PartialEq)]
pub struct OcSession {
    pub id: String,
    pub directory: String,
    pub title: Option<String>,
    pub version: Option<String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub turns: u32,
    pub tool_calls: u32,
    pub last_prompt: Option<String>,
    pub last_activity: Option<String>,
    pub status: OcStatus,
    /// Files written, and when — see [`OcThread::touched`].
    pub touched: Vec<(String, i64)>,
    /// Where this session's rows are: the database, and the session within
    /// it. **Not a file** — see [`OpenCodeInstall::locator`].
    pub locator: String,
}

/// opencode names an untitled session `New session - <ISO time>` (or
/// `Child session - …`), and renames it once a title is generated. The
/// placeholder is not a title anyone chose, so it is not shown as one.
pub fn chosen_title(title: &str) -> Option<String> {
    let t = title.trim();
    let placeholder = ["New session - ", "Child session - "].iter().any(|p| {
        t.strip_prefix(p).is_some_and(|rest| DateTime::parse_from_rfc3339(rest).is_ok())
    });
    (!t.is_empty() && !placeholder).then(|| t.to_string())
}

fn ms_to_utc(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(Utc::now)
}

// --- the database -------------------------------------------------------------

pub struct OpenCodeInstall {
    pub home: PathBuf,
    pub db: PathBuf,
}

impl OpenCodeInstall {
    /// Present when the database exists. A home with no database is an
    /// opencode that has never been run, which has nothing to show.
    pub fn discover(home: &Path) -> Option<OpenCodeInstall> {
        let db = home.join("opencode.db");
        db.is_file().then(|| OpenCodeInstall { home: home.to_path_buf(), db })
    }

    /// What goes in `Session.transcript_path`: `<database>#<session id>`.
    ///
    /// Deliberately **not** a path to a file. Everything that reads that field
    /// streams it as a Claude Code JSONL transcript — the *why* and decision
    /// panels — and pointed at the database they would read megabytes of
    /// binary as lines, find nothing, and do it again on the next ask. A
    /// locator that opens nothing gives them the empty answer directly. It
    /// must not be empty either: an empty one tells the diff that the session
    /// has never been read, and it then computes no diff at all — which is
    /// how every opencode session first read *ended with no changes*.
    pub fn locator(&self, session_id: &str) -> String {
        format!("{}#{session_id}", self.db.display())
    }
}

/// A `session` row, mapped by column name.
#[derive(Debug, Clone)]
struct SessionRow {
    id: String,
    parent_id: Option<String>,
    directory: String,
    title: String,
    version: Option<String>,
    created_ms: i64,
    updated_ms: i64,
    archived: bool,
    tokens: Option<(u64, u64)>,
}

/// Incremental state across scans.
#[derive(Default)]
pub struct ScanCache {
    threads: HashMap<String, OcThread>,
}

/// What one scan read.
pub struct ScanResult {
    pub sessions: Vec<OcSession>,
    /// Unrecognised shapes, merged across sessions — the canary.
    pub unknown: Vec<(String, u64)>,
    /// The database could not be read as expected; why.
    pub error: Option<String>,
}

fn open_read_only(db: &Path) -> rusqlite::Result<Connection> {
    // Strictly read-only, as for Codex: opencode owns this database and
    // holds a live WAL on it. `SQLITE_OPEN_READ_ONLY` never creates or
    // modifies a side file; a read that fails, fails, and is reported.
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    // A writer mid-checkpoint can hold the file for a moment; wait briefly
    // rather than report a healthy database as unreadable.
    conn.busy_timeout(std::time::Duration::from_millis(250))?;
    Ok(conn)
}

fn read_sessions(conn: &Connection, cutoff_ms: i64) -> rusqlite::Result<(Vec<SessionRow>, Vec<String>)> {
    // `SELECT *` on purpose, as in `codex.rs`: naming columns would fail on a
    // database at a different migration level. Take what is there by name.
    let mut stmt = conn.prepare("SELECT * FROM session WHERE time_updated >= ?1")?;
    let cols: HashMap<String, usize> =
        stmt.column_names().iter().enumerate().map(|(i, n)| (n.to_string(), i)).collect();
    let missing: Vec<String> = EXPECTED_SESSION_COLUMNS
        .iter()
        .filter(|c| !cols.contains_key(**c))
        .map(|c| c.to_string())
        .collect();
    if !missing.is_empty() {
        return Ok((Vec::new(), missing));
    }
    let col = |name: &str| cols.get(name).copied();
    let rows = stmt.query_map([cutoff_ms], |r| {
        let text = |name: &str| -> Option<String> { col(name).and_then(|i| r.get::<_, Option<String>>(i).ok().flatten()) };
        let int = |name: &str| -> Option<i64> { col(name).and_then(|i| r.get::<_, Option<i64>>(i).ok().flatten()) };
        let tokens = match (int("tokens_input"), int("tokens_output")) {
            (Some(i), Some(o)) => {
                let cache = int("tokens_cache_read").unwrap_or(0) + int("tokens_cache_write").unwrap_or(0);
                let reasoning = int("tokens_reasoning").unwrap_or(0);
                // In is everything the model was sent, cached share included —
                // the convention every other source here reports.
                Some(((i + cache).max(0) as u64, (o + reasoning).max(0) as u64))
            }
            _ => None,
        };
        Ok(SessionRow {
            id: text("id").unwrap_or_default(),
            parent_id: text("parent_id").filter(|p| !p.is_empty()),
            directory: text("directory").unwrap_or_default(),
            title: text("title").unwrap_or_default(),
            version: text("version"),
            created_ms: int("time_created").unwrap_or(0),
            updated_ms: int("time_updated").unwrap_or(0),
            archived: int("time_archived").is_some(),
            tokens,
        })
    })?;
    Ok((rows.filter_map(Result::ok).filter(|s| !s.id.is_empty()).collect(), Vec::new()))
}

/// `event_sequence` as `session id → seq`, or `None` where the table is
/// missing — an older or newer layout — in which case every recent session
/// is simply re-read.
fn read_seqs(conn: &Connection) -> Option<HashMap<String, i64>> {
    let mut stmt = conn.prepare("SELECT aggregate_id, seq FROM event_sequence").ok()?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .ok()?;
    Some(rows.filter_map(Result::ok).collect())
}

fn read_thread_rows(conn: &Connection, id: &str, thread: &mut OcThread) -> rusqlite::Result<()> {
    let since = thread.watermark;
    let mut high = since;
    {
        let mut stmt = conn.prepare("SELECT id, time_updated, data FROM message WHERE session_id = ?1 AND time_updated >= ?2")?;
        let rows = stmt.query_map(rusqlite::params![id, since], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
        })?;
        for (mid, tu, data) in rows.filter_map(Result::ok) {
            high = high.max(tu);
            // A row that is not JSON is skipped, never fatal: the database
            // is undocumented and a malformed row must degrade one message,
            // not the session.
            if let Ok(v) = serde_json::from_str::<Value>(&data) {
                thread.absorb_message(mid, &v);
            }
        }
    }
    {
        let mut stmt =
            conn.prepare("SELECT id, message_id, time_updated, data FROM part WHERE session_id = ?1 AND time_updated >= ?2")?;
        let rows = stmt.query_map(rusqlite::params![id, since], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
        })?;
        for (pid, mid, tu, data) in rows.filter_map(Result::ok) {
            high = high.max(tu);
            if let Ok(v) = serde_json::from_str::<Value>(&data) {
                thread.absorb_part(pid, &mid, &v);
            }
        }
    }
    // An undo (`/undo`, a revert) deletes rows, and a watermark only ever
    // sees rows arrive. So whenever a session is re-read, the ids that still
    // exist are asked for too, and anything else is forgotten.
    let alive: HashSet<String> = {
        let mut stmt = conn.prepare("SELECT id FROM message WHERE session_id = ?1")?;
        let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
        rows.filter_map(Result::ok).collect()
    };
    thread.messages.retain(|k, _| alive.contains(k));
    thread.parts.retain(|_, p| alive.contains(&p.message_id));
    let (messages, parts) = (&thread.messages, &thread.parts);
    thread.unknown.retain(|k, _| messages.contains_key(k) || parts.contains_key(k));
    thread.watermark = high;
    Ok(())
}

impl ScanCache {
    /// Bring every recent top-level session up to date, and summarise them.
    ///
    /// Only sessions whose `event_sequence` moved are re-read, and then only
    /// the rows updated since the last read. Subagent sessions (`parent_id`
    /// set) are not sessions of yours: their work shows in the parent as a
    /// running `task` tool, which reads as working. Archived ones are gone.
    pub fn refresh(&mut self, install: &OpenCodeInstall, max_age_days: i64, now_ms: i64) -> ScanResult {
        let fail = |error: String| ScanResult { sessions: Vec::new(), unknown: Vec::new(), error: Some(error) };
        let conn = match open_read_only(&install.db) {
            Ok(c) => c,
            Err(e) => return fail(format!("opencode.db could not be opened: {e}")),
        };
        let cutoff = now_ms - max_age_days * 86_400_000;
        let (rows, missing) = match read_sessions(&conn, cutoff) {
            Ok(r) => r,
            Err(e) => return fail(format!("opencode.db's session table could not be read: {e}")),
        };
        if !missing.is_empty() {
            return fail(format!("opencode.db's session table has no {} — its layout has changed", missing.join(", ")));
        }
        let seqs = read_seqs(&conn);

        let mine: Vec<SessionRow> = rows.into_iter().filter(|r| r.parent_id.is_none() && !r.archived).collect();
        let keep: HashSet<String> = mine.iter().map(|r| r.id.clone()).collect();
        self.threads.retain(|id, _| keep.contains(id));

        let mut error = None;
        let mut sessions = Vec::with_capacity(mine.len());
        for row in &mine {
            let thread = self.threads.entry(row.id.clone()).or_default();
            let seq = seqs.as_ref().and_then(|s| s.get(&row.id).copied());
            // Without the sequence table, a session is re-read whenever it is
            // recent enough that it could be moving.
            let changed = match (&seqs, seq) {
                (Some(_), Some(n)) => thread.seq != Some(n),
                _ => thread.seq.is_none() || now_ms - row.updated_ms < 10 * 60_000,
            };
            if changed {
                match read_thread_rows(&conn, &row.id, thread) {
                    Ok(()) => thread.seq = seq.or(Some(row.updated_ms)),
                    Err(e) => error = Some(format!("opencode.db's messages could not be read: {e}")),
                }
            }
            let status = derive_status(&thread.messages, &thread.parts, now_ms);
            let (tokens_in, tokens_out) = row.tokens.unwrap_or_else(|| thread.message_tokens());
            let last_part = thread.parts.values().filter_map(|p| p.started_ms).max().unwrap_or(0);
            let last_message = thread.messages.values().map(|m| m.created_ms).max().unwrap_or(0);
            let updated_ms = row.updated_ms.max(last_part).max(last_message);
            sessions.push(OcSession {
                id: row.id.clone(),
                directory: row.directory.clone(),
                title: chosen_title(&row.title),
                version: row.version.clone(),
                created: ms_to_utc(row.created_ms),
                updated: ms_to_utc(updated_ms),
                created_ms: row.created_ms,
                updated_ms,
                tokens_in,
                tokens_out,
                turns: thread.turns(),
                tool_calls: thread.tool_calls(),
                last_prompt: thread.last_prompt(),
                last_activity: thread.last_activity(&status),
                status,
                touched: thread.touched(),
                locator: install.locator(&row.id),
            });
        }

        let mut unknown: BTreeMap<String, u64> = BTreeMap::new();
        for t in self.threads.values() {
            for kind in t.unknown.values() {
                *unknown.entry(kind.clone()).or_default() += 1;
            }
        }
        ScanResult { sessions, unknown: unknown.into_iter().collect(), error }
    }
}

// --- liveness -------------------------------------------------------------------

/// A running opencode process, and where it is working.
#[derive(Debug, Clone, PartialEq)]
pub struct OcProcess {
    pub pid: u32,
    pub started_ms: Option<i64>,
    /// The directory it opened: a path argument, else `$PWD`, else its cwd.
    pub dir: Option<PathBuf>,
    /// A session named on its command line with `-s`/`--session`.
    pub session: Option<String>,
}

/// Is this program opencode? The file stem, so `opencode.exe` — what the npm
/// package actually runs — counts.
pub fn is_opencode(program: &str) -> bool {
    Path::new(program.trim())
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("opencode"))
}

/// Subcommands that never hold a session of yours open. An `opencode models`
/// or `opencode stats` in a directory must not make a session there live.
const SESSIONLESS: &[&str] = &[
    "completion", "mcp", "debug", "providers", "auth", "agent", "upgrade", "uninstall", "models", "stats",
    "export", "import", "session", "plugin", "plug", "db",
];

/// Flags of the TUI that take a value — skipped with it when looking for the
/// positional project path.
const VALUE_FLAGS: &[&str] = &[
    "-m", "--model", "-s", "--session", "--prompt", "--agent", "--port", "--hostname", "--log-level",
    "--mdns-domain", "--cors", "--replay-limit",
];

/// Subcommands that keep a session: their positional arguments are messages
/// or URLs, not a project directory.
const SESSION_COMMANDS: &[&str] = &["run", "serve", "web", "acp", "attach", "github", "pr"];

/// What an opencode command line says: `None` when it is a sessionless
/// subcommand, else `(session, project)`.
pub fn read_args(args: &[String]) -> Option<(Option<String>, Option<String>)> {
    let mut session = None;
    let mut project = None;
    let mut positional_seen = false;
    let mut i = 1;
    while i < args.len() {
        let a = args[i].as_str();
        if let Some(v) = a.strip_prefix("--session=") {
            session = Some(v.to_string());
        } else if a == "-s" || a == "--session" {
            session = args.get(i + 1).cloned();
            i += 1;
        } else if VALUE_FLAGS.contains(&a) {
            i += 1;
        } else if a.starts_with('-') {
            // A boolean flag, or `--flag=value`.
        } else if !positional_seen {
            positional_seen = true;
            if SESSIONLESS.contains(&a) {
                return None;
            }
            if !SESSION_COMMANDS.contains(&a) {
                project = Some(a.to_string());
            }
        }
        i += 1;
    }
    Some((session, project))
}

/// Resolve where a process is working, the way opencode itself decides it:
/// a project argument if it names a directory, else `$PWD` — opencode uses
/// `$PWD` over the real cwd, which was seen when the two differed — else the
/// cwd the kernel reports.
fn working_dir(project: Option<&str>, pwd: Option<&str>, cwd: Option<&Path>) -> Option<PathBuf> {
    let base = pwd.map(PathBuf::from).or_else(|| cwd.map(Path::to_path_buf));
    if let Some(p) = project {
        let path = match &base {
            Some(b) if Path::new(p).is_relative() => b.join(p),
            _ => PathBuf::from(p),
        };
        if path.is_dir() {
            return Some(path);
        }
    }
    base
}

/// Every opencode process on this machine that could hold a session open.
///
/// Children of another opencode process are dropped: one running opencode is
/// one liveness claim, whatever it spawns.
pub fn scan_processes() -> Vec<OcProcess> {
    let found = platform::opencode_processes();
    let pids: HashSet<u32> = found.iter().map(|(p, _)| p.pid).collect();
    found
        .into_iter()
        .filter(|(_, ppid)| !pids.contains(ppid))
        .map(|(p, _)| p)
        .collect()
}

#[cfg(target_os = "linux")]
mod platform {
    //! `/proc`, read directly: no fork per scan, which matters because this
    //! runs on every tick whether or not opencode is running.
    use super::*;

    extern "C" {
        fn sysconf(name: i32) -> i64;
    }
    /// `_SC_CLK_TCK` on Linux, glibc and musl alike.
    const SC_CLK_TCK: i32 = 2;

    fn boot_ms() -> Option<i64> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let btime: i64 = stat.lines().find_map(|l| l.strip_prefix("btime "))?.trim().parse().ok()?;
        Some(btime * 1000)
    }

    fn clock_ticks() -> i64 {
        // SAFETY: `sysconf` is async-signal-safe, takes no pointers, and
        // `_SC_CLK_TCK` is defined on every Linux libc.
        match unsafe { sysconf(SC_CLK_TCK) } {
            n if n > 0 => n,
            _ => 100,
        }
    }

    /// When `pid` started, from field 22 of its `stat` and the boot time.
    pub(super) fn started_ms(pid: u32) -> Option<i64> {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let fields: Vec<&str> = stat[stat.rfind(')')? + 1..].split_whitespace().collect();
        let ticks: i64 = fields.get(19)?.parse().ok()?;
        Some(boot_ms()? + ticks * 1000 / clock_ticks())
    }

    pub(super) fn opencode_processes() -> Vec<(OcProcess, u32)> {
        let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
        let boot = boot_ms();
        let tck = clock_ticks();
        let mut out = Vec::new();
        for entry in dir.flatten() {
            let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
            let base = entry.path();
            let Ok(comm) = std::fs::read_to_string(base.join("comm")) else { continue };
            if !is_opencode(comm.trim()) {
                continue;
            }
            let Ok(stat) = std::fs::read_to_string(base.join("stat")) else { continue };
            // After the last `)`: the name can contain one.
            let Some(close) = stat.rfind(')') else { continue };
            let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
            if fields.first() == Some(&"Z") {
                // A zombie holds its pid and nothing else.
                continue;
            }
            let ppid: u32 = fields.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            let started_ms = fields
                .get(19)
                .and_then(|s| s.parse::<i64>().ok())
                .zip(boot)
                .map(|(ticks, boot)| boot + ticks * 1000 / tck);
            let args: Vec<String> = std::fs::read(base.join("cmdline"))
                .map(|b| b.split(|c| *c == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect())
                .unwrap_or_default();
            let Some((session, project)) = read_args(&args) else { continue };
            let pwd = std::fs::read(base.join("environ")).ok().and_then(|env| {
                env.split(|c| *c == 0)
                    .find_map(|kv| kv.strip_prefix(b"PWD="))
                    .map(|v| String::from_utf8_lossy(v).into_owned())
            });
            let cwd = std::fs::read_link(base.join("cwd")).ok();
            out.push((
                OcProcess { pid, started_ms, dir: working_dir(project.as_deref(), pwd.as_deref(), cwd.as_deref()), session },
                ppid,
            ));
        }
        out
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    //! macOS and the rest: one `ps` for the table and, only when opencode is
    //! running, one `lsof` for every opencode process's cwd. `$PWD` is not
    //! readable here, so a process whose `$PWD` and cwd differ is matched by
    //! its cwd. **Not exercised on a real Mac yet** — feature 0048 says so.
    use super::*;

    /// When `pid` started, from `ps -o etime=`.
    pub(super) fn started_ms(pid: u32) -> Option<i64> {
        let out = std::process::Command::new("ps").args(["-o", "etime=", "-p", &pid.to_string()]).output().ok()?;
        let secs = parse_etime(String::from_utf8_lossy(&out.stdout).trim())?;
        Some(Utc::now().timestamp_millis() - secs * 1000)
    }

    fn cwds(pids: &[u32]) -> HashMap<u32, PathBuf> {
        if pids.is_empty() {
            return HashMap::new();
        }
        let list = pids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let Ok(out) = std::process::Command::new("lsof").args(["-a", "-d", "cwd", "-Fn", "-p", &list]).output() else {
            return HashMap::new();
        };
        parse_lsof_cwds(&String::from_utf8_lossy(&out.stdout))
    }

    pub(super) fn opencode_processes() -> Vec<(OcProcess, u32)> {
        let Ok(out) = std::process::Command::new("ps").args(["-axo", "pid=,ppid=,etime=,command="]).output() else {
            return Vec::new();
        };
        let now = Utc::now().timestamp_millis();
        let mut rows = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut it = line.split_whitespace();
            let (Some(pid), Some(ppid), Some(etime)) = (it.next(), it.next(), it.next()) else { continue };
            let args: Vec<String> = it.map(str::to_string).collect();
            if !args.first().is_some_and(|a| is_opencode(a)) {
                continue;
            }
            let (Ok(pid), Ok(ppid)) = (pid.parse::<u32>(), ppid.parse::<u32>()) else { continue };
            let Some((session, project)) = read_args(&args) else { continue };
            rows.push((pid, ppid, parse_etime(etime).map(|s| now - s * 1000), session, project));
        }
        let cwd = cwds(&rows.iter().map(|r| r.0).collect::<Vec<_>>());
        rows.into_iter()
            .map(|(pid, ppid, started_ms, session, project)| {
                let dir = working_dir(project.as_deref(), None, cwd.get(&pid).map(PathBuf::as_path));
                (OcProcess { pid, started_ms, dir, session }, ppid)
            })
            .collect()
    }
}

/// When a process started, where the platform will say. For the *waiting
/// since* clock of an opencode that has not been spoken to yet, whose row is
/// rebuilt every scan and so cannot remember when it was first seen.
pub fn process_started(pid: u32) -> Option<DateTime<Utc>> {
    platform::started_ms(pid).and_then(|ms| Utc.timestamp_millis_opt(ms).single())
}

/// `[[dd-]hh:]mm:ss` — what BSD `ps` prints for `etime` — as seconds. At top
/// level rather than in the macOS module so it is tested on every platform.
pub fn parse_etime(s: &str) -> Option<i64> {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<i64>().ok()?, r),
        None => (0, s),
    };
    let nums: Vec<i64> = rest.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let secs = match nums.as_slice() {
        [m, s] => m * 60 + s,
        [h, m, s] => h * 3600 + m * 60 + s,
        _ => return None,
    };
    Some(days * 86_400 + secs)
}

/// `lsof -Fn` output — `p<pid>` then `n<path>` — as `pid → cwd`.
pub fn parse_lsof_cwds(out: &str) -> HashMap<u32, PathBuf> {
    let mut map = HashMap::new();
    let mut current = None;
    for line in out.lines() {
        if let Some(p) = line.strip_prefix('p') {
            current = p.parse::<u32>().ok();
        } else if let (Some(n), Some(pid)) = (line.strip_prefix('n'), current) {
            map.insert(pid, PathBuf::from(n));
        }
    }
    map
}

/// How much earlier than a process's start a session may have last moved and
/// still be taken for that process's: the TUI loads a resumed session a
/// moment after it starts, and clocks are read at different instants.
const START_SLACK_MS: i64 = 5_000;

/// Which session each running opencode process is holding open.
///
/// opencode records no pid against a session, so this is a match, and its
/// rule is: **a session named on the command line is that process's**; else
/// a process takes the most recently active session **in its directory that
/// has moved since the process started** — so yesterday's session in the same
/// directory is not claimed by a TUI you opened this morning and have not
/// typed into. The newest process picks first.
///
/// The limit, stated rather than hidden: two opencode processes in **one**
/// directory, both active, cannot be told apart from disk, and may be paired
/// crosswise. Liveness is right either way; which pane hosts which session
/// may not be.
pub fn match_processes(procs: &[OcProcess], sessions: &[OcSession]) -> HashMap<String, u32> {
    let mut out: HashMap<String, u32> = HashMap::new();
    let mut taken: HashSet<u32> = HashSet::new();
    for p in procs {
        if let Some(id) = &p.session {
            if sessions.iter().any(|s| &s.id == id) && !out.contains_key(id) {
                out.insert(id.clone(), p.pid);
                taken.insert(p.pid);
            }
        }
    }

    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let session_dirs: HashMap<&str, PathBuf> =
        sessions.iter().map(|s| (s.id.as_str(), canon(Path::new(&s.directory)))).collect();

    let mut rest: Vec<&OcProcess> = procs.iter().filter(|p| !taken.contains(&p.pid)).collect();
    rest.sort_by_key(|p| std::cmp::Reverse(p.started_ms.unwrap_or(0)));
    for p in rest {
        let Some(dir) = p.dir.as_deref().map(canon) else { continue };
        let best = sessions
            .iter()
            .filter(|s| !out.contains_key(&s.id))
            .filter(|s| session_dirs.get(s.id.as_str()) == Some(&dir))
            .filter(|s| p.started_ms.is_none_or(|t| s.updated_ms >= t - START_SLACK_MS))
            .max_by_key(|s| s.updated_ms);
        if let Some(s) = best {
            out.insert(s.id.clone(), p.pid);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Rows copied from the author's `opencode.db` (1.18.32), with the scratch
    /// paths shortened and the times of rows from different sessions put in
    /// one order. Each is the shape at one moment of a real turn.
    mod rows {
        use serde_json::{json, Value};
        pub fn user() -> Value {
            json!({"role":"user","time":{"created":1790170000000_i64},"agent":"build",
                   "model":{"providerID":"spark","modelID":"local-inference-lab/Qwen3.8-Flash-Next-NVFP4"},"summary":{"diffs":[]}})
        }
        pub fn assistant_open() -> Value {
            json!({"parentID":"msg_a","role":"assistant","mode":"build","agent":"build","path":{"cwd":"/home/me/demo","root":"/home/me/demo"},
                   "cost":0,"tokens":{"total":0,"input":0,"output":0,"reasoning":0,"cache":{"write":0,"read":0}},
                   "modelID":"local-inference-lab/Qwen3.8-Flash-Next-NVFP4","providerID":"spark","time":{"created":1790177153105_i64}})
        }
        pub fn assistant_step_done() -> Value {
            json!({"parentID":"msg_a","role":"assistant","mode":"build","agent":"build","path":{"cwd":"/home/me/demo","root":"/home/me/demo"},
                   "cost":0,"tokens":{"total":11274,"input":11245,"output":7,"reasoning":22,"cache":{"write":0,"read":0}},
                   "modelID":"m","providerID":"spark","time":{"created":1790177153105_i64,"completed":1790177161931_i64},"finish":"tool-calls"})
        }
        pub fn assistant_stop() -> Value {
            json!({"parentID":"msg_a","role":"assistant","mode":"build","agent":"build","path":{"cwd":"/home/me/demo","root":"/home/me/demo"},
                   "cost":0,"tokens":{"total":11617,"input":11581,"output":12,"reasoning":24,"cache":{"write":0,"read":0}},
                   "modelID":"m","providerID":"spark","time":{"created":1790177162000_i64,"completed":1790177165000_i64},"finish":"stop"})
        }
        pub fn assistant_aborted() -> Value {
            json!({"parentID":"msg_a","role":"assistant","mode":"build","agent":"build","path":{"cwd":"/home/me/demo","root":"/home/me/demo"},
                   "cost":0,"tokens":{"total":0,"input":0,"output":0,"reasoning":0,"cache":{"write":0,"read":0}},"modelID":"m","providerID":"spark",
                   "time":{"created":1790177309200_i64,"completed":1790177371072_i64},"error":{"name":"MessageAbortedError","data":{"message":"Aborted"}}})
        }
        pub fn text(t: &str) -> Value {
            json!({"type":"text","text":t})
        }
        /// `bash` blocked on the permission prompt: running, input and a start
        /// time, **no metadata**. Held like this until approved.
        pub fn bash_waiting() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-9c1","tool":"bash",
                   "state":{"status":"running","input":{"command":"echo permission-test"},"time":{"start":1790177319034_i64}}})
        }
        /// The same call a moment after approval: metadata before any output.
        pub fn bash_running() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-9c1","tool":"bash",
                   "state":{"status":"running","input":{"command":"sleep 6; echo slept"},"metadata":{"output":""},"time":{"start":1790177439934_i64}}})
        }
        pub fn bash_done() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-9c1","tool":"bash",
                   "state":{"status":"completed","input":{"command":"git diff --stat"},"output":"README.md | 1 +\n",
                            "title":"git diff --stat","metadata":{"output":"README.md | 1 +\n","exit":0,"truncated":false},
                            "time":{"start":1790177254164_i64,"end":1790177254566_i64}}})
        }
        pub fn bash_interrupted() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-9c1","tool":"bash",
                   "state":{"status":"error","input":{"command":"echo permission-test"},"time":{"start":1790177319034_i64,"end":1790177371072_i64},
                            "error":"Tool execution aborted","metadata":{"interrupted":true}}})
        }
        pub fn edit_waiting() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-e2","tool":"edit",
                   "state":{"status":"running","input":{"filePath":"/home/me/demo/a.txt","oldString":"x","newString":"y"},"time":{"start":1790177489000_i64}}})
        }
        pub fn question_waiting() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-q1","tool":"question",
                   "state":{"status":"running","input":{"questions":[{"question":"Which colour do you prefer?","header":"Colour preference",
                            "options":[{"label":"Red"},{"label":"Blue"}]}]},"time":{"start":1790177541000_i64}}})
        }
        pub fn task_running() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-t1","tool":"task",
                   "state":{"status":"running","input":{"description":"List files in directory","prompt":"List the files","subagent_type":"general"},
                            "title":"List files in directory","metadata":{"model":{},"parentSessionId":"ses_p","sessionId":"ses_c"},
                            "time":{"start":1790177211900_i64}}})
        }
        pub fn pending() -> Value {
            json!({"type":"tool","callID":"chatcmpl-tool-p","tool":"write","state":{"status":"pending","input":{},"raw":""}})
        }
    }

    fn thread(msgs: &[(&str, Value)], parts: &[(&str, &str, Value)]) -> OcThread {
        let mut t = OcThread::default();
        for (id, v) in msgs {
            t.absorb_message(id.to_string(), v);
        }
        for (id, mid, v) in parts {
            t.absorb_part(id.to_string(), mid, v);
        }
        t
    }

    fn status_of(t: &OcThread, now: i64) -> OcStatus {
        derive_status(&t.messages, &t.parts, now)
    }

    #[test]
    fn a_turn_that_stopped_is_finished_and_one_between_steps_is_working() {
        let done = thread(&[("msg_a", rows::user()), ("msg_b", rows::assistant_stop())], &[("prt_1", "msg_a", rows::text("Testing"))]);
        assert_eq!(status_of(&done, 1790177170000), OcStatus::Finished);
        assert_eq!(status_of(&done, 0).live_status(), LiveStatus::Idle);

        // The step ended in a tool call and the next step has not begun.
        let between = thread(&[("msg_a", rows::user()), ("msg_b", rows::assistant_step_done())], &[]);
        assert_eq!(status_of(&between, 1790177162000), OcStatus::Working);

        let open = thread(&[("msg_a", rows::user()), ("msg_b", rows::assistant_open())], &[]);
        assert_eq!(status_of(&open, 1790177160000), OcStatus::Working);

        // Your prompt is in and nothing has answered yet.
        let asked = thread(&[("msg_a", rows::user())], &[]);
        assert_eq!(status_of(&asked, 1790177160000), OcStatus::Working);
        assert_eq!(status_of(&OcThread::default(), 0), OcStatus::Empty);
    }

    #[test]
    fn a_tool_blocked_on_a_prompt_reads_as_approval_once_the_grace_has_passed() {
        let t = thread(
            &[("msg_a", rows::user()), ("msg_b", rows::assistant_open())],
            &[("prt_1", "msg_b", rows::bash_waiting())],
        );
        let start = 1790177319034;
        assert_eq!(status_of(&t, start + 1_000), OcStatus::Working, "inside the grace it could still be starting");
        match status_of(&t, start + APPROVAL_GRACE_MS) {
            OcStatus::Approval { tool, summary, part_id, .. } => {
                assert_eq!(tool, "bash");
                assert_eq!(summary, "echo permission-test");
                assert_eq!(part_id, "prt_1");
            }
            other => panic!("expected an approval, got {other:?}"),
        }
        let edit = thread(&[("msg_a", rows::user()), ("msg_b", rows::assistant_open())], &[("prt_1", "msg_b", rows::edit_waiting())]);
        assert!(matches!(status_of(&edit, 1790177489000 + 10_000), OcStatus::Approval { ref summary, .. } if summary == "/home/me/demo/a.txt"));
    }

    #[test]
    fn a_tool_that_is_actually_running_is_working_however_long_it_takes() {
        // Metadata appears the moment an approved call starts — before output.
        let t = thread(
            &[("msg_a", rows::user()), ("msg_b", rows::assistant_open())],
            &[("prt_1", "msg_b", rows::bash_running())],
        );
        assert_eq!(status_of(&t, 1790177439934 + 600_000), OcStatus::Working);
        // A subagent at work is a running `task` with metadata: working.
        let task = thread(&[("msg_a", rows::user()), ("msg_b", rows::assistant_open())], &[("prt_1", "msg_b", rows::task_running())]);
        assert_eq!(status_of(&task, 1790177211900 + 600_000), OcStatus::Working);
    }

    #[test]
    fn a_slow_tool_without_metadata_is_not_mistaken_for_a_prompt() {
        let slow = json!({"type":"tool","tool":"webfetch","state":{"status":"running","input":{"url":"https://example.com"},"time":{"start":1000}}});
        let t = thread(&[("msg_a", rows::user()), ("msg_b", rows::assistant_open())], &[("prt_1", "msg_b", slow)]);
        assert_eq!(status_of(&t, 1000 + 60_000), OcStatus::Working);
    }

    #[test]
    fn the_question_tool_is_the_agent_waiting_on_you_with_its_question() {
        let t = thread(
            &[("msg_a", rows::user()), ("msg_b", rows::assistant_open())],
            &[("prt_1", "msg_b", rows::question_waiting())],
        );
        let st = status_of(&t, 1790177541000);
        assert_eq!(st, OcStatus::Asking { question: Some("Which colour do you prefer?".into()) });
        assert_eq!(st.live_status(), LiveStatus::Idle);
        assert_eq!(t.last_activity(&st).as_deref(), Some("asks: Which colour do you prefer?"));
    }

    #[test]
    fn an_interrupted_turn_is_over_not_running() {
        let t = thread(
            &[("msg_a", rows::user()), ("msg_b", rows::assistant_aborted())],
            &[("prt_1", "msg_b", rows::bash_interrupted())],
        );
        let st = status_of(&t, 1790177400000);
        assert_eq!(st, OcStatus::Finished);
        assert_eq!(t.last_activity(&st).as_deref(), Some("interrupted"));
    }

    #[test]
    fn counts_prompt_and_activity_come_from_the_rows() {
        let t = thread(
            &[("msg_a", rows::user()), ("msg_b", rows::assistant_step_done()), ("msg_c", rows::assistant_stop())],
            &[
                ("prt_1", "msg_a", rows::text("Read README.md and run git diff --stat")),
                ("prt_2", "msg_b", rows::bash_done()),
                ("prt_3", "msg_c", rows::text("\n\nOne file changed.")),
            ],
        );
        assert_eq!(t.turns(), 1);
        assert_eq!(t.tool_calls(), 1);
        assert_eq!(t.last_prompt().as_deref(), Some("Read README.md and run git diff --stat"));
        let st = status_of(&t, 1790180000000);
        assert_eq!(t.last_activity(&st).as_deref(), Some("One file changed."));
        // A reply that opens with a code fence is read from its first real line.
        let fenced = thread(&[("msg_c", rows::assistant_stop())], &[("prt_3", "msg_c", rows::text("```\n M a.txt\n```"))]);
        assert_eq!(fenced.last_activity(&OcStatus::Finished).as_deref(), Some("M a.txt"));
        // A tool's own title wins over its input.
        let only_tool = thread(&[("msg_b", rows::assistant_step_done())], &[("prt_2", "msg_b", rows::bash_done())]);
        assert_eq!(only_tool.last_activity(&OcStatus::Working).as_deref(), Some("bash: git diff --stat"));
    }

    #[test]
    fn synthetic_text_is_not_a_turn_or_a_prompt() {
        let injected = json!({"type":"text","text":"<file contents>","synthetic":true});
        let t = thread(&[("msg_a", rows::user())], &[("prt_1", "msg_a", injected)]);
        assert_eq!(t.turns(), 0);
        assert_eq!(t.last_prompt(), None);
    }

    #[test]
    fn every_seen_part_and_state_is_classified_and_a_new_one_is_loud() {
        for v in [rows::text("x"), rows::bash_waiting(), rows::bash_done(), rows::bash_interrupted(), rows::pending(), rows::question_waiting(),
                  json!({"type":"step-start"}), json!({"type":"step-finish","reason":"stop"}), json!({"type":"reasoning","text":"…"}),
                  json!({"type":"patch","hash":"5dad","files":["/a"]})] {
            assert_eq!(classify_part(&v), None, "{v}");
        }
        assert_eq!(classify_part(&json!({"type":"telepathy"})), Some("part/telepathy".into()));
        assert_eq!(classify_part(&json!({"type":"tool","tool":"bash","state":{"status":"daydreaming"}})), Some("tool/daydreaming".into()));
        assert_eq!(classify_message(&rows::user()), None);
        assert_eq!(classify_message(&json!({"role":"system"})), Some("message/system".into()));
        // Each list names a kind once: a kind in both would hide a decision.
        for k in HANDLED_PARTS {
            assert!(!KNOWN_IGNORED_PARTS.contains(k), "{k} is both handled and ignored");
        }
    }

    #[test]
    fn a_re_read_row_is_counted_once_by_the_canary() {
        let mut t = OcThread::default();
        let odd = json!({"type":"telepathy"});
        t.absorb_part("prt_1".into(), "msg_a", &odd);
        t.absorb_part("prt_1".into(), "msg_a", &odd);
        assert_eq!(t.unknown.len(), 1);
        // And a row that becomes known stops counting.
        t.absorb_part("prt_1".into(), "msg_a", &rows::text("fine"));
        assert!(t.unknown.is_empty());
    }

    #[test]
    fn the_files_a_session_wrote_come_from_its_patches_and_its_writes() {
        let patch = json!({"type":"patch","hash":"5dad","files":["/home/me/demo/README.md"]});
        let edit_done = json!({"type":"tool","tool":"edit","state":{"status":"completed","input":{"filePath":"/home/me/demo/a.txt"},
                               "title":"a.txt","metadata":{"diff":""},"time":{"start":1790177489500_i64,"end":1790177489800_i64}}});
        let t = thread(
            &[("msg_a", rows::user()), ("msg_b", rows::assistant_step_done())],
            &[
                ("prt_1", "msg_b", patch),
                ("prt_2", "msg_b", edit_done),
                // Asked for and not yet allowed: nothing was written.
                ("prt_3", "msg_b", rows::edit_waiting()),
            ],
        );
        let files: Vec<String> = t.touched().into_iter().map(|(f, _)| f).collect();
        assert_eq!(files, ["/home/me/demo/README.md", "/home/me/demo/a.txt"]);
    }

    #[test]
    fn a_placeholder_title_is_not_a_title() {
        assert_eq!(chosen_title("New session - 2026-09-23T15:26:33.263Z"), None);
        assert_eq!(chosen_title("Child session - 2026-09-23T15:26:33.263Z"), None);
        assert_eq!(chosen_title("README edit and diff review workflow").as_deref(), Some("README edit and diff review workflow"));
        assert_eq!(chosen_title("New session - not a date").as_deref(), Some("New session - not a date"));
    }

    #[test]
    fn a_command_line_says_which_session_and_which_directory() {
        let args = |s: &[&str]| s.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert_eq!(read_args(&args(&["opencode"])), Some((None, None)));
        assert_eq!(
            read_args(&args(&["opencode", "-m", "spark/x", "--prompt", "hello there", "../api"])),
            Some((None, Some("../api".into())))
        );
        assert_eq!(read_args(&args(&["opencode", "-s", "ses_1"])), Some((Some("ses_1".into()), None)));
        assert_eq!(read_args(&args(&["opencode", "--session=ses_2", "--auto"])), Some((Some("ses_2".into()), None)));
        // `run`'s positionals are the message, not a directory.
        assert_eq!(read_args(&args(&["opencode", "run", "fix", "the", "bug"])), Some((None, None)));
        // A subcommand that never holds a session is not a live claim at all.
        assert_eq!(read_args(&args(&["opencode", "models"])), None);
        assert_eq!(read_args(&args(&["opencode", "stats", "--days", "3"])), None);
    }

    #[test]
    fn opencode_is_recognised_as_the_npm_package_runs_it() {
        assert!(is_opencode("opencode"));
        assert!(is_opencode("opencode.exe"));
        assert!(is_opencode("/home/me/.local/lib/node_modules/opencode-ai/bin/opencode.exe"));
        assert!(!is_opencode("opencode-helper"));
        assert!(!is_opencode("claude"));
    }

    fn session(id: &str, dir: &str, created: i64, updated: i64) -> OcSession {
        OcSession {
            id: id.into(),
            directory: dir.into(),
            title: None,
            version: None,
            created: ms_to_utc(created),
            updated: ms_to_utc(updated),
            created_ms: created,
            updated_ms: updated,
            tokens_in: 0,
            tokens_out: 0,
            turns: 0,
            tool_calls: 0,
            last_prompt: None,
            last_activity: None,
            status: OcStatus::Finished,
            touched: Vec::new(),
            locator: String::new(),
        }
    }

    fn proc_(pid: u32, dir: &str, started: i64, session: Option<&str>) -> OcProcess {
        OcProcess { pid, started_ms: Some(started), dir: Some(PathBuf::from(dir)), session: session.map(str::to_string) }
    }

    #[test]
    fn a_process_holds_the_session_it_has_moved_since_it_started() {
        let hour = 3_600_000;
        let sessions = [
            session("ses_old", "/nowhere/api", 0, 10 * hour),
            session("ses_new", "/nowhere/api", 20 * hour, 21 * hour),
            session("ses_web", "/nowhere/web", 20 * hour, 21 * hour),
        ];
        // Started at 20h: yesterday's session in the same directory is not its.
        let m = match_processes(&[proc_(7, "/nowhere/api", 20 * hour, None)], &sessions);
        assert_eq!(m.get("ses_new"), Some(&7));
        assert_eq!(m.get("ses_old"), None);
        assert_eq!(m.get("ses_web"), None, "another directory");

        // Opened this morning, not typed into yet: it holds nothing.
        let fresh = match_processes(&[proc_(8, "/nowhere/api", 22 * hour, None)], &sessions);
        assert!(fresh.is_empty());

        // Named on the command line: that one, whatever the times say.
        let named = match_processes(&[proc_(9, "/elsewhere", 22 * hour, Some("ses_old"))], &sessions);
        assert_eq!(named.get("ses_old"), Some(&9));
    }

    #[test]
    fn the_mac_process_table_is_read_as_bsd_writes_it() {
        assert_eq!(parse_etime("05:07"), Some(307));
        assert_eq!(parse_etime("02:05:07"), Some(7507));
        assert_eq!(parse_etime("3-02:05:07"), Some(3 * 86_400 + 7507));
        assert_eq!(parse_etime("soon"), None);
        let m = parse_lsof_cwds("p4210\nfcwd\nn/Users/me/api\np4211\nfcwd\nn/Users/me/web\n");
        assert_eq!(m.get(&4210), Some(&PathBuf::from("/Users/me/api")));
        assert_eq!(m.get(&4211), Some(&PathBuf::from("/Users/me/web")));
    }

    #[test]
    fn tokens_fall_back_to_the_messages_own_counts() {
        let t = thread(&[("msg_b", rows::assistant_step_done()), ("msg_c", rows::assistant_stop())], &[]);
        assert_eq!(t.message_tokens(), (11245 + 11581, 7 + 22 + 12 + 24));
    }

    #[test]
    fn home_beside_an_injected_claude_home_is_a_fake_one_too() {
        let fake = Path::new("/tmp/mogeung-test-home/.claude");
        assert_eq!(home_beside(fake), PathBuf::from("/tmp/mogeung-test-home/.local/share/opencode"));
    }
}
