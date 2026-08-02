//! Client-side state for the Git pane. `R-D10`, deepened by `R-D11`.
//!
//! A cache, not an authority — commits, status, refs, stashes, diffs and
//! blame all come from the daemon over the wire, and nothing here (or
//! there) can write to a repository. The pane is scoped to the selected
//! session: switching sessions drops the cache, because git state is cheap
//! to re-ask for and a stale log is worse than a moment of "loading".

use mogeung_core::change::FileChange;
use mogeung_core::wire::{
    BlameLine, CommitDetail, CommitInfo, ReflogEntry, RefsInfo, StashInfo, StatusEntry,
    SubmoduleInfo, WorktreeInfo,
};
use mogeung_core::SessionId;
use std::collections::{HashMap, HashSet};

/// What the right-hand diff shows.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Selection {
    #[default]
    None,
    /// A commit from the log, by full sha.
    Commit(String),
    /// An uncommitted file from local changes, by repo-relative path.
    Local(String),
    /// A stash, by its position in the list.
    Stash(u32),
    /// The diff between two marked commits, oldest first as diffed.
    Range(String, String),
    /// A conflicted file's three stages, by repo-relative path. `R-D16`.
    Conflict(String),
}

#[derive(Default)]
pub struct GitView {
    /// The session the cache belongs to; everything below dies with it.
    pub session: Option<SessionId>,
    pub commits: Vec<CommitInfo>,
    /// History ended inside what we have — no "show more" to offer.
    pub log_done: bool,
    pub log_pending: bool,
    /// The ref the log is scoped to; `None` is HEAD. Changing it goes
    /// through [`GitView::set_log_rev`], which restarts the log.
    pub log_rev: Option<String>,
    /// The filter narrowing the log (`R-D12`); all three set through
    /// [`GitView::set_log_filter`], which restarts the log. A set `path`
    /// makes the log a file's history (the daemon follows renames).
    pub log_grep: Option<String>,
    pub log_author: Option<String>,
    pub log_path: Option<String>,
    /// Pickaxe (`find:`): commits that changed this literal. `R-D13`.
    pub log_pickaxe: Option<String>,
    /// Show only commits wearing the session-attribution dot. Display
    /// filter over loaded pages; the graph hides while it is on, because
    /// the topology of a subset would be a lie.
    pub attribution_only: bool,
    /// What the filter box currently reads — the uncommitted keystrokes;
    /// the parsed, active filter is the three fields above.
    pub filter_input: String,
    /// Graph lanes for `commits`, recomputed whenever the log changes.
    pub graph: Vec<GraphRow>,
    pub status: Vec<StatusEntry>,
    /// Which rows are ticked for a write verb. `R-D19`.
    ///
    /// Separate from `selection`, which decides what the diff pane shows.
    /// Clicking a row to read it and ticking it to act on it are different
    /// intentions, and a list where reading something arms a Discard button
    /// is a list nobody can use quickly.
    pub checked: std::collections::BTreeSet<String>,
    /// Files a confirmed Discard would destroy, while the confirmation is up.
    /// `None` when nothing is being asked. `R-D19`.
    pub confirm_discard: Option<Vec<String>>,
    /// The commit message being written. `R-D20`.
    ///
    /// Kept here rather than in `prefs`: a half-typed message is not a
    /// setting, and one restored on a later launch would be a sentence about
    /// work that has since been committed by other means.
    pub commit_msg: String,
    /// Replace the tip commit rather than adding one.
    pub commit_amend: bool,
    /// The last fetch's result, while the popup is showing it. `R-D25`.
    pub fetched: Option<Fetched>,
    /// A fetch is in flight. The button says so and refuses to start a second
    /// one — a network call behind a keystroke is a keystroke people repeat.
    pub fetching: bool,
    /// A branch a Switch is waiting to be confirmed for. `R-D21`.
    pub confirm_switch: Option<String>,
    /// A stash index a Drop is waiting to be confirmed for. `R-D21`.
    pub confirm_stash_drop: Option<u32>,
    /// The new-branch name being typed, when the field is open. `R-D21`.
    pub new_branch: Option<String>,
    /// Record which session the work came from, for `R-F2`.
    ///
    /// On by default, and a visible checkbox rather than a setting, because it
    /// is the reason committing from here beats committing from a terminal —
    /// off by default it would be a feature nobody finds. Visible because the
    /// trailer becomes part of the commit message: it is a UUID and carries no
    /// prompt text, but it is permanent, and anyone who reads the commit sees
    /// it. That is a choice to put in front of a hand, not in a preferences
    /// dialog.
    pub commit_trailer: bool,
    pub status_pending: bool,
    /// Set once the first status answer lands, so an empty repo reads as
    /// "clean" instead of "loading" forever.
    pub status_loaded: bool,
    pub selection: Selection,
    /// sha → its parsed diff.
    pub commit_diffs: HashMap<String, Vec<FileChange>>,
    /// sha → its header, when the daemon could produce one. `R-D12`.
    pub commit_details: HashMap<String, CommitDetail>,
    /// path → its uncommitted diff against HEAD.
    pub local_diffs: HashMap<String, Vec<FileChange>>,
    pub pending_shows: HashSet<String>,
    pub pending_file_diffs: HashSet<String>,
    /// (path, revision — `""` is the worktree) → (per-line authorship,
    /// truncated). Read by the Editor's annotate gutter; the revision key
    /// is what lets a revision tab blame its own era.
    pub blame: HashMap<(String, String), (Vec<BlameLine>, bool)>,
    pub pending_blame: HashSet<(String, String)>,
    /// Show only files this session is believed to have touched.
    pub session_only: bool,
    /// Branches, tags, remotes, HEAD — the pane header and branch list.
    pub refs: Option<RefsInfo>,
    pub refs_pending: bool,
    pub stashes: Vec<StashInfo>,
    pub stashes_loaded: bool,
    pub stashes_pending: bool,
    /// stash index → its diff.
    pub stash_diffs: HashMap<u32, Vec<FileChange>>,
    pub pending_stash_shows: HashSet<u32>,
    pub submodules: Vec<SubmoduleInfo>,
    pub submodules_loaded: bool,
    pub submodules_pending: bool,
    /// (from, to) → the range diff.
    pub range_diffs: HashMap<(String, String), Vec<FileChange>>,
    pub pending_ranges: HashSet<(String, String)>,
    /// A commit marked as one end of a range diff, waiting for the other.
    pub range_from: Option<String>,
    /// Index into [`CONTEXT_STEPS`] — how much context diffs are cut with.
    pub context_step: usize,
    /// Mute whitespace-only changes in diffs (`-w`).
    pub diff_ignore_ws: bool,
    /// A `GitCompare` is in flight; the next range answer is its result
    /// and becomes the selection.
    pub compare_pending: bool,
    pub reflog: Vec<ReflogEntry>,
    pub reflog_loaded: bool,
    pub reflog_pending: bool,
    pub worktrees: Vec<WorktreeInfo>,
    pub worktrees_loaded: bool,
    pub worktrees_pending: bool,
    /// path → (base, ours, theirs, truncated). `R-D16`.
    pub conflicts: HashMap<String, (String, String, String, bool)>,
    pub pending_conflicts: HashSet<String>,
    /// The hunk `n`/`p` walk through the diff panel: current index and a
    /// pending scroll request consumed by the render.
    pub hunk_cursor: usize,
    pub hunk_goto: Option<usize>,
    /// The file the diff panel is narrowed to (`R-D18`) — `None` shows the
    /// whole diff. Owned by [`GitView::focus_for`], which forgets it when
    /// the selection moves.
    pub focus_file: Option<String>,
    /// The selection `focus_file` belongs to.
    focus_owner: Selection,
}

