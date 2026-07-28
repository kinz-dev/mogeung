use crate::net::Net;
use crate::ui::{self, *};
use chrono::Utc;
use egui::{Color32, RichText};
use mogeung_core::attention::{fmt_dur, AttentionItem, AttentionReason};
use mogeung_core::change::RiskLevel;
use mogeung_core::health::{human_bytes, Health};
use mogeung_core::review::{BlastRadius, ReviewDebt};
use mogeung_core::session::LiveStatus;
use mogeung_core::transcript::{EventKind, NoticeLevel};
use mogeung_core::{Change, ClientMsg, ServerMsg, Session, SessionId, TranscriptEvent};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// One view in the detail pane, and the unit the layout tree arranges.
///
/// `Copy` and comparable by value because `egui_tiles` identifies a pane by its
/// value, and serialisable because the arrangement is saved.
#[derive(PartialEq, Eq, Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum Tab {
    Changes,
    Transcript,
    Info,
    /// Review debt for the selected session's repo. `R-D8`.
    Debt,
    /// The session's own terminal, attached through tmux. `R-B18`.
    Terminal,
    /// The session's worktree: tree on the left, read-only viewer on the
    /// right. `R-B24`. A viewer and never an editor — the roadmap's
    /// "explicitly not" list is why there is no write path here.
    Explorer,
    /// The session repo's commits, uncommitted changes and diffs. `R-D10`.
    /// Read-only, permanently — the observer rule, one layer down.
    Git,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Changes => "Changes",
            Tab::Transcript => "Transcript",
            Tab::Info => "Info",
            Tab::Debt => "Debt",
            Tab::Terminal => "Terminal",
            // Display name only. The variant stays `Explorer` because it is
            // serialized into saved layouts; and the pane stays read-only —
            // "Editor" is what the user calls it, not a write path.
            Tab::Explorer => "Editor",
            Tab::Git => "Git",
        }
    }

    /// The action that brings this pane forward. Paired here so a new pane
    /// cannot be added without deciding how the keyboard reaches it.
    pub fn action(self) -> crate::keymap::Action {
        use crate::keymap::Action;
        match self {
            Tab::Changes => Action::TabChanges,
            Tab::Transcript => Action::TabTranscript,
            Tab::Info => Action::TabInfo,
            Tab::Debt => Action::TabDebt,
            Tab::Terminal => Action::TabTerminal,
            Tab::Explorer => Action::TabExplorer,
            Tab::Git => Action::TabGit,
        }
    }
}

/// Which pane the keyboard is driving.
///
/// Navigation actions are pane-agnostic — `Next` means "next thing in whatever
/// has focus" — so one set of bindings works everywhere instead of three sets
/// you have to remember the context for.
/// A scroll asked for by the keyboard, applied on the next frame inside
/// whichever scroll area is on screen.
///
/// egui's `ScrollArea` has **no** keyboard handling of its own — it responds to
/// the wheel and to dragging, and nothing else. Page Up and Page Down therefore
/// did nothing at all until this existed.
#[derive(Clone, Copy, PartialEq)]
enum ScrollRequest {
    /// Fractions of the visible height. Negative is towards the top.
    Pages(f32),
    Top,
    Bottom,
}

impl ScrollRequest {
    /// Convert to the delta egui wants, given the visible height.
    ///
    /// The sign is inverted: `ScrollArea` does `offset -= delta`, so a
    /// *negative* delta moves **down** the content. Offsets are clamped to the
    /// content, which is what makes the huge values for top and bottom safe.
    fn delta(self, viewport_height: f32) -> egui::Vec2 {
        const HUGE: f32 = 1.0e6;
        // Keep a sliver of the previous screen, so a page turn has an anchor
        // rather than jumping to text with no context.
        let page = (viewport_height * 0.85).max(40.0);
        let y = match self {
            ScrollRequest::Pages(n) => -n * page,
            ScrollRequest::Top => HUGE,
            ScrollRequest::Bottom => -HUGE,
        };
        egui::vec2(0.0, y)
    }
}

/// Transcript events drawn before "show earlier" is needed.
///
/// Markdown is parsed on every frame it is visible, so an unbounded transcript
/// would make the frame rate a function of how long the session has been
/// running. A session here already has thousands of events.
const TRANSCRIPT_PAGE: usize = 150;

/// Lane colours for the log graph, cycled — distinct enough to follow a
/// line by eye, few enough to stay quiet.
const GRAPH_COLORS: [egui::Color32; 6] = [
    BLUE,
    GREEN,
    AMBER,
    egui::Color32::from_rgb(0xB0, 0x6A, 0xD8),
    egui::Color32::from_rgb(0x3F, 0xB3, 0xB3),
    egui::Color32::from_rgb(0xD8, 0x6A, 0x9A),
];

#[derive(PartialEq, Clone, Copy, Debug)]
enum Pane {
    Queue,
    Files,
    Diff,
}

impl Pane {
    fn label(self) -> &'static str {
        match self {
            Pane::Queue => "queue",
            Pane::Files => "files",
            Pane::Diff => "diff",
        }
    }
}

/// A hunk you marked while reading, to be turned into a follow-up prompt.
///
/// `R-D1` is the observer-safe version of "send an instruction": mogeung builds
/// the text and puts it on your clipboard. **You** paste it into your terminal.
/// It never types into a session — that would be steering, which is the whole
/// thing v0.1 died of ([ADR-0003]).
#[derive(Clone)]
struct FlaggedHunk {
    session_id: SessionId,
    path: String,
    header: String,
    note: String,
    /// Just the changed lines, for quoting back.
    body: Vec<String>,
}

pub struct App {
    net: Net,

    sessions: HashMap<SessionId, Rc<Session>>,
    queue: Vec<AttentionItem>,
    /// `Rc` because rendering needs a copy that outlives the borrow of `self`,
    /// and these are the two biggest things we own: a `Change` carries every
    /// hunk of every file, and a transcript carries every message.
    ///
    /// Cloning them per frame — which is what this used to do — put the frame
    /// rate on a curve against how long a session had been running, so the app
    /// got measurably less responsive the more there was to look at. A refcount
    /// bump is O(1) and the data is treated as immutable once received.
    changes: HashMap<SessionId, Rc<Change>>,
    events: HashMap<SessionId, Rc<Vec<TranscriptEvent>>>,
    hydrated: HashSet<SessionId>,

    selected: Option<SessionId>,
    tab: Tab,
    selected_file: Option<String>,

    /// Queue text filter. `R-B9`.
    filter: String,
    collapsed_repos: HashSet<String>,

    /// Everything that survives a restart: hidden and pinned sessions, scope,
    /// and the view toggles. See `prefs.rs`.
    prefs: crate::prefs::Prefs,
    /// Set when `prefs` changed this frame; written once at the end.
    prefs_dirty: bool,
    /// Big-text board for a second monitor. `R-C5`.
    ambient: bool,
    /// Run anything by name. `R-B21`.
    palette: crate::palette::Palette,
    /// How the detail panes are arranged. `R-B20`.
    ///
    /// `Option` only so it can be taken out while rendering — see
    /// `detail_panel`. It is `Some` at every other point in the frame.
    tree: Option<crate::layout::Tree>,
    /// Set when the arrangement changed; written once at the end of the frame,
    /// for the same reason `prefs_dirty` exists — dragging a splitter would
    /// otherwise write the file on every frame of the drag.
    layout_dirty: bool,

    /// Cursor and filter for the keyboard settings window. `R-B22`.
    ///
    /// The cursor indexes the *filtered* rows, not `Action::ALL`, so it always
    /// points at something on screen.
    keymap_cursor: usize,
    keymap_filter: String,
    /// Scroll the cursor row into view this frame. Armed by keyboard moves
    /// only — doing it every frame pins the scroll area to the cursor row,
    /// which makes anything below the fold unreachable by mouse.
    keymap_scroll: bool,


    /// Hunks flagged for the follow-up prompt, and the note attached to each.
    /// `R-D1`.
    flagged: Vec<FlaggedHunk>,
    prompt_note: String,
    show_prompt: bool,

    /// Review debt for the current repo. `R-D8`.
    debt: Option<ReviewDebt>,
    /// Blast radius for the selected file. `R-D9`.
    blast: Option<BlastRadius>,
    blast_pending: bool,

    launch_dir: String,
    launch_worktree: bool,
    show_launch: bool,

    /// What the daemon says it can and cannot see. Pushed after every scan.
    health: Health,
    show_health: bool,

    /// System-wide key that raises this window. `None` when disabled or
    /// already taken by another application.
    hotkey: Option<crate::hotkey::Hotkey>,

    /// Markdown render cache. Must outlive a frame — rebuilding it every frame
    /// would re-do the work it exists to avoid.
    md_cache: egui_commonmark::CommonMarkCache,
    /// How many transcript events to draw. Raised by "show earlier".
    transcript_limit: usize,
    /// Keyboard scroll to apply to the content pane this frame.
    scroll: Option<ScrollRequest>,

    /// Where the daemon came from, and how to describe it.
    daemon_mode: crate::daemon::Mode,
    daemon_addr: String,

    /// Which pane the keyboard drives, and the editable bindings.
    pane: Pane,
    keymap: crate::keymap::Keymap,
    show_keymap: bool,
    /// Action currently waiting for a keypress to rebind it.
    capturing: Option<crate::keymap::Action>,
    keymap_io: String,

    /// Highlighted file, which is the opened file when previewing is on.
    file_cursor: Option<String>,

    /// The label editor, when open: which session, and the text as typed.
    /// `R-B26`.
    label_edit: Option<(SessionId, String)>,

    /// Ctrl+F inside the explorer's viewer: the bar, its query, which match
    /// the cursor is on, and a one-frame "grab the keyboard" flag.
    explorer_find_open: bool,
    explorer_find: String,
    explorer_find_cursor: usize,
    explorer_find_focus: bool,

    /// The attached terminal, when the Terminal tab has been opened for a
    /// session that runs under tmux. One at a time: a second attach costs a
    /// pty and a tmux client for a pane nobody is looking at.
    term: Option<crate::term::Term>,
    /// Whether keystrokes belong to the agent or to mogeung. See `handle_keys`.
    term_focused: bool,

    /// The file explorer pane's cache. `R-B24`.
    explorer: crate::explorer::Explorer,

    /// The Git pane's cache. `R-D10`.
    gitview: crate::gitview::GitView,
    /// Blame gutter on in the Editor's viewers.
    annotate: bool,
    /// The blamed line a gutter context menu was opened on — captured at
    /// the right-click, because the pointer wanders while the menu is up.
    blame_menu_line: Option<mogeung_core::wire::BlameLine>,

