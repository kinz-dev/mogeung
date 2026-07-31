//! Client-side state for the file explorer pane. `R-B24`, grown into a
//! workbench by `R-B25`.
//!
//! A cache, not an authority: every listing, file body, tree walk and search
//! answer comes from the daemon over the wire, the same as diffs and
//! transcripts ([ADR-0001]). The explorer is **read-only by design** — the
//! roadmap's "an editor — explicitly not" is load-bearing, and nothing in this
//! module can write to a worktree.
//!
//! What `R-B25` changed: state is **per session and kept**, not wiped on every
//! switch — a review that crosses two sessions must not pay the navigation
//! cost twice. Several files are open at once in tabs, IntelliJ-fashion: one
//! *preview* tab that single-clicks reuse, and pinned tabs that nothing
//! replaces. Shape (which dirs are open, which files, which pins) persists to
//! `~/.mogeung/explorer.json`; bodies never do — they are re-fetched, which is
//! also what keeps a restored tab honest about a file that changed since.
//!
//! Paths are relative to the session root and `/`-joined on every platform,
//! because they are wire identifiers the daemon resolves — not local paths.

use mogeung_core::wire::{ContentMatch, DirEntry};
use mogeung_core::SessionId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// How many tabs may hold a fetched body at once. Tabs beyond this keep their
/// place and lose their bytes — a body is up to 256 KiB, and a long review
/// session must not quietly hoard megabytes of files nobody is looking at.
const LIVE_BODIES: usize = 8;

/// A file body as the daemon sent it.
pub struct FileView {
    pub content: String,
    /// The daemon cut it short at its size cap; this is the head, not the file.
    pub truncated: bool,
}

/// One open file. The tab is the durable thing; the body comes and goes.
pub struct FileTab {
    pub path: String,
    /// `Some(rev)` makes this a *revision* tab: the body is the file as it
    /// stood at that revision (`R-D11`), fetched over the git wire instead
    /// of from the worktree. Revision tabs are never persisted — a sha in
    /// `explorer.json` would outlive its meaning.
    pub rev: Option<String>,
    /// A pinned tab is never reused as the preview slot.
    pub pinned: bool,
    /// Which side of the split this tab lives on: 0 = left, 1 = right.
    /// The split exists exactly when any tab says 1.
    pub group: u8,
    /// `None` while loading, and after eviction — either way the pane asks
    /// the daemon again before showing it.
    pub view: Option<FileView>,
    /// Scroll the viewer to this 1-based line on the next paint. Set by a
    /// search result; consumed by the render.
    pub goto_line: Option<u64>,
    /// Tick of last activation, for the MRU switcher and body eviction.
    last_used: u64,
}

/// One issued content search and what came back.
pub struct SearchState {
    pub query: String,
    pub matches: Vec<ContentMatch>,
    pub truncated: bool,
    /// Sent and not yet answered.
    pub in_flight: bool,
}

/// Everything the explorer knows about one session.
#[derive(Default)]
pub struct SessionExplorer {
    /// Directory (relative to the session root, `""` = the root) → listing.
    pub dirs: HashMap<String, Vec<DirEntry>>,
    /// Directories open in the tree. The pane requests any expanded directory
    /// with no listing, which is what makes restore-from-disk re-fetch free.
    pub expanded: HashSet<String>,
    /// Listings asked for and not yet answered, so a repaint does not re-ask.
    pub pending: HashSet<String>,
    /// File bodies asked for and not yet answered.
    pub pending_files: HashSet<String>,
    /// The open tabs, in strip order, both splits together — a tab's `group`
    /// says which side it renders on.
    pub open: Vec<FileTab>,
    /// The active tab per split, as indices into `open`.
    pub actives: [Option<usize>; 2],
    /// Which split the keyboard drives. Follows the last activation, so
    /// "close the tab" always means the tab you last touched.
    pub focus: u8,
    /// Tree row to scroll into view; consumed by the render once the row
    /// actually appears (its listing may still be in flight).
    pub reveal: Option<String>,
    /// The flat file list for go-to-file, when fetched: (paths, truncated).
    pub tree_paths: Option<(Vec<String>, bool)>,
    pub tree_pending: bool,
    /// The latest content search for this session.
    pub search: Option<SearchState>,
}

impl SessionExplorer {
    /// Whether a second editor column exists at all.
    pub fn split(&self) -> bool {
        self.open.iter().any(|t| t.group == 1)
    }

    /// Indices of the tabs on one side, in strip order.
    pub fn group_tabs(&self, group: u8) -> Vec<usize> {
        (0..self.open.len()).filter(|i| self.open[*i].group == group).collect()
    }

    /// The active tab of one side — validated, so a stale index (or one
    /// whose tab moved sides) reads as "nothing active", never a lie.
    pub fn active_of(&self, group: u8) -> Option<usize> {
        self.actives[usize::from(group.min(1))]
            .filter(|i| self.open.get(*i).is_some_and(|t| t.group == group))
    }