/// The context widths the ± control cycles through; the last is "all of
/// it" for any file that is not generated output.
pub const CONTEXT_STEPS: [u32; 4] = [3, 10, 30, 400];

/// What a fetch did, as the popup needs it. `R-D25`.
pub struct Fetched {
    pub updates: Vec<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// When it landed, so the popup can dismiss itself rather than needing a
    /// click for something that is only ever good news.
    pub at: std::time::Instant,
}

impl Fetched {
    /// The one-line verdict, which is the sentence people actually read.
    pub fn headline(&self) -> String {
        match (&self.upstream, self.behind, self.ahead) {
            (None, _, _) => "Fetched. This branch tracks nothing.".into(),
            (Some(u), 0, 0) => format!("Fetched. Up to date with {u}."),
            (Some(u), 0, a) => format!("Fetched. {a} ahead of {u}, nothing to catch up on."),
            (Some(u), b, 0) => format!("Fetched. {b} behind {u}."),
            (Some(u), b, a) => format!("Fetched. {a} ahead and {b} behind {u} — diverged."),
        }
    }

    /// Whether the user has anything to do about it.
    pub fn needs_a_merge(&self) -> bool {
        self.behind > 0
    }
}

impl GitView {
    /// Point the cache at `id`, dropping everything when it moves.
    pub fn ensure_session(&mut self, id: &SessionId) {
        if self.session.as_ref() == Some(id) {
            return;
        }
        *self = GitView {
            session: Some(id.clone()),
            // On for each new session rather than derived, and set here
            // because this is the one path that builds a live `GitView` — see
            // the field for why it defaults on. Unticking it is a decision
            // about *this* commit, so it does not follow you to the next
            // session.
            commit_trailer: true,
            ..Default::default()
        };
    }

    /// Drop the answers, keep the selection — the refresh gesture. The pane
    /// re-requests whatever it is missing on the next paint.
    pub fn refresh(&mut self) {
        self.commits.clear();
        self.log_done = false;
        self.log_pending = false;
        self.graph.clear();
        self.status.clear();
        self.status_pending = false;
        self.status_loaded = false;
        self.commit_diffs.clear();
        self.commit_details.clear();
        self.local_diffs.clear();
        self.pending_shows.clear();
        self.pending_file_diffs.clear();
        self.blame.clear();
        self.pending_blame.clear();
        self.refs = None;
        self.refs_pending = false;
        self.stashes.clear();
        self.stashes_loaded = false;
        self.stashes_pending = false;
        self.stash_diffs.clear();
        self.pending_stash_shows.clear();
        self.submodules.clear();
        self.submodules_loaded = false;
        self.submodules_pending = false;
        self.range_diffs.clear();
        self.pending_ranges.clear();
        self.compare_pending = false;
        self.reflog.clear();
        self.reflog_loaded = false;
        self.reflog_pending = false;
        self.worktrees.clear();
        self.worktrees_loaded = false;
        self.worktrees_pending = false;
        self.conflicts.clear();
        self.pending_conflicts.clear();
    }

    /// The current diff context width.
    pub fn context(&self) -> u32 {
        CONTEXT_STEPS[self.context_step % CONTEXT_STEPS.len()]
    }

    /// Change how diffs are cut (`R-D14`). Every cached diff was cut the
    /// old way, so they all go — the selection stays and refetches.
    pub fn set_diff_opts(&mut self, context_step: usize, ignore_ws: bool) {
        if (self.context_step, self.diff_ignore_ws) == (context_step, ignore_ws) {
            return;
        }
        self.context_step = context_step % CONTEXT_STEPS.len();
        self.diff_ignore_ws = ignore_ws;
        self.commit_diffs.clear();
        self.local_diffs.clear();
        self.stash_diffs.clear();
        self.range_diffs.clear();
        self.pending_shows.clear();
        self.pending_file_diffs.clear();
        self.pending_stash_shows.clear();
        self.pending_ranges.clear();
    }

    /// Do a diff answer's echoed options match what the pane wants now?
    /// `None` means "asked with defaults" — a compare answer, or an old
    /// daemon — and is accepted only when the pane is at defaults too.
    fn opts_match(&self, context: Option<u32>, ignore_ws: Option<bool>) -> bool {
        context.unwrap_or(3) == self.context()
            && ignore_ws.unwrap_or(false) == self.diff_ignore_ws
    }