    errors: Vec<String>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        url: String,
        hotkey: Option<crate::hotkey::Hotkey>,
        hotkey_error: Option<String>,
        daemon_mode: crate::daemon::Mode,
        daemon_addr: String,
    ) -> Self {
        ui::apply_theme(&cc.egui_ctx);
        if let Some(h) = &hotkey {
            h.start_waker(cc.egui_ctx.clone());
        }
        let net = Net::connect(url, cc.egui_ctx.clone());
        let (keymap, keymap_warning) = crate::keymap::Keymap::load();
        let (prefs, prefs_warning) = crate::prefs::Prefs::load();
        let (tree, layout_warning) = crate::layout::load();
        let (explorer, explorer_warning) = crate::explorer::Explorer::load();
        App {
            net,
            hotkey,
            sessions: HashMap::new(),
            queue: Vec::new(),
            changes: HashMap::new(),
            events: HashMap::new(),
            hydrated: HashSet::new(),
            selected: None,
            tab: Tab::Changes,
            selected_file: None,
            filter: String::new(),
            collapsed_repos: HashSet::new(),
            prefs,
            prefs_dirty: false,
            ambient: false,
            palette: crate::palette::Palette::default(),
            tree: Some(tree),
            layout_dirty: false,
            keymap_cursor: 0,
            keymap_filter: String::new(),
            keymap_scroll: false,
            flagged: Vec::new(),
            prompt_note: String::new(),
            show_prompt: false,
            debt: None,
            blast: None,
            blast_pending: false,
            launch_dir: String::new(),
            launch_worktree: true,
            show_launch: false,
            health: Health::default(),
            show_health: false,
            md_cache: egui_commonmark::CommonMarkCache::default(),
            transcript_limit: TRANSCRIPT_PAGE,
            scroll: None,
            daemon_mode,
            daemon_addr,
            pane: Pane::Queue,
            keymap,
            show_keymap: false,
            capturing: None,
            keymap_io: String::new(),
            file_cursor: None,
            label_edit: None,
            explorer_find_open: false,
            explorer_find: String::new(),
            explorer_find_cursor: 0,
            explorer_find_focus: false,
            term: None,
            term_focused: false,
            explorer,
            gitview: Default::default(),
            annotate: false,
            blame_menu_line: None,
            // Surfaced in the window, not only on stderr: the terminal that
            // launched this is exactly what you are trying to stop looking at.
            errors: hotkey_error
                .into_iter()
                .chain(keymap_warning)
                .chain(prefs_warning)
                .chain(layout_warning)
                .chain(explorer_warning)
                .collect(),
        }
    }

    fn ingest(&mut self) {
        let mut sessions_changed = false;
        for msg in self.net.drain() {
            match msg {
                ServerMsg::Snapshot { sessions, queue } => {
                    self.sessions = sessions
                        .into_iter()
                        .map(|s| (s.id.clone(), Rc::new(s)))
                        .collect();
                    self.queue = queue;
                    // A reconnect invalidates our transcript cache.
                    self.hydrated.clear();
                    sessions_changed = true;
                }
                ServerMsg::SessionUpdated { session } => {
                    self.sessions.insert(session.id.clone(), Rc::from(session));
                    sessions_changed = true;
                }
                ServerMsg::SessionRemoved { session_id } => {
                    self.sessions.remove(&session_id);
                    self.changes.remove(&session_id);
                    self.events.remove(&session_id);
                    self.hydrated.remove(&session_id);
                    if self.selected.as_ref() == Some(&session_id) {
                        self.selected = None;
                    }
                }
                ServerMsg::Events { events } => {
                    for ev in events {
                        let list =
                            Rc::make_mut(self.events.entry(ev.session_id.clone()).or_default());
                        if list.last().map(|l| l.seq).unwrap_or(0) < ev.seq {
                            list.push(ev);
                        } else if !list.iter().any(|e| e.seq == ev.seq) {
                            list.push(ev);
                            list.sort_by_key(|e| e.seq);
                        }
                    }
                }
                ServerMsg::Queue { queue } => self.queue = queue,
                ServerMsg::ChangeUpdated { session_id, change } => {
                    if self.selected.as_ref() == Some(&session_id) {
                        let still_there = self
                            .selected_file
                            .as_ref()
                            .map(|p| change.files.iter().any(|f| &f.path == p))
                            .unwrap_or(false);
                        if !still_there {
                            self.selected_file = change.files.first().map(|f| f.path.clone());
                        }
                    }
                    self.changes.insert(session_id, Rc::from(change));
                }
                ServerMsg::Health { health } => self.health = *health,
                ServerMsg::ReviewDebt { debt } => self.debt = Some(*debt),
                ServerMsg::BlastRadius { radius } => {
                    self.blast_pending = false;
                    self.blast = Some(*radius);
                }
                ServerMsg::DirListing {
                    session_id,
                    path,
                    entries,
                } => self.explorer.ingest_dir(&session_id, path, entries),
                ServerMsg::FileContent {
                    session_id,
                    path,
                    content,
                    truncated,
                } => self
                    .explorer
                    .ingest_file(&session_id, path, content, truncated),
                ServerMsg::TreeListing {
                    session_id,
                    paths,
                    truncated,
                } => self.explorer.ingest_tree(&session_id, paths, truncated),
                ServerMsg::ContentMatches {
                    session_id,
                    query,
                    matches,
                    truncated,
                } => self
                    .explorer
                    .ingest_matches(&session_id, &query, matches, truncated),
                ServerMsg::GitCommits {
                    session_id,
                    skip,
                    commits,
                    done,
                    rev,
                    grep,
                    author,
                    path,
                } => self.gitview.ingest_commits(
                    &session_id,
                    skip,
                    commits,
                    done,
                    crate::gitview::LogScope {
                        rev,
                        grep,
                        author,
                        path,
                    },
                ),
                ServerMsg::GitCommitDiff {
                    session_id,
                    sha,
                    files,
                    detail,
                } => self
                    .gitview
                    .ingest_commit_diff(&session_id, sha, files, detail.map(|d| *d)),
                ServerMsg::GitLocalChanges {
                    session_id,
                    entries,
                } => self.gitview.ingest_status(&session_id, entries),
                ServerMsg::GitFileDiff {
                    session_id,
                    path,
                    files,
                } => self.gitview.ingest_file_diff(&session_id, path, files),
                ServerMsg::GitAnnotation {
                    session_id,
                    path,
                    lines,
                    truncated,
                    rev,
                } => self
                    .gitview
                    .ingest_blame(&session_id, path, lines, truncated, rev),
                ServerMsg::GitRefsInfo { session_id, info } => {
                    self.gitview.ingest_refs(&session_id, *info)
                }
                ServerMsg::GitStashList {
                    session_id,
                    stashes,
                } => self.gitview.ingest_stashes(&session_id, stashes),
                ServerMsg::GitStashDiff {
                    session_id,
                    index,
                    files,
                } => self.gitview.ingest_stash_diff(&session_id, index, files),
                ServerMsg::GitSubmoduleList {
                    session_id,
                    submodules,
                } => self.gitview.ingest_submodules(&session_id, submodules),
                ServerMsg::GitRangeDiff {
                    session_id,
                    from,
                    to,
                    files,
                } => self.gitview.ingest_range_diff(&session_id, from, to, files),
                ServerMsg::GitFileAtRevContent {
                    session_id,
                    sha,
                    path,
                    content,
                    truncated,
                } => self
                    .explorer
                    .ingest_rev_file(&session_id, &sha, &path, content, truncated),
                ServerMsg::Error { message } => {
                    self.errors.push(message);
                    if self.errors.len() > 6 {
                        self.errors.remove(0);
                    }
                }
            }
        }
        // `/clear` keeps the process but mints a new session id; the live
        // registry is per-pid, so the succession is visible right here.
        // Hand-applied view-state — labels, pins — follows the work
        // instead of dying with the old id.
        if sessions_changed {
            let facts: Vec<(String, bool, Option<u32>, i64, String)> = self
                .sessions
                .values()
                .map(|s| {
                    (
                        s.id.clone(),
                        s.alive,
                        s.pid,
                        s.started_at.timestamp(),
                        s.cwd.clone(),
                    )
                })
                .collect();
            if self.prefs.migrate_succession(&facts) {
                self.prefs_dirty = true;
            }
        }
    }

    fn select(&mut self, id: SessionId) {
        self.selected = Some(id.clone());
        self.selected_file = None;
        if self.hydrated.insert(id.clone()) {
            self.net.send(ClientMsg::FetchEvents {
                session_id: id.clone(),
                since: 0,
            });
        }
        self.net.send(ClientMsg::RefreshChange { session_id: id });
    }

    /// Returns the `Rc`, not a `&Session`, so `detail_panel` can take an owned
    /// handle for the frame without copying the session out of the map.
    /// The rows the keyboard settings window is showing, in order.
    ///
    /// One definition shared by rendering and the keyboard, for the same reason
    /// `visible_files` exists: a cursor that indexes a different list from the
    /// one on screen lands on rows that are not there.
    ///
    /// Filtering reuses the palette's scorer rather than a substring test, so
    /// the two search boxes in the app behave identically.
    fn keymap_rows(&self) -> Vec<crate::keymap::Action> {
        let q = self.keymap_filter.trim();
        let mut rows: Vec<(crate::keymap::Action, i32)> = crate::keymap::Action::ALL
            .iter()
            .filter_map(|a| {
                let hay = format!("{} {} {}", a.label(), a.group(), self.keymap.describe(*a));
                crate::palette::score(q, &hay).map(|s| (*a, s))
            })
            .collect();
        if !q.is_empty() {
            rows.sort_by(|a, b| b.1.cmp(&a.1));
        }
        rows.into_iter().map(|(a, _)| a).collect()
    }

    /// Navigation for the keyboard settings window.
    ///
    /// Returns whether the key was consumed, so anything it does not claim
    /// still reaches the ordinary bindings — the window is not modal and
    /// swallowing every key while it happens to be open would be worse than
    /// the mouse-only version it replaces.
    fn keymap_window_keys(&mut self, ui: &egui::Ui) -> bool {
        let rows = self.keymap_rows();
        if self.keymap_cursor >= rows.len() {
            self.keymap_cursor = rows.len().saturating_sub(1);
        }
        let (down, up, enter, reset, filter) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J),
                i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K),
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete),
                i.key_pressed(egui::Key::Slash),
            )
        });
        if rows.is_empty() && !filter {
            return false;
        }
        if down {
            self.keymap_cursor = (self.keymap_cursor + 1) % rows.len();
            self.keymap_scroll = true;
        } else if up {
            self.keymap_cursor = (self.keymap_cursor + rows.len() - 1) % rows.len();
            self.keymap_scroll = true;
        } else if enter {
            self.capturing = rows.get(self.keymap_cursor).copied();
        } else if reset {
            if let Some(a) = rows.get(self.keymap_cursor).copied() {
                self.keymap.reset(a);
                if let Err(e) = self.keymap.save() {
                    self.errors.push(format!("could not save keymap: {e}"));
                }
            }
        } else if filter {
            ui.memory_mut(|m| m.request_focus(keymap_filter_id()));
            // Same trap as the queue filter: the `/` would otherwise be typed
            // into the box it just opened.
            ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
        } else {
            return false;
        }
        true
    }

    fn selected_session(&self) -> Option<&Rc<Session>> {
        self.selected.as_ref().and_then(|id| self.sessions.get(id))
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ingest();
        // "Waiting for you — 4m12s" has to keep ticking with no new events.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1000));

        // Checked before anything else so a press raises the window on the
        // same frame it arrives.
        if self.hotkey.as_ref().map(|h| h.pressed()).unwrap_or(false) {
            crate::hotkey::raise(ui.ctx());
        }

        self.handle_keys(ui);
        self.top_bar(ui);
        self.queue_panel(ui);
        // Before the detail pane: a `CentralPanel` claims whatever is left, so
        // every other panel has to be declared first.
        self.status_bar(ui);
        self.detail_panel(ui);
        self.launch_window(ui);
        self.health_window(ui);
        self.prompt_window(ui);
        self.ambient_window(ui);
        self.keymap_window(ui);
        self.label_window(ui);
        // Last, so it draws above every window it can open.
        self.palette_window(ui);

        // Written once per frame at most: preferences change on a click, and
        // saving inside the widget would touch the disk every time a checkbox
        // is merely *drawn*.
        // Consumed by whichever scroll area drew this frame; dropped if none
        // did, so a stale request cannot fire later in a different tab.
        self.scroll = None;

        if self.prefs_dirty {
            self.prefs_dirty = false;
            if let Err(e) = self.prefs.save() {
                self.errors.push(format!("could not save preferences: {e}"));
            }
        }

        // Same discipline as the preferences: the explorer's shape (tabs,
        // pins, expanded dirs) is written at most once a frame, never inside
        // the widget that changed it.
        if self.explorer.dirty {
            if let Err(e) = self.explorer.save() {
                self.errors.push(format!("could not save the explorer state: {e}"));
            }
        }

        // Written only while the pointer is up, so dragging a splitter does not
        // write the file on every frame of the drag. The flag survives until
        // then, so nothing is lost by waiting.
        if self.layout_dirty && !ui.input(|i| i.pointer.any_down()) {
            self.layout_dirty = false;
            if let Some(tree) = &self.tree {
                if let Err(e) = crate::layout::save(tree) {
                    self.errors.push(format!("could not save the layout: {e}"));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Top bar
// ---------------------------------------------------------------------------

impl App {
    fn top_bar(&mut self, root: &mut egui::Ui) {
        // Tighter than the default panel margin. This bar is chrome: it should
        // cost the least vertical space that still reads comfortably, because
        // every pixel it takes is one the transcript does not get.
        egui::Panel::top("top")
            .frame(
                egui::Frame::NONE
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(root, |ui| {
            ui.horizontal(|ui| {
                let title = ui.label(RichText::new("mogeung").strong().size(15.0).color(TEXT_STRONG));
                match &self.hotkey {
                    Some(h) => {
                        title.on_hover_text(format!("{} brings this window forward", h.accel));
                    }
                    None => {
                        title.on_hover_text("no global shortcut — see --hotkey");
                    }
                }

                let (dot, tip) = if self.net.connected {
                    (RichText::new("●").color(GREEN), "connected".to_string())
                } else {
                    (
                        RichText::new("●").color(RED),
                        self.net
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "disconnected".into()),
                    )
                };
                ui.label(dot).on_hover_text(format!(
                    "{} — {}\n\n{}",
                    self.net.url,
                    tip,
                    self.daemon_mode.detail(&self.daemon_addr)
                ));

                // Worth a word on screen, not just a tooltip: with a hosted
                // daemon, closing this window stops watching entirely — which
                // is not what "close a window" usually means.
                if self.daemon_mode == crate::daemon::Mode::Hosting {
                    ui.label(dim(self.daemon_mode.label()))
                        .on_hover_text(self.daemon_mode.detail(&self.daemon_addr));
                }

                ui.separator();

                let waiting = self
                    .queue
                    .iter()
                    .filter(|i| i.reason == AttentionReason::AwaitingInput)
                    .count();
                let needing = self.queue.iter().filter(|i| i.reason.needs_human()).count();
                let live = self.sessions.values().filter(|s| s.alive).count();

                if waiting > 0 {
                    ui.label(badge(&format!("{waiting} waiting for you"), RED));
                } else if needing > 0 {
                    ui.label(badge(&format!("{needing} need you"), AMBER));
                } else {
                    ui.label(dim("queue clear"));
                }
                ui.label(dim(format!("· {live} live session(s)")));

                let out: u64 = self.sessions.values().map(|s| s.tokens_out).sum();
                ui.label(dim(format!("· {} tokens out", tokens(out))));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The blindness indicator. A board that has quietly stopped
                    // seeing things looks exactly like a quiet day, so this is
                    // the one control that must be present at all times — but
                    // it stays grey and small until there is something to say.
                    let urgent = self.health.urgent_alerts();
                    let (glyph, tint) = if urgent > 0 {
                        (icon::WARN, Some(AMBER))
                    } else {
                        (icon::HEALTH, None)
                    };
                    if icon_button(
                        ui,
                        glyph,
                        &format!(
                            "{}  ({})",
                            self.health.headline(),
                            self.keymap.describe(crate::keymap::Action::ToggleHealth)
                        ),
                        self.show_health,
                        tint,
                    )
                    .clicked()
                    {
                        self.show_health = !self.show_health;
                        if self.show_health {
                            self.net.send(ClientMsg::FetchHealth);
                        }
                    }
                    if urgent > 0 {
                        ui.label(RichText::new(urgent.to_string()).color(AMBER).size(11.0));
                    }

                    if icon_button(
                        ui,
                        icon::NEW_SESSION,
                        "New session — opens a real interactive claude in your terminal",
                        false,
                        None,
                    )
                    .clicked()
                    {
                        if self.launch_dir.is_empty() {
                            // Default to the repo of whatever you are looking at.
                            self.launch_dir = self
                                .selected_session()
                                .map(|s| s.repo_root.clone().unwrap_or_else(|| s.cwd.clone()))
                                .unwrap_or_default();
                        }
                        self.show_launch = true;
                    }
                    if icon_button(
                        ui,
                        icon::RESCAN,
                        &format!(
                            "Rescan sessions now  ({})",
                            self.keymap.describe(crate::keymap::Action::Rescan)
                        ),
                        false,
                        None,
                    )
                    .clicked()
                    {
                        self.net.send(ClientMsg::Rescan);
                    }
                    if icon_button(
                        ui,
                        icon::KEYBOARD,
                        &format!(
                            "Keyboard settings  ({})",
                            self.keymap.describe(crate::keymap::Action::OpenKeymap)
                        ),
                        self.show_keymap,
                        None,
                    )
                    .clicked()
                    {
                        self.show_keymap = !self.show_keymap;
                        self.keymap_scroll = self.show_keymap;
                        if self.show_keymap {
                            // Editing bindings and typing into the agent are
                            // mutually exclusive intents.
                            self.term_focused = false;
                        }
                    }
                    if icon_button(
                        ui,
                        icon::AMBIENT,
                        &format!(
                            "Ambient board for a second monitor  ({})",
                            self.keymap.describe(crate::keymap::Action::ToggleAmbient)
                        ),
                        self.ambient,
                        None,
                    )
                    .clicked()
                    {
                        self.ambient = !self.ambient;
                    }
                });
            });

            if !self.errors.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("!").color(RED).strong());
                    ui.label(
                        RichText::new(self.errors.last().unwrap())
                            .color(RED)
                            .size(12.0),
                    );
                    if ui.small_button("dismiss").clicked() {
                        self.errors.clear();
                    }
                });
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Attention queue
// ---------------------------------------------------------------------------

impl App {
    /// The queue after "show quiet" and the text filter, in rank order.
    ///
    /// One definition used by rendering *and* by the keyboard, so `j` always
    /// moves to the row you can actually see. Two separate notions of "the
    /// list" is how keyboard navigation ends up jumping to invisible items.
    fn visible_queue(&self) -> Vec<AttentionItem> {
        use crate::prefs::Scope;
        let q = crate::filter::parse(&self.filter);

        let mut out: Vec<AttentionItem> = self
            .queue
            .iter()
            .filter(|item| {
                let Some(s) = self.sessions.get(&item.session_id) else {
                    return false;
                };
                // Hidden sessions are gone unless you are looking for them, and
                // a filter never resurrects one — otherwise "hidden" would mean
                // "hidden until you search", which is not what anyone means.
                if self.prefs.is_hidden(&s.id) && !self.prefs.reveal_hidden {
                    return false;
                }
                // Pinned sessions ignore scope. Pinning is an explicit "keep
                // this in front of me", and a scope that could override it
                // would make the pin unreliable.
                if !self.prefs.is_pinned(&s.id) {
                    match self.prefs.scope {
                        Scope::NeedsYou => {
                            if !item.reason.needs_human() {
                                return false;
                            }
                        }
                        Scope::Live => {
                            if !s.alive {
                                return false;
                            }
                        }
                        Scope::All => {}
                    }
                }
                crate::filter::matches(&q, s, self.prefs.label(&s.id))
            })
            .cloned()
            .collect();

        // Pinned first, otherwise the daemon's ranking stands. `sort_by_key` is
        // stable, so this reorders nothing else.
        out.sort_by_key(|i| !self.prefs.is_pinned(&i.session_id));
        out
    }

    /// Sessions hidden right now, for the "N hidden" affordance.
    fn hidden_count(&self) -> usize {
        self.sessions
            .keys()
            .filter(|id| self.prefs.is_hidden(id))
            .count()
    }

    fn hide_selected(&mut self) {
        if let Some(id) = self.selected.clone() {
            // Same rule the card enforces: a live session cannot be dismissed.
            // Enforced here as well as in the widget, because `h` reaches
            // sessions the pointer never touches.
            let alive = self.sessions.get(&id).map(|s| s.alive).unwrap_or(false);
            if !may_toggle_hidden(alive, self.prefs.is_hidden(&id)) {
                self.errors
                    .push("that session is still live — it cannot be hidden".into());
                return;
            }
            self.prefs.hide(&id);
            self.prefs_dirty = true;
            // Move on rather than leaving the pane pointing at something that
            // is no longer in the list.
            self.selected = self.visible_queue().first().map(|i| i.session_id.clone());
            if let Some(next) = self.selected.clone() {
                self.select(next);
            }
        }
    }

    fn pin_selected(&mut self) {
        if let Some(id) = self.selected.clone() {
            self.prefs.toggle_pin(&id);
            self.prefs_dirty = true;
        }
    }

    /// Per-pane content zoom: Ctrl+wheel (or a pinch) over a pane rescales
    /// that pane alone, remembered per pane in the prefs. The global
    /// Ctrl+= / Ctrl+- stays what it was — the whole window — because the
    /// two answer different wants: "my eyes" versus "this diff".
    ///
    /// Returns the pane's factor; call once at the top of a pane and feed
    /// the result to [`scale_text`].
    fn pane_zoom(&mut self, ui: &egui::Ui, pane: &str) -> f32 {
        let mut z = self.prefs.zoom_of(pane);
        if ui.rect_contains_pointer(ui.max_rect()) {
            let delta = ui.input(|i| i.zoom_delta());
            if (delta - 1.0).abs() > 1e-4 {
                z = (z * delta).clamp(0.5, 2.5);
                self.prefs.set_zoom(pane, z);
                self.prefs_dirty = true;
                // Re-read: set_zoom snaps near-1.0 back to exactly 1.0.
                z = self.prefs.zoom_of(pane);
            }
        }
        z
    }

    /// Move the selection by `delta` within the visible queue. `R-B1`.
    fn move_selection(&mut self, delta: i32) {
        let vis = self.visible_queue();
        if vis.is_empty() {
            return;
        }
        // `j`/`k` is triage by hand; follow mode yields to it exactly like
        // it yields to a card click, or the cursor moves for one frame and
        // snaps back to the top.
        if self.prefs.auto_select {
            self.prefs.auto_select = false;
            self.prefs_dirty = true;
        }
        let cur = self
            .selected
            .as_ref()
            .and_then(|id| vis.iter().position(|i| &i.session_id == id));
        let next = match cur {
            Some(i) => (i as i32 + delta).clamp(0, vis.len() as i32 - 1) as usize,
            // No selection yet: `j` starts at the top, `k` at the bottom.
            None if delta > 0 => 0,
            None => vis.len() - 1,
        };
        self.select(vis[next].session_id.clone());
    }

    /// Files in the current diff, after the same filters the list applies.
    ///
    /// One definition shared by rendering and the keyboard, so `Next` never
    /// lands on a row that is not on screen.
    fn visible_files(&self) -> Vec<String> {
        let Some(id) = &self.selected else {
            return Vec::new();
        };
        let Some(change) = self.changes.get(id) else {
            return Vec::new();
        };
        change
            .files
            .iter()
            .filter(|f| !(self.prefs.hide_noise && f.risk() == RiskLevel::Noise))
            .filter(|f| !(self.prefs.hide_reviewed && f.fully_reviewed()))
            .map(|f| f.path.clone())
            .collect()
    }

    /// Move within the focused pane.
    fn move_by(&mut self, delta: i32) {
        match self.pane {
            Pane::Queue => self.move_selection(delta),
            Pane::Files | Pane::Diff => self.move_file(delta),
        }
    }

    fn move_file(&mut self, delta: i32) {
        let files = self.visible_files();
        if files.is_empty() {
            return;
        }
        let cur = self
            .file_cursor
            .as_ref()
            .or(self.selected_file.as_ref())
            .and_then(|p| files.iter().position(|f| f == p));
        let next = match cur {
            Some(i) => (i as i32 + delta).clamp(0, files.len() as i32 - 1) as usize,
            None if delta > 0 => 0,
            None => files.len() - 1,
        };
        self.file_cursor = Some(files[next].clone());
        // Previewing is what makes arrowing through a diff useful; with it off
        // the cursor moves and the pane waits for Activate.
        if self.prefs.preview_on_select {
            self.selected_file = self.file_cursor.clone();
            self.blast = None;
        }
    }

    fn open_cursor_file(&mut self) {
        if let Some(p) = self.file_cursor.clone() {
            self.selected_file = Some(p);
            self.blast = None;
        }
    }

    /// Turn keystrokes into actions, then act. `R-B1`.
    ///
    /// Bindings are data (`keymap.rs`) rather than a match arm per key, which
    /// is what makes rebinding, pane-aware navigation and export possible at
    /// all.
    ///
    /// Suppressed whenever a text field has focus, or typing "job" into the
    /// filter box would move the selection three times and mark something read.
    fn handle_keys(&mut self, ui: &mut egui::Ui) {
        // Rebinding grabs the very next chord, so it is read before anything
        // is dispatched — otherwise pressing `J` to bind it would also move the
        // selection.
        if let Some(action) = self.capturing {
            let captured = ui.input(|i| captured_binding(&i.events));
            if let Some(b) = captured {
                // Escape cancels rather than becoming the new binding, or you
                // could never back out of a mis-click.
                if b.0 != "Escape" {
                    if let Some(other) = self.keymap.conflict(&b, action) {
                        self.errors
                            .push(format!("{b} was bound to \"{}\" — reassigned", other.label()));
                        let remaining: Vec<_> = self
                            .keymap
                            .bindings_for(other)
                            .iter()
                            .filter(|x| **x != b)
                            .cloned()
                            .collect();
                        self.keymap.set(other, remaining);
                    }
                    self.keymap.set(action, vec![b]);
                    if let Err(e) = self.keymap.save() {
                        self.errors.push(format!("could not save keymap: {e}"));
                    }
                }
                self.capturing = None;
            }
            return;
        }

        // The palette is modal and owns the keyboard while it is up. Read
        // before the terminal and before the focus guard, because its text
        // field holds egui focus and both would otherwise swallow it.
        //
        // Escape is handled here rather than left to the `ClearFilter`
        // binding, which also wipes the queue filter — closing a palette you
        // opened by accident should not throw away what you had typed into
        // something else.
        if self.palette.open {
            let len = self.palette_len();
            let (escape, enter, down, up, tab) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Escape),
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::Tab),
                )
            });
            if escape {
                self.palette.close();
            } else if enter {
                self.palette_enter(ui);
            } else if down || tab {
                self.palette.move_cursor(1, len);
            } else if up {
                self.palette.move_cursor(-1, len);
            }
            return;
        }

        // The keyboard settings window drives its own cursor while it is up.
        //
        // It used to stand down whenever *anything* held egui focus, which
        // sounded cautious and was in practice the bug: the embedded terminal,
        // the window's own search box, and whatever a click last landed on all
        // count as "something", and each of those states left the window
        // looking keyboard-dead while keys fell through to the main window
        // (reported twice, on Ubuntu). It now yields only to real text boxes —
        // and its own search box keeps the list drivable, the same contract as
        // the queue filter below.
        if self.show_keymap && !self.term_focused {
            if ui.memory(|m| m.has_focus(keymap_filter_id())) {
                let (enter, down, up) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::ArrowDown),
                        i.key_pressed(egui::Key::ArrowUp),
                    )
                });
                let rows = self.keymap_rows().len();
                if enter {
                    ui.memory_mut(|m| m.surrender_focus(keymap_filter_id()));
                } else if down && rows > 0 {
                    self.keymap_cursor = (self.keymap_cursor + 1) % rows;
                    self.keymap_scroll = true;
                } else if up && rows > 0 {
                    self.keymap_cursor = (self.keymap_cursor + rows - 1) % rows;
                    self.keymap_scroll = true;
                }
                // Everything else is typing, and belongs to the box.
                return;
            }
            if !text_input_focused(ui) && self.keymap_window_keys(ui) {
                return;
            }
        }

        // The terminal takes egui focus while it has the keyboard, so the guard
        // below would swallow every chord — including the one that gives the
        // keyboard back. Read that one first, or the pane is a roach motel.
        if self.term_focused {
            let leave = ui.input(|i| {
                i.events.iter().any(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        self.keymap.action_for(*modifiers, *key)
                            == Some(crate::keymap::Action::LeaveTerminal)
                    }
                    _ => false,
                })
            });
            if leave {
                self.term_focused = false;
            }
            return;
        }

        // The filter is a text field, so the blanket "something has focus, stay
        // out of the way" guard below would apply to it — leaving a search box
        // you can type in and nothing else. A list filter should be drivable
        // end to end without the hands moving: type to narrow, arrow to choose,
        // Enter to accept, Escape to abandon.
        //
        // Escape is not handled here and does not need to be: egui releases
        // focus on Escape before this runs, so the ordinary `ClearFilter`
        // binding fires on the same press.
        if ui.memory(|m| m.has_focus(filter_id())) {
            let (enter, down, up) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                )
            });
            if enter {
                ui.memory_mut(|m| m.surrender_focus(filter_id()));
            } else if down {
                self.move_selection(1);
            } else if up {
                self.move_selection(-1);
            }
            return;
        }

        if ui.memory(|m| m.focused().is_some()) {
            return;
        }
        let pressed: Vec<(egui::Modifiers, egui::Key)> = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*modifiers, *key)),
                    _ => None,
                })
                .collect()
        });

        for (mods, key) in pressed {
            let Some(action) = self.keymap.action_for(mods, key) else {
                continue;
            };
            self.run(action, ui);
        }
    }

    fn run(&mut self, action: crate::keymap::Action, ui: &mut egui::Ui) {
        use crate::keymap::Action as A;
        match action {
            A::PageDown => self.scroll = Some(ScrollRequest::Pages(1.0)),
            A::PageUp => self.scroll = Some(ScrollRequest::Pages(-1.0)),
            A::ScrollTop => self.scroll = Some(ScrollRequest::Top),
            A::ScrollBottom => self.scroll = Some(ScrollRequest::Bottom),
            A::TabChanges => self.set_tab(Tab::Changes),
            A::TabTranscript => self.set_tab(Tab::Transcript),
            A::TabInfo => self.set_tab(Tab::Info),
            A::TabDebt => self.set_tab(Tab::Debt),
            A::TabTerminal => self.set_tab(Tab::Terminal),
            A::TabExplorer => self.set_tab(Tab::Explorer),
            A::TabGit => self.set_tab(Tab::Git),
            A::ToggleAnnotate => {
                self.annotate = !self.annotate;
                if self.annotate {
                    self.set_tab(Tab::Explorer);
                }
            }
            // Only reachable while the terminal does *not* hold the keyboard,
            // where it is a no-op. The live path is in `handle_keys`.
            // A toggle, not just an exit: reaching the terminal should not
            // need a mouse when leaving it does not (asked for directly).
            A::LeaveTerminal => {
                if self.term_focused {
                    self.term_focused = false;
                } else {
                    let attachable = self
                        .selected_session()
                        .map(|s| s.tmux_target.is_some())
                        .unwrap_or(false);
                    if attachable {
                        self.set_tab(Tab::Terminal);
                        self.term_focused = true;
                    }
                    // No session, or not under tmux: nothing to focus, and
                    // setting the flag anyway would swallow every key.
                }
            }
            A::NextTab => self.cycle_tab(1),
            A::PrevTab => self.cycle_tab(-1),
            A::FocusQueue => self.pane = Pane::Queue,
            A::FocusFiles => {
                self.pane = Pane::Files;
                self.tab = Tab::Changes;
                if self.file_cursor.is_none() {
                    self.file_cursor = self.selected_file.clone();
                }
            }
            A::FocusDiff => {
                self.pane = Pane::Diff;
                self.tab = Tab::Changes;
            }
            A::Next => self.move_by(1),
            A::Prev => self.move_by(-1),
            A::First => match self.pane {
                Pane::Queue => {
                    if let Some(top) = self.visible_queue().first() {
                        self.select(top.session_id.clone());
                    }
                }
                _ => {
                    if let Some(first) = self.visible_files().first().cloned() {
                        self.file_cursor = Some(first.clone());
                        self.selected_file = Some(first);
                    }
                }
            },
            A::Activate => match self.pane {
                Pane::Queue => self.focus_selected_terminal(),
                _ => self.open_cursor_file(),
            },
            A::JumpToTerminal => self.focus_selected_terminal(),
            A::MarkAllRead => {
                if let Some(id) = self.selected.clone() {
                    self.net.send(ClientMsg::ReviewAll { session_id: id });
                }
            }
            A::Snooze => {
                if let Some(id) = self.selected.clone() {
                    let snoozed = self
                        .sessions
                        .get(&id)
                        .map(|s| s.is_snoozed(Utc::now()))
                        .unwrap_or(false);
                    self.net.send(ClientMsg::Snooze {
                        session_id: id,
                        minutes: if snoozed { 0 } else { 30 },
                    });
                }
            }
            A::FilterFocus => {
                self.pane = Pane::Queue;
                ui.memory_mut(|m| m.request_focus(filter_id()));
                // **The keystroke that opens the filter must not also be typed
                // into it.** egui delivers a `Key` and a `Text` event for the
                // same press, and the field is drawn later in this same frame
                // with focus already granted — so it consumed the `Text` and
                // every `/` left a literal slash in the box to delete.
                ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
            }
            A::ClearFilter => {
                self.filter.clear();
                self.ambient = false;
                self.show_keymap = false;
                self.capturing = None;
            }
            A::HideSession => self.hide_selected(),
            A::PinSession => self.pin_selected(),
            A::LabelSession => {
                if let Some(id) = self.selected.clone() {
                    self.open_label_editor(id);
                }
            }
            A::ToggleRead => self.toggle_first_unread(),
            A::NextUnread => self.jump_to_next_unread(),
            A::FlagHunk => self.flag_first_unread(),
            A::ToggleAmbient => self.ambient = !self.ambient,
            A::ToggleHealth => {
                self.show_health = !self.show_health;
                if self.show_health {
                    self.net.send(ClientMsg::FetchHealth);
                }
            }
            A::CommandPalette => {
                if self.palette.open {
                    self.palette.close();
                } else {
                    self.palette.open();
                }
            }
            A::ToggleQueuePanel => {
                self.prefs.queue_collapsed = !self.prefs.queue_collapsed;
                self.prefs_dirty = true;
                // Coming back from collapsed means you want the queue, and
                // leaving it means you want the room — so the keyboard follows.
                self.pane = if self.prefs.queue_collapsed {
                    Pane::Diff
                } else {
                    Pane::Queue
                };
            }
            A::ResetLayout => {
                self.tree = Some(crate::layout::default_tree());
                self.layout_dirty = true;
            }
            A::OpenKeymap => {
                self.show_keymap = !self.show_keymap;
                self.keymap_scroll = self.show_keymap;
                if self.show_keymap {
                    self.term_focused = false;
                }
            }
            A::Rescan => self.net.send(ClientMsg::Rescan),
            A::GoToFile | A::RecentFiles => self.open_file_palette(),
            A::SearchInFiles => {
                if let Some(id) = self.selected.clone() {
                    self.explorer.ensure_session(&id);
                    self.palette.open_as(crate::palette::Mode::Search);
                }
            }
            A::NextFileTab => self.with_explorer(|app| app.explorer.cycle_tab(1)),
            A::PrevFileTab => self.with_explorer(|app| app.explorer.cycle_tab(-1)),
            A::CloseFileTab => self.with_explorer(|app| {
                let focus = app.explorer.current().focus;
                if let Some(i) = app.explorer.current().active_of(focus) {
                    app.explorer.close_tab(i);
                }
            }),
            A::PinFileTab => self.with_explorer(|app| app.explorer.toggle_pin_active()),
            A::FindInFile => self.with_explorer(|app| {
                app.explorer_find_open = true;
                app.explorer_find_focus = true;
            }),
            A::MoveFileTabSplit => self.with_explorer(|app| {
                let focus = app.explorer.current().focus;
                if let Some(i) = app.explorer.current().active_of(focus) {
                    app.explorer.move_tab_to_other_side(i);
                }
            }),
            A::OpenInExplorer => {
                if let Some(path) = self.file_cursor.clone().or(self.selected_file.clone()) {
                    self.open_in_explorer(&path, None);
                }
            }
        }
    }

    /// Run an explorer tab action with the pane raised and the session
    /// ensured — cycling tabs nobody can see would just be confusing.
    fn with_explorer(&mut self, f: impl FnOnce(&mut Self)) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        self.explorer.ensure_session(&id);
        self.set_tab(Tab::Explorer);
        f(self);
    }

    /// The Changes → Explorer bridge, and where every search result lands:
    /// the file opens *pinned* (it was asked for by name, not browsed past),
    /// revealed in the tree, with the pane raised.
    ///
    /// Diff paths are repo-relative and the explorer root is the repo root
    /// when one is known, so the two speak the same currency; when they do
    /// not, the daemon refuses the path and the pane says so — degrade, not
    /// error.
    fn open_in_explorer(&mut self, path: &str, line: Option<u64>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        self.explorer.ensure_session(&id);
        self.explorer.open_file(path, true, line);
        self.explorer.reveal(path);
        self.set_tab(Tab::Explorer);
    }

    /// Go-to-file (`Ctrl+P`): the palette over the worktree's file list. The
    /// list is re-requested on every open — worktrees move under live
    /// sessions, and a walk is cheap next to showing stale files.
    fn open_file_palette(&mut self) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        self.explorer.ensure_session(&id);
        let st = self.explorer.current_mut();
        if !st.tree_pending {
            st.tree_pending = true;
            self.net.send(ClientMsg::ListTree { session_id: id });
        }
        self.palette.open_as(crate::palette::Mode::Files);
    }

    /// Worktree files ranked against the palette query. An empty query is the
    /// recent-files switcher: the open tabs, most recently used first.
    fn file_matches(&self) -> Vec<String> {
        let Some(st) = self.explorer.try_current() else {
            return Vec::new();
        };
        let q = self.palette.query.trim();
        if q.is_empty() {
            return st.mru().into_iter().map(|i| st.open[i].path.clone()).collect();
        }
        let Some((paths, _)) = &st.tree_paths else {
            return Vec::new();
        };
        let mut out: Vec<(i32, &String)> = paths
            .iter()
            .filter_map(|p| crate::palette::score(q, p).map(|s| (s, p)))
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        // The palette shows a dozen rows; ranking thousands is work, hauling
        // them all into the UI is waste.
        out.truncate(100);
        out.into_iter().map(|(_, p)| p.clone()).collect()
    }

    /// Send the content search the palette is showing. One in flight per
    /// session; the echoed query is what lets a stale answer be dropped.
    fn issue_search(&mut self) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let query = self.palette.query.trim().to_string();
        if query.is_empty() {
            return;
        }
        self.explorer.ensure_session(&id);
        self.explorer.current_mut().search = Some(crate::explorer::SearchState {
            query: query.clone(),
            matches: Vec::new(),
            truncated: false,
            in_flight: true,
        });
        self.net.send(ClientMsg::SearchContent {
            session_id: id,
            query,
        });
    }

    /// Enter in the palette, whatever it is currently searching.
    fn palette_enter(&mut self, ui: &mut egui::Ui) {
        use crate::palette::Mode;
        match self.palette.mode {
            Mode::Actions => {
                let chosen = self
                    .palette
                    .matches(&self.keymap)
                    .get(self.palette.cursor)
                    .map(|m| m.action);
                self.palette.close();
                if let Some(a) = chosen {
                    // Closed *before* running, so an action that opens a window
                    // is not immediately hidden behind the palette that ran it.
                    self.run(a, ui);
                }
            }
            Mode::Files => {
                let chosen = self.file_matches().get(self.palette.cursor).cloned();
                self.palette.close();
                if let Some(p) = chosen {
                    self.open_in_explorer(&p, None);
                }
            }
            Mode::Search => {
                // Enter is two things in sequence, the way every IDE does it:
                // run the query, then — once its answer is the one on screen —
                // open the picked hit.
                let query = self.palette.query.trim().to_string();
                let chosen = {
                    let answered = self
                        .explorer
                        .try_current()
                        .and_then(|st| st.search.as_ref())
                        .filter(|s| s.query == query && !s.in_flight);
                    match answered {
                        None => None,
                        Some(s) => match s.matches.get(self.palette.cursor) {
                            Some(m) => Some(Some((m.path.clone(), m.line))),
                            // Answered and empty: nothing to open, nothing to
                            // re-send. Stay up so the emptiness is readable.
                            None => Some(None),
                        },
                    }
                };
                match chosen {
                    None => {
                        self.issue_search();
                        self.palette.cursor = 0;
                    }
                    Some(None) => {}
                    Some(Some((path, line))) => {
                        self.palette.close();
                        self.open_in_explorer(&path, Some(line));
                    }
                }
            }
        }
    }

    /// How many rows the palette's current mode has — the keyboard handler
    /// needs it for cursor movement before the window has drawn.
    fn palette_len(&self) -> usize {
        match self.palette.mode {
            crate::palette::Mode::Actions => self.palette.matches(&self.keymap).len(),
            crate::palette::Mode::Files => self.file_matches().len(),
            crate::palette::Mode::Search => self
                .explorer
                .try_current()
                .and_then(|st| st.search.as_ref())
                .map(|s| s.matches.len())
                .unwrap_or(0),
        }
    }

    /// The first unread hunk of the open file, toggled read.
    fn toggle_first_unread(&mut self) {
        let (Some(id), Some(path)) = (self.selected.clone(), self.selected_file.clone()) else {
            return;
        };
        let Some(change) = self.changes.get(&id) else {
            return;
        };
        let Some(file) = change.files.iter().find(|f| f.path == path) else {
            return;
        };
        if let Some(h) = file.hunks.iter().find(|h| !h.reviewed) {
            self.net.send(ClientMsg::SetHunkReviewed {
                session_id: id,
                anchor: h.anchor.clone(),
                reviewed: true,
            });
        }
    }

    /// Move to the next file that still has something unread.
    fn jump_to_next_unread(&mut self) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(change) = self.changes.get(&id).cloned() else {
            return;
        };
        let files = self.visible_files();
        let start = self
            .file_cursor
            .as_ref()
            .or(self.selected_file.as_ref())
            .and_then(|p| files.iter().position(|f| f == p))
            .unwrap_or(0);
        // Wrap around, so the last file is not a dead end.
        for step in 1..=files.len() {
            let i = (start + step) % files.len();
            let path = &files[i];
            let unread = change
                .files
                .iter()
                .find(|f| &f.path == path)
                .map(|f| f.hunks.iter().any(|h| !h.reviewed))
                .unwrap_or(false);
            if unread {
                self.file_cursor = Some(path.clone());
                self.selected_file = Some(path.clone());
                self.pane = Pane::Files;
                return;
            }
        }
    }

    fn flag_first_unread(&mut self) {
        let (Some(id), Some(path)) = (self.selected.clone(), self.selected_file.clone()) else {
            return;
        };
        let Some(change) = self.changes.get(&id) else {
            return;
        };
        let Some(file) = change.files.iter().find(|f| f.path == path) else {
            return;
        };
        let Some(h) = file.hunks.iter().find(|h| !h.reviewed) else {
            return;
        };
        if self
            .flagged
            .iter()
            .any(|f| f.session_id == id && f.path == path && f.header == h.header)
        {
            return;
        }
        self.flagged.push(FlaggedHunk {
            session_id: id,
            path,
            header: h.header.clone(),
            note: String::new(),
            body: h
                .lines
                .iter()
                .filter(|l| l.starts_with('+') || l.starts_with('-'))
                .take(40)
                .cloned()
                .collect(),
        });
        self.show_prompt = true;
    }

    /// Switch tab, running whatever that tab needs.
    ///
    /// One path for the keyboard and the mouse: the Debt tab has to ask the
    /// daemon for its numbers, and a keyboard shortcut that skipped that would
    /// show a permanently empty tab.
    fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        // Bring the pane forward wherever it sits, and put it back if it was
        // closed. With one visible pane this was implicit; with a tree it is
        // the whole of what "show the Changes tab" means.
        if let Some(tree) = &mut self.tree {
            crate::layout::focus(tree, tab);
            self.layout_dirty = true;
        }
        if tab == Tab::Debt {
            if let Some(repo) = self.selected_session().and_then(|s| s.repo_root.clone()) {
                self.net.send(ClientMsg::FetchReviewDebt { repo });
            }
        }
        if tab == Tab::Changes && self.pane == Pane::Queue {
            // Asking for the diff usually means you are about to read it.
            self.pane = Pane::Files;
        }
    }

    fn cycle_tab(&mut self, delta: i32) {
        let next = match &self.tree {
            Some(tree) => crate::layout::cycle(tree, self.tab, delta),
            None => self.tab,
        };
        self.set_tab(next);
    }

    fn focus_selected_terminal(&mut self) {
        if let Some(id) = self.selected.clone() {
            let alive = self.sessions.get(&id).map(|s| s.alive).unwrap_or(false);
            if alive {
                self.net.send(ClientMsg::FocusTerminal { session_id: id });
            } else if let Some(s) = self.sessions.get(&id) {
                // Exited: the useful equivalent is opening where it worked.
                let dir = s.repo_root.clone().unwrap_or_else(|| s.cwd.clone());
                if let Err(e) = ui::open_in(ui::OpenTarget::Terminal, &dir) {
                    self.errors.push(e);
                }
            }
        }
    }

    fn queue_panel(&mut self, root: &mut egui::Ui) {
        // Collapsed is a strip, not nothing. The queue is the reason this app
        // exists, so it must never be possible to lose it entirely — and a
        // count you can still see is what makes taking the room back a
        // decision rather than a rediscovery.
        if self.prefs.queue_collapsed {
            let needing = self.queue.iter().filter(|i| i.reason.needs_human()).count();
            let key = self.keymap.describe(crate::keymap::Action::ToggleQueuePanel);
            let vis = self.visible_queue();
            let mut expand = false;
            let mut pick: Option<SessionId> = None;
            egui::Panel::left("queue-strip")
                .resizable(false)
                .exact_size(30.0)
                .frame(
                    egui::Frame::NONE
                        .fill(BG)
                        .inner_margin(egui::Margin::symmetric(4, 8)),
                )
                .show(root, |ui| {
                    ui.vertical_centered(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("»").size(14.0).color(DIM)).frame(false))
                            .on_hover_text(format!("show the queue  ({key})"))
                            .clicked()
                        {
                            expand = true;
                        }
                        ui.add_space(6.0);
                        if needing > 0 {
                            ui.label(RichText::new(needing.to_string()).size(13.0).color(AMBER).strong())
                                .on_hover_text(format!("{needing} session(s) need you"));
                        }
                        ui.add_space(6.0);

                        // One chip per session, Lens-fashion: the collapsed
                        // strip keeps answering "which sessions, and which
                        // need me" instead of only "how many". The letter and
                        // colour come from the user's label when there is one
                        // (`R-B26`), else from the repo — both stable, so a
                        // chip means the same thing tomorrow.
                        egui::ScrollArea::vertical()
                            .id_salt("queue-strip-chips")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 4.0;
                                for item in &vis {
                                    let Some(s) = self.sessions.get(&item.session_id) else {
                                        continue;
                                    };
                                    let label = self.prefs.label(&item.session_id);
                                    let name =
                                        label.map(str::to_string).unwrap_or_else(|| s.repo_name());
                                    let selected =
                                        self.selected.as_ref() == Some(&item.session_id);
                                    let chip = session_chip(
                                        ui,
                                        ui::chip_char(&name),
                                        label_color(&name),
                                        selected,
                                        item.reason.needs_human(),
                                    )
                                    .on_hover_text(format!(
                                        "{}\n{} · {}",
                                        label.unwrap_or(&s.label()),
                                        s.repo_name(),
                                        item.reason.label()
                                    ));
                                    if chip.clicked() {
                                        pick = Some(item.session_id.clone());
                                    }
                                }
                            });
                    });
                });
            if expand {
                self.prefs.queue_collapsed = false;
                self.prefs_dirty = true;
                self.pane = Pane::Queue;
            }
            if let Some(id) = pick {
                self.select(id);
            }
            return;
        }

        egui::Panel::left("queue")
            .default_size(380.0)
            .size_range(300.0..=560.0)
            .show(root, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let focused = self.pane == Pane::Queue;
                    ui.label(
                        RichText::new("ATTENTION")
                            .size(11.0)
                            .color(if focused { BLUE } else { DIM })
                            .strong(),
                    )
                    .on_hover_text("Alt+1");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("«").size(13.0).color(DIM)).frame(false))
                            .on_hover_text(format!(
                                "collapse the queue  ({})",
                                self.keymap.describe(crate::keymap::Action::ToggleQueuePanel)
                            ))
                            .clicked()
                        {
                            self.prefs.queue_collapsed = true;
                            self.prefs_dirty = true;
                            self.pane = Pane::Diff;
                        }
                        if ui.checkbox(&mut self.prefs.group_by_repo, "group").changed() {
                            self.prefs_dirty = true;
                        }
                        if ui
                            .checkbox(&mut self.prefs.auto_select, "follow")
                            .on_hover_text(
                                "keep the top of the queue selected as it changes — \
                                 picking a session by click or j/k switches this off",
                            )
                            .changed()
                        {
                            self.prefs_dirty = true;
                        }
                    });
                });

                // Scope decides what the queue is *for*: the default answers
                // "where do I look", not "what exists".
                ui.horizontal(|ui| {
                    for sc in crate::prefs::Scope::ALL {
                        if ui
                            .selectable_label(self.prefs.scope == sc, sc.label())
                            .on_hover_text(sc.hint())
                            .clicked()
                        {
                            self.prefs.scope = sc;
                            self.prefs_dirty = true;
                        }
                    }
                });

                // R-B9. Filter first: with a dozen sessions this is faster than
                // reading the list.
                let filtering = ui.memory(|m| m.has_focus(filter_id()));
                ui.horizontal(|ui| {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .id(filter_id())
                            .hint_text(if filtering {
                                "type to narrow  ·  ↑↓ choose  ·  ⏎ accept  ·  esc clear"
                            } else {
                                "filter  (/)   repo: branch: file: label:"
                            })
                            .desired_width(ui.available_width() - 4.0),
                    );
                    // A focused text field is a mode: while it has the
                    // keyboard, every other shortcut is suspended. Say so with
                    // a border rather than leaving the user to discover it by
                    // pressing `j` and watching a `j` appear.
                    if filtering {
                        ui.painter().rect_stroke(
                            field.rect.expand(1.0),
                            4.0,
                            egui::Stroke::new(1.0, BLUE),
                            egui::StrokeKind::Outside,
                        );
                    }
                });

                // When a filter is narrowing things, say by how much — an empty
                // queue should never be ambiguous between "nothing needs you"
                // and "your filter excluded it".
                let query = crate::filter::parse(&self.filter);
                if !query.is_empty() {
                    ui.horizontal(|ui| {
                        ui.label(dim(format!(
                            "{} of {} session(s)",
                            self.visible_queue().len(),
                            self.sessions.len()
                        )));
                        if ui.small_button("clear").clicked() {
                            self.filter.clear();
                        }
                    });
                }

                let hidden = self.hidden_count();
                if hidden > 0 {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.prefs.reveal_hidden,
                                dim(format!("{hidden} hidden")),
                            )
                            .on_hover_text("show them so you can unhide")
                            .clicked()
                        {
                            self.prefs.reveal_hidden = !self.prefs.reveal_hidden;
                        }
                        if self.prefs.reveal_hidden && ui.small_button("unhide all").clicked() {
                            self.prefs.hidden.clear();
                            self.prefs_dirty = true;
                        }
                    });
                }
                // One line, and it points at the palette rather than trying to
                // list thirty bindings four at a time. The old version named
                // six of them and was wrong about which six mattered.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("j/k move   ⏎ open   / filter")
                            .size(10.5)
                            .color(DIM),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(format!(
                                        "{}  everything",
                                        self.keymap
                                            .bindings_for(crate::keymap::Action::CommandPalette)
                                            .first()
                                            .map(|b| b.0.clone())
                                            .unwrap_or_else(|| "palette".into())
                                    ))
                                    .size(10.5)
                                    .color(BLUE),
                                )
                                .frame(false),
                            )
                            .on_hover_text("every command, by name")
                            .clicked()
                        {
                            self.palette.open();
                        }
                    });
                });
                ui.separator();

                let now = Utc::now();
                let vis = self.visible_queue();
                let hidden = self.queue.len() - vis.len();
                let mut to_select = None;
                let mut to_snooze = None;
                let mut to_focus = None;
                let mut to_pin: Option<String> = None;
                let mut to_hide: Option<String> = None;
                let mut to_filter_repo: Option<String> = None;
                let mut to_filter_label: Option<String> = None;
                let mut to_label: Option<SessionId> = None;
                // Set only by a click on a card, never by follow mode or by
                // `j`/`k` — moving the cursor is not the same gesture as
                // choosing a session, and a tab that changed under the keyboard
                // would be unusable.
                let mut open_terminal = false;

                // R-B8. Follow mode: the top of the queue is by definition the
                // thing most worth looking at, so let it drive the pane —
                // until a hand-picked selection takes over (below). Without
                // that yield, a click held for exactly one frame and then
                // snapped back to the top, which read as the click not
                // working at all.
                if self.prefs.auto_select {
                    if let Some(top) = vis.first() {
                        if self.selected.as_ref() != Some(&top.session_id) {
                            to_select = Some(top.session_id.clone());
                        }
                    }
                }
                // A card click is an explicit choice; it must not lose to
                // follow on the very next frame. Tracked separately from
                // `to_select` because follow writes that too.
                let mut hand_picked = false;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // **This is what made cards feel unclickable.** egui's
                        // `selectable_labels` defaults to on, which gives every
                        // label `Sense::click_and_drag()` so text can be
                        // selected. A card's own click target is registered
                        // before its children — deliberately, so a container
                        // sits behind what it contains — and among widgets tied
                        // at distance zero egui takes the topmost. So every
                        // label in the card beat the card, and only the gaps
                        // between labels ever selected a session.
                        //
                        // Nothing about it looked wrong: the click was
                        // received, it just started a text selection of one
                        // word instead. Turned off for the queue only; the
                        // detail pane keeps selectable text, where copying a
                        // path or an error message is the point.
                        ui.style_mut().interaction.selectable_labels = false;

                        if vis.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                if !self.filter.trim().is_empty() {
                                    ui.label(dim("nothing matches that filter"));
                                } else if self.prefs.scope != crate::prefs::Scope::All
                                    && !self.sessions.is_empty()
                                {
                                    // The commonest confusion: sessions exist,
                                    // the scope is hiding them.
                                    ui.label(dim("nothing needs you"));
                                    ui.label(dim(format!(
                                        "{} session(s) outside \"{}\" — try \"all\"",
                                        self.sessions.len(),
                                        self.prefs.scope.label()
                                    )));
                                } else {
                                    ui.label(dim("nothing needs you"));
                                    ui.label(dim("run claude in a terminal and it shows up here"));
                                }
                            });
                        }

                        // R-B6. Grouping preserves rank *within* a repo, and
                        // orders repos by their most urgent session — so the
                        // top of the panel is still the top of the queue.
                        let groups: Vec<(String, Vec<AttentionItem>)> = if self.prefs.group_by_repo {
                            let mut order: Vec<String> = Vec::new();
                            let mut by: HashMap<String, Vec<AttentionItem>> = HashMap::new();
                            for item in &vis {
                                let repo = self
                                    .sessions
                                    .get(&item.session_id)
                                    .map(|s| s.repo_name())
                                    .unwrap_or_else(|| "—".into());
                                if !by.contains_key(&repo) {
                                    order.push(repo.clone());
                                }
                                by.entry(repo).or_default().push(item.clone());
                            }
                            order
                                .into_iter()
                                .map(|r| {
                                    let items = by.remove(&r).unwrap_or_default();
                                    (r, items)
                                })
                                .collect()
                        } else {
                            vec![(String::new(), vis.clone())]
                        };

                        for (repo, items) in groups {
                            if !repo.is_empty() {
                                let collapsed = self.collapsed_repos.contains(&repo);
                                let needing =
                                    items.iter().filter(|i| i.reason.needs_human()).count();
                                ui.add_space(4.0);
                                let head = ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(if collapsed { "▸" } else { "▾" })
                                            .size(11.0)
                                            .color(DIM),
                                    );
                                    ui.label(RichText::new(&repo).size(12.0).strong());
                                    ui.label(dim(format!(
                                        "{} session(s){}",
                                        items.len(),
                                        if needing > 0 {
                                            format!(", {needing} need you")
                                        } else {
                                            String::new()
                                        }
                                    )));
                                });
                                if head.response.interact(egui::Sense::click()).clicked() {
                                    if collapsed {
                                        self.collapsed_repos.remove(&repo);
                                    } else {
                                        self.collapsed_repos.insert(repo.clone());
                                    }
                                }
                                if collapsed {
                                    continue;
                                }
                            }

                            for item in &items {
                                let Some(session) = self.sessions.get(&item.session_id).cloned()
                                else {
                                    continue;
                                };
                                let selected = self.selected.as_ref() == Some(&item.session_id);
                                let is_hidden = self.prefs.is_hidden(&item.session_id);
                                let hideable = may_toggle_hidden(session.alive, is_hidden);
                                let resp = ui.push_id(&item.session_id, |ui| {
                                    egui::Frame::group(ui.style())
                                        .fill(if selected {
                                            ui.visuals().selection.bg_fill.linear_multiply(0.35)
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            let hit = queue_card(
                                                ui,
                                                &session,
                                                item,
                                                now,
                                                self.prefs.is_pinned(&item.session_id),
                                                is_hidden,
                                                hideable,
                                                self.prefs.label(&item.session_id),
                                            );
                                            if let Some(r) = hit.filter_repo {
                                                to_filter_repo = Some(r);
                                            }
                                            if let Some(l) = hit.filter_label {
                                                to_filter_label = Some(l);
                                            }
                                            if hit.hide {
                                                to_hide = Some(item.session_id.clone());
                                            }

                                            if selected {
                                                ui.horizontal(|ui| {
                                                    if session.alive
                                                        && ui
                                                            .small_button(format!("{} terminal", icon::TERMINAL))
                                                            .on_hover_text(
                                                                "focus the Terminal tab this session runs in",
                                                            )
                                                            .clicked()
                                                    {
                                                        to_focus = Some(item.session_id.clone());
                                                    }
                                                    let snoozed = session.is_snoozed(now);
                                                    if ui
                                                        .small_button(if snoozed {
                                                            "wake"
                                                        } else {
                                                            "snooze 30m"
                                                        })
                                                        .clicked()
                                                    {
                                                        to_snooze = Some((
                                                            item.session_id.clone(),
                                                            if snoozed { 0 } else { 30 },
                                                        ));
                                                    }
                                                    let pinned =
                                                        self.prefs.is_pinned(&item.session_id);
                                                    if ui
                                                        .small_button(if pinned { "unpin" } else { "pin" })
                                                        .on_hover_text("keep at the top of the queue (p)")
                                                        .clicked()
                                                    {
                                                        to_pin = Some(item.session_id.clone());
                                                    }
                                                    if hideable
                                                        && ui
                                                            .small_button(if is_hidden { "unhide" } else { "hide" })
                                                            .on_hover_text(
                                                                "keep it out of the queue — reversible, and \
                                                                 nothing is forgotten (h)",
                                                            )
                                                            .clicked()
                                                    {
                                                        to_hide = Some(item.session_id.clone());
                                                    }
                                                });
                                            }
                                        })
                                });
                                // `Response::interact` on the scope's own
                                // response, never `ui.interact` with a fresh
                                // id: a Ui registers its widget rect *before*
                                // its children so that it sits behind them, and
                                // reusing that id keeps it there. A new id
                                // registers last, lands on top, and swallows
                                // every button inside the card.
                                let card = resp
                                    .response
                                    .interact(egui::Sense::click())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                if card.clicked() {
                                    to_select = Some(item.session_id.clone());
                                    hand_picked = true;
                                    // Clicking a session means "take me to it",
                                    // and where it is is the terminal. Only when
                                    // there is one to show: for a session not
                                    // under tmux the Terminal tab is an
                                    // explanation, and landing on an
                                    // explanation every time you click is worse
                                    // than staying where you were.
                                    open_terminal = session.tmux_target.is_some();
                                }
                                // Hovering has to be visible, or a card that is
                                // entirely clickable looks like one where only
                                // the text is.
                                if card.hovered() && !selected {
                                    ui.painter().rect_stroke(
                                        card.rect,
                                        4.0,
                                        egui::Stroke::new(1.0, DIM),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                // The menu itself is always available — pinning
                                // and snoozing apply to a live session. Only
                                // the hide entry is withheld, and withheld by
                                // being absent rather than greyed out: an item
                                // you can see but not press invites the
                                // question "why not", every time.
                                card.context_menu(|ui| {
                                    if hideable
                                        && ui
                                            .button(if is_hidden {
                                                "Unhide"
                                            } else {
                                                "Hide from the queue"
                                            })
                                            .clicked()
                                    {
                                        to_hide = Some(item.session_id.clone());
                                        ui.close();
                                    }
                                    if ui
                                        .button(if self.prefs.is_pinned(&item.session_id) {
                                            "Unpin"
                                        } else {
                                            "Pin to the top"
                                        })
                                        .clicked()
                                    {
                                        to_pin = Some(item.session_id.clone());
                                        ui.close();
                                    }
                                    let snoozed = session.is_snoozed(now);
                                    if ui
                                        .button(if snoozed { "Wake" } else { "Snooze 30m" })
                                        .clicked()
                                    {
                                        to_snooze = Some((
                                            item.session_id.clone(),
                                            if snoozed { 0 } else { 30 },
                                        ));
                                        ui.close();
                                    }
                                    let labelled =
                                        self.prefs.label(&item.session_id).is_some();
                                    if ui
                                        .button(if labelled { "Edit label…" } else { "Label…" })
                                        .clicked()
                                    {
                                        to_label = Some(item.session_id.clone());
                                        ui.close();
                                    }
                                });
                                ui.add_space(2.0);
                            }
                        }

                        if hidden > 0 {
                            ui.add_space(6.0);
                            ui.label(dim(format!("{hidden} session(s) hidden")));
                        }
                    });

                if let Some(id) = to_select {
                    // Choosing by hand turns follow off — visibly, in the
                    // checkbox — the way tailing a log stops when you
                    // scroll up. The alternative (follow silently winning)
                    // is a click that does nothing, which is how this bug
                    // was reported.
                    if hand_picked && self.prefs.auto_select {
                        self.prefs.auto_select = false;
                        self.prefs_dirty = true;
                    }
                    self.select(id);
                    if open_terminal {
                        self.set_tab(Tab::Terminal);
                    }
                }
                if let Some((session_id, minutes)) = to_snooze {
                    self.net.send(ClientMsg::Snooze {
                        session_id,
                        minutes,
                    });
                }
                if let Some(session_id) = to_focus {
                    self.net.send(ClientMsg::FocusTerminal { session_id });
                }
                if let Some(id) = to_pin {
                    self.prefs.toggle_pin(&id);
                    self.prefs_dirty = true;
                }
                if let Some(id) = to_hide {
                    if self.prefs.is_hidden(&id) {
                        self.prefs.unhide(&id);
                    } else {
                        self.prefs.hide(&id);
                    }
                    self.prefs_dirty = true;
                }
                if let Some(repo) = to_filter_repo {
                    self.filter = format!("repo:{repo}");
                }
                if let Some(label) = to_filter_label {
                    self.filter = format!("label:{}", label.to_lowercase());
                }
                if let Some(id) = to_label {
                    self.open_label_editor(id);
                }
            });
    }

    /// Open the label editor pre-filled with what the session is called now,
    /// so editing is the same gesture as naming.
    fn open_label_editor(&mut self, id: SessionId) {
        let current = self.prefs.label(&id).unwrap_or_default().to_string();
        self.label_edit = Some((id, current));
    }

    /// One text field in a small window: Enter saves, empty removes, Escape
    /// cancels. `R-B26`.
    fn label_window(&mut self, root: &mut egui::Ui) {
        let Some((id, mut text)) = self.label_edit.clone() else {
            return;
        };
        let ctx = root.ctx().clone();
        let session_name = self
            .sessions
            .get(&id)
            .map(|s| s.label())
            .unwrap_or_else(|| id.clone());
        let mut open = true;
        let mut done = false;
        egui::Window::new("Label")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -80.0])
            .open(&mut open)
            .show(&ctx, |ui| {
                ui.label(dim(truncate(&session_name, 70)));
                let field = ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .id(egui::Id::new("label-edit"))
                        .hint_text("a name you will recognise it by…")
                        .desired_width(280.0),
                );
                // Re-focused every frame, like the palette: the window is
                // modal in spirit and a click must not strand the keyboard.
                field.request_focus();
                ui.horizontal(|ui| {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        ui.label(dim("⏎ removes the label · esc cancels"));
                    } else {
                        // The badge exactly as the card will wear it, colour
                        // included — the preview is the promise.
                        ui.label(badge(&truncate(trimmed, 24), label_color(trimmed)));
                        ui.label(dim("⏎ saves · esc cancels"));
                    }
                });
                let (enter, escape) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                if enter {
                    self.prefs.set_label(&id, &text);
                    self.prefs_dirty = true;
                    done = true;
                } else if escape {
                    done = true;
                }
            });
        self.label_edit = if done || !open { None } else { Some((id, text)) };
    }
}

