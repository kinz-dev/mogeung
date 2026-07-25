use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Stable identifier for a run. Doubles as the Claude Code `--session-id`, so a
/// run in mogeung and a session in the agent CLI are the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RunId(pub Uuid);

impl RunId {
    pub fn new() -> Self {
        RunId(Uuid::new_v4())
    }

    /// Short form for UI labels and worktree directory names.
    pub fn short(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for RunId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RunId(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    ClaudeCode,
}

impl AgentKind {
    pub fn label(&self) -> &'static str {
        match self {
            AgentKind::ClaudeCode => "claude",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    AcceptEdits,
    Auto,
    BypassPermissions,
    Manual,
    Plan,
}

impl PermissionMode {
    /// The exact string Claude Code's `--permission-mode` expects.
    pub fn as_cli(&self) -> &'static str {
        match self {
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Auto => "auto",
            PermissionMode::BypassPermissions => "bypassPermissions",
            PermissionMode::Manual => "manual",
            PermissionMode::Plan => "plan",
        }
    }

    pub const ALL: [PermissionMode; 5] = [
        PermissionMode::AcceptEdits,
        PermissionMode::Auto,
        PermissionMode::BypassPermissions,
        PermissionMode::Manual,
        PermissionMode::Plan,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Process spawned, no events seen yet.
    Starting,
    /// Streaming events.
    Running,
    /// Finished successfully. Waiting for a human to look at the diff.
    AwaitingReview,
    /// A human marked every hunk reviewed (or there was nothing to review).
    Reviewed,
    /// Agent exited non-zero, or emitted an error result.
    Failed,
    /// Killed by the user.
    Cancelled,
}

impl RunStatus {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, RunStatus::Starting | RunStatus::Running)
    }

    pub fn label(&self) -> &'static str {
        match self {
            RunStatus::Starting => "starting",
            RunStatus::Running => "running",
            RunStatus::AwaitingReview => "needs review",
            RunStatus::Reviewed => "reviewed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
        }
    }
}

/// One agent session: intent in, evidence out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub id: RunId,
    pub intent: String,
    pub agent: AgentKind,
    pub model: Option<String>,
    pub permission_mode: PermissionMode,

    /// Repository this run belongs to (the main checkout, not the worktree).
    pub repo_path: String,
    /// Where the agent actually ran. Equals `repo_path` when `worktree` is false.
    pub cwd: String,
    /// True when mogeung created a dedicated git worktree for this run.
    pub worktree: bool,
    pub branch: Option<String>,
    /// Commit the worktree started from. All diffs are computed against this.
    pub base_sha: Option<String>,

    /// The agent CLI's own session id, learned from its init event. For a root
    /// run this equals `id`; for a follow-up it is the resumed parent's session.
    pub session_id: Option<String>,

    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    /// Timestamp of the most recent transcript event. Drives stall detection.
    pub last_event_at: DateTime<Utc>,
    /// Name of the tool most recently invoked, for at-a-glance "what is it doing".
    pub last_activity: Option<String>,

    pub cost_usd: f64,
    pub num_turns: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,

    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,

    /// Tools the agent tried to use but was not permitted to. A real "needs you" signal.
    pub permission_denials: u32,
    pub error: Option<String>,

    /// Run this one continued from, if it was a follow-up.
    pub parent: Option<RunId>,
    /// First run in this session chain (equals `id` for a root run).
    ///
    /// Review marks are keyed by `root`, not `id`, so a follow-up never
    /// re-presents code you already read in an earlier turn of the same
    /// session. This is what makes "diff of the diff" work across follow-ups.
    pub root: RunId,
}

impl Run {
    /// Age of the newest activity, in seconds.
    pub fn seconds_since_activity(&self, now: DateTime<Utc>) -> i64 {
        (now - self.last_event_at).num_seconds().max(0)
    }

    pub fn duration_secs(&self, now: DateTime<Utc>) -> i64 {
        let end = self.ended_at.unwrap_or(now);
        (end - self.created_at).num_seconds().max(0)
    }

    pub fn has_diff(&self) -> bool {
        self.files_changed > 0
    }
}