    /// The active tab of the focused split — what keyboard actions act on.
    pub fn active_tab(&self) -> Option<&FileTab> {
        self.active_of(self.focus).and_then(|i| self.open.get(i))
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut FileTab> {
        self.active_of(self.focus).and_then(|i| self.open.get_mut(i))
    }

    /// Tab indices, most recently used first — the order a switcher shows.
    /// Both splits together: "recent" is about your attention, not geometry.
    pub fn mru(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.open.len()).collect();
        idx.sort_by(|a, b| self.open[*b].last_used.cmp(&self.open[*a].last_used));
        idx
    }
}

pub struct Explorer {
    /// The session on display. `states` outlives switches — that is the point.
    pub session: Option<SessionId>,
    states: HashMap<SessionId, SessionExplorer>,
    /// Shape loaded from disk, consumed lazily as sessions are first shown.
    saved: HashMap<String, SavedSession>,
    /// Monotonic activation counter; never wall-clock, only ordering.
    clock: u64,
    /// The shape changed and `~/.mogeung/explorer.json` is stale. Flushed once
    /// per frame by the app, the same discipline as `prefs_dirty`.
    pub dirty: bool,
}

impl Explorer {
    /// Drop every session's live state, keeping what was loaded from disk.
    ///
    /// For switching daemons (`R-I7`): open tabs and file bodies belong to the
    /// machine that served them, and a file left on screen after a switch is a
    /// file the new daemon may not have. `saved` stays — it is keyed by session
    /// id and is this window's memory of *shape*, re-applied only when a
    /// session of that id is seen again, so keeping it costs nothing and
    /// switching back to a daemon restores the arrangement.
    pub fn forget_all(&mut self) {
        self.session = None;
        self.states.clear();
    }

    /// Load the remembered shape. Any failure yields an empty explorer and a
    /// warning, never a blocked launch — the layout's rule, applied here.
    pub fn load() -> (Self, Option<String>) {
        let path = disk_path();
        let (saved, warning) = match std::fs::read_to_string(&path) {
            Err(_) => (SavedFile::default(), None),
            Ok(text) => match parse_saved(&text) {
                Ok(f) => (f, None),
                Err(e) => (
                    SavedFile::default(),
                    Some(format!(
                        "{} is unreadable ({e}) — the explorer starts blank",
                        path.display()
                    )),
                ),
            },
        };
        (
            Explorer {
                session: None,
                states: HashMap::new(),
                saved: saved.sessions,
                clock: 0,
                dirty: false,
            },
            warning,
        )
    }

    #[cfg(test)]
    fn blank() -> Self {
        Explorer {
            session: None,
            states: HashMap::new(),
            saved: HashMap::new(),
            clock: 0,
            dirty: false,
        }
    }

    /// Point the explorer at `id`. Nothing is dropped: the session you left
    /// keeps its tree and tabs for when you come back.
    pub fn ensure_session(&mut self, id: &SessionId) {
        if self.session.as_ref() == Some(id) {
            return;
        }
        self.session = Some(id.clone());
        if !self.states.contains_key(id) {
            let seeded = self.seed(id);
            self.states.insert(id.clone(), seeded);
        }
    }

    /// A fresh state for `id`, restored from disk when it was saved: same
    /// expanded dirs, same tabs, same pins — and no bodies, which the pane
    /// re-fetches. Content is never trusted across a restart.
    fn seed(&mut self, id: &str) -> SessionExplorer {
        let Some(saved) = self.saved.get(id) else {
            return SessionExplorer::default();
        };
        let mut st = SessionExplorer {
            expanded: saved.expanded.iter().cloned().collect(),
            ..Default::default()
        };
        for t in &saved.open {
            self.clock += 1;
            st.open.push(FileTab {
                path: t.path.clone(),
                rev: None,
                pinned: t.pinned,
                group: t.group.min(1),
                view: None,
                goto_line: None,
                last_used: self.clock,
            });
        }
        // A saved index pointing past the tabs, or at a tab of the wrong
        // side (a hand-edit, a truncated write), falls back to that side's
        // first tab rather than being trusted.
        let pick = |wanted: Option<usize>, g: u8| -> Option<usize> {
            wanted
                .filter(|i| st.open.get(*i).is_some_and(|t| t.group == g))
                .or_else(|| st.open.iter().position(|t| t.group == g))
        };
        st.actives[0] = pick(saved.active, 0);
        st.actives[1] = pick(saved.active_right, 1);
        st
    }

    /// The state of the session on display, if any is. For callers — the
    /// palette — that render without having ensured a session first.
    pub fn try_current(&self) -> Option<&SessionExplorer> {
        self.session.as_ref().and_then(|id| self.states.get(id))
    }

    /// The state of the session on display. Callers run `ensure_session`
    /// first; a panic here is a sequencing bug, not a data condition.
    pub fn current(&self) -> &SessionExplorer {
        self.session
            .as_ref()
            .and_then(|id| self.states.get(id))
            .expect("explorer used before ensure_session")
    }

    pub fn current_mut(&mut self) -> &mut SessionExplorer {
        let id = self.session.as_ref().expect("explorer used before ensure_session");
        self.states
            .get_mut(id)
            .expect("explorer used before ensure_session")
    }

    // -- opening, closing, pinning ------------------------------------------

