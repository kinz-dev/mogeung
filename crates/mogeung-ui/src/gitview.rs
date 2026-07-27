//! Client-side state for the Git pane. `R-D10`.
//!
//! A cache, not an authority — commits, status, diffs and blame all come
//! from the daemon over the wire, and nothing here (or there) can write to
//! a repository. The pane is scoped to the selected session: switching
//! sessions drops the cache, because git state is cheap to re-ask for and a
//! stale log is worse than a moment of "loading".

use mogeung_core::change::FileChange;
use mogeung_core::wire::{BlameLine, CommitInfo, StatusEntry};
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
}

#[derive(Default)]
pub struct GitView {
    /// The session the cache belongs to; everything below dies with it.
    pub session: Option<SessionId>,
    pub commits: Vec<CommitInfo>,
    /// History ended inside what we have — no "show more" to offer.
    pub log_done: bool,
    pub log_pending: bool,
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
    /// path → (per-line authorship, truncated). Read by the Editor's
    /// annotate gutter.
    pub blame: HashMap<String, (Vec<BlameLine>, bool)>,
    pub pending_blame: HashSet<String>,
    /// Show only files this session is believed to have touched.
    pub session_only: bool,
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
        self.status.clear();
        self.status_pending = false;
        self.status_loaded = false;
        self.commit_diffs.clear();
        self.local_diffs.clear();
        self.pending_shows.clear();
        self.pending_file_diffs.clear();
        self.blame.clear();
        self.pending_blame.clear();
    }

    // Strays are dropped by session, the explorer's rule: broadcast answers
    // for a session this pane is not showing are not errors, just not ours.

    pub fn ingest_commits(
        &mut self,
        session_id: &SessionId,
        skip: u32,
        commits: Vec<CommitInfo>,
        done: bool,
    ) {
        if self.session.as_ref() != Some(session_id) {
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
        }
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
    ) {
        if self.session.as_ref() != Some(session_id) {
            return;
        }
        self.pending_blame.remove(&path);
        self.blame.insert(path, (lines, truncated));
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(sha: &str) -> CommitInfo {
        CommitInfo {
            sha: sha.into(),
            short: sha.chars().take(3).collect(),
            author: "a".into(),
            epoch: 0,
            summary: "s".into(),
        }
    }

    /// The explorer's stray rule, applied here: an answer for the session
    /// you just left must not land in the one you are looking at.
    #[test]
    fn stray_answers_for_another_session_are_dropped() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"b".to_string(), 0, vec![commit("x")], true);
        assert!(g.commits.is_empty(), "session b's log landed in session a");
        g.ingest_blame(&"b".to_string(), "f.rs".into(), vec![], false);
        assert!(g.blame.is_empty());
    }

    #[test]
    fn switching_sessions_drops_the_cache_and_staying_keeps_it() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("x")], true);
        g.selection = Selection::Commit("x".into());
        g.ensure_session(&"b".to_string());
        assert!(g.commits.is_empty(), "session a's log survived into b");
        assert_eq!(g.selection, Selection::None);
        g.ingest_commits(&"b".to_string(), 0, vec![commit("y")], true);
        g.ensure_session(&"b".to_string());
        assert_eq!(g.commits.len(), 1, "re-ensuring the same session wiped it");
    }

    /// Paging appends exactly once: the next page extends, a duplicate or
    /// out-of-order page is dropped, and page zero replaces.
    #[test]
    fn log_pages_append_in_order_and_duplicates_are_dropped() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("1"), commit("2")], false);
        g.ingest_commits(&"a".to_string(), 2, vec![commit("3")], true);
        assert_eq!(g.commits.len(), 3);
        assert!(g.log_done);
        // A stale second answer for an offset we are past: dropped.
        g.ingest_commits(&"a".to_string(), 2, vec![commit("3")], true);
        assert_eq!(g.commits.len(), 3, "a repeated page duplicated the log");
        // Page zero is a refresh: replace, not append.
        g.ingest_commits(&"a".to_string(), 0, vec![commit("9")], true);
        assert_eq!(g.commits.len(), 1);
    }

    /// Refresh forgets answers but not the user's place.
    #[test]
    fn refresh_clears_answers_but_keeps_selection() {
        let mut g = GitView::default();
        g.ensure_session(&"a".to_string());
        g.ingest_commits(&"a".to_string(), 0, vec![commit("x")], true);
        g.selection = Selection::Commit("x".into());
        g.refresh();
        assert!(g.commits.is_empty());
        assert_eq!(g.selection, Selection::Commit("x".into()));
        assert!(!g.status_loaded);
    }

    #[test]
    fn ages_are_coarse_and_never_negative() {
        assert_eq!(age(1000, 1000), "now");
        assert_eq!(age(1000, 940), "1m");
        assert_eq!(age(90_000, 0), "1d");
        assert_eq!(age(0, 5000), "now", "clock skew must not print negative ages");
    }
}