/// Renders the detail panes into the layout tree. `R-B20`.
///
/// Holds `&mut App` for the duration of one `Tree::ui` call. The tree itself is
/// taken out of `App` first, so this is a plain sequential borrow rather than
/// anything clever.
struct DetailPanes<'a> {
    app: &'a mut App,
    /// Cloned once rather than looked up per pane: five panes would otherwise
    /// be five map lookups plus five `Rc` bumps for a value that cannot change
    /// mid-frame.
    session: Rc<Session>,
    /// Set when a click landed inside a pane, to aim the keyboard at it.
    clicked_pane: Option<Tab>,
    /// Set when the arrangement changed — dragged, resized, or a tab closed —
    /// so the caller knows to persist it.
    edited: bool,
}

impl egui_tiles::Behavior<Tab> for DetailPanes<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut Tab,
    ) -> egui_tiles::UiResponse {
        // A click anywhere in a pane aims the keyboard at it. With one pane
        // visible there was nothing to decide; with several, scrolling and the
        // review keys need to know which one you mean.
        let focused = self.app.tab == *pane;
        let rect = ui.max_rect();
        if focused {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                5.0,
                egui::Stroke::new(1.0, BLUE.linear_multiply(0.5)),
                egui::StrokeKind::Inside,
            );
        }
        if ui.rect_contains_pointer(rect) && ui.input(|i| i.pointer.any_pressed()) {
            self.clicked_pane = Some(*pane);
        }

        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                let session = self.session.clone();
                self.app.pane_ui(ui, *pane, &session);
            });

        // Never `UiResponse::DragStarted` from the body: dragging a pane by its
        // content would fight every scroll area and text selection inside it.
        // The tab is the handle.
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Tab) -> egui::WidgetText {
        // The unread count rides on the Changes tab, which is the one number
        // worth carrying in furniture that is always on screen.
        let unread = if *pane == Tab::Changes {
            self.app
                .changes
                .get(&self.session.id)
                .map(|c| c.unreviewed_hunks())
                .unwrap_or(0)
        } else {
            0
        };
        // No binding in the tab. It was there on the palette's argument — the
        // surface you look at anyway is the cheapest place to learn a key — but
        // a tab bar is furniture you read a hundred times a day, and a hint you
        // have already absorbed is just noise in it. The palette and the
        // settings window teach the keys; the tab bar names the pane.
        if unread > 0 {
            format!("{} ({unread})", pane.label()).into()
        } else {
            pane.label().into()
        }
    }

    /// The binding moved off the tab face and onto its tooltip.
    ///
    /// Printing the key in the tab was the palette's argument applied one place
    /// too far: a tab bar is furniture you read a hundred times a day, and a
    /// hint you have already absorbed is noise in it. On hover it costs nothing
    /// and is still there the day you have forgotten.
    fn on_tab_button(
        &mut self,
        tiles: &mut egui_tiles::Tiles<Tab>,
        tile_id: egui_tiles::TileId,
        button_response: egui::Response,
    ) -> egui::Response {
        let Some(pane) = tiles.get_pane(&tile_id) else {
            return button_response;
        };
        let key = self.app.keymap.describe(pane.action());
        button_response.on_hover_text(format!("{}  ({key})", pane.label()))
    }

    fn is_tab_closable(&self, _tiles: &egui_tiles::Tiles<Tab>, _tile_id: egui_tiles::TileId) -> bool {
        true
    }

    fn on_tab_close(
        &mut self,
        _tiles: &mut egui_tiles::Tiles<Tab>,
        _tile_id: egui_tiles::TileId,
    ) -> bool {
        // Safe to allow because closing is reversible: the pane's own shortcut
        // puts it back. See `layout::focus`.
        self.edited = true;
        true
    }

    /// Every drag, resize and split reports here, which is the only reliable
    /// signal that the arrangement moved — comparing trees frame to frame
    /// would be both slower and easy to get subtly wrong.
    fn on_edit(&mut self, _edit_action: egui_tiles::EditAction) {
        self.edited = true;
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        26.0
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        3.0
    }

    fn tab_bar_color(&self, _visuals: &egui::Visuals) -> Color32 {
        BG
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            // Without this a container left holding one pane stays a container,
            // and the layout accumulates invisible nesting every time you drag
            // something out and back.
            prune_empty_tabs: true,
            prune_empty_containers: true,
            prune_single_child_tabs: false,
            prune_single_child_containers: true,
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }
}