    /// Open `path` in the current session — into its existing tab if it has
    /// one, else into the preview slot (`pin: false`), else as a new tab.
    ///
    /// This is the IntelliJ contract: a single click reuses the one unpinned
    /// tab so browsing does not shed tabs behind it; pinning (or opening
    /// pinned, as search results do) makes the tab permanent.
    pub fn open_file(&mut self, path: &str, pin: bool, goto_line: Option<u64>) {
        self.clock += 1;
        let clock = self.clock;
        let st = self.current_mut();
        let group = st.focus.min(1);
        if let Some(i) = st
            .open
            .iter()
            .position(|t| t.rev.is_none() && t.path == path)
        {
            // Already open — on either side. Activation follows it there
            // rather than opening the same file twice.
            let t = &mut st.open[i];
            t.pinned |= pin;
            t.goto_line = goto_line;
            t.last_used = clock;
            st.focus = st.open[i].group;
            st.actives[usize::from(st.focus)] = Some(i);
        } else if let (false, Some(i)) = (
            pin,
            // The preview slot is per side: browsing on the right must not
            // eat the preview you were reading on the left. A revision tab
            // is never the preview slot — it was opened deliberately.
            st.open
                .iter()
                .position(|t| !t.pinned && t.rev.is_none() && t.group == group),
        ) {
            st.open[i] = FileTab {
                path: path.to_string(),
                rev: None,
                pinned: false,
                group,
                view: None,
                goto_line,
                last_used: clock,
            };
            st.actives[usize::from(group)] = Some(i);
        } else {
            st.open.push(FileTab {
                path: path.to_string(),
                rev: None,
                pinned: pin,
                group,
                view: None,
                goto_line,
                last_used: clock,
            });
            st.actives[usize::from(group)] = Some(st.open.len() - 1);
        }
        self.evict_bodies();
        self.dirty = true;
    }

    /// Open `path` as it stood at `rev` — a read-only revision tab, always
    /// deliberate (pinned), reused when the same (path, rev) is already
    /// open. `R-D11`.
    pub fn open_file_at_rev(&mut self, path: &str, rev: &str) {
        self.clock += 1;
        let clock = self.clock;
        let st = self.current_mut();
        let group = st.focus.min(1);
        if let Some(i) = st
            .open
            .iter()
            .position(|t| t.path == path && t.rev.as_deref() == Some(rev))
        {
            st.open[i].last_used = clock;
            st.focus = st.open[i].group;
            st.actives[usize::from(st.focus)] = Some(i);
        } else {
            st.open.push(FileTab {
                path: path.to_string(),
                rev: Some(rev.to_string()),
                pinned: true,
                group,
                view: None,
                goto_line: None,
                last_used: clock,
            });
            st.actives[usize::from(group)] = Some(st.open.len() - 1);
        }
        self.evict_bodies();
        // Not marked dirty: revision tabs are not saved, so the shape on
        // disk did not change.
    }

    pub fn close_tab(&mut self, i: usize) {
        let st = self.current_mut();
        if i >= st.open.len() {
            return;
        }
        let closed_group = st.open[i].group;
        st.open.remove(i);
        for g in 0..2 {
            st.actives[g] = match st.actives[g] {
                None => None,
                Some(a) if a == i => None,
                Some(a) if a > i => Some(a - 1),
                some => some,
            };
        }
        // The side that lost its active tab lands on its most recently used
        // remaining one — attention order, not strip order.
        if st.active_of(closed_group).is_none() {
            st.actives[usize::from(closed_group)] = st
                .group_tabs(closed_group)
                .into_iter()
                .max_by_key(|ix| st.open[*ix].last_used);
        }
        // A focus pointing at an empty side is a dead keyboard; follow the
        // tabs that still exist.
        if st.group_tabs(st.focus).is_empty() {
            st.focus ^= 1;
            if st.group_tabs(st.focus).is_empty() {
                st.focus = 0;
            }
        }
        self.dirty = true;
    }

    pub fn toggle_pin_active(&mut self) {
        if let Some(t) = self.current_mut().active_tab_mut() {
            t.pinned = !t.pinned;
            self.dirty = true;
        }
    }

    /// Move the active tab along its own side's strip, wrapping.
    pub fn cycle_tab(&mut self, delta: i32) {
        self.clock += 1;
        let clock = self.clock;
        let st = self.current_mut();
        let idxs = st.group_tabs(st.focus);
        if idxs.is_empty() {
            return;
        }
        let pos = st
            .active_of(st.focus)
            .and_then(|a| idxs.iter().position(|x| *x == a))
            .unwrap_or(0);
        let next = idxs[(pos as i64 + i64::from(delta)).rem_euclid(idxs.len() as i64) as usize];
        st.actives[usize::from(st.focus)] = Some(next);
        st.open[next].last_used = clock;
        self.evict_bodies();
    }

    /// Activate tab `i` (a click on the strip, or a switcher pick). Focus
    /// follows to the tab's side.
    pub fn activate(&mut self, i: usize) {
        self.clock += 1;
        let clock = self.clock;
        let st = self.current_mut();
        if i < st.open.len() {
            st.focus = st.open[i].group;
            st.actives[usize::from(st.focus)] = Some(i);
            st.open[i].last_used = clock;
        }
        self.evict_bodies();
    }

