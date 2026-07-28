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
    ///
    /// `rev` scopes the log to a branch or ref without checking anything
    /// out; `None` means `HEAD` (`R-D11`). `grep`/`author` narrow it by
    /// literal, case-insensitive text; `path` narrows it to one file *and
    /// follows renames* — which is what makes a filtered log double as
    /// file history (`R-D12`).
    GitLog {
        session_id: SessionId,
        skip: u32,
        limit: u32,
        #[serde(default)]
        rev: Option<String>,
        #[serde(default)]
        grep: Option<String>,
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        path: Option<String>,
    },
    /// One commit's diff, parsed into the same file/hunk shapes as `Change`.
    GitShow { session_id: SessionId, sha: String },
    /// The repo's uncommitted state — staged, unstaged and untracked.
    GitStatus { session_id: SessionId },
    /// One uncommitted file's diff against `HEAD`.
    GitDiffFile { session_id: SessionId, path: String },
    /// Per-line authorship of one file, for the Editor's annotate gutter.
    /// `rev: None` blames the worktree file; `Some(sha)` blames the file as
    /// it stood at that revision — which is what makes re-blame ("who
    /// touched this *before* that commit") possible. `R-D11`.
    GitBlame {
        session_id: SessionId,
        path: String,
        #[serde(default)]
        rev: Option<String>,
    },
    /// The repo's refs in one answer: where HEAD is, local branches with
    /// their tracking state, tags, remotes, and how stale the last fetch
    /// is. Display only — the daemon never fetches; even `git fetch`
    /// writes `.git`. `R-D11`.
    GitRefs { session_id: SessionId },
    /// The stash list. Read-only: listing stashes, never pushing or
    /// popping them.
    GitStashes { session_id: SessionId },
    /// One stash's diff, by its position in the list (`stash@{index}`).
    GitStashShow { session_id: SessionId, index: u32 },
    /// Submodule paths and their state.
    GitSubmodules { session_id: SessionId },
    /// The diff between two commits, `from` → `to`.
    GitDiffRange {
        session_id: SessionId,
        from: String,
        to: String,
    },
    /// One file's content as it stood at one revision, for the Editor's
    /// revision tabs. `sha` may carry a trailing `^` — "the parent of" —
    /// which is what re-blame opens.
    GitFileAtRev {
        session_id: SessionId,
        sha: String,
        path: String,
    },
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
    /// Ref decorations pointing here — branch heads, tags, `HEAD ->` —
    /// exactly as `%D` names them, split on commas.
    #[serde(default)]
    pub refs: Vec<String>,
    /// Abbreviated parent shas, in order. Two or more means a merge; the
    /// graph column is drawn from these.
    #[serde(default)]
    pub parents: Vec<String>,
    /// Heuristic: this commit lands inside the selected session's lifetime
    /// and touches files that session edited. A hint badge, never an
    /// author column — the daemon cannot actually know.
    #[serde(default)]
    pub touches_session: bool,
}

/// Everything about one commit beyond its diff — the header a commercial
/// client shows above the patch. Carried on [`ServerMsg::GitCommitDiff`]
/// as an optional extra, so a detail-fetch failure degrades to "no
/// header", never to "no diff". `R-D12`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommitDetail {
    pub author: String,
    pub committer: String,
    /// Unix seconds of the author date.
    pub epoch: i64,
    /// Unix seconds of the committer date — differs after rebase/amend.
    pub commit_epoch: i64,
    /// Abbreviated parent shas, clickable on the other side.
    pub parents: Vec<String>,
    /// Ref decorations, as `%D` names them.
    pub refs: Vec<String>,
    /// The full message — subject line first, body after. Agents write
    /// long bodies, and the body is often the only honest record of why.
    pub message: String,
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
    /// Ignored paths arrive as `!!` — clients dim them, not list them.
    pub state: String,
    /// An unresolved merge conflict — the one uncommitted state that is
    /// never routine.
    #[serde(default)]
    pub conflicted: bool,
}

/// One line's authorship in a [`ClientMsg::GitBlame`] answer, in file order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameLine {
    /// Abbreviated; all zeros means not yet committed.
    pub sha: String,
    pub author: String,
    pub epoch: i64,
    /// The commit's subject line, for the gutter's hover card.
    #[serde(default)]
    pub summary: String,
}