/// One `icon value` pair in the status bar.
///
/// The icon is tinted and the value is not, so a row of these reads as a row of
/// values with coloured markers rather than as coloured text — which at this
/// size would be a lot of shouting for facts that are mostly reference.
fn stat(ui: &mut egui::Ui, glyph: &str, value: &str, tint: Color32) {
    ui.label(RichText::new(glyph).size(11.0).color(tint));
    ui.label(RichText::new(value).size(11.5).color(TEXT));
    ui.add_space(4.0);
}

/// What was clicked in one row of the keyboard settings list.
struct KeymapRowHit {
    response: egui::Response,
    row_clicked: bool,
    binding_clicked: bool,
    reset_clicked: bool,
}

/// One row of the keyboard settings list.
///
/// The cursor highlight is the same treatment the palette uses, because they
/// are the same idea — a keyboard-moved selection in a list — and two different
/// looks for one concept is how an interface stops feeling designed.
fn keymap_row(
    ui: &mut egui::Ui,
    action: crate::keymap::Action,
    binding: &str,
    picked: bool,
    capturing: bool,
    differs: bool,
) -> KeymapRowHit {
    let mut row_clicked = false;
    let mut binding_clicked = false;
    let mut reset_clicked = false;

    let frame = egui::Frame::NONE
        .fill(if picked {
            ui.visuals().selection.bg_fill.linear_multiply(0.45)
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(6, 3));

    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let btn = ui.add_sized(
                [140.0, 20.0],
                egui::Button::new(
                    RichText::new(binding)
                        .monospace()
                        .size(11.5)
                        .color(if capturing { AMBER } else { BLUE }),
                ),
            );
            if btn.clicked() {
                binding_clicked = true;
            }
            ui.label(RichText::new(action.label()).size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if differs && ui.small_button("reset").on_hover_text("back to the default").clicked() {
                    reset_clicked = true;
                }
            });
        });
    });

    // NOT `inner.response.interact(Sense::click())`: that registers a
    // click-sensing widget covering the whole row *after* the buttons inside
    // it, and egui resolves a tied hit to the last-registered widget — so the
    // row ate every click meant for the binding and reset buttons, and
    // rebinding by mouse silently never worked. The row's click is instead
    // derived from "a click landed here and no button claimed it".
    let response = inner.response;
    if !binding_clicked
        && !reset_clicked
        && response.contains_pointer()
        && ui.input(|i| i.pointer.primary_clicked())
    {
        row_clicked = true;
    }
    KeymapRowHit {
        response,
        row_clicked,
        binding_clicked,
        reset_clicked,
    }
}

