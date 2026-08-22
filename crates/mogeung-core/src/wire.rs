use crate::attention::AttentionItem;
use crate::change::Change;
use crate::health::Health;
use crate::review::{BlastRadius, ReviewDebt};
use crate::session::{Session, SessionId};
use crate::transcript::TranscriptEvent;
use crate::usage::UsageReport;
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
    /// Show a session's folder in the desktop's file manager — Finder on
    /// macOS, whatever `xdg-open` resolves to elsewhere. `R-J34`.
    ///
    /// A **handoff**, which is the one thing pillar K positively asks for:
    /// mogeung reads the worktree and never writes it, so the moment you want
    /// to *do* something with a file the answer is another application. It
    /// runs on the daemon's machine, like `FocusTerminal` and for the same
    /// reason — that is where the folder is.
    OpenFolder { session_id: SessionId },
    /// The folders added to this session's workspace by hand. `R-J40`.
    FetchWorkspace { session_id: SessionId },
    /// Add a folder to this session's workspace, so the tree and the file
    /// surface can see it. `R-J40`.
    ///
    /// **Gated with the repository writes**, and not because it writes a
    /// repository — it does not. It widens what this daemon will *read out*,
    /// and "an open socket must not be able to extend the read surface" is the
    /// same rule [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md)
    /// already applies to the verbs that reach beyond a session's own root.
    AddWorkspaceDir { session_id: SessionId, path: String },
    /// Drop a folder from this session's workspace. Gated for the same reason
    /// — the pair has to live on the same side of the fence.
    RemoveWorkspaceDir { session_id: SessionId, path: String },
    /// List one directory of the session's worktree. `path` is relative to the
    /// session root (repo root when known, else cwd); empty means the root.
    ///
    /// **A path may also be absolute** (`R-J40`), in which case it names
    /// itself and is served only if it falls inside one of the folders this
    /// session's workspace holds. That is one grammar rather than two: a
    /// relative path means the session's own root, as it always has, and an
    /// absolute one is checked against the whitelist the user built.
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
        /// Pickaxe (`-S`): commits that changed the number of occurrences
        /// of this literal string — "when did this appear/vanish". `R-D13`.
        #[serde(default)]
        pickaxe: Option<String>,
    },
    /// One commit's diff, parsed into the same file/hunk shapes as `Change`.
    /// `context` widens the hunks' surrounding lines (default 3);
    /// `ignore_ws` mutes whitespace-only noise (`-w`). `R-D14`.
    GitShow {
        session_id: SessionId,
        sha: String,
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
    },
    /// The repo's uncommitted state — staged, unstaged and untracked.
    GitStatus { session_id: SessionId },

    // -- The write family. `R-D19`.
    //
    // Everything above this line reads. These change the repository, and are
    // the only messages in this protocol that do — see
    // [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md),
    // which permits the working tree and the local repository and refuses
    // every remote verb. There is deliberately no `GitPush`, `GitPull` or
    // `GitFetch`: that is `R-D24`, behind a second ADR that has not been
    // written.
    //
    // They are grouped rather than filed beside their read siblings so that
    // the guard which refuses them can name a contiguous list, and so that
    // adding a fourth is visibly joining a family that has a rule.
    /// Stage the given worktree paths.
    GitStage {
        session_id: SessionId,
        paths: Vec<String>,
    },
    /// Unstage them, leaving the working tree untouched.
    GitUnstage {
        session_id: SessionId,
        paths: Vec<String>,
    },
    /// Throw local changes away. **The one verb here with no undo** — for an
    /// untracked path it means deletion, and git keeps no copy.
    GitDiscard {
        session_id: SessionId,
        paths: Vec<String>,
    },
    /// Commit what is staged. `R-D20`.
    ///
    /// Only what is staged: the checkboxes above are the instruction, and a
    /// commit that swept in unstaged files would make them a suggestion.
    GitCommit {
        session_id: SessionId,
        message: String,
        /// Replace the tip commit instead of adding one.
        #[serde(default)]
        amend: bool,
        /// Record which session's work this was, for `R-F2` prompt-blame.
        /// Optional because a commit made by hand in the pane need not have
        /// come from an agent at all.
        #[serde(default)]
        session_trailer: bool,
    },
    /// Create a branch, and optionally move onto it. `R-D21`.
    GitBranchCreate {
        session_id: SessionId,
        name: String,
        #[serde(default)]
        switch_to: bool,
    },
    /// Move the worktree onto an existing branch. `R-D21`.
    ///
    /// Git refuses a switch that would lose work. What it cannot know is that
    /// an agent may be reading those files right now — the window warns about
    /// that, because it is the only side that knows a session is live.
    GitSwitch {
        session_id: SessionId,
        name: String,
    },
    /// Shelve the working tree. `R-D21`.
    GitStashPush {
        session_id: SessionId,
        #[serde(default)]
        message: String,
        /// Take untracked files along. Off by default, as in git.
        #[serde(default)]
        include_untracked: bool,
    },
    /// Restore a stash and remove it. `R-D21`.
    GitStashPop { session_id: SessionId, index: u32 },
    /// Throw a stash away without restoring it. `R-D21`.
    GitStashDrop { session_id: SessionId, index: u32 },
    /// Update remote-tracking refs. `R-D25`.
    ///
    /// The one message here that reaches a network beyond this machine, and
    /// the only remote verb there is:
    /// [ADR-0014](../../../docs/decisions/0014-fetch-is-not-publishing.md)
    /// admits `fetch` and refuses `pull` and `push`. Never sent on a timer —
    /// a human asks, or it does not happen.
    GitFetch { session_id: SessionId },
    // -- Notes. `R-B35`, pillar L.
    //
    // Not in the write family above: those change a *repository*, and the
    // guard that refuses them is about not letting an open socket run git.
    // These change the daemon's own store, like `SetHunkReviewed` and
    // `SetSignalCommand` already do, and are gated by the token layer along
    // with everything else when the bind is not loopback.
    /// Send every note. Small by nature, so it is not paged.
    NoteList,
    /// Create or update one. An empty `id` mints a new note and the daemon
    /// answers with the id it chose.
    NoteSave {
        #[serde(default)]
        id: String,
        body: String,
        #[serde(default)]
        session_id: Option<SessionId>,
        #[serde(default)]
        seq: Option<u64>,
        #[serde(default)]
        repo: Option<String>,
    },
    NoteDelete { id: String },

    /// Resolve one conflicted file. `R-D22`.
    ///
    /// Whole-file, matching what `R-D16`'s three-way view shows. A resolution
    /// that mixes both sides is editing, which mogeung does not do — you do it
    /// elsewhere and send [`ResolveSide::Mine`] to say it is done.
    GitResolve {
        session_id: SessionId,
        path: String,
        side: ResolveSide,
    },

    /// One uncommitted file's diff against `HEAD`.
    GitDiffFile {
        session_id: SessionId,
        path: String,
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
    },
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
    GitStashShow {
        session_id: SessionId,
        index: u32,
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
    },
    /// Submodule paths and their state.
    GitSubmodules { session_id: SessionId },
    /// The diff between two commits, `from` → `to`.
    GitDiffRange {
        session_id: SessionId,
        from: String,
        to: String,
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
    },
    /// The diff a branch would bring to the current branch, from their
    /// merge base — three-dot semantics, resolved daemon-side. Answers as
    /// [`ServerMsg::GitRangeDiff`] with the resolved shas. `R-D15`.
    GitCompare {
        session_id: SessionId,
        branch: String,
    },
    /// Where HEAD has been — `git reflog`, read-only recovery sight.
    GitReflog { session_id: SessionId },
    /// The repo's worktrees, `git worktree list` — including the ones
    /// mogeung itself created for sessions.
    GitWorktrees { session_id: SessionId },
    /// A conflicted file's three stages — base (`:1:`), ours (`:2:`),
    /// theirs (`:3:`) — for the read-only three-way view. `R-D16`.
    GitConflictFile {
        session_id: SessionId,
        path: String,
    },
    /// One file's content as it stood at one revision, for the Editor's
    /// revision tabs. `sha` may carry a trailing `^` — "the parent of" —
    /// which is what re-blame opens.
    GitFileAtRev {
        session_id: SessionId,
        sha: String,
        path: String,
    },
    /// Token burn per session/day/repo, the trailing five-hour window, and
    /// known limit hits. Computed from the transcripts on demand and cached
    /// incrementally daemon-side. Tokens, never dollars. `R-G1`–`R-G3`.
    FetchUsage,
    /// What this session's repository could be asked to run. `R-N5`.
    FetchRunConfigs { session_id: SessionId },
    /// Start a configuration **by id**. `R-N4`.
    ///
    /// **There is deliberately no field for a command.**
    /// [ADR-0025](../../../docs/decisions/0025-run-a-process-you-named-never-an-agent.md)
    /// clause 1 is the whole security argument for this feature: the daemon
    /// runs what the repository already describes, so reaching the port lets
    /// you run its test suite rather than handing you a shell. Adding a
    /// `command: String` here would quietly undo that.
    RunStart {
        session_id: SessionId,
        config_id: String,
    },
    /// Stop a run this daemon owns, and everything it started.
    RunStop { run_id: String },
    /// The output buffered so far, for a client that just connected.
    FetchRunOutput { run_id: String },
    /// Reveal **one** environment value, deliberately. `R-N6`.
    ///
    /// One at a time and by name, because unmasking is a per-value act: the
    /// sweep behind ADR-0026 found a plaintext API key in a checked-in
    /// configuration, and a verb that returned the whole block would make
    /// "show me this one" indistinguishable from "print every secret".
    RevealRunEnv {
        session_id: SessionId,
        config_id: String,
        key: String,
    },
    /// Configure a repo's signal command (tests/typecheck); empty clears it.
    /// The command is the user's own — mogeung never invents one. `R-E2`.
    SetSignalCommand { repo: String, command: String },
    /// Run the configured signal command for this session's repo. Fires only
    /// on an explicit human action, never automatically — running *checks*
    /// is allowed, re-acquiring a conversation loop is not (ADR-0003).
    RunSignal { session_id: SessionId },
    /// Current signal state for a repo: command, whether a run is in flight,
    /// and the last result.
    FetchSignal { repo: String },
    /// Literal search across every transcript and all prompt history.
    /// `R-F1`. Sent on Enter, not per keystroke — the corpus is tens of MB.
    InsightSearch { query: String },
    /// Every session's measurable activity on one local day (`YYYY-MM-DD`).
    /// Evidence only, never assistant self-reports. `R-F3`.
    FetchDigest { day: String },
    /// Error shapes recurring across sessions. `R-F4`.
    FetchRecurring,
    /// Prompt-history reuse clusters. `R-F6`.
    FetchPromptLibrary,
    /// Prompt-history analytics (sessions/day, hour histogram, per-project).
    /// Token burn per day rides `FetchUsage` instead. `R-F5`.
    FetchAnalytics,
    /// The nested subagent transcripts of one session. `R-F8`.
    FetchSubagents { session_id: SessionId },
    /// Decision-shaped sentences from one session's transcript — candidates
    /// for a human to skim, never authority. `R-F7`.
    FetchDecisions { session_id: SessionId },
    /// Sessions whose edits touched a file, with the prompts that drove
    /// them — the heuristic is named in each answer row. `R-F2`.
    FetchFileSessions { path: String },
    /// A repo's markdown inventory: kinds, staleness, GC proposals, plan
    /// evidence, instruction drift. Proposals only — the daemon never
    /// writes to a watched repo. `R-H1`–`R-H5`.
    FetchDocScan { repo: String },
    /// Every memory and skill under `~/.claude`, without bodies. `R-F14`,
    /// `R-F15`.
    FetchKit,
    /// One memory or skill's text. The daemon refuses a path outside the roots
    /// it published — see `kit::read_doc`, and note that this port has no token
    /// on loopback.
    FetchKitDoc { path: String },
}

