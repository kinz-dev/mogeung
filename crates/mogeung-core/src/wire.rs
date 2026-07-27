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
    /// List one directory of the session's worktree. `path` is relative to the
    /// session root (repo root when known, else cwd); empty means the root.
    ///
    /// Read-only, like everything here. The explorer (`R-B24`) is a viewer;
    /// the roadmap's "an editor — explicitly not" still stands.
    ListDir { session_id: SessionId, path: String },
    /// Read one file of the session's worktree, same path rules as `ListDir`.
    FetchFile { session_id: SessionId, path: String },
    /// Every file path under the session root in one answer, for go-to-file.
    /// Capped and gitignore-aware; still read-only (`R-B25`).
    ListTree { session_id: SessionId },
    /// Find lines containing `query` (a literal, not a regex) across the
    /// session's worktree. Case-insensitive unless the query has an uppercase
    /// letter — ripgrep's smart case, because that is what the hands expect.
    SearchContent { session_id: SessionId, query: String },
    /// A page of the session repo's commit log, newest first. `R-D10`.
    ///
    /// Like everything git here: read-only, permanently. The daemon never
    /// stages, commits, or checks out — the observer rule, one layer down.
    GitLog { session_id: SessionId, skip: u32, limit: u32 },
    /// One commit's diff, parsed into the same file/hunk shapes as `Change`.
    GitShow { session_id: SessionId, sha: String },
    /// The repo's uncommitted state — staged, unstaged and untracked.
    GitStatus { session_id: SessionId },
    /// One uncommitted file's diff against `HEAD`.
    GitDiffFile { session_id: SessionId, path: String },
    /// Per-line authorship of one worktree file, for the Editor's annotate
    /// gutter.
    GitBlame { session_id: SessionId, path: String },
}

/// One entry of a [`ClientMsg::ListDir`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

/// One commit of a [`ClientMsg::GitLog`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short: String,
    pub author: String,
    /// Unix seconds of the author date.
    pub epoch: i64,
    /// The subject line only; bodies stay in the terminal.
    pub summary: String,
}

/// One entry of a [`ClientMsg::GitStatus`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEntry {
    /// Repo-relative, `/`-joined.
    pub path: String,
    /// Something for this path sits in the index.
    pub staged: bool,
    /// The worktree differs from the index (or the file is untracked).
    pub unstaged: bool,
    /// The raw porcelain `XY` code, for display: `M `, ` M`, `??`, `A `…
    pub state: String,
}

/// One line's authorship in a [`ClientMsg::GitBlame`] answer, in file order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    /// Abbreviated; all zeros means not yet committed.
    pub sha: String,
    pub author: String,
    pub epoch: i64,
}

/// One matching line of a [`ClientMsg::SearchContent`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMatch {
    /// Relative to the session root, `/`-joined — a wire identifier, the same
    /// currency `ListDir` and `FetchFile` deal in.
    pub path: String,
    /// 1-based, as every editor counts.
    pub line: u64,
    /// The matching line, clipped — a minified bundle is not a search result.
    pub text: String,
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
    /// One directory of a session's worktree, dirs first then files, both
    /// sorted by name. `.git` is never included.
    DirListing {
        session_id: SessionId,
        path: String,
        entries: Vec<DirEntry>,
    },
    /// One worktree file. `truncated` means the file went on past the size cap
    /// and the client is looking at the head of it, not all of it.
    FileContent {
        session_id: SessionId,
        path: String,
        content: String,
        truncated: bool,
    },
    /// Every file of a session's worktree, sorted, `.git` never included and
    /// gitignore respected when the root is a repo. `truncated` means the walk
    /// hit its cap and this is a prefix of the tree, not the tree.
    TreeListing {
        session_id: SessionId,
        paths: Vec<String>,
        truncated: bool,
    },
    /// The matches for one [`ClientMsg::SearchContent`] query. The query is
    /// echoed back so a client can drop the answer to a search it has since
    /// abandoned — same shape as the stray-session rule.
    ContentMatches {
        session_id: SessionId,
        query: String,
        matches: Vec<ContentMatch>,
        truncated: bool,
    },
    /// A page of commits. `done` means history ended inside this page.
    GitCommits {
        session_id: SessionId,
        skip: u32,
        commits: Vec<CommitInfo>,
        done: bool,
    },
    /// One commit's diff, in the same shapes the Changes tab renders.
    GitCommitDiff {
        session_id: SessionId,
        sha: String,
        files: Vec<crate::change::FileChange>,
    },
    /// The uncommitted state of the session's repo.
    GitLocalChanges {
        session_id: SessionId,
        entries: Vec<StatusEntry>,
    },
    /// One uncommitted file against `HEAD`.
    GitFileDiff {
        session_id: SessionId,
        path: String,
        files: Vec<crate::change::FileChange>,
    },
    /// Authorship per line of one file. `truncated` means the file went on
    /// past the blame cap.
    GitAnnotation {
        session_id: SessionId,
        path: String,
        lines: Vec<BlameLine>,
        truncated: bool,
    },
    Error { message: String },
}