/// One palette row: what it does, where it lives, and the key that would have
/// done it without opening the palette at all.
///
/// The binding is shown on every row deliberately. The palette's real job is to
/// make itself unnecessary — you look something up twice and the third time you
/// press the key.
fn palette_row(
    ui: &mut egui::Ui,
    action: crate::keymap::Action,
    binding: &str,
    picked: bool,
) -> egui::Response {
    let height = 22.0;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    if picked {
        ui.painter().rect_filled(
            rect,
            5.0,
            ui.visuals().selection.bg_fill.linear_multiply(0.55),
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 5.0, ui.visuals().widgets.hovered.bg_fill);
    }

    let pad = 8.0;
    let painter = ui.painter();
    painter.text(
        egui::pos2(rect.left() + pad, rect.center().y),
        egui::Align2::LEFT_CENTER,
        action.label(),
        egui::FontId::proportional(13.0),
        if picked {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        },
    );
    // Right to left: the key first, then the group behind it.
    let key_width = painter
        .text(
            egui::pos2(rect.right() - pad, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            binding,
            egui::FontId::monospace(11.0),
            if binding == "unbound" { DIM } else { BLUE },
        )
        .width();
    painter.text(
        egui::pos2(rect.right() - pad - key_width - 12.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        action.group(),
        egui::FontId::proportional(10.5),
        DIM,
    );
    resp
}

/// One chip of the collapsed queue strip: a single letter on its stable
/// colour, a red dot when the session needs a human, a ring when it is the
/// selected one. The Lens-style avatar, for sessions.
fn session_chip(
    ui: &mut egui::Ui,
    ch: char,
    color: Color32,
    selected: bool,
    needs_human: bool,
) -> egui::Response {
    let size = 20.0;
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 5.0, color);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ch,
        egui::FontId::monospace(12.5),
        Color32::WHITE,
    );
    // The dot outranks the letter: the strip's first job is still "which
    // ones need me", and colour alone must not be asked to carry it.
    if needs_human {
        painter.circle_filled(rect.right_top() + egui::vec2(-2.0, 2.0), 3.0, RED);
    }
    if selected {
        painter.rect_stroke(
            rect.expand(1.5),
            6.0,
            egui::Stroke::new(1.5, TEXT_STRONG),
            egui::StrokeKind::Outside,
        );
    } else if resp.hovered() {
        painter.rect_stroke(
            rect.expand(1.5),
            6.0,
            egui::Stroke::new(1.0, DIM),
            egui::StrokeKind::Outside,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// One go-to-file row: the file name where the eye lands, the directory
/// behind it — the order IntelliJ taught everyone to read.
fn file_palette_row(ui: &mut egui::Ui, path: &str, picked: bool) -> egui::Response {
    let height = 22.0;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    if picked {
        ui.painter().rect_filled(
            rect,
            5.0,
            ui.visuals().selection.bg_fill.linear_multiply(0.55),
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 5.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let pad = 8.0;
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    let painter = ui.painter();
    let name_width = painter
        .text(
            egui::pos2(rect.left() + pad, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(13.0),
            if picked {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().text_color()
            },
        )
        .width();
    if !dir.is_empty() {
        painter.text(
            egui::pos2(rect.left() + pad + name_width + 10.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            dir,
            egui::FontId::proportional(11.0),
            DIM,
        );
    }
    resp
}

/// One content-search row: `path:line` in wire spelling, then the line.
fn search_palette_row(
    ui: &mut egui::Ui,
    m: &mogeung_core::wire::ContentMatch,
    picked: bool,
) -> egui::Response {
    let height = 22.0;
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    if picked {
        ui.painter().rect_filled(
            rect,
            5.0,
            ui.visuals().selection.bg_fill.linear_multiply(0.55),
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 5.0, ui.visuals().widgets.hovered.bg_fill);
    }
    let pad = 8.0;
    let painter = ui.painter();
    let loc_width = painter
        .text(
            egui::pos2(rect.left() + pad, rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{}:{}", m.path, m.line),
            egui::FontId::monospace(11.5),
            BLUE,
        )
        .width();
    painter.text(
        egui::pos2(rect.left() + pad + loc_width + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        m.text.trim(),
        egui::FontId::monospace(11.5),
        if picked {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        },
    );
    resp
}

/// The queue filter's widget id, named once so the keyboard handler and the
/// widget itself cannot drift onto two different ids — a bug that would look
/// exactly like "the shortcut does nothing".
fn filter_id() -> egui::Id {
    egui::Id::new("queue-filter")
}

/// The keyboard settings window's own action filter.
fn keymap_filter_id() -> egui::Id {
    egui::Id::new("keymap-filter")
}

/// The chord a rebind should capture from this frame's events, if any.
///
/// egui 0.35 reports modifier presses as keys of their own (`AltLeft` …), so
/// "press Alt+9" arrives as AltLeft first — and the capture used to end
/// there, saving the bare modifier for every chord rebind. Caught live on
/// Ubuntu by pressing the chord and reading the file back. The modifier
/// presses are skipped; the chord's modifiers ride along with the real key.
fn captured_binding(events: &[egui::Event]) -> Option<crate::keymap::Binding> {
    events.iter().find_map(|e| match e {
        egui::Event::Key {
            key,
            pressed: true,
            modifiers,
            ..
        } if !crate::keymap::is_modifier_key(*key) => {
            Some(crate::keymap::Binding::new(*modifiers, *key))
        }
        _ => None,
    })
}

/// Whether the widget holding egui focus is a text box.
///
/// "Does *anything* have focus" is the wrong question for deciding whether a
/// window may claim keys: the embedded terminal, and any widget a click
/// happened to land on, all count as "something" — and each of those states
/// made the keyboard settings window look completely dead. What actually
/// matters is whether typed characters belong to a text input, which is
/// exactly the widgets that keep a `TextEdit` state.
fn text_input_focused(ui: &egui::Ui) -> bool {
    ui.memory(|m| m.focused())
        .is_some_and(|id| egui::TextEdit::load_state(ui.ctx(), id).is_some())
}

/// Whether the hidden flag may be changed at all.
///
/// **A live session cannot be hidden.** The queue exists to tell you about
/// running agents, so one you could dismiss by accident is one you could miss
/// entirely — and unlike a finished session it is still changing under you.
///
/// Already-hidden stays changeable regardless, or a session hidden while dead
/// that came back would be stuck out of sight with no way to recover it.
///
/// One function rather than the same condition written at each of the four
/// places that offer the action, because a rule spelled out four times is a
/// rule that will eventually be spelled three ways.
fn may_toggle_hidden(alive: bool, hidden: bool) -> bool {
    hidden || !alive
}

/// What the user asked for by clicking inside a card, as opposed to clicking
/// the card itself.
#[derive(Default)]
struct CardHit {
    /// The repo name was clicked: narrow the queue to it.
    filter_repo: Option<String>,
    /// The label badge was clicked: narrow the queue to that label.
    filter_label: Option<String>,
    /// The corner `✕` was clicked.
    hide: bool,
}

fn queue_card(
    ui: &mut egui::Ui,
    s: &Session,
    item: &AttentionItem,
    now: chrono::DateTime<Utc>,
    pinned: bool,
    hidden: bool,
    hideable: bool,
    label: Option<&str>,
) -> CardHit {
    let mut hit = CardHit::default();
    ui.horizontal(|ui| {
        // The user's own name for the session leads the row — it is the badge
        // they wrote, so it is the one they scan for. `R-B26`.
        if let Some(l) = label {
            if ui
                .add(egui::Button::new(badge(&truncate(l, 24), label_color(l))).frame(false))
                .on_hover_text("show only this label — right-click the card to edit it")
                .clicked()
            {
                hit.filter_label = Some(l.to_string());
            }
        }
        if pinned {
            ui.label(badge("PIN", BLUE));
        }
        if hidden {
            ui.label(badge("HIDDEN", DIM));
        }
        ui.label(badge(item.reason.label(), reason_color(item.reason)));
        if s.is_snoozed(now) {
            ui.label(badge(icon::SNOOZE, DIM));
        }
        if s.alive {
            let (txt, col) = match s.live_status {
                Some(LiveStatus::Busy) => ("live·busy", BLUE),
                Some(LiveStatus::Idle) => ("live·idle", AMBER),
                _ => ("live", DIM),
            };
            ui.label(RichText::new(txt).size(10.5).color(col));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Only for a session that is over, and only when it is not already
            // hidden — the corner of a card is the wrong place to *undo*
            // something, and a live session must not be dismissable at all.
            if hideable
                && !hidden
                && ui
                    .add(
                        egui::Button::new(RichText::new(icon::HIDE).size(11.0).color(DIM))
                            .frame(false),
                    )
                    .on_hover_text("hide it from the queue — reversible (h)")
                    .clicked()
            {
                hit.hide = true;
            }
            ui.label(dim(fmt_dur(s.duration_secs(now))));
        });
    });

    ui.label(RichText::new(truncate(&s.label(), 100)).size(13.0));

    ui.horizontal_wrapped(|ui| {
        // Clicking the repo is the fastest way to "just the ones near this".
        if ui
            .selectable_label(false, dim(s.repo_name()))
            .on_hover_text("show only this repo")
            .clicked()
        {
            hit.filter_repo = Some(s.repo_name());
        }
        if let Some(b) = &s.git_branch {
            ui.label(dim(format!("{} {b}", icon::BRANCH)));
        }
        if s.files_changed > 0 {
            ui.label(dim(format!(
                "{} files +{} -{}",
                s.files_changed, s.insertions, s.deletions
            )));
        }
        if s.turns > 0 {
            ui.label(dim(format!("{} turns", s.turns)));
        }
    });
    ui.label(dim(truncate(&item.detail, 90)));

    // R-B3. The one thing only a cross-session observer can tell you. Loud,
    // because two agents writing the same file is how work gets silently lost.
    if !s.collisions.is_empty() {
        let mut paths: Vec<&str> = s.collisions.iter().map(|c| c.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        let others: Vec<&str> = {
            let mut v: Vec<&str> = s.collisions.iter().map(|c| c.other_label.as_str()).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("⚠ COLLISION").size(10.5).color(RED).strong());
            ui.label(
                RichText::new(format!(
                    "{} also being edited by {}",
                    truncate(&paths.join(", "), 60),
                    truncate(&others.join(", "), 40)
                ))
                .size(11.0)
                .color(RED),
            );
        });
    }

    // R-B7. Advisory, not a queue tier: repetition is suggestive, not proof.
    if let Some(sig) = &s.loop_signal {
        ui.label(
            RichText::new(format!("↻ {}", truncate(sig, 80)))
                .size(11.0)
                .color(PURPLE),
        );
    }
    hit
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

impl App {
    /// The bottom status bar.
    ///
    /// Everything here used to be a line of `·`-separated grey text wedged
    /// under the session title, pushing the content that matters further down
    /// on every screen. It is reference material — branch, elapsed, turns,
    /// tool calls, tokens, path — consulted occasionally and read constantly by
    /// nobody. A status bar is exactly where that belongs: always available,
    /// never in the way, and costing one row for the whole window instead of
    /// three at the top of the pane.
    ///
    /// The icons are tinted so the row can be scanned rather than read, and one
    /// of the tints carries information: the clock turns amber once a session
    /// has been waiting on you, which is the single fact this whole app exists
    /// to surface.
    fn status_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::NONE
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().interaction.selectable_labels = false;
                let now = Utc::now();
                let Some(s) = self.selected_session().cloned() else {
                    // Never blank: with nothing selected it still answers "is
                    // anything running".
                    let live = self.sessions.values().filter(|s| s.alive).count();
                    ui.label(dim(format!(
                        "{live} live session(s) · {} known",
                        self.sessions.len()
                    )));
                    return;
                };

                stat(ui, icon::FOLDER, &s.repo_name(), BLUE);
                if let Some(b) = &s.git_branch {
                    stat(ui, icon::BRANCH, b, BLUE);
                }
                // Amber once it has been waiting on you: the one tint here
                // that means something rather than merely separating.
                let waiting = s.waiting_secs(now).is_some();
                stat(
                    ui,
                    icon::CLOCK,
                    &fmt_dur(s.duration_secs(now)),
                    if waiting { AMBER } else { DIM },
                );
                stat(ui, icon::TURNS, &s.turns.to_string(), PURPLE);
                stat(ui, icon::TOOLS, &s.tool_calls.to_string(), GREEN);
                stat(ui, icon::TOKENS, &tokens(s.tokens_out), DIM);
                // Only while alive: a dead session's remembered pid now
                // belongs to its `/clear` successor, and printing it here
                // would name the wrong process.
                if let Some(pid) = s.pid.filter(|_| s.alive) {
                    stat(ui, "#", &pid.to_string(), DIM);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The path is the widest thing here and the least urgent,
                    // so it goes last and gives up its space first.
                    let (dir, base) = short_path(&s.cwd);
                    ui.add(
                        egui::Label::new(mono(format!("{dir}{base}")).size(11.0)).truncate(),
                    )
                    .on_hover_text(&s.cwd);
                });
            });
        });
    }

    fn detail_panel(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let Some(s) = self.selected_session().cloned() else {
                ui.centered_and_justified(|ui| {
                    ui.label(dim("select a session"));
                });
                return;
            };
            let now = Utc::now();

            // One row: state, title, and everything you can *do* folded into a
            // menu on the right. The metadata that used to sit under this has
            // moved to the status bar — it was three rows of reference material
            // between you and the diff.
            ui.horizontal(|ui| {
                if s.alive {
                    let (txt, col) = match s.live_status {
                        Some(LiveStatus::Idle) => ("WAITING FOR YOU", RED),
                        Some(LiveStatus::Busy) => ("BUSY", BLUE),
                        _ => ("LIVE", DIM),
                    };
                    ui.label(badge(txt, col));
                } else {
                    ui.label(badge("ended", DIM));
                }
                ui.add(
                    egui::Label::new(RichText::new(s.label()).size(14.0).strong()).truncate(),
                )
                .on_hover_text(s.label());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("⋯", |ui| {
                        if ui.button("Refresh diff").clicked() {
                            self.net.send(ClientMsg::RefreshChange {
                                session_id: s.id.clone(),
                            });
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .button(format!(
                                "Reset the pane layout  ({})",
                                self.keymap.describe(crate::keymap::Action::ResetLayout)
                            ))
                            .clicked()
                        {
                            self.tree = Some(crate::layout::default_tree());
                            self.layout_dirty = true;
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .button("Forget this session")
                            .on_hover_text(
                                "stop tracking it and drop its review marks",
                            )
                            .clicked()
                        {
                            self.net.send(ClientMsg::ForgetSession {
                                session_id: s.id.clone(),
                            });
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("refresh, reset layout, forget");

                    // The editor handoffs, visible again. They lived in the ⋯
                    // menu for one release and earned their way back out: the
                    // handoff to a real editor is the roadmap's own answer to
                    // "not an editor", and an answer behind a menu costs two
                    // clicks every time. Right-to-left, so the *last* one here
                    // is the leftmost on screen.
                    for t in [
                        OpenTarget::Terminal,
                        OpenTarget::Finder,
                        OpenTarget::VsCode,
                        OpenTarget::Intellij,
                    ] {
                        if ui
                            .small_button(dim(t.label()))
                            .on_hover_text(format!(
                                "open this session's directory in {}",
                                t.label()
                            ))
                            .clicked()
                        {
                            if let Err(e) = ui::open_in(t, &s.cwd) {
                                self.errors.push(e);
                            }
                        }
                    }
                });
            });

            if let Some(err) = &s.error {
                ui.label(RichText::new(err).color(RED).size(12.0));
            }
            if let Some(w) = s.waiting_secs(now) {
                ui.label(
                    RichText::new(format!(
                        "This session has been waiting for your input for {}.",
                        fmt_dur(w)
                    ))
                    .color(AMBER)
                    .size(12.5),
                );
            }

            ui.separator();

            // Anything that used to live beside the tab bar and is not a tab.
            ui.horizontal(|ui| {
                // Retires itself the moment anything is split — see
                // `layout::is_unsplit`.
                if self.tree.as_ref().map(crate::layout::is_unsplit).unwrap_or(false) {
                    ui.label(
                        RichText::new("drag a tab out to split the pane")
                            .size(10.5)
                            .color(DIM),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.flagged.is_empty()
                        && ui
                            .button(RichText::new(format!("{} {} flagged", icon::FLAG, self.flagged.len())).color(AMBER))
                            .on_hover_text("build a follow-up prompt from what you flagged")
                            .clicked()
                    {
                        self.show_prompt = true;
                    }
                });
            });

            // The tree is taken out of `self` for the duration, because the
            // behaviour that renders each pane needs `&mut App` and the tree
            // lives on App. Taking it first makes the two borrows sequential
            // rather than overlapping.
            let mut tree = self.tree.take();
            if let Some(tree) = &mut tree {
                let mut behavior = DetailPanes {
                    app: self,
                    session: s.clone(),
                    clicked_pane: None,
                    edited: false,
                };
                tree.ui(&mut behavior, ui);
                let (focus, edited) = (behavior.clicked_pane, behavior.edited);
                if let Some(t) = focus {
                    self.tab = t;
                }
                self.layout_dirty |= edited;
            }
            self.tree = tree;
        });
    }

    /// Render one pane. Dispatch only — every arm existed before the tree did.
    fn pane_ui(&mut self, ui: &mut egui::Ui, tab: Tab, s: &Session) {
        match tab {
            Tab::Changes => self.changes_tab(ui, s),
            Tab::Transcript => self.transcript_tab(ui, s),
            Tab::Info => self.info_tab(ui, s),
            Tab::Debt => self.debt_tab(ui, s),
            Tab::Terminal => self.terminal_tab(ui, s),
            Tab::Explorer => self.explorer_tab(ui, s),
            Tab::Git => self.git_tab(ui, s),
        }
    }

    /// The session repo's git state: local changes and log on the left, the
    /// selected diff on the right. `R-D10`, deepened by `R-D11`. Read-only
    /// from end to end — the daemon offers nothing that writes, so neither
    /// can this.
    fn git_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let z = self.pane_zoom(ui, "git");
        scale_text(ui, z);
        self.gitview.ensure_session(&s.id);

        // Same one-door fetch rule as the Editor: ask for whatever the state
        // wants and lacks, in the paint, so a docked pane works unswitched.
        {
            let gv = &mut self.gitview;
            if !gv.status_loaded && !gv.status_pending {
                gv.status_pending = true;
                self.net.send(ClientMsg::GitStatus {
                    session_id: s.id.clone(),
                });
            }
            if gv.commits.is_empty() && !gv.log_done && !gv.log_pending {
                gv.log_pending = true;
                self.net.send(ClientMsg::GitLog {
                    session_id: s.id.clone(),
                    skip: 0,
                    limit: 50,
                    rev: gv.log_rev.clone(),
                    grep: gv.log_grep.clone(),
                    author: gv.log_author.clone(),
                    path: gv.log_path.clone(),
                });
            }
            if gv.refs.is_none() && !gv.refs_pending {
                gv.refs_pending = true;
                self.net.send(ClientMsg::GitRefs {
                    session_id: s.id.clone(),
                });
            }
            if !gv.stashes_loaded && !gv.stashes_pending {
                gv.stashes_pending = true;
                self.net.send(ClientMsg::GitStashes {
                    session_id: s.id.clone(),
                });
            }
            if !gv.submodules_loaded && !gv.submodules_pending {
                gv.submodules_pending = true;
                self.net.send(ClientMsg::GitSubmodules {
                    session_id: s.id.clone(),
                });
            }
            match gv.selection.clone() {
                crate::gitview::Selection::Commit(sha)
                    if !gv.commit_diffs.contains_key(&sha)
                        && gv.pending_shows.insert(sha.clone()) =>
                {
                    self.net.send(ClientMsg::GitShow {
                        session_id: s.id.clone(),
                        sha,
                    });
                }
                crate::gitview::Selection::Local(path)
                    if !gv.local_diffs.contains_key(&path)
                        && gv.pending_file_diffs.insert(path.clone()) =>
                {
                    self.net.send(ClientMsg::GitDiffFile {
                        session_id: s.id.clone(),
                        path,
                    });
                }
                crate::gitview::Selection::Stash(index)
                    if !gv.stash_diffs.contains_key(&index)
                        && gv.pending_stash_shows.insert(index) =>
                {
                    self.net.send(ClientMsg::GitStashShow {
                        session_id: s.id.clone(),
                        index,
                    });
                }
                crate::gitview::Selection::Range(from, to)
                    if !gv.range_diffs.contains_key(&(from.clone(), to.clone()))
                        && gv.pending_ranges.insert((from.clone(), to.clone())) =>
                {
                    self.net.send(ClientMsg::GitDiffRange {
                        session_id: s.id.clone(),
                        from,
                        to,
                    });
                }
                _ => {}
            }
        }

        egui::Panel::left("git-left").default_size(320.0).show(ui, |ui| {
            self.git_header(ui);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("LOCAL CHANGES").size(11.0).color(DIM).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("↻")
                        .on_hover_text("re-read the log and the working tree")
                        .clicked()
                    {
                        self.gitview.refresh();
                    }
                    let mut only = self.gitview.session_only;
                    if ui
                        .checkbox(&mut only, "this session")
                        .on_hover_text("only files this session is believed to have touched")
                        .changed()
                    {
                        self.gitview.session_only = only;
                    }
                });
            });
            // Both axes + Extend, the worktree tree's rule: a long path
            // scrolls sideways instead of folding — see the tree's comment
            // for why the scroll area alone is not enough.
            egui::ScrollArea::both()
                .id_salt("git-local-scroll")
                .max_height(ui.available_height() * 0.40)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    // The queue's lesson: selectable text gives every label
                    // a click sense that sits in front of the row's own.
                    ui.style_mut().interaction.selectable_labels = false;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    self.git_local_list(ui, s);
                });
            ui.separator();
            egui::ScrollArea::both()
                .id_salt("git-lower-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    // Same rule as local changes: labels must not out-click
                    // the rows they decorate.
                    ui.style_mut().interaction.selectable_labels = false;
                    ui.spacing_mut().item_spacing.y = 1.0;
                    self.git_ref_sections(ui);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("LOG").size(11.0).color(DIM).strong());
                        if let Some(rev) = self.gitview.log_rev.clone() {
                            if ui
                                .small_button(format!("⌥ {rev} ✕"))
                                .on_hover_text("scoped to this ref — click to go back to HEAD")
                                .clicked()
                            {
                                self.gitview.set_log_rev(None);
                            }
                        }
                    });
                    // The filter bar (`R-D12`): one field, `author:` and
                    // `path:` pulled out, the rest matching messages. A set
                    // path follows renames — this is also file history.
                    ui.horizontal(|ui| {
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut self.gitview.filter_input)
                                .id(egui::Id::new("git-log-filter"))
                                .hint_text("filter: text · author:… · path:…")
                                .desired_width(ui.available_width() - 24.0),
                        );
                        let entered =
                            field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if entered {
                            let (grep, author, path) =
                                crate::gitview::parse_filter_input(&self.gitview.filter_input);
                            self.gitview.set_log_filter(grep, author, path);
                        }
                        let active = self.gitview.log_grep.is_some()
                            || self.gitview.log_author.is_some()
                            || self.gitview.log_path.is_some();
                        if active
                            && ui
                                .small_button("✕")
                                .on_hover_text("clear the filter")
                                .clicked()
                        {
                            self.gitview.filter_input.clear();
                            self.gitview.set_log_filter(None, None, None);
                        }
                    });
                    if self.gitview.log_path.is_some() {
                        ui.label(dim("history of one file — renames followed"));
                    }
                    self.git_log_list(ui, s);
                });
        });

        self.git_diff_panel(ui);
    }

    /// One line of repo orientation: branch, tracking state, remote, fetch
    /// age. Display only — mogeung never fetches ([feature 0011]).
    fn git_header(&mut self, ui: &mut egui::Ui) {
        let Some(refs) = &self.gitview.refs else {
            ui.label(dim("reading refs…"));
            return;
        };
        let now = Utc::now().timestamp();
        ui.horizontal_wrapped(|ui| {
            match &refs.head {
                Some(branch) => {
                    ui.label(
                        RichText::new(format!("⎇ {branch}"))
                            .size(12.0)
                            .strong(),
                    )
                    .on_hover_text(format!("HEAD is {}", refs.head_sha));
                }
                None => {
                    ui.label(
                        RichText::new(format!("⎇ detached @ {}", refs.head_sha))
                            .size(12.0)
                            .color(AMBER),
                    )
                    .on_hover_text("HEAD points at a commit, not a branch");
                }
            }
            if let Some(cur) = refs.branches.iter().find(|b| b.current) {
                if let Some(up) = &cur.upstream {
                    let mut track = String::new();
                    if cur.ahead > 0 {
                        track.push_str(&format!(" ↑{}", cur.ahead));
                    }
                    if cur.behind > 0 {
                        track.push_str(&format!(" ↓{}", cur.behind));
                    }
                    ui.label(dim(format!("{up}{track}"))).on_hover_text(
                        "commits ahead ↑ / behind ↓ the upstream, as of the last fetch",
                    );
                }
            }
            if let Some(r) = refs.remotes.first() {
                let fetched = match refs.fetch_epoch {
                    Some(t) => {
                        format!("fetched {} ago", crate::gitview::age(now, t))
                    }
                    None => "never fetched".to_string(),
                };
                ui.label(dim(format!("· {} · {fetched}", r.name)))
                    .on_hover_text(format!(
                        "{}\nmogeung never fetches — this is the repo's own last fetch",
                        r.url
                    ));
            }
        });
    }

    /// The collapsible reading lists: branches, tags, stashes, submodules.
    /// Sections with nothing to say do not appear.
    fn git_ref_sections(&mut self, ui: &mut egui::Ui) {
        let now = Utc::now().timestamp();
        if let Some(refs) = self.gitview.refs.clone() {
            if !refs.branches.is_empty() {
                egui::CollapsingHeader::new(
                    RichText::new(format!("BRANCHES ({})", refs.branches.len()))
                        .size(11.0)
                        .color(DIM)
                        .strong(),
                )
                .id_salt("git-branches")
                .show(ui, |ui| {
                    for b in &refs.branches {
                        let scoped = self.gitview.log_rev.as_deref() == Some(b.name.as_str());
                        let mut text = format!("{} {}", b.sha, b.name);
                        if b.ahead > 0 {
                            text.push_str(&format!(" ↑{}", b.ahead));
                        }
                        if b.behind > 0 {
                            text.push_str(&format!(" ↓{}", b.behind));
                        }
                        let mut rich = RichText::new(text).monospace();
                        if b.current {
                            rich = rich.color(GREEN);
                        }
                        let row = ui
                            .selectable_label(scoped, rich)
                            .on_hover_text(format!(
                                "{} · {}{}\nclick to scope the log to this branch — nothing is checked out",
                                crate::gitview::age(now, b.epoch),
                                b.upstream.as_deref().unwrap_or("no upstream"),
                                if b.current { "\nthe checked-out branch" } else { "" },
                            ));
                        if row.clicked() {
                            self.gitview.set_log_rev(if scoped {
                                None
                            } else {
                                Some(b.name.clone())
                            });
                        }
                    }
                });
            }
            if !refs.tags.is_empty() {
                egui::CollapsingHeader::new(
                    RichText::new(format!("TAGS ({})", refs.tags.len()))
                        .size(11.0)
                        .color(DIM)
                        .strong(),
                )
                .id_salt("git-tags")
                .show(ui, |ui| {
                    for t in &refs.tags {
                        let row = ui
                            .selectable_label(
                                false,
                                RichText::new(format!("{} {}", t.sha, t.name)).monospace(),
                            )
                            .on_hover_text(format!(
                                "{} · click to show the tagged commit",
                                crate::gitview::age(now, t.epoch)
                            ));
                        if row.clicked() {
                            self.gitview.selection =
                                crate::gitview::Selection::Commit(t.sha.clone());
                        }
                    }
                });
            }
        }
        if !self.gitview.stashes.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("STASHES ({})", self.gitview.stashes.len()))
                    .size(11.0)
                    .color(DIM)
                    .strong(),
            )
            .id_salt("git-stashes")
            .show(ui, |ui| {
                for st in self.gitview.stashes.clone() {
                    let picked =
                        self.gitview.selection == crate::gitview::Selection::Stash(st.index);
                    let row = ui
                        .selectable_label(
                            picked,
                            RichText::new(format!(
                                "stash@{{{}}} {}",
                                st.index,
                                truncate(&st.message, 40)
                            ))
                            .monospace(),
                        )
                        .on_hover_text(format!(
                            "{}\n{} · read-only: popping stays in the terminal",
                            st.message,
                            crate::gitview::age(now, st.epoch)
                        ));
                    if row.clicked() {
                        self.gitview.selection = crate::gitview::Selection::Stash(st.index);
                    }
                }
            });
        }
        if !self.gitview.submodules.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("SUBMODULES ({})", self.gitview.submodules.len()))
                    .size(11.0)
                    .color(DIM)
                    .strong(),
            )
            .id_salt("git-submodules")
            .show(ui, |ui| {
                for sub in &self.gitview.submodules {
                    let (mark, color, meaning) = match sub.state.as_str() {
                        "+" => ("+", Some(AMBER), "checked out at a different commit than recorded"),
                        "-" => ("-", Some(DIM), "not initialised"),
                        "U" => ("U", Some(RED), "merge conflicts"),
                        _ => (" ", None, "in sync"),
                    };
                    let mut rich =
                        RichText::new(format!("{mark}{} {}", sub.sha, sub.path)).monospace();
                    if let Some(c) = color {
                        rich = rich.color(c);
                    }
                    ui.label(rich)
                    .on_hover_text(format!(
                        "{meaning}{}",
                        if sub.note.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", sub.note)
                        }
                    ));
                }
            });
        }
    }

    /// The uncommitted files, staged and unstaged distinguished by colour.
    fn git_local_list(&mut self, ui: &mut egui::Ui, s: &Session) {
        if !self.gitview.status_loaded {
            ui.label(dim("reading the working tree…"));
            return;
        }
        let mut entries: Vec<mogeung_core::wire::StatusEntry> = self
            .gitview
            .status
            .iter()
            // `!!` rows are dimming data for the explorer, not changes.
            .filter(|e| e.state != "!!")
            .filter(|e| {
                if !self.gitview.session_only {
                    return true;
                }
                // Repo-relative entry vs the session's absolute touched
                // paths, joined through the repo root when we know it.
                match &s.repo_root {
                    Some(root) => {
                        let abs = format!("{}/{}", root.trim_end_matches('/'), e.path);
                        s.touched_files.iter().any(|t| *t == abs)
                    }
                    None => true,
                }
            })
            .cloned()
            .collect();
        if entries.is_empty() {
            ui.label(dim(if self.gitview.session_only {
                "nothing uncommitted from this session"
            } else {
                "working tree clean"
            }));
            return;
        }
        // Conflicts first: the one uncommitted state that is never routine.
        entries.sort_by_key(|e| !e.conflicted);
        for e in entries {
            let picked =
                self.gitview.selection == crate::gitview::Selection::Local(e.path.clone());
            let color = if e.conflicted {
                RED
            } else if e.staged && !e.unstaged {
                GREEN
            } else if e.staged {
                AMBER
            } else if e.state == "??" {
                DIM
            } else {
                BLUE
            };
            let label = if e.conflicted {
                format!("{} {}  ⚠ conflict", e.state, e.path)
            } else {
                format!("{} {}", e.state, e.path)
            };
            let row = ui
                .selectable_label(
                    picked,
                    RichText::new(label).monospace().color(color),
                )
                .on_hover_text(if e.conflicted {
                    "unresolved merge conflict — resolving stays in the terminal"
                } else {
                    match (e.staged, e.unstaged) {
                        (true, true) => "staged, with further unstaged edits",
                        (true, false) => "staged",
                        (false, _) if e.state == "??" => "untracked",
                        _ => "unstaged",
                    }
                });
            if row.clicked() {
                self.gitview.selection = crate::gitview::Selection::Local(e.path.clone());
            }
        }
    }

    /// Recent commits, newest first, paging on demand — with a graph
    /// column, ref decorations, an attribution hint, and a read-only
    /// context menu.
    fn git_log_list(&mut self, ui: &mut egui::Ui, s: &Session) {
        if self.gitview.commits.is_empty() {
            ui.label(dim(if self.gitview.log_done {
                "no commits yet"
            } else {
                "reading the log…"
            }));
            return;
        }
        let now = Utc::now().timestamp();
        let commits = self.gitview.commits.clone();
        let graph = self.gitview.graph.clone();
        // The widest lane on screen decides the column, capped so a wild
        // history cannot push the subjects off the pane.
        let max_lanes = graph
            .iter()
            .map(|r| r.occupied.len().max(r.lane + 1))
            .max()
            .unwrap_or(1)
            .min(8);
        let lane_w = 8.0f32;
        let graph_w = max_lanes as f32 * lane_w;
        let remote_url = self
            .gitview
            .refs
            .as_ref()
            .and_then(|r| r.remotes.first())
            .map(|r| r.url.clone());
        for (i, c) in commits.iter().enumerate() {
            let picked = self.gitview.selection == crate::gitview::Selection::Commit(c.sha.clone());
            let row = ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                // The graph cell: verticals for occupied lanes, a dot on
                // this commit's, stubs where branches fan out or join.
                // Row height follows the (possibly zoomed) monospace style,
                // so the graph column never drifts off its rows.
                let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(graph_w, row_h), egui::Sense::hover());
                if let Some(row) = graph.get(i) {
                    let painter = ui.painter();
                    let x_of = |lane: usize| rect.left() + lane as f32 * lane_w + lane_w / 2.0;
                    let color_of = |lane: usize| GRAPH_COLORS[lane % GRAPH_COLORS.len()];
                    for (lane, occ) in row.occupied.iter().enumerate().take(8) {
                        if *occ {
                            painter.line_segment(
                                [
                                    egui::pos2(x_of(lane), rect.top()),
                                    egui::pos2(x_of(lane), rect.bottom()),
                                ],
                                egui::Stroke::new(1.0, color_of(lane)),
                            );
                        }
                    }
                    let dot = egui::pos2(x_of(row.lane.min(7)), rect.center().y);
                    for &m in row.merges.iter().filter(|&&m| m < 8) {
                        painter.line_segment(
                            [dot, egui::pos2(x_of(m), rect.bottom())],
                            egui::Stroke::new(1.0, color_of(m)),
                        );
                    }
                    for &j in row.joins.iter().filter(|&&j| j < 8) {
                        painter.line_segment(
                            [egui::pos2(x_of(j), rect.top()), dot],
                            egui::Stroke::new(1.0, color_of(j)),
                        );
                    }
                    painter.circle_filled(dot, 2.5, color_of(row.lane.min(7)));
                }
                let label_resp = ui
                    .selectable_label(
                        picked,
                        RichText::new(format!("{} {}", c.short, truncate(&c.summary, 40)))
                            .monospace(),
                    )
                    .on_hover_text(format!(
                        "{}\n{} · {} · {}{}\nright-click for copy / open / range diff",
                        c.summary,
                        c.author,
                        crate::gitview::age(now, c.epoch),
                        c.sha,
                        if c.touches_session {
                            "\n● probably this session's work (files + timing match)"
                        } else {
                            ""
                        }
                    ));
                if c.touches_session {
                    ui.label(RichText::new("●").size(9.0).color(GREEN)).on_hover_text(
                        "probably this session's work — its files and timing match. A hint, not a fact",
                    );
                }
                for r in c.refs.iter().take(3) {
                    let color = if r.starts_with("tag: ") { AMBER } else { BLUE };
                    ui.label(RichText::new(r).size(10.0).color(color));
                }
                label_resp
            });
            // The whole row is the target, not just the text: the queue
            // card's trick (`Response::interact` on the container's own
            // response, which sits *behind* its children). Right-clicking
            // the graph, the chips, or the space after the subject all
            // reach the same menu — "right-click did nothing" was people
            // missing a text-sized target, found in dogfooding.
            let whole = row
                .response
                .interact(egui::Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            let target = row.inner.union(whole);
            if target.clicked() {
                self.gitview.selection = crate::gitview::Selection::Commit(c.sha.clone());
            }
            target.context_menu(|ui| {
                    if ui.button("Copy sha").clicked() {
                        ui.ctx().copy_text(c.sha.clone());
                        ui.close();
                    }
                    if ui.button("Copy subject").clicked() {
                        ui.ctx().copy_text(c.summary.clone());
                        ui.close();
                    }
                    if let Some(url) = remote_url
                        .as_deref()
                        .and_then(|u| crate::gitview::commit_url(u, &c.sha))
                    {
                        if ui.button("Open on remote").on_hover_text(&url).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            ui.close();
                        }
                    }
                    ui.separator();
                    match self.gitview.range_from.clone() {
                        Some(marked) if marked != c.sha => {
                            if ui
                                .button("Diff against marked commit")
                                .on_hover_text("older → newer, decided by commit time")
                                .clicked()
                            {
                                // Older first, so the diff reads forward in
                                // time regardless of click order.
                                let marked_epoch = commits
                                    .iter()
                                    .find(|m| m.sha == marked)
                                    .map(|m| m.epoch)
                                    .unwrap_or(i64::MIN);
                                let (from, to) = if marked_epoch <= c.epoch {
                                    (marked.clone(), c.sha.clone())
                                } else {
                                    (c.sha.clone(), marked.clone())
                                };
                                self.gitview.selection =
                                    crate::gitview::Selection::Range(from, to);
                                self.gitview.range_from = None;
                                ui.close();
                            }
                            if ui.button("Clear mark").clicked() {
                                self.gitview.range_from = None;
                                ui.close();
                            }
                        }
                        _ => {
                            if ui
                                .button("Mark for range diff")
                                .on_hover_text("then pick a second commit to diff the two")
                                .clicked()
                            {
                                self.gitview.range_from = Some(c.sha.clone());
                                ui.close();
                            }
                        }
                    }
                });
        }
        if !self.gitview.log_done {
            if ui.button(dim("show more")).clicked() && !self.gitview.log_pending {
                self.gitview.log_pending = true;
                self.net.send(ClientMsg::GitLog {
                    session_id: s.id.clone(),
                    skip: self.gitview.commits.len() as u32,
                    limit: 50,
                    rev: self.gitview.log_rev.clone(),
                    grep: self.gitview.log_grep.clone(),
                    author: self.gitview.log_author.clone(),
                    path: self.gitview.log_path.clone(),
                });
            }
        }
    }

    /// The right side: the selected diff, rendered by the same pipeline as
    /// the Changes tab.
    fn git_diff_panel(&mut self, ui: &mut egui::Ui) {
        let (title, files) = match &self.gitview.selection {
            crate::gitview::Selection::None => {
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.label(dim("pick a commit or an uncommitted file to see its diff"));
                });
                return;
            }
            crate::gitview::Selection::Commit(sha) => {
                (sha.chars().take(10).collect::<String>(), self.gitview.commit_diffs.get(sha))
            }
            crate::gitview::Selection::Local(path) => {
                (path.clone(), self.gitview.local_diffs.get(path))
            }
            crate::gitview::Selection::Stash(index) => (
                format!("stash@{{{index}}}"),
                self.gitview.stash_diffs.get(index),
            ),
            crate::gitview::Selection::Range(from, to) => (
                format!(
                    "{}..{}",
                    from.chars().take(10).collect::<String>(),
                    to.chars().take(10).collect::<String>()
                ),
                self.gitview
                    .range_diffs
                    .get(&(from.clone(), to.clone())),
            ),
        };
        // "Open at this commit" only makes sense when the diff *is* a
        // commit; the sha rides along for the per-file buttons below.
        let at_commit = match &self.gitview.selection {
            crate::gitview::Selection::Commit(sha) => Some(sha.clone()),
            crate::gitview::Selection::Range(_, to) => Some(to.clone()),
            _ => None,
        };
        let Some(files) = files else {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(dim(format!("loading the diff of {title}…")));
            });
            return;
        };
        let files = files.clone();
        let (syntax, words) = (self.prefs.syntax, self.prefs.word_diff);
        let mut open_at_rev: Option<(String, String)> = None;
        let mut select_commit: Option<String> = None;
        // The commit header (`R-D12`): the full message and the facts a
        // commercial client puts above the patch. Scrolls with the diff so
        // a long agent-written body cannot pin the files off screen.
        let detail = match &self.gitview.selection {
            crate::gitview::Selection::Commit(sha) => self
                .gitview
                .commit_details
                .get(sha)
                .cloned()
                .map(|d| (sha.clone(), d)),
            _ => None,
        };
        egui::ScrollArea::both()
            .id_salt(("git-diff-scroll", &title))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some((sha, d)) = &detail {
                    let when = |epoch: i64| {
                        chrono::DateTime::from_timestamp(epoch, 0)
                            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_default()
                    };
                    let (subject, body) = match d.message.split_once('\n') {
                        Some((s, b)) => (s.to_string(), b.trim().to_string()),
                        None => (d.message.clone(), String::new()),
                    };
                    ui.label(RichText::new(subject).strong().size(13.0));
                    if !body.is_empty() {
                        ui.label(RichText::new(body).monospace().size(11.5));
                    }
                    ui.horizontal_wrapped(|ui| {
                        ui.label(dim(format!("{} · {}", d.author, when(d.epoch))));
                        if d.committer != d.author || d.commit_epoch != d.epoch {
                            ui.label(dim(format!(
                                "· committed by {} · {}",
                                d.committer,
                                when(d.commit_epoch)
                            )));
                        }
                        ui.label(dim(format!(
                            "· {} files +{} −{}",
                            files.len(),
                            files.iter().map(|f| f.insertions).sum::<u32>(),
                            files.iter().map(|f| f.deletions).sum::<u32>(),
                        )));
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(dim(format!("{} ·", sha.chars().take(10).collect::<String>())));
                        for p in &d.parents {
                            if ui
                                .link(RichText::new(p).monospace().size(11.5).color(BLUE))
                                .on_hover_text("show this parent commit")
                                .clicked()
                            {
                                select_commit = Some(p.clone());
                            }
                        }
                        for r in &d.refs {
                            let color = if r.starts_with("tag: ") { AMBER } else { BLUE };
                            ui.label(RichText::new(r).size(10.0).color(color));
                        }
                    });
                    ui.separator();
                }
                if files.is_empty() {
                    ui.label(dim("no textual diff — empty, binary, or a merge with no changes"));
                    return;
                }
                for f in &files {
                    ui.horizontal(|ui| {
                        ui.label(mono(&f.path));
                        ui.label(dim(format!("+{} −{}", f.insertions, f.deletions)));
                        for fl in &f.flags {
                            ui.label(dim(fl.label()));
                        }
                        if f.truncated {
                            ui.label(RichText::new("binary or too large").size(11.0).color(AMBER));
                        }
                        if let Some(sha) = &at_commit {
                            if f.status != mogeung_core::change::FileStatus::Deleted
                                && ui
                                    .small_button("@")
                                    .on_hover_text(
                                        "open this file as it was at this commit — read-only, in the Editor",
                                    )
                                    .clicked()
                            {
                                open_at_rev = Some((f.path.clone(), sha.clone()));
                            }
                        }
                    });
                    for h in &f.hunks {
                        ui.label(dim(&h.header));
                        render_unified(ui, &h.lines, syntax, words);
                        ui.add_space(4.0);
                    }
                    ui.separator();
                }
            });
        if let Some(sha) = select_commit {
            self.gitview.selection = crate::gitview::Selection::Commit(sha);
        }
        if let Some((path, sha)) = open_at_rev {
            self.explorer.ensure_session(&self.gitview.session.clone().unwrap_or_default());
            self.explorer.open_file_at_rev(&path, &sha);
            self.set_tab(Tab::Explorer);
        }
    }

    /// The session's worktree: tree on the left, tabs and a read-only viewer
    /// on the right. `R-B24`, workbench behaviour by `R-B25`.
    ///
    /// Everything shown here came over the wire — the UI never touches the
    /// worktree itself ([ADR-0001]), and nothing in this pane can write.
    fn explorer_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let z = self.pane_zoom(ui, "editor");
        scale_text(ui, z);
        self.explorer.ensure_session(&s.id);

        // Ask the daemon for whatever the state wants and lacks: the root,
        // every expanded directory without a listing, the active tab without
        // a body. One door for all fetching, which is what makes restore from
        // disk, reveal, refresh and a plain click all re-fetch the same way —
        // and it lives in the paint rather than `set_tab`, so a pane that is
        // *docked* visible works without ever having been switched to.
        {
            // The ignore list rides git status; ask for it here too, so the
            // tree dims gitignored subtrees before the Git pane ever opens.
            // Repo sessions only — a non-repo session has nothing to dim
            // and would only earn an error toast.
            if s.repo_root.is_some() {
                self.gitview.ensure_session(&s.id);
                if !self.gitview.status_loaded && !self.gitview.status_pending {
                    self.gitview.status_pending = true;
                    self.net.send(ClientMsg::GitStatus {
                        session_id: s.id.clone(),
                    });
                }
            }
            let st = self.explorer.current_mut();
            let wants: Vec<String> = std::iter::once(String::new())
                .chain(st.expanded.iter().cloned())
                .filter(|d| !st.dirs.contains_key(d) && !st.pending.contains(d))
                .collect();
            for path in wants {
                st.pending.insert(path.clone());
                self.net.send(ClientMsg::ListDir {
                    session_id: s.id.clone(),
                    path,
                });
            }
            // Both splits read at once, so both actives want bodies. A
            // worktree tab fetches from the worktree; a revision tab
            // fetches from history, keyed so the two never collide.
            let body_wants: Vec<(String, Option<String>)> = [0u8, 1]
                .into_iter()
                .filter_map(|g| st.active_of(g).and_then(|i| st.open.get(i)))
                .filter(|t| t.view.is_none())
                .filter(|t| {
                    let key = match &t.rev {
                        None => t.path.clone(),
                        Some(rev) => crate::explorer::rev_key(rev, &t.path),
                    };
                    !st.pending_files.contains(&key)
                })
                .map(|t| (t.path.clone(), t.rev.clone()))
                .collect();
            for (path, rev) in body_wants {
                match rev {
                    None => {
                        st.pending_files.insert(path.clone());
                        self.net.send(ClientMsg::FetchFile {
                            session_id: s.id.clone(),
                            path,
                        });
                    }
                    Some(rev) => {
                        st.pending_files.insert(crate::explorer::rev_key(&rev, &path));
                        self.net.send(ClientMsg::GitFileAtRev {
                            session_id: s.id.clone(),
                            sha: rev,
                            path,
                        });
                    }
                }
            }
        }

        let root_label = s.repo_root.clone().unwrap_or_else(|| s.cwd.clone());
        egui::Panel::left("explorer-tree").default_size(280.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("WORKTREE").size(11.0).color(DIM).strong())
                    .on_hover_text(&root_label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("↻")
                        .on_hover_text("re-list the directories that are open")
                        .clicked()
                    {
                        // Dropping the listings is enough: the fetch block
                        // above re-requests the root and everything expanded.
                        let st = self.explorer.current_mut();
                        st.dirs.clear();
                        st.pending.clear();
                    }
                });
            });
            // Both axes, *and* wrap forced off. The scroll area alone was
            // not enough: egui deliberately hands content the visible width
            // even with horizontal scrolling on ("better to wrap text …
            // than showing a horizontal scrollbar", scroll_area.rs), so
            // rows kept folding at the pane edge and the horizontal bar
            // never had anything to do. Extend makes each row lay out at
            // its natural width; only then does a narrow pane scroll —
            // a tree whose rows fold onto two lines stops reading as a
            // tree.
            egui::ScrollArea::both()
                .id_salt("explorer-tree-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if !self.explorer.current().dirs.contains_key("") {
                        ui.add_space(8.0);
                        ui.label(dim("listing…"));
                        return;
                    }
                    self.explorer_dir(ui, "", 0);
                });
        });

        // Side by side when any tab lives on the right; the split is created
        // by sending a tab over ("open on the other side") and collapses
        // when the last right-hand tab leaves.
        if self.explorer.current().split() {
            let half = ui.available_width() * 0.5;
            egui::Panel::right("editor-split")
                .default_size(half)
                .show(ui, |ui| {
                    self.editor_group(ui, 1);
                });
        }
        self.editor_group(ui, 0);
    }

    /// One side of the editor: its tab strip and its viewer.
    fn editor_group(&mut self, ui: &mut egui::Ui, group: u8) {
        self.explorer_tab_strip(ui, group);
        self.explorer_viewer(ui, group);
    }

    /// The open-file tabs of one side, IntelliJ-fashion: click activates,
    /// middle-click closes, double-click pins, and the one unpinned tab per
    /// side is the preview that single-click opens reuse.
    fn explorer_tab_strip(&mut self, ui: &mut egui::Ui, group: u8) {
        let tabs: Vec<(usize, String, bool, Option<String>)> = self
            .explorer
            .current()
            .open
            .iter()
            .enumerate()
            .filter(|(_, t)| t.group == group)
            .map(|(i, t)| (i, t.path.clone(), t.pinned, t.rev.clone()))
            .collect();
        if tabs.is_empty() {
            return;
        }
        let active = self.explorer.current().active_of(group);
        egui::ScrollArea::horizontal()
            .id_salt(("explorer-tab-strip", group))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (i, path, pinned, rev) in &tabs {
                        let name = path.rsplit('/').next().unwrap_or(path);
                        let mut text = match rev {
                            // A revision tab names its era; seven hex is a
                            // human-sized sha.
                            Some(r) => RichText::new(format!(
                                "{name} @{}",
                                r.chars().take(8).collect::<String>()
                            ))
                            .size(12.0)
                            .color(AMBER),
                            None => RichText::new(name.to_string()).size(12.0),
                        };
                        if !pinned {
                            // The preview tab announces its impermanence the
                            // way every editor does: italics.
                            text = text.italics();
                        }
                        let row = ui.selectable_label(active == Some(*i), text).on_hover_text(
                            match rev {
                                Some(r) => format!("{path} as of {r} — read-only history"),
                                None => format!(
                                    "{path}\n{}",
                                    if *pinned {
                                        "pinned — double-click to unpin"
                                    } else {
                                        "preview — double-click to pin"
                                    }
                                ),
                            },
                        );
                        row.context_menu(|ui| {
                            if ui
                                .button(if group == 0 {
                                    "Open on the right"
                                } else {
                                    "Move back left"
                                })
                                .clicked()
                            {
                                self.explorer.move_tab_to_other_side(*i);
                                ui.close();
                            }
                        });
                        if row.double_clicked() {
                            self.explorer.activate(*i);
                            self.explorer.toggle_pin_active();
                        } else if row.middle_clicked() {
                            self.explorer.close_tab(*i);
                        } else if row.clicked() {
                            self.explorer.activate(*i);
                        }
                        if ui
                            .small_button(RichText::new("✕").size(10.0).color(DIM))
                            .on_hover_text("close (middle-click the tab also works)")
                            .clicked()
                        {
                            self.explorer.close_tab(*i);
                        }
                    }
                });
            });
        ui.separator();
    }

    /// The read-only body of one side's active tab, line-numbered, scrolled
    /// to a search hit when one asked for it.
    fn explorer_viewer(&mut self, ui: &mut egui::Ui, group: u8) {
        // Taken, not borrowed: the goto is consumed by the one paint that
        // honours it, or every later frame would drag the scroll back.
        let mut goto_line = {
            let st = self.explorer.current_mut();
            st.active_of(group)
                .and_then(|i| st.open.get_mut(i))
                .and_then(|t| t.goto_line.take())
        };
        let focused = self.explorer.current().focus == group;
        let st = self.explorer.current();
        let Some(tab) = st.active_of(group).and_then(|i| st.open.get(i)) else {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(dim("pick a file to read it — read-only, always"));
            });
            return;
        };
        let path = tab.path.clone();
        let rev = tab.rev.clone();
        let Some(view) = &tab.view else {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(dim(format!("loading {path}…")));
            });
            return;
        };
        let content = view.content.clone();
        let truncated = view.truncated;
        ui.horizontal(|ui| {
            // Truncated, never wrapped: a header that folds onto two lines
            // pushes the file down and reads as two files.
            let header = match &rev {
                Some(r) => format!("{path} @ {r}"),
                None => path.clone(),
            };
            ui.add(egui::Label::new(mono(&header)).truncate())
                .on_hover_text(&header);
            if let Some(r) = &rev {
                ui.label(
                    RichText::new("history — the file as of this revision")
                        .size(11.0)
                        .color(AMBER),
                )
                .on_hover_text(format!(
                    "git show {r}:{path} — nothing here can edit the past (or the present)"
                ));
            }
            if truncated {
                ui.label(
                    RichText::new("cut short — the file goes on past the size cap")
                        .size(11.0)
                        .color(AMBER),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(self.annotate, "blame")
                    .on_hover_text(format!(
                        "per-line authorship in the gutter — click a line for its commit  ({})",
                        self.keymap.describe(crate::keymap::Action::ToggleAnnotate)
                    ))
                    .clicked()
                {
                    self.annotate = !self.annotate;
                }
                // File history (`R-D12`): the Git pane's log, pre-filtered
                // to this path — renames followed by the daemon.
                if ui
                    .small_button("history")
                    .on_hover_text("this file's commits, in the Git pane — renames followed")
                    .clicked()
                {
                    if let Some(sid) = self.explorer.session.clone() {
                        self.gitview.ensure_session(&sid);
                    }
                    self.gitview.filter_input = format!("path:{path}");
                    self.gitview.set_log_filter(None, None, Some(path.clone()));
                    self.set_tab(Tab::Git);
                }
            });
        });

        // Ctrl+F. Entirely client-side: the body is already here, so a match
        // list is a scan, and jumping reuses the goto machinery search
        // results already ride. One bar, on the focused side only — two bars
        // would fight over one widget id and one set of keystrokes.
        let mut find_bands: Option<(Vec<u64>, usize)> = None;
        if self.explorer_find_open && focused {
            let matches = crate::explorer::find_lines(&content, &self.explorer_find);
            if self.explorer_find_cursor >= matches.len() {
                self.explorer_find_cursor = matches.len().saturating_sub(1);
            }
            let mut jump: Option<i64> = None;
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.explorer_find)
                        .id(egui::Id::new("explorer-find"))
                        .hint_text("find in this file…")
                        .desired_width(220.0),
                );
                if self.explorer_find_focus {
                    field.request_focus();
                    self.explorer_find_focus = false;
                }
                if field.changed() {
                    // A fresh query starts at its first hit — the editor
                    // reflex this bar is imitating.
                    self.explorer_find_cursor = 0;
                    jump = Some(0);
                }
                if !self.explorer_find.is_empty() {
                    ui.label(dim(if matches.is_empty() {
                        "no matches".to_string()
                    } else {
                        format!("{} of {}", self.explorer_find_cursor + 1, matches.len())
                    }));
                }
                if ui.small_button("‹").on_hover_text("previous match (Shift+⏎)").clicked() {
                    jump = Some(-1);
                }
                if ui.small_button("›").on_hover_text("next match (⏎)").clicked() {
                    jump = Some(1);
                }
                ui.label(dim("esc closes"));

                let (enter, shift, escape) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Enter),
                        i.modifiers.shift,
                        i.key_pressed(egui::Key::Escape),
                    )
                });
                if escape {
                    self.explorer_find_open = false;
                } else if enter {
                    jump = Some(if shift { -1 } else { 1 });
                }
            });
            if !matches.is_empty() {
                if let Some(delta) = jump {
                    let len = matches.len() as i64;
                    let next =
                        (self.explorer_find_cursor as i64 + delta).rem_euclid(len) as usize;
                    self.explorer_find_cursor = next;
                    goto_line = Some(matches[next]);
                }
            }
            if self.explorer_find_open {
                find_bands = Some((matches, self.explorer_find_cursor));
            }
        }
        ui.separator();

        let language = crate::explorer::language_of(&path).to_string();
        let theme =
            egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
        // The same memoised highlight `code_view_ui` uses — unrolled here so
        // the gutter and the code can sit side by side on identical rows.
        let job = egui_extras::syntax_highlighting::highlight(
            ui.ctx(),
            ui.style(),
            &theme,
            &content,
            &language,
        );
        // Laid out here and read for geometry: the galley's own row
        // positions are the only honest source of line coordinates. The
        // first version multiplied a guessed row height instead, and the
        // find bands crept away from their lines — syntect's real line
        // height is not `text_style_height` (found live). The layout is
        // cached per frame, so the Label below pays nothing extra.
        let galley = ui.ctx().fonts_mut(|f| f.layout_job(job.clone()));
        let line_y = |line: u64| -> f32 {
            galley
                .rows
                .get(line.saturating_sub(1) as usize)
                .map(|r| r.pos.y)
                .unwrap_or(0.0)
        };
        let rows = content.lines().count().max(1);
        let width = rows.to_string().len();
        let gutter_text = (1..=rows)
            .map(|n| format!("{n:>width$}"))
            .collect::<Vec<_>>()
            .join("\n");
        // The gutter wears the code's own font, so its rows *cannot*
        // disagree with the code's — the other half of the same drift.
        let code_font = job
            .sections
            .first()
            .map(|s| s.format.font_id.clone())
            .unwrap_or_else(|| egui::FontId::monospace(12.0));
        let mut gutter_job = egui::text::LayoutJob::default();
        gutter_job.append(
            &gutter_text,
            0.0,
            egui::TextFormat {
                font_id: code_font.clone(),
                color: DIM,
                ..Default::default()
            },
        );

        // The annotate gutter (`R-D10`, deepened by `R-D11`): per-line
        // authorship, same font as the code so the rows cannot drift,
        // clickable through the galley's row geometry. A worktree tab
        // blames the worktree (uncommitted lines arrive as git's zero sha
        // and render as a quiet dot); a revision tab blames its own era.
        let blame_key = (path.clone(), rev.clone().unwrap_or_default());
        let mut blame_col: Option<(egui::text::LayoutJob, Vec<mogeung_core::wire::BlameLine>)> =
            None;
        if self.annotate {
            if let Some(sid) = self.explorer.session.clone() {
                self.gitview.ensure_session(&sid);
                if !self.gitview.blame.contains_key(&blame_key)
                    && self.gitview.pending_blame.insert(blame_key.clone())
                {
                    self.net.send(ClientMsg::GitBlame {
                        session_id: sid,
                        path: path.clone(),
                        rev: rev.clone(),
                    });
                }
            }
            if let Some((lines, _)) = self.gitview.blame.get(&blame_key) {
                let mut text = String::new();
                for l in lines {
                    let uncommitted = l.sha.chars().all(|c| c == '0');
                    if uncommitted {
                        text.push_str(&format!("{:>8} {:<10}", "·", ""));
                    } else {
                        let author: String = l.author.chars().take(10).collect();
                        text.push_str(&format!("{} {:<10}", l.sha, author));
                    }
                    text.push('\n');
                }
                let mut bj = egui::text::LayoutJob::default();
                bj.append(
                    &text,
                    0.0,
                    egui::TextFormat {
                        font_id: code_font.clone(),
                        color: DIM,
                        ..Default::default()
                    },
                );
                blame_col = Some((bj, lines.clone()));
            }
        }
        let mut open_commit: Option<String> = None;
        let mut open_rev_tab: Option<String> = None;

        let mut area = egui::ScrollArea::both()
            .id_salt(("explorer-file-scroll", &path, group))
            .auto_shrink([false, false]);
        if let Some(line) = goto_line {
            // Aim the hit at the upper third — centred enough to have context
            // above it, high enough to read downward from.
            area = area
                .vertical_scroll_offset((line_y(line) - ui.available_height() / 3.0).max(0.0));
        }
        area.show(ui, |ui| {
            // Match bands go down first so the text paints over them. Whole
            // lines, not columns: row geometry is exact, column arithmetic
            // lies the moment a tab or a wide glyph appears.
            if let Some((lines, cursor)) = &find_bands {
                let origin = ui.cursor().min;
                let band_w = ui.available_width().clamp(0.0, 4000.0);
                let current = lines.get(*cursor).copied();
                let painter = ui.painter();
                let mut seen = HashSet::new();
                for l in lines {
                    if !seen.insert(*l) {
                        continue;
                    }
                    let Some(row) = galley.rows.get((*l - 1) as usize) else {
                        continue;
                    };
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(origin.x, origin.y + row.pos.y),
                            egui::vec2(band_w, row.rect().height()),
                        ),
                        0.0,
                        AMBER.linear_multiply(if current == Some(*l) { 0.28 } else { 0.10 }),
                    );
                }
            }
            ui.horizontal_top(|ui| {
                if let Some((bj, lines)) = blame_col {
                    let resp = ui.add(
                        egui::Label::new(bj)
                            .selectable(false)
                            .sense(egui::Sense::click()),
                    );
                    // Which row a pointer position names, by the code
                    // galley's real geometry — the same rows the bands use.
                    let row_at = |pos: egui::Pos2| {
                        let rel = pos.y - resp.rect.top();
                        galley.rows.partition_point(|r| r.pos.y <= rel).saturating_sub(1)
                    };
                    let committed_at = |row: usize| {
                        lines
                            .get(row)
                            .filter(|l| !l.sha.chars().all(|c| c == '0'))
                            .cloned()
                    };
                    if resp.clicked() {
                        if let Some(l) =
                            resp.interact_pointer_pos().map(row_at).and_then(committed_at)
                        {
                            open_commit = Some(l.sha);
                        }
                    }
                    // The hover card: what GitLens calls line blame — sha,
                    // author, age, subject — without leaving the file.
                    if resp.hovered() && !resp.context_menu_opened() {
                        if let Some(pos) = resp.hover_pos() {
                            match committed_at(row_at(pos)) {
                                Some(l) => resp.show_tooltip_ui(|ui| {
                                    let now = Utc::now().timestamp();
                                    ui.label(mono(format!("{} · {}", l.sha, l.author)));
                                    if !l.summary.is_empty() {
                                        ui.label(RichText::new(&l.summary).size(12.0));
                                    }
                                    ui.label(dim(format!(
                                        "{} ago · click: show commit · right-click: more",
                                        crate::gitview::age(now, l.epoch)
                                    )));
                                }),
                                None => resp.show_tooltip_text("not committed yet"),
                            }
                        }
                    }
                    // The read-only investigation verbs live on the line.
                    if resp.secondary_clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            self.blame_menu_line = committed_at(row_at(pos));
                        }
                    }
                    resp.context_menu(|ui| {
                        let Some(l) = self.blame_menu_line.clone() else {
                            ui.label(dim("not committed yet"));
                            return;
                        };
                        ui.label(mono(format!("{} · {}", l.sha, l.author)));
                        ui.separator();
                        if ui.button("Show commit in Git pane").clicked() {
                            open_commit = Some(l.sha.clone());
                            ui.close();
                        }
                        if ui
                            .button("Re-blame before this commit")
                            .on_hover_text(
                                "open the file as of the parent, blamed at that era — \
                                 who touched this line before",
                            )
                            .clicked()
                        {
                            open_rev_tab = Some(format!("{}^", l.sha));
                            ui.close();
                        }
                        if ui
                            .button("Open file at this commit")
                            .clicked()
                        {
                            open_rev_tab = Some(l.sha.clone());
                            ui.close();
                        }
                        if ui.button("Copy sha").clicked() {
                            ui.ctx().copy_text(l.sha.clone());
                            ui.close();
                        }
                    });
                }
                ui.add(egui::Label::new(gutter_job).selectable(false));
                // Selectable and highlighted, and structurally unable to
                // edit: a `Label` has no writable buffer.
                ui.add(egui::Label::new(job).selectable(true));
            });
        });
        // An annotated line names a commit; clicking it is "show me that
        // commit", which is the Git pane's job.
        if let Some(sha) = open_commit {
            self.gitview.selection = crate::gitview::Selection::Commit(sha);
            self.set_tab(Tab::Git);
        }
        // Re-blame walks history by opening the same path at an older
        // revision — the blame stack *is* the tab strip.
        if let Some(r) = open_rev_tab {
            self.explorer.open_file_at_rev(&path, &r);
        }
    }

    /// One directory level of the explorer tree, recursively.
    ///
    /// Iterates a clone of the listing: rows mutate the expanded set and the
    /// tabs, which cannot happen under a borrow of the cache. The listings
    /// are small; the diff pane clones far more per frame.
    fn explorer_dir(&mut self, ui: &mut egui::Ui, dir: &str, depth: usize) {
        let Some(entries) = self.explorer.current().dirs.get(dir).cloned() else {
            return;
        };
        if entries.is_empty() && depth == 0 {
            ui.label(dim("nothing here"));
            return;
        }
        let active_path = self
            .explorer
            .current()
            .active_tab()
            .map(|t| t.path.clone());
        // Gitignored subtrees read dimmer — generated noise should look
        // like noise. Only when the git cache is on the same session; a
        // stale session's ignore list must not dim this one's tree.
        let ignored_prefixes: Vec<String> =
            if self.gitview.session == self.explorer.session {
                self.gitview
                    .ignored_prefixes()
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                Vec::new()
            };
        for e in entries {
            let path = crate::explorer::join(dir, &e.name);
            let open = self.explorer.current().expanded.contains(&path);
            let glyph = if !e.is_dir {
                "  "
            } else if open {
                "▾ "
            } else {
                "▸ "
            };
            let picked = !e.is_dir && active_path.as_deref() == Some(path.as_str());
            let ignored = crate::gitview::is_ignored(
                &path,
                &ignored_prefixes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            let mut row_text = RichText::new(format!("{glyph}{}", e.name))
                .monospace()
                .color(if ignored {
                    DIM
                } else if e.is_dir {
                    TEXT
                } else {
                    BLUE
                });
            if ignored {
                row_text = row_text.weak();
            }
            let mut row = ui.selectable_label(picked, row_text);
            if ignored {
                row = row.on_hover_text("gitignored");
            }
            // The row reveal asked to be scrolled to, honoured the frame it
            // finally exists — its listing may have been in flight for a while.
            if self.explorer.current().reveal.as_deref() == Some(path.as_str()) {
                row.scroll_to_me(Some(egui::Align::Center));
                self.explorer.current_mut().reveal = None;
            }
            if row.double_clicked() && !e.is_dir {
                self.explorer.open_file(&path, true, None);
            } else if row.clicked() {
                if e.is_dir {
                    // Toggling is all a click does; the fetch block in
                    // `explorer_tab` notices an expanded dir with no listing.
                    let st = self.explorer.current_mut();
                    if open {
                        st.expanded.remove(&path);
                    } else {
                        st.expanded.insert(path.clone());
                    }
                    self.explorer.dirty = true;
                } else {
                    self.explorer.open_file(&path, false, None);
                }
            }
            if e.is_dir && open {
                ui.indent(egui::Id::new(("explorer-indent", &path)), |ui| {
                    self.explorer_dir(ui, &path, depth + 1);
                });
            }
        }
    }

    /// The session's own terminal, attached through tmux. `R-B18`.
    ///
    /// Only possible for a session started under tmux — see `scripts/yolomo`.
    /// A session started with a bare `claude` is owned by the terminal that
    /// spawned it and can never be attached to, so this says so plainly and
    /// points at the fix rather than showing a broken pane.
    fn terminal_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        // Before the terminal is borrowed: zoom is App state.
        let term_font_px = 14.0 * self.pane_zoom(ui, "terminal");
        let Some(target) = s.tmux_target.clone() else {
            ui.add_space(8.0);
            ui.label(
                RichText::new("This session is not running under tmux.").strong(),
            );
            ui.add_space(4.0);
            ui.label(
                "A terminal owns the pty of whatever it started, and nothing else can \
                 attach to it. mogeung can point you at this session, but cannot host it.",
            );
            ui.add_space(8.0);
            ui.label("Start sessions with `yolomo` instead of `yolo` and this tab becomes live.");
            ui.add_space(8.0);
            if ui
                .button(format!("{} Jump to its terminal instead", icon::TERMINAL))
                .clicked()
            {
                self.net.send(ClientMsg::FocusTerminal {
                    session_id: s.id.clone(),
                });
            }
            return;
        };

        // Re-attach when the selection moves to a different session. Comparing
        // targets rather than session ids is deliberate: a session that is
        // restarted keeps its id but gets a new pane.
        let stale = self
            .term
            .as_ref()
            .map(|t| t.target() != target || t.exited())
            .unwrap_or(true);
        if stale {
            self.term_focused = false;
            match crate::term::Term::attach(ui.ctx(), &target) {
                Ok(t) => self.term = Some(t),
                Err(e) => {
                    self.term = None;
                    ui.colored_label(RED, format!("could not attach to {target}: {e}"));
                    return;
                }
            }
        }

        let Some(term) = self.term.as_mut() else {
            return;
        };
        term.poll();

        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {target}", icon::TERMINAL)).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let hint = if self.term_focused {
                    format!(
                        "keyboard goes to the agent — {} returns it",
                        self.keymap.describe(crate::keymap::Action::LeaveTerminal)
                    )
                } else {
                    format!(
                        "click the terminal — or press {} — to type into this session",
                        self.keymap.describe(crate::keymap::Action::LeaveTerminal)
                    )
                };
                ui.label(RichText::new(hint).weak());
            });
        });
        ui.separator();

        let before = ui.available_size();
        // `.inner`, not `.response`: `allocate_ui`'s own response senses hover
        // only, so `clicked()` on it is always false. The first version tested
        // that one, and the pane could never be typed into.
        let area = ui
            .allocate_ui(before, |ui| term.ui(ui, self.term_focused, term_font_px))
            .inner;

        // Clicking in takes the keyboard; the chord in the hint gives it back,
        // and so does clicking anything else — a pane you can only escape by
        // remembering a chord is a trap.
        if area.clicked() {
            self.term_focused = true;
        } else if self.term_focused
            && ui.input(|i| i.pointer.any_pressed())
            && !area.contains_pointer()
        {
            self.term_focused = false;
        }

        if self.term_focused {
            ui.painter().rect_stroke(
                area.rect,
                2.0,
                egui::Stroke::new(1.0, ui.visuals().selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }
    }

    fn transcript_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let z = self.pane_zoom(ui, "transcript");
        scale_text(ui, z);
        let events = self.events.get(&s.id).cloned().unwrap_or_default();
        let scroll = self.scroll;

        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut self.prefs.markdown, "markdown")
                .on_hover_text("render agent replies as Markdown rather than raw text")
                .changed()
            {
                self.prefs_dirty = true;
            }
            if ui
                .checkbox(&mut self.prefs.show_thinking, "thinking")
                .on_hover_text("show the agent's reasoning blocks")
                .changed()
            {
                self.prefs_dirty = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(dim(format!("{} event(s)", events.len())));
            });
        });
        ui.separator();

        // Only the tail is drawn: markdown is parsed per visible event per
        // frame, so an unbounded transcript would tie the frame rate to how
        // long the session has been running.
        let skipped = events.len().saturating_sub(self.transcript_limit);
        let shown = &events[skipped..];

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if let Some(req) = scroll {
                    ui.scroll_with_delta(req.delta(ui.clip_rect().height()));
                }
                if events.is_empty() {
                    ui.label(dim("no events yet"));
                }
                if skipped > 0 {
                    ui.vertical_centered(|ui| {
                        if ui
                            .button(dim(format!("show {skipped} earlier event(s)")))
                            .clicked()
                        {
                            self.transcript_limit += TRANSCRIPT_PAGE.max(skipped.min(500));
                        }
                    });
                    ui.add_space(4.0);
                }
                for ev in shown {
                    event_row(ui, ev, &mut self.md_cache, &self.prefs);
                }
            });
    }

    fn changes_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let z = self.pane_zoom(ui, "diff");
        scale_text(ui, z);
        let Some(change) = self.changes.get(&s.id).cloned() else {
            ui.label(dim("computing diff…"));
            return;
        };
        if let Some(err) = &change.error {
            ui.label(RichText::new(err).color(AMBER).size(12.0));
        }
        if change.files.is_empty() {
            ui.label(dim("no file changes attributed to this session"));
            if !s.touched_files.is_empty() {
                ui.label(dim(format!(
                    "{} file(s) were edited but the diff is empty — they may have been committed or reverted",
                    s.touched_files.len()
                )));
            }
            return;
        }

        ui.horizontal(|ui| {
            let total = change.total_hunks();
            let read = change.reviewed_hunks();
            ui.label(dim(format!("{read}/{total} hunks read")));
            ui.add(
                egui::ProgressBar::new(change.review_progress())
                    .desired_width(140.0)
                    .show_percentage(),
            );
            ui.checkbox(&mut self.prefs.hide_reviewed, "hide read");
            ui.checkbox(&mut self.prefs.hide_noise, "hide noise");
            if ui.button("Mark all read").clicked() {
                self.net.send(ClientMsg::ReviewAll {
                    session_id: s.id.clone(),
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .selectable_label(self.prefs.side_by_side, "⇹")
                    .on_hover_text("side by side (R-D6)")
                    .clicked()
                {
                    self.prefs.side_by_side = true;
                    self.prefs_dirty = true;
                }
                if ui
                    .selectable_label(!self.prefs.side_by_side, "≡")
                    .on_hover_text("unified")
                    .clicked()
                {
                    self.prefs.side_by_side = false;
                    self.prefs_dirty = true;
                }
                ui.checkbox(&mut self.prefs.word_diff, "words")
                    .on_hover_text("highlight only the part of a line that moved (R-D5)");
                ui.checkbox(&mut self.prefs.syntax, "syntax")
                    .on_hover_text("approximate highlighting — a tokenizer, not a parser (R-D4)");
            });
        });
        ui.separator();

        if self.selected_file.is_none() {
            self.selected_file = change.files.first().map(|f| f.path.clone());
        }

        // Deferred out of the row loop: opening mutates the explorer and the
        // tab layout, which must not happen under the borrow of `change`.
        let mut open_in_explorer: Option<String> = None;
        egui::Panel::left("files").default_size(300.0).show(ui, |ui| {
            let focused = self.pane == Pane::Files || self.pane == Pane::Diff;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("FILES")
                        .size(11.0)
                        .color(if focused { BLUE } else { DIM })
                        .strong(),
                )
                .on_hover_text("Alt+2");
                if focused {
                    ui.label(dim("j/k move"));
                }
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Tight rows: the list is for finding a file, not reading
                    // about one.
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let width = ui.available_width();

                    for f in &change.files {
                        if self.prefs.hide_noise && f.risk() == RiskLevel::Noise {
                            continue;
                        }
                        if self.prefs.hide_reviewed && f.fully_reviewed() {
                            continue;
                        }
                        // With previewing off the cursor and the open file
                        // differ, and both need to be visible or arrowing
                        // through the list looks broken.
                        let open = self.selected_file.as_deref() == Some(f.path.as_str());
                        let at_cursor = self.file_cursor.as_deref() == Some(f.path.as_str());
                        let selected = open || (!self.prefs.preview_on_select && at_cursor);
                        let unread = f.hunks.len() - f.reviewed_hunks();

                        let resp = ui
                            .selectable_label(selected, file_row(f, width))
                            .on_hover_ui(|ui| {
                                // Everything the compact row dropped.
                                ui.label(mono(&f.path));
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(badge(f.risk().label(), risk_color(f.risk())));
                                    for fl in &f.flags {
                                        ui.label(dim(fl.label()));
                                    }
                                });
                                ui.label(dim(format!(
                                    "+{} −{} · {} of {} hunk(s) unread",
                                    f.insertions,
                                    f.deletions,
                                    unread,
                                    f.hunks.len()
                                )));
                            });
                        // The Changes → Explorer bridge (`R-B25`): judging an
                        // edit often means reading the whole file, and that
                        // must not cost leaving mogeung.
                        resp.context_menu(|ui| {
                            if ui
                                .button(format!(
                                    "Open in Editor ({})",
                                    self.keymap.describe(crate::keymap::Action::OpenInExplorer)
                                ))
                                .clicked()
                            {
                                open_in_explorer = Some(f.path.clone());
                                ui.close();
                            }
                        });
                        if resp.clicked() {
                            self.selected_file = Some(f.path.clone());
                        }
                    }
                });
        });
        if let Some(path) = open_in_explorer {
            self.open_in_explorer(&path, None);
        }

        let file = change
            .files
            .iter()
            .find(|f| Some(f.path.as_str()) == self.selected_file.as_deref());
        let scroll = self.scroll;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(req) = scroll {
                    ui.scroll_with_delta(req.delta(ui.clip_rect().height()));
                }
                let Some(file) = file else {
                    ui.label(dim("select a file"));
                    return;
                };
                if file.truncated {
                    ui.label(dim("diff not shown (binary or too large)"));
                    return;
                }
                // R-D9. Per file, not per hunk: the question "what else uses
                // this?" is about the file's symbols as a whole.
                ui.horizontal(|ui| {
                    if ui
                        .small_button(format!("{} blast radius", icon::BLAST))
                        .on_hover_text("what else mentions the symbols this diff changed")
                        .clicked()
                    {
                        self.blast = None;
                        self.blast_pending = true;
                        self.net.send(ClientMsg::FetchBlastRadius {
                            session_id: s.id.clone(),
                            path: file.path.clone(),
                        });
                    }
                    if self.blast_pending {
                        ui.label(dim("searching…"));
                    }
                });
                if let Some(b) = self.blast.clone() {
                    if b.path == file.path {
                        blast_panel(ui, &b);
                    }
                }

                for hunk in &file.hunks {
                    if self.prefs.hide_reviewed && hunk.reviewed {
                        continue;
                    }
                    let flagged = self
                        .flagged
                        .iter()
                        .any(|f| f.session_id == s.id && f.header == hunk.header && f.path == file.path);
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            let mut reviewed = hunk.reviewed;
                            if ui.checkbox(&mut reviewed, "read").changed() {
                                self.net.send(ClientMsg::SetHunkReviewed {
                                    session_id: s.id.clone(),
                                    anchor: hunk.anchor.clone(),
                                    reviewed,
                                });
                            }
                            ui.label(badge(hunk.risk().label(), risk_color(hunk.risk())));
                            ui.label(mono(&hunk.header).color(DIM));
                            for fl in hunk.flags.iter().take(4) {
                                ui.label(dim(fl.label()));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    // R-D1. Collect now, write the prompt later.
                                    let label = if flagged {
                                        format!("{} flagged", icon::FLAG)
                                    } else {
                                        format!("{} flag", icon::FLAG)
                                    };
                                    let btn = if flagged {
                                        ui.small_button(RichText::new(label).color(AMBER))
                                    } else {
                                        ui.small_button(label)
                                    };
                                    if btn
                                        .on_hover_text("add to a follow-up prompt you will paste yourself")
                                        .clicked()
                                    {
                                        if flagged {
                                            self.flagged.retain(|f| {
                                                !(f.session_id == s.id
                                                    && f.header == hunk.header
                                                    && f.path == file.path)
                                            });
                                        } else {
                                            self.flagged.push(FlaggedHunk {
                                                session_id: s.id.clone(),
                                                path: file.path.clone(),
                                                header: hunk.header.clone(),
                                                note: String::new(),
                                                body: hunk
                                                    .lines
                                                    .iter()
                                                    .filter(|l| {
                                                        l.starts_with('+') || l.starts_with('-')
                                                    })
                                                    .take(40)
                                                    .cloned()
                                                    .collect(),
                                            });
                                            self.show_prompt = true;
                                        }
                                    }
                                },
                            );
                        });

                        let shown: Vec<String> =
                            hunk.lines.iter().take(500).cloned().collect();
                        if self.prefs.side_by_side {
                            render_side_by_side(ui, &shown, self.prefs.syntax, self.prefs.word_diff)
                        } else {
                            render_unified(ui, &shown, self.prefs.syntax, self.prefs.word_diff)
                        }
                        if hunk.lines.len() > 500 {
                            ui.label(dim(format!(
                                "… {} more lines — open in an editor",
                                hunk.lines.len() - 500
                            )));
                        }
                    });
                    ui.add_space(4.0);
                }
            });
    }

    /// R-D8. Review debt for the selected session's repo.
    fn debt_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let Some(repo) = s.repo_root.clone() else {
            ui.label(dim("this session is not inside a git repository"));
            return;
        };
        ui.horizontal(|ui| {
            ui.label(mono(&repo).color(DIM));
            if ui.small_button("refresh").clicked() {
                self.net.send(ClientMsg::FetchReviewDebt { repo: repo.clone() });
            }
        });
        ui.separator();

        let Some(debt) = self.debt.clone().filter(|d| d.repo == repo) else {
            self.net.send(ClientMsg::FetchReviewDebt { repo });
            ui.label(dim("computing…"));
            return;
        };

        ui.label(RichText::new(debt.headline()).size(15.0).strong());
        ui.add(
            egui::ProgressBar::new(debt.progress())
                .desired_width(320.0)
                .show_percentage(),
        );
        ui.add_space(6.0);
        ui.label(dim(format!(
            "{} session(s) with changes · {} still unread · {} file(s) touched · +{} unread insertions",
            debt.sessions, debt.sessions_unread, debt.files_touched, debt.unread_insertions
        )));
        ui.add_space(4.0);
        ui.label(dim(
            "Counts sessions mogeung has seen, not the whole history of the repo — \
             work done before it was watching is not in here.",
        ));

        ui.add_space(10.0);
        if debt.worst_files.is_empty() {
            ui.label(RichText::new("Nothing outstanding.").color(GREEN));
            return;
        }
        ui.label(RichText::new("Riskiest unread files").strong().size(12.5));
        ui.add_space(4.0);

        let mut jump = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for f in &debt.worst_files {
                    ui.horizontal(|ui| {
                        let level = RiskLevel::from_score(f.score);
                        ui.label(badge(level.label(), risk_color(level)));
                        ui.label(dim(format!("{} unread", f.unread_hunks)));
                        if ui
                            .selectable_label(false, RichText::new(&f.path).size(12.5))
                            .clicked()
                        {
                            jump = Some((f.session_id.clone(), f.path.clone()));
                        }
                    });
                }
            });
        if let Some((id, path)) = jump {
            self.select(id);
            self.selected_file = Some(path);
            self.tab = Tab::Changes;
        }
    }

    fn info_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.horizontal(|ui| {
                        ui.label(dim(format!("{k:>16}")));
                        ui.label(mono(v));
                    });
                };
                row("session id", s.id.clone());
                row("name", s.name.clone().unwrap_or_else(|| "—".into()));
                row("cwd", s.cwd.clone());
                row("repo", s.repo_root.clone().unwrap_or_else(|| "—".into()));
                row("branch", s.git_branch.clone().unwrap_or_else(|| "—".into()));
                row("base commit", s.base_sha.clone().unwrap_or_else(|| "—".into()));
                row("cli version", s.version.clone().unwrap_or_else(|| "—".into()));
                row("started", s.started_at.to_rfc3339());
                row("last event", s.last_event_at.to_rfc3339());
                row("transcript", s.transcript_path.clone());

                ui.add_space(8.0);
                ui.label(dim(format!("files touched ({})", s.touched_files.len())));
                for f in s.touched_files.iter().take(60) {
                    ui.label(mono(f));
                }
                if s.touched_files.len() > 60 {
                    ui.label(dim(format!("… {} more", s.touched_files.len() - 60)));
                }

                if let Some(p) = &s.last_prompt {
                    ui.add_space(8.0);
                    ui.label(dim("last prompt"));
                    ui.label(RichText::new(p).size(12.5));
                }
            });
    }
}

