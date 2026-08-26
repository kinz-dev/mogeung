//! Reader for OpenAI Codex CLI's on-disk session state. Roadmap `R-I1`.
//!
//! Codex keeps a session index in `~/.codex/state_<N>.sqlite` (a `threads`
//! table; 42 migrations as of CLI 0.145.0) and per-session rollout transcripts
//! as `rollout-*.jsonl` / `rollout-*.jsonl.zst` under `sessions/` and
//! `archived_sessions/`. mogeung reads both, strictly read-only. Nothing here
//! starts or controls an agent.
//!
//! This module is standalone: discovery, index read, rollout classification and
//! a pure status heuristic. Wiring into the scan loop happens elsewhere.
//!
//! **Evidence honesty.** The index schema was read from a real (fresh, empty)
//! install. Every claim about rollout line shapes is inference from the CLI
//! binary — no rollout file has ever existed on the author's machine. So the
//! classifier follows the `adapter.rs` canary discipline exactly: a kind we
//! have not heard of is a counted, named [`CodexLineOutcome::Unknown`], never a
//! panic and never a silent drop, because here more than anywhere we expect to
//! be wrong.

use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use mogeung_core::health::LineClass;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read as _, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Where Codex keeps its state, unless told otherwise.
///
/// Mirrors `watcher::default_home`: callers pass the resolved path around
/// explicitly so tests can point everything at a synthetic install.
pub fn default_home() -> PathBuf {
    if let Ok(p) = std::env::var("CODEX_HOME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".codex")
}

/// A Codex installation on disk (the `~/.codex` directory).
#[derive(Debug, Clone)]
pub struct CodexInstall {
    pub home: PathBuf,
}

impl CodexInstall {
    /// `Some` iff the directory exists. An existing-but-empty install is still
    /// an install — the product must report "present, no sessions" truthfully
    /// rather than nothing at all.
    pub fn discover(home: &Path) -> Option<CodexInstall> {
        if home.is_dir() {
            Some(CodexInstall {
                home: home.to_path_buf(),
            })
        } else {
            None
        }
    }

    /// The newest session index: `state_<N>.sqlite` with the highest `N`.
    ///
    /// Globbed, never hardcoded — the suffix is a migration-generation counter
    /// and has already reached 5 in a young tool.
    pub fn state_db(&self) -> Option<PathBuf> {
        let entries = std::fs::read_dir(&self.home).ok()?;
        let mut best: Option<(u64, PathBuf)> = None;
        for e in entries.flatten() {
            let path = e.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(version) = name
                .strip_prefix("state_")
                .and_then(|r| r.strip_suffix(".sqlite"))
                .and_then(|n| n.parse::<u64>().ok())
            else {
                continue;
            };
            if best.as_ref().map(|(v, _)| version > *v).unwrap_or(true) {
                best = Some((version, path));
            }
        }
        best.map(|(_, p)| p)
    }

    /// Read the thread index. Never fails: an unreadable database comes back as
    /// an empty index with `error` set, because Codex holds a WAL on this file
    /// and a locked or half-migrated read must degrade, not stop the scan.
    pub fn list_threads(&self) -> ThreadIndex {
        let Some(db) = self.state_db() else {
            // Present but indexless — a fresh install before first run, or a
            // future layout change. Honest empty, not an error.
            return ThreadIndex::default();
        };
        let mut out = ThreadIndex {
            db: Some(db.clone()),
            ..Default::default()
        };
        match read_threads(&db) {
            Ok((threads, missing)) => {
                out.threads = threads;
                out.missing_columns = missing;
                out.threads
                    .sort_by_key(|t| std::cmp::Reverse(t.recency()));
            }
            Err(e) => out.error = Some(e.to_string()),
        }
        out
    }

    /// Find the rollout file for a thread, tolerating a stale index.
    ///
    /// `rollout_path` in the index can point at a file that has since been
    /// archived or moved — Codex itself carries read-repair for it. If the
    /// recorded path is gone, search `sessions/` and `archived_sessions/` for a
    /// rollout file named with the thread id. `None` means index-only: the
    /// thread is still real, we just have no transcript for it.
    pub fn resolve_rollout(&self, thread: &CodexThread) -> Option<PathBuf> {
        if let Some(p) = &thread.rollout_path {
            if p.is_file() {
                return Some(p.clone());
            }
        }
        for root in ["sessions", "archived_sessions"] {
            if let Some(p) = find_rollout_by_id(&self.home.join(root), &thread.id, 0) {
                return Some(p);
            }
        }
        None
    }

    /// Resolve and read a thread's rollout whole. `None` means no rollout file
    /// exists anywhere — degrade to what the index alone says.
    pub fn read_thread_rollout(&self, thread: &CodexThread) -> Option<CodexRollout> {
        self.resolve_rollout(thread).map(|p| read_rollout(&p))
    }
}

/// The result of one index read. `threads` empty with `error` set means the
/// read failed; empty with no error means an honestly empty install.
#[derive(Debug, Default)]
pub struct ThreadIndex {
    /// The database that was read, if one was found.
    pub db: Option<PathBuf>,
    /// Newest first, by `recency_at` (falling back through the other stamps).
    pub threads: Vec<CodexThread>,
    /// Columns we know how to use that this database does not have. Schema
    /// drift made visible — the index-side canary.
    pub missing_columns: Vec<String>,
    /// Set when the database could not be read at all.
    pub error: Option<String>,
}

/// One row of the Codex `threads` table, every field optional.
///
/// Columns are selected by name and any absence degrades that field to its
/// default: 42 migrations exist and both older and newer installs will differ
/// from the one this was written against.
#[derive(Debug, Clone, Default)]
pub struct CodexThread {
    pub id: String,
    /// As recorded in the index. May be stale — resolve through
    /// [`CodexInstall::resolve_rollout`], never trust it blindly.
    pub rollout_path: Option<PathBuf>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub recency_at: Option<DateTime<Utc>>,
    pub source: Option<String>,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub name: Option<String>,
    pub preview: Option<String>,
    pub first_user_message: Option<String>,
    pub sandbox_policy: Option<String>,
    pub approval_mode: Option<String>,
    pub tokens_used: Option<u64>,
    pub archived: bool,
    pub git_sha: Option<String>,
    pub git_branch: Option<String>,
    pub git_origin_url: Option<String>,
    pub cli_version: Option<String>,
}

impl CodexThread {
    /// Best ordering stamp available.
    pub fn recency(&self) -> Option<DateTime<Utc>> {
        self.recency_at.or(self.updated_at).or(self.created_at)
    }
}

/// Columns this reader uses, for drift reporting. Only names we would actually
/// read belong here — reporting a column we ignore anyway would be noise.
const EXPECTED_COLUMNS: &[&str] = &[
    "id",
    "rollout_path",
    "created_at",
    "updated_at",
    "recency_at",
    "source",
    "model_provider",
    "model",
    "cwd",
    "title",
    "name",
    "preview",
    "first_user_message",
    "sandbox_policy",
    "approval_mode",
    "tokens_used",
    "archived",
    "git_sha",
    "git_branch",
    "git_origin_url",
    "cli_version",
];

fn read_threads(db: &Path) -> Result<(Vec<CodexThread>, Vec<String>)> {
    // Strictly read-only. Codex owns this database and holds a live WAL on it;
    // we must never create or modify WAL/SHM side files. SQLITE_OPEN_READ_ONLY
    // guarantees that: a read may fail if the -wal file is unreadable, and
    // failing is the correct behaviour — the caller flags it and moves on.
    let conn = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // `SELECT *` on purpose: naming columns would error on any install whose
    // migration level differs. We take what is there and map by name.
    let mut stmt = conn.prepare("SELECT * FROM threads")?;
    let cols: HashMap<String, usize> = stmt
        .column_names()
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_string(), i))
        .collect();
    let missing: Vec<String> = EXPECTED_COLUMNS
        .iter()
        .filter(|c| !cols.contains_key(**c))
        .map(|c| c.to_string())
        .collect();

    let mut threads = Vec::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let text = |name: &str| -> Option<String> {
            let i = *cols.get(name)?;
            match row.get_ref(i).ok()? {
                ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Integer(n) => Some(n.to_string()),
                ValueRef::Real(f) => Some(f.to_string()),
                _ => None,
            }
        };
        let int = |name: &str| -> Option<i64> {
            let i = *cols.get(name)?;
            match row.get_ref(i).ok()? {
                ValueRef::Integer(n) => Some(n),
                ValueRef::Real(f) => Some(f as i64),
                ValueRef::Text(t) => std::str::from_utf8(t).ok()?.parse().ok(),
                _ => None,
            }
        };
        // Prefer the millisecond column, fall back to unix seconds.
        let stamp = |base: &str| -> Option<DateTime<Utc>> {
            let ms = int(&format!("{base}_ms")).or_else(|| int(base).map(|s| s * 1000))?;
            Utc.timestamp_millis_opt(ms).single()
        };

        // A row with no id is unusable; skip it rather than fail the read.
        let Some(id) = text("id").filter(|s| !s.is_empty()) else {
            continue;
        };
        threads.push(CodexThread {
            id,
            rollout_path: text("rollout_path")
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
            created_at: stamp("created_at"),
            updated_at: stamp("updated_at"),
            recency_at: stamp("recency_at"),
            source: text("source"),
            model_provider: text("model_provider"),
            model: text("model"),
            cwd: text("cwd"),
            title: text("title"),
            name: text("name"),
            preview: text("preview"),
            first_user_message: text("first_user_message"),
            sandbox_policy: text("sandbox_policy"),
            approval_mode: text("approval_mode"),
            tokens_used: int("tokens_used").and_then(|n| u64::try_from(n).ok()),
            archived: int("archived").map(|n| n != 0).unwrap_or(false),
            git_sha: text("git_sha"),
            git_branch: text("git_branch"),
            git_origin_url: text("git_origin_url"),
            cli_version: text("cli_version"),
        });
    }
    Ok((threads, missing))
}