/// A folder this session has been seen working in, offered for the workspace
/// and never added by itself. `R-J39`.
///
/// **A suggestion is not a permission.** The daemon widens what it will read
/// only when you widen it, so this carries what was noticed and why, and stops
/// there — the `+` is yours to press.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceHint {
    /// The folder, absolute and real at the moment it was suggested.
    pub path: String,
    /// How it was noticed: `add-dir` when the CLI confirmed a `/add-dir`,
    /// `edits` when the session wrote files there.
    pub source: String,
    /// Files this session wrote inside it — `0` for an `add-dir` record, which
    /// can be offered before a single file has been touched.
    pub files: u32,
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
    /// Branches (local and remote) containing this commit — "has the fix
    /// reached main?". Defaulted so a detail from an older daemon still
    /// parses. `R-D18`.
    #[serde(default)]
    pub branches: Vec<String>,
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
    /// Remote-tracking branches (`origin/…`), as of the last fetch —
    /// scope the log to one like any local branch. `R-D15`.
    #[serde(default)]
    pub remote_branches: Vec<BranchInfo>,
}

/// One line of [`ClientMsg::GitReflog`]'s answer — where HEAD has been.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflogEntry {
    /// Abbreviated sha the entry points at.
    pub sha: String,
    /// The selector, e.g. `HEAD@{3}`.
    pub selector: String,
    /// Freeform action text: "checkout: moving from x to y", …
    pub summary: String,
}