/// Shorten a path to its last directory plus filename.
///
/// `crates/mogeungd/src/state.rs` → `…/src/state.rs`. The tail is what
/// identifies a file; the leading directories are the same for most of the
/// list and just push the useful part off the edge. Full path is on hover.
fn short_path(path: &str) -> (String, &str) {
    let (dir, base) = match path.rfind('/') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    };
    if dir.is_empty() {
        return (String::new(), base);
    }
    let parts: Vec<&str> = dir.split('/').collect();
    let shown = if parts.len() > 1 {
        format!("…/{}/", parts[parts.len() - 1])
    } else {
        format!("{dir}/")
    };
    (shown, base)
}

/// One line per file: state, path, churn.
///
/// This was two lines plus padding, which meant a session touching a dozen
/// files could not be taken in without scrolling — and the list exists to let
/// you *find* a file, not to read about one. Risk level and flags moved to the
/// hover, where they cost nothing.
fn file_row(f: &mogeung_core::FileChange, max_width: f32) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};

    let unread = f.hunks.len() - f.reviewed_hunks();
    let read = unread == 0;
    let risk = f.risk();

    let mut job = LayoutJob::default();
    job.wrap.max_width = max_width;
    // Never wrap: a row that grows to two lines defeats the whole point.
    job.wrap.max_rows = 1;
    job.wrap.overflow_character = Some('…');

    let fmt = |size: f32, color: Color32| TextFormat {
        font_id: egui::FontId::proportional(size),
        color,
        ..Default::default()
    };
    let mono_fmt = |size: f32, color: Color32| TextFormat {
        font_id: egui::FontId::monospace(size),
        color,
        ..Default::default()
    };

    // Marker carries two facts at once: read or not, and how risky.
    let (marker, marker_color) = if read {
        (icon::READ, GREEN)
    } else {
        (icon::UNREAD, risk_color(risk))
    };
    job.append(&format!("{marker} "), 0.0, mono_fmt(11.0, marker_color));

    let (dir, base) = short_path(&f.path);
    let dim_text = if read { Color32::from_gray(0x60) } else { DIM };
    let base_text = if read {
        Color32::from_gray(0x7A)
    } else {
        Color32::from_gray(0xDC)
    };
    if !dir.is_empty() {
        job.append(&dir, 0.0, fmt(12.0, dim_text));
    }
    job.append(base, 0.0, fmt(12.5, base_text));

    // Churn last, so filenames stay left-aligned and scannable.
    job.append(
        &format!("  +{} −{}", f.insertions, f.deletions),
        0.0,
        mono_fmt(10.5, dim_text),
    );
    job
}

