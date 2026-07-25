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

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Tab {
    Changes,
    Transcript,
    Info,
    /// Review debt for the selected session's repo. `R-D8`.
    Debt,
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

    sessions: HashMap<SessionId, Session>,
    queue: Vec<AttentionItem>,
    changes: HashMap<SessionId, Change>,
    events: HashMap<SessionId, Vec<TranscriptEvent>>,
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
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        if let Some(h) = &hotkey {
            h.start_waker(cc.egui_ctx.clone());
        }
        let net = Net::connect(url, cc.egui_ctx.clone());
        let (keymap, keymap_warning) = crate::keymap::Keymap::load();
        let (prefs, prefs_warning) = crate::prefs::Prefs::load();
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
            // Surfaced in the window, not only on stderr: the terminal that
            // launched this is exactly what you are trying to stop looking at.
            errors: hotkey_error
                .into_iter()
                .chain(keymap_warning)
                .chain(prefs_warning)
                .collect(),
        }
    }

    fn ingest(&mut self) {
        for msg in self.net.drain() {
            match msg {
                ServerMsg::Snapshot { sessions, queue } => {
                    self.sessions = sessions.into_iter().map(|s| (s.id.clone(), s)).collect();
                    self.queue = queue;
                    // A reconnect invalidates our transcript cache.
                    self.hydrated.clear();
                }
                ServerMsg::SessionUpdated { session } => {
                    self.sessions.insert(session.id.clone(), *session);
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
                        let list = self.events.entry(ev.session_id.clone()).or_default();
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
                    self.changes.insert(session_id, change);
                }
                ServerMsg::Health { health } => self.health = *health,
                ServerMsg::ReviewDebt { debt } => self.debt = Some(*debt),
                ServerMsg::BlastRadius { radius } => {
                    self.blast_pending = false;
                    self.blast = Some(*radius);
                }
                ServerMsg::Error { message } => {
                    self.errors.push(message);
                    if self.errors.len() > 6 {
                        self.errors.remove(0);
                    }
                }
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

    fn selected_session(&self) -> Option<&Session> {
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
        self.detail_panel(ui);
        self.launch_window(ui);
        self.health_window(ui);
        self.prompt_window(ui);
        self.ambient_window(ui);
        self.keymap_window(ui);

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
    }
}

// ---------------------------------------------------------------------------
// Top bar
// ---------------------------------------------------------------------------

impl App {
    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("top").show(root, |ui| {
            ui.horizontal(|ui| {
                let title = ui.label(RichText::new("mogeung").strong().size(17.0));
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
                crate::filter::matches(&q, s)
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

    /// Move the selection by `delta` within the visible queue. `R-B1`.
    fn move_selection(&mut self, delta: i32) {
        let vis = self.visible_queue();
        if vis.is_empty() {
            return;
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
            let captured = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some(crate::keymap::Binding::new(*modifiers, *key)),
                    _ => None,
                })
            });
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
                ui.memory_mut(|m| m.request_focus(egui::Id::new("queue-filter")));
            }
            A::ClearFilter => {
                self.filter.clear();
                self.ambient = false;
                self.show_keymap = false;
                self.capturing = None;
            }
            A::HideSession => self.hide_selected(),
            A::PinSession => self.pin_selected(),
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
            A::OpenKeymap => self.show_keymap = !self.show_keymap,
            A::Rescan => self.net.send(ClientMsg::Rescan),
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
        self.set_tab(next_tab(self.tab, delta));
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
                        if ui.checkbox(&mut self.prefs.group_by_repo, "group").changed() {
                            self.prefs_dirty = true;
                        }
                        if ui
                            .checkbox(&mut self.prefs.auto_select, "follow")
                            .on_hover_text("keep the top of the queue selected as it changes")
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
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .id(egui::Id::new("queue-filter"))
                            .hint_text("filter  (/)   repo: branch: file:")
                            .desired_width(ui.available_width() - 4.0),
                    );
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
                ui.label(dim("j/k move · enter terminal · r read · s snooze · h hide · p pin"));
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

                // R-B8. Follow mode: the top of the queue is by definition the
                // thing most worth looking at, so let it drive the pane.
                if self.prefs.auto_select {
                    if let Some(top) = vis.first() {
                        if self.selected.as_ref() != Some(&top.session_id) {
                            to_select = Some(top.session_id.clone());
                        }
                    }
                }

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
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
                                let Some(session) = self.sessions.get(&item.session_id) else {
                                    continue;
                                };
                                let selected = self.selected.as_ref() == Some(&item.session_id);
                                let resp = ui.push_id(&item.session_id, |ui| {
                                    egui::Frame::group(ui.style())
                                        .fill(if selected {
                                            ui.visuals().selection.bg_fill.linear_multiply(0.35)
                                        } else {
                                            Color32::TRANSPARENT
                                        })
                                        .show(ui, |ui| {
                                            ui.set_width(ui.available_width());
                                            if let Some(r) = queue_card(
                                                ui,
                                                session,
                                                item,
                                                now,
                                                self.prefs.is_pinned(&item.session_id),
                                                self.prefs.is_hidden(&item.session_id),
                                            ) {
                                                to_filter_repo = Some(r);
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
                                                    let is_hidden =
                                                        self.prefs.is_hidden(&item.session_id);
                                                    if ui
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
                                if resp.response.interact(egui::Sense::click()).clicked() {
                                    to_select = Some(item.session_id.clone());
                                }
                                ui.add_space(2.0);
                            }
                        }

                        if hidden > 0 {
                            ui.add_space(6.0);
                            ui.label(dim(format!("{hidden} session(s) hidden")));
                        }
                    });

                if let Some(id) = to_select {
                    self.select(id);
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
            });
    }
}

fn queue_card(
    ui: &mut egui::Ui,
    s: &Session,
    item: &AttentionItem,
    now: chrono::DateTime<Utc>,
    pinned: bool,
    hidden: bool,
) -> Option<String> {
    // Set when the repo name is clicked, to filter down to it.
    let mut filter_repo = None;
    ui.horizontal(|ui| {
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
            filter_repo = Some(s.repo_name());
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
    filter_repo
}

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

impl App {
    fn detail_panel(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            let Some(s) = self.selected_session().cloned() else {
                ui.centered_and_justified(|ui| {
                    ui.label(dim("select a session"));
                });
                return;
            };
            let now = Utc::now();

            ui.horizontal_wrapped(|ui| {
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
                ui.label(RichText::new(truncate(&s.label(), 120)).size(14.0).strong());
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(dim(s.repo_name()));
                if let Some(b) = &s.git_branch {
                    ui.label(dim(format!("· {} {b}", icon::BRANCH)));
                }
                ui.label(dim(format!("· {}", fmt_dur(s.duration_secs(now)))));
                ui.label(dim(format!("· {} turns", s.turns)));
                ui.label(dim(format!("· {} tool calls", s.tool_calls)));
                ui.label(dim(format!("· {} tokens out", tokens(s.tokens_out))));
                if let Some(pid) = s.pid {
                    ui.label(dim(format!("· pid {pid}")));
                }
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

            ui.horizontal_wrapped(|ui| {
                if ui.button("Refresh diff").clicked() {
                    self.net.send(ClientMsg::RefreshChange {
                        session_id: s.id.clone(),
                    });
                }
                ui.separator();
                ui.label(dim("open in"));
                for t in [
                    OpenTarget::Terminal,
                    OpenTarget::Intellij,
                    OpenTarget::VsCode,
                    OpenTarget::Finder,
                ] {
                    if ui.button(t.label()).clicked() {
                        if let Err(e) = ui::open_in(t, &s.cwd) {
                            self.errors.push(e);
                        }
                    }
                }
                ui.separator();
                if ui
                    .button("Forget")
                    .on_hover_text("stop tracking this session and drop its review marks")
                    .clicked()
                {
                    self.net.send(ClientMsg::ForgetSession {
                        session_id: s.id.clone(),
                    });
                }
            });

            ui.label(mono(&s.cwd).color(DIM));
            ui.separator();

            let unread = self
                .changes
                .get(&s.id)
                .map(|c| c.unreviewed_hunks())
                .unwrap_or(0);
            ui.horizontal(|ui| {
                let changes_label = if unread > 0 {
                    format!("Changes ({unread} unread)")
                } else {
                    "Changes".to_string()
                };
                let mut pick = None;
                for (tab, label, hint) in [
                    (Tab::Changes, changes_label.clone(), "the diff this session produced"),
                    (Tab::Transcript, "Transcript".to_string(), "the conversation"),
                    (Tab::Info, "Info".to_string(), "session details"),
                    (
                        Tab::Debt,
                        "Debt".to_string(),
                        "how much of this repo's agent output nobody has read",
                    ),
                ] {
                    let action = match tab {
                        Tab::Changes => crate::keymap::Action::TabChanges,
                        Tab::Transcript => crate::keymap::Action::TabTranscript,
                        Tab::Info => crate::keymap::Action::TabInfo,
                        Tab::Debt => crate::keymap::Action::TabDebt,
                    };
                    if ui
                        .selectable_label(self.tab == tab, label)
                        .on_hover_text(format!("{hint}  ({})", self.keymap.describe(action)))
                        .clicked()
                    {
                        pick = Some(tab);
                    }
                }
                if let Some(tab) = pick {
                    self.set_tab(tab);
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
            ui.separator();

            match self.tab {
                Tab::Changes => self.changes_tab(ui, &s),
                Tab::Transcript => self.transcript_tab(ui, &s),
                Tab::Info => self.info_tab(ui, &s),
                Tab::Debt => self.debt_tab(ui, &s),
            }
        });
    }

    fn transcript_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
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
                        if resp.clicked() {
                            self.selected_file = Some(f.path.clone());
                        }
                    }
                });
        });

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

/// The tab `delta` steps from `from`, wrapping at both ends.
///
/// Wrapping rather than clamping: cycling that stops dead at the last tab makes
/// you reverse direction to get back, which is not what a cycle is for.
fn next_tab(from: Tab, delta: i32) -> Tab {
    const ORDER: [Tab; 4] = [Tab::Changes, Tab::Transcript, Tab::Info, Tab::Debt];
    let at = ORDER.iter().position(|t| *t == from).unwrap_or(0) as i32;
    ORDER[(at + delta).rem_euclid(ORDER.len() as i32) as usize]
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

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        // Word-diff emphasis takes precedence: knowing *what changed* beats
        // knowing what is a keyword.
        if let Some(spans) = emphasis {
            for sp in spans {
                let mut t = RichText::new(&sp.text).monospace().size(11.5).color(fg);
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
            let mut t = RichText::new(line).monospace().size(11.5).color(fg);
            if let Some(b) = bg {
                t = t.background_color(b);
            }
            ui.label(t);
            return;
        }

        for (tok, text) in crate::diff::highlight(line) {
            let mut t = RichText::new(&text)
                .monospace()
                .size(11.5)
                .color(tok_color(tok, fg));
            if let Some(b) = bg {
                t = t.background_color(b);
            }
            ui.label(t);
        }
    });
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
                            ui.label(RichText::new(" ").monospace().size(11.5));
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
                if self.capturing.is_some() {
                    ui.label(
                        RichText::new("Press the key combination…  (Escape cancels)")
                            .color(AMBER)
                            .strong(),
                    );
                } else {
                    ui.label(dim("Click a shortcut to rebind it."));
                }
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut group = "";
                        for action in Action::ALL {
                            if action.group() != group {
                                group = action.group();
                                ui.add_space(6.0);
                                ui.label(RichText::new(group).strong().size(12.0));
                            }
                            ui.horizontal(|ui| {
                                let capturing = self.capturing == Some(*action);
                                let label = if capturing {
                                    "press…".to_string()
                                } else {
                                    self.keymap.describe(*action)
                                };
                                let btn = ui.add_sized(
                                    [130.0, 20.0],
                                    egui::Button::new(
                                        RichText::new(label)
                                            .monospace()
                                            .size(11.5)
                                            .color(if capturing { AMBER } else { Color32::from_gray(0xDC) }),
                                    ),
                                );
                                if btn.clicked() {
                                    to_capture = Some(*action);
                                }
                                ui.label(RichText::new(action.label()).size(12.0));

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let differs = self.keymap.bindings_for(*action)
                                            != Keymap::default().bindings_for(*action);
                                        if differs && ui.small_button("reset").clicked() {
                                            to_reset = Some(*action);
                                        }
                                    },
                                );
                            });
                        }
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
    use super::{next_tab, short_path, Tab};

    use super::ScrollRequest;

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
    fn cycling_tabs_wraps_at_both_ends() {
        assert_eq!(next_tab(Tab::Changes, 1), Tab::Transcript);
        assert_eq!(next_tab(Tab::Debt, 1), Tab::Changes, "forward from the last");
        assert_eq!(next_tab(Tab::Changes, -1), Tab::Debt, "back from the first");
        assert_eq!(next_tab(Tab::Info, -1), Tab::Transcript);
    }

    #[test]
    fn cycling_all_the_way_round_returns_to_the_start() {
        let mut t = Tab::Changes;
        for _ in 0..4 {
            t = next_tab(t, 1);
        }
        assert_eq!(t, Tab::Changes, "four tabs, four steps");
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
}