/// One row of [`ClientMsg::GitWorktrees`]'s answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeInfo {
    /// Absolute path of the worktree.
    pub path: String,
    /// Abbreviated HEAD sha there.
    pub sha: String,
    /// Checked-out branch, or `None` when detached.
    pub branch: Option<String>,
}

/// A piece of the user's own writing. `R-B35`, pillar L.
///
/// The only thing this daemon stores that it could not recompute — see
/// [ADR-0015](../../../docs/decisions/0015-markdown-is-the-truth.md), which is
/// also why it is mirrored to disk as markdown.
///
/// `session_id` and `seq` are **tags, not a location**. Together they anchor a
/// note to one turn of one transcript; a note keeps existing when that session
/// ends, is forgotten, or the repository moves. A note with neither is a
/// free-standing document, which is what `R-L2` will be made of.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    /// Markdown. Empty is legal and means a plain bookmark — the mark *is* the
    /// note, so marking a turn and writing about it are one gesture with two
    /// depths rather than two features.
    pub body: String,
    /// Unix seconds.
    pub created: i64,
    pub updated: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Which turn, within that session's transcript.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// Which version of a conflicted file to keep. `R-D22`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveSide {
    /// The version on the branch you are on — git's `--ours`.
    Ours,
    /// The version being merged in — git's `--theirs`.
    Theirs,
    /// Neither: what is on disk is already the answer, resolved by hand
    /// somewhere else, and all that is missing is telling git so.
    Mine,
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