    /// Scope the log to a ref (`None` = HEAD). A no-op when unchanged;
    /// otherwise the log restarts from page zero under the new scope.
    pub fn set_log_rev(&mut self, rev: Option<String>) {
        if self.log_rev == rev {
            return;
        }
        self.log_rev = rev;
        self.restart_log();
    }

    /// Narrow the log (`R-D12`). A no-op when nothing changed; otherwise
    /// the log restarts from page zero under the new filter.
    pub fn set_log_filter(
        &mut self,
        grep: Option<String>,
        author: Option<String>,
        path: Option<String>,
        pickaxe: Option<String>,
    ) {
        if (
            self.log_grep.as_ref(),
            self.log_author.as_ref(),
            self.log_path.as_ref(),
            self.log_pickaxe.as_ref(),
        ) == (grep.as_ref(), author.as_ref(), path.as_ref(), pickaxe.as_ref())
        {
            return;
        }
        self.log_grep = grep;
        self.log_author = author;
        self.log_path = path;
        self.log_pickaxe = pickaxe;
        self.restart_log();
    }

    fn restart_log(&mut self) {
        self.commits.clear();
        self.graph.clear();
        self.log_done = false;
        self.log_pending = false;
    }

    // Strays are dropped by session, the explorer's rule: broadcast answers
    // for a session this pane is not showing are not errors, just not ours.

    pub fn ingest_commits(
        &mut self,
        session_id: &SessionId,
        skip: u32,
        commits: Vec<CommitInfo>,
        done: bool,
        scope: LogScope,
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        // A page for a scope or filter we have since left is a stray — the
        // branch-switch rule, extended to retyped filters.
        if scope.rev != self.log_rev
            || scope.grep != self.log_grep
            || scope.author != self.log_author
            || scope.path != self.log_path
            || scope.pickaxe != self.log_pickaxe
        {
            return;
        }
        self.log_pending = false;
        self.log_done = done;
        // Pages append in order; a page we already hold (a re-ask after
        // refresh raced an old answer) must not duplicate.
        if skip as usize == self.commits.len() {
            self.commits.extend(commits);
        } else if skip == 0 {
            self.commits = commits;
        } else {
            return;
        }
        self.graph = lanes(&self.commits);
    }

    pub fn ingest_status(&mut self, session_id: &SessionId, entries: Vec<StatusEntry>) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.status_pending = false;
        self.status_loaded = true;
        // Ticks follow the paths that still have a row. A write is answered by
        // re-reading the tree (`R-D19`), so this runs after every one of them:
        // a file that was staged, discarded or committed away must not stay
        // ticked and travel into the next click.
        self.checked
            .retain(|p| entries.iter().any(|e| &e.path == p));
        self.status = entries;
    }

    /// The ticked paths that still exist, in the order the pane lists them.
    pub fn checked_paths(&self) -> Vec<String> {
        self.status
            .iter()
            .filter(|e| self.checked.contains(&e.path))
            .map(|e| e.path.clone())
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ingest_commit_diff(
        &mut self,
        session_id: &SessionId,
        sha: String,
        files: Vec<FileChange>,
        detail: Option<CommitDetail>,
        context: Option<u32>,
        ignore_ws: Option<bool>,
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        // The detail is option-independent; keep it even from a stray.
        if let Some(d) = detail {
            self.commit_details.insert(sha.clone(), d);
        }
        if !self.opts_match(context, ignore_ws) {
            return;
        }
        self.pending_shows.remove(&sha);
        self.commit_diffs.insert(sha, files);
    }

    pub fn ingest_file_diff(
        &mut self,
        session_id: &SessionId,
        path: String,
        files: Vec<FileChange>,
        context: Option<u32>,
        ignore_ws: Option<bool>,
    ) {
        if self.session.as_ref() != Some(session_id) || !self.opts_match(context, ignore_ws) {
            return;
        }
        self.pending_file_diffs.remove(&path);
        self.local_diffs.insert(path, files);
    }

    pub fn ingest_blame(
        &mut self,
        session_id: &SessionId,
        path: String,
        lines: Vec<BlameLine>,
        truncated: bool,
        rev: Option<String>,
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        let key = (path, rev.unwrap_or_default());
        self.pending_blame.remove(&key);
        self.blame.insert(key, (lines, truncated));
    }

    pub fn ingest_refs(&mut self, session_id: &SessionId, info: RefsInfo) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.refs_pending = false;
        self.refs = Some(info);
    }

    pub fn ingest_stashes(&mut self, session_id: &SessionId, stashes: Vec<StashInfo>) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.stashes_pending = false;
        self.stashes_loaded = true;
        self.stashes = stashes;
    }

    pub fn ingest_stash_diff(
        &mut self,
        session_id: &SessionId,
        index: u32,
        files: Vec<FileChange>,
        context: Option<u32>,
        ignore_ws: Option<bool>,
    ) {
        if self.session.as_ref() != Some(session_id) || !self.opts_match(context, ignore_ws) {
            return;
        }
        self.pending_stash_shows.remove(&index);
        self.stash_diffs.insert(index, files);
    }

    pub fn ingest_submodules(
        &mut self,
        session_id: &SessionId,
        submodules: Vec<SubmoduleInfo>,
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.submodules_pending = false;
        self.submodules_loaded = true;
        self.submodules = submodules;
    }

    pub fn ingest_range_diff(
        &mut self,
        session_id: &SessionId,
        from: String,
        to: String,
        files: Vec<FileChange>,
        context: Option<u32>,
        ignore_ws: Option<bool>,
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        let key = (from.clone(), to.clone());
        if self.pending_ranges.contains(&key) {
            if !self.opts_match(context, ignore_ws) {
                return;
            }
            self.pending_ranges.remove(&key);
            self.range_diffs.insert(key, files);
        } else if self.compare_pending {
            // A branch compare answers as a range with daemon-resolved
            // shas the client could not have known; adopt it as the
            // selection. `R-D15`.
            self.compare_pending = false;
            self.range_diffs.insert(key, files);
            self.selection = Selection::Range(from, to);
        }
    }

    pub fn ingest_reflog(&mut self, session_id: &SessionId, entries: Vec<ReflogEntry>) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.reflog_pending = false;
        self.reflog_loaded = true;
        self.reflog = entries;
    }

    pub fn ingest_worktrees(&mut self, session_id: &SessionId, worktrees: Vec<WorktreeInfo>) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.worktrees_pending = false;
        self.worktrees_loaded = true;
        self.worktrees = worktrees;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ingest_conflict(
        &mut self,
        session_id: &SessionId,
        path: String,
        base: String,
        ours: String,
        theirs: String,
        truncated: bool,
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.pending_conflicts.remove(&path);
        self.conflicts.insert(path, (base, ours, theirs, truncated));
    }

    /// The ignored path prefixes from the last status answer — `!!` rows,
    /// directories arriving with their trailing `/`. The explorer dims by
    /// these; local changes never lists them.
    pub fn ignored_prefixes(&self) -> Vec<&str> {
        self.status
            .iter()
            .filter(|e| e.state == "!!")
            .map(|e| e.path.as_str())
            .collect()
    }
}

