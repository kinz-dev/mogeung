//! Client-side state for the Git pane. `R-D10`, deepened by `R-D11`.
//!
//! A cache, not an authority — commits, status, refs, stashes, diffs and
//! blame all come from the daemon over the wire, and nothing here (or
//! there) can write to a repository. The pane is scoped to the selected
//! session: switching sessions drops the cache, because git state is cheap
//! to re-ask for and a stale log is worse than a moment of "loading".

use mogeung_core::change::FileChange;
use mogeung_core::wire::{
    BlameLine, CommitInfo, RefsInfo, StashInfo, StatusEntry, SubmoduleInfo,
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
    /// Graph lanes for `commits`, recomputed whenever the log changes.
    pub graph: Vec<GraphRow>,
    pub status: Vec<StatusEntry>,
    pub status_pending: bool,
    /// Set once the first status answer lands, so an empty repo reads as
    /// "clean" instead of "loading" forever.
    pub status_loaded: bool,
    pub selection: Selection,
    /// sha → its parsed diff.
    pub commit_diffs: HashMap<String, Vec<FileChange>>,
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
}

impl GitView {
    /// Point the cache at `id`, dropping everything when it moves.
    pub fn ensure_session(&mut self, id: &SessionId) {
        if self.session.as_ref() == Some(id) {
            return;
        }
        *self = GitView {
            session: Some(id.clone()),
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
    }

    /// Scope the log to a ref (`None` = HEAD). A no-op when unchanged;
    /// otherwise the log restarts from page zero under the new scope.
    pub fn set_log_rev(&mut self, rev: Option<String>) {
        if self.log_rev == rev {
            return;
        }
        self.log_rev = rev;
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
        rev: Option<String>,
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        // A page for a scope we have since left is the branch-switch stray.
        if rev != self.log_rev {
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
        self.status = entries;
    }

    pub fn ingest_commit_diff(
        &mut self,
        session_id: &SessionId,
        sha: String,
        files: Vec<FileChange>,
    ) {
        if self.session.as_ref() != Some(session_id) {
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
    ) {
        if self.session.as_ref() != Some(session_id) {
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
    ) {
        if self.session.as_ref() != Some(session_id) {
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
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        let key = (from, to);
        self.pending_ranges.remove(&key);
        self.range_diffs.insert(key, files);
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
        g.ingest_commits(&"b".to_string(), 0, vec![commit("x")], true, None);
        assert!(g.commits.is_empty(), "session b's log landed in session a");
        g.ingest_blame(&"b".to_string(), "f.rs".into(), vec![], false, None);
        assert!(g.blame.is_empty());
        g.ingest_refs(&"b".to_string(), RefsInfo::default());
        assert!(g.refs.is_none());
        g.ingest_stashes(&"b".to_string(), vec![]);
        assert!(!g.stashes_loaded);
        g.ingest_submodules(&"b".to_string(), vec![]);
        assert!(!g.submodules_loaded);
        g.ingest_range_diff(&"b".to_string(), "x".into(), "y".into(), vec![]);
        assert!(g.range_diffs.is_empty());
    }

    #[test]
    fn switching_sessions_drops_the_cache_and_staying_keeps_it() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("x")], true, None);
        g.selection = Selection::Commit("x".into());
        g.ensure_session(&"b".to_string());
        assert!(g.commits.is_empty(), "session a's log survived into b");
        assert_eq!(g.selection, Selection::None);
        g.ingest_commits(&"b".to_string(), 0, vec![commit("y")], true, None);
        g.ensure_session(&"b".to_string());
        assert_eq!(g.commits.len(), 1, "re-ensuring the same session wiped it");
    }

    /// Paging appends exactly once: the next page extends, a duplicate or
    /// out-of-order page is dropped, and page zero replaces.
    #[test]
    fn log_pages_append_in_order_and_duplicates_are_dropped() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("1"), commit("2")], false, None);
        g.ingest_commits(&"a".to_string(), 2, vec![commit("3")], true, None);
        assert_eq!(g.commits.len(), 3);
        assert!(g.log_done);
        // A stale second answer for an offset we are past: dropped.
        g.ingest_commits(&"a".to_string(), 2, vec![commit("3")], true, None);
        assert_eq!(g.commits.len(), 3, "a repeated page duplicated the log");
        // Page zero is a refresh: replace, not append.
        g.ingest_commits(&"a".to_string(), 0, vec![commit("9")], true, None);
        assert_eq!(g.commits.len(), 1);
    }

    /// Scoping the log to a branch restarts it, and a page from the old
    /// scope arriving late must not land in the new one.
    #[test]
    fn changing_the_log_scope_restarts_the_log_and_drops_strays() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("1")], true, None);
        g.set_log_rev(Some("fix/x".into()));
        assert!(g.commits.is_empty(), "the old scope's log survived");
        // The stray: an answer for the HEAD scope after switching to fix/x.
        g.ingest_commits(&"a".to_string(), 0, vec![commit("2")], true, None);
        assert!(g.commits.is_empty(), "a stray scope's page landed");
        g.ingest_commits(
            &"a".to_string(),
            0,
            vec![commit("3")],
            true,
            Some("fix/x".into()),
        );
        assert_eq!(g.commits.len(), 1);
        // Same scope again: nothing moves.
        g.set_log_rev(Some("fix/x".into()));
        assert_eq!(g.commits.len(), 1);
    }

    /// Refresh forgets answers but not the user's place.
    #[test]
    fn refresh_clears_answers_but_keeps_selection() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("x")], true, None);
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