    /// Send tab `i` to the other side of the split and follow it there.
    /// Moving the last right-hand tab back left is also how the split closes.
    pub fn move_tab_to_other_side(&mut self, i: usize) {
        self.activate(i);
        let st = self.current_mut();
        let Some(i) = st.active_of(st.focus) else {
            return;
        };
        let from = st.open[i].group;
        let to = from ^ 1;
        st.open[i].group = to;
        st.actives[usize::from(to)] = Some(i);
        // The side it left shows its most recently used remaining tab.
        st.actives[usize::from(from)] = st
            .group_tabs(from)
            .into_iter()
            .max_by_key(|ix| st.open[*ix].last_used);
        st.focus = to;
        self.dirty = true;
    }

    /// Drop the bodies of everything but the `LIVE_BODIES` most recently used
    /// tabs. The tab stays; activating it fetches again.
    fn evict_bodies(&mut self) {
        let st = self.current_mut();
        let mru = st.mru();
        for (rank, i) in mru.into_iter().enumerate() {
            if rank >= LIVE_BODIES {
                st.open[i].view = None;
            }
        }
    }

    /// Expand every ancestor of `path` and ask the render to scroll to it.
    /// Listings the tree lacks are requested by the pane, which is also what
    /// makes this work when half the chain has never been listed.
    pub fn reveal(&mut self, path: &str) {
        let st = self.current_mut();
        for a in ancestors(path) {
            st.expanded.insert(a);
        }
        st.reveal = Some(path.to_string());
        self.dirty = true;
    }

    // -- wire ingest --------------------------------------------------------
    //
    // Everything is routed by the session on the message, not the session on
    // display — events are broadcast to every client, so strays are normal.
    // A stray for a session this explorer has never shown is dropped; a stray
    // for one it has lands in *that* session's state, where it is not a stray
    // at all.

    pub fn ingest_dir(&mut self, session_id: &SessionId, path: String, entries: Vec<DirEntry>) {
        if let Some(st) = self.states.get_mut(session_id) {
            st.pending.remove(&path);
            st.expanded.insert(path.clone());
            st.dirs.insert(path, entries);
        }
    }

    pub fn ingest_file(
        &mut self,
        session_id: &SessionId,
        path: String,
        content: String,
        truncated: bool,
    ) {
        if let Some(st) = self.states.get_mut(session_id) {
            st.pending_files.remove(&path);
            // Filling every tab with the path, not just the active one — the
            // user may have moved on while the answer was in flight. Never a
            // revision tab: its body is a different era of the file.
            for t in st
                .open
                .iter_mut()
                .filter(|t| t.rev.is_none() && t.path == path)
            {
                t.view = Some(FileView {
                    content: content.clone(),
                    truncated,
                });
            }
        }
    }

    /// A revision tab's body arriving. Keyed by (path, rev) — the worktree
    /// twin of the same path must not swallow it, nor vice versa.
    pub fn ingest_rev_file(
        &mut self,
        session_id: &SessionId,
        sha: &str,
        path: &str,
        content: String,
        truncated: bool,
    ) {
        if let Some(st) = self.states.get_mut(session_id) {
            st.pending_files.remove(&rev_key(sha, path));
            for t in st
                .open
                .iter_mut()
                .filter(|t| t.path == path && t.rev.as_deref() == Some(sha))
            {
                t.view = Some(FileView {
                    content: content.clone(),
                    truncated,
                });
            }
        }
    }

    pub fn ingest_tree(&mut self, session_id: &SessionId, paths: Vec<String>, truncated: bool) {
        if let Some(st) = self.states.get_mut(session_id) {
            st.tree_pending = false;
            st.tree_paths = Some((paths, truncated));
        }
    }

    /// Answers for a query that is no longer the one on screen are dropped —
    /// the stray-session rule, applied to superseded searches.
    pub fn ingest_matches(
        &mut self,
        session_id: &SessionId,
        query: &str,
        matches: Vec<ContentMatch>,
        truncated: bool,
    ) {
        if let Some(st) = self.states.get_mut(session_id) {
            if let Some(s) = &mut st.search {
                if s.query == query {
                    s.matches = matches;
                    s.truncated = truncated;
                    s.in_flight = false;
                }
            }
        }
    }

    // -- persistence --------------------------------------------------------

    /// The shape worth keeping, merged over what was on disk — sessions not
    /// shown this run keep their saved entry rather than being forgotten.
    fn to_saved(&self) -> SavedFile {
        let mut sessions = self.saved.clone();
        for (id, st) in &self.states {
            // Revision tabs stay out of the file, so the surviving actives
            // must be re-pointed at the filtered list.
            let kept: Vec<usize> = (0..st.open.len())
                .filter(|&i| st.open[i].rev.is_none())
                .collect();
            let remap = |a: Option<usize>| a.and_then(|i| kept.iter().position(|&k| k == i));
            let entry = SavedSession {
                expanded: {
                    let mut v: Vec<String> = st.expanded.iter().cloned().collect();
                    v.sort();
                    v
                },
                open: kept
                    .iter()
                    .map(|&i| SavedTab {
                        path: st.open[i].path.clone(),
                        pinned: st.open[i].pinned,
                        group: st.open[i].group,
                    })
                    .collect(),
                active: remap(st.active_of(0)),
                active_right: remap(st.active_of(1)),
            };
            if entry.expanded.is_empty() && entry.open.is_empty() {
                sessions.remove(id);
            } else {
                sessions.insert(id.clone(), entry);
            }
        }
        SavedFile { sessions }
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.dirty = false;
        let path = disk_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(&self.to_saved()).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }
}

