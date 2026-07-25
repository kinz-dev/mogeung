use crate::attention::AttentionItem;
use crate::change::Change;
use crate::health::Health;
use crate::review::{BlastRadius, ReviewDebt};
use crate::session::{Session, SessionId};
use crate::transcript::TranscriptEvent;
use serde::{Deserialize, Serialize};

/// What a client asks the daemon to do.
///
/// Note what is absent: nothing here starts, steers or stops an agent. mogeung
/// observes sessions you run yourself. The only thing it launches is a real
/// interactive `claude` in your own terminal, which is the opposite of wrapping
/// it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Send the full current state. Sent by the client on (re)connect.
    Subscribe,
    /// Mark or unmark one hunk as read.
    SetHunkReviewed {
        session_id: SessionId,
        anchor: String,
        reviewed: bool,
    },
    /// Mark every hunk currently in the diff as read.
    ReviewAll { session_id: SessionId },
    /// Recompute the diff for a session from disk.
    RefreshChange { session_id: SessionId },
    /// Replay stored transcript events so a fresh client can fill in history.
    FetchEvents { session_id: SessionId, since: u64 },
    /// Stop tracking a session and forget its review state.
    ForgetSession { session_id: SessionId },
    /// Open a terminal running a real interactive `claude` in `dir`.
    ///
    /// This is how mogeung helps you reach three or four parallel sessions
    /// without owning the conversation loop.
    LaunchTerminal {
        dir: String,
        /// Create a fresh git worktree first, so the new session is isolated.
        worktree: bool,
    },
    /// Rescan for sessions immediately instead of waiting for the next poll.
    Rescan,
    /// Ask what mogeung can and cannot currently see.
    FetchHealth,
    /// Silence a session for `minutes`. Zero or less clears the snooze.
    Snooze { session_id: SessionId, minutes: i64 },
    /// How much of a repo's agent output nobody has read.
    FetchReviewDebt { repo: String },
    /// What else references the symbols this file's diff changed.
    FetchBlastRadius { session_id: SessionId, path: String },
    /// Focus the terminal a live session is running in.
    ///
    /// Still not steering the agent: it moves *your* window, and then you type.
    FocusTerminal { session_id: SessionId },
}

/// What the daemon tells clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Full state, sent on connect.
    Snapshot {
        sessions: Vec<Session>,
        queue: Vec<AttentionItem>,
    },
    SessionUpdated { session: Box<Session> },
    SessionRemoved { session_id: SessionId },
    Events { events: Vec<TranscriptEvent> },
    /// Recomputed whenever ranking could change, including on a timer so that
    /// "waiting for you" ages correctly with no new events.
    Queue { queue: Vec<AttentionItem> },
    ChangeUpdated {
        session_id: SessionId,
        change: Change,
    },
    /// Sent after every scan. A client should never have to ask whether the
    /// board it is showing is complete — the daemon volunteers it.
    Health { health: Box<Health> },
    ReviewDebt { debt: Box<ReviewDebt> },
    BlastRadius { radius: Box<BlastRadius> },
    Error { message: String },
}