/// Who the daemon is, and which machine's world it is describing (`R-I5`).
///
/// Before this existed a client worked out "am I looking at another machine?"
/// from the address string it had dialled, which is a question about *routing*
/// standing in for a question about *whose filesystem this is*. An
/// `ssh -L 7717:localhost:7717` tunnel makes a remote daemon answer on
/// `127.0.0.1`, and the guess flips to "local" — re-enabling every action that
/// opens an editor or a terminal on a path that exists somewhere else.
///
/// So the daemon says who it is and the client compares, rather than inferring.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonIdentity {
    /// Stable per machine *and* per user: `~/.mogeung/machine-id`, written once
    /// on first run. This is the comparison that decides local-or-remote —
    /// hostnames collide (two boxes both called `localhost`, two fresh VMs off
    /// one image) and a collision here would silently re-enable the actions
    /// this type exists to gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_id: Option<String>,
    /// For people, not for logic: "watching devbox" beats "watching
    /// 10.0.0.4:7717". Never compared — see `machine_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The Claude Code root this daemon watches. Two daemons on one machine
    /// pointed at different homes are genuinely different worlds.
    pub claude_home: String,
    pub pid: u32,
    /// The daemon's build. A client that meets a shape it does not know can say
    /// *which* build to blame rather than guessing.
    pub version: String,
    /// Where to `ssh` to reach this machine, when the daemon has been told
    /// (`ssh_target` in the config file). Declared here so the wire shape is
    /// settled before `R-I6` needs it, since daemon and window upgrade
    /// separately and a field added later is a field half the fleet lacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_target: Option<String>,
}