/// One local branch in a [`ClientMsg::GitRefs`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchInfo {
    pub name: String,
    /// Abbreviated tip sha.
    pub sha: String,
    /// This is the checked-out branch.
    pub current: bool,
    /// The upstream it tracks, e.g. `origin/main`, if any.
    pub upstream: Option<String>,
    /// Commits ahead of / behind that upstream. Zeros when no upstream.
    pub ahead: u32,
    pub behind: u32,
    /// Unix seconds of the tip's committer date.
    pub epoch: i64,
}

/// One tag in a [`ClientMsg::GitRefs`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagInfo {
    pub name: String,
    /// Abbreviated sha of the *commit* the tag lands on — annotated tags
    /// are dereferenced, so clicking a tag always selects a commit.
    pub sha: String,
    pub epoch: i64,
}

/// One remote in a [`ClientMsg::GitRefs`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

/// Everything [`ClientMsg::GitRefs`] answers with.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefsInfo {
    /// The checked-out branch name; `None` means detached HEAD.
    pub head: Option<String>,
    /// Abbreviated sha of HEAD itself.
    pub head_sha: String,
    pub branches: Vec<BranchInfo>,
    pub tags: Vec<TagInfo>,
    pub remotes: Vec<RemoteInfo>,
    /// Unix seconds of the last `git fetch` anyone ran, from
    /// `FETCH_HEAD`'s mtime. `None` when nothing was ever fetched.
    pub fetch_epoch: Option<i64>,
}

/// One stash in a [`ClientMsg::GitStashes`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StashInfo {
    /// Position in the list — the `N` of `stash@{N}`.
    pub index: u32,
    pub message: String,
    pub epoch: i64,
}

/// One submodule in a [`ClientMsg::GitSubmodules`] answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmoduleInfo {
    pub path: String,
    /// Abbreviated recorded sha.
    pub sha: String,
    /// The raw `git submodule status` prefix: ` ` in sync, `+` checked out
    /// at a different commit, `-` not initialised, `U` merge conflicts.
    pub state: String,
    /// The describe suffix, when git offers one.
    pub note: String,
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
    /// The scope and filters echo back exactly as asked, so a client that
    /// has since switched branches — or retyped the filter — can drop the
    /// stray.
    GitCommits {
        session_id: SessionId,
        skip: u32,
        commits: Vec<CommitInfo>,
        done: bool,
        #[serde(default)]
        rev: Option<String>,
        #[serde(default)]
        grep: Option<String>,
        #[serde(default)]
        author: Option<String>,
        #[serde(default)]
        path: Option<String>,
    },
    /// One commit's diff, in the same shapes the Changes tab renders —
    /// plus its header (`R-D12`), absent when the detail fetch failed.
    GitCommitDiff {
        session_id: SessionId,
        sha: String,
        files: Vec<crate::change::FileChange>,
        #[serde(default)]
        detail: Option<Box<CommitDetail>>,
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
    /// past the blame cap. `rev` echoes the revision blamed (`None` = the
    /// worktree), the cache key on the other side.
    GitAnnotation {
        session_id: SessionId,
        path: String,
        lines: Vec<BlameLine>,
        truncated: bool,
        #[serde(default)]
        rev: Option<String>,
    },
    /// The repo's refs, in one shot. `R-D11`.
    GitRefsInfo {
        session_id: SessionId,
        info: Box<RefsInfo>,
    },
    /// The stash list.
    GitStashList {
        session_id: SessionId,
        stashes: Vec<StashInfo>,
    },
    /// One stash's diff, same shapes as every other diff here.
    GitStashDiff {
        session_id: SessionId,
        index: u32,
        files: Vec<crate::change::FileChange>,
    },
    /// Submodules and their state.
    GitSubmoduleList {
        session_id: SessionId,
        submodules: Vec<SubmoduleInfo>,
    },
    /// The diff between two commits.
    GitRangeDiff {
        session_id: SessionId,
        from: String,
        to: String,
        files: Vec<crate::change::FileChange>,
    },
    /// One file as it stood at one revision, for the Editor's revision
    /// tabs. Read-only like the worktree twin, and doubly so — the past
    /// cannot be edited even in the terminal.
    GitFileAtRevContent {
        session_id: SessionId,
        sha: String,
        path: String,
        content: String,
        truncated: bool,
    },
    Error { message: String },
}