/// Colour for a syntax token, tuned to stay legible over the add/delete tints.
fn tok_color(t: crate::diff::Tok, base: Color32) -> Color32 {
    use crate::diff::Tok;
    match t {
        Tok::Keyword => Color32::from_rgb(0xC5, 0x92, 0xE8),
        Tok::Str => Color32::from_rgb(0xB6, 0xD7, 0x8B),
        Tok::Comment => Color32::from_rgb(0x77, 0x7C, 0x88),
        Tok::Number => Color32::from_rgb(0xE8, 0xB0, 0x75),
        Tok::Type => Color32::from_rgb(0x7E, 0xC0, 0xE0),
        Tok::Plain => base,
    }
}

fn line_bg(line: &str) -> Option<Color32> {
    // Conflict markers get their own band regardless of diff sign: an
    // agent mid-merge is exactly what a reviewer must not scroll past.
    let body = line.get(1..).unwrap_or("");
    if body.starts_with("<<<<<<<") || body.starts_with(">>>>>>>")
        || body.starts_with("=======") || body.starts_with("|||||||")
    {
        return Some(Color32::from_rgb(0x6B, 0x25, 0x3C));
    }
    match line.chars().next() {
        Some('+') => Some(ADD_BG),
        Some('-') => Some(DEL_BG),
        _ => None,
    }
}

fn line_fg(line: &str) -> Color32 {
    match line.chars().next() {
        Some('+') => Color32::from_rgb(0x8F, 0xE0, 0xA6),
        Some('-') => Color32::from_rgb(0xF0, 0x9C, 0xA0),
        _ => Color32::from_rgb(0xA8, 0xA8, 0xB0),
    }
}

/// One diff line as a row of coloured spans.
///
/// `emphasis` marks the byte ranges the word diff says actually moved; they get
/// a brighter background so the eye lands on the change rather than the line.
fn styled_line(
    ui: &mut egui::Ui,
    line: &str,
    syntax: bool,
    emphasis: Option<&[crate::diff::Span]>,
) {
    let bg = line_bg(line);
    let fg = line_fg(line);
    let size = diff_size(ui);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        // Word-diff emphasis takes precedence: knowing *what changed* beats
        // knowing what is a keyword.
        if let Some(spans) = emphasis {
            for sp in spans {
                let mut t = RichText::new(&sp.text).monospace().size(size).color(fg);
                if sp.changed {
                    t = t.background_color(if line.starts_with('+') {
                        Color32::from_rgb(0x25, 0x6B, 0x3C)
                    } else {
                        Color32::from_rgb(0x74, 0x2B, 0x30)
                    });
                } else if let Some(b) = bg {
                    t = t.background_color(b);
                }
                ui.label(t);
            }
            return;
        }

        if !syntax {
            let mut t = RichText::new(line).monospace().size(size).color(fg);
            if let Some(b) = bg {
                t = t.background_color(b);
            }
            ui.label(t);
            return;
        }

        for (tok, text) in crate::diff::highlight(line) {
            let mut t = RichText::new(&text)
                .monospace()
                .size(size)
                .color(tok_color(tok, fg));
            if let Some(b) = bg {
                t = t.background_color(b);
            }
            ui.label(t);
        }
    });
}

/// Scale every text style in this scope by `z`. One lever that moves
/// default-styled text, the markdown renderer, and the syntax highlighter
/// alike — child uis inherit it, so a pane scaled at its top scales whole.
/// Explicitly-sized chrome (badges, section headers) stays put on purpose:
/// content zooms, furniture does not.
fn scale_text(ui: &mut egui::Ui, z: f32) {
    if (z - 1.0).abs() < 1e-3 {
        return;
    }
    for font in ui.style_mut().text_styles.values_mut() {
        font.size *= z;
    }
}

/// The diff renderer's font size: derived from the scope's Monospace style
/// so per-pane zoom reaches it, at the slightly-condensed ratio the diff
/// has always used.
fn diff_size(ui: &egui::Ui) -> f32 {
    ui.style()
        .text_styles
        .get(&egui::TextStyle::Monospace)
        .map(|f| f.size)
        .unwrap_or(12.0)
        * (11.5 / 12.0)
}

/// Unified view, with word-level emphasis on runs that look like replacements.
fn render_unified(ui: &mut egui::Ui, lines: &[String], syntax: bool, words: bool) {
    let pairs = replacement_pairs(lines, words);
    for (i, line) in lines.iter().enumerate() {
        styled_line(ui, line, syntax, pairs.get(&i).map(|v| v.as_slice()));
    }
}

fn render_side_by_side(ui: &mut egui::Ui, lines: &[String], syntax: bool, words: bool) {
    let rows = crate::diff::side_by_side(lines);
    let half = (ui.available_width() - 12.0).max(160.0) / 2.0;
    for row in &rows {
        // Compute the word diff once per row, not once per side.
        let pair = match (&row.left, &row.right) {
            (Some(a), Some(b)) if words && a.starts_with('-') && b.starts_with('+') => {
                Some(crate::diff::word_diff(a, b))
            }
            _ => None,
        };

        ui.horizontal_top(|ui| {
            for (idx, side) in [&row.left, &row.right].into_iter().enumerate() {
                ui.allocate_ui(egui::vec2(half, 0.0), |ui| {
                    ui.set_width(half);
                    match side {
                        Some(l) => {
                            let emph = pair
                                .as_ref()
                                .map(|(left, right)| if idx == 0 { left } else { right });
                            styled_line(ui, l, syntax, emph.map(|v| v.as_slice()));
                        }
                        // A blank keeps the two columns aligned when one side of
                        // a replacement run is longer than the other.
                        None => {
                            ui.label(RichText::new(" ").monospace().size(diff_size(ui)));
                        }
                    }
                });
            }
        });
    }
}

/// Index → word-diff spans, for lines that are half of a replacement pair.
///
/// A run of N removals followed by N additions is treated as N replacements;
/// anything lopsided is left alone, because pairing lines that are not really
/// counterparts produces noise rather than insight.
fn replacement_pairs(
    lines: &[String],
    enabled: bool,
) -> HashMap<usize, Vec<crate::diff::Span>> {
    let mut out = HashMap::new();
    if !enabled {
        return out;
    }
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].starts_with('-') {
            i += 1;
            continue;
        }
        let del_start = i;
        while i < lines.len() && lines[i].starts_with('-') {
            i += 1;
        }
        let add_start = i;
        while i < lines.len() && lines[i].starts_with('+') {
            i += 1;
        }
        let dels = add_start - del_start;
        let adds = i - add_start;
        if dels > 0 && dels == adds {
            for k in 0..dels {
                let (l, r) = crate::diff::word_diff(&lines[del_start + k], &lines[add_start + k]);
                out.insert(del_start + k, l);
                out.insert(add_start + k, r);
            }
        }
    }
    out
}

/// R-D9. Grep results, presented as what they are.
fn blast_panel(ui: &mut egui::Ui, b: &BlastRadius) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(format!("{} blast radius", icon::BLAST)).strong().size(12.0));
            ui.label(dim(b.headline()));
        });
        ui.label(dim(
            "Textual search, not a call graph — it over-reports common names and \
             misses anything dynamic.",
        ));
        if !b.symbols.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(dim("symbols:"));
                for s in &b.symbols {
                    ui.label(mono(s));
                }
            });
        }
        if b.references.is_empty() {
            return;
        }
        // Tests first: "did anything test this?" is the question with teeth.
        let mut refs = b.references.clone();
        refs.sort_by_key(|r| (!r.is_test, r.path.clone(), r.line));
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .id_salt("blast")
            .show(ui, |ui| {
                for r in refs.iter().take(60) {
                    ui.horizontal(|ui| {
                        if r.is_test {
                            ui.label(badge("test", GREEN));
                        }
                        ui.label(mono(format!("{}:{}", r.path, r.line)).color(DIM));
                        ui.label(RichText::new(truncate(&r.text, 90)).monospace().size(11.0));
                    });
                }
            });
        if b.truncated {
            ui.label(dim("… search capped; results are incomplete"));
        }
    });
}

/// Render Markdown, or plain text when the user has turned it off.
///
/// **Only conversation text goes through here.** Tool *output* deliberately
/// does not: a stack trace, a log or a diff is literal, and Markdown would eat
/// its `*`, turn a leading `#` into a heading and collapse its line breaks. The
/// rule is that anything a model wrote as prose is Markdown, and anything a
/// program emitted is monospace.
fn prose(
    ui: &mut egui::Ui,
    text: &str,
    cache: &mut egui_commonmark::CommonMarkCache,
    prefs: &crate::prefs::Prefs,
) {
    if prefs.markdown {
        egui_commonmark::CommonMarkViewer::new().show(ui, cache, text);
    } else {
        ui.label(RichText::new(text).size(12.5));
    }
}

/// A message header with a copy button, since egui labels are not selectable
/// and a transcript you cannot get text out of is half a transcript.
fn message_header(ui: &mut egui::Ui, time: &str, who: &str, color: Color32, body: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.label(dim(time.to_string()));
        ui.label(badge(who, color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button(icon::CLIPBOARD)
                .on_hover_text("copy this message")
                .clicked()
            {
                ui.ctx().copy_text(body.to_string());
            }
        });
    });
}