/// Recursive search for `rollout-*<id>*.jsonl[.zst]`, bounded so a hostile or
/// cyclic tree cannot hang the scan. Rollouts live a few directories deep
/// (`sessions/<year>/<month>/<day>/`).
fn find_rollout_by_id(dir: &Path, id: &str, depth: u32) -> Option<PathBuf> {
    if depth > 6 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            if let Some(found) = find_rollout_by_id(&path, id, depth + 1) {
                return Some(found);
            }
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with("rollout-")
            && name.contains(id)
            && (name.ends_with(".jsonl") || name.ends_with(".jsonl.zst"))
        {
            return Some(path);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rollout line classification
// ---------------------------------------------------------------------------

/// Top-level rollout line kinds we understand and deliberately do not surface.
///
/// Inference from the CLI binary, not observation — see the module note. Each
/// entry is a deliberate act, exactly as in `adapter::KNOWN_IGNORED`.
pub const KNOWN_IGNORED: &[&str] = &[
    // Compaction marker: history was summarised. Carries nothing live.
    "compacted",
    // Codex's pre-edit file snapshot bookkeeping, analogous to Claude Code's
    // `file-history-snapshot` which ADR-0004 declines to read.
    "ghost_snapshot",
    // `0.149`: a full snapshot of the environment the turn ran in — AGENTS.md
    // digests, app instructions, the collaboration mode, a filesystem listing.
    // Ignored on the same grounds as `ghost_snapshot`: everything in it that
    // this product acts on (`cwd`) is already carried by `session_meta`, and
    // the rest is a large blob re-emitted per turn.
    "world_state",
];

/// Top-level kinds this parser extracts data from.
pub const HANDLED: &[&str] = &["session_meta", "turn_context", "response_item", "event_msg"];

/// Item kinds we understand under `response_item` / `event_msg` payloads.
/// An item kind outside this list classifies the whole line as
/// [`CodexLineOutcome::Unknown`] with a `kind/item` compound name, so drift in
/// the nested taxonomy is exactly as loud as drift in the outer one.
pub const KNOWN_ITEMS: &[&str] = &[
    "agent_message",
    "reasoning",
    "function_call",
    "function_call_output",
    "local_shell_call",
    "command_execution",
    "user_message",
    "turn_started",
    "turn_complete",
    "turn_aborted",
    "exec_approval_request",
    "apply_patch_approval_request",
    // `0.149.1` renamed the turn boundary and moved message content behind a
    // wrapper. The old names stay: this list is what we can *read*, and a
    // rollout written by an older Codex is still on disk and still valid.
    // `R-J70`.
    "task_started",
    "task_complete",
    "token_count",
    "item_completed",
    "message",
];

/// The `item.type` values understood inside an `event_msg/item_completed`.
///
/// `0.149` wraps what used to be top-level items in a completion envelope, so
/// the taxonomy gained a third level. This list keeps drift down there exactly
/// as loud as drift in the two above it: an item type outside it surfaces as
/// `event_msg/item_completed/<Type>` in the health panel rather than being
/// silently dropped.
///
/// Deliberately short. Only what has actually been observed is listed — a tool
/// call or a file change will announce itself the first time one happens, which
/// is the canary doing its job rather than a gap. `R-J70`.
pub const KNOWN_COMPLETED_ITEMS: &[&str] = &["UserMessage", "AgentMessage"];

/// A Codex thread that is **running right now**, and the process running it.
///
/// `R-J70`. Until `0.149` there was no way to know: the index records threads,
/// not liveness, so `alive` was a recency guess — "it wrote something in the
/// last ten minutes" — which is wrong in the direction that matters most. A
/// session sitting and waiting for you is the single most important row on the
/// board, and it is also the one that has written nothing for twenty minutes.
///
/// Codex now takes an advisory `flock` on
/// `~/.codex/thread-writer-locks/<thread-id>.lock` for as long as a thread is
/// open. That is a real registry: the file names the thread, and the lock names
/// the process.
#[derive(Debug, Clone, Default)]
pub struct CodexLive {
    /// Thread id → the pid holding its writer lock, where the platform can say.
    pub threads: HashMap<String, Option<u32>>,
    /// False when this install has no lock directory at all — an older Codex.
    /// Callers must then fall back rather than conclude "nothing is alive".
    pub supported: bool,
}

/// Which threads are open, by asking the lock directory.
///
/// **Never acquires anything.** Testing a lock by trying to take it is the
/// obvious implementation and is forbidden here: this daemon must not compete
/// with an agent for a resource the agent needs (ADR-0003), and a momentary
/// grab is exactly the race that would make a Codex thread fail to start. On
/// Linux the holder is read out of `/proc/locks`, which touches nothing. Where
/// that is unavailable — macOS — presence of the file is the answer and the pid
/// is `None`; a lock left behind by a crash reads as alive there, which is the
/// stated cost of not interfering.
pub fn scan_live(home: &Path) -> CodexLive {
    let dir = home.join("thread-writer-locks");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return CodexLive::default();
    };
    let held = locks_by_inode();
    let mut out = CodexLive {
        supported: true,
        ..Default::default()
    };
    for e in entries.flatten() {
        let path = e.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // `.coordination.lock` sits beside the per-thread ones and names no
        // thread.
        let Some(id) = name.strip_suffix(".lock").filter(|s| !s.starts_with('.')) else {
            continue;
        };
        match &held {
            // Linux: the file is evidence only if something actually holds it,
            // so a crash leaves no ghost on the board.
            Some(by_inode) => {
                let Ok(meta) = e.metadata() else { continue };
                let ino = std::os::unix::fs::MetadataExt::ino(&meta);
                if let Some(pid) = by_inode.get(&ino) {
                    out.threads.insert(id.to_string(), Some(*pid));
                }
            }
            None => {
                out.threads.insert(id.to_string(), None);
            }
        }
    }
    out
}

/// `inode → holder pid` for every advisory lock on this machine, or `None`
/// where the kernel does not publish them.
fn locks_by_inode() -> Option<HashMap<u64, u32>> {
    let text = std::fs::read_to_string("/proc/locks").ok()?;
    let mut out = HashMap::new();
    for line in text.lines() {
        // `161: FLOCK  ADVISORY  WRITE 487395 fc:01:47072312 0 EOF`
        let f: Vec<&str> = line.split_whitespace().collect();
        let Some(pos) = f.iter().position(|x| x.contains(':') && x.matches(':').count() == 2)
        else {
            continue;
        };
        let Some(pid) = pos.checked_sub(1).and_then(|i| f.get(i)) else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else { continue };
        let Some(ino) = f[pos].rsplit(':').next().and_then(|i| i.parse::<u64>().ok()) else {
            continue;
        };
        out.insert(ino, pid);
    }
    Some(out)
}

/// The outcome of reading one rollout line. Mirrors `adapter::LineOutcome` so
/// Codex drift feeds the same health vocabulary as Claude drift.
#[derive(Debug)]
pub enum CodexLineOutcome {
    /// Understood, and it produced something.
    Parsed(Box<CodexParsed>),
    /// A known kind we deliberately skip.
    Ignored,
    /// A handled kind that yielded nothing this time.
    Barren { kind: String },
    /// A kind (or nested item kind, as `outer/inner`) we have never heard of.
    /// The canary — and since every shape here is inference, the part most
    /// likely to fire first.
    Unknown { kind: String },
    /// Not JSON, or no `type` field.
    Malformed,
}

impl CodexLineOutcome {
    pub fn class(&self) -> LineClass {
        match self {
            CodexLineOutcome::Parsed(_) => LineClass::Parsed,
            CodexLineOutcome::Ignored => LineClass::Ignored,
            CodexLineOutcome::Barren { .. } => LineClass::Barren,
            CodexLineOutcome::Unknown { .. } => LineClass::Unknown,
            CodexLineOutcome::Malformed => LineClass::Malformed,
        }
    }

    pub fn parsed(self) -> Option<CodexParsed> {
        match self {
            CodexLineOutcome::Parsed(p) => Some(*p),
            _ => None,
        }
    }
}

/// What one rollout line means for the waiting/working/done heuristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailEvent {
    TurnStarted,
    TurnComplete,
    TurnAborted,
    /// Codex asked the human to approve an exec or a patch.
    ApprovalRequest,
    /// The human sent a prompt.
    UserMessage,
    /// The agent did something mid-turn (message, reasoning, tool call).
    AgentActivity,
}