/// The scope a log page was answered under — compared against the pane's
/// current scope to drop strays.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LogScope {
    pub rev: Option<String>,
    pub grep: Option<String>,
    pub author: Option<String>,
    pub path: Option<String>,
    pub pickaxe: Option<String>,
}

/// Parse the filter box: `author:`, `path:` and `find:` tokens pull out,
/// everything else joins into the message filter. Order-free, quotes not
/// needed. `find:` is the pickaxe — commits that changed how often the
/// text occurs (`R-D13`).
pub fn parse_filter_input(
    input: &str,
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let mut grep_words: Vec<&str> = Vec::new();
    let mut author = None;
    let mut path = None;
    let mut pickaxe = None;
    for tok in input.split_whitespace() {
        if let Some(a) = tok.strip_prefix("author:") {
            if !a.is_empty() {
                author = Some(a.to_string());
            }
        } else if let Some(p) = tok.strip_prefix("path:") {
            if !p.is_empty() {
                path = Some(p.to_string());
            }
        } else if let Some(x) = tok.strip_prefix("find:") {
            if !x.is_empty() {
                pickaxe = Some(x.to_string());
            }
        } else {
            grep_words.push(tok);
        }
    }
    let grep = if grep_words.is_empty() {
        None
    } else {
        Some(grep_words.join(" "))
    };
    (grep, author, path, pickaxe)
}