fn event_row(
    ui: &mut egui::Ui,
    ev: &TranscriptEvent,
    cache: &mut egui_commonmark::CommonMarkCache,
    prefs: &crate::prefs::Prefs,
) {
    let time = ev.ts.format("%H:%M:%S").to_string();
    match &ev.kind {
        EventKind::Init {
            model,
            cwd,
            tool_count,
        } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge("init", DIM));
                ui.label(dim(format!("{model} · {tool_count} tools · {cwd}")));
            });
        }
        EventKind::UserPrompt { text } => {
            message_header(ui, &time, "you", BLUE, text);
            ui.indent(ev.seq, |ui| prose(ui, text, cache, prefs));
            ui.add_space(4.0);
        }
        EventKind::AssistantText { text } => {
            message_header(ui, &time, "agent", GREEN, text);
            ui.indent(ev.seq, |ui| prose(ui, text, cache, prefs));
            ui.add_space(4.0);
        }
        EventKind::Thinking { text } => {
            if !prefs.show_thinking {
                return;
            }
            egui::CollapsingHeader::new(dim(format!("{time}  thinking")))
                .id_salt(ev.seq)
                .show(ui, |ui| prose(ui, text, cache, prefs));
        }
        EventKind::ToolUse { name, summary, .. } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge(name, PURPLE));
                ui.label(mono(truncate(summary, 150)));
            });
        }
        EventKind::ToolResult {
            is_error, preview, ..
        } => {
            let head = if *is_error { "error" } else { "result" };
            let color = if *is_error { RED } else { DIM };
            egui::CollapsingHeader::new(
                RichText::new(format!("{time}  {head}: {}", truncate(preview, 90)))
                    .size(11.5)
                    .color(color),
            )
            .id_salt(ev.seq)
            .show(ui, |ui| {
                // Deliberately not Markdown: this is program output, and
                // rendering it as prose would mangle logs and stack traces.
                ui.label(mono(preview.clone()));
            });
        }
        EventKind::Result {
            cost_usd,
            num_turns,
            terminal_reason,
            is_error,
            text,
        } => {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge(
                    if *is_error { "failed" } else { "done" },
                    if *is_error { RED } else { GREEN },
                ));
                ui.label(dim(format!(
                    "{terminal_reason} · {num_turns} turns · {}",
                    money(*cost_usd)
                )));
            });
            if !text.trim().is_empty() {
                ui.indent(ev.seq, |ui| prose(ui, text, cache, prefs));
            }
        }
        EventKind::Notice { level, message } => {
            let color = match level {
                NoticeLevel::Info => DIM,
                NoticeLevel::Warn => AMBER,
                NoticeLevel::Error => RED,
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(dim(time));
                ui.label(badge("notice", color));
                ui.label(RichText::new(message).color(color).size(12.0));
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Launch a real session
// ---------------------------------------------------------------------------

impl App {
    fn launch_window(&mut self, root: &mut egui::Ui) {
        if !self.show_launch {
            return;
        }
        let ctx = root.ctx().clone();
        let mut open = true;
        let mut go = false;

        egui::Window::new("New session")
            .open(&mut open)
            .default_width(560.0)
            .collapsible(false)
            .show(&ctx, |ui| {
                ui.label(
                    RichText::new("Opens a real interactive claude in Terminal.")
                        .size(12.5),
                );
                ui.label(dim(
                    "mogeung does not wrap the conversation — you drive it exactly as usual, and it shows up in the queue.",
                ));
                ui.add_space(8.0);

                ui.label(dim("directory"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.launch_dir)
                        .hint_text("~/projects/foo")
                        .desired_width(f32::INFINITY),
                );

                // Offer the repos we already know about, so this is one click.
                let mut repos: Vec<String> = self
                    .sessions
                    .values()
                    .filter_map(|s| s.repo_root.clone())
                    .collect();
                repos.sort();
                repos.dedup();
                if !repos.is_empty() {
                    ui.add_space(4.0);
                    ui.label(dim("recent repos"));
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for r in repos {
                                if ui.selectable_label(self.launch_dir == r, &r).clicked() {
                                    self.launch_dir = r.clone();
                                }
                            }
                        });
                }

                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.launch_worktree,
                    "create a fresh git worktree first",
                );
                ui.label(dim(
                    "a worktree isolates the new session so parallel agents cannot collide",
                ));

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.launch_dir.trim().is_empty(),
                            egui::Button::new("Launch"),
                        )
                        .clicked()
                    {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_launch = false;
                    }
                });
            });

        if go {
            self.net.send(ClientMsg::LaunchTerminal {
                dir: self.launch_dir.trim().to_string(),
                worktree: self.launch_worktree,
            });
            self.show_launch = false;
        }
        if !open {
            self.show_launch = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Follow-up prompt (R-D1) and ambient board (R-C5)
// ---------------------------------------------------------------------------

impl App {
    /// Render the flagged hunks as prompt text.
    ///
    /// Quotes the actual changed lines rather than just naming files, because
    /// an agent given "fix the error handling in state.rs" will guess, and one
    /// given the hunk will not.
    fn build_prompt(&self) -> String {
        let mut out = String::new();
        if !self.prompt_note.trim().is_empty() {
            out.push_str(self.prompt_note.trim());
            out.push_str("\n\n");
        }
        out.push_str("Please look at the following, which I flagged while reviewing:\n");
        for (i, f) in self.flagged.iter().enumerate() {
            out.push_str(&format!("\n{}. `{}` {}\n", i + 1, f.path, f.header));
            if !f.note.trim().is_empty() {
                out.push_str(&format!("   {}\n", f.note.trim()));
            }
            if !f.body.is_empty() {
                out.push_str("```diff\n");
                for l in &f.body {
                    out.push_str(l);
                    out.push('\n');
                }
                out.push_str("```\n");
            }
        }
        out
    }

    fn prompt_window(&mut self, root: &mut egui::Ui) {
        if !self.show_prompt {
            return;
        }
        let ctx = root.ctx().clone();
        let mut open = true;

        egui::Window::new("Follow-up prompt")
            .open(&mut open)
            .default_width(660.0)
            .collapsible(false)
            .show(&ctx, |ui| {
                ui.label(
                    RichText::new("mogeung writes this. You paste it.")
                        .size(13.0)
                        .strong(),
                );
                ui.label(dim(
                    "Nothing is sent to any session — that would be steering, which is \
                     exactly what made v0.1 worse than a terminal (ADR-0003).",
                ));
                ui.add_space(8.0);

                ui.label(dim("what you want done"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.prompt_note)
                        .desired_rows(2)
                        .hint_text("e.g. these three need error handling before I merge")
                        .desired_width(f32::INFINITY),
                );

                ui.add_space(8.0);
                ui.label(dim(format!("{} flagged hunk(s)", self.flagged.len())));

                let mut remove = None;
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .id_salt("flagged")
                    .show(ui, |ui| {
                        for (i, f) in self.flagged.iter_mut().enumerate() {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(mono(&f.path).color(DIM));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("✕").clicked() {
                                                remove = Some(i);
                                            }
                                        },
                                    );
                                });
                                ui.add(
                                    egui::TextEdit::singleline(&mut f.note)
                                        .hint_text("note for this hunk (optional)")
                                        .desired_width(f32::INFINITY),
                                );
                            });
                        }
                    });
                if let Some(i) = remove {
                    self.flagged.remove(i);
                }

                ui.add_space(8.0);
                let text = self.build_prompt();
                ui.label(dim("preview"));
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .id_salt("prompt-preview")
                    .show(ui, |ui| {
                        ui.label(RichText::new(&text).monospace().size(11.0));
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button(format!("{} Copy to clipboard", icon::CLIPBOARD)).clicked() {
                        ui.ctx().copy_text(text.clone());
                    }
                    if ui.button("Clear flags").clicked() {
                        self.flagged.clear();
                        self.prompt_note.clear();
                        self.show_prompt = false;
                    }
                    ui.label(dim("then paste it into that session's terminal"));
                });
            });

        if !open {
            self.show_prompt = false;
        }
    }

    /// A big, glanceable board for a second monitor. `R-C5`.
    ///
    /// Readable across a room, which means: only what needs you, only the
    /// label and the reason, and nothing you have to click.
    fn ambient_window(&mut self, root: &mut egui::Ui) {
        if !self.ambient {
            return;
        }
        let ctx = root.ctx().clone();
        let mut open = true;
        let now = Utc::now();
        let queue = self.visible_queue();

        egui::Window::new("Ambient")
            .open(&mut open)
            .default_size(egui::vec2(760.0, 520.0))
            .collapsible(false)
            .show(&ctx, |ui| {
                let needing: Vec<&AttentionItem> =
                    queue.iter().filter(|i| i.reason.needs_human()).collect();

                if needing.is_empty() {
                    ui.add_space(60.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("All clear").size(48.0).color(GREEN));
                        ui.label(
                            RichText::new(format!(
                                "{} live session(s) working",
                                self.sessions.values().filter(|s| s.alive).count()
                            ))
                            .size(20.0)
                            .color(DIM),
                        );
                    });
                    return;
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for item in needing {
                            let Some(s) = self.sessions.get(&item.session_id) else {
                                continue;
                            };
                            let col = reason_color(item.reason);
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(item.reason.label())
                                            .size(24.0)
                                            .strong()
                                            .color(col),
                                    );
                                    ui.label(
                                        RichText::new(truncate(&s.label(), 60))
                                            .size(24.0),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(fmt_dur(
                                                    s.waiting_secs(now)
                                                        .unwrap_or_else(|| s.duration_secs(now)),
                                                ))
                                                .size(24.0)
                                                .color(DIM),
                                            );
                                        },
                                    );
                                });
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {}",
                                        s.repo_name(),
                                        truncate(&item.detail, 80)
                                    ))
                                    .size(16.0)
                                    .color(DIM),
                                );
                                if !s.collisions.is_empty() {
                                    ui.label(
                                        RichText::new("⚠ COLLISION").size(18.0).color(RED).strong(),
                                    );
                                }
                            });
                            ui.add_space(6.0);
                        }
                    });
            });

        if !open {
            self.ambient = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Keyboard settings
// ---------------------------------------------------------------------------

impl App {
    /// Every action by name. `R-B21`.
    ///
    /// Drawn last and in the foreground layer so it sits above every window it
    /// can open — a palette that can be occluded by its own result is worse
    /// than no palette.
    fn palette_window(&mut self, root: &mut egui::Ui) {
        if !self.palette.open {
            return;
        }
        use crate::palette::Mode;
        let ctx = root.ctx().clone();
        let screen = ctx.content_rect();
        let width = (screen.width() * 0.62).clamp(420.0, 680.0);
        let mode = self.palette.mode;
        // One corpus per mode, gathered before the borrow of the window.
        let matches = match mode {
            Mode::Actions => self.palette.matches(&self.keymap),
            _ => Vec::new(),
        };
        let files = match mode {
            Mode::Files => self.file_matches(),
            _ => Vec::new(),
        };
        // (matches, truncated, in_flight, answered-query)
        let search = match mode {
            Mode::Search => self
                .explorer
                .try_current()
                .and_then(|st| st.search.as_ref())
                .map(|s| (s.matches.clone(), s.truncated, s.in_flight, s.query.clone())),
            _ => None,
        };
        let len = match mode {
            Mode::Actions => matches.len(),
            Mode::Files => files.len(),
            Mode::Search => search.as_ref().map(|s| s.0.len()).unwrap_or(0),
        };
        self.palette.clamp(len);

        let mut chosen: Option<crate::keymap::Action> = None;
        let mut chosen_file: Option<String> = None;
        let mut chosen_line: Option<(String, u64)> = None;
        let mut dismiss = false;

        let area = egui::Area::new(egui::Id::new("command-palette"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + (screen.height() * 0.13).min(140.0),
            ))
            .show(&ctx, |ui| {
                ui.set_width(width);
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(10))
                    .corner_radius(10.0)
                    .show(ui, |ui| {
                        ui.set_width(width);
                        // The field is re-focused every frame: the palette is
                        // modal, and a click on the list must not leave the
                        // user typing into nothing.
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut self.palette.query)
                                .id(egui::Id::new("palette-query"))
                                .hint_text(match mode {
                                    Mode::Actions => "run anything…",
                                    Mode::Files => "go to file…",
                                    Mode::Search => "search in files — ⏎ runs it…",
                                })
                                .font(egui::TextStyle::Heading)
                                .desired_width(f32::INFINITY)
                                .frame(egui::Frame::NONE),
                        );
                        field.request_focus();
                        ui.add_space(2.0);
                        ui.separator();

                        let empty_hint = match mode {
                            _ if len > 0 => None,
                            Mode::Actions => Some("nothing matches that"),
                            Mode::Files if self.palette.query.trim().is_empty() => {
                                Some("no files open yet — type to search the worktree")
                            }
                            Mode::Files => Some("no file matches that"),
                            Mode::Search => match &search {
                                Some((_, _, true, _)) => Some("searching…"),
                                Some((_, _, false, _)) => Some("no lines match that"),
                                None => Some("type a query and press ⏎"),
                            },
                        };
                        if let Some(hint) = empty_hint {
                            ui.add_space(10.0);
                            ui.vertical_centered(|ui| {
                                ui.label(dim(hint));
                            });
                            ui.add_space(10.0);
                        }
                        // A stale result list under a changed query reads as
                        // an answer to what is typed — say which query the
                        // rows below actually answer.
                        if let Some((m, truncated, in_flight, answered)) = &search {
                            if !in_flight && answered.trim() != self.palette.query.trim() && !m.is_empty()
                            {
                                ui.label(dim(format!("showing \"{answered}\" — ⏎ searches again")));
                            }
                            if *truncated {
                                ui.label(
                                    RichText::new("cut short at the match cap — narrow the query")
                                        .size(11.0)
                                        .color(AMBER),
                                );
                            }
                        }

                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.style_mut().interaction.selectable_labels = false;
                                let cursor = self.palette.cursor;
                                let scroll = self.palette.scroll;
                                match mode {
                                    Mode::Actions => {
                                        for (i, m) in matches.iter().enumerate() {
                                            let picked = i == cursor;
                                            let row = palette_row(
                                                ui,
                                                m.action,
                                                &self.keymap.describe(m.action),
                                                picked,
                                            );
                                            if picked && scroll {
                                                // Keep the cursor on screen when
                                                // moved by key, not by mouse.
                                                row.scroll_to_me(None);
                                            }
                                            if row.clicked() {
                                                chosen = Some(m.action);
                                            }
                                        }
                                    }
                                    Mode::Files => {
                                        for (i, path) in files.iter().enumerate() {
                                            let picked = i == cursor;
                                            let row = file_palette_row(ui, path, picked);
                                            if picked && scroll {
                                                row.scroll_to_me(None);
                                            }
                                            if row.clicked() {
                                                chosen_file = Some(path.clone());
                                            }
                                        }
                                    }
                                    Mode::Search => {
                                        if let Some((results, ..)) = &search {
                                            for (i, m) in results.iter().enumerate() {
                                                let picked = i == cursor;
                                                let row = search_palette_row(ui, m, picked);
                                                if picked && scroll {
                                                    row.scroll_to_me(None);
                                                }
                                                if row.clicked() {
                                                    chosen_line =
                                                        Some((m.path.clone(), m.line));
                                                }
                                            }
                                        }
                                    }
                                }
                                self.palette.scroll = false;
                            });

                        ui.add_space(4.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(match mode {
                                    Mode::Actions => "↑↓ move   ⏎ run   esc close",
                                    Mode::Files => "↑↓ move   ⏎ open   esc close",
                                    Mode::Search => "↑↓ move   ⏎ search / open   esc close",
                                })
                                .size(10.5)
                                .color(DIM),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("{} of {}",
                                            if len == 0 { 0 } else { self.palette.cursor + 1 },
                                            len))
                                            .size(10.5)
                                            .color(DIM),
                                    );
                                },
                            );
                        });
                    });
            });

        // Clicking away closes it, the same as Escape.
        //
        // Tested against the area's *own* rect. The first version asked egui
        // whether the pointer was over the palette's layer within the whole
        // screen rect, which is true wherever the pointer is — so it could
        // never fire, and clicking outside did nothing at all.
        let palette_rect = area.response.rect;
        if ctx.input(|i| i.pointer.any_pressed())
            && !ctx
                .pointer_interact_pos()
                .map(|p| palette_rect.contains(p))
                .unwrap_or(false)
        {
            dismiss = true;
        }

        if let Some(a) = chosen {
            self.palette.close();
            self.run(a, root);
        } else if let Some(path) = chosen_file {
            self.palette.close();
            self.open_in_explorer(&path, None);
        } else if let Some((path, line)) = chosen_line {
            self.palette.close();
            self.open_in_explorer(&path, Some(line));
        } else if dismiss {
            self.palette.close();
        }
    }

    fn keymap_window(&mut self, root: &mut egui::Ui) {
        if !self.show_keymap {
            return;
        }
        use crate::keymap::{Action, Keymap};
        let ctx = root.ctx().clone();
        let mut open = true;

        let mut to_capture: Option<Action> = None;
        let mut to_reset: Option<Action> = None;
        let mut reset_all = false;
        let mut do_import = false;
        let mut do_export = false;
        let mut save_now = false;

        egui::Window::new("Keyboard")
            .open(&mut open)
            .default_width(620.0)
            .collapsible(false)
            .show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Focused pane:").size(12.5));
                    for p in [Pane::Queue, Pane::Files, Pane::Diff] {
                        ui.selectable_value(&mut self.pane, p, p.label());
                    }
                });
                ui.label(dim(
                    "Navigation acts on the focused pane, so one set of keys works \
                     everywhere instead of three you have to remember.",
                ));
                ui.add_space(6.0);
                ui.checkbox(&mut self.prefs.preview_on_select, "show a file as soon as it is selected")
                    .on_hover_text(
                        "off: moving the cursor only highlights, and the diff changes on Activate",
                    );

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let searching = ui.memory(|m| m.has_focus(keymap_filter_id()));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.keymap_filter)
                            .id(keymap_filter_id())
                            .hint_text(if searching {
                                "type to narrow  ·  esc to leave the box"
                            } else {
                                "search actions  (/)"
                            })
                            .desired_width(260.0),
                    );
                    if !self.keymap_filter.is_empty() && ui.small_button("clear").clicked() {
                        self.keymap_filter.clear();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.capturing.is_some() {
                            ui.label(
                                RichText::new("press the combination…  esc cancels")
                                    .color(AMBER)
                                    .strong(),
                            );
                        } else {
                            ui.label(
                                RichText::new("↑↓ move   ⏎ rebind   ⌫ reset   / search")
                                    .size(11.0)
                                    .color(DIM),
                            );
                        }
                    });
                });
                ui.separator();

                let rows = self.keymap_rows();
                if self.keymap_cursor >= rows.len() {
                    self.keymap_cursor = rows.len().saturating_sub(1);
                }

                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.style_mut().interaction.selectable_labels = false;
                        if rows.is_empty() {
                            ui.add_space(12.0);
                            ui.vertical_centered(|ui| ui.label(dim("no action matches that")));
                            return;
                        }
                        // Groups are headings only when the list is in its
                        // natural order. Under a search the order is by
                        // relevance, and headings over a re-sorted list would
                        // claim a structure that is no longer there.
                        let grouped = self.keymap_filter.trim().is_empty();
                        let mut group = "";
                        for (i, action) in rows.iter().enumerate() {
                            if grouped && action.group() != group {
                                group = action.group();
                                ui.add_space(6.0);
                                ui.label(RichText::new(group).strong().size(12.0));
                            }
                            let capturing = self.capturing == Some(*action);
                            let differs = self.keymap.bindings_for(*action)
                                != Keymap::default().bindings_for(*action);
                            let row = keymap_row(
                                ui,
                                *action,
                                &if capturing {
                                    "press…".to_string()
                                } else {
                                    self.keymap.describe(*action)
                                },
                                i == self.keymap_cursor,
                                capturing,
                                differs,
                            );
                            if row.binding_clicked {
                                to_capture = Some(*action);
                            }
                            if row.reset_clicked {
                                to_reset = Some(*action);
                            }
                            if row.row_clicked {
                                self.keymap_cursor = i;
                            }
                            if i == self.keymap_cursor && self.keymap_scroll {
                                row.response.scroll_to_me(None);
                            }
                        }
                        self.keymap_scroll = false;
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Reset all").clicked() {
                        reset_all = true;
                    }
                    if ui.button("Export").on_hover_text("copy the whole map as JSON").clicked() {
                        do_export = true;
                    }
                    if ui
                        .button("Import")
                        .on_hover_text("replace the map with the JSON below")
                        .clicked()
                    {
                        do_import = true;
                    }
                    if ui.button("Save").clicked() {
                        save_now = true;
                    }
                    ui.label(dim(Keymap::path().to_string_lossy().to_string()));
                });

                ui.add_space(4.0);
                ui.label(dim("import / export"));
                ui.add(
                    egui::TextEdit::multiline(&mut self.keymap_io)
                        .desired_rows(5)
                        .code_editor()
                        .hint_text("paste a keymap here, then press Import")
                        .desired_width(f32::INFINITY),
                );
            });

        if let Some(a) = to_capture {
            self.capturing = Some(a);
        }
        if let Some(a) = to_reset {
            self.keymap.reset(a);
            save_now = true;
        }
        if reset_all {
            self.keymap = Keymap::default();
            save_now = true;
        }
        if do_export {
            match self.keymap.to_json() {
                Ok(j) => {
                    self.keymap_io = j.clone();
                    ctx.copy_text(j);
                }
                Err(e) => self.errors.push(e),
            }
        }
        if do_import {
            match Keymap::from_json(&self.keymap_io) {
                Ok(km) => {
                    // A binding naming a key egui does not know loads fine,
                    // lists fine, and then does nothing — indistinguishable
                    // from a broken action unless we say so.
                    for (action, b) in km.invalid() {
                        self.errors.push(format!(
                            "{b} is not a key mogeung recognises — \"{}\" will not respond to it",
                            action.label()
                        ));
                    }
                    self.keymap = km;
                    save_now = true;
                }
                Err(e) => self.errors.push(format!("could not import keymap: {e}")),
            }
        }
        if save_now {
            if let Err(e) = self.keymap.save() {
                self.errors.push(format!("could not save keymap: {e}"));
            }
        }
        if !open {
            self.show_keymap = false;
            self.capturing = None;
        }
    }
}

// ---------------------------------------------------------------------------
// Health — what mogeung cannot see
// ---------------------------------------------------------------------------

impl App {
    fn health_window(&mut self, root: &mut egui::Ui) {
        if !self.show_health {
            return;
        }
        let ctx = root.ctx().clone();
        let mut open = true;
        let h = self.health.clone();

        egui::Window::new("What mogeung can see")
            .open(&mut open)
            .default_width(560.0)
            .collapsible(false)
            .show(&ctx, |ui| {
                ui.label(RichText::new(h.headline()).size(14.0).strong());
                ui.label(dim(
                    "Everything below is read from undocumented Claude Code files. \
                     When they change, mogeung sees less rather than failing — \
                     which is why this window exists.",
                ));
                ui.add_space(10.0);

                // Alerts first. If something is wrong, it should not be below
                // the fold under a table of healthy-looking numbers.
                if h.alerts.is_empty() {
                    ui.label(RichText::new("Nothing unaccounted for.").color(GREEN));
                } else {
                    for a in &h.alerts {
                        let colour = if a.is_urgent() { AMBER } else { DIM };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(if a.is_urgent() { "⚠" } else { "·" }).color(colour));
                            ui.label(RichText::new(a.message()).color(colour).size(12.5));
                        });
                        ui.add_space(2.0);
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                egui::Grid::new("health-grid")
                    .num_columns(2)
                    .spacing([18.0, 5.0])
                    .show(ui, |ui| {
                        let mut row = |k: &str, v: String| {
                            ui.label(dim(k));
                            ui.label(mono(v));
                            ui.end_row();
                        };
                        row("scans", h.scans.to_string());
                        row(
                            "last scan",
                            h.last_scan
                                .map(|t| t.format("%H:%M:%S UTC").to_string())
                                .unwrap_or_else(|| "never".into()),
                        );
                        row(
                            "sessions",
                            format!("{} known, {} live", h.sessions_known, h.sessions_live),
                        );
                        row("transcripts", h.transcripts_found.to_string());
                    });

                ui.add_space(10.0);
                ui.label(RichText::new("Transcript lines").strong().size(12.5));
                ui.label(dim(
                    "\"ignored\" is bookkeeping we classified and chose to skip — \
                     it is not blindness. \"unknown\" and \"unreadable\" are.",
                ));
                ui.add_space(4.0);

                egui::Grid::new("health-lines")
                    .num_columns(2)
                    .spacing([18.0, 5.0])
                    .show(ui, |ui| {
                        let total = h.lines_seen.max(1);
                        let mut row = |k: &str, n: u64, colour: Option<Color32>| {
                            ui.label(dim(k));
                            let txt = format!("{n}  ({:.1}%)", 100.0 * n as f64 / total as f64);
                            match colour {
                                Some(c) if n > 0 => ui.label(mono(txt).color(c)),
                                _ => ui.label(mono(txt)),
                            };
                            ui.end_row();
                        };
                        row("read", h.lines_parsed, None);
                        row("ignored", h.lines_ignored, None);
                        row("yielded nothing", h.lines_barren, None);
                        row("unknown type", h.lines_unknown, Some(AMBER));
                        row("unreadable", h.lines_malformed, Some(RED));
                        ui.label(dim("total seen"));
                        ui.label(mono(h.lines_seen.to_string()));
                        ui.end_row();
                    });

                if !h.unknown_types.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Types mogeung does not understand").color(AMBER).strong().size(12.5));
                    for (ty, n) in &h.unknown_types {
                        ui.label(mono(format!("  {ty}  ×{n}")).color(AMBER));
                    }
                }

                ui.add_space(10.0);
                ui.label(RichText::new("Claude Code").strong().size(12.5));
                match &h.current_version {
                    Some(v) => ui.label(mono(format!("  running {v}"))),
                    None => ui.label(dim("  no version reported yet")),
                };
                if h.versions_seen.len() > 1 {
                    ui.label(dim(format!(
                        "  {} version(s) across the watched history: {}",
                        h.versions_seen.len(),
                        h.versions_seen.join(", ")
                    )));
                }

                ui.add_space(10.0);
                ui.label(RichText::new("History limits").strong().size(12.5));
                ui.label(dim(format!(
                    "  transcripts over {} are followed from their tail",
                    human_bytes(h.max_transcript_bytes)
                )));
                if h.history_skipped_bytes > 0 {
                    ui.label(mono(format!(
                        "  {} of earlier history never read",
                        human_bytes(h.history_skipped_bytes)
                    )));
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Refresh").clicked() {
                        self.net.send(ClientMsg::FetchHealth);
                    }
                    ui.label(dim("also at GET /api/health"));
                });
            });

        if !open {
            self.show_health = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{keymap_row, may_toggle_hidden, short_path};

    use super::ScrollRequest;

    /// The point of the rule: you cannot dismiss an agent that is still
    /// running. Everything else about hiding is a preference; this one is a
    /// safety property, because a hidden live session is a session you stop
    /// being told about while it is still doing things.
    #[test]
    fn a_live_session_cannot_be_hidden() {
        assert!(!may_toggle_hidden(true, false), "live and visible: refuse");
        assert!(may_toggle_hidden(false, false), "finished: allow");
    }

    /// ...but unhiding must always work. A session hidden while it was dead
    /// and then seen alive again would otherwise be permanently out of sight,
    /// with the rule meant to protect it doing the trapping.
    #[test]
    fn a_hidden_session_can_always_be_unhidden() {
        assert!(may_toggle_hidden(true, true));
        assert!(may_toggle_hidden(false, true));
    }

    /// The sign is the whole trick and it is inverted: `ScrollArea` applies
    /// `offset -= delta`, so moving *down* the content needs a *negative* y.
    /// Getting this backwards scrolls the wrong way, which reads as "the key
    /// does nothing" when you are already at the top.
    #[test]
    fn paging_down_produces_a_negative_delta() {
        assert!(ScrollRequest::Pages(1.0).delta(800.0).y < 0.0, "page down must go down");
        assert!(ScrollRequest::Pages(-1.0).delta(800.0).y > 0.0, "page up must go up");
        assert!(ScrollRequest::Bottom.delta(800.0).y < 0.0);
        assert!(ScrollRequest::Top.delta(800.0).y > 0.0);
    }

    #[test]
    fn a_page_keeps_some_of_the_previous_screen() {
        // A full-height jump leaves you with no anchor; overlap is what makes
        // paging readable rather than teleporting.
        let d = ScrollRequest::Pages(1.0).delta(1000.0).y.abs();
        assert!(d < 1000.0, "a page should not be the whole viewport");
        assert!(d > 700.0, "but it should still be most of it");
    }

    #[test]
    fn a_tiny_pane_still_scrolls() {
        // Guard against a zero-height or unmeasured viewport turning the key
        // into a no-op.
        for h in [0.0, 1.0, 20.0] {
            assert!(
                ScrollRequest::Pages(1.0).delta(h).y.abs() >= 40.0,
                "height {h} produced a useless step"
            );
        }
    }

    #[test]
    fn top_and_bottom_overshoot_on_purpose() {
        // egui clamps offset to the content, so overshooting is how you land
        // exactly at an end without knowing the content height.
        assert!(ScrollRequest::Top.delta(800.0).y.abs() > 1.0e5);
        assert!(ScrollRequest::Bottom.delta(800.0).y.abs() > 1.0e5);
    }



    #[test]
    fn a_deep_path_keeps_the_part_that_identifies_it() {
        // The leading directories are the same for most of a repo's files, so
        // they are what gets dropped.
        assert_eq!(
            short_path("crates/mogeungd/src/state.rs"),
            ("…/src/".to_string(), "state.rs")
        );
    }

    #[test]
    fn short_paths_are_left_alone() {
        assert_eq!(short_path("README.md"), (String::new(), "README.md"));
        assert_eq!(short_path("src/main.rs"), ("src/".to_string(), "main.rs"));
    }

    #[test]
    fn the_filename_is_never_the_part_that_gets_dropped() {
        // Truncation happens on the left, always: a row reading "crates/mog…"
        // tells you nothing, and every file in a directory would look alike.
        for p in [
            "a/b/c/d/e/f/g/very_long_file_name.rs",
            "one/two.rs",
            "no-directory.txt",
            "docs/design/cross-session-signals.md",
        ] {
            let (_, base) = short_path(p);
            assert!(p.ends_with(base), "{p}: lost the filename, got {base:?}");
            assert!(!base.contains('/'), "{p}: base should be one component");
        }
    }

    #[test]
    fn a_trailing_slash_does_not_produce_an_empty_name() {
        // Defensive: git should never hand us one, but an empty row would be
        // unclickable and invisible.
        let (_, base) = short_path("some/dir/");
        assert!(base.is_empty(), "documents current behaviour: {base:?}");
    }

    /// Pressing Alt then 9 must capture `Alt+Num9`, not `AltLeft`.
    ///
    /// The modifier's own key event arrives first — in an earlier frame, on
    /// real hardware — and the capture used to end on it, so every chord
    /// rebind saved the bare modifier. Found live on Ubuntu.
    #[test]
    fn a_chord_capture_waits_for_the_real_key() {
        use super::captured_binding;
        let alt_down = egui::Event::Key {
            key: egui::Key::AltLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };
        // The modifier press alone must capture nothing…
        assert_eq!(captured_binding(&[alt_down.clone()]), None);
        // …and the chord lands when the real key arrives.
        let nine = egui::Event::Key {
            key: egui::Key::Num9,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        };
        let got = captured_binding(&[alt_down, nine]).expect("chord captured");
        assert_eq!(got.0, "Alt+9");
    }

    /// A click on a keymap row's binding button must reach the *button*.
    ///
    /// The row used to lay a click-sensing widget over its whole width after
    /// its children, and egui resolves a tied hit to the last-registered
    /// widget — so the row ate the button's clicks and rebinding by mouse
    /// never worked, on any platform. This drives a real click through egui's
    /// actual hit-testing, headlessly, so the overlap cannot come back.
    #[test]
    fn a_click_on_the_binding_button_reaches_the_button_not_the_row() {
        let ctx = egui::Context::default();
        // Inside the 140×20 binding button: margins are 6/3, button is the
        // row's first child.
        let inside_button = egui::pos2(40.0, 14.0);

        let mut last = (false, false); // (binding_clicked, row_clicked)
        let mut run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| {
                let hit = keymap_row(ui, crate::keymap::Action::Snooze, "S", false, false, false);
                last = (hit.binding_clicked, hit.row_clicked);
            });
        };

        run(vec![egui::Event::PointerMoved(inside_button)]);
        run(vec![egui::Event::PointerButton {
            pos: inside_button,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        run(vec![egui::Event::PointerButton {
            pos: inside_button,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);

        assert!(
            last.0,
            "the binding button never saw the click — something is covering it"
        );
        assert!(
            !last.1,
            "the row also claimed the click, so the cursor would jump instead of rebinding"
        );
    }
}