/// Token usage as Codex names it. Note `cached_input_tokens`, not Claude's
/// `cache_read_input_tokens` — the two vocabularies must not be blurred.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_output_tokens: u64,
}

impl TokenUsage {
    pub fn total_in(&self) -> u64 {
        self.input_tokens + self.cached_input_tokens
    }
    pub fn total_out(&self) -> u64 {
        self.output_tokens + self.reasoning_output_tokens
    }
}

/// Everything one rollout line tells us.
#[derive(Debug, Default)]
pub struct CodexParsed {
    pub ts: Option<DateTime<Utc>>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub cli_version: Option<String>,
    pub git_branch: Option<String>,
    /// Message text (agent or user), truncated.
    pub text: Option<String>,
    /// This line is a fresh prompt from the human.
    pub is_user_turn: bool,
    pub tool_calls: u32,
    /// One-line description of what the agent is doing right now.
    pub last_activity: Option<String>,
    /// A usage snapshot seen on this line. Whether Codex reports these per-turn
    /// or cumulatively is unverified; callers should prefer the index's
    /// `tokens_used` and treat this as supporting evidence.
    pub usage: Option<TokenUsage>,
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

fn one_line(s: &str, n: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate(&flat, n)
}

fn str_at<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str())
}

/// Message text wherever a payload might keep it: a `message`/`text` string, or
/// a `content` array of `{text}` blocks. Codex's exact field is unverified, so
/// try each shape and take the first that yields.
fn message_text(payload: &Value) -> Option<String> {
    if let Some(s) = str_at(payload, "message").or_else(|| str_at(payload, "text")) {
        if !s.trim().is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(items) = payload.get("content").and_then(|c| c.as_array()) {
        let joined = items
            .iter()
            .filter_map(|b| str_at(b, "text"))
            .collect::<Vec<_>>()
            .join("\n");
        if !joined.trim().is_empty() {
            return Some(joined);
        }
    }
    None
}

/// Read a Codex-named usage object from a payload, looking in the places the
/// binary suggests one can appear.
fn usage_from(payload: &Value) -> Option<TokenUsage> {
    let candidates = [
        payload.get("usage"),
        payload.get("info").and_then(|i| i.get("total_token_usage")),
        payload.get("info").and_then(|i| i.get("last_token_usage")),
        Some(payload),
    ];
    for c in candidates.into_iter().flatten() {
        let n = |k: &str| c.get(k).and_then(|x| x.as_u64());
        let fields = [
            n("input_tokens"),
            n("cached_input_tokens"),
            n("output_tokens"),
            n("reasoning_output_tokens"),
        ];
        if fields.iter().any(|f| f.is_some()) {
            return Some(TokenUsage {
                input_tokens: fields[0].unwrap_or(0),
                cached_input_tokens: fields[1].unwrap_or(0),
                output_tokens: fields[2].unwrap_or(0),
                reasoning_output_tokens: fields[3].unwrap_or(0),
            });
        }
    }
    None
}

/// Classify and read one line of a rollout file.
///
/// Never panics and never fails — same contract as `adapter::parse_line`, and
/// held to more strictly here because the format was never observed, only
/// inferred.
pub fn parse_rollout_line(line: &str) -> CodexLineOutcome {
    let line = line.trim();
    if line.is_empty() {
        return CodexLineOutcome::Ignored;
    }
    if !line.starts_with('{') {
        return CodexLineOutcome::Malformed;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return CodexLineOutcome::Malformed;
    };
    let Some(kind) = str_at(&v, "type") else {
        return CodexLineOutcome::Malformed;
    };
    let kind = kind.to_string();

    if KNOWN_IGNORED.contains(&kind.as_str()) {
        return CodexLineOutcome::Ignored;
    }
    if !HANDLED.contains(&kind.as_str()) {
        return CodexLineOutcome::Unknown { kind };
    }
    extract(&v, &kind)
}

fn extract(v: &Value, kind: &str) -> CodexLineOutcome {
    let mut out = CodexParsed {
        ts: str_at(v, "timestamp")
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc)),
        ..Default::default()
    };
    match kind {
        "session_meta" => {
            // Tolerate the payload living at the top level: for these two
            // kinds the envelope shape is inferred and the fallback cannot be
            // confused with anything else.
            let payload = v.get("payload").unwrap_or(v);
            out.cwd = str_at(payload, "cwd").map(str::to_string);
            out.cli_version = str_at(payload, "cli_version").map(str::to_string);
            out.git_branch = payload
                .get("git")
                .and_then(|g| str_at(g, "branch"))
                .filter(|b| !b.is_empty() && *b != "HEAD")
                .map(str::to_string);
        }
        "turn_context" => {
            let payload = v.get("payload").unwrap_or(v);
            out.cwd = str_at(payload, "cwd").map(str::to_string);
            out.model = str_at(payload, "model").map(str::to_string);
        }
        "response_item" | "event_msg" => {
            // No top-level fallback here: the envelope's own `type` would
            // masquerade as an item kind and turn every payloadless line into
            // fake nested drift.
            let Some(payload) = v.get("payload") else {
                return CodexLineOutcome::Barren {
                    kind: kind.to_string(),
                };
            };
            let Some(item) = str_at(payload, "type") else {
                return CodexLineOutcome::Barren {
                    kind: kind.to_string(),
                };
            };
            if !KNOWN_ITEMS.contains(&item) {
                // Nested drift, reported compound so the health panel can say
                // exactly where the format moved.
                return CodexLineOutcome::Unknown {
                    kind: format!("{kind}/{item}"),
                };
            }
            match item {
                "agent_message" => {
                    out.text = message_text(payload).map(|t| truncate(&t, 8000));
                    out.tail = Some(TailEvent::AgentActivity);
                }
                "user_message" => {
                    out.text = message_text(payload).map(|t| truncate(&t, 4000));
                    out.is_user_turn = true;
                    out.tail = Some(TailEvent::UserMessage);
                }
                "reasoning" => {
                    out.tail = Some(TailEvent::AgentActivity);
                }
                "function_call" => {
                    let name = str_at(payload, "name").unwrap_or("tool");
                    let args = str_at(payload, "arguments").unwrap_or("");
                    out.tool_calls = 1;
                    out.last_activity = Some(one_line(&format!("{name}: {args}"), 160));
                    out.tail = Some(TailEvent::AgentActivity);
                }
                "local_shell_call" | "command_execution" => {
                    let cmd = command_of(payload);
                    out.tool_calls = 1;
                    out.last_activity = Some(one_line(&format!("shell: {cmd}"), 160));
                    out.tail = Some(TailEvent::AgentActivity);
                }
                "function_call_output" => {
                    out.tail = Some(TailEvent::AgentActivity);
                }
                "turn_started" => out.tail = Some(TailEvent::TurnStarted),
                // `0.149`'s turn boundary, under new names. `R-J70`.
                "task_started" => out.tail = Some(TailEvent::TurnStarted),
                "task_complete" => {
                    out.tail = Some(TailEvent::TurnComplete);
                    // The reply itself rides on the completion event, which is
                    // the only place it appears without the model-facing
                    // boilerplate around it.
                    out.text = str_at(payload, "last_agent_message")
                        .map(|t| truncate(t, 8000).to_string());
                    // No usage here: `0.149` moved it to its own `token_count`
                    // event, which may arrive before or after this one.
                    out.usage = usage_from(payload);
                }
                // Usage as its own event since `0.149`. `usage_from` already
                // looks in `info.total_token_usage`, which is where it landed.
                "token_count" => out.usage = usage_from(payload),
                // The completion envelope `0.149` wrapped items in. The turn
                // *and* the text are read from here rather than from
                // `response_item/message`, because this stream carries only
                // what actually happened: the model-facing one also replays
                // the system prompt, the skills block and an
                // `<environment_context>` under `role: "user"`, any of which
                // would otherwise be mistaken for something you typed.
                "item_completed" => {
                    let Some(inner) = payload.get("item") else {
                        return CodexLineOutcome::Barren {
                            kind: format!("{kind}/{item}"),
                        };
                    };
                    let Some(it) = str_at(inner, "type") else {
                        return CodexLineOutcome::Barren {
                            kind: format!("{kind}/{item}"),
                        };
                    };
                    if !KNOWN_COMPLETED_ITEMS.contains(&it) {
                        return CodexLineOutcome::Unknown {
                            kind: format!("{kind}/{item}/{it}"),
                        };
                    }
                    match it {
                        "UserMessage" => {
                            out.text = message_text(inner).map(|t| truncate(&t, 4000));
                            out.is_user_turn = true;
                            out.tail = Some(TailEvent::UserMessage);
                        }
                        "AgentMessage" => {
                            out.text = message_text(inner).map(|t| truncate(&t, 8000));
                            out.tail = Some(TailEvent::AgentActivity);
                        }
                        _ => {
                            return CodexLineOutcome::Barren {
                                kind: format!("{kind}/{item}/{it}"),
                            }
                        }
                    }
                }
                // The model-facing transcript. Read as **activity only**: every
                // line of it is duplicated by `item_completed` above, and the
                // duplicate is the clean one. Counting a turn here as well
                // would double every turn a session has.
                "message" => out.tail = Some(TailEvent::AgentActivity),
                "turn_complete" => {
                    out.tail = Some(TailEvent::TurnComplete);
                    out.usage = usage_from(payload);
                }
                "turn_aborted" => out.tail = Some(TailEvent::TurnAborted),
                "exec_approval_request" => {
                    let cmd = command_of(payload);
                    out.last_activity =
                        Some(one_line(&format!("approval requested: {cmd}"), 160));
                    out.tail = Some(TailEvent::ApprovalRequest);
                }
                "apply_patch_approval_request" => {
                    out.last_activity = Some("approval requested: apply patch".to_string());
                    out.tail = Some(TailEvent::ApprovalRequest);
                }
                // Unreachable: every item is already sorted into known or
                // unknown above. Kept so adding a name to `KNOWN_ITEMS` without
                // an arm degrades to barren rather than a silent lie.
                _ => {
                    return CodexLineOutcome::Barren {
                        kind: format!("{kind}/{item}"),
                    }
                }
            }
        }
        // Unreachable by construction, same defence as adapter::extract.
        _ => {
            return CodexLineOutcome::Barren {
                kind: kind.to_string(),
            }
        }
    }

    if out.cwd.is_none()
        && out.model.is_none()
        && out.cli_version.is_none()
        && out.git_branch.is_none()
        && out.text.is_none()
        && out.last_activity.is_none()
        && out.usage.is_none()
        && out.tail.is_none()
    {
        return CodexLineOutcome::Barren {
            kind: kind.to_string(),
        };
    }
    CodexLineOutcome::Parsed(Box::new(out))
}