impl DaemonIdentity {
    /// Whether this daemon is describing the same machine as `local`.
    ///
    /// Unknown on either side means **not the same machine**. Refusing a local
    /// action prints a sentence; performing one against the wrong filesystem
    /// opens an editor on a path that is not there, or a terminal in the wrong
    /// place. The costs are not symmetric, so the doubt resolves the safe way.
    pub fn is_same_machine_as(&self, local: Option<&str>) -> bool {
        match (self.machine_id.as_deref(), local) {
            (Some(theirs), Some(ours)) => theirs == ours,
            _ => false,
        }
    }

    /// What to call this daemon in the UI.
    pub fn label(&self) -> String {
        self.host.clone().unwrap_or_else(|| "unknown host".into())
    }
}

/// What the daemon tells clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Full state, sent on connect.
    Snapshot {
        sessions: Vec<Session>,
        queue: Vec<AttentionItem>,
        /// Who is answering (`R-I5`). `None` from a daemon older than this
        /// field — clients must fall back rather than assume.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        daemon: Option<DaemonIdentity>,
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
    /// A session's workspace: its own root, the folders added to it, and the
    /// ones that have since gone away. `R-J40`.
    ///
    /// `missing` is answered rather than quietly dropped — a folder you added
    /// and then moved should say so, which is `R-J5`'s rule about empty states
    /// applied to a root.
    Workspace {
        session_id: SessionId,
        root: String,
        dirs: Vec<String>,
        missing: Vec<String>,
        /// Folders this session has been seen working in that are not in the
        /// workspace yet. Offered, never added. `R-J39`.
        #[serde(default)]
        hints: Vec<WorkspaceHint>,
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
        #[serde(default)]
        pickaxe: Option<String>,
    },
    /// One commit's diff, in the same shapes the Changes tab renders —
    /// plus its header (`R-D12`), absent when the detail fetch failed.
    /// The diff options echo back so a superseded answer is dropped.
    GitCommitDiff {
        session_id: SessionId,
        sha: String,
        files: Vec<crate::change::FileChange>,
        #[serde(default)]
        detail: Option<Box<CommitDetail>>,
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
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
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
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
    /// What a fetch did. `R-D25`.
    ///
    /// Always sent, including when nothing moved: a sync that succeeds
    /// silently cannot be told from one that silently did nothing, which is
    /// the failure ADR-0014 exists to stop.
    GitFetched {
        session_id: SessionId,
        /// Ref updates, in git's own words, one per line. Empty means the
        /// remotes had nothing new — which is worth saying, not hiding.
        updates: Vec<String>,
        /// The current branch's upstream, when it has one.
        upstream: Option<String>,
        ahead: u32,
        behind: u32,
    },
    /// Every note the daemon holds. `R-B35`.
    ///
    /// Always the whole set, and always after any change, so two windows on
    /// one daemon cannot drift — which is the property daemon ownership was
    /// chosen for.
    Notes { notes: Vec<Note> },
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
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
    },
    /// Submodules and their state.
    GitSubmoduleList {
        session_id: SessionId,
        submodules: Vec<SubmoduleInfo>,
    },
    /// The diff between two commits. [`ClientMsg::GitCompare`] answers
    /// here too, with the merge base as `from`.
    GitRangeDiff {
        session_id: SessionId,
        from: String,
        to: String,
        files: Vec<crate::change::FileChange>,
        #[serde(default)]
        context: Option<u32>,
        #[serde(default)]
        ignore_ws: Option<bool>,
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
    /// Where HEAD has been, newest first.
    GitReflogList {
        session_id: SessionId,
        entries: Vec<ReflogEntry>,
    },
    /// The repo's worktrees.
    GitWorktreeList {
        session_id: SessionId,
        worktrees: Vec<WorktreeInfo>,
    },
    /// A conflicted file's three stages, read-only. A stage a merge did
    /// not produce (added-by-both has no base) arrives empty.
    GitConflictStages {
        session_id: SessionId,
        path: String,
        base: String,
        ours: String,
        theirs: String,
        truncated: bool,
    },
    /// The burn and limit picture. Any warning threshold inside is an
    /// estimate derived from observed limit hits, and clients must label it
    /// as one — the CLI publishes no quota. `R-G1`–`R-G3`.
    UsageStats { report: Box<UsageReport> },
    /// A repo's signal state — sent on fetch, when a run starts, and when it
    /// ends. `R-E2`/`R-E5`.
    SignalStatus {
        repo: String,
        command: Option<String>,
        running: bool,
        last: Option<Box<crate::verify::SignalRun>>,
    },
    /// Search hits, echoing the query so stale answers drop. `R-F1`.
    InsightSearchResults {
        query: String,
        results: Box<crate::insight::SearchResults>,
    },
    /// One day's digest, echoing the day. `R-F3`.
    DayDigestReport {
        day: String,
        digest: Box<crate::insight::DayDigest>,
    },
    /// Recurring failures across sessions. `R-F4`.
    RecurringFailures {
        failures: Vec<crate::insight::RecurringFailure>,
    },
    /// Prompt reuse clusters, most-reused first. `R-F6`.
    PromptLibrary {
        clusters: Vec<crate::insight::PromptCluster>,
    },
    /// Prompt-history analytics. `R-F5`.
    AnalyticsReport {
        analytics: Box<crate::insight::Analytics>,
    },
    /// One session's subagent transcripts. `R-F8`.
    SubagentTreeReport {
        session_id: SessionId,
        nodes: Vec<crate::insight::SubagentNode>,
    },
    /// Decision candidates from one session. `R-F7`.
    DecisionReport {
        session_id: SessionId,
        candidates: Vec<crate::insight::DecisionCandidate>,
    },
    /// Sessions that touched a file, echoing the path. `R-F2`.
    FileSessions {
        path: String,
        entries: Vec<crate::insight::FileSession>,
    },
    /// A repo's doc inventory, echoing the repo. `R-H1`–`R-H5`.
    DocReport {
        repo: String,
        inventory: Box<crate::docs::DocInventory>,
    },
    /// Every memory and skill this machine carries. `R-F14`, `R-F15`.
    Kit { entries: Vec<crate::kit::KitEntry> },
    /// One of them, echoing the path so a late answer cannot be shown against
    /// the wrong file — the same rule the search answers follow.
    KitDoc { doc: crate::kit::KitDoc },
    Error { message: String },
    /// What a session's repository offers, and whether this daemon will run
    /// anything at all. `R-N5`.
    ///
    /// `allowed` is ADR-0025 clause 4 travelling to the client, so the panel
    /// can say *"this daemon was started without `--allow-run`"* instead of
    /// drawing buttons that will be refused.
    RunConfigs {
        session_id: SessionId,
        configs: Vec<crate::run::RunConfig>,
        allowed: bool,
    },
    /// Every run this daemon owns, newest first. Sent on connect, so a window
    /// reopened after a restart still shows what is going.
    Runs { runs: Vec<crate::run::Run> },
    RunStarted { run: Box<crate::run::Run> },
    RunOutput { line: crate::run::RunLine },
    RunEnded { run: Box<crate::run::Run> },
    /// The buffered output of one run, in order.
    RunOutputHistory {
        run_id: String,
        lines: Vec<crate::run::RunLine>,
    },
    /// One environment value, because it was asked for by name. `R-N6`.
    RunEnvValue {
        config_id: String,
        key: String,
        value: String,
    },
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    fn id(machine: &str) -> DaemonIdentity {
        DaemonIdentity {
            machine_id: Some(machine.into()),
            host: Some("devbox".into()),
            claude_home: "/home/dev/.claude".into(),
            pid: 4242,
            version: "0.1.0".into(),
            ssh_target: None,
        }
    }

    /// The bug `R-I5` exists to kill.
    ///
    /// `ssh -N -L 7717:localhost:7717 devbox` is the *recommended* way to reach
    /// a remote daemon, and it makes that daemon answer on `127.0.0.1`. The old
    /// address-substring test read that as "local" and re-enabled every action
    /// that opens an editor or a terminal — on a machine a thousand miles from
    /// the files. Identity does not care what address carried the bytes.
    #[test]
    fn a_tunnelled_daemon_is_still_another_machine() {
        let through_a_tunnel = id("devbox-id");
        assert!(
            !through_a_tunnel.is_same_machine_as(Some("laptop-id")),
            "a tunnel changes the route, not the filesystem"
        );
    }

    #[test]
    fn the_same_machine_is_recognised_whatever_the_route() {
        assert!(id("same").is_same_machine_as(Some("same")));
    }

    /// Two hosts can share a name — two fresh VMs off one image, two boxes both
    /// called `localhost`. If that decided the question, the actions this gates
    /// would come back on silently, which is the failure mode being fixed.
    #[test]
    fn a_shared_hostname_does_not_make_two_machines_one() {
        let a = id("machine-a");
        let mut b = id("machine-b");
        b.host = a.host.clone();
        assert_eq!(a.host, b.host);
        assert!(!a.is_same_machine_as(b.machine_id.as_deref()));
    }

    /// Missing on either side must not read as a match.
    #[test]
    fn an_unknown_identity_is_never_this_machine() {
        let mut unknown = id("x");
        unknown.machine_id = None;
        assert!(!unknown.is_same_machine_as(Some("x")));
        assert!(!id("x").is_same_machine_as(None));
        assert!(!unknown.is_same_machine_as(None));
    }

    /// Daemon and window live on different machines and upgrade at different
    /// times, so both directions of skew have to be non-fatal.
    #[test]
    fn a_snapshot_survives_a_version_gap_in_both_directions() {
        // A new daemon's snapshot, read by a client that knows the field.
        let fresh = ServerMsg::Snapshot {
            sessions: vec![],
            queue: vec![],
            daemon: Some(id("m1")),
        };
        let wire = serde_json::to_string(&fresh).expect("serialise");
        assert!(wire.contains("machine_id"), "identity must reach the wire");

        // An *older* daemon: no `daemon` key at all. Must still parse.
        let older = r#"{"ev":"snapshot","sessions":[],"queue":[]}"#;
        match serde_json::from_str::<ServerMsg>(older).expect("old snapshot must parse") {
            ServerMsg::Snapshot { daemon, .. } => assert!(daemon.is_none()),
            other => panic!("expected a snapshot, got {other:?}"),
        }

        // A *newer* daemon sending a field this build has never heard of: the
        // unknown key is ignored rather than failing the whole message.
        let newer = r#"{"ev":"snapshot","sessions":[],"queue":[],
                        "daemon":{"claude_home":"/h","pid":1,"version":"9.9.9",
                                  "some_future_field":true}}"#;
        match serde_json::from_str::<ServerMsg>(newer).expect("new snapshot must parse") {
            ServerMsg::Snapshot { daemon, .. } => {
                let d = daemon.expect("identity present");
                assert_eq!(d.version, "9.9.9");
                assert!(d.machine_id.is_none(), "absent id stays absent");
            }
            other => panic!("expected a snapshot, got {other:?}"),
        }
    }
}