fn disk_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("explorer.json")
}

// What lands in `~/.mogeung/explorer.json`. Shape only — never file bodies:
// content on disk would go stale silently, and re-fetching is cheap.
#[derive(Serialize, Deserialize, Default, Clone)]
struct SavedSession {
    #[serde(default)]
    expanded: Vec<String>,
    #[serde(default)]
    open: Vec<SavedTab>,
    /// Active tab of the left split. Named `active` since before the split
    /// existed, and kept so older files load unchanged.
    #[serde(default)]
    active: Option<usize>,
    #[serde(default)]
    active_right: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SavedTab {
    path: String,
    #[serde(default)]
    pinned: bool,
    /// 0 = left, 1 = right. Absent in files from before the split: left.
    #[serde(default)]
    group: u8,
}

#[derive(Serialize, Deserialize, Default)]
struct SavedFile {
    #[serde(default)]
    sessions: HashMap<String, SavedSession>,
}

fn parse_saved(text: &str) -> Result<SavedFile, serde_json::Error> {
    serde_json::from_str(text)
}

/// The in-flight key for a revision-tab fetch. Shares `pending_files` with
/// worktree fetches; the `sha:` prefix keeps the two namespaces apart.
pub fn rev_key(sha: &str, path: &str) -> String {
    format!("{sha}:{path}")
}

/// The directories above `path`, nearest the root first — what reveal expands.
/// The root itself (`""`) is not included; it is always listed anyway.
pub fn ancestors(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut prefix = String::new();
    let Some((dirs, _file)) = path.rsplit_once('/') else {
        return out;
    };
    for part in dirs.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        out.push(prefix.clone());
    }
    out
}

/// The 1-based lines of `content` containing `query`, one entry per
/// occurrence — Ctrl+F's corpus. Smart-cased like the worktree search, and
/// entirely client-side: the body is already in the tab, so the daemon never
/// hears about keystrokes.
pub fn find_lines(content: &str, query: &str) -> Vec<u64> {
    if query.is_empty() {
        return Vec::new();
    }
    let sensitive = query.chars().any(|c| c.is_uppercase());
    let needle = if sensitive { query.to_string() } else { query.to_lowercase() };
    let mut out = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let hits = if sensitive {
            line.matches(needle.as_str()).count()
        } else {
            line.to_lowercase().matches(needle.as_str()).count()
        };
        for _ in 0..hits {
            out.push((i + 1) as u64);
        }
    }
    out
}