/// Rebuild unified-diff text from the cached shapes — what "Copy as
/// patch" puts on the clipboard. The hunks kept their raw lines and
/// headers, so this round-trips what git said, minus the index lines a
/// patch does not need. `R-D13`.
pub fn patch_text(files: &[FileChange]) -> String {
    let mut out = String::new();
    for f in files {
        let old = f.old_path.as_deref().unwrap_or(&f.path);
        out.push_str(&format!("diff --git a/{} b/{}
", old, f.path));
        match f.status {
            mogeung_core::change::FileStatus::Added => {
                out.push_str(&format!("--- /dev/null
+++ b/{}
", f.path));
            }
            mogeung_core::change::FileStatus::Deleted => {
                out.push_str(&format!("--- a/{}
+++ /dev/null
", old));
            }
            _ => {
                out.push_str(&format!("--- a/{}
+++ b/{}
", old, f.path));
            }
        }
        for h in &f.hunks {
            out.push_str(&h.header);
            out.push('\n');
            for line in &h.lines {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

impl GitView {
    /// The diff panel's file focus for `sel`, forgetting it the moment the
    /// selection moves — a file picked on one commit must not silently
    /// narrow the next. `R-D18`.
    pub fn focus_for(&mut self, sel: &Selection) -> Option<String> {
        if self.focus_owner != *sel {
            self.focus_owner = sel.clone();
            self.focus_file = None;
        }
        self.focus_file.clone()
    }
}

/// One row of the commit file tree (`R-D18`): a directory holding
/// children, or a leaf holding its index into the diff's file list.
#[derive(Debug, PartialEq, Eq)]
pub struct TreeNode {
    /// A path segment — or several joined with `/` where a chain of
    /// single-child directories was flattened, IntelliJ-style.
    pub label: String,
    pub children: Vec<TreeNode>,
    /// `Some(index into files)` for a leaf; `None` for a directory.
    pub file: Option<usize>,
}

/// A diff's files as a directory tree: directories first, each level
/// sorted, single-child directory chains flattened to one `a/b/c` row.
pub fn file_tree(files: &[FileChange]) -> Vec<TreeNode> {
    fn insert(nodes: &mut Vec<TreeNode>, parts: &[&str], idx: usize) {
        let (head, rest) = match parts.split_first() {
            Some(x) => x,
            None => return,
        };
        if rest.is_empty() {
            nodes.push(TreeNode { label: head.to_string(), children: Vec::new(), file: Some(idx) });
            return;
        }
        let dir = match nodes.iter_mut().find(|n| n.file.is_none() && n.label == *head) {
            Some(d) => d,
            None => {
                nodes.push(TreeNode {
                    label: head.to_string(),
                    children: Vec::new(),
                    file: None,
                });
                nodes.last_mut().unwrap()
            }
        };
        insert(&mut dir.children, rest, idx);
    }
    fn tidy(nodes: &mut Vec<TreeNode>) {
        for n in nodes.iter_mut() {
            tidy(&mut n.children);
            // Flatten `a` → `b` → … while each holds exactly one directory.
            while n.file.is_none()
                && n.children.len() == 1
                && n.children[0].file.is_none()
            {
                let child = n.children.pop().unwrap();
                n.label = format!("{}/{}", n.label, child.label);
                n.children = child.children;
            }
        }
        nodes.sort_by(|a, b| {
            a.file.is_some().cmp(&b.file.is_some()).then(a.label.cmp(&b.label))
        });
    }
    let mut roots = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let parts: Vec<&str> = f.path.split('/').filter(|s| !s.is_empty()).collect();
        insert(&mut roots, &parts, i);
    }
    tidy(&mut roots);
    roots
}

/// One `n`/`p` step through a diff read file-at-a-time (`R-D18`):
/// `counts` is hunks per file, the position is (file, hunk within it).
/// Walks within the file, crosses into the next/previous file with hunks
/// at the edges, wraps at the ends — IntelliJ's behavior. `None` when no
/// file has any hunks.
pub fn step_hunk(
    counts: &[usize],
    file: usize,
    hunk: usize,
    forward: bool,
) -> Option<(usize, usize)> {
    if counts.iter().all(|&c| c == 0) {
        return None;
    }
    let file = file.min(counts.len() - 1);
    if forward {
        if hunk + 1 < counts[file] {
            return Some((file, hunk + 1));
        }
        let mut f = file;
        loop {
            f = (f + 1) % counts.len();
            if counts[f] > 0 {
                return Some((f, 0));
            }
        }
    } else {
        if hunk > 0 && counts[file] > 0 {
            return Some((file, (hunk - 1).min(counts[file] - 1)));
        }
        let mut f = file;
        loop {
            f = (f + counts.len() - 1) % counts.len();
            if counts[f] > 0 {
                return Some((f, counts[f] - 1));
            }
        }
    }
}

/// Is `path` (repo-relative, no trailing slash) under any ignored prefix?
pub fn is_ignored(path: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|p| {
        let p = p.trim_end_matches('/');
        path == p || path.strip_prefix(p).is_some_and(|r| r.starts_with('/'))
    })
}

// ---------------------------------------------------------------------------
// The graph column
// ---------------------------------------------------------------------------

/// One log row's shape in the graph column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// The column this commit's dot sits in.
    pub lane: usize,
    /// Which lanes are occupied on this row — the vertical continuations.
    pub occupied: Vec<bool>,
    /// Lanes this commit's extra parents flow out to (a merge fans out).
    pub merges: Vec<usize>,
    /// Lanes that were waiting for this same commit and end here (branches
    /// joining back in).
    pub joins: Vec<usize>,
}

/// Assign lanes top-down. Each active lane holds the sha it expects next;
/// a commit lands in the first lane expecting it (or a free one), passes
/// its first parent down its own lane, and routes extra parents to new or
/// existing lanes. Straight lines and dots — deliberately no curve-fitting.
pub fn lanes(commits: &[CommitInfo]) -> Vec<GraphRow> {
    let mut active: Vec<Option<String>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for c in commits {
        let lane = match active
            .iter()
            .position(|s| s.as_deref() == Some(c.short.as_str()))
        {
            Some(i) => i,
            None => match active.iter().position(|s| s.is_none()) {
                Some(i) => {
                    active[i] = Some(c.short.clone());
                    i
                }
                None => {
                    active.push(Some(c.short.clone()));
                    active.len() - 1
                }
            },
        };
        // Other lanes waiting for this same commit converge here.
        let joins: Vec<usize> = (0..active.len())
            .filter(|&j| j != lane && active[j].as_deref() == Some(c.short.as_str()))
            .collect();
        for &j in &joins {
            active[j] = None;
        }
        let occupied: Vec<bool> = active.iter().map(|s| s.is_some()).collect();
        // First parent continues this lane; the rest fan out.
        let mut parents = c.parents.iter();
        active[lane] = parents.next().cloned();
        let mut merges = Vec::new();
        for p in parents {
            if let Some(j) = active
                .iter()
                .position(|s| s.as_deref() == Some(p.as_str()))
            {
                merges.push(j);
            } else if let Some(j) = active.iter().position(|s| s.is_none()) {
                active[j] = Some(p.clone());
                merges.push(j);
            } else {
                active.push(Some(p.clone()));
                merges.push(active.len() - 1);
            }
        }
        while active.last().is_some_and(|s| s.is_none()) {
            active.pop();
        }
        rows.push(GraphRow {
            lane,
            occupied,
            merges,
            joins,
        });
    }
    rows
}

// ---------------------------------------------------------------------------
// Small pure helpers the pane leans on
// ---------------------------------------------------------------------------

/// Relative age for a commit row: coarse on purpose — a log is read by
/// shape ("yesterday-ish"), not by the minute.
pub fn age(now_epoch: i64, epoch: i64) -> String {
    let s = (now_epoch - epoch).max(0);
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        86_400..=2_591_999 => format!("{}d", s / 86_400),
        _ => format!("{}mo", s / 2_592_000),
    }
}

/// The web page for a commit, derived from a remote URL — the hosts we can
/// recognise, and honesty (`None`) for the rest. Never fetched by us; this
/// only feeds "open in browser".
pub fn commit_url(remote: &str, sha: &str) -> Option<String> {
    let (host, path) = if let Some(rest) = remote.strip_prefix("git@") {
        rest.split_once(':')?
    } else if let Some(rest) = remote.strip_prefix("ssh://git@") {
        rest.split_once('/')?
    } else if let Some(rest) = remote
        .strip_prefix("https://")
        .or_else(|| remote.strip_prefix("http://"))
    {
        rest.split_once('/')?
    } else {
        return None;
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if host.is_empty() || !path.contains('/') || path.is_empty() {
        return None;
    }
    let tail = if host.contains("gitlab") {
        format!("{path}/-/commit/{sha}")
    } else if host.contains("bitbucket") {
        format!("{path}/commits/{sha}")
    } else {
        // GitHub, gitea, forgejo, and most self-hosted forges agree.
        format!("{path}/commit/{sha}")
    };
    Some(format!("https://{host}/{tail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str) -> CommitInfo {
        commit_with_parents(sha, &[])
    }

    fn commit_with_parents(sha: &str, parents: &[&str]) -> CommitInfo {
        CommitInfo {
            sha: sha.into(),
            short: sha.chars().take(3).collect(),
            author: "a".into(),
            epoch: 0,
            summary: "s".into(),
            refs: Vec::new(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            touches_session: false,
        }
    }

    /// The explorer's stray rule, applied here: an answer for the session
    /// you just left must not land in the one you are looking at.
    #[test]
    fn stray_answers_for_another_session_are_dropped() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"b".to_string(), 0, vec![commit("x")], true, LogScope::default());
        assert!(g.commits.is_empty(), "session b's log landed in session a");
        g.ingest_blame(&"b".to_string(), "f.rs".into(), vec![], false, None);
        assert!(g.blame.is_empty());
        g.ingest_refs(&"b".to_string(), RefsInfo::default());
        assert!(g.refs.is_none());
        g.ingest_stashes(&"b".to_string(), vec![]);
        assert!(!g.stashes_loaded);
        g.ingest_submodules(&"b".to_string(), vec![]);
        assert!(!g.submodules_loaded);
        g.ingest_range_diff(&"b".to_string(), "x".into(), "y".into(), vec![], None, None);
        assert!(g.range_diffs.is_empty());
    }

    #[test]
    fn switching_sessions_drops_the_cache_and_staying_keeps_it() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("x")], true, LogScope::default());
        g.selection = Selection::Commit("x".into());
        g.ensure_session(&"b".to_string());
        assert!(g.commits.is_empty(), "session a's log survived into b");
        assert_eq!(g.selection, Selection::None);
        g.ingest_commits(&"b".to_string(), 0, vec![commit("y")], true, LogScope::default());
        g.ensure_session(&"b".to_string());
        assert_eq!(g.commits.len(), 1, "re-ensuring the same session wiped it");
    }

    /// Paging appends exactly once: the next page extends, a duplicate or
    /// out-of-order page is dropped, and page zero replaces.
    #[test]
    fn log_pages_append_in_order_and_duplicates_are_dropped() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("1"), commit("2")], false, LogScope::default());
        g.ingest_commits(&"a".to_string(), 2, vec![commit("3")], true, LogScope::default());
        assert_eq!(g.commits.len(), 3);
        assert!(g.log_done);
        // A stale second answer for an offset we are past: dropped.
        g.ingest_commits(&"a".to_string(), 2, vec![commit("3")], true, LogScope::default());
        assert_eq!(g.commits.len(), 3, "a repeated page duplicated the log");
        // Page zero is a refresh: replace, not append.
        g.ingest_commits(&"a".to_string(), 0, vec![commit("9")], true, LogScope::default());
        assert_eq!(g.commits.len(), 1);
    }

    /// Scoping the log to a branch restarts it, and a page from the old
    /// scope arriving late must not land in the new one.
    #[test]
    fn changing_the_log_scope_restarts_the_log_and_drops_strays() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("1")], true, LogScope::default());
        g.set_log_rev(Some("fix/x".into()));
        assert!(g.commits.is_empty(), "the old scope's log survived");
        // The stray: an answer for the HEAD scope after switching to fix/x.
        g.ingest_commits(&"a".to_string(), 0, vec![commit("2")], true, LogScope::default());
        assert!(g.commits.is_empty(), "a stray scope's page landed");
        g.ingest_commits(
            &"a".to_string(),
            0,
            vec![commit("3")],
            true,
            LogScope {
                rev: Some("fix/x".into()),
                ..Default::default()
            },
        );
        assert_eq!(g.commits.len(), 1);
        // Same scope again: nothing moves.
        g.set_log_rev(Some("fix/x".into()));
        assert_eq!(g.commits.len(), 1);
    }

    /// The filter box's little grammar: prefixes pull out, order is free,
    /// the rest is message text.
    #[test]
    fn filter_input_parses_prefixes_in_any_order() {
        assert_eq!(
            parse_filter_input("fix parser author:keith path:src/git.rs find:retry"),
            (
                Some("fix parser".into()),
                Some("keith".into()),
                Some("src/git.rs".into()),
                Some("retry".into())
            )
        );
        assert_eq!(
            parse_filter_input("path:a.rs still a grep"),
            (Some("still a grep".into()), None, Some("a.rs".into()), None)
        );
        assert_eq!(parse_filter_input("   "), (None, None, None, None));
        assert_eq!(
            parse_filter_input("author: find:"),
            (None, None, None, None),
            "an empty prefix is no filter, not an empty one"
        );
    }

    /// Retyping the filter restarts the log, and a page answered under the
    /// old filter must not land in the new one.
    #[test]
    fn changing_the_filter_restarts_the_log_and_drops_strays() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("1")], true, LogScope::default());
        g.set_log_filter(Some("fix".into()), None, None, None);
        assert!(g.commits.is_empty(), "the unfiltered log survived");
        // The stray: an unfiltered page arriving after the filter was set.
        g.ingest_commits(&"a".to_string(), 0, vec![commit("2")], true, LogScope::default());
        assert!(g.commits.is_empty(), "a stray unfiltered page landed");
        g.ingest_commits(
            &"a".to_string(),
            0,
            vec![commit("3")],
            true,
            LogScope {
                grep: Some("fix".into()),
                ..Default::default()
            },
        );
        assert_eq!(g.commits.len(), 1);
        // The same filter again is a no-op, not a restart.
        g.set_log_filter(Some("fix".into()), None, None, None);
        assert_eq!(g.commits.len(), 1);
    }

    /// A commit's header rides its diff and is cached by sha; a diff
    /// without a header (the detail fetch failed) still lands.
    #[test]
    fn commit_details_ride_the_diff_and_absence_is_fine() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        let detail = CommitDetail {
            author: "keith".into(),
            message: "feat: x\n\nwhy".into(),
            ..Default::default()
        };
        g.ingest_commit_diff(&"a".to_string(), "abc".into(), vec![], Some(detail), None, None);
        g.ingest_commit_diff(&"a".to_string(), "def".into(), vec![], None, None, None);
        assert_eq!(g.commit_details["abc"].author, "keith");
        assert!(!g.commit_details.contains_key("def"));
        assert!(g.commit_diffs.contains_key("def"), "no header must not mean no diff");
    }

    /// Refresh forgets answers but not the user's place.
    #[test]
    fn refresh_clears_answers_but_keeps_selection() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("x")], true, LogScope::default());
        g.selection = Selection::Commit("x".into());
        g.ingest_refs(&"a".to_string(), RefsInfo::default());
        g.ingest_stashes(&"a".to_string(), vec![]);
        g.refresh();
        assert!(g.commits.is_empty());
        assert_eq!(g.selection, Selection::Commit("x".into()));
        assert!(!g.status_loaded);
        assert!(g.refs.is_none());
        assert!(!g.stashes_loaded);
    }

    /// Blame is cached per (path, revision): the worktree's gutter and a
    /// revision tab's gutter must never collide.
    #[test]
    fn blame_is_keyed_by_path_and_revision() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        let line = |author: &str| BlameLine {
            sha: "aaa".into(),
            author: author.into(),
            epoch: 0,
            summary: String::new(),
        };
        g.ingest_blame(&"a".to_string(), "f.rs".into(), vec![line("now")], false, None);
        g.ingest_blame(
            &"a".to_string(),
            "f.rs".into(),
            vec![line("then")],
            false,
            Some("abc123^".into()),
        );
        assert_eq!(g.blame.len(), 2);
        assert_eq!(
            g.blame[&("f.rs".to_string(), String::new())].0[0].author,
            "now"
        );
        assert_eq!(
            g.blame[&("f.rs".to_string(), "abc123^".to_string())].0[0].author,
            "then"
        );
    }

    #[test]
    fn ages_are_coarse_and_never_negative() {
        assert_eq!(age(1000, 1000), "now");
        assert_eq!(age(1000, 940), "1m");
        assert_eq!(age(90_000, 0), "1d");
        assert_eq!(age(0, 5000), "now", "clock skew must not print negative ages");
    }

    /// A linear history is a single straight lane.
    #[test]
    fn linear_history_stays_in_lane_zero() {
        let commits = vec![
            commit_with_parents("aaa1", &["bbb"]),
            commit_with_parents("bbb1", &["ccc"]),
            commit_with_parents("ccc1", &[]),
        ];
        let rows = lanes(&commits);
        assert!(rows.iter().all(|r| r.lane == 0));
        assert!(rows.iter().all(|r| r.merges.is_empty() && r.joins.is_empty()));
        assert_eq!(rows[0].occupied, vec![true]);
    }

    /// One branch and its merge: the merge fans out to lane 1, the side
    /// branch lives there, and the fork point joins it back.
    #[test]
    fn a_merge_opens_a_lane_and_the_fork_point_closes_it() {
        let commits = vec![
            commit_with_parents("mmm1", &["aaa", "bbb"]),
            commit_with_parents("aaa1", &["rrr"]),
            commit_with_parents("bbb1", &["rrr"]),
            commit_with_parents("rrr1", &[]),
        ];
        let rows = lanes(&commits);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[0].merges, vec![1], "the second parent fans out to lane 1");
        assert_eq!(rows[1].lane, 0);
        assert_eq!(rows[2].lane, 1, "the side branch keeps its own lane");
        assert_eq!(rows[2].occupied, vec![true, true]);
        assert_eq!(rows[3].lane, 0);
        assert_eq!(rows[3].joins, vec![1], "the fork point closes the side lane");
    }

    /// Two independent roots (a squash-merged repo, a graft) may reuse a
    /// freed lane, and must never panic.
    #[test]
    fn disjoint_histories_reuse_freed_lanes() {
        let commits = vec![
            commit_with_parents("aaa1", &[]),
            commit_with_parents("bbb1", &[]),
        ];
        let rows = lanes(&commits);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[1].lane, 0, "a freed lane is reused, not leaked");
    }

    #[test]
    fn ignored_prefixes_dim_their_subtrees() {
        assert!(is_ignored("target", &["target/"]));
        assert!(is_ignored("target/debug/x", &["target/"]));
        assert!(!is_ignored("target2/x", &["target/"]));
        assert!(!is_ignored("src/a.rs", &["target/"]));
        assert!(is_ignored("a/b.log", &["a/b.log"]));
    }

    fn hunk(header: &str, lines: &[&str]) -> mogeung_core::change::Hunk {
        mogeung_core::change::Hunk {
            anchor: String::new(),
            header: header.into(),
            lines: lines.iter().map(|l| l.to_string()).collect(),
            insertions: 0,
            deletions: 0,
            flags: Vec::new(),
            score: 0,
            reviewed: false,
        }
    }

    /// The clipboard patch must round-trip what git said: headers, signs,
    /// context — and /dev/null on the created/deleted ends.
    #[test]
    fn patch_text_rebuilds_a_unified_diff() {
        let mut f = FileChange {
            path: "src/a.rs".into(),
            old_path: None,
            status: mogeung_core::change::FileStatus::Modified,
            insertions: 0,
            deletions: 0,
            hunks: vec![hunk("@@ -1,2 +1,2 @@", &[" fn a() {}", "-old", "+new"])],
            score: 0,
            flags: Vec::new(),
            truncated: false,
        };
        let text = patch_text(std::slice::from_ref(&f));
        assert!(text.starts_with("diff --git a/src/a.rs b/src/a.rs
"));
        assert!(text.contains("--- a/src/a.rs
+++ b/src/a.rs
@@ -1,2 +1,2 @@
"));
        assert!(text.contains(" fn a() {}
-old
+new
"));
        f.status = mogeung_core::change::FileStatus::Added;
        assert!(patch_text(std::slice::from_ref(&f)).contains("--- /dev/null"));
        f.status = mogeung_core::change::FileStatus::Deleted;
        assert!(patch_text(&[f]).contains("+++ /dev/null"));
    }

    /// The commit file tree (`R-D18`): single-child directory chains
    /// flatten to one row, directories sort before files, and no file is
    /// ever lost on the way in.
    #[test]
    fn the_file_tree_flattens_chains_and_loses_nothing() {
        let file = |p: &str| FileChange {
            path: p.into(),
            old_path: None,
            status: mogeung_core::change::FileStatus::Modified,
            insertions: 0,
            deletions: 0,
            hunks: Vec::new(),
            score: 0,
            flags: Vec::new(),
            truncated: false,
        };
        let files = vec![
            file("src/deep/only/child.rs"),
            file("README.md"),
            file("src/a.rs"),
            file("docs/x.md"),
        ];
        let tree = file_tree(&files);
        // Roots: dirs (docs, src) before the root file.
        let labels: Vec<&str> = tree.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["docs", "src", "README.md"]);
        // `src` keeps two children, so it does not flatten; its lone-chain
        // child does: `deep/only` as one row.
        let src = &tree[1];
        assert_eq!(src.children[0].label, "deep/only");
        assert_eq!(src.children[0].children[0].file, Some(0));
        // Every index surfaces exactly once.
        fn leaves(nodes: &[TreeNode], out: &mut Vec<usize>) {
            for n in nodes {
                if let Some(i) = n.file {
                    out.push(i);
                }
                leaves(&n.children, out);
            }
        }
        let mut seen = Vec::new();
        leaves(&tree, &mut seen);
        seen.sort();
        assert_eq!(seen, vec![0, 1, 2, 3]);
    }

    /// `n`/`p` file-at-a-time (`R-D18`): walk within the file, cross into
    /// the next file with hunks at the edge, wrap at the ends, and answer
    /// `None` only when there is nothing to walk.
    #[test]
    fn the_hunk_walk_crosses_files_and_wraps() {
        let counts = [2, 0, 1];
        assert_eq!(step_hunk(&counts, 0, 0, true), Some((0, 1)), "within the file first");
        assert_eq!(step_hunk(&counts, 0, 1, true), Some((2, 0)), "skips the hunkless file");
        assert_eq!(step_hunk(&counts, 2, 0, true), Some((0, 0)), "wraps forward");
        assert_eq!(step_hunk(&counts, 0, 0, false), Some((2, 0)), "wraps backward");
        assert_eq!(step_hunk(&counts, 2, 0, false), Some((0, 1)), "enters at the last hunk");
        assert_eq!(step_hunk(&[0, 0], 0, 0, true), None, "nothing to walk");
    }

    /// A file focus belongs to the selection it was made on; moving the
    /// selection forgets it. `R-D18`.
    #[test]
    fn a_file_focus_dies_with_its_selection() {
        let mut g = GitView::default();
        let commit_a = Selection::Commit("a".into());
        g.focus_for(&commit_a);
        g.focus_file = Some("src/a.rs".into());
        assert_eq!(g.focus_for(&commit_a), Some("src/a.rs".into()), "stable on the same commit");
        assert_eq!(g.focus_for(&Selection::Commit("b".into())), None, "gone on another");
    }

    /// Changing the diff options invalidates every cached diff and drops
    /// answers cut the old way.
    #[test]
    fn diff_option_changes_drop_stale_answers() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.pending_shows.insert("abc".into());
        g.ingest_commit_diff(&"a".to_string(), "abc".into(), vec![], None, None, None);
        assert!(g.commit_diffs.contains_key("abc"), "defaults must match defaults");
        g.set_diff_opts(1, true); // ±10, -w
        assert!(g.commit_diffs.is_empty(), "old-cut diffs must not survive");
        // An answer cut with the old options arrives late: dropped.
        g.pending_shows.insert("abc".into());
        g.ingest_commit_diff(&"a".to_string(), "abc".into(), vec![], None, Some(3), Some(false));
        assert!(g.commit_diffs.is_empty(), "a stale-cut answer landed");
        g.ingest_commit_diff(&"a".to_string(), "abc".into(), vec![], None, Some(10), Some(true));
        assert!(g.commit_diffs.contains_key("abc"));
    }

    /// A compare's answer becomes the selection; an unrelated range answer
    /// does not.
    #[test]
    fn a_compare_answer_is_adopted_as_the_selection() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_range_diff(&"a".to_string(), "base".into(), "tip".into(), vec![], None, None);
        assert_eq!(g.selection, Selection::None, "unsolicited ranges must not steal the pane");
        g.compare_pending = true;
        g.ingest_range_diff(&"a".to_string(), "base".into(), "tip".into(), vec![], None, None);
        assert_eq!(
            g.selection,
            Selection::Range("base".into(), "tip".into()),
            "the compare's answer is the selection"
        );
        assert!(!g.compare_pending);
    }

    /// The hosts we can recognise get a commit URL; everything else is an
    /// honest `None` rather than a guessed 404.
    #[test]
    fn commit_urls_for_common_hosts() {
        assert_eq!(
            commit_url("git@github.com:kinz/mogeung.git", "abc").as_deref(),
            Some("https://github.com/kinz/mogeung/commit/abc")
        );
        assert_eq!(
            commit_url("https://github.com/kinz/mogeung", "abc").as_deref(),
            Some("https://github.com/kinz/mogeung/commit/abc")
        );
        assert_eq!(
            commit_url("https://gitlab.com/g/p.git", "abc").as_deref(),
            Some("https://gitlab.com/g/p/-/commit/abc")
        );
        assert_eq!(
            commit_url("ssh://git@bitbucket.org/t/r.git", "abc").as_deref(),
            Some("https://bitbucket.org/t/r/commits/abc")
        );
        assert_eq!(commit_url("/local/path/repo", "abc"), None);
        assert_eq!(commit_url("git@github.com:noslash", "abc"), None);
    }
}
