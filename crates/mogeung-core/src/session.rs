use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Claude Code's own session id. mogeung does not mint identifiers any more —
/// it observes sessions you started yourself, so their identity is theirs.
pub type SessionId = String;

/// First-party liveness, read from `~/.claude/sessions/<pid>.json`.
///
/// This is the single most valuable thing the observer model buys: the CLI
/// tells us whether a session is working or waiting, so "needs you" stops being
/// a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveStatus {
    /// Actively working.
    Busy,
    /// Alive but waiting — almost always waiting for you to type something.
    Idle,
    /// Registry reported something we do not model yet.
    Unknown,
}

impl LiveStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "busy" => LiveStatus::Busy,
            "idle" => LiveStatus::Idle,
            _ => LiveStatus::Unknown,
        }
    }
}

/// An agent session mogeung is watching.
///
/// Everything here is derived from files Claude Code writes for its own
/// purposes. mogeung never starts, steers or stops a session — that is what
/// made v0.1 feel like a worse terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,

    /// Claude Code's generated title for the conversation, when it has made one.
    pub title: Option<String>,
    /// Human-friendly name from the live registry, e.g. "mogeung-95".
    pub name: Option<String>,
    /// Most recent thing you typed.
    pub last_prompt: Option<String>,

    pub cwd: String,
    /// Enclosing git repository, if the cwd is inside one.
    pub repo_root: Option<String>,
    pub git_branch: Option<String>,

    /// Process id while alive. `None` once the session has exited.
    pub pid: Option<u32>,
    pub alive: bool,
    pub live_status: Option<LiveStatus>,
    /// CLI version that wrote the transcript.
    pub version: Option<String>,

    pub started_at: DateTime<Utc>,
    /// Last append to the transcript. Drives stall detection.
    pub last_event_at: DateTime<Utc>,
    /// When the live registry last changed its mind about `live_status`.
    /// Used to measure how long a session has been waiting for you.
    pub status_since: Option<DateTime<Utc>>,

    pub turns: u32,
    pub tool_calls: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub last_activity: Option<String>,

    /// Files this session touched, from Edit/Write tool calls and file-history
    /// records. Lets a diff be attributed to one session even when several
    /// share a working tree.
    pub touched_files: Vec<String>,

    /// Commit the repo was on when mogeung first saw this session. Diffs are
    /// computed against it when known.
    pub base_sha: Option<String>,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,

    pub error: Option<String>,

    /// Absolute path of the `.jsonl` transcript.
    pub transcript_path: String,
    /// True once every hunk in this session's diff has been read.
    pub reviewed: bool,
}

impl Session {
    pub fn seconds_since_activity(&self, now: DateTime<Utc>) -> i64 {
        (now - self.last_event_at).num_seconds().max(0)
    }

    pub fn duration_secs(&self, now: DateTime<Utc>) -> i64 {
        let end = if self.alive { now } else { self.last_event_at };
        (end - self.started_at).num_seconds().max(0)
    }

    /// How long this session has been waiting for you, if it is.
    pub fn waiting_secs(&self, now: DateTime<Utc>) -> Option<i64> {
        if self.alive && self.live_status == Some(LiveStatus::Idle) {
            let since = self.status_since.unwrap_or(self.last_event_at);
            Some((now - since).num_seconds().max(0))
        } else {
            None
        }
    }

    pub fn has_diff(&self) -> bool {
        self.files_changed > 0
    }

    /// Best available label, in descending order of usefulness.
    pub fn label(&self) -> String {
        self.title
            .clone()
            .or_else(|| self.last_prompt.clone())
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| format!("session {}", &self.id[..8.min(self.id.len())]))
    }

    pub fn repo_name(&self) -> String {
        let p = self.repo_root.as_deref().unwrap_or(&self.cwd);
        p.rsplit('/').next().unwrap_or(p).to_string()
    }
}
