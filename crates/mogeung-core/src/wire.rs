use crate::attention::AttentionItem;
use crate::change::Change;
use crate::run::{PermissionMode, Run, RunId};
use crate::transcript::TranscriptEvent;
use serde::{Deserialize, Serialize};

/// What a client asks the daemon to do.
///
/// Commands are fire-and-forget: the resulting state arrives on the event
/// stream like any other change. That keeps the client a pure projection of
/// daemon state and avoids a request/response correlation layer in v0.1.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Send the full current state. Sent by the client on (re)connect.
    Subscribe,
    StartRun(NewRunSpec),
    /// Continue an existing run's agent session with more instructions.
    /// Creates a new child Run that resumes the parent's session.
    FollowUp { run_id: RunId, prompt: String },
    CancelRun { run_id: RunId },
    /// Forget a run. Optionally remove its git worktree too.
    DeleteRun { run_id: RunId, remove_worktree: bool },
    /// Mark or unmark one hunk as read.
    SetHunkReviewed {
        run_id: RunId,
        anchor: String,
        reviewed: bool,
    },
    /// Mark every hunk currently in the diff as read.
    ReviewAll { run_id: RunId },
    /// Recompute the diff for a run from disk.
    RefreshChange { run_id: RunId },
    /// Replay stored transcript events. Lets a client that just connected fill
    /// in history without a second transport.
    FetchEvents { run_id: RunId, since: u64 },
    /// Register a repository with the daemon.
    AddRepo { path: String },
    RemoveRepo { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRunSpec {
    pub repo_path: String,
    pub intent: String,
    pub model: Option<String>,
    pub permission_mode: PermissionMode,
    /// Create a dedicated git worktree instead of running in the main checkout.
    pub worktree: bool,
}

/// What the daemon tells clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Full state, sent on connect.
    Snapshot {
        runs: Vec<Run>,
        repos: Vec<String>,
        queue: Vec<AttentionItem>,
    },
    /// A run was created or changed.
    RunUpdated { run: Run },
    RunDeleted { run_id: RunId },
    /// New transcript events for a run.
    Events { events: Vec<TranscriptEvent> },
    /// The attention queue was recomputed. Sent whenever ranking could change,
    /// including on a timer so that stall detection fires without new events.
    Queue { queue: Vec<AttentionItem> },
    /// A recomputed diff, in response to RefreshChange or a run finishing.
    ChangeUpdated { run_id: RunId, change: Change },
    RepoList { repos: Vec<String> },
    /// Something went wrong handling a command.
    Error { message: String },
}