/// A command rendered from whatever shape it arrives in: a string, or an argv
/// array.
fn command_of(payload: &Value) -> String {
    match payload.get("command") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Rollout aggregation
// ---------------------------------------------------------------------------

/// Per-class line counts for one rollout — the canary's raw material.
#[derive(Debug, Default, Clone)]
pub struct RolloutCounts {
    pub parsed: u64,
    pub ignored: u64,
    pub barren: u64,
    pub unknown: u64,
    pub malformed: u64,
    /// Every unknown kind by name (compound `outer/inner` for nested drift).
    pub unknown_kinds: Vec<(String, u64)>,
}

impl RolloutCounts {
    pub fn record(&mut self, outcome: &CodexLineOutcome) {
        match outcome {
            CodexLineOutcome::Parsed(_) => self.parsed += 1,
            CodexLineOutcome::Ignored => self.ignored += 1,
            CodexLineOutcome::Barren { .. } => self.barren += 1,
            CodexLineOutcome::Unknown { kind } => {
                self.unknown += 1;
                match self.unknown_kinds.iter_mut().find(|(k, _)| k == kind) {
                    Some((_, n)) => *n += 1,
                    None => self.unknown_kinds.push((kind.clone(), 1)),
                }
            }
            CodexLineOutcome::Malformed => self.malformed += 1,
        }
    }

    pub fn lines_seen(&self) -> u64 {
        self.parsed + self.ignored + self.barren + self.unknown + self.malformed
    }
}

/// Everything a rollout file (or a tail of one) told us, with the counts that
/// say how much of it we actually understood.
#[derive(Debug, Default, Clone)]
pub struct CodexRollout {
    pub counts: RolloutCounts,
    /// Tail-relevant events in file order; feed to [`derive_status`].
    pub tail: Vec<TailEvent>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub cli_version: Option<String>,
    pub git_branch: Option<String>,
    pub last_text: Option<String>,
    pub last_activity: Option<String>,
    pub last_ts: Option<DateTime<Utc>>,
    pub tool_calls: u64,
    /// The most recent usage snapshot seen. See [`CodexParsed::usage`] for the
    /// honesty caveat.
    pub last_usage: Option<TokenUsage>,
    /// Set when the file could not be read or decoded at all; every other
    /// field is then empty. Flagged, never panicked.
    pub error: Option<String>,
}

impl CodexRollout {
    /// Classify one line and fold it in.
    pub fn absorb_line(&mut self, line: &str) {
        let outcome = parse_rollout_line(line);
        self.counts.record(&outcome);
        if let Some(p) = outcome.parsed() {
            if p.ts.is_some() {
                self.last_ts = p.ts;
            }
            if p.cwd.is_some() {
                self.cwd = p.cwd;
            }
            if p.model.is_some() {
                self.model = p.model;
            }
            if p.cli_version.is_some() {
                self.cli_version = p.cli_version;
            }
            if p.git_branch.is_some() {
                self.git_branch = p.git_branch;
            }
            if p.text.is_some() {
                self.last_text = p.text;
            }
            if p.last_activity.is_some() {
                self.last_activity = p.last_activity;
            }
            if p.usage.is_some() {
                self.last_usage = p.usage;
            }
            self.tool_calls += u64::from(p.tool_calls);
            if let Some(t) = p.tail {
                self.tail.push(t);
            }
        }
    }

    /// The waiting/working/done heuristic over this rollout's tail.
    pub fn status(&self) -> CodexStatus {
        derive_status(&self.tail)
    }
}

/// Read a rollout file whole, decoding `.zst` transparently. Errors come back
/// as an empty rollout with `error` set.
pub fn read_rollout(path: &Path) -> CodexRollout {
    let mut out = CodexRollout::default();
    match read_rollout_bytes(path) {
        Ok(text) => {
            for line in text.lines() {
                if !line.trim().is_empty() {
                    out.absorb_line(line);
                }
            }
        }
        Err(e) => out.error = Some(format!("{}: {e}", path.display())),
    }
    out
}

fn is_zst(path: &Path) -> bool {
    path.extension().and_then(|x| x.to_str()) == Some("zst")
}

fn read_rollout_bytes(path: &Path) -> Result<String> {
    let raw = std::fs::read(path)?;
    let bytes = if is_zst(path) {
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(std::io::Cursor::new(&raw[..]))
            .map_err(|e| anyhow::anyhow!("zstd: {e}"))?;
        let mut buf = Vec::new();
        decoder
            .read_to_end(&mut buf)
            .map_err(|e| anyhow::anyhow!("zstd: {e}"))?;
        buf
    } else {
        raw
    };
    // Lossy: a stray invalid byte should cost one mangled line, not the file.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Incremental reader for rollout files, following `watcher::Tailer`'s
/// byte-offset shape.
///
/// Plain `.jsonl` files are seeked like transcripts. `.zst` files cannot be
/// seeked into mid-stream, so they are decoded whole on each call and the
/// offset tracks *decoded* bytes — acceptable because archived rollouts are
/// finished files that change rarely if ever.
#[derive(Default)]
pub struct RolloutTailer {
    offsets: HashMap<PathBuf, u64>,
}

impl RolloutTailer {
    /// Read whatever has been appended since last time. A file that shrank was
    /// rewritten, so it is re-read from the start.
    pub fn read_new(&mut self, path: &Path) -> Result<Vec<String>> {
        if is_zst(path) {
            return self.read_new_zst(path);
        }
        let meta = std::fs::metadata(path)?;
        let size = meta.len();
        let offset = self.offsets.entry(path.to_path_buf()).or_insert(0);
        if size < *offset {
            *offset = 0;
        }
        if size == *offset {
            return Ok(Vec::new());
        }
        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(*offset))?;
        let reader = std::io::BufReader::new(file);
        let mut lines = Vec::new();
        let mut consumed = *offset;
        for line in std::io::BufRead::lines(reader) {
            let line = line?;
            consumed += line.len() as u64 + 1;
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
        // Never advance past the observed size: a line mid-write is re-read
        // next tick rather than half-parsed.
        *offset = consumed.min(size);
        Ok(lines)
    }

    fn read_new_zst(&mut self, path: &Path) -> Result<Vec<String>> {
        let text = read_rollout_bytes(path)?;
        let total = text.len() as u64;
        let offset = self.offsets.entry(path.to_path_buf()).or_insert(0);
        if total < *offset {
            *offset = 0;
        }
        if total == *offset {
            return Ok(Vec::new());
        }
        let new = &text[*offset as usize..];
        // Only consume up to the last newline, so a final unterminated line is
        // re-read whole next time.
        let consumable = match new.rfind('\n') {
            Some(i) => i + 1,
            None => 0,
        };
        let lines = new[..consumable]
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect();
        *offset += consumable as u64;
        Ok(lines)
    }

    pub fn forget(&mut self, path: &Path) {
        self.offsets.remove(path);
    }
}

/// How much of a rollout's tail to keep between scans. [`derive_status`]
/// walks backwards and returns at the first decisive event, so only the
/// recent end of the tail ever matters; the cap only has to be comfortably
/// larger than the longest run of `AgentActivity` chatter between turn
/// boundaries.
const MAX_TAIL_EVENTS: usize = 64;

/// Incremental scan state: one folded [`CodexRollout`] per thread, advanced by
/// a [`RolloutTailer`].
///
/// This exists because the scan loop used to call [`read_rollout`] on every
/// non-archived thread every tick — the whole file, re-read and re-parsed at
/// the poll rate, forever. Folding lines into a kept rollout costs only the
/// appended bytes. The trade is memory: one rollout summary per live thread,
/// with its tail capped at [`MAX_TAIL_EVENTS`].
#[derive(Default)]
pub struct ScanCache {
    tailer: RolloutTailer,
    threads: HashMap<String, (PathBuf, CodexRollout)>,
}

impl ScanCache {
    /// The thread's rollout, brought up to date by reading only what was
    /// appended since last time. `None` when no rollout file can be found —
    /// the same answer [`CodexInstall::read_thread_rollout`] gave.
    ///
    /// The resolved path is cached per thread, so the recursive
    /// search-by-id that a stale index entry triggers happens once, not once
    /// per tick.
    pub fn update(&mut self, install: &CodexInstall, thread: &CodexThread) -> Option<CodexRollout> {
        let need_resolve = match self.threads.get(&thread.id) {
            Some((p, _)) => !p.exists(),
            None => true,
        };
        if need_resolve {
            let path = install.resolve_rollout(thread)?;
            self.tailer.forget(&path);
            self.threads.insert(thread.id.clone(), (path, CodexRollout::default()));
        }
        let (path, roll) = self.threads.get_mut(&thread.id).expect("inserted above");
        match self.tailer.read_new(path) {
            Ok(lines) => {
                for line in &lines {
                    roll.absorb_line(line);
                }
                if roll.tail.len() > MAX_TAIL_EVENTS {
                    let excess = roll.tail.len() - MAX_TAIL_EVENTS;
                    roll.tail.drain(..excess);
                }
                roll.error = None;
            }
            // Flagged, never panicked — and the state already folded is kept.
            Err(e) => roll.error = Some(format!("{}: {e}", path.display())),
        }
        Some(roll.clone())
    }

    /// Drop threads no longer worth following (archived or deleted), and
    /// their byte offsets with them.
    pub fn retain(&mut self, keep: &std::collections::HashSet<String>) {
        let dropped: Vec<PathBuf> = self
            .threads
            .iter()
            .filter(|(id, _)| !keep.contains(*id))
            .map(|(_, (p, _))| p.clone())
            .collect();
        for p in dropped {
            self.tailer.forget(&p);
        }
        self.threads.retain(|id, _| keep.contains(id));
    }
}

// ---------------------------------------------------------------------------
// Status heuristic
// ---------------------------------------------------------------------------

/// What the rollout tail suggests the session is doing. A heuristic — the
/// health panel must label it as such — because Codex has no live-status
/// registry like Claude Code's `sessions/<pid>.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodexStatus {
    /// A turn is underway.
    Working,
    /// Codex asked for approval and nothing has moved since. The state the
    /// whole product exists to surface.
    Waiting,
    /// The last turn finished or was aborted (or the file shows no turns).
    Done,
}

/// Pure derivation of status from tail events, newest last.
///
/// Walking backwards: a trailing approval request with no later turn event is
/// waiting; the most recent turn boundary otherwise decides working vs done; a
/// trailing user message means a turn is about to start. Mid-turn agent
/// activity is skipped so the decision rests on turn boundaries, not chatter.
pub fn derive_status(tail: &[TailEvent]) -> CodexStatus {
    for e in tail.iter().rev() {
        match e {
            TailEvent::ApprovalRequest => return CodexStatus::Waiting,
            TailEvent::TurnStarted | TailEvent::UserMessage => return CodexStatus::Working,
            TailEvent::TurnComplete | TailEvent::TurnAborted => return CodexStatus::Done,
            TailEvent::AgentActivity => continue,
        }
    }
    CodexStatus::Done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(line: &str) -> LineClass {
        parse_rollout_line(line).class()
    }

    fn parsed(line: &str) -> CodexParsed {
        match parse_rollout_line(line) {
            CodexLineOutcome::Parsed(p) => *p,
            other => panic!("expected a parsed line, got {other:?}"),
        }
    }

    /// The canary. Every shape here is inference, so an unknown kind is the
    /// single most likely thing to meet in the wild — it must be loud.
    #[test]
    fn an_unheard_of_kind_is_flagged_not_dropped() {
        match parse_rollout_line(r#"{"type":"quantum_entanglement","payload":{}}"#) {
            CodexLineOutcome::Unknown { kind } => assert_eq!(kind, "quantum_entanglement"),
            other => panic!("a future kind must be flagged, got {other:?}"),
        }
    }

    /// Nested drift is as loud as outer drift, and names where it happened.
    #[test]
    fn an_unknown_item_kind_is_flagged_with_a_compound_name() {
        let l = r#"{"type":"response_item","payload":{"type":"telepathy","x":1}}"#;
        match parse_rollout_line(l) {
            CodexLineOutcome::Unknown { kind } => assert_eq!(kind, "response_item/telepathy"),
            other => panic!("nested drift must be flagged, got {other:?}"),
        }
    }

    #[test]
    fn known_bookkeeping_is_ignored_quietly() {
        assert_eq!(class(""), LineClass::Ignored);
        for kind in KNOWN_IGNORED {
            let line = format!(r#"{{"type":"{kind}"}}"#);
            assert_eq!(class(&line), LineClass::Ignored, "{kind} should be ignored");
        }
    }

    #[test]
    fn unreadable_lines_are_malformed_not_silently_dropped() {
        assert_eq!(class("not json"), LineClass::Malformed);
        assert_eq!(class("{ truncated"), LineClass::Malformed);
        assert_eq!(class(r#"{"payload":{}}"#), LineClass::Malformed);
    }

    #[test]
    fn a_payloadless_envelope_is_barren() {
        match parse_rollout_line(r#"{"type":"response_item"}"#) {
            CodexLineOutcome::Barren { kind } => assert_eq!(kind, "response_item"),
            other => panic!("expected barren, got {other:?}"),
        }
    }

    #[test]
    fn session_meta_yields_cwd_version_and_branch() {
        let l = r#"{"timestamp":"2026-07-29T10:00:00.000Z","type":"session_meta","payload":{"id":"u","cwd":"/w","cli_version":"0.145.0","git":{"branch":"main","commit_hash":"abc"}}}"#;
        let p = parsed(l);
        assert_eq!(p.cwd.as_deref(), Some("/w"));
        assert_eq!(p.cli_version.as_deref(), Some("0.145.0"));
        assert_eq!(p.git_branch.as_deref(), Some("main"));
        assert!(p.ts.is_some());
    }

    #[test]
    fn detached_head_is_not_a_branch() {
        let l = r#"{"type":"session_meta","payload":{"cwd":"/w","git":{"branch":"HEAD"}}}"#;
        assert!(parsed(l).git_branch.is_none());
    }

    #[test]
    fn turn_context_yields_cwd_and_model() {
        let l = r#"{"type":"turn_context","payload":{"cwd":"/w","model":"gpt-5.2-codex","approval_policy":"on-request"}}"#;
        let p = parsed(l);
        assert_eq!(p.cwd.as_deref(), Some("/w"));
        assert_eq!(p.model.as_deref(), Some("gpt-5.2-codex"));
    }

    #[test]
    fn a_user_message_is_a_turn() {
        let l = r#"{"type":"event_msg","payload":{"type":"user_message","message":"do the thing"}}"#;
        let p = parsed(l);
        assert!(p.is_user_turn);
        assert_eq!(p.text.as_deref(), Some("do the thing"));
        assert_eq!(p.tail, Some(TailEvent::UserMessage));
    }

    #[test]
    fn usage_uses_codex_field_names_not_claudes() {
        let l = r#"{"type":"event_msg","payload":{"type":"turn_complete","usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20,"reasoning_output_tokens":5}}}"#;
        let p = parsed(l);
        let u = p.usage.expect("usage should parse");
        assert_eq!(u.total_in(), 140);
        assert_eq!(u.total_out(), 25);
        // Claude's name for the cache field must not be picked up here.
        let l2 = r#"{"type":"event_msg","payload":{"type":"turn_complete","usage":{"cache_read_input_tokens":40}}}"#;
        assert!(parsed(l2).usage.is_none() || parsed(l2).usage.unwrap().cached_input_tokens == 0);
    }

    #[test]
    fn shell_calls_become_one_line_activity() {
        let l = r#"{"type":"response_item","payload":{"type":"local_shell_call","command":["bash","-lc","ls\n-la"]}}"#;
        let p = parsed(l);
        let a = p.last_activity.unwrap();
        assert!(!a.contains('\n'), "activity leaked a newline: {a:?}");
        assert!(a.starts_with("shell: bash -lc ls"));
        assert_eq!(p.tool_calls, 1);
    }

    #[test]
    fn approval_requests_are_tail_events() {
        let l = r#"{"type":"event_msg","payload":{"type":"exec_approval_request","command":"rm -rf build"}}"#;
        let p = parsed(l);
        assert_eq!(p.tail, Some(TailEvent::ApprovalRequest));
        assert!(p.last_activity.unwrap().contains("rm -rf build"));
    }

    #[test]
    fn status_heuristic_from_tails() {
        use TailEvent::*;
        // Trailing approval request with no later turn event: waiting.
        assert_eq!(
            derive_status(&[UserMessage, TurnStarted, AgentActivity, ApprovalRequest]),
            CodexStatus::Waiting
        );
        // Last turn event is a start: working, even through trailing chatter.
        assert_eq!(
            derive_status(&[TurnStarted, AgentActivity, AgentActivity]),
            CodexStatus::Working
        );
        // A prompt with no turn started yet is already working.
        assert_eq!(
            derive_status(&[TurnComplete, UserMessage]),
            CodexStatus::Working
        );
        // Completed or aborted: done.
        assert_eq!(
            derive_status(&[TurnStarted, AgentActivity, TurnComplete]),
            CodexStatus::Done
        );
        assert_eq!(derive_status(&[TurnStarted, TurnAborted]), CodexStatus::Done);
        // An approval that was answered (turn moved on) is not waiting.
        assert_eq!(
            derive_status(&[ApprovalRequest, TurnComplete]),
            CodexStatus::Done
        );
        // No events at all: nothing is running.
        assert_eq!(derive_status(&[]), CodexStatus::Done);
    }

    #[test]
    fn rollout_counts_add_up() {
        let mut r = CodexRollout::default();
        for l in [
            r#"{"type":"session_meta","payload":{"cwd":"/w"}}"#,
            r#"{"type":"ghost_snapshot","payload":{}}"#,
            r#"{"type":"response_item"}"#,
            r#"{"type":"warp_drive","payload":{}}"#,
            "not json",
        ] {
            r.absorb_line(l);
        }
        assert_eq!(r.counts.parsed, 1);
        assert_eq!(r.counts.ignored, 1);
        assert_eq!(r.counts.barren, 1);
        assert_eq!(r.counts.unknown, 1);
        assert_eq!(r.counts.malformed, 1);
        assert_eq!(r.counts.lines_seen(), 5);
        assert_eq!(r.counts.unknown_kinds, vec![("warp_drive".to_string(), 1)]);
    }

    #[test]
    fn state_db_glob_prefers_the_newest_version() {
        let dir = std::env::temp_dir().join(format!("mogeung-codex-glob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state_3.sqlite"), b"x").unwrap();
        std::fs::write(dir.join("state_12.sqlite"), b"x").unwrap();
        std::fs::write(dir.join("state_5.sqlite"), b"x").unwrap();
        std::fs::write(dir.join("goals_1.sqlite"), b"x").unwrap();
        let install = CodexInstall::discover(&dir).unwrap();
        let db = install.state_db().unwrap();
        assert_eq!(db.file_name().unwrap(), "state_12.sqlite");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_install_is_none_not_an_error() {
        assert!(CodexInstall::discover(Path::new("/nonexistent/definitely/not")).is_none());
    }

    #[test]
    fn an_unreadable_rollout_is_flagged_not_panicked() {
        let r = read_rollout(Path::new("/nonexistent/rollout-x.jsonl"));
        assert!(r.error.is_some());
        assert_eq!(r.counts.lines_seen(), 0);
    }
    // ---------------------------------------------------------------------
    // Codex 0.149 — the taxonomy rename. `R-J70`.
    //
    // Shapes copied from a real rollout and cut down to the fields that carry
    // something. Written out rather than pointing at a corpus file because
    // these are the *new* names, and a test that would have passed before the
    // rename proves nothing.
    // ---------------------------------------------------------------------

    /// `turn_started` became `task_started`; the turn boundary must still be
    /// found, or `derive_status` answers `Done` for a session mid-turn and the
    /// board calls a working agent finished.
    #[test]
    fn the_renamed_turn_boundary_is_still_a_turn() {
        let started = r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"t1","started_at":1787763756}}"#;
        assert_eq!(parsed(started).tail, Some(TailEvent::TurnStarted));

        let done = r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t1","last_agent_message":"Ready—what would you like to test?","duration_ms":2237}}"#;
        let p = parsed(done);
        assert_eq!(p.tail, Some(TailEvent::TurnComplete));
        assert_eq!(
            p.text.as_deref(),
            Some("Ready—what would you like to test?"),
            "the reply rides on the completion event"
        );

        assert_eq!(derive_status(&[TailEvent::TurnStarted]), CodexStatus::Working);
        assert_eq!(
            derive_status(&[TailEvent::TurnStarted, TailEvent::TurnComplete]),
            CodexStatus::Done
        );
    }

    /// Usage moved out of the turn-complete event into its own. Before this,
    /// a Codex session reported every token as input and none as output.
    #[test]
    fn usage_arrives_as_its_own_event_now() {
        let l = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":13601,"cached_input_tokens":9984,"output_tokens":13,"reasoning_output_tokens":0,"total_tokens":13614}}}}"#;
        let u = parsed(l).usage.expect("token_count carries usage");
        assert_eq!(u.input_tokens, 13601);
        assert_eq!(u.output_tokens, 13);
        assert_eq!(u.cached_input_tokens, 9984);
    }

    /// Messages are wrapped in a completion envelope. The *user* one is what
    /// counts a turn — and it is read from here rather than from
    /// `response_item`, which is tested below for why.
    #[test]
    fn a_wrapped_user_message_is_the_turn_that_counts() {
        let l = r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th","turn_id":"t1","item":{"type":"UserMessage","id":"i1","content":[{"type":"text","text":"Testing"}]}}}"#;
        let p = parsed(l);
        assert!(p.is_user_turn, "a wrapped user message is still a turn");
        assert_eq!(p.text.as_deref(), Some("Testing"));
        assert_eq!(p.tail, Some(TailEvent::UserMessage));

        let a = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","content":[{"type":"text","text":"Ready"}]}}}"#;
        let p = parsed(a);
        assert!(!p.is_user_turn, "the agent's reply is not a turn you took");
        assert_eq!(p.text.as_deref(), Some("Ready"));
    }

    /// **The double-count this avoids.** `response_item/message` replays the
    /// model-facing transcript, including an `<environment_context>` blob under
    /// `role: "user"`. Counting turns there would count every turn twice and
    /// would show a system preamble as the last thing you typed.
    #[test]
    fn the_model_facing_transcript_is_activity_only() {
        let env = r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>\n<cwd>/home/kinz</cwd>\n</environment_context>"}]}}"#;
        let p = parsed(env);
        assert!(!p.is_user_turn, "a replayed preamble is not a prompt");
        assert_eq!(p.text, None, "and must not become `last_prompt`");
        assert_eq!(p.tail, Some(TailEvent::AgentActivity));

        let dev = r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<skills_instructions>..."}]}}"#;
        assert!(!parsed(dev).is_user_turn);
    }

    /// The third level of the taxonomy must drift as loudly as the first two,
    /// or a tool call becomes silence. `KNOWN_COMPLETED_ITEMS` is deliberately
    /// short, so this is the shape most users will meet first.
    #[test]
    fn an_unknown_completed_item_names_all_three_levels() {
        let l = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":"ls"}}}"#;
        match parse_rollout_line(l) {
            CodexLineOutcome::Unknown { kind } => {
                assert_eq!(kind, "event_msg/item_completed/CommandExecution")
            }
            other => panic!("third-level drift must be flagged, got {other:?}"),
        }
    }

    /// A live thread is the lock file that names it; `.coordination.lock` is
    /// not a thread, and an install with no lock directory must report
    /// `supported: false` so callers fall back instead of concluding the
    /// machine is empty.
    #[test]
    fn live_threads_come_from_the_writer_locks() {
        let home = std::env::temp_dir().join(format!("mogeung-codex-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();

        let none = scan_live(&home);
        assert!(!none.supported, "no lock directory means we cannot say");
        assert!(none.threads.is_empty());

        let dir = home.join("thread-writer-locks");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".coordination.lock"), "").unwrap();
        std::fs::write(dir.join("01a03ed7-1217-7530-90b7-ddd5e7255b5d.lock"), "").unwrap();

        let live = scan_live(&home);
        assert!(live.supported, "the directory exists, so the answer is real");
        assert!(
            !live.threads.contains_key(".coordination"),
            "the coordination lock names no thread"
        );
        // On Linux an unheld lock file is correctly *not* live; elsewhere
        // presence is all we have. Both are asserted through the same door.
        let held = std::fs::read_to_string("/proc/locks").is_ok();
        assert_eq!(
            live.threads.contains_key("01a03ed7-1217-7530-90b7-ddd5e7255b5d"),
            !held,
            "a lock nobody holds is a ghost where the kernel will say so"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

}