/// Join a relative directory and an entry name into the wire path.
pub fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// The language hint for the highlighter: the extension, or the bare file name
/// for the `Makefile`/`Dockerfile` kind, which is how syntect knows those.
pub fn language_of(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext,
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> DirEntry {
        DirEntry {
            name: name.into(),
            is_dir,
        }
    }

    fn explorer_on(id: &str) -> Explorer {
        let mut e = Explorer::blank();
        e.ensure_session(&id.to_string());
        e
    }

    /// The R-B25 reversal of the old wipe-on-switch: each session keeps its
    /// own tree and tabs, and coming back finds them as they were left.
    #[test]
    fn switching_sessions_keeps_each_sessions_state() {
        let mut e = explorer_on("a");
        e.ingest_dir(&"a".to_string(), "".into(), vec![entry("src", true)]);
        e.open_file("src/lib.rs", true, None);
        e.ensure_session(&"b".to_string());
        assert!(e.current().dirs.is_empty(), "session b sees session a's tree");
        assert!(e.current().open.is_empty(), "session b sees session a's tabs");
        e.ensure_session(&"a".to_string());
        assert!(!e.current().dirs.is_empty(), "session a's tree was dropped");
        assert_eq!(e.current().open.len(), 1, "session a's tab was dropped");
    }

    /// A stray answer must never land in the session on display; one for a
    /// session we know goes to that session, where it is not a stray.
    #[test]
    fn stray_answers_land_in_their_own_session_or_nowhere() {
        let mut e = explorer_on("a");
        e.ensure_session(&"b".to_string());
        // Looking at b; an answer for a lands in a, not b.
        e.ingest_dir(&"a".to_string(), "src".into(), vec![entry("lib.rs", false)]);
        assert!(e.current().dirs.is_empty(), "a's listing landed in b");
        // An answer for a session never shown is dropped entirely.
        e.ingest_dir(&"zz".to_string(), "src".into(), vec![entry("x", false)]);
        e.ensure_session(&"a".to_string());
        assert!(e.current().dirs.contains_key("src"), "a's own answer was lost");
    }

    /// The preview contract: single-click opens replace the one unpinned tab;
    /// a pinned tab survives everything short of being closed.
    #[test]
    fn preview_tabs_are_replaced_and_pinned_tabs_survive() {
        let mut e = explorer_on("a");
        e.open_file("a.rs", false, None);
        e.open_file("b.rs", false, None);
        let st = e.current();
        assert_eq!(st.open.len(), 1, "a preview open must reuse the preview tab");
        assert_eq!(st.open[0].path, "b.rs");

        e.toggle_pin_active();
        e.open_file("c.rs", false, None);
        let st = e.current();
        assert_eq!(st.open.len(), 2, "a pinned tab must not be replaced");
        assert_eq!(st.open[0].path, "b.rs");
        assert_eq!(st.open[1].path, "c.rs");

        // Re-opening an open file activates its tab instead of duplicating it.
        e.open_file("b.rs", false, None);
        assert_eq!(e.current().open.len(), 2);
        assert_eq!(e.current().active_tab().unwrap().path, "b.rs");
    }

    #[test]
    fn opening_pinned_never_touches_the_preview_slot() {
        let mut e = explorer_on("a");
        e.open_file("preview.rs", false, None);
        e.open_file("result.rs", true, Some(12));
        let st = e.current();
        assert_eq!(st.open.len(), 2, "a pinned open must be its own tab");
        assert_eq!(st.open[0].path, "preview.rs");
        assert!(st.open[1].pinned);
        assert_eq!(st.open[1].goto_line, Some(12), "the search line must ride along");
    }

    /// Bodies are capped; tabs are not. The oldest bodies go first, and the
    /// tab that lost one is refetched on activation, not broken.
    #[test]
    fn only_the_most_recent_bodies_are_kept() {
        let id = "a".to_string();
        let mut e = explorer_on(&id);
        for i in 0..(LIVE_BODIES + 3) {
            let path = format!("f{i}.rs");
            e.open_file(&path, true, None);
            e.ingest_file(&id, path, "body".into(), false);
        }
        let st = e.current();
        assert_eq!(st.open.len(), LIVE_BODIES + 3, "tabs must all survive");
        let with_bodies = st.open.iter().filter(|t| t.view.is_some()).count();
        assert_eq!(with_bodies, LIVE_BODIES, "bodies past the cap must be dropped");
        assert!(st.open[0].view.is_none(), "the oldest body should be the one evicted");
        assert!(st.active_tab().unwrap().view.is_some(), "the active tab keeps its body");
    }

    #[test]
    fn closing_tabs_keeps_the_active_index_honest() {
        let mut e = explorer_on("a");
        for p in ["a.rs", "b.rs", "c.rs"] {
            e.open_file(p, true, None);
        }
        e.activate(1);
        // Closing a tab to the left shifts the active index with it.
        e.close_tab(0);
        assert_eq!(e.current().active_tab().unwrap().path, "b.rs");
        // Closing the active tab lands on its neighbour.
        e.close_tab(0);
        assert_eq!(e.current().active_tab().unwrap().path, "c.rs");
        e.close_tab(0);
        assert!(e.current().active_tab().is_none(), "no tabs, no active tab");
        assert!(e.current().active_of(0).is_none());
        // Closing what is not there must not panic.
        e.close_tab(5);
    }

    #[test]
    fn cycling_wraps_both_ways() {
        let mut e = explorer_on("a");
        for p in ["a.rs", "b.rs", "c.rs"] {
            e.open_file(p, true, None);
        }
        e.cycle_tab(1);
        assert_eq!(e.current().active_tab().unwrap().path, "a.rs", "forward wraps");
        e.cycle_tab(-1);
        assert_eq!(e.current().active_tab().unwrap().path, "c.rs", "backward wraps");
    }

    /// The switcher shows what you touched last, first.
    #[test]
    fn mru_orders_by_last_activation() {
        let mut e = explorer_on("a");
        for p in ["a.rs", "b.rs", "c.rs"] {
            e.open_file(p, true, None);
        }
        e.activate(0); // a.rs is now the most recent
        let mru = e.current().mru();
        let order: Vec<&str> = mru.iter().map(|i| e.current().open[*i].path.as_str()).collect();
        assert_eq!(order, ["a.rs", "c.rs", "b.rs"]);
    }

    /// Reveal expands exactly the ancestor chain — no more, and the root is
    /// implicit because it is always listed.
    #[test]
    fn reveal_expands_the_ancestor_chain() {
        let mut e = explorer_on("a");
        e.reveal("src/net/wire.rs");
        let st = e.current();
        assert!(st.expanded.contains("src"));
        assert!(st.expanded.contains("src/net"));
        assert_eq!(st.expanded.len(), 2, "only the chain: {:?}", st.expanded);
        assert_eq!(st.reveal.as_deref(), Some("src/net/wire.rs"));
    }

    #[test]
    fn ancestors_of_a_root_file_are_none() {
        assert!(ancestors("README.md").is_empty());
        assert_eq!(ancestors("a/b/c.rs"), ["a", "a/b"]);
    }

    /// A search answer for the query on screen lands; one for a query the
    /// user has since replaced is dropped — otherwise a slow old search
    /// overwrites a fast new one and the results lie about the box above them.
    #[test]
    fn superseded_search_answers_are_dropped() {
        let id = "a".to_string();
        let mut e = explorer_on(&id);
        e.current_mut().search = Some(SearchState {
            query: "new".into(),
            matches: Vec::new(),
            truncated: false,
            in_flight: true,
        });
        let m = |t: &str| ContentMatch {
            path: "x.rs".into(),
            line: 1,
            text: t.into(),
        };
        e.ingest_matches(&id, "old", vec![m("stale")], false);
        let s = e.current().search.as_ref().unwrap();
        assert!(s.matches.is_empty(), "a superseded answer landed");
        assert!(s.in_flight, "a superseded answer ended the wait");
        e.ingest_matches(&id, "new", vec![m("fresh")], true);
        let s = e.current().search.as_ref().unwrap();
        assert_eq!(s.matches.len(), 1);
        assert!(s.truncated);
        assert!(!s.in_flight);
    }

    /// The round trip that makes "remember where it was" true across a
    /// restart: shape survives, bodies do not.
    #[test]
    fn the_saved_shape_round_trips_and_restores_without_bodies() {
        let id = "a".to_string();
        let mut e = explorer_on(&id);
        e.current_mut().expanded.insert("src".into());
        e.open_file("src/lib.rs", true, None);
        e.open_file("README.md", false, None);
        e.ingest_file(&id, "src/lib.rs".into(), "body".into(), false);
        e.activate(0);

        let json = serde_json::to_string(&e.to_saved()).unwrap();
        let mut back = Explorer::blank();
        back.saved = parse_saved(&json).unwrap().sessions;
        back.ensure_session(&id);
        let st = back.current();
        assert!(st.expanded.contains("src"));
        assert_eq!(st.open.len(), 2);
        assert_eq!(st.open[0].path, "src/lib.rs");
        assert!(st.open[0].pinned);
        assert!(!st.open[1].pinned);
        assert_eq!(st.active_of(0), Some(0));
        assert!(st.open.iter().all(|t| t.view.is_none()), "bodies must never persist");
    }

    /// Revision tabs are session-local by design: they never reach the
    /// saved file, and the surviving actives are re-pointed at the tabs
    /// that do.
    #[test]
    fn revision_tabs_are_not_saved_and_actives_survive_the_filter() {
        let id = "a".to_string();
        let mut e = explorer_on(&id);
        e.open_file_at_rev("src/lib.rs", "abc123");
        e.open_file("src/lib.rs", true, None);
        let out = e.to_saved();
        let saved = &out.sessions[&id];
        assert_eq!(saved.open.len(), 1, "the revision tab leaked to disk");
        assert_eq!(
            saved.active,
            Some(0),
            "the active index must be remapped past the dropped revision tab"
        );
    }

    /// A revision tab and its worktree twin hold different eras of the same
    /// path; each answer must land only in its own.
    #[test]
    fn revision_bodies_and_worktree_bodies_do_not_cross() {
        let id = "a".to_string();
        let mut e = explorer_on(&id);
        e.open_file("f.rs", true, None);
        e.open_file_at_rev("f.rs", "abc123");
        e.ingest_file(&id, "f.rs".into(), "worktree era".into(), false);
        e.ingest_rev_file(&id, "abc123", "f.rs", "commit era".into(), false);
        let st = e.current();
        let worktree = st.open.iter().find(|t| t.rev.is_none()).unwrap();
        let rev = st.open.iter().find(|t| t.rev.is_some()).unwrap();
        assert_eq!(worktree.view.as_ref().unwrap().content, "worktree era");
        assert_eq!(rev.view.as_ref().unwrap().content, "commit era");
        // Re-opening the same (path, rev) reuses the tab.
        e.open_file_at_rev("f.rs", "abc123");
        assert_eq!(e.current().open.len(), 2);
        // A different rev of the same path is its own tab.
        e.open_file_at_rev("f.rs", "abc123^");
        assert_eq!(e.current().open.len(), 3);
    }

    /// A session merely mentioned on disk is not forgotten by a run that
    /// never showed it — and one whose state emptied out is dropped.
    #[test]
    fn saving_merges_over_sessions_not_shown_this_run() {
        let json = r#"{"sessions":{"old":{"expanded":["src"],"open":[],"active":null}}}"#;
        let mut e = Explorer::blank();
        e.saved = parse_saved(json).unwrap().sessions;
        e.ensure_session(&"new".to_string());
        e.current_mut().expanded.insert("docs".into());
        let out = e.to_saved();
        assert!(out.sessions.contains_key("old"), "an unvisited session was forgotten");
        assert!(out.sessions.contains_key("new"));
    }

    /// Corrupt state on disk degrades to a blank explorer with a warning —
    /// never a crash, never a blocked launch. The layout's rule.
    #[test]
    fn a_corrupt_saved_file_degrades_to_blank() {
        assert!(parse_saved("not json").is_err());
        let f = parse_saved("{}").unwrap();
        assert!(f.sessions.is_empty(), "an empty object is a valid empty file");
        // A file from a future version with fields we lack still loads.
        let f = parse_saved(r#"{"sessions":{"a":{"expanded":[],"someday":1}}}"#).unwrap();
        assert!(f.sessions.contains_key("a"));
    }

    /// An `active` index pointing past the saved tabs (a hand-edit, or a
    /// truncated write) is clamped, not trusted.
    #[test]
    fn a_saved_active_index_out_of_range_is_clamped() {
        let json = r#"{"sessions":{"a":{"expanded":[],"open":[{"path":"x.rs"}],"active":7}}}"#;
        let mut e = Explorer::blank();
        e.saved = parse_saved(json).unwrap().sessions;
        e.ensure_session(&"a".to_string());
        assert_eq!(e.current().active_of(0), Some(0));
    }

    /// The split contract: a tab sent to the other side takes focus with it,
    /// each side keeps its own active tab, and closing the last right-hand
    /// tab collapses the split and hands the keyboard back to the left.
    #[test]
    fn moving_a_tab_across_the_split_moves_focus_and_actives() {
        let mut e = explorer_on("a");
        e.open_file("a.rs", true, None);
        e.open_file("b.rs", true, None);
        assert!(!e.current().split());

        e.move_tab_to_other_side(1); // b.rs goes right
        let st = e.current();
        assert!(st.split());
        assert_eq!(st.focus, 1, "focus follows the moved tab");
        assert_eq!(st.open[st.active_of(1).unwrap()].path, "b.rs");
        assert_eq!(st.open[st.active_of(0).unwrap()].path, "a.rs", "the left keeps its own");

        // Opening while the right is focused lands on the right.
        e.open_file("c.rs", false, None);
        let st = e.current();
        assert_eq!(st.open[st.active_of(1).unwrap()].path, "c.rs");
        assert_eq!(st.open[st.active_of(0).unwrap()].path, "a.rs");

        // Closing everything on the right collapses the split.
        let right = e.current().group_tabs(1);
        for i in right.into_iter().rev() {
            e.close_tab(i);
        }
        let st = e.current();
        assert!(!st.split());
        assert_eq!(st.focus, 0, "an empty side must not keep the keyboard");
        assert_eq!(st.open[st.active_of(0).unwrap()].path, "a.rs");
    }

    /// Each side has its own preview slot: browsing on the right must not
    /// replace the preview being read on the left.
    #[test]
    fn the_preview_slot_is_per_side() {
        let mut e = explorer_on("a");
        e.open_file("left.rs", false, None);
        e.move_tab_to_other_side(0);
        e.open_file("right2.rs", false, None); // replaces the right preview
        let st = e.current();
        assert_eq!(st.group_tabs(1).len(), 1);
        assert_eq!(st.open[st.active_of(1).unwrap()].path, "right2.rs");
        // Focus left, then open and preview there — the right preview must
        // survive both.
        e.current_mut().focus = 0;
        e.open_file("newleft.rs", true, None);
        e.open_file("peek.rs", false, None);
        let st = e.current();
        assert_eq!(st.open[st.active_of(1).unwrap()].path, "right2.rs", "right preview eaten");
        assert!(st.group_tabs(0).iter().any(|i| st.open[*i].path == "peek.rs"));
    }

    /// Splits survive a restart: groups and both actives round-trip, and a
    /// pre-split file (no `group`, no `active_right`) still loads as before.
    #[test]
    fn the_split_round_trips_and_old_files_still_load() {
        let id = "a".to_string();
        let mut e = explorer_on(&id);
        e.open_file("a.rs", true, None);
        e.open_file("b.rs", true, None);
        e.move_tab_to_other_side(1);
        let json = serde_json::to_string(&e.to_saved()).unwrap();
        let mut back = Explorer::blank();
        back.saved = parse_saved(&json).unwrap().sessions;
        back.ensure_session(&id);
        let st = back.current();
        assert!(st.split());
        assert_eq!(st.open[st.active_of(0).unwrap()].path, "a.rs");
        assert_eq!(st.open[st.active_of(1).unwrap()].path, "b.rs");

        let old = r#"{"sessions":{"a":{"expanded":[],"open":[{"path":"x.rs","pinned":true}],"active":0}}}"#;
        let mut e = Explorer::blank();
        e.saved = parse_saved(old).unwrap().sessions;
        e.ensure_session(&"a".to_string());
        assert!(!e.current().split(), "a pre-split file must load entirely on the left");
        assert_eq!(e.current().active_of(0), Some(0));
    }

    /// Ctrl+F's contract: 1-based, one entry per occurrence, smart-cased,
    /// and an empty query finds nothing rather than everything.
    #[test]
    fn find_lines_counts_occurrences_and_smart_cases() {
        let text = "fn main() {\n    let x = x + x;\n}\nX marks the spot\n";
        assert_eq!(find_lines(text, "x"), [2, 2, 2, 4], "lowercase finds both cases");
        assert_eq!(find_lines(text, "X"), [4], "an uppercase query is exact");
        assert_eq!(find_lines(text, "fn"), [1]);
        assert!(find_lines(text, "").is_empty());
        assert!(find_lines(text, "banana").is_empty());
    }

    #[test]
    fn wire_paths_join_with_slashes_and_no_leading_one() {
        assert_eq!(join("", "src"), "src");
        assert_eq!(join("src", "lib.rs"), "src/lib.rs");
    }

    #[test]
    fn the_language_hint_is_the_extension_or_the_bare_name() {
        assert_eq!(language_of("src/main.rs"), "rs");
        assert_eq!(language_of("a/b/query.test.sql"), "sql");
        assert_eq!(language_of("Makefile"), "Makefile");
        assert_eq!(language_of("src/Dockerfile"), "Dockerfile");
        // A trailing dot is nobody's extension; fall back to the name.
        assert_eq!(language_of("weird."), "weird.");
    }
}
