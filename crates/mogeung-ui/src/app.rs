use crate::net::Net;
use crate::theme::{on_accent_for, pal};
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
    ///
    /// Displays as "Agent", and the variant follows the display name here
    /// rather than lagging it as `Explorer` does — because the word this pane
    /// gives up is one another pane is going to want. A variant still called
    /// `Terminal`, meaning "the agent's tmux client", sitting next to a tab
    /// labelled Terminal meaning "your shell", is a trap laid for whoever
    /// edits this next.
    ///
    /// The *serialised* name stays `"Terminal"`: it is in every saved
    /// `layout.json`, and a rename would quietly reset the arrangement.
    #[serde(rename = "Terminal")]
    Agent,
    /// **Retired.** The shell was a pane for one day.
    ///
    /// It moved to the bottom panel on 2026-07-30 (`R-B33`), because a shell is
    /// not a view of a session: as a pane it followed the selection, vanished
    /// when the selection moved, and could not exist at all before a session
    /// did — which is exactly when you want a shell to start one.
    ///
    /// The variant stays because `"Shell"` is written into saved layouts and
    /// serde would refuse a file that still holds it — and [`crate::layout`]
    /// degrades an unparseable layout to the default, so removing this would
    /// have cost people the arrangement they built. It is stripped on load and
    /// unreachable thereafter; the arm in `pane_ui` is the belt to that braces.
    #[serde(rename = "Shell")]
    Terminal,
    /// The session's worktree: tree on the left, read-only viewer on the
    /// right. `R-B24`. A viewer and never an editor — the roadmap's
    /// "explicitly not" list is why there is no write path here.
    Explorer,
    /// The session repo's commits, uncommitted changes and diffs. `R-D10`.
    /// Read-only, permanently — the observer rule, one layer down.
    Git,
    /// Cross-session intelligence: search, digest, analytics, prompts,
    /// failures, docs. Pillars F and H, with pillar G's burn tables.
    Insight,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Changes => "Changes",
            Tab::Transcript => "Transcript",
            Tab::Info => "Info",
            Tab::Debt => "Debt",
            Tab::Agent => "Agent",
            Tab::Terminal => "Terminal",
            // Display name only. The variant stays `Explorer` because it is
            // serialized into saved layouts; and the pane stays read-only —
            // "Editor" is what the user calls it, not a write path.
            Tab::Explorer => "Editor",
            Tab::Git => "Git",
            Tab::Insight => "Insight",
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
            Tab::Agent => Action::TabAgent,
            Tab::Terminal => Action::TabTerminal,
            Tab::Explorer => Action::TabExplorer,
            Tab::Git => Action::TabGit,
            Tab::Insight => Action::TabInsight,
        }
    }
}

/// Which of the two terminal panes is being typed into.
///
/// They are the same widget over different ptys, and everything about handing
/// the keyboard over is identical — which is why `ToggleTerminalFocus` is one
/// chord for both rather than one each. This is the "which", and it exists
/// only because there are now two.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PtyPane {
    /// The agent's session, attached through tmux.
    Agent,
    /// The worktree's shell.
    Shell,
}

/// What a chord means while a terminal holds the keyboard.
///
/// Exactly two things reach mogeung in that state. Everything else — including
/// every other shortcut this app has — is typing, and must arrive at the pty
/// untouched: `Alt+1` is a tab shortcut out here and something the program
/// inside may want in there, and mogeung is not the one to decide otherwise.
///
/// Pure, so the rule is testable. The live path needs an egui context and a
/// running shell, and this is the part that is easy to get wrong.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PtyChord {
    /// Give the keyboard back.
    Leave,
    /// Show or hide the terminal panel — the toggle half of `TabTerminal`,
    /// which is useless if the only way to reach it is to leave the shell
    /// first.
    ToggleTerminal,
    /// Not ours. Hands off.
    Pass,
}

fn pty_chord(action: Option<crate::keymap::Action>) -> PtyChord {
    match action {
        Some(crate::keymap::Action::ToggleTerminalFocus) => PtyChord::Leave,
        Some(crate::keymap::Action::TabTerminal) => PtyChord::ToggleTerminal,
        _ => PtyChord::Pass,
    }
}

/// What a click in the terminal panel's header asked for.
///
/// Collected and applied after the header is drawn, because every one of these
/// mutates the list the header is iterating — closing a tab from inside the
/// loop over the tabs is the classic version of that bug.
enum PanelAction {
    None,
    Select(usize),
    OpenIn(String),
    Close(usize),
    Restart(usize),
    Hide,
    /// Start renaming a tab. `R-B34`.
    Rename(usize),
    /// Finish the rename in progress, or abandon it.
    CommitRename,
    CancelRename,
    /// Put the derived name back.
    ClearName(usize),
}

/// Draw a terminal pane's body, and own the whole of "who has the keyboard".
///
/// A free function over the two fields it touches, not a method: the caller is
/// already holding a `&mut` to its own `Term`. Shared because both panes must
/// answer the mouse identically, and because every line here was a bug once —
/// the click that could never be detected, and the pane you could only leave
/// by remembering a chord.
fn pty_body(
    ui: &mut egui::Ui,
    term: &mut crate::term::Term,
    which: PtyPane,
    focus: &mut Option<PtyPane>,
    font: egui::FontId,
) {
    let focused = *focus == Some(which);
    let before = ui.available_size();
    // `.inner`, not `.response`: `allocate_ui`'s own response senses hover
    // only, so `clicked()` on it is always false. The first version tested
    // that one, and the pane could never be typed into.
    let area = ui
        .allocate_ui(before, |ui| term.ui(ui, focused, font))
        .inner;

    // Clicking in takes the keyboard; the chord in the hint gives it back, and
    // so does clicking anything else — a pane you can only escape by
    // remembering a chord is a trap.
    if area.clicked() {
        *focus = Some(which);
    } else if focused && ui.input(|i| i.pointer.any_pressed()) && !area.contains_pointer() {
        *focus = None;
    }

    if focused {
        ui.painter().rect_stroke(
            area.rect,
            2.0,
            egui::Stroke::new(1.0, ui.visuals().selection.bg_fill),
            egui::StrokeKind::Inside,
        );
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
///
/// A function rather than the `const` array this was: a `const` cannot ask
/// which theme is running, and the light palette needs its own lanes (the
/// dark ones are mid-tone and vanish against a white page).
fn graph_colors() -> [egui::Color32; 6] {
    pal().graph
}

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

    /// Token burn and the limit picture, refreshed on a slow cadence. Any
    /// threshold in it is an estimate from observed limit hits, and the UI
    /// says so wherever it shows one. `R-G1`–`R-G3`.
    usage: Option<mogeung_core::usage::UsageReport>,
    usage_asked: Option<chrono::DateTime<Utc>>,

    /// Per-repo signal-runner state, keyed by repo root. `R-E2`.
    signals: HashMap<String, SignalState>,

    /// The Insight pane's caches and pending flags. Pillars F and H.
    insight: InsightState,
    /// Scroll the Transcript pane to the first event at or after this time —
    /// how a search hit (`R-F1`) or a commit (`R-F9`) "opens the transcript
    /// at" a moment. Consumed by the next Transcript render.
    focus_event_ts: Option<chrono::DateTime<Utc>>,
    /// Subagent trees per session, fetched once per session per run. `R-F8`.
    subagents: HashMap<SessionId, Vec<mogeung_core::insight::SubagentNode>>,
    subagents_asked: HashSet<SessionId>,

    /// Editor navigation state — outline panel, go-to-line, occurrence
    /// highlight, markdown preview. `R-B28`/`R-B29`. Transient by design;
    /// only wrap and bookmarks persist (prefs).
    outline_open: bool,
    outline_filter: String,
    show_goto: bool,
    goto_input: String,
    occurrences_word: Option<String>,
    md_preview: HashSet<String>,

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
    /// What `daemon::acquire` decided at launch, kept for the whole run.
    ///
    /// Switching away from LOCAL and back must not turn "this window is hosting
    /// the daemon" into "nothing is serving it" — the thread is still there and
    /// still serving, so saying otherwise would be a lie the status bar tells
    /// about its own process.
    launch_mode: crate::daemon::Mode,
    /// Who the daemon says it is (`R-I5`). `None` until the first snapshot, or
    /// for good against a daemon too old to say.
    daemon_identity: Option<mogeung_core::wire::DaemonIdentity>,
    /// Daemons this window can reach, and which is current. `R-I7`.
    connections: crate::connections::Connections,
    show_connections: bool,
    /// The row being added or edited, and where it will land. `None` for a new
    /// one — kept out of `connections` until saved so an abandoned edit leaves
    /// nothing behind.
    conn_draft: Option<(Option<usize>, crate::connections::Connection)>,
    /// A live mDNS subscription, held while the Daemons window is open
    /// (`R-I8`). Dropping it stops the queries.
    scan: Option<mogeungd::discovery::Watcher>,
    /// Everything seen since the window was opened, newest sighting merged in.
    ///
    /// **Accumulates and never clears.** A host answers piecemeal — per
    /// interface, per address family, over several seconds — so a list rebuilt
    /// from each round of answers flickers, drops rows, and looks like an
    /// unreliable network. This is a wifi picker, not a search result.
    ///
    /// Still offers, not connections: nothing here is dialled until a hand
    /// says so.
    scanned: Vec<mogeungd::discovery::Found>,
    /// When the subscription started, so the panel can say "still looking"
    /// rather than "nothing there" during the seconds before the first answer.
    scan_since: Option<std::time::Instant>,
    /// What the Transcript's find box holds. `R-B36`. Not persisted — a
    /// half-typed query is not a setting.
    transcript_find: String,
    /// Which tool the rail shows when it is opened without being told which.
    /// `R-B40`. Not persisted: it only has to survive a collapse, and a
    /// preference that remembered it would be a second copy of `prefs.rail`.
    last_rail: crate::prefs::RailTool,
    /// The rail's global search. `R-F13`.
    search: SearchPanel,
    /// Whether a snapshot has ever arrived. `R-J7`.
    ///
    /// Not the same as being connected: the socket is up well before the
    /// first scan finishes, and the gap between them is exactly the moment an
    /// empty board is misleading.
    first_snapshot: bool,
    /// Every note the daemon holds. `R-B35`.
    ///
    /// Daemon-owned, so this is a cache of theirs rather than a store of ours
    /// — replaced wholesale on every `Notes` message, never edited in place.
    /// That is what makes two windows agree (ADR-0015).
    notes: Vec<mogeung_core::wire::Note>,
    /// The turn whose note is open for editing, and the text so far.
    /// `(session_id, seq, body)`.
    note_draft: Option<(String, u64, String)>,
    /// This machine's id, resolved once — the other half of the comparison.
    this_machine: Option<String>,

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

    /// Ctrl+F while the Git pane holds the keyboard: a one-frame "grab the
    /// log filter" flag, the git twin of `explorer_find_focus`.
    git_filter_focus: bool,

    /// The attached terminal, when the Agent tab has been opened for a
    /// session that runs under tmux. One at a time: a second attach costs a
    /// pty and a tmux client for a pane nobody is looking at.
    ///
    /// `agent_`, not `term_`, because this is the pty mogeung *borrows*. A pty
    /// mogeung owns would be a different field, under a different rule
    /// ([ADR-0010](../../../docs/decisions/0010-attach-a-terminal-never-own-one.md)).
    agent_term: Option<crate::term::Term>,
    /// A refused attach, remembered against its target so the pane stops
    /// trying. Without it a failure that recurs — the wrong `TERM`, a session
    /// that has ended — is retried every frame, spawning a tmux per frame and
    /// flashing its own error at the refresh rate.
    agent_failed: Option<(String, String)>,

    /// Your own shells, in the bottom panel. `R-B31`, `R-B33`.
    ///
    /// Owned rather than borrowed — the distinction `agent_term`'s note draws
    /// — and allowed to be, because they run under tmux and so are no more
    /// trapped here than the agent is
    /// ([ADR-0011](../../../docs/decisions/0011-own-a-shell-never-an-agent.md)).
    ///
    /// Not keyed to the selected session, and this is the field that says so:
    /// nothing here is indexed by `SessionId`, so there is no path by which
    /// moving the selection could disturb a shell.
    shells: crate::shells::Shells,

    /// The terminal font family currently registered with egui, and whether
    /// one has ever been. `None` forces the first install, which is what
    /// registers the family name the panes ask for at all — see
    /// [`crate::font::definitions`].
    font_installed: Option<Option<String>>,

    /// Which pty pane, if either, is being typed into. See `handle_keys`.
    ///
    /// One field rather than a `bool` per pane, because "both have the
    /// keyboard" is not a state that means anything — with two panes on screen
    /// in a split it would deliver every keystroke twice — and one field makes
    /// it unrepresentable instead of merely unlikely.
    pty_focus: Option<PtyPane>,

    /// Whether the restored window position has been checked against the
    /// monitors that actually exist. One-shot, on the first frame. `R-J1`.
    geometry_checked: bool,

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

/// The rail's global search: one box over three corpora. `R-F13`.
///
/// Not one search. Three, with three different costs, kept apart so the
/// cheapest is never held up by the dearest:
///
/// - **this transcript** is [`crate::search`] over events already in memory,
///   and runs on every keystroke because it is free;
/// - **this session's files** and **every session** are daemon round-trips and
///   wait for Enter, which is the rule `R-F1` shipped with — the corpus is
///   tens of MB and a per-keystroke scan would punish typing.
///
/// The two daemon answers echo their query back, so each group drops a reply
/// to a question that has since been retyped without knowing anything about
/// the other.
struct SearchPanel {
    /// What the box holds. Not what was asked — see `issued`.
    query: String,
    /// The query the daemon groups were actually sent. Kept apart from
    /// `query` so the panel can say "press Enter" while you are still typing,
    /// and so a stale answer can be recognised by comparing against what was
    /// asked rather than against what is now typed.
    issued: String,
    files: Group<mogeung_core::wire::ContentMatch>,
    insight: Group<mogeung_core::insight::SearchHit>,
    /// Search every session, or only the selected one.
    ///
    /// **On by default**, though it is the expensive half. Three groups is
    /// what this panel is for — a cross-session group that arrived switched
    /// off would be a feature you have to discover before it is a feature at
    /// all. The toggle exists to turn the slow one *off* when you already know
    /// the answer is in front of you.
    everywhere: bool,
    sel: Option<SearchSel>,
    /// Every visible row, in the order they are drawn, rebuilt each paint.
    ///
    /// The keyboard handler runs *before* the panel draws, so it steers by
    /// last frame's list. One frame stale is the immediate-mode bargain and
    /// costs nothing here: the list only changes when an answer lands or you
    /// type, and both already redraw.
    rows: Vec<SearchSel>,
    /// The arrows are walking the results rather than the query box.
    ///
    /// This is what makes Enter mean two things safely: **run the search**
    /// while you are typing, **open this row** once you have arrowed into the
    /// list. Down enters the list, Up off the top leaves it.
    in_list: bool,
    /// Bring the selected row into view. Armed by keyboard moves only — an
    /// unconditional scroll pins the list to the selection and puts every row
    /// below the fold out of the mouse's reach.
    scroll_row: bool,
    /// The previewed file's body, as `(path, content)`. One at a time — this
    /// is a preview, not a second editor.
    preview: Option<(String, String)>,
    /// The path a preview was asked for, so one request goes out rather than
    /// one per frame for as long as the answer is in flight.
    preview_asked: Option<String>,
    /// Bring the matched line into view — **once**, when the selection moves.
    ///
    /// Armed by a click and consumed by the paint that has a body to scroll,
    /// which may be several frames later. Scrolling unconditionally instead
    /// would pin the preview to the hit and make every other line of the file
    /// unreachable by mouse: the same trap the palette's cursor documents.
    preview_scroll: bool,
    /// Take the caret this frame, because the key that opened the panel meant
    /// "let me type".
    focus: bool,
}

impl Default for SearchPanel {
    fn default() -> Self {
        SearchPanel {
            query: String::new(),
            issued: String::new(),
            files: Group::default(),
            insight: Group::default(),
            everywhere: true,
            sel: None,
            rows: Vec::new(),
            in_list: false,
            scroll_row: false,
            preview: None,
            preview_asked: None,
            preview_scroll: false,
            focus: false,
        }
    }
}

/// One group's answer, and whether it is still coming.
///
/// `hits: None` is *not yet answered* and `Some(vec![])` is *answered, and
/// there is nothing* — `R-J5`'s distinction, which is the whole reason this is
/// not a bare `Vec`.
struct Group<T> {
    running: bool,
    hits: Option<Vec<T>>,
    truncated: bool,
}

impl<T> Default for Group<T> {
    fn default() -> Self {
        Group {
            running: false,
            hits: None,
            truncated: false,
        }
    }
}

impl<T> Group<T> {
    /// Forget the answer and mark the question asked.
    fn asked(&mut self) {
        self.running = true;
        self.hits = None;
        self.truncated = false;
    }

    fn idle(&mut self) {
        self.running = false;
        self.hits = None;
        self.truncated = false;
    }

    fn count(&self) -> Option<usize> {
        self.hits.as_ref().map(|h| h.len())
    }
}

/// The selected result, and therefore what the preview pane shows.
#[derive(Clone, PartialEq)]
enum SearchSel {
    /// A turn of the selected session's transcript, by `seq`.
    Turn(u64),
    /// A line of a worktree file of the selected session.
    File { path: String, line: u64 },
    /// A line of some session's transcript, from the cross-session corpus.
    ///
    /// Carries its own preview text because that clip is all the daemon
    /// returns for one — unlike the other two, there is nothing further to
    /// fetch without a wire message that does not exist yet.
    Hit {
        session: String,
        line: u64,
        ts: Option<chrono::DateTime<Utc>>,
        preview: String,
    },
}

/// Which Insight sub-view is forward.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum InsightView {
    #[default]
    Search,
    Digest,
    Analytics,
    Prompts,
    Failures,
    Decisions,
    File,
    Docs,
}

impl InsightView {
    const ALL: [InsightView; 8] = [
        InsightView::Search,
        InsightView::Digest,
        InsightView::Analytics,
        InsightView::Prompts,
        InsightView::Failures,
        InsightView::Decisions,
        InsightView::File,
        InsightView::Docs,
    ];
    fn label(self) -> &'static str {
        match self {
            InsightView::Search => "Search",
            InsightView::Digest => "Digest",
            InsightView::Analytics => "Analytics",
            InsightView::Prompts => "Prompts",
            InsightView::Failures => "Failures",
            InsightView::Decisions => "Decisions",
            InsightView::File => "File",
            InsightView::Docs => "Docs",
        }
    }
}

/// The Insight pane's caches. Answers echo their question (query, day, repo,
/// path) and stale ones are dropped — the stray-answer rule, pane-wide.
#[derive(Default)]
struct InsightState {
    view: InsightView,
    query: String,
    results: Option<(String, mogeung_core::insight::SearchResults)>,
    search_pending: bool,
    day: String,
    digest: Option<mogeung_core::insight::DayDigest>,
    digest_pending: bool,
    analytics: Option<mogeung_core::insight::Analytics>,
    analytics_asked: bool,
    prompts: Option<Vec<mogeung_core::insight::PromptCluster>>,
    prompts_asked: bool,
    failures: Option<Vec<mogeung_core::insight::RecurringFailure>>,
    failures_asked: bool,
    decisions: HashMap<SessionId, Vec<mogeung_core::insight::DecisionCandidate>>,
    decisions_asked: HashSet<SessionId>,
    file_query: String,
    file_results: Option<(String, Vec<mogeung_core::insight::FileSession>)>,
    docs_repo: String,
    docs: Option<(String, mogeung_core::docs::DocInventory)>,
    docs_pending: bool,
}

/// One repo's signal-runner view state. `R-E2`.
#[derive(Default)]
struct SignalState {
    command: Option<String>,
    running: bool,
    last: Option<mogeung_core::verify::SignalRun>,
    /// The command as typed; initialised from the store once.
    edit: String,
    edit_init: bool,
    fetched: bool,
    asked: bool,
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
        // Before the first frame is styled: the palette has to be in force
        // when `apply_theme` reads it, or the window flashes the wrong theme.
        let (prefs_for_theme, _) = crate::prefs::Prefs::load();
        crate::theme::set(
            prefs_for_theme
                .theme
                .resolve(cc.egui_ctx.input(|i| i.raw.system_theme)),
        );
        ui::apply_theme(&cc.egui_ctx);
        // R-B29: PNG/JPEG decoding for the Editor's image preview.
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(h) = &hotkey {
            h.start_waker(cc.egui_ctx.clone());
        }
        let net = Net::connect(url, cc.egui_ctx.clone());
        let (keymap, keymap_warning) = crate::keymap::Keymap::load();
        let (prefs, prefs_warning) = crate::prefs::Prefs::load();
        // Read again rather than threaded down from `main`: a detached window
        // has no terminal, so the complaint printed there reaches nobody.
        let config_warning = mogeung_core::config::Config::load().1;
        let (tree, layout_warning) = crate::layout::load();
        let (explorer, explorer_warning) = crate::explorer::Explorer::load();
        let (connections, connections_warning) = crate::connections::Connections::load();
        // Before `prefs` is moved into the struct. The shells are restored
        // here but not spawned — a tab from the last run costs a tmux client
        // only once the panel is actually opened.
        let shells = crate::shells::Shells::from_prefs(&prefs.terminal_panel, &prefs.scoped.shells);
        // And before the first frame, which is the point: `set_fonts` takes
        // effect at the *next* frame boundary, and a `FontFamily::Name` egui
        // has not been told about is a panic in the paint loop rather than a
        // fallback. Registering here means frame one already knows the name.
        let font_warning = crate::font::install(&cc.egui_ctx, prefs.terminal_font.as_deref());
        let font_installed = Some(prefs.terminal_font.clone());
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
            usage: None,
            usage_asked: None,
            signals: HashMap::new(),
            insight: InsightState::default(),
            focus_event_ts: None,
            subagents: HashMap::new(),
            subagents_asked: HashSet::new(),
            outline_open: false,
            outline_filter: String::new(),
            show_goto: false,
            goto_input: String::new(),
            occurrences_word: None,
            md_preview: HashSet::new(),
            md_cache: egui_commonmark::CommonMarkCache::default(),
            transcript_limit: TRANSCRIPT_PAGE,
            scroll: None,
            launch_mode: daemon_mode.clone(),
            daemon_mode,
            daemon_addr,
            daemon_identity: None,
            connections,
            show_connections: false,
            conn_draft: None,
            scan: None,
            scanned: Vec::new(),
            scan_since: None,
            transcript_find: String::new(),
            last_rail: crate::prefs::RailTool::Files,
            search: SearchPanel::default(),
            first_snapshot: false,
            notes: Vec::new(),
            note_draft: None,
            this_machine: mogeungd::machine::machine_id(),
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
            git_filter_focus: false,
            agent_term: None,
            agent_failed: None,
            shells,
            font_installed,
            pty_focus: None,
            geometry_checked: false,
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
                .chain(config_warning)
                .chain(layout_warning)
                .chain(explorer_warning)
                .chain(connections_warning)
                .chain(font_warning)
                .collect(),
        }
    }

    fn ingest(&mut self) {
        let mut sessions_changed = false;
        for msg in self.net.drain() {
            match msg {
                ServerMsg::Snapshot {
                    sessions,
                    queue,
                    daemon,
                } => {
                    self.sessions = sessions
                        .into_iter()
                        .map(|s| (s.id.clone(), Rc::new(s)))
                        .collect();
                    self.queue = queue;
                    // `None` from a daemon older than R-I5; keep whatever we
                    // were told rather than forgetting who we are talking to.
                    if daemon.is_some() {
                        self.daemon_identity = daemon;
                    }
                    self.first_snapshot = true;
                    // First moment we know whose machine this is, which is
                    // when the view state that belongs to it can be loaded.
                    // `R-I11`.
                    self.adopt_scope();
                    // And the notes, which belong to the *daemon* rather than
                    // to this machine (`R-B35`, ADR-0015) — so they are asked
                    // for rather than loaded, and re-asked on every reconnect
                    // because another window may have written one meanwhile.
                    self.net.send(ClientMsg::NoteList);
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
                ServerMsg::UsageStats { report } => self.usage = Some(*report),
                ServerMsg::SignalStatus {
                    repo,
                    command,
                    running,
                    last,
                } => {
                    let st = self.signals.entry(repo).or_default();
                    st.running = running;
                    st.last = last.map(|b| *b);
                    st.fetched = true;
                    // Fill the editor from the store once; after that the
                    // user's typing wins over echoes.
                    if !st.edit_init {
                        st.edit = command.clone().unwrap_or_default();
                        st.edit_init = true;
                    }
                    st.command = command;
                }
                ServerMsg::InsightSearchResults { query, results } => {
                    // Two askers, each dropping what is not theirs. Neither
                    // clobbers the other, because the daemon echoes the query
                    // and both compare against their own. `R-F13`.
                    if self.search.issued == query && self.search.everywhere {
                        self.search.insight.running = false;
                        self.search.insight.hits = Some(results.hits.clone());
                        self.search.insight.truncated = results.truncated;
                    }
                    self.insight.search_pending = false;
                    // Only the answer to the query still on screen.
                    if query == self.insight.query.trim() {
                        self.insight.results = Some((query, *results));
                    }
                }
                ServerMsg::DayDigestReport { day, digest } => {
                    self.insight.digest_pending = false;
                    if day == self.insight.day {
                        self.insight.digest = Some(*digest);
                    }
                }
                ServerMsg::RecurringFailures { failures } => {
                    self.insight.failures = Some(failures);
                }
                ServerMsg::PromptLibrary { clusters } => {
                    self.insight.prompts = Some(clusters);
                }
                ServerMsg::AnalyticsReport { analytics } => {
                    self.insight.analytics = Some(*analytics);
                }
                ServerMsg::SubagentTreeReport { session_id, nodes } => {
                    self.subagents.insert(session_id, nodes);
                }
                ServerMsg::DecisionReport {
                    session_id,
                    candidates,
                } => {
                    self.insight.decisions.insert(session_id, candidates);
                }
                ServerMsg::FileSessions { path, entries } => {
                    if path == self.insight.file_query.trim() {
                        self.insight.file_results = Some((path, entries));
                    }
                }
                ServerMsg::DocReport { repo, inventory } => {
                    self.insight.docs_pending = false;
                    if repo == self.insight.docs_repo {
                        self.insight.docs = Some((repo, *inventory));
                    }
                }
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
                } => {
                    // Two consumers, neither aware of the other. The Editor
                    // fills whichever tabs hold this path; the search panel's
                    // preview takes it only if this is the file it asked for.
                    // `R-F13` — a preview that opened a tab would be a
                    // selection changing the thing it is previewing.
                    if self.search.preview_asked.as_deref() == Some(path.as_str()) {
                        self.search.preview = Some((path.clone(), content.clone()));
                    }
                    self.explorer
                        .ingest_file(&session_id, path, content, truncated)
                }
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
                } => {
                    // The palette and the search panel both ask this, and both
                    // are fed here rather than sharing a slot on the explorer
                    // state — two writers into one `SearchState` would blank
                    // each other's results and each other's in-flight flag.
                    // `R-F13`.
                    if self.search.issued == query
                        && self.selected.as_ref() == Some(&session_id)
                    {
                        self.search.files.running = false;
                        self.search.files.hits = Some(matches.clone());
                        self.search.files.truncated = truncated;
                    }
                    self.explorer
                        .ingest_matches(&session_id, &query, matches, truncated)
                }
                ServerMsg::GitCommits {
                    session_id,
                    skip,
                    commits,
                    done,
                    rev,
                    grep,
                    author,
                    path,
                    pickaxe,
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
                        pickaxe,
                    },
                ),
                ServerMsg::GitCommitDiff {
                    session_id,
                    sha,
                    files,
                    detail,
                    context,
                    ignore_ws,
                } => self.gitview.ingest_commit_diff(
                    &session_id,
                    sha,
                    files,
                    detail.map(|d| *d),
                    context,
                    ignore_ws,
                ),
                ServerMsg::GitLocalChanges {
                    session_id,
                    entries,
                } => self.gitview.ingest_status(&session_id, entries),
                ServerMsg::GitFileDiff {
                    session_id,
                    path,
                    files,
                    context,
                    ignore_ws,
                } => self
                    .gitview
                    .ingest_file_diff(&session_id, path, files, context, ignore_ws),
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
                // `R-B35`. The whole set, every time — the daemon owns them,
                // so this is a replacement rather than a merge and two windows
                // cannot drift apart (ADR-0015).
                ServerMsg::Notes { notes } => self.notes = notes,
                ServerMsg::GitFetched {
                    session_id,
                    updates,
                    upstream,
                    ahead,
                    behind,
                } => {
                    self.gitview.fetching = false;
                    if self.gitview.session.as_ref() == Some(&session_id) {
                        self.gitview.fetched = Some(crate::gitview::Fetched {
                            updates,
                            upstream,
                            ahead,
                            behind,
                            at: std::time::Instant::now(),
                        });
                    }
                }
                ServerMsg::GitStashDiff {
                    session_id,
                    index,
                    files,
                    context,
                    ignore_ws,
                } => self
                    .gitview
                    .ingest_stash_diff(&session_id, index, files, context, ignore_ws),
                ServerMsg::GitSubmoduleList {
                    session_id,
                    submodules,
                } => self.gitview.ingest_submodules(&session_id, submodules),
                ServerMsg::GitRangeDiff {
                    session_id,
                    from,
                    to,
                    files,
                    context,
                    ignore_ws,
                } => self
                    .gitview
                    .ingest_range_diff(&session_id, from, to, files, context, ignore_ws),
                ServerMsg::GitReflogList {
                    session_id,
                    entries,
                } => self.gitview.ingest_reflog(&session_id, entries),
                ServerMsg::GitWorktreeList {
                    session_id,
                    worktrees,
                } => self.gitview.ingest_worktrees(&session_id, worktrees),
                ServerMsg::GitConflictStages {
                    session_id,
                    path,
                    base,
                    ours,
                    theirs,
                    truncated,
                } => self
                    .gitview
                    .ingest_conflict(&session_id, path, base, ours, theirs, truncated),
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
                    // A failed fetch answers with this and never with
                    // `GitFetched`, so the in-flight flag has to be cleared
                    // here or `Ctrl+T` is dead for the rest of the session.
                    self.gitview.fetching = false;
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
        // Two of the search panel's three groups are scoped to the selected
        // session, so they now describe somewhere else. Dropped rather than
        // re-run: a selection change is not a search, and re-issuing one on
        // every click through the queue would be a scan per click. `R-F13`.
        self.search.files.idle();
        self.search.preview = None;
        self.search.preview_asked = None;
        if matches!(
            self.search.sel,
            Some(SearchSel::Turn(_) | SearchSel::File { .. })
        ) {
            self.search.sel = None;
        }
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

    /// Keep the window's styling in step with the theme preference. `R-J6`.
    ///
    /// Checked every frame rather than only when the setting is touched,
    /// because `system` mode has a third party in it: the desktop can change
    /// underneath us — most visibly on a machine that switches at sunset — and
    /// nothing tells the app but the next frame's input.
    ///
    /// `theme::set` reports whether anything moved, so the styles are rebuilt
    /// on the frame the answer changes and never on the thousands in between.
    fn follow_theme(&mut self, ui: &egui::Ui) {
        let system = ui.ctx().input(|i| i.raw.system_theme);
        if crate::theme::set(self.prefs.theme.resolve(system)) {
            ui::apply_theme(ui.ctx());
        }
    }

    /// Remember where the window is, and rescue it if it came back somewhere
    /// unreachable. `R-J1`.
    ///
    /// # Size always, position only where the platform has one
    ///
    /// **Wayland reports no window position at all** — `outer_position()`
    /// returns `NotSupported`, so egui leaves *both* `outer_rect` and
    /// `inner_rect` empty and there is no rect to read. The first version keyed
    /// everything off `outer_rect` and therefore did nothing whatsoever on the
    /// machine it was written on: no file, no size, no complaint. Found by
    /// running the window, which no test here would have done.
    ///
    /// So the size comes from the drawing surface, which every platform has,
    /// and the position from `outer_rect` when there is one. Under Wayland a
    /// window keeps its size across launches and is placed by the compositor,
    /// which is that platform's answer rather than a degradation of ours.
    ///
    /// The rescue is here rather than at startup because the monitor
    /// arrangement is not knowable before the window exists: `main` restores
    /// the stored position optimistically, and this corrects it on the first
    /// frame that reports a monitor. Unplugging the display the window was
    /// last closed on is the case that matters, and it is not exotic.
    fn track_geometry(&mut self, ui: &egui::Ui) {
        let (outer, monitor, maximized, fullscreen) = ui.ctx().input(|i| {
            let v = i.viewport();
            (
                v.outer_rect,
                v.monitor_size,
                v.maximized.unwrap_or(false),
                v.fullscreen.unwrap_or(false),
            )
        });

        // Points to logical pixels. `viewport_rect` and `outer_rect` are both in
        // egui points, which shrink as the global zoom grows; `with_inner_size`
        // is in logical pixels. Without the factor, launching zoomed would
        // store a smaller window than the one on screen, and every restart
        // would shrink it again.
        let zoom = ui.ctx().zoom_factor();
        let size = ui.ctx().viewport_rect().size() * zoom;
        let pos = outer.map(|r| r.min * zoom);

        if !self.geometry_checked {
            self.geometry_checked = true;
            if let Some(p) = pos {
                let here = crate::prefs::Window {
                    width: size.x,
                    height: size.y,
                    x: Some(p.x),
                    y: Some(p.y),
                };
                let reachable = here
                    .usable(monitor.map(|m| (m.x * zoom, m.y * zoom)))
                    .is_some_and(|w| w.x.is_some());
                if !reachable {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                        egui::pos2(48.0, 48.0),
                    ));
                    return;
                }
            }
        }

        // A maximized or fullscreen window reports the screen, not the size you
        // would want back on restore — remembering it would mean un-maximizing
        // into a window that fills the screen anyway, with no way back to the
        // size you had.
        if maximized || fullscreen {
            return;
        }
        // Written while the pointer is up, on the same reasoning as the layout
        // below: a drag-resize would otherwise write the file every frame.
        if ui.input(|i| i.pointer.any_down()) {
            return;
        }
        // A window not yet mapped reports nothing usable; storing it would
        // overwrite a good remembered size with a placeholder.
        if size.x < crate::prefs::MIN_SIZE.0 || size.y < crate::prefs::MIN_SIZE.1 {
            return;
        }
        let now = crate::prefs::Window {
            width: size.x,
            height: size.y,
            x: pos.map(|p| p.x),
            y: pos.map(|p| p.y),
        };
        let moved = match self.prefs.window {
            Some(w) => {
                (w.width - now.width).abs() > 1.0
                    || (w.height - now.height).abs() > 1.0
                    || w.x.zip(now.x).is_none_or(|(a, b)| (a - b).abs() > 1.0)
                    || w.y.zip(now.y).is_none_or(|(a, b)| (a - b).abs() > 1.0)
            }
            None => true,
        };
        if moved {
            self.prefs.window = Some(now);
            self.prefs_dirty = true;
        }
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

        // Before anything measures text in the terminal family.
        self.sync_terminal_font(ui.ctx());

        self.handle_keys(ui);
        self.top_bar(ui);
        self.queue_panel(ui);
        // Beside the queue and on the opposite edge, deliberately: both are
        // chrome rather than panes (ADR-0017), and declaring them together is
        // what keeps them symmetric — each full height, with the status bar
        // spanning between them.
        self.rail_panel(ui);
        // Before the detail pane: a `CentralPanel` claims whatever is left, so
        // every other panel has to be declared first. The status bar is
        // declared before the terminal, which is what puts it *below* it —
        // panels claim edges in the order they are shown.
        self.status_bar(ui);
        self.terminal_panel(ui);
        self.detail_panel(ui);
        self.launch_window(ui);
        self.health_window(ui);
        self.connections_window(ui);
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

        // Before the write below, so a move lands in the same save.
        self.track_geometry(ui);
        self.follow_theme(ui);

        // The panel's shape rides along in the preferences. Held back while
        // the pointer is down so dragging its top edge does not write the file
        // on every frame of the drag — the rule the layout already follows.
        if self.shells.dirty && !ui.input(|i| i.pointer.any_down()) {
            self.shells.dirty = false;
            let (panel, tabs) = self.shells.to_prefs();
            self.prefs.terminal_panel = panel;
            self.prefs.scoped.shells = tabs;
            self.prefs_dirty = true;
        }

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
                    .fill(pal().bg)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(root, |ui| {
            ui.horizontal(|ui| {
                // No wordmark. The window title says what this is, and a bar
                // this dense should spend its width on the queue rather than
                // on the app's own name (asked for directly, 2026-07-30).
                //
                // What the wordmark carried is the global shortcut, which is
                // worth keeping and belongs on the connection dot beside it —
                // both answer "how do I get to this window".
                let hotkey_tip = match &self.hotkey {
                    Some(h) => format!("{} brings this window forward", h.accel),
                    None => "no global shortcut — see --hotkey".to_string(),
                };

                let (dot, tip) = if self.net.connected {
                    (RichText::new(icon::DOT).color(pal().green), "connected".to_string())
                } else {
                    (
                        RichText::new(icon::DOT).color(pal().red),
                        self.net
                            .last_error
                            .clone()
                            .unwrap_or_else(|| "disconnected".into()),
                    )
                };
                // Clickable: the dot already answers "which daemon, and is it
                // up", so it is where a hand goes to change the answer. `R-I7`.
                let dot_hit = ui
                    .add(egui::Label::new(dot).sense(egui::Sense::click()))
                    .on_hover_text(format!(
                        "{} — {}\n\n{}{}\n\nClick to choose a daemon.\n\n{hotkey_tip}",
                        crate::connections::redacted(&self.net.url),
                        tip,
                        self.daemon_mode.detail(&self.daemon_addr),
                        self.daemon_provenance()
                    ));
                if dot_hit.clicked() {
                    self.show_connections = !self.show_connections;
                }

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
                    ui.label(badge(&format!("{waiting} waiting for you"), pal().red));
                } else if needing > 0 {
                    ui.label(badge(&format!("{needing} need you"), pal().amber));
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
                        (icon::WARN, Some(pal().amber))
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
                        ui.label(RichText::new(urgent.to_string()).color(pal().amber).size(11.0));
                    }

                    if icon_button(
                        ui,
                        icon::NEW_SESSION,
                        "New session — a real interactive claude in your terminal, \
                         with permission prompts skipped",
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
                    // The terminal left the tab bar for the bottom panel
                    // (`R-B33`), and a panel with no chrome anywhere is a
                    // feature only the people who read the changelog have.
                    if icon_button(
                        ui,
                        icon::TERMINAL,
                        &format!(
                            "Terminal — your own shells  ({})",
                            self.keymap.describe(crate::keymap::Action::TabTerminal)
                        ),
                        self.shells.open,
                        None,
                    )
                    .clicked()
                    {
                        self.toggle_terminal_panel();
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
                            // Editing bindings and typing into a terminal are
                            // mutually exclusive intents.
                            self.pty_focus = None;
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
                    ui.label(RichText::new("!").color(pal().red).strong());
                    ui.label(
                        RichText::new(self.errors.last().unwrap())
                            .color(pal().red)
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
        self.zoom_over(ui, pane, ui.max_rect())
    }

    /// [`pane_zoom`] over an explicit rect, for a pane that wants more than
    /// one zoom inside it. A pane whose regions are laid out with
    /// [`egui::Panel`] cannot use `max_rect` for the leftover region: a panel
    /// moves the parent's *cursor*, not its `max_rect`, so the leftover ui
    /// still claims the whole pane and would swallow a wheel over the panel
    /// too. Pass `available_rect_before_wrap()` for that region instead.
    ///
    /// [`pane_zoom`]: Self::pane_zoom
    fn zoom_over(&mut self, ui: &egui::Ui, pane: &str, rect: egui::Rect) -> f32 {
        let mut z = self.prefs.zoom_of(pane);
        if ui.rect_contains_pointer(rect) {
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

        // The search panel drives its own results while its box has focus,
        // the same contract the keyboard window's filter has below: arrows
        // walk the list without leaving the box, so refining a query and
        // reading its answers is one uninterrupted gesture.
        //
        // `R-F13`. Read before the generic dispatch, because `J`/`K`/`Down`
        // are queue navigation out here and would move the *selection* while
        // you were reading search results.
        if self.prefs.rail == Some(crate::prefs::RailTool::Search)
            && ui.memory(|m| m.has_focus(search_field_id()))
        {
            let (enter, down, up) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                )
            });
            let len = self.search.rows.len();
            let at = self
                .search
                .sel
                .as_ref()
                .and_then(|s| self.search.rows.iter().position(|r| r == s));
            if down || up {
                let (in_list, to) = search_move(self.search.in_list, at, len, down);
                self.search.in_list = in_list;
                if to != at {
                    if let Some(i) = to.and_then(|i| self.search.rows.get(i)) {
                        self.search.sel = Some(i.clone());
                        self.search.scroll_row = true;
                        self.search.preview_scroll = true;
                    }
                }
            } else if enter && self.search.in_list {
                if let Some(sel) = self.search.sel.clone() {
                    self.open_search_hit(&sel);
                }
            }
            // Everything else is typing, and belongs to the box. Enter while
            // *not* in the list falls through to the panel, which reads it as
            // "run this query".
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
        if self.show_keymap && self.pty_focus.is_none() {
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
        if self.pty_focus.is_some() {
            // Two chords, not one. `TabTerminal` is a *toggle*, and a toggle
            // that only works in one direction is not one: you press it, the
            // keyboard goes to the shell — which is the point of the shortcut
            // — and pressing it again to put the panel away would otherwise
            // type `\x1b[24;3~` into your prompt.
            //
            // Both are *consumed* rather than merely observed. This runs
            // before the pane draws, so an event left in the queue is one the
            // pty also receives — and `Alt+F12` reaching a shell is a stray
            // `\x1b[24;3~` in whatever you were halfway through typing.
            let mut leave = false;
            let mut toggle = false;
            ui.input_mut(|i| {
                i.events.retain(|e| {
                    let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = e
                    else {
                        return true;
                    };
                    match pty_chord(self.keymap.action_for(*modifiers, *key)) {
                        PtyChord::Leave => {
                            leave = true;
                            false
                        }
                        PtyChord::ToggleTerminal => {
                            toggle = true;
                            false
                        }
                        PtyChord::Pass => true,
                    }
                });
            });
            if leave {
                self.pty_focus = None;
            }
            if toggle {
                self.toggle_terminal_panel();
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
            // `R-D25`. The only key here that reaches a network beyond this
            // machine, which is why it does nothing without a session to say
            // *which* repository, and refuses to start a second fetch.
            A::SyncRemote => {
                if let Some(id) = self.selected.clone() {
                    if !self.gitview.fetching {
                        self.gitview.fetching = true;
                        self.net.send(ClientMsg::GitFetch { session_id: id });
                    }
                } else {
                    self.errors
                        .push("pick a session first — a fetch needs to know which repo".into());
                }
            }
            A::PageDown => self.scroll = Some(ScrollRequest::Pages(1.0)),
            A::PageUp => self.scroll = Some(ScrollRequest::Pages(-1.0)),
            A::ScrollTop => self.scroll = Some(ScrollRequest::Top),
            A::ScrollBottom => self.scroll = Some(ScrollRequest::Bottom),
            A::TabChanges => self.set_tab(Tab::Changes),
            A::TabTranscript => self.set_tab(Tab::Transcript),
            A::TabInfo => self.set_tab(Tab::Info),
            A::TabDebt => self.set_tab(Tab::Debt),
            A::TabAgent => self.set_tab(Tab::Agent),
            // Not a tab any more, and the only one of these that toggles: the
            // terminal is a panel taking a third of the window, so the key
            // that shows it has to be the key that puts it away. Alt+F12 in
            // IntelliJ and Ctrl+` in VS Code both behave exactly this way, and
            // both also put the cursor in the shell — a shell you must click
            // before typing has wasted the shortcut. The keyboard is handed
            // over a frame later, once the pty exists; see
            // `Shells::focus_wanted`.
            A::TabTerminal => self.toggle_terminal_panel(),
            A::TabExplorer => self.set_tab(Tab::Explorer),
            A::TabGit => self.set_tab(Tab::Git),
            A::TabInsight => self.set_tab(Tab::Insight),
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
            // There are two ptys now, so this arm has to pick one. The rule is
            // "the one you are looking at, else the agent": the shell when the
            // panel is up, because that is what is in front of you, and the
            // agent otherwise — which is the behaviour this had when the agent
            // was the only pty.
            //
            // A *closed* panel is deliberately not opened here. Doing so would
            // mean this chord silently spawned a shell nobody asked for; that
            // is what `TabTerminal` is for.
            A::ToggleTerminalFocus => {
                if self.pty_focus.is_some() {
                    self.pty_focus = None;
                } else if self.shells.open {
                    self.shells.focus_wanted = true;
                } else {
                    let attachable = self
                        .selected_session()
                        .map(|s| s.tmux_target.is_some())
                        .unwrap_or(false);
                    if attachable {
                        self.set_tab(Tab::Agent);
                        self.pty_focus = Some(PtyPane::Agent);
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
            A::FocusTerminalApp => self.focus_selected_terminal(),
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
            A::CycleTheme => {
                use crate::theme::Mode;
                self.prefs.theme = match self.prefs.theme {
                    Mode::Dark => Mode::Light,
                    Mode::Light => Mode::System,
                    Mode::System => Mode::Dark,
                };
                self.prefs_dirty = true;
                self.errors.push(format!(
                    "theme: {} — {} cycles",
                    self.prefs.theme.label(),
                    self.keymap.describe(crate::keymap::Action::CycleTheme)
                ));
            }
            A::ToggleAmbient => self.ambient = !self.ambient,
            A::ToggleHealth => {
                self.show_health = !self.show_health;
                if self.show_health {
                    self.net.send(ClientMsg::FetchHealth);
                }
            }
            A::ToggleConnections => {
                self.show_connections = !self.show_connections;
                if !self.show_connections {
                    self.conn_draft = None;
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
            // `R-B40`. Collapsing remembers nothing on purpose: the strip
            // shows which tool was last open, so reopening lands where you
            // left rather than on a default you did not choose.
            A::ToggleRailPanel => {
                self.prefs.rail = match self.prefs.rail {
                    Some(_) => None,
                    None => Some(self.last_rail),
                };
                self.prefs_dirty = true;
            }
            A::RailFiles => self.show_rail(crate::prefs::RailTool::Files),
            A::RailSearch => {
                self.show_rail(crate::prefs::RailTool::Search);
                // A search you asked for by key wants the caret, not another
                // click. The panel consumes this the frame it draws.
                self.search.focus = true;
            }
            A::ResetLayout => {
                self.tree = Some(crate::layout::default_tree());
                self.layout_dirty = true;
            }
            A::OpenKeymap => {
                self.show_keymap = !self.show_keymap;
                self.keymap_scroll = self.show_keymap;
                if self.show_keymap {
                    self.pty_focus = None;
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
            A::FindInFile => match find_target(self.tab) {
                FindTarget::GitFilter => {
                    self.set_tab(Tab::Git); // surface the pane if it was buried
                    self.git_filter_focus = true;
                }
                FindTarget::EditorFind => self.with_explorer(|app| {
                    app.explorer_find_open = true;
                    app.explorer_find_focus = true;
                }),
            },
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

    /// A daemon on another machine: local-only actions must refuse rather
    /// than act on the wrong box. `R-I4`, corrected by `R-I5`.
    ///
    /// The daemon now says which machine it is on and we compare ids. What this
    /// replaces was a substring test on the address we dialled — which answers
    /// *where did I route this*, not *whose filesystem is on the other end*.
    /// `ssh -N -L 7717:localhost:7717 devbox` makes a daemon a thousand miles
    /// away answer on `127.0.0.1`, and the old test read that as local: "Open
    /// in IntelliJ" would open a path that does not exist here, and "Launch
    /// terminal" would start a shell on the wrong machine. That tunnel is the
    /// *recommended* way to reach a remote daemon, so the guess was wrong
    /// exactly where it mattered most.
    fn remote_daemon(&self) -> bool {
        match &self.daemon_identity {
            Some(id) => !id.is_same_machine_as(self.this_machine.as_deref()),
            // Nothing has told us yet — either the first snapshot has not
            // landed or the daemon predates R-I5. Fall back to the old guess:
            // wrong for tunnels, but no worse than the behaviour it replaces,
            // and refusing everything against an older daemon would be.
            None => Self::addr_looks_remote(&self.daemon_addr),
        }
    }

    /// Point the per-machine view state at whichever daemon is now answering,
    /// and swap the terminal tabs with it. `R-I11`.
    ///
    /// Idempotent: called on every snapshot, and does nothing unless the
    /// machine actually changed. Reconnects to the same daemon therefore cost
    /// nothing, which matters because a flapping link produces a snapshot each
    /// time it comes back.
    fn adopt_scope(&mut self) {
        // A daemon older than `R-I5` publishes no identity, and one that could
        // not write `~/.mogeung/machine-id` publishes an identity without an
        // id in it. The address is a poor substitute — a tunnel makes two
        // machines look like one — but it is stable for as long as that daemon
        // is the one being dialled, which is this scope's whole lifetime.
        let origin = self
            .daemon_identity
            .as_ref()
            .and_then(|id| id.machine_id.clone())
            .unwrap_or_else(|| self.daemon_addr.clone());
        if self.prefs.origin() == Some(origin.as_str()) {
            return;
        }
        // The outgoing machine's tabs, saved before its state is swapped out.
        let (panel, tabs) = self.shells.to_prefs();
        self.prefs.terminal_panel = panel;
        if self.prefs.origin().is_some() {
            self.prefs.scoped.shells = tabs;
        }
        if let Some(note) = self.prefs.adopt_origin(&origin) {
            self.errors.push(note);
        }
        // Dropping a tmux-backed `Term` detaches rather than kills, so the
        // shells on the machine we just left keep running and come back with
        // their tabs when it does.
        self.shells = crate::shells::Shells::from_prefs(
            &self.prefs.terminal_panel,
            &self.prefs.scoped.shells,
        );
    }

    /// The pre-`R-I5` heuristic, kept only as the fallback above.
    fn addr_looks_remote(a: &str) -> bool {
        !(a.contains("127.0.0.1") || a.contains("localhost") || a.contains("[::1]"))
    }

    /// Point this window at a different daemon, without restarting it. `R-I7`.
    ///
    /// Two halves, and the second is the one that can go quietly wrong.
    ///
    /// Replacing `net` drops the old one, which is what stops its thread —
    /// see `net::post`. Then **everything the previous daemon told us has to
    /// go**, because it describes a different machine: its sessions, its
    /// diffs, its repos, its files. A stale session left in the map would
    /// render as a real row, and clicking it would ask the *new* daemon about
    /// an id it has never heard of.
    ///
    /// The rule for anything added later: if it arrived over the wire, it is
    /// cleared here. If the user chose it — the keymap, the layout, prefs — it
    /// survives, because those describe the window rather than the daemon.
    fn switch_to(&mut self, conn: &crate::connections::Connection, ctx: &egui::Context) {
        self.net = Net::connect(conn.dial_url(), ctx.clone());
        self.daemon_addr = conn.url.clone();
        // Going back to LOCAL returns to the daemon this process started or
        // found at launch, which is still exactly where it was.
        self.daemon_mode = if conn.is_local() {
            self.launch_mode.clone()
        } else {
            crate::daemon::Mode::None
        };
        self.daemon_identity = None;

        // Daemon-derived, all of it.
        self.sessions.clear();
        self.queue.clear();
        self.changes.clear();
        self.events.clear();
        self.hydrated.clear();
        self.subagents.clear();
        self.subagents_asked.clear();
        self.selected = None;
        self.selected_file = None;
        self.debt = None;
        self.blast = None;
        self.blast_pending = false;
        self.usage = None;
        self.usage_asked = None;
        self.health = Health::default();
        self.insight = InsightState::default();
        self.gitview = Default::default();
        self.explorer.forget_all();

        // Terminal panes hold a shell on the *old* machine, rooted at a path
        // the new one need not have. Closing the view is safe and is the point
        // of ADR-0011: tmux keeps the session, so this detaches rather than
        // kills, and the tab comes back if you switch back.
        self.shells.detach_all();
        self.agent_term = None;
        self.agent_failed = None;
        self.pty_focus = None;

        self.errors.clear();
    }

    /// Whether a scan is young enough that an empty list means "not yet"
    /// rather than "nothing there". mDNS answers take seconds, and telling
    /// someone nothing is out there before anyone could have replied is the
    /// difference between a tool that looks broken and one that looks patient.
    fn scan_says_waiting(&self) -> bool {
        self.scan.is_some()
            && self
                .scan_since
                .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(4))
    }

    /// Where a terminal pane should run its tmux. `R-I6`.
    ///
    /// Local while the daemon is on this machine. Otherwise the daemon's
    /// `ssh_target`, so the pane drives tmux where the files and the sessions
    /// actually are, rather than starting a local shell in a directory that
    /// exists somewhere else.
    ///
    /// `None` is the case worth naming: a remote daemon that has not been told
    /// how to be reached. There is no honest guess available — the hostname it
    /// reports need not resolve from here, and need not be the name ssh wants —
    /// so callers refuse and say what to configure.
    fn terminal_reach(&self) -> Option<crate::term::Reach> {
        if !self.remote_daemon() {
            return Some(crate::term::Reach::Local);
        }
        self.daemon_identity
            .as_ref()?
            .ssh_target
            .as_ref()
            .map(|t| crate::term::Reach::Ssh(t.clone()))
    }

    /// Why a terminal cannot be opened against this daemon, in a sentence the
    /// user can act on.
    fn no_reach_reason(&self) -> String {
        format!(
            "terminals run on {}, and it has not published an ssh target — \
             start its daemon with --ssh-target user@host (or set ssh_target \
             in its ~/.mogeung/config.toml)",
            self.other_machine()
        )
    }

    /// What to call the machine on the other end, in a sentence.
    ///
    /// Naming it is the point: "its terminals are on the other machine" leaves
    /// you wondering which, and a refusal you cannot act on reads as a bug.
    fn other_machine(&self) -> String {
        match &self.daemon_identity {
            Some(id) => id.label(),
            None => "the other machine".into(),
        }
    }

    /// The identity block appended to the connection tooltip.
    fn daemon_provenance(&self) -> String {
        let Some(id) = &self.daemon_identity else {
            return String::new();
        };
        let here = if self.remote_daemon() { "" } else { " (this machine)" };
        let ssh = match &id.ssh_target {
            Some(t) => format!("\nssh: {t}"),
            None => String::new(),
        };
        format!(
            "\n\nwatching {} on {}{here}\nmogeungd {} · pid {}{ssh}",
            id.claude_home,
            id.label(),
            id.version,
            id.pid,
        )
    }

    /// Jump-to-terminal, refused against a remote daemon — the terminal it
    /// would focus is on the other machine.
    fn send_focus(&mut self, session_id: SessionId) {
        if self.remote_daemon() {
            self.errors.push(
                format!(
                    "remote daemon — its terminals are on {}; \
                     jump-to-terminal is not available",
                    self.other_machine()
                ),
            );
            return;
        }
        self.net.send(ClientMsg::FocusTerminal { session_id });
    }

    /// Open-in, refused against a remote daemon — the path exists over there.
    fn open_in_local(&mut self, target: ui::OpenTarget, dir: &str) {
        if self.remote_daemon() {
            self.errors.push(format!(
                "remote daemon — {dir} lives on {}; open-in is not available",
                self.other_machine()
            ));
            return;
        }
        if let Err(e) = ui::open_in(target, dir) {
            self.errors.push(e);
        }
    }

    fn focus_selected_terminal(&mut self) {
        if let Some(id) = self.selected.clone() {
            let alive = self.sessions.get(&id).map(|s| s.alive).unwrap_or(false);
            if alive {
                self.send_focus(id);
            } else if let Some(s) = self.sessions.get(&id) {
                // Exited: the useful equivalent is opening where it worked.
                let dir = s.repo_root.clone().unwrap_or_else(|| s.cwd.clone());
                self.open_in_local(ui::OpenTarget::Terminal, &dir);
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
                        .fill(pal().bg)
                        .inner_margin(egui::Margin::symmetric(4, 8)),
                )
                .show(root, |ui| {
                    ui.vertical_centered(|ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("»").size(14.0).color(pal().dim)).frame(false))
                            .on_hover_text(format!("show the queue  ({key})"))
                            .clicked()
                        {
                            expand = true;
                        }
                        ui.add_space(6.0);
                        if needing > 0 {
                            ui.label(RichText::new(needing.to_string()).size(13.0).color(pal().amber).strong())
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
                            .color(if focused { pal().blue } else { pal().dim })
                            .strong(),
                    )
                    .on_hover_text("Alt+1");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(RichText::new("«").size(13.0).color(pal().dim)).frame(false))
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
                                "type to narrow  ·  ⬆⬇ choose  ·  ↩ accept  ·  esc clear"
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
                            egui::Stroke::new(1.0, pal().blue),
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
                            self.prefs.scoped.hidden.clear();
                            self.prefs_dirty = true;
                        }
                    });
                }
                // One line, and it points at the palette rather than trying to
                // list thirty bindings four at a time. The old version named
                // six of them and was wrong about which six mattered.
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("j/k move   ↩ open   / filter")
                            .size(10.5)
                            .color(pal().dim),
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
                                    .color(pal().blue),
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
                                // `R-J7`. Until the first snapshot lands, an
                                // empty board and a board that is empty
                                // *because nothing is running* look identical
                                // — which is the one confusion `R-J5`'s empty
                                // states were built to remove and the single
                                // place they did not reach, because there was
                                // no state to describe yet.
                                if !self.first_snapshot {
                                    ui.add(egui::Spinner::new().size(18.0));
                                    ui.add_space(4.0);
                                    if !self.net.connected {
                                        ui.label(dim("connecting to the daemon…"));
                                    } else {
                                        ui.label(dim("reading your sessions…"));
                                    }
                                    // Progress where there is any. The daemon
                                    // counts what it reads for the health
                                    // panel, so this costs a field rather than
                                    // a mechanism.
                                    if self.health.transcripts_found > 0 {
                                        ui.label(dim(format!(
                                            "{} transcript(s) read",
                                            self.health.transcripts_found
                                        )));
                                    }
                                    ui.add_space(6.0);
                                    ui.label(dim(
                                        "a first scan of a long history takes a moment",
                                    ));
                                } else if !self.filter.trim().is_empty() {
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
                                        RichText::new(if collapsed {
                                            icon::COLLAPSED
                                        } else {
                                            icon::EXPANDED
                                        })
                                            .size(11.0)
                                            .color(pal().dim),
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
                                                                "switch to the terminal app this session runs in",
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
                                    // under tmux the Agent tab is an
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
                                        egui::Stroke::new(1.0, pal().dim),
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
                        self.set_tab(Tab::Agent);
                    }
                }
                if let Some((session_id, minutes)) = to_snooze {
                    self.net.send(ClientMsg::Snooze {
                        session_id,
                        minutes,
                    });
                }
                if let Some(session_id) = to_focus {
                    self.send_focus(session_id);
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

    /// Show a tool in the rail — or collapse the rail, if it is the tool
    /// already showing. One key both ways, the rule `ToggleTerminalFocus`
    /// already follows: reaching a thing and leaving it should not be two
    /// chords to learn.
    fn show_rail(&mut self, tool: crate::prefs::RailTool) {
        self.last_rail = tool;
        self.prefs.rail = if self.prefs.rail == Some(tool) {
            None
        } else {
            Some(tool)
        };
        self.prefs_dirty = true;
    }

    /// The right rail: an always-present strip of tool windows, and whichever
    /// one is open. `R-B40`.
    ///
    /// Chrome, not a pane —
    /// [ADR-0017](../../../docs/decisions/0017-the-rail-is-chrome.md). Two
    /// things about where this sits are load-bearing:
    ///
    /// - It is declared beside [`Self::queue_panel`] and before the central
    ///   panel, because a `CentralPanel` claims whatever is left and anything
    ///   declared after it has already lost the argument.
    /// - The **strip** is declared before the open tool, so the strip keeps the
    ///   outermost edge. Panels claim edges in the order they are shown, and a
    ///   strip that slid inboard whenever a tool opened would move the very
    ///   button you are about to click to close it.
    fn rail_panel(&mut self, root: &mut egui::Ui) {
        use crate::keymap::Action;
        use crate::prefs::RailTool;

        let open = self.prefs.rail;
        let mut pick: Option<RailTool> = None;
        egui::Panel::right("rail-strip")
            .resizable(false)
            .exact_size(30.0)
            .frame(
                egui::Frame::NONE
                    .fill(pal().bg)
                    .inner_margin(egui::Margin::symmetric(4, 8)),
            )
            .show(root, |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for tool in RailTool::ALL {
                        let on = open == Some(tool);
                        let key = self.keymap.describe(match tool {
                            RailTool::Files => Action::RailFiles,
                            RailTool::Search => Action::RailSearch,
                        });
                        let btn = egui::Button::new(
                            RichText::new(tool.glyph())
                                .size(14.0)
                                .color(if on { pal().blue } else { pal().dim }),
                        )
                        .frame(false);
                        if ui
                            .add(btn)
                            .on_hover_text(format!("{}  ({key})", tool.label()))
                            .clicked()
                        {
                            pick = Some(tool);
                        }
                    }
                });
            });
        if let Some(t) = pick {
            self.show_rail(t);
        }

        let Some(tool) = self.prefs.rail else {
            return;
        };
        self.last_rail = tool;
        let close_key = self.keymap.describe(Action::ToggleRailPanel);
        let mut collapse = false;
        let resp = egui::Panel::right("rail")
            .default_size(self.prefs.rail_width)
            .size_range(200.0..=760.0)
            .show(root, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(tool.label().to_uppercase())
                            .size(11.0)
                            .color(pal().dim)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("»").size(13.0).color(pal().dim))
                                    .frame(false),
                            )
                            .on_hover_text(format!("collapse to the strip  ({close_key})"))
                            .clicked()
                        {
                            collapse = true;
                        }
                    });
                });
                ui.separator();
                match tool {
                    RailTool::Files => self.rail_files(ui),
                    RailTool::Search => self.rail_search(ui),
                }
            });

        // The panel owns its width while you drag it; this copies the result
        // back, because egui's own `PanelState` dies with the process — eframe
        // is built here without its `persistence` feature, so every panel width
        // in this window would otherwise reset at the next launch. Written to
        // disk only once the pointer is up, the rule the layout and the
        // terminal panel both follow.
        let now = resp.response.rect.width();
        if (now - self.prefs.rail_width).abs() > 0.5 {
            self.prefs.rail_width = now;
            self.prefs_dirty = true;
        }
        if collapse {
            self.prefs.rail = None;
            self.prefs_dirty = true;
        }
    }

    /// The worktree, in the rail. `R-B41`.
    ///
    /// This is the tree that used to sit inside the Editor tab. It is here so
    /// it is readable with the Transcript, the Git pane or a terminal forward
    /// — a map of the project is the wrong thing to be able to see only from
    /// one tab.
    fn rail_files(&mut self, ui: &mut egui::Ui) {
        let Some(s) = self.selected_session().cloned() else {
            ui.add_space(8.0);
            ui.label(dim("no session selected"));
            ui.add_space(2.0);
            ui.label(dim("a worktree belongs to one — pick a session in the queue"));
            return;
        };
        self.explorer.ensure_session(&s.id);
        self.explorer_fetch(&s);

        // Kept on the `editor-tree` zoom key it was written under. `R-B39`
        // shipped that key the day before the tree moved, and renaming it now
        // would silently reset a preference somebody had already set.
        let z = self.zoom_over(ui, "editor-tree", ui.max_rect());
        scale_text(ui, z);

        let root_label = s.repo_root.clone().unwrap_or_else(|| s.cwd.clone());
        ui.horizontal(|ui| {
            let name = root_label
                .rsplit('/')
                .next()
                .unwrap_or(&root_label)
                .to_string();
            ui.add(egui::Label::new(RichText::new(name).strong()).truncate())
                .on_hover_text(&root_label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("↻")
                    .on_hover_text("re-list the directories that are open")
                    .clicked()
                {
                    // Dropping the listings is enough: `explorer_fetch` above
                    // re-requests the root and everything expanded.
                    let st = self.explorer.current_mut();
                    st.dirs.clear();
                    st.pending.clear();
                }
            });
        });

        // Both axes, *and* wrap forced off. The scroll area alone was not
        // enough: egui deliberately hands content the visible width even with
        // horizontal scrolling on ("better to wrap text … than showing a
        // horizontal scrollbar", scroll_area.rs), so rows kept folding at the
        // pane edge and the horizontal bar never had anything to do. Extend
        // makes each row lay out at its natural width; only then does a narrow
        // panel scroll — and a tree whose rows fold onto two lines stops
        // reading as a tree.
        egui::ScrollArea::both()
            .id_salt("rail-tree-scroll")
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
    }

    /// One query over three corpora. `R-F13`.
    ///
    /// See [`SearchPanel`] for why the three are kept apart. The layout is
    /// results above, preview below, with a draggable divider — a rail is
    /// narrow, and side-by-side at 300pt would be two columns of nothing.
    fn rail_search(&mut self, ui: &mut egui::Ui) {
        let z = self.zoom_over(ui, "search", ui.max_rect());
        scale_text(ui, z);

        let session = self.selected_session().cloned();
        let mut run = false;

        ui.add_space(2.0);
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.search.query)
                .id(search_field_id())
                .hint_text("find…")
                .desired_width(f32::INFINITY),
        );
        if std::mem::take(&mut self.search.focus) {
            field.request_focus();
        }
        // Only while the arrows are still in the box. Once they are walking
        // the results, Enter belongs to the row under them — see
        // [`SearchPanel::in_list`].
        if !self.search.in_list
            && field.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
        {
            run = true;
        }
        if field.changed() {
            // Typing is leaving the list, whichever row you had reached.
            self.search.in_list = false;
            // ...and it invalidates the two daemon groups the moment the box
            // stops agreeing with what was asked. Showing yesterday's hits
            // under today's query is the one failure mode a search must not
            // have.
            if self.search.issued != self.search.query.trim() {
                self.search.files.idle();
                self.search.insight.idle();
            }
        }

        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut self.search.everywhere, "every session")
                .on_hover_text(
                    "Search the transcripts and prompt history of every session ever run \
                     (`R-F1`), not only this one's files and conversation. It is the slow \
                     half of this box.",
                )
                .changed()
            {
                run = !self.search.query.trim().is_empty();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("search")
                    .on_hover_text("or press Enter in the box")
                    .clicked()
                {
                    run = true;
                }
            });
        });

        if run {
            self.issue_global_search(session.as_deref());
            // Enter surrenders a singleline's focus in egui, and without this
            // the very next keystroke — the Down that walks into the results
            // you just asked for — would go to the queue instead.
            self.search.focus = true;
            self.search.in_list = false;
        }

        let q = self.search.query.trim().to_string();

        // The free group. Recomputed every frame from events already in
        // memory, which is what lets it answer while you type — the two below
        // cannot, and the panel says which is which rather than pretending
        // they are one search.
        let turns: Vec<(u64, crate::search::Engine, String)> = match &session {
            Some(s) if !q.is_empty() => {
                let mut v: Vec<(i32, u64, crate::search::Engine, String)> = self
                    .events
                    .get(&s.id)
                    .map(|evs| {
                        evs.iter()
                            .filter_map(|ev| {
                                let text = event_text(&ev.kind);
                                crate::search::best(&q, &text)
                                    .map(|h| (h.score, ev.seq, h.engine, one_line(&text, 200)))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                v.sort_by(|a, b| b.0.cmp(&a.0));
                v.truncate(200);
                v.into_iter().map(|(_, seq, e, t)| (seq, e, t)).collect()
            }
            _ => Vec::new(),
        };

        ui.add_space(4.0);

        // Declared before the list, so the list gets what is left rather than
        // pushing the preview off the bottom.
        egui::Panel::bottom("search-preview")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                self.search_preview(ui, session.as_deref());
            });

        let mut open: Option<SearchSel> = None;
        // Rebuilt every paint, in draw order, so the keyboard has a list to
        // walk. Read once here rather than taken per group: one keyboard move
        // must scroll exactly the group the selection landed in.
        let scroll_row = std::mem::take(&mut self.search.scroll_row);
        self.search.rows.clear();
        egui::ScrollArea::vertical()
            .id_salt("search-results")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if q.is_empty() {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.label(dim("type to search this conversation"));
                        ui.label(dim("Enter also searches files and sessions"));
                    });
                    return;
                }

                // -- this conversation ---------------------------------
                let head = match &session {
                    Some(_) => format!("this conversation · {} turn(s)", turns.len()),
                    None => "this conversation · no session selected".to_string(),
                };
                search_group(ui, "turns", &head, |ui| {
                    if session.is_none() {
                        return;
                    }
                    if turns.is_empty() {
                        ui.label(dim("no match"));
                        return;
                    }
                    for (seq, engine, text) in &turns {
                        let sel = SearchSel::Turn(*seq);
                        let picked = self.search.sel.as_ref() == Some(&sel);
                        self.search.rows.push(sel.clone());
                        let r =
                            search_row(ui, picked, picked && scroll_row, engine.label(), text);
                        if r.clicked() {
                            self.search.sel = Some(sel.clone());
                            self.search.preview_scroll = true;
                        }
                        if r.double_clicked() {
                            open = Some(sel);
                        }
                    }
                });

                // -- this session's files ------------------------------
                let head = group_head(
                    "this session's files",
                    &self.search.files,
                    session.is_none().then_some("no session"),
                );
                search_group(ui, "files", &head, |ui| {
                    let Some(hits) = &self.search.files.hits else {
                        return;
                    };
                    if hits.is_empty() {
                        ui.label(dim("no match"));
                        return;
                    }
                    for m in hits {
                        let sel = SearchSel::File {
                            path: m.path.clone(),
                            line: m.line,
                        };
                        let picked = self.search.sel.as_ref() == Some(&sel);
                        self.search.rows.push(sel.clone());
                        let (dir, base) = short_path(&m.path);
                        let r = search_row(
                            ui,
                            picked,
                            picked && scroll_row,
                            &format!("{dir}{base}:{}", m.line),
                            m.text.trim(),
                        );
                        if r.clicked() {
                            self.search.sel = Some(sel.clone());
                            self.search.preview_scroll = true;
                        }
                        if r.double_clicked() {
                            open = Some(sel);
                        }
                    }
                    if self.search.files.truncated {
                        ui.label(dim("… capped — narrow the query"));
                    }
                });

                // -- every session -------------------------------------
                let head = group_head(
                    "every session",
                    &self.search.insight,
                    (!self.search.everywhere).then_some("off"),
                );
                search_group(ui, "insight", &head, |ui| {
                    if !self.search.everywhere {
                        ui.label(dim("off — tick “every session” above"));
                        return;
                    }
                    let Some(hits) = &self.search.insight.hits else {
                        return;
                    };
                    if hits.is_empty() {
                        ui.label(dim("no match"));
                        return;
                    }
                    for h in hits {
                        let sel = SearchSel::Hit {
                            session: h.session_id.clone(),
                            line: h.line,
                            ts: h.timestamp,
                            preview: h.preview.clone(),
                        };
                        let picked = self.search.sel.as_ref() == Some(&sel);
                        self.search.rows.push(sel.clone());
                        let src = match h.source {
                            mogeung_core::insight::SearchSource::History => "hist",
                            mogeung_core::insight::SearchSource::Prompt => "you",
                            mogeung_core::insight::SearchSource::Transcript => "sess",
                        };
                        let tag = format!(
                            "{src} {}",
                            &h.session_id[..8.min(h.session_id.len())]
                        );
                        let r = search_row(ui, picked, picked && scroll_row, &tag, h.preview.trim());
                        if r.clicked() {
                            self.search.sel = Some(sel.clone());
                            self.search.preview_scroll = true;
                        }
                        if r.double_clicked() {
                            open = Some(sel);
                        }
                    }
                    if self.search.insight.truncated {
                        ui.label(dim("… capped — narrow the query"));
                    }
                });
            });

        if let Some(sel) = open {
            self.open_search_hit(&sel);
        }
    }

    /// Send the two daemon halves. The free one needs no sending.
    fn issue_global_search(&mut self, session: Option<&Session>) {
        let q = self.search.query.trim().to_string();
        self.search.issued = q.clone();
        if q.is_empty() {
            self.search.files.idle();
            self.search.insight.idle();
            return;
        }
        match session {
            Some(s) => {
                self.search.files.asked();
                self.net.send(ClientMsg::SearchContent {
                    session_id: s.id.clone(),
                    query: q.clone(),
                });
            }
            // Nothing to search, and nothing pending — a group that spun for
            // ever because no session was selected would read as a hang.
            None => self.search.files.idle(),
        }
        if self.search.everywhere {
            self.search.insight.asked();
            self.net.send(ClientMsg::InsightSearch { query: q });
        } else {
            self.search.insight.idle();
        }
    }

    /// What the selected result looks like, at more than one line of it.
    ///
    /// Three kinds of result, three fidelities, and the difference is visible
    /// on purpose: a file and a turn can be shown in full because we can
    /// fetch them, a cross-session hit cannot, because the ~200-char clip is
    /// the whole of what the daemon returns for one. `R-F13` files closing
    /// that gap as a separate wire message rather than guessing here.
    fn search_preview(&mut self, ui: &mut egui::Ui, session: Option<&Session>) {
        let Some(sel) = self.search.sel.clone() else {
            ui.add_space(6.0);
            ui.vertical_centered(|ui| ui.label(dim("pick a result")));
            return;
        };
        match sel {
            SearchSel::Turn(seq) => {
                // Taken owned before the pane is drawn: the buttons below need
                // `&mut self`, and holding a borrow of `self.events` across
                // them would be the borrow checker's problem to state and
                // ours to have caused.
                let found = session
                    .and_then(|s| self.events.get(&s.id))
                    .and_then(|evs| evs.iter().find(|e| e.seq == seq))
                    .map(|ev| (ev.ts, event_text(&ev.kind)));
                let Some((ts, text)) = found else {
                    ui.label(dim("that turn is no longer in this transcript"));
                    return;
                };
                let when = ts.format("%m-%d %H:%M").to_string();
                ui.horizontal(|ui| {
                    ui.label(dim(format!("turn {seq} · {when}")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("open").clicked() {
                            let sel = SearchSel::Turn(seq);
                            self.open_search_hit(&sel);
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("search-preview-turn")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(mono(&text).size(11.5)).wrap());
                    });
            }
            SearchSel::File { path, line } => {
                let Some(s) = session else {
                    ui.label(dim("no session selected"));
                    return;
                };
                // One request per path, not one per frame. The answer lands
                // through the same `FileContent` the Editor uses.
                if self.search.preview_asked.as_deref() != Some(path.as_str()) {
                    self.search.preview_asked = Some(path.clone());
                    self.search.preview = None;
                    self.net.send(ClientMsg::FetchFile {
                        session_id: s.id.clone(),
                        path: path.clone(),
                    });
                }
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(dim(format!("{path}:{line}"))).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("open").clicked() {
                            let sel = SearchSel::File {
                                path: path.clone(),
                                line,
                            };
                            self.open_search_hit(&sel);
                        }
                    });
                });
                let body = self
                    .search
                    .preview
                    .as_ref()
                    .filter(|(p, _)| *p == path)
                    .map(|(_, c)| c.clone());
                let Some(body) = body else {
                    ui.label(dim("reading…"));
                    return;
                };
                // Consumed here rather than at the click: the body may be
                // several frames behind the selection, and a flag spent before
                // there was anything to scroll would scroll nothing.
                let scroll_to_hit = std::mem::take(&mut self.search.preview_scroll);
                egui::ScrollArea::both()
                    .id_salt("search-preview-file")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, l) in body.lines().enumerate() {
                            let n = i as u64 + 1;
                            let hit = n == line;
                            let row = ui.horizontal(|ui| {
                                ui.label(
                                    mono(format!("{n:>5} "))
                                        .size(11.0)
                                        .color(pal().dim),
                                );
                                ui.label(if hit {
                                    mono(l).size(11.5).color(pal().amber)
                                } else {
                                    mono(l).size(11.5)
                                });
                            });
                            if hit && scroll_to_hit {
                                row.response.scroll_to_me(Some(egui::Align::Center));
                            }
                        }
                    });
            }
            SearchSel::Hit {
                session: sid,
                line,
                ts,
                preview,
            } => {
                let known = self.sessions.contains_key(&sid);
                ui.horizontal(|ui| {
                    let when = ts
                        .map(|t| t.format("%m-%d %H:%M").to_string())
                        .unwrap_or_default();
                    ui.label(dim(format!(
                        "{} L{line} {when}",
                        &sid[..8.min(sid.len())]
                    )));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = ui
                            .add_enabled(known, egui::Button::new("open").small())
                            .on_disabled_hover_text(
                                "this session is not in the queue — the search found it on \
                                 disk, but nothing here is watching it",
                            );
                        if btn.clicked() {
                            let sel = SearchSel::Hit {
                                session: sid.clone(),
                                line,
                                ts,
                                preview: preview.clone(),
                            };
                            self.open_search_hit(&sel);
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .id_salt("search-preview-hit")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(mono(&preview).size(11.5)).wrap());
                        ui.add_space(6.0);
                        // Said rather than hidden. The clip is all there is,
                        // and a preview pane that looked truncated without
                        // saying why would read as a bug in the fetch.
                        ui.label(dim(
                            "this is the whole clip the search returns — open it to read \
                             the conversation around it",
                        ));
                    });
            }
        }
    }

    /// Open a result where it lives. Three destinations, all of which already
    /// existed as jump targets before this panel did.
    fn open_search_hit(&mut self, sel: &SearchSel) {
        match sel {
            SearchSel::Turn(seq) => {
                let Some(id) = self.selected.clone() else {
                    return;
                };
                let ts = self
                    .events
                    .get(&id)
                    .and_then(|evs| evs.iter().find(|e| e.seq == *seq))
                    .map(|e| e.ts);
                // A jump to a moment must be able to reach skipped history.
                if let Some(evs) = self.events.get(&id) {
                    self.transcript_limit = self.transcript_limit.max(evs.len());
                }
                self.focus_event_ts = ts;
                self.set_tab(Tab::Transcript);
            }
            SearchSel::File { path, line } => self.open_in_explorer(path, Some(*line)),
            SearchSel::Hit { session, ts, .. } => {
                if self.sessions.contains_key(session) {
                    // `select`, not a bare assignment: a session reached from
                    // the cross-session corpus is usually one this window has
                    // never hydrated, and landing on an empty Transcript is
                    // the failure this jump exists to avoid. Setting the
                    // moment *after* is what survives the switch.
                    self.select(session.clone());
                    self.focus_event_ts = *ts;
                    self.set_tab(Tab::Transcript);
                }
            }
        }
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
                        ui.label(dim("↩ removes the label · esc cancels"));
                    } else {
                        // The badge exactly as the card will wear it, colour
                        // included — the preview is the promise.
                        ui.label(badge(&truncate(trimmed, 24), label_color(trimmed)));
                        ui.label(dim("↩ saves · esc cancels"));
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
                egui::Stroke::new(1.0, pal().blue.linear_multiply(0.5)),
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
        pal().bg
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
    ui.label(RichText::new(value).size(11.5).color(pal().text));
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
                        .color(if capturing { pal().amber } else { pal().blue }),
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
            if binding == "unbound" { pal().dim } else { pal().blue },
        )
        .width();
    painter.text(
        egui::pos2(rect.right() - pad - key_width - 12.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        action.group(),
        egui::FontId::proportional(10.5),
        pal().dim,
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
        // Chosen against the chip it sits on, not fixed white: see
        // `theme::on_accent_for`, and the amber badge nobody could read.
        on_accent_for(color),
    );
    // The dot outranks the letter: the strip's first job is still "which
    // ones need me", and colour alone must not be asked to carry it.
    if needs_human {
        painter.circle_filled(rect.right_top() + egui::vec2(-2.0, 2.0), 3.0, pal().red);
    }
    if selected {
        painter.rect_stroke(
            rect.expand(1.5),
            6.0,
            egui::Stroke::new(1.5, pal().text_strong),
            egui::StrokeKind::Outside,
        );
    } else if resp.hovered() {
        painter.rect_stroke(
            rect.expand(1.5),
            6.0,
            egui::Stroke::new(1.0, pal().dim),
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
            pal().dim,
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
            pal().blue,
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

/// Where an arrow lands in the search results. `R-F13`.
///
/// Returns the new `(in_list, index)`. `in_list == false` means the arrows
/// belong to the query box again, which is the state that lets Enter go back
/// to meaning "run this search".
///
/// Deliberately **not** a wrapping cursor. Wrapping is right for a modal
/// palette, where the list is the only thing there; here the list sits under a
/// box you are still editing, and a cursor that jumped from the last hit to the
/// first would leave no keyboard way back to the query.
fn search_move(
    in_list: bool,
    at: Option<usize>,
    len: usize,
    down: bool,
) -> (bool, Option<usize>) {
    if len == 0 {
        return (false, None);
    }
    if down {
        return match (in_list, at) {
            (true, Some(i)) => (true, Some((i + 1).min(len - 1))),
            // Down from the box enters at the top, wherever the mouse last
            // left the selection — arriving mid-list because of an earlier
            // click would read as the cursor jumping.
            _ => (true, Some(0)),
        };
    }
    match (in_list, at) {
        (false, _) => (false, at),
        (true, Some(0)) | (true, None) => (false, at),
        (true, Some(i)) => (true, Some(i - 1)),
    }
}

/// The search panel's query box. `R-F13`.
///
/// Fixed rather than generated, for the reason [`keymap_filter_id`] is: the
/// keyboard handler runs before the panel is drawn and has to ask whether this
/// particular box holds focus.
fn search_field_id() -> egui::Id {
    egui::Id::new("rail-search-query")
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

/// Where Ctrl+F lands, decided by the pane the keyboard is aimed at
/// (`self.tab` — a click in any pane aims it there).
///
/// Find used to mean "raise the Editor and open its find bar" from anywhere,
/// which read as a misfire when the Git pane was focused: the pane you were
/// searching vanished behind another tab. In the Git pane, find is the log
/// filter — it already speaks `find:` pickaxe, `author:` and `path:`.
/// Everywhere else the Editor behaviour stands, because no other pane has a
/// search of its own to offer instead.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum FindTarget {
    GitFilter,
    EditorFind,
}

fn find_target(tab: Tab) -> FindTarget {
    match tab {
        Tab::Git => FindTarget::GitFilter,
        _ => FindTarget::EditorFind,
    }
}

/// What the user asked for by clicking inside a card, as opposed to clicking
/// the card itself.
#[derive(Default)]
struct CardHit {
    /// The repo name was clicked: narrow the queue to it.
    filter_repo: Option<String>,
    /// The label badge was clicked: narrow the queue to that label.
    filter_label: Option<String>,
    /// The corner `×` was clicked.
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
            ui.label(badge("PIN", pal().blue));
        }
        if hidden {
            ui.label(badge("HIDDEN", pal().dim));
        }
        ui.label(badge(item.reason.label(), reason_color(item.reason)));
        if s.is_snoozed(now) {
            ui.label(badge(icon::SNOOZE, pal().dim));
        }
        if s.alive {
            let (txt, col) = match s.live_status {
                Some(LiveStatus::Busy) => ("live·busy", pal().blue),
                Some(LiveStatus::Idle) => ("live·idle", pal().amber),
                _ => ("live", pal().dim),
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
                        egui::Button::new(RichText::new(icon::HIDE).size(11.0).color(pal().dim))
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
            // Blue, not dim — repo and branch sat side by side in one grey and
            // read as a single phrase. Blue is what the status bar already
            // tints git identity, so the two places agree.
            ui.label(
                RichText::new(format!("{} {b}", icon::BRANCH))
                    .size(12.0)
                    .color(pal().blue),
            );
        }
        // R-I1: sessions from another CLI say so; Claude Code stays unmarked
        // as the default it has always been.
        if s.source == mogeung_core::session::SessionSource::Codex {
            ui.label(RichText::new("codex").size(10.5).color(pal().purple));
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
        // R-E4. Quiet — a mark, not an alarm; the Info tab has the long form.
        if !s.alive && s.unverified() {
            ui.label(RichText::new("unverified").size(10.5).color(pal().amber))
                .on_hover_text("edited files; no build/test/typecheck completed");
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
            ui.label(RichText::new("⚠ COLLISION").size(10.5).color(pal().red).strong());
            ui.label(
                RichText::new(format!(
                    "{} also being edited by {}",
                    truncate(&paths.join(", "), 60),
                    truncate(&others.join(", "), 40)
                ))
                .size(11.0)
                .color(pal().red),
            );
        });
    }

    // R-B7. Advisory, not a queue tier: repetition is suggestive, not proof.
    if let Some(sig) = &s.loop_signal {
        ui.label(
            RichText::new(format!("↻ {}", truncate(sig, 80)))
                .size(11.0)
                .color(pal().purple),
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
                    .fill(pal().bg)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(root, |ui| {
            ui.horizontal(|ui| {
                ui.style_mut().interaction.selectable_labels = false;
                let now = Utc::now();
                // Burn is a slow-moving number; a minute of staleness is
                // invisible, a scan per frame would not be.
                if self
                    .usage_asked
                    .map(|t| (now - t).num_seconds() >= 60)
                    .unwrap_or(true)
                {
                    self.usage_asked = Some(now);
                    self.net.send(ClientMsg::FetchUsage);
                }
                let Some(s) = self.selected_session().cloned() else {
                    // Never blank: with nothing selected it still answers "is
                    // anything running".
                    let live = self.sessions.values().filter(|s| s.alive).count();
                    ui.label(dim(format!(
                        "{live} live session(s) · {} known",
                        self.sessions.len()
                    )));
                    self.burn_stat(ui);
                    return;
                };

                stat(ui, icon::FOLDER, &s.repo_name(), pal().blue);
                if let Some(b) = &s.git_branch {
                    stat(ui, icon::BRANCH, b, pal().blue);
                }
                // Amber once it has been waiting on you: the one tint here
                // that means something rather than merely separating.
                let waiting = s.waiting_secs(now).is_some();
                stat(
                    ui,
                    icon::CLOCK,
                    &fmt_dur(s.duration_secs(now)),
                    if waiting { pal().amber } else { pal().dim },
                );
                stat(ui, icon::TURNS, &s.turns.to_string(), pal().purple);
                stat(ui, icon::TOOLS, &s.tool_calls.to_string(), pal().green);
                stat(ui, icon::TOKENS, &tokens(s.tokens_out), pal().dim);
                // Only while alive: a dead session's remembered pid now
                // belongs to its `/clear` successor, and printing it here
                // would name the wrong process.
                if let Some(pid) = s.pid.filter(|_| s.alive) {
                    stat(ui, "#", &pid.to_string(), pal().dim);
                }
                self.burn_stat(ui);

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

    /// The trailing five-hour burn, tinted by how close it sits to the
    /// estimated limit — and always labelled an estimate, because the CLI
    /// publishes no quota. `R-G2`.
    fn burn_stat(&self, ui: &mut egui::Ui) {
        let Some(u) = &self.usage else { return };
        let base = format!("5h {}", tokens(u.window_tokens_out));
        match u.window_fraction() {
            Some(f) if f >= 0.8 => {
                stat(ui, icon::TOKENS, &format!("{base} ≈{:.0}% of est. limit", f * 100.0), pal().red)
            }
            Some(f) if f >= 0.5 => {
                stat(ui, icon::TOKENS, &format!("{base} ≈{:.0}% of est. limit", f * 100.0), pal().amber)
            }
            _ => stat(ui, icon::TOKENS, &base, pal().dim),
        }
        ui.label(dim("")).on_hover_text(
            "output tokens in the trailing five hours; the limit fraction is \
             an estimate from past limit hits, not a published quota",
        );
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
                    // A limit-hit session must not claim to be waiting on you
                    // — typing at it does nothing until the reset. `R-G1`.
                    let (txt, col) = if s.limit_hit_at.is_some() {
                        ("RATE LIMITED", pal().amber)
                    } else {
                        match s.live_status {
                            Some(LiveStatus::Idle) => ("WAITING FOR YOU", pal().red),
                            Some(LiveStatus::Busy) => ("BUSY", pal().blue),
                            _ => ("LIVE", pal().dim),
                        }
                    };
                    ui.label(badge(txt, col));
                } else {
                    ui.label(badge("ended", pal().dim));
                    // R-E4: the header twin of the queue-row mark.
                    if s.unverified() {
                        ui.label(RichText::new("unverified").size(10.5).color(pal().amber))
                            .on_hover_text(
                                "edited files; no build/test/typecheck completed — see Info",
                            );
                    }
                }
                ui.add(
                    egui::Label::new(RichText::new(s.label()).size(14.0).strong()).truncate(),
                )
                .on_hover_text(s.label());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("…", |ui| {
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

                    // The editor handoffs, visible again. They lived in the …
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
                            self.open_in_local(t, &s.cwd);
                        }
                    }
                });
            });

            if let Some(err) = &s.error {
                ui.label(RichText::new(err).color(pal().red).size(12.0));
            }
            if let Some(w) = s.waiting_secs(now) {
                ui.label(
                    RichText::new(format!(
                        "This session has been waiting for your input for {}.",
                        fmt_dur(w)
                    ))
                    .color(pal().amber)
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
                            .color(pal().dim),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.flagged.is_empty()
                        && ui
                            .button(RichText::new(format!("{} {} flagged", icon::FLAG, self.flagged.len())).color(pal().amber))
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
            Tab::Agent => self.agent_tab(ui, s),
            // Stripped from every layout on load, so this is unreachable in
            // practice. It says where the shell went rather than drawing an
            // empty pane, because "unreachable in practice" is a claim about
            // code someone will edit later.
            Tab::Terminal => {
                ui.add_space(8.0);
                ui.label("The terminal is a panel now, not a pane.");
                ui.add_space(4.0);
                if ui
                    .button(format!(
                        "{} Open it  ({})",
                        icon::TERMINAL,
                        self.keymap.describe(crate::keymap::Action::TabTerminal)
                    ))
                    .clicked()
                {
                    self.open_terminal_panel();
                }
            }
            Tab::Explorer => self.explorer_tab(ui, s),
            Tab::Git => self.git_tab(ui, s),
            Tab::Insight => self.insight_tab(ui),
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
                    pickaxe: gv.log_pickaxe.clone(),
                });
            }
            if !gv.reflog_loaded && !gv.reflog_pending {
                gv.reflog_pending = true;
                self.net.send(ClientMsg::GitReflog {
                    session_id: s.id.clone(),
                });
            }
            if !gv.worktrees_loaded && !gv.worktrees_pending {
                gv.worktrees_pending = true;
                self.net.send(ClientMsg::GitWorktrees {
                    session_id: s.id.clone(),
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
                        context: Some(gv.context()),
                        ignore_ws: Some(gv.diff_ignore_ws),
                    });
                }
                crate::gitview::Selection::Local(path)
                    if !gv.local_diffs.contains_key(&path)
                        && gv.pending_file_diffs.insert(path.clone()) =>
                {
                    self.net.send(ClientMsg::GitDiffFile {
                        session_id: s.id.clone(),
                        path,
                        context: Some(gv.context()),
                        ignore_ws: Some(gv.diff_ignore_ws),
                    });
                }
                crate::gitview::Selection::Stash(index)
                    if !gv.stash_diffs.contains_key(&index)
                        && gv.pending_stash_shows.insert(index) =>
                {
                    self.net.send(ClientMsg::GitStashShow {
                        session_id: s.id.clone(),
                        index,
                        context: Some(gv.context()),
                        ignore_ws: Some(gv.diff_ignore_ws),
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
                        context: Some(gv.context()),
                        ignore_ws: Some(gv.diff_ignore_ws),
                    });
                }
                crate::gitview::Selection::Conflict(path)
                    if !gv.conflicts.contains_key(&path)
                        && gv.pending_conflicts.insert(path.clone()) =>
                {
                    self.net.send(ClientMsg::GitConflictFile {
                        session_id: s.id.clone(),
                        path,
                    });
                }
                _ => {}
            }
        }

        egui::Panel::left("git-left")
            .default_size(320.0)
            .resizable(true)
            .show(ui, |ui| {
            self.git_header(ui);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("LOCAL CHANGES").size(11.0).color(pal().dim).strong());
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
                    self.git_ref_sections(ui, s);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("LOG").size(11.0).color(pal().dim).strong());
                        if let Some(rev) = self.gitview.log_rev.clone() {
                            if ui
                                .small_button(format!("{} {rev} ×", icon::BRANCH))
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
                                .hint_text("filter: text · author:… · path:… · find:…")
                                .desired_width(ui.available_width() - 24.0),
                        );
                        if self.git_filter_focus {
                            field.request_focus();
                            self.git_filter_focus = false;
                        }
                        let entered =
                            field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if entered {
                            let (grep, author, path, pickaxe) =
                                crate::gitview::parse_filter_input(&self.gitview.filter_input);
                            self.gitview.set_log_filter(grep, author, path, pickaxe);
                        }
                        let active = self.gitview.log_grep.is_some()
                            || self.gitview.log_author.is_some()
                            || self.gitview.log_path.is_some()
                            || self.gitview.log_pickaxe.is_some();
                        if active
                            && ui
                                .small_button("×")
                                .on_hover_text("clear the filter")
                                .clicked()
                        {
                            self.gitview.filter_input.clear();
                            self.gitview.set_log_filter(None, None, None, None);
                        }
                        let dot = self.gitview.attribution_only;
                        if ui
                            .selectable_label(dot, RichText::new(icon::DOT).size(10.0).color(pal().green))
                            .on_hover_text(
                                "only commits that look like this session's work — \
                                 the graph hides while this is on",
                            )
                            .clicked()
                        {
                            self.gitview.attribution_only = !dot;
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
                        RichText::new(format!("{} {branch}", icon::BRANCH))
                            .size(12.0)
                            .strong(),
                    )
                    .on_hover_text(format!("HEAD is {}", refs.head_sha));
                }
                None => {
                    ui.label(
                        RichText::new(format!("{} detached @ {}", icon::BRANCH, refs.head_sha))
                            .size(12.0)
                            .color(pal().amber),
                    )
                    .on_hover_text("HEAD points at a commit, not a branch");
                }
            }
            if let Some(cur) = refs.branches.iter().find(|b| b.current) {
                if let Some(up) = &cur.upstream {
                    let mut track = String::new();
                    if cur.ahead > 0 {
                        track.push_str(&format!(" ⬆{}", cur.ahead));
                    }
                    if cur.behind > 0 {
                        track.push_str(&format!(" ⬇{}", cur.behind));
                    }
                    ui.label(dim(format!("{up}{track}"))).on_hover_text(
                        "commits ahead ⬆ / behind ⬇ the upstream, as of the last fetch",
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

    /// The live sessions sharing this session's worktree. `R-D21`.
    ///
    /// The thing mogeung knows and git cannot: an agent may be reading these
    /// files right now. Git refuses a switch that would *lose* work and has no
    /// opinion at all about work it is merely changing underneath.
    fn live_neighbours(&self, s: &Session) -> Vec<String> {
        let Some(root) = s.repo_root.as_deref() else {
            return Vec::new();
        };
        self.sessions
            .values()
            .filter(|o| o.alive && o.repo_root.as_deref() == Some(root))
            .map(|o| {
                let name = self
                    .prefs
                    .label(&o.id)
                    .map(str::to_string)
                    .unwrap_or_else(|| crate::ui::truncate(&o.id, 8));
                match o.live_status {
                    Some(mogeung_core::session::LiveStatus::Busy) => format!("{name} (working)"),
                    _ => name,
                }
            })
            .collect()
    }

    /// Confirm a branch switch when something is live in the worktree.
    /// `R-D21`.
    ///
    /// It only asks when there is something to say. A switch with nothing live
    /// in the worktree is an ordinary git operation and gets no dialog —
    /// a confirmation that always appears is one that is always dismissed.
    fn git_switch_confirm(&mut self, ctx: &egui::Context, s: &Session) {
        let Some(branch) = self.gitview.confirm_switch.clone() else {
            return;
        };
        let live = self.live_neighbours(s);
        // Nothing live: no question worth asking, so do it.
        if live.is_empty() {
            self.net.send(ClientMsg::GitSwitch {
                session_id: s.id.clone(),
                name: branch,
            });
            self.gitview.confirm_switch = None;
            return;
        }

        let mut open = true;
        let mut go = false;
        egui::Window::new("Switch branch under a running session?")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.label(format!(
                    "Moving this worktree to {branch} while {} session{} {} running in it:",
                    live.len(),
                    if live.len() == 1 { "" } else { "s" },
                    if live.len() == 1 { "is" } else { "are" },
                ));
                ui.add_space(4.0);
                for name in &live {
                    ui.label(RichText::new(name).monospace().size(12.0).color(pal().amber));
                }
                ui.add_space(6.0);
                ui.label(dim(
                    "Git will refuse if the switch would overwrite uncommitted work, and \
                     nothing is destroyed either way. What it cannot see is that an agent \
                     has these files open: after the switch it is reading different \
                     content than it was told about, and it will not notice.",
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.gitview.confirm_switch = None;
                    }
                    if ui
                        .button(RichText::new("Switch anyway").color(pal().amber))
                        .clicked()
                    {
                        go = true;
                    }
                });
            });

        if go {
            self.net.send(ClientMsg::GitSwitch {
                session_id: s.id.clone(),
                name: branch,
            });
            self.gitview.confirm_switch = None;
        } else if !open {
            self.gitview.confirm_switch = None;
        }
    }

    /// Confirm dropping a stash. `R-D21`.
    ///
    /// `git stash drop` prints the dropped commit's sha and it stays reachable
    /// until gc, so this is recoverable — by somebody who knows that and read
    /// the sha as it went past. In practice it is a loss, and it asks.
    fn git_stash_drop_confirm(&mut self, ctx: &egui::Context, s: &Session) {
        let Some(index) = self.gitview.confirm_stash_drop else {
            return;
        };
        let message = self
            .gitview
            .stashes
            .iter()
            .find(|st| st.index == index)
            .map(|st| st.message.clone())
            .unwrap_or_default();

        let mut open = true;
        let mut go = false;
        egui::Window::new("Drop this stash?")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.label(RichText::new(format!("stash@{{{index}}}")).monospace());
                if !message.is_empty() {
                    ui.label(dim(&message));
                }
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Dropping does not restore it. The changes in it are gone from \
                         everywhere you can reach from here.",
                    )
                    .color(pal().red),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        self.gitview.confirm_stash_drop = None;
                    }
                    if ui.button(RichText::new("Drop").color(pal().red)).clicked() {
                        go = true;
                    }
                });
            });

        if go {
            self.net.send(ClientMsg::GitStashDrop {
                session_id: s.id.clone(),
                index,
            });
            self.gitview.confirm_stash_drop = None;
        } else if !open {
            self.gitview.confirm_stash_drop = None;
        }
    }

    /// The collapsible reading lists: branches, tags, stashes, submodules.
    /// Sections with nothing to say do not appear.
    fn git_ref_sections(&mut self, ui: &mut egui::Ui, s: &Session) {
        let now = Utc::now().timestamp();
        let ctx = ui.ctx().clone();
        self.git_switch_confirm(&ctx, s);
        self.git_stash_drop_confirm(&ctx, s);
        if let Some(refs) = self.gitview.refs.clone() {
            if !refs.branches.is_empty() {
                egui::CollapsingHeader::new(
                    RichText::new(format!("BRANCHES ({})", refs.branches.len()))
                        .size(11.0)
                        .color(pal().dim)
                        .strong(),
                )
                .id_salt("git-branches")
                .show(ui, |ui| {
                    // `R-D21`. A field rather than a dialog: a branch name is
                    // one short string and a modal for it would be three
                    // clicks around a text box.
                    match self.gitview.new_branch.as_mut() {
                        None => {
                            if ui
                                .small_button(format!("{} New branch", icon::NEW_SESSION))
                                .clicked()
                            {
                                self.gitview.new_branch = Some(String::new());
                            }
                        }
                        Some(name) => {
                            let mut make = false;
                            let mut cancel = false;
                            ui.horizontal(|ui| {
                                let field = ui.add(
                                    egui::TextEdit::singleline(name)
                                        .hint_text("branch name")
                                        .desired_width(160.0),
                                );
                                field.request_focus();
                                // Enter commits the name — the field is one
                                // short string and reaching for a button is
                                // the slower half of typing it.
                                make = field.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                let ok = !name.trim().is_empty();
                                if ui.add_enabled(ok, egui::Button::new("Create")).clicked() {
                                    make = true;
                                }
                                if ui.button("Cancel").clicked() {
                                    cancel = true;
                                }
                            });
                            let name = name.trim().to_string();
                            if make && !name.is_empty() {
                                // Created *and* switched to: making a branch
                                // you do not then move onto is a rarer want
                                // than the one gesture everybody means.
                                self.net.send(ClientMsg::GitBranchCreate {
                                    session_id: s.id.clone(),
                                    name,
                                    switch_to: true,
                                });
                                self.gitview.new_branch = None;
                            } else if cancel {
                                self.gitview.new_branch = None;
                            }
                        }
                    }
                    // `R-D25`. Beside the numbers it refreshes, because a
                    // key nobody knows about does not fix a stale count.
                    ui.horizontal(|ui| {
                        let busy = self.gitview.fetching;
                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new(if busy {
                                    format!("{} Syncing…", icon::RESCAN)
                                } else {
                                    format!("{} Sync", icon::RESCAN)
                                }),
                            )
                            .on_hover_text(format!(
                                "Fetch from the remotes and update these counts ({}).\n\n\
                                 Never pushes and never merges — mogeung fetches only.",
                                self.keymap.describe(crate::keymap::Action::SyncRemote)
                            ))
                            .clicked()
                        {
                            self.gitview.fetching = true;
                            self.net.send(ClientMsg::GitFetch {
                                session_id: s.id.clone(),
                            });
                        }
                        ui.label(dim(match refs.fetch_epoch {
                            Some(e) => format!("last fetch {}", crate::gitview::age(now, e)),
                            None => "never fetched".to_string(),
                        }));
                    });

                    // `R-D23`. Ahead/behind is measured against a
                    // remote-tracking ref, which only moves when something
                    // fetches — and until ADR-0014 nothing here could. So the
                    // counts are never rendered bare: they carry how old the
                    // measurement is, or say they were never measured at all.
                    // A number that cannot be refreshed and does not admit it
                    // is the confident lie this pane exists not to tell.
                    let staleness = match refs.fetch_epoch {
                        Some(e) => format!("as of {}", crate::gitview::age(now, e)),
                        None => "never fetched — these counts mean nothing yet".to_string(),
                    };
                    for b in &refs.branches {
                        let scoped = self.gitview.log_rev.as_deref() == Some(b.name.as_str());
                        let mut text = format!("{} {}", b.sha, b.name);
                        let tracked = b.upstream.is_some();
                        if b.ahead > 0 {
                            text.push_str(&format!(" ⬆{}", b.ahead));
                        }
                        if b.behind > 0 {
                            text.push_str(&format!(" ⬇{}", b.behind));
                        }
                        // The qualifier goes on the row, not only the hover:
                        // the hover is where you look once you already doubt
                        // the number, and the point is to be doubted in time.
                        if tracked && (b.ahead > 0 || b.behind > 0) {
                            text.push_str(&match refs.fetch_epoch {
                                Some(e) => format!(" ({})", crate::gitview::age(now, e)),
                                None => " (unverified)".to_string(),
                            });
                        }
                        let mut rich = RichText::new(text).monospace();
                        if b.current {
                            rich = rich.color(pal().green);
                        }
                        let row = ui
                            .selectable_label(scoped, rich)
                            .on_hover_text(format!(
                                "{} · {}\n{}{}\nclick to scope the log to this branch — nothing is checked out",
                                crate::gitview::age(now, b.epoch),
                                b.upstream.as_deref().unwrap_or("no upstream"),
                                if tracked { format!("ahead/behind {staleness}\n") } else { String::new() },
                                if b.current { "the checked-out branch\n" } else { "" },
                            ));
                        if row.clicked() {
                            self.gitview.set_log_rev(if scoped {
                                None
                            } else {
                                Some(b.name.clone())
                            });
                        }
                        row.context_menu(|ui| {
                            // `R-D21`. Asks first when something is live in
                            // the worktree; see `git_switch_confirm`.
                            if !b.current && ui.button("Switch to this branch").clicked() {
                                self.gitview.confirm_switch = Some(b.name.clone());
                                ui.close();
                            }
                            if !b.current
                                && ui
                                    .button("Diff vs current, from the merge base")
                                    .on_hover_text(
                                        "what merging this branch would bring — three-dot \
                                         semantics, nothing is checked out",
                                    )
                                    .clicked()
                            {
                                if let Some(sid) = self.gitview.session.clone() {
                                    self.gitview.compare_pending = true;
                                    self.net.send(ClientMsg::GitCompare {
                                        session_id: sid,
                                        branch: b.name.clone(),
                                    });
                                }
                                ui.close();
                            }
                            if ui.button("Copy tip sha").clicked() {
                                ui.ctx().copy_text(b.sha.clone());
                                ui.close();
                            }
                        });
                    }
                });
            }
            // Remote-tracking branches: same gestures, last-fetch truth.
            if !refs.remote_branches.is_empty() {
                egui::CollapsingHeader::new(
                    RichText::new(format!("REMOTE BRANCHES ({})", refs.remote_branches.len()))
                        .size(11.0)
                        .color(pal().dim)
                        .strong(),
                )
                .id_salt("git-remote-branches")
                .show(ui, |ui| {
                    for b in &refs.remote_branches {
                        let scoped = self.gitview.log_rev.as_deref() == Some(b.name.as_str());
                        let row = ui
                            .selectable_label(
                                scoped,
                                RichText::new(format!("{} {}", b.sha, b.name)).monospace(),
                            )
                            .on_hover_text(format!(
                                "{} · as of the last fetch\nclick to scope the log to it",
                                crate::gitview::age(now, b.epoch)
                            ));
                        if row.clicked() {
                            self.gitview.set_log_rev(if scoped {
                                None
                            } else {
                                Some(b.name.clone())
                            });
                        }
                        row.context_menu(|ui| {
                            if ui
                                .button("Diff vs current, from the merge base")
                                .clicked()
                            {
                                if let Some(sid) = self.gitview.session.clone() {
                                    self.gitview.compare_pending = true;
                                    self.net.send(ClientMsg::GitCompare {
                                        session_id: sid,
                                        branch: b.name.clone(),
                                    });
                                }
                                ui.close();
                            }
                        });
                    }
                });
            }
            if !refs.tags.is_empty() {
                egui::CollapsingHeader::new(
                    RichText::new(format!("TAGS ({})", refs.tags.len()))
                        .size(11.0)
                        .color(pal().dim)
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
                    .color(pal().dim)
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
                            "{}\n{}",
                            st.message,
                            crate::gitview::age(now, st.epoch)
                        ));
                    if row.clicked() {
                        self.gitview.selection = crate::gitview::Selection::Stash(st.index);
                    }
                    // `R-D21`. Pop restores and removes in one gesture, which
                    // is git's own pairing; Drop asks, because it is the half
                    // that keeps nothing.
                    row.context_menu(|ui| {
                        if ui
                            .button("Pop — restore it and remove it")
                            .clicked()
                        {
                            self.net.send(ClientMsg::GitStashPop {
                                session_id: s.id.clone(),
                                index: st.index,
                            });
                            ui.close();
                        }
                        if ui
                            .button(RichText::new("Drop — throw it away").color(pal().red))
                            .clicked()
                        {
                            self.gitview.confirm_stash_drop = Some(st.index);
                            ui.close();
                        }
                    });
                }
            });
        }
        if !self.gitview.reflog.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new("REFLOG").size(11.0).color(pal().dim).strong(),
            )
            .id_salt("git-reflog")
            .show(ui, |ui| {
                for e in self.gitview.reflog.clone() {
                    let row = ui
                        .selectable_label(
                            false,
                            RichText::new(format!(
                                "{} {} {}",
                                e.sha,
                                e.selector,
                                truncate(&e.summary, 44)
                            ))
                            .monospace(),
                        )
                        .on_hover_text(format!(
                            "{}\nwhere HEAD has been — click to show this commit",
                            e.summary
                        ));
                    if row.clicked() {
                        self.gitview.selection =
                            crate::gitview::Selection::Commit(e.sha.clone());
                    }
                }
            });
        }
        if !self.gitview.worktrees.is_empty() {
            // Worktrees are mogeung's own device (yolomo makes one per
            // session), so each row names the session living in it.
            let session_of = |path: &str| -> Option<String> {
                self.sessions.values().find_map(|sess| {
                    let root = sess.repo_root.as_deref().unwrap_or(&sess.cwd);
                    (root == path).then(|| {
                        self.prefs
                            .label(&sess.id)
                            .map(str::to_string)
                            .unwrap_or_else(|| sess.label())
                    })
                })
            };
            egui::CollapsingHeader::new(
                RichText::new(format!("WORKTREES ({})", self.gitview.worktrees.len()))
                    .size(11.0)
                    .color(pal().dim)
                    .strong(),
            )
            .id_salt("git-worktrees")
            .show(ui, |ui| {
                for w in self.gitview.worktrees.clone() {
                    ui.horizontal(|ui| {
                        let name = w.path.rsplit('/').next().unwrap_or(&w.path);
                        ui.label(
                            RichText::new(format!(
                                "{} {} ({})",
                                w.sha,
                                name,
                                w.branch.as_deref().unwrap_or("detached"),
                            ))
                            .monospace(),
                        )
                        .on_hover_text(&w.path);
                        if let Some(label) = session_of(&w.path) {
                            ui.label(RichText::new(label).size(10.0).color(pal().green))
                                .on_hover_text("the session running in this worktree");
                        }
                    });
                }
            });
        }
        if !self.gitview.submodules.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("SUBMODULES ({})", self.gitview.submodules.len()))
                    .size(11.0)
                    .color(pal().dim)
                    .strong(),
            )
            .id_salt("git-submodules")
            .show(ui, |ui| {
                for sub in &self.gitview.submodules {
                    let (mark, color, meaning) = match sub.state.as_str() {
                        "+" => ("+", Some(pal().amber), "checked out at a different commit than recorded"),
                        "-" => ("-", Some(pal().dim), "not initialised"),
                        "U" => ("U", Some(pal().red), "merge conflicts"),
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
        // Before the early returns below: a confirmation must not vanish
        // because the list it was raised from went empty underneath it.
        let ctx = ui.ctx().clone();
        self.git_discard_confirm(&ctx, &s.id);

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

        // The commit box, then the write bar. `R-D19`/`R-D20` — the first
        // things in this pane that change the repository rather than
        // describing it.
        self.git_commit_box(ui, s);
        self.git_write_bar(ui, s, &entries);

        for e in entries {
            let picked =
                self.gitview.selection == crate::gitview::Selection::Local(e.path.clone());
            let color = if e.conflicted {
                pal().red
            } else if e.staged && !e.unstaged {
                pal().green
            } else if e.staged {
                pal().amber
            } else if e.state == "??" {
                pal().dim
            } else {
                pal().blue
            };
            let label = if e.conflicted {
                format!("{} {}  ⚠ conflict", e.state, e.path)
            } else {
                format!("{} {}", e.state, e.path)
            };
            let mut ticked = self.gitview.checked.contains(&e.path);
            let mut row = None;
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Checkbox::without_text(&mut ticked))
                    .on_hover_text("pick this file for the buttons above")
                    .changed()
                {
                    if ticked {
                        self.gitview.checked.insert(e.path.clone());
                    } else {
                        self.gitview.checked.remove(&e.path);
                    }
                }
                row = Some(ui.selectable_label(
                    picked,
                    RichText::new(label).monospace().color(color),
                ));
            });
            let row = row
                .expect("the row is drawn on every pass")
                .on_hover_text(if e.conflicted {
                    "unresolved merge conflict — right-click to resolve it"
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
            if e.conflicted {
                row.context_menu(|ui| {
                    if ui
                        .button("Three-way view (base · ours · theirs)")
                        .on_hover_text("the three staged versions, read-only")
                        .clicked()
                    {
                        self.gitview.selection =
                            crate::gitview::Selection::Conflict(e.path.clone());
                        ui.close();
                    }
                    // `R-D22`. Whole-file, which is what the three-way view
                    // above shows; a resolution mixing both sides is editing,
                    // and that stays out of mogeung permanently.
                    ui.separator();
                    let mut resolve = None;
                    if ui
                        .button("Take ours")
                        .on_hover_text("keep this branch's version of the whole file")
                        .clicked()
                    {
                        resolve = Some(mogeung_core::wire::ResolveSide::Ours);
                    }
                    if ui
                        .button("Take theirs")
                        .on_hover_text("keep the incoming version of the whole file")
                        .clicked()
                    {
                        resolve = Some(mogeung_core::wire::ResolveSide::Theirs);
                    }
                    if ui
                        .button("Mark resolved")
                        .on_hover_text(
                            "stage the file exactly as it is on disk — for when you \
                             fixed it in an editor. The content is not checked, so \
                             markers left in it will be committed.",
                        )
                        .clicked()
                    {
                        resolve = Some(mogeung_core::wire::ResolveSide::Mine);
                    }
                    if let Some(side) = resolve {
                        self.net.send(ClientMsg::GitResolve {
                            session_id: s.id.clone(),
                            path: e.path.clone(),
                            side,
                        });
                        ui.close();
                    }
                });
            }
        }
    }

    /// Write and make a commit. `R-D20`.
    ///
    /// Above the file list, IntelliJ's layout, which is already this pane's
    /// reference from `R-D18`. Below it would put the message further from the
    /// staged files it describes the more of them there are.
    fn git_commit_box(&mut self, ui: &mut egui::Ui, s: &Session) {
        let staged = self
            .gitview
            .status
            .iter()
            .filter(|e| e.staged && e.state != "!!")
            .count();
        let conflicts = self.gitview.status.iter().filter(|e| e.conflicted).count();

        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.add(
                egui::TextEdit::multiline(&mut self.gitview.commit_msg)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("Commit message"),
            );

            ui.horizontal(|ui| {
                ui.checkbox(&mut self.gitview.commit_amend, "Amend")
                    .on_hover_text(
                        "Replace the last commit instead of adding one. \
                         Rewrites history — safe until it has been pushed.",
                    );
                ui.checkbox(&mut self.gitview.commit_trailer, "Record session")
                    .on_hover_text(
                        "Add a Mogeung-Session trailer naming the session this work \
                         came from, so a line can later be traced back to the prompt \
                         that produced it.\n\nIt becomes part of the commit message: \
                         permanent, and visible to anyone who reads the commit. It is \
                         an id and carries no prompt text.",
                    );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Amend needs no staged files — its whole use is fixing a
                    // message — so the two conditions differ.
                    let has_message = !self.gitview.commit_msg.trim().is_empty();
                    let ready = has_message && (staged > 0 || self.gitview.commit_amend);
                    if ui
                        .add_enabled(ready, egui::Button::new("Commit"))
                        .on_disabled_hover_text(if !has_message {
                            "a commit needs a message"
                        } else {
                            "nothing is staged — tick the files to include"
                        })
                        .clicked()
                    {
                        self.net.send(ClientMsg::GitCommit {
                            session_id: s.id.clone(),
                            message: self.gitview.commit_msg.clone(),
                            amend: self.gitview.commit_amend,
                            session_trailer: self.gitview.commit_trailer,
                        });
                        // Cleared optimistically, and that is a deliberate
                        // trade: a refused commit costs a retyped message,
                        // while a message left in the box after a successful
                        // one gets committed twice by the next click. The
                        // daemon's refusal arrives in the error bar with git's
                        // own words, so the failure is never silent.
                        self.gitview.commit_msg.clear();
                        self.gitview.commit_amend = false;
                        // The log on screen no longer has the tip it was drawn
                        // from. Asking again is cheaper than a protocol message
                        // that would exist for this one caller.
                        self.gitview.commits.clear();
                        self.gitview.log_pending = false;
                    }
                    ui.label(dim(match staged {
                        0 => "nothing staged".to_string(),
                        1 => "1 file staged".to_string(),
                        n => format!("{n} files staged"),
                    }));
                });
            });

            // Committing over an unresolved conflict is legal git and almost
            // never intended: it records the markers as content.
            if conflicts > 0 {
                ui.label(
                    RichText::new(format!(
                        "{} unresolved conflict{} — committing now records the \
                         markers as code",
                        conflicts,
                        if conflicts == 1 { "" } else { "s" }
                    ))
                    .color(pal().amber)
                    .size(12.0),
                );
            }
        });
        ui.add_space(4.0);
    }

    /// What the fetch did, said out loud. `R-D25`.
    ///
    /// ADR-0014 makes reporting non-separable from the verb: a sync that
    /// succeeds silently cannot be told from one that silently did nothing,
    /// and being confidently wrong about how current you are is the whole
    /// problem the fetch was admitted to solve.
    ///
    /// It dismisses itself when there is nothing to act on and waits when
    /// there is. "Up to date" is worth a glance and never worth a click;
    /// "6 behind" is a thing you have to do something about, and something you
    /// have to do something about should not evaporate while you read it.
    fn git_fetch_popup(&mut self, ctx: &egui::Context) {
        let Some(f) = &self.gitview.fetched else {
            return;
        };
        const LINGER: std::time::Duration = std::time::Duration::from_secs(7);
        if !f.needs_a_merge() && f.at.elapsed() > LINGER {
            self.gitview.fetched = None;
            return;
        }
        // Repaint as the deadline passes, or an idle window sits on a popup
        // that expired a minute ago.
        if !f.needs_a_merge() {
            ctx.request_repaint_after(LINGER.saturating_sub(f.at.elapsed()));
        }

        let headline = f.headline();
        let behind = f.behind;
        let updates = f.updates.clone();
        let mut dismiss = false;

        egui::Window::new("Sync")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            // Bottom right: it is a report, not a question, and it must not
            // land over the list it is reporting on.
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(icon::DOT)
                            .color(if behind > 0 { pal().amber } else { pal().green }),
                    );
                    ui.label(RichText::new(&headline).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::HIDE).on_hover_text("dismiss").clicked() {
                            dismiss = true;
                        }
                    });
                });

                if updates.is_empty() {
                    // Worth saying rather than leaving blank: "no output" and
                    // "did not run" look identical when both are empty.
                    ui.label(dim("No refs moved — the remotes had nothing new."));
                } else {
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .show(ui, |ui| {
                            for line in &updates {
                                ui.label(mono(line).size(11.0));
                            }
                        });
                }

                if behind > 0 {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Merging stays in a terminal — mogeung fetches and never pulls.",
                        )
                        .color(pal().dim)
                        .size(11.0),
                    );
                }
            });

        if dismiss {
            self.gitview.fetched = None;
        }
    }

    /// Stage, unstage and discard the ticked files. `R-D19`.
    ///
    /// The concrete moment this exists for: you finish reading twelve changed
    /// files, decide eight are the commit and four are debris, and then retype
    /// from memory in a terminal the list you were just looking at.
    ///
    /// Discard is the only verb here with no undo, so it is the only one that
    /// asks — and it is drawn apart from the other two, because a button that
    /// destroys work should not sit where a hand lands by momentum.
    fn git_write_bar(
        &mut self,
        ui: &mut egui::Ui,
        s: &Session,
        entries: &[mogeung_core::wire::StatusEntry],
    ) {
        let picked = self.gitview.checked_paths();
        let any = !picked.is_empty();

        ui.horizontal(|ui| {
            // Tick-all, over the rows actually listed — the session filter and
            // the `!!` drop mean "everything" is a smaller set than the status.
            let all: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
            let every = !all.is_empty() && all.iter().all(|p| self.gitview.checked.contains(p));
            let mut want = every;
            if ui
                .add(egui::Checkbox::without_text(&mut want))
                .on_hover_text(if every {
                    "clear the selection"
                } else {
                    "pick every file listed"
                })
                .changed()
            {
                match want {
                    true => self.gitview.checked.extend(all),
                    false => self.gitview.checked.clear(),
                }
            }

            ui.add_enabled_ui(any, |ui| {
                if ui.button("Stage").clicked() {
                    self.net.send(ClientMsg::GitStage {
                        session_id: s.id.clone(),
                        paths: picked.clone(),
                    });
                }
                if ui.button("Unstage").clicked() {
                    self.net.send(ClientMsg::GitUnstage {
                        session_id: s.id.clone(),
                        paths: picked.clone(),
                    });
                }
            });

            // Not limited to the ticked files, and said so on the hover:
            // `git stash` takes the whole worktree, and a button beside three
            // that respect the ticks would otherwise read as a fourth that
            // does. `R-D21`.
            if ui
                .button("Stash all")
                .on_hover_text(
                    "Shelve every change in the worktree, tracked and untracked, and                      leave it clean. Not just the ticked files — git stash takes the                      lot. Restore it from the STASHES list below.",
                )
                .clicked()
            {
                self.net.send(ClientMsg::GitStashPush {
                    session_id: s.id.clone(),
                    message: String::new(),
                    // Untracked included: an agent's new files are exactly the
                    // ones you meant to get out of the way, and a "clean"
                    // worktree still full of them is a surprise.
                    include_untracked: true,
                });
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(any, |ui| {
                    if ui
                        .button(RichText::new("Discard").color(pal().red))
                        .on_hover_text("throw these changes away — asks first")
                        .clicked()
                    {
                        self.gitview.confirm_discard = Some(picked.clone());
                    }
                });
                if any {
                    ui.label(dim(format!("{} picked", picked.len())));
                }
            });
        });
        ui.add_space(2.0);
    }

    /// The confirmation Discard needs, naming every file. `R-D19`.
    ///
    /// It counts the untracked ones separately and says out loud that git has
    /// no copy of them, because that is the difference between an action that
    /// is annoying to undo and one that cannot be undone at all — and the row
    /// in the list looks identical either way.
    fn git_discard_confirm(&mut self, ctx: &egui::Context, session_id: &str) {
        let Some(paths) = self.gitview.confirm_discard.clone() else {
            return;
        };
        // Untracked as the daemon last reported it. `??` is the porcelain's
        // "git has never seen this file".
        let untracked: Vec<String> = paths
            .iter()
            .filter(|p| {
                self.gitview
                    .status
                    .iter()
                    .any(|e| &e.path == *p && e.state.starts_with("??"))
            })
            .cloned()
            .collect();
        let n = untracked.len();
        let (is_are, it_them) = if n == 1 { ("is", "it") } else { ("are", "them") };

        let mut open = true;
        let mut go = false;
        egui::Window::new("Discard changes?")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label(format!(
                    "Throw away local changes to {} file{}:",
                    paths.len(),
                    if paths.len() == 1 { "" } else { "s" }
                ));
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for p in &paths {
                            let lost = untracked.contains(p);
                            ui.label(
                                RichText::new(p)
                                    .monospace()
                                    .size(12.0)
                                    .color(if lost { pal().red } else { pal().text }),
                            );
                        }
                    });
                ui.add_space(6.0);
                if n > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{n} of these {is_are} untracked. Git has never seen {it_them}, \
                             so it cannot bring {it_them} back — deleting {it_them} is permanent."
                        ))
                        .color(pal().red),
                    );
                } else {
                    ui.label(dim(
                        "All tracked, so git restores them from the last commit.",
                    ));
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    // Cancel first: the safe answer should be where a hand
                    // lands, and the destructive one should take an aim.
                    if ui.button("Cancel").clicked() {
                        self.gitview.confirm_discard = None;
                    }
                    if ui
                        .button(RichText::new("Discard").color(pal().red))
                        .clicked()
                    {
                        go = true;
                    }
                });
            });

        if go {
            self.net.send(ClientMsg::GitDiscard {
                session_id: session_id.to_string(),
                paths,
            });
            self.gitview.confirm_discard = None;
        } else if !open {
            // Closing the window is a refusal, like every other dialog here.
            self.gitview.confirm_discard = None;
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
        // The attribution filter is display-side: it narrows loaded pages,
        // and the graph hides with it — the topology of a subset would lie.
        let attribution_only = self.gitview.attribution_only;
        let graph = if attribution_only {
            Vec::new()
        } else {
            self.gitview.graph.clone()
        };
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
            if attribution_only && !c.touches_session {
                continue;
            }
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
                    let color_of = |lane: usize| graph_colors()[lane % graph_colors().len()];
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
                            "\n⚫ probably this session's work (files + timing match)"
                        } else {
                            ""
                        }
                    ));
                if c.touches_session {
                    ui.label(RichText::new(icon::DOT).size(9.0).color(pal().green)).on_hover_text(
                        "probably this session's work — its files and timing match. A hint, not a fact",
                    );
                }
                for r in c.refs.iter().take(3) {
                    let color = if r.starts_with("tag: ") { pal().amber } else { pal().blue };
                    ui.label(RichText::new(r).size(10.0).color(color));
                }
                // R-D18: IntelliJ's author and date columns, adapted to a
                // narrow pane — dimmed, truncated, after the decorations.
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        truncate(&c.author, 12),
                        crate::gitview::age(now, c.epoch)
                    ))
                    .size(10.0)
                    .color(pal().dim),
                );
                // R-D17: once this commit's diff has been fetched, we know
                // whether a human has read its hunks — say so, quietly.
                if let Some(files) = self.gitview.commit_diffs.get(&c.sha) {
                    let total: usize = files.iter().map(|f| f.hunks.len()).sum();
                    let read: usize = files
                        .iter()
                        .flat_map(|f| &f.hunks)
                        .filter(|h| h.reviewed)
                        .count();
                    if total > 0 && read == total {
                        ui.label(RichText::new("✔read").size(9.0).color(pal().dim))
                            .on_hover_text("every hunk of this commit has been marked read");
                    } else if read > 0 {
                        ui.label(RichText::new(format!("{read}/{total}")).size(9.0).color(pal().dim))
                            .on_hover_text("hunks of this commit marked read");
                    }
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
                    // Enabled once the diff is cached — which it is for any
                    // commit being looked at, and that is the one you copy.
                    if let Some(files) = self.gitview.commit_diffs.get(&c.sha) {
                        if ui
                            .button("Copy as patch")
                            .on_hover_text("unified diff text — paste into a terminal or a prompt")
                            .clicked()
                        {
                            ui.ctx().copy_text(crate::gitview::patch_text(files));
                            ui.close();
                        }
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
                    // R-F9: from a commit to the conversation that (probably)
                    // produced it — the session's transcript at that moment.
                    if ui
                        .button("Open transcript here")
                        .on_hover_text(
                            "the selected session's transcript, scrolled to this commit's time",
                        )
                        .clicked()
                    {
                        self.focus_event_ts = chrono::DateTime::from_timestamp(c.epoch, 0);
                        self.set_tab(Tab::Transcript);
                        ui.close();
                    }
                    ui.separator();
                    match self.gitview.range_from.clone() {
                        Some(marked) if marked != c.sha => {
                            if ui
                                .button("Diff against marked commit")
                                .on_hover_text("older to newer, decided by commit time")
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
                    pickaxe: self.gitview.log_pickaxe.clone(),
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
            crate::gitview::Selection::Conflict(path) => {
                // Not a diff at all: three staged versions side by side.
                self.git_conflict_panel(ui, &path.clone());
                return;
            }
        };
        // Cloned before the toolbar below, whose option changes mutate the
        // very caches `files` borrows from.
        let files: Option<Vec<mogeung_core::change::FileChange>> = files.cloned();
        // The reading controls (`R-D14`): context width, whitespace,
        // side-by-side — any change invalidates every cached diff, which
        // the fetch door then re-requests the current selection through.
        ui.horizontal(|ui| {
            let step = self.gitview.context_step;
            let ws = self.gitview.diff_ignore_ws;
            if ui
                .small_button(format!("±{}", self.gitview.context()))
                .on_hover_text("lines of context around each hunk — click to widen, wraps around")
                .clicked()
            {
                self.gitview.set_diff_opts(step + 1, ws);
            }
            if ui
                .selectable_label(ws, "w")
                .on_hover_text("ignore whitespace-only changes")
                .clicked()
            {
                self.gitview.set_diff_opts(step, !ws);
            }
            if ui
                .selectable_label(self.prefs.side_by_side, icon::SIDE_BY_SIDE)
                .on_hover_text("side-by-side (shared with the Changes tab)")
                .clicked()
            {
                self.prefs.side_by_side = !self.prefs.side_by_side;
                self.prefs_dirty = true;
            }
            if let Some(files) = files.as_ref() {
                if !files.is_empty()
                    && ui
                        .small_button(format!("{} patch", icon::COPY))
                        .on_hover_text(
                            "copy this whole diff as unified patch text — \
                             paste into a terminal or a prompt",
                        )
                        .clicked()
                {
                    ui.ctx().copy_text(crate::gitview::patch_text(files));
                }
            }
            ui.label(dim("n/p walk the hunks"));
        });
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
        let (syntax, words) = (self.prefs.syntax, self.prefs.word_diff);
        let mut open_at_rev: Option<(String, String)> = None;
        let mut select_commit: Option<String> = None;
        // The commit header (`R-D12`) — rendered in the sidebar (`R-D18`)
        // so it stays on screen while the hunks scroll.
        let detail = match &self.gitview.selection {
            crate::gitview::Selection::Commit(sha) => self
                .gitview
                .commit_details
                .get(sha)
                .cloned()
                .map(|d| (sha.clone(), d)),
            _ => None,
        };
        // R-D18: the file the pane is narrowed to, owned by the selection —
        // a file picked on one commit narrows nothing on the next.
        let selection = self.gitview.selection.clone();
        let mut focus_idx = self
            .gitview
            .focus_for(&selection)
            .and_then(|p| files.iter().position(|f| f.path == p));
        // `n`/`p` walk hunks (`R-D13`): pointer over the panel, no text
        // field focused — the same gate the pane zoom uses. With a file
        // focused they cross into the neighbouring file at the edges
        // (`R-D18`), the way IntelliJ's diff walk does.
        let counts: Vec<usize> = files.iter().map(|f| f.hunks.len()).collect();
        let total_all: usize = counts.iter().sum();
        if total_all > 0
            && ui.rect_contains_pointer(ui.max_rect())
            && ui.ctx().memory(|m| m.focused().is_none())
        {
            let (next, prev) = ui.input(|i| {
                (i.key_pressed(egui::Key::N), i.key_pressed(egui::Key::P))
            });
            if next || prev {
                match focus_idx {
                    Some(fi) => {
                        let cur =
                            self.gitview.hunk_cursor.min(counts[fi].saturating_sub(1));
                        if let Some((nf, nh)) =
                            crate::gitview::step_hunk(&counts, fi, cur, next)
                        {
                            if nf != fi {
                                self.gitview.focus_file = Some(files[nf].path.clone());
                                focus_idx = Some(nf);
                            }
                            self.gitview.hunk_cursor = nh;
                            self.gitview.hunk_goto = Some(nh);
                        }
                    }
                    None => {
                        let cur = self.gitview.hunk_cursor.min(total_all - 1);
                        self.gitview.hunk_cursor = if next {
                            (cur + 1) % total_all
                        } else {
                            (cur + total_all - 1) % total_all
                        };
                        self.gitview.hunk_goto = Some(self.gitview.hunk_cursor);
                    }
                }
            }
        }
        let hunk_goto = self.gitview.hunk_goto.take();
        let hunk_cursor = self.gitview.hunk_cursor;
        // R-D18: the IntelliJ shape — a sidebar with the commit's changed
        // files as a selectable tree and its details beneath, the remaining
        // width showing the focused file's diff (or every file, from the
        // tree's root row). The details live outside the diff scroll so a
        // long agent-written body cannot pin the hunks off screen.
        let sidebar = detail.is_some() || files.len() >= 2;
        // `Some(None)` is the root row: back to the whole diff.
        let mut tree_pick: Option<Option<usize>> = None;
        if sidebar {
            egui::Panel::left("git-commit-sidebar")
                .resizable(true)
                .default_size(230.0)
                .show(ui, |ui| {
                    egui::ScrollArea::both()
                        .id_salt(("git-sidebar", &title))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if !files.is_empty() {
                                if ui
                                    .selectable_label(
                                        focus_idx.is_none(),
                                        RichText::new(format!(
                                            "{} files  +{} −{}",
                                            files.len(),
                                            files.iter().map(|f| f.insertions).sum::<u32>(),
                                            files.iter().map(|f| f.deletions).sum::<u32>(),
                                        ))
                                        .size(11.5),
                                    )
                                    .on_hover_text("every file's diff in one scroll")
                                    .clicked()
                                {
                                    tree_pick = Some(None);
                                }
                                let tree = crate::gitview::file_tree(&files);
                                render_file_tree(ui, &tree, &files, focus_idx, "", &mut tree_pick);
                            }
                            if let Some((sha, d)) = &detail {
                                ui.separator();
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
                                    // R-D17: what a human has already read of
                                    // this commit — the anchors travel
                                    // content-hashed, so a mark made in the
                                    // Changes tab lands here too.
                                    let total: usize =
                                        files.iter().map(|f| f.hunks.len()).sum();
                                    let read: usize = files
                                        .iter()
                                        .flat_map(|f| &f.hunks)
                                        .filter(|h| h.reviewed)
                                        .count();
                                    if read > 0 {
                                        ui.label(
                                            RichText::new(format!(
                                                "· {read} of {total} hunks read"
                                            ))
                                            .size(11.0)
                                            .color(pal().green),
                                        );
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(dim(format!(
                                        "{} ·",
                                        sha.chars().take(10).collect::<String>()
                                    )));
                                    for p in &d.parents {
                                        if ui
                                            .link(
                                                RichText::new(p)
                                                    .monospace()
                                                    .size(11.5)
                                                    .color(pal().blue),
                                            )
                                            .on_hover_text("show this parent commit")
                                            .clicked()
                                        {
                                            select_commit = Some(p.clone());
                                        }
                                    }
                                    for r in &d.refs {
                                        let color =
                                            if r.starts_with("tag: ") { pal().amber } else { pal().blue };
                                        ui.label(RichText::new(r).size(10.0).color(color));
                                    }
                                });
                                // R-D18: "has this reached main?" — the one
                                // question the header never answered.
                                if !d.branches.is_empty() {
                                    let named: Vec<&str> =
                                        d.branches.iter().take(3).map(String::as_str).collect();
                                    let text = if d.branches.len() <= 3 {
                                        format!("in: {}", named.join(", "))
                                    } else {
                                        format!(
                                            "in {} branches: {}, …",
                                            d.branches.len(),
                                            named.join(", ")
                                        )
                                    };
                                    ui.label(RichText::new(text).size(10.0).color(pal().blue))
                                        .on_hover_text(d.branches.join("\n"));
                                }
                            }
                        });
                });
        }
        if let Some(pick) = tree_pick {
            self.gitview.focus_file = pick.map(|i| files[i].path.clone());
            self.gitview.hunk_cursor = 0;
            focus_idx = pick;
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
        egui::ScrollArea::both()
            // The focus is part of the scroll identity: each file starts at
            // its own top instead of inheriting the previous file's offset.
            .id_salt(("git-diff-scroll", &title, focus_idx))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if files.is_empty() {
                    ui.label(dim("no textual diff — empty, binary, or a merge with no changes"));
                    return;
                }
                let mut hunk_idx = 0usize;
                for f in files
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| focus_idx.map_or(true, |fi| fi == *i))
                    .map(|(_, f)| f)
                {
                    ui.horizontal(|ui| {
                        ui.label(mono(&f.path));
                        ui.label(dim(format!("+{} −{}", f.insertions, f.deletions)));
                        for fl in &f.flags {
                            ui.label(dim(fl.label()));
                        }
                        if f.truncated {
                            ui.label(RichText::new("binary or too large").size(11.0).color(pal().amber));
                        }
                        if !f.hunks.is_empty()
                            && ui
                                .small_button(icon::COPY)
                                .on_hover_text("copy this file's diff as patch text")
                                .clicked()
                        {
                            ui.ctx()
                                .copy_text(crate::gitview::patch_text(std::slice::from_ref(f)));
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
                        let current = hunk_idx == hunk_cursor && total_all > 0;
                        let mut header = dim(&h.header);
                        if current {
                            header = RichText::new(&h.header).size(12.0).color(pal().blue);
                        }
                        let hr = ui.horizontal(|ui| {
                            ui.label(header);
                            // R-D17: a hunk a human has read says so.
                            if h.reviewed {
                                ui.label(RichText::new(icon::READ).size(10.0).color(pal().green))
                                    .on_hover_text("marked read (R-D8's marks, by content)");
                            }
                        });
                        if hunk_goto == Some(hunk_idx) {
                            hr.response.scroll_to_me(Some(egui::Align::Center));
                        }
                        hunk_idx += 1;
                        if self.prefs.side_by_side {
                            render_side_by_side(ui, &h.lines, syntax, words);
                        } else {
                            render_unified(ui, &h.lines, syntax, words);
                        }
                        ui.add_space(4.0);
                    }
                    ui.separator();
                }
            });
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

    /// A conflicted file's three stages, read-only, side by side. `R-D16`.
    /// No diffing between them (yet, deliberately): three readable columns
    /// beat a clever view that hides one of the three truths.
    fn git_conflict_panel(&mut self, ui: &mut egui::Ui, path: &str) {
        ui.horizontal(|ui| {
            ui.label(mono(path));
            ui.label(
                RichText::new("merge conflict — base · ours · theirs")
                    .size(11.0)
                    .color(pal().red),
            );
        });
        let Some((base, ours, theirs, truncated)) = self.gitview.conflicts.get(path).cloned()
        else {
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(dim("loading the three stages…"));
            });
            return;
        };
        if truncated {
            ui.label(
                RichText::new("cut short — a stage went on past the size cap")
                    .size(11.0)
                    .color(pal().amber),
            );
        }
        ui.separator();
        egui::ScrollArea::both()
            .id_salt(("git-conflict-scroll", path))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.columns(3, |cols| {
                    for (col, (name, body)) in cols.iter_mut().zip([
                        ("base — the common ancestor", &base),
                        ("ours — the current branch", &ours),
                        ("theirs — the branch being merged", &theirs),
                    ]) {
                        col.label(RichText::new(name).size(11.0).color(pal().dim).strong());
                        if body.is_empty() {
                            col.label(dim("(this side has no version — added or deleted)"));
                        } else {
                            col.add(
                                egui::Label::new(
                                    RichText::new(body).monospace().size(11.5),
                                )
                                .selectable(true),
                            );
                        }
                    }
                });
            });
    }

    /// The session's worktree: tabs and a read-only viewer.
    /// `R-B24`, workbench behaviour by `R-B25`.
    ///
    /// **No tree.** It moved to the rail on 2026-08-03 (`R-B41`,
    /// [ADR-0017](../../../docs/decisions/0017-the-rail-is-chrome.md)) so it
    /// could be read with any tab forward. What is left here is the half that
    /// is genuinely about the file you are reading.
    ///
    /// Everything shown here came over the wire — the UI never touches the
    /// worktree itself ([ADR-0001]), and nothing in this pane can write.
    fn explorer_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        // No pane-wide `scale_text` here: `editor_group` takes the "editor"
        // zoom for both sides of the split. `R-B39`.
        self.explorer.ensure_session(&s.id);
        self.explorer_fetch(s);

        // Side by side when any tab lives on the right; the split is created
        // by sending a tab over ("open on the other side") and collapses
        // when the last right-hand tab leaves.
        if self.explorer.current().split() {
            let half = ui.available_width() * 0.5;
            egui::Panel::right("editor-split")
                .default_size(half)
                .resizable(true)
                .show(ui, |ui| {
                    self.editor_group(ui, 1);
                });
        }
        self.editor_group(ui, 0);
    }

    /// Ask the daemon for whatever the explorer state wants and lacks: the
    /// root, every expanded directory without a listing, the active tab
    /// without a body.
    ///
    /// One door for all fetching, which is what makes restore from disk,
    /// reveal, refresh and a plain click all re-fetch the same way — and it
    /// lives in the paint rather than `set_tab`, so a pane that is *docked*
    /// visible works without ever having been switched to.
    ///
    /// Called by both the Editor tab and the rail's Files tool since `R-B41`
    /// split them apart, which is why every branch guards on `pending` rather
    /// than on who is asking: with both on screen this runs twice in a frame
    /// and must still send once.
    fn explorer_fetch(&mut self, s: &Session) {
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

    }

    /// One side of the editor: its tab strip and its viewer.
    ///
    /// Both sides share the `"editor"` zoom: a split is one document read two
    /// ways, and two halves of the same file at different sizes is a bug, not
    /// a feature. The tree — now in the rail — is the thing that zooms apart
    /// from them.
    fn editor_group(&mut self, ui: &mut egui::Ui, group: u8) {
        // `available_rect_before_wrap`, not `max_rect` — see [`Self::zoom_over`].
        // Group 0 is drawn into the pane ui *after* the split panel has taken
        // its slice, and that ui's `max_rect` is still the whole pane.
        let z = self.zoom_over(ui, "editor", ui.available_rect_before_wrap());
        scale_text(ui, z);
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
                            .color(pal().amber),
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
                            .small_button(RichText::new("×").size(10.0).color(pal().dim))
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
            // Says where the tree went. It was in this pane until `R-B41`
            // moved it to the rail, and "pick a file" is unhelpful advice when
            // the thing you pick from is no longer on screen.
            let files = self.keymap.describe(crate::keymap::Action::RailFiles);
            let goto = self.keymap.describe(crate::keymap::Action::GoToFile);
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                ui.label(dim("nothing open — read-only, always"));
                ui.add_space(4.0);
                ui.label(dim(format!("{files} for the worktree · {goto} to open by name")));
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
        let language = crate::explorer::language_of(&path).to_string();
        let line_count = content.lines().count().max(1);

        // R-B29: image files preview instead of rendering as bytes-as-text.
        // The bytes are read from this machine's disk — the worktree is local
        // in every supported layout except a remote daemon, which refuses.
        if matches!(language.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
            && rev.is_none()
        {
            ui.add(egui::Label::new(mono(&path)).truncate());
            if self.remote_daemon() {
                ui.label(dim("remote daemon — the image lives on the other machine"));
                return;
            }
            let root = self
                .explorer
                .session
                .as_ref()
                .and_then(|sid| self.sessions.get(sid))
                .map(|s| s.repo_root.clone().unwrap_or_else(|| s.cwd.clone()));
            if let Some(root) = root {
                let full = std::path::Path::new(&root).join(&path);
                match std::fs::read(&full) {
                    Ok(bytes) => {
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.add(
                                egui::Image::from_bytes(format!("bytes://{path}"), bytes)
                                    .max_width(ui.available_width()),
                            );
                        });
                    }
                    Err(e) => {
                        ui.label(dim(format!("could not read the image: {e}")));
                    }
                }
            }
            return;
        }

        ui.horizontal(|ui| {
            // Truncated, never wrapped: a header that folds onto two lines
            // pushes the file down and reads as two files.
            let header = match &rev {
                Some(r) => format!("{path} @ {r}"),
                None => path.clone(),
            };
            ui.add(egui::Label::new(mono(&header)).truncate())
                .on_hover_text(&header);
            // R-B29: file facts where the eye already is.
            ui.label(dim(format!(
                "{} · {} line(s) · {}",
                mogeung_core::health::human_bytes(content.len() as u64),
                line_count,
                if language.is_empty() { "text" } else { &language }
            )));
            if let Some(r) = &rev {
                ui.label(
                    RichText::new("history — the file as of this revision")
                        .size(11.0)
                        .color(pal().amber),
                )
                .on_hover_text(format!(
                    "git show {r}:{path} — nothing here can edit the past (or the present)"
                ));
            }
            if truncated {
                ui.label(
                    RichText::new("cut short — the file goes on past the size cap")
                        .size(11.0)
                        .color(pal().amber),
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
                // R-B28: the symbol outline.
                if ui
                    .selectable_label(self.outline_open, "outline")
                    .on_hover_text("symbols in this file — click to jump, type to filter")
                    .clicked()
                {
                    self.outline_open = !self.outline_open;
                }
                // R-B29: wrap is a property of the file, so it persists per path.
                let wrapped = self.prefs.scoped.editor_wrap.contains(&path);
                if ui
                    .selectable_label(wrapped, "wrap")
                    .on_hover_text("wrap long lines — remembered for this file")
                    .clicked()
                {
                    if wrapped {
                        self.prefs.scoped.editor_wrap.remove(&path);
                    } else {
                        self.prefs.scoped.editor_wrap.insert(path.clone());
                    }
                    self.prefs_dirty = true;
                }
                // R-B29: markdown renders as prose on demand.
                if language == "md" {
                    let on = self.md_preview.contains(&path);
                    if ui
                        .selectable_label(on, "preview")
                        .on_hover_text("render the markdown instead of showing its source")
                        .clicked()
                    {
                        if on {
                            self.md_preview.remove(&path);
                        } else {
                            self.md_preview.insert(path.clone());
                        }
                    }
                }
                if ui
                    .small_button("copy path")
                    .on_hover_text("the path, relative to the session root")
                    .clicked()
                {
                    ui.ctx().copy_text(path.clone());
                }
                // R-B27: this file beside its HEAD version — revision tabs and
                // the split already existed; this is the one-click pairing.
                if rev.is_none() {
                    if ui
                        .small_button("vs HEAD")
                        .on_hover_text("open the committed version beside this one")
                        .clicked()
                    {
                        self.explorer.open_file_at_rev(&path, "HEAD");
                        if let Some(i) = {
                            let st = self.explorer.current();
                            st.active_of(st.focus)
                        } {
                            self.explorer.move_tab_to_other_side(i);
                        }
                    }
                }
                // R-B29: the jump list.
                let marks: Vec<(String, String, u64)> = self
                    .prefs
                    .scoped
                    .bookmarks
                    .iter()
                    .filter(|(sid, _, _)| Some(sid) == self.explorer.session.as_ref())
                    .cloned()
                    .collect();
                if !marks.is_empty() {
                    ui.menu_button(format!("marks ({})", marks.len()), |ui| {
                        let mut open: Option<(String, u64)> = None;
                        for (_, p, l) in &marks {
                            if ui.button(mono(format!("{p}:{l}")).size(11.0)).clicked() {
                                open = Some((p.clone(), *l));
                                ui.close();
                            }
                        }
                        if let Some((p, l)) = open {
                            self.explorer.open_file(&p, false, Some(l));
                        }
                    });
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
                    self.gitview.set_log_filter(None, None, Some(path.clone()), None);
                    self.set_tab(Tab::Git);
                }
            });
        });

        // R-B29: markdown preview replaces the source entirely — a preview
        // beside the source is two panes' worth of one file.
        if language == "md" && self.md_preview.contains(&path) {
            egui::ScrollArea::vertical()
                .id_salt(("md-preview", &path, group))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui_commonmark::CommonMarkViewer::new()
                        .show(ui, &mut self.md_cache, &content);
                });
            return;
        }

        // R-B28: the outline, in a side panel so the code keeps its scroll.
        if self.outline_open {
            let symbols = crate::symbols::outline(&content, &language);
            let mut goto_sym: Option<u64> = None;
            egui::Panel::right(egui::Id::new(("outline", group)))
                .frame(egui::Frame::NONE.fill(pal().bg_panel).inner_margin(6))
                .default_size(220.0)
                .resizable(true)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.outline_filter)
                            .hint_text("go to symbol…")
                            .desired_width(f32::INFINITY),
                    );
                    if symbols.is_empty() {
                        ui.label(dim("no symbols recognised in this file"));
                    }
                    let filter = self.outline_filter.to_lowercase();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for sym in &symbols {
                            if !filter.is_empty()
                                && !sym.name.to_lowercase().contains(&filter)
                            {
                                continue;
                            }
                            let indent = "  ".repeat(sym.depth as usize);
                            let glyph = match sym.kind {
                                crate::symbols::SymbolKind::Function
                                | crate::symbols::SymbolKind::Method => "ƒ",
                                crate::symbols::SymbolKind::Type => "T",
                                crate::symbols::SymbolKind::Const => "c",
                                crate::symbols::SymbolKind::Module => "m",
                                crate::symbols::SymbolKind::Heading => "#",
                                crate::symbols::SymbolKind::Other => "·",
                            };
                            if ui
                                .selectable_label(
                                    false,
                                    mono(format!("{indent}{glyph} {}", sym.name)).size(11.0),
                                )
                                .clicked()
                            {
                                goto_sym = Some(sym.line as u64);
                            }
                        }
                    });
                });
            if goto_sym.is_some() {
                goto_line = goto_sym;
            }
        }

        // R-B28: go-to-line. Ctrl+G on the focused side.
        if focused && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::G)) {
            self.show_goto = !self.show_goto;
            self.goto_input.clear();
        }
        if self.show_goto {
            ui.horizontal(|ui| {
                ui.label(dim("go to line"));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.goto_input).desired_width(80.0),
                );
                resp.request_focus();
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.show_goto = false;
                } else if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Ok(n) = self.goto_input.trim().parse::<u64>() {
                        goto_line = Some(n.clamp(1, line_count as u64));
                    }
                    self.show_goto = false;
                }
            });
        }

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
                if ui.small_button("‹").on_hover_text("previous match (Shift+↩)").clicked() {
                    jump = Some(-1);
                }
                if ui.small_button("›").on_hover_text("next match (↩)").clicked() {
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

        let theme =
            egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
        // The same memoised highlight `code_view_ui` uses — unrolled here so
        // the gutter and the code can sit side by side on identical rows.
        let mut job = egui_extras::syntax_highlighting::highlight(
            ui.ctx(),
            ui.style(),
            &theme,
            &content,
            &language,
        );
        // R-B29: per-file wrap. The width estimate leaves room for the
        // gutters; exactness does not matter, only that lines stop escaping.
        if self.prefs.scoped.editor_wrap.contains(&path) {
            let reserved = 80.0 + if self.annotate { 240.0 } else { 0.0 };
            job.wrap.max_width = (ui.available_width() - reserved).max(200.0);
        }

        // R-B27: the diff gutter's data. Changed-vs-HEAD from the same
        // per-file diff the Git pane uses; this session's own lines from the
        // session diff already on hand. Worktree tabs only — a revision has
        // no uncommitted delta.
        let mut changed_head: Vec<u32> = Vec::new();
        let mut changed_session: Vec<u32> = Vec::new();
        if rev.is_none() {
            if let Some(sid) = self.explorer.session.clone() {
                self.gitview.ensure_session(&sid);
                if !self.gitview.local_diffs.contains_key(&path)
                    && self.gitview.pending_file_diffs.insert(path.clone())
                {
                    self.net.send(ClientMsg::GitDiffFile {
                        session_id: sid.clone(),
                        path: path.clone(),
                        context: Some(self.gitview.context()),
                        ignore_ws: Some(self.gitview.diff_ignore_ws),
                    });
                }
                if let Some(files) = self.gitview.local_diffs.get(&path) {
                    changed_head = changed_lines_of(files);
                }
                if let Some(ch) = self.changes.get(&sid) {
                    if let Some(f) = ch
                        .files
                        .iter()
                        .find(|f| f.path == path || f.path.ends_with(&path))
                    {
                        changed_session = changed_lines_of(std::slice::from_ref(f));
                    }
                }
            }
        }
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
                color: pal().dim,
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
                        color: pal().dim,
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
        let scroll_out = area.show(ui, |ui| {
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
                        pal().amber.linear_multiply(if current == Some(*l) { 0.28 } else { 0.10 }),
                    );
                }
            }
            // R-B28: occurrence bands, under the text like the find bands —
            // a different tint so a search and a highlight stay tellable.
            if let Some(word) = &self.occurrences_word {
                let origin = ui.cursor().min;
                let band_w = ui.available_width().clamp(0.0, 4000.0);
                let painter = ui.painter();
                let mut seen = HashSet::new();
                for (l, _) in crate::symbols::occurrences(&content, word) {
                    if !seen.insert(l) {
                        continue;
                    }
                    let Some(row) = galley.rows.get(l - 1) else { continue };
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(origin.x, origin.y + row.pos.y),
                            egui::vec2(band_w, row.rect().height()),
                        ),
                        0.0,
                        pal().blue.linear_multiply(0.10),
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
                let gutter_resp = ui.add(egui::Label::new(gutter_job).selectable(false));
                // R-B27: the diff gutter — a bar beside the line number.
                // Session-changed lines get the attribution colour on top of
                // the plain changed bar, so "changed" and "changed *by this
                // session*" stay distinguishable at a glance.
                if !changed_head.is_empty() || !changed_session.is_empty() {
                    let painter = ui.painter();
                    let top = gutter_resp.rect.top();
                    let x = gutter_resp.rect.left() - 4.0;
                    let bar = |l: u32, color: egui::Color32, w: f32| {
                        if let Some(row) = galley.rows.get(l.saturating_sub(1) as usize) {
                            painter.rect_filled(
                                egui::Rect::from_min_size(
                                    egui::pos2(x, top + row.pos.y),
                                    egui::vec2(w, row.rect().height()),
                                ),
                                0.0,
                                color,
                            );
                        }
                    };
                    for l in &changed_head {
                        bar(*l, pal().blue, 3.0);
                    }
                    for l in &changed_session {
                        bar(*l, pal().purple, 2.0);
                    }
                }
                // Selectable and highlighted, and structurally unable to
                // edit: a `Label` has no writable buffer.
                let code_resp = ui.add(
                    egui::Label::new(job)
                        .selectable(true)
                        .sense(egui::Sense::click()),
                );
                // R-B28/R-B29: the line the pointer is on, by real geometry.
                let code_row_at = |pos: egui::Pos2| -> usize {
                    let rel = pos.y - code_resp.rect.top();
                    galley.rows.partition_point(|r| r.pos.y <= rel).saturating_sub(1)
                };
                code_resp.context_menu(|ui| {
                    let line = ui
                        .ctx()
                        .pointer_interact_pos()
                        .map(code_row_at)
                        .unwrap_or(0) as u64
                        + 1;
                    let word = ui
                        .ctx()
                        .pointer_interact_pos()
                        .and_then(|pos| {
                            let rel = pos - code_resp.rect.min;
                            let idx = galley.cursor_from_pos(rel).index;
                            word_at_char_index(&content, idx.0)
                        });
                    if let Some(w) = &word {
                        if ui
                            .button(format!("Highlight occurrences of \"{w}\""))
                            .clicked()
                        {
                            self.occurrences_word = Some(w.clone());
                            ui.close();
                        }
                    }
                    if self.occurrences_word.is_some()
                        && ui.button("Clear occurrence highlight").clicked()
                    {
                        self.occurrences_word = None;
                        ui.close();
                    }
                    if ui.button(format!("Copy {path}:{line}")).clicked() {
                        ui.ctx().copy_text(format!("{path}:{line}"));
                        ui.close();
                    }
                    // R-B29: bookmarks, toggled where the pointer is.
                    if let Some(sid) = self.explorer.session.clone() {
                        let mark = (sid, path.clone(), line);
                        let existing = self.prefs.scoped.bookmarks.iter().position(|b| *b == mark);
                        let label = if existing.is_some() {
                            format!("Remove bookmark at line {line}")
                        } else {
                            format!("Bookmark line {line}")
                        };
                        if ui.button(label).clicked() {
                            match existing {
                                Some(i) => {
                                    self.prefs.scoped.bookmarks.remove(i);
                                }
                                None => {
                                    self.prefs.scoped.bookmarks.push(mark);
                                    if self.prefs.scoped.bookmarks.len() > 100 {
                                        self.prefs.scoped.bookmarks.remove(0);
                                    }
                                }
                            }
                            self.prefs_dirty = true;
                            ui.close();
                        }
                    }
                });
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

        // R-B27: n/p walk the changed lines — the Git pane's hunk keys, the
        // same gating: pointer over this pane, no text widget focused.
        if rev.is_none()
            && !changed_head.is_empty()
            && ui.ui_contains_pointer()
            && !self.explorer_find_open
            && !self.show_goto
            && ui.ctx().memory(|m| m.focused().is_none())
        {
            let (next, prev) = ui.input(|i| {
                (i.key_pressed(egui::Key::N), i.key_pressed(egui::Key::P))
            });
            if next || prev {
                let top_row = galley
                    .rows
                    .partition_point(|r| r.pos.y <= scroll_out.state.offset.y);
                let current = top_row as u32 + 1;
                let target = if next {
                    changed_head
                        .iter()
                        .copied()
                        .find(|l| *l > current)
                        .or_else(|| changed_head.first().copied())
                } else {
                    changed_head
                        .iter()
                        .rev()
                        .copied()
                        .find(|l| *l < current)
                        .or_else(|| changed_head.last().copied())
                };
                if let Some(t) = target {
                    let st = self.explorer.current_mut();
                    if let Some(i) = st.active_of(group) {
                        if let Some(tab) = st.open.get_mut(i) {
                            tab.goto_line = Some(t as u64);
                        }
                    }
                }
            }
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
            // A directory that could not be read never reaches here — the
            // daemon answers those with an error, which lands in the error
            // strip. So this is genuinely empty, and says so. `R-J5`.
            ui.label(dim("this folder is empty"));
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
                "⏷ "
            } else {
                "⏵ "
            };
            let picked = !e.is_dir && active_path.as_deref() == Some(path.as_str());
            let ignored = crate::gitview::is_ignored(
                &path,
                &ignored_prefixes.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            );
            let mut row_text = RichText::new(format!("{glyph}{}", e.name))
                .monospace()
                .color(if ignored {
                    pal().dim
                } else if e.is_dir {
                    pal().text
                } else {
                    pal().blue
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
            // Opening raises the Editor. The tree lives in the rail now
            // (`R-B41`), so the tab holding the viewer may not be the one in
            // front of you — and a click that filled a pane you cannot see
            // would look like a click that did nothing.
            if row.double_clicked() && !e.is_dir {
                self.explorer.open_file(&path, true, None);
                self.set_tab(Tab::Explorer);
            } else if row.clicked() {
                if e.is_dir {
                    // Toggling is all a click does; `explorer_fetch` notices
                    // an expanded dir with no listing.
                    let st = self.explorer.current_mut();
                    if open {
                        st.expanded.remove(&path);
                    } else {
                        st.expanded.insert(path.clone());
                    }
                    self.explorer.dirty = true;
                } else {
                    self.explorer.open_file(&path, false, None);
                    self.set_tab(Tab::Explorer);
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
    fn agent_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        // Before the terminal is borrowed: zoom and the font are App state.
        let term_font = self.terminal_font(ui, "agent");
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
                .button(format!("{} Switch to its terminal app instead", icon::TERMINAL))
                .clicked()
            {
                self.send_focus(s.id.clone());
            }
            return;
        };

        // Re-attach when the selection moves to a different session. Comparing
        // targets rather than session ids is deliberate: a session that is
        // restarted keeps its id but gets a new pane.
        if self.agent_term.as_ref().is_some_and(|t| t.target() != target) {
            self.agent_term = None;
        }
        if self.agent_failed.as_ref().is_some_and(|(t, _)| *t != target) {
            self.agent_failed = None;
        }

        // A refusal is remembered and *not* retried. It used to be retried on
        // the next frame, which is fine for a one-off but a spin for anything
        // that recurs — a tmux that rejects the environment it was handed, a
        // session that has ended — because each frame spawned another tmux and
        // redrew the same error. What looked like a flickering message was the
        // pane failing sixty times a second.
        if let Some((_, err)) = self.agent_failed.clone() {
            ui.add_space(8.0);
            ui.colored_label(pal().red, format!("could not attach to {target}: {err}"));
            ui.add_space(8.0);
            if ui.button("Try again").clicked() {
                self.agent_failed = None;
            }
            return;
        }

        if self.agent_term.is_none() {
            self.release_pty(PtyPane::Agent);
            // The session's tmux lives wherever the daemon does. Attaching to
            // that name locally would either miss or — worse — match an
            // unrelated session of the same name on this machine.
            let Some(reach) = self.terminal_reach() else {
                self.agent_failed = Some((target.clone(), self.no_reach_reason()));
                return;
            };
            match crate::term::Term::attach(ui.ctx(), &target, &reach) {
                Ok(t) => self.agent_term = Some(t),
                Err(e) => {
                    self.agent_term = None;
                    self.agent_failed = Some((target.clone(), e.to_string()));
                    return;
                }
            }
        }

        let focused = self.pty_focus == Some(PtyPane::Agent);
        let Some(term) = self.agent_term.as_mut() else {
            return;
        };
        term.poll();
        // Keep drawing an exited terminal rather than replacing it: whatever
        // tmux printed on the way out is still in the grid, and that text is
        // the only explanation the user gets.
        let exited = term.exited();
        let mut reattach = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} {target}", icon::TERMINAL)).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if exited {
                    reattach = ui.button("Re-attach").clicked();
                    ui.label(
                        RichText::new("the tmux client ended — the session itself is untouched")
                            .color(pal().red),
                    );
                    return;
                }
                let hint = if focused {
                    format!(
                        "keyboard goes to the agent — {} returns it",
                        self.keymap.describe(crate::keymap::Action::ToggleTerminalFocus)
                    )
                } else {
                    format!(
                        "click the terminal — or press {} — to type into this session",
                        self.keymap.describe(crate::keymap::Action::ToggleTerminalFocus)
                    )
                };
                ui.label(RichText::new(hint).weak());
            });
        });
        ui.separator();

        pty_body(ui, term, PtyPane::Agent, &mut self.pty_focus, term_font);

        // After the last use of `term`, which borrows the field this drops.
        if reattach {
            self.agent_term = None;
            self.release_pty(PtyPane::Agent);
        }
    }

    /// Your own shells, in a panel across the bottom. `R-B31`, `R-B33`.
    ///
    /// Under tmux, so what you start here — a build, a test run, a `claude` —
    /// outlives this window and stays reachable from a real terminal
    /// ([ADR-0011](../../../docs/decisions/0011-own-a-shell-never-an-agent.md)).
    ///
    /// A panel and not a pane, which is the whole of `R-B33`. As a pane it was
    /// *about* the selected session, like every other pane: it followed the
    /// selection, so a shell in one worktree vanished when you looked at
    /// another, and it could not exist at all until a session did — which is
    /// exactly the moment you want a shell, because starting one is what you
    /// are about to do. Nothing here is keyed by session id, and that is the
    /// point rather than an implementation detail.
    fn terminal_panel(&mut self, root: &mut egui::Ui) {
        if !self.shells.open {
            return;
        }
        let resp = egui::Panel::bottom("terminal")
            .resizable(true)
            .default_size(self.shells.height)
            .min_size(crate::shells::MIN_HEIGHT)
            .frame(
                egui::Frame::NONE
                    .fill(pal().bg)
                    .inner_margin(egui::Margin::symmetric(10, 6)),
            )
            .show(root, |ui| self.terminal_body(ui));

        // The panel owns its height while you drag it; this copies the result
        // back. Written to disk only once the pointer is up — the same rule
        // the layout follows, and for the same reason: a drag would otherwise
        // write the file sixty times a second.
        let now = resp.response.rect.height();
        if (now - self.shells.height).abs() > 0.5 {
            self.shells.height = now;
            self.shells.dirty = true;
        }
    }

    fn terminal_body(&mut self, ui: &mut egui::Ui) {
        let font = self.terminal_font(ui, "terminal");
        // A remote daemon with no ssh target has nowhere to run a shell. Say
        // so and stop, rather than starting one here — which is what this did
        // before `R-I6`, in a directory that only exists on the other machine.
        let Some(reach) = self.terminal_reach() else {
            ui.add_space(8.0);
            ui.label(dim(self.no_reach_reason()));
            return;
        };
        self.shells.tick(ui.ctx(), &reach);

        // Read before the shell is borrowed below.
        let leave = self.keymap.describe(crate::keymap::Action::ToggleTerminalFocus);
        let default_root = self.default_shell_root();
        let roots = self.known_roots();

        let mut act = PanelAction::None;
        // Taken for the frame. The text being typed is state on `self.shells`,
        // and the loop below holds the tab list borrowed to draw it.
        let mut editing = self.shells.rename.take();
        ui.horizontal(|ui| {
            for (i, s) in self.shells.tabs.iter().enumerate() {
                let name = s.session_name();
                let tip = match s.kind() {
                    Some(crate::term::Kind::Bare) => format!(
                        "{}\n\nNo tmux — this shell dies with the window.\n\
                         Double-click the tab to rename it.",
                        s.root
                    ),
                    // Naming the host matters here: `tmux attach -t …` is
                    // advice you act on, and run on the wrong machine it finds
                    // nothing — or something else entirely.
                    _ => match reach.host_label() {
                        Some(host) => format!(
                            "{}\n\nA tmux session on {host}. It survives mogeung \
                             closing, and `tmux attach -t {name}` reaches it from \
                             any terminal *there*.\n\
                             Double-click the tab to rename it — the label only, \
                             never the session.",
                            s.root
                        ),
                        None => format!(
                            "{}\n\nA tmux session. It survives mogeung closing, and \
                             `tmux attach -t {name}` reaches it from any terminal.\n\
                             Double-click the tab to rename it — the label only, \
                             never the session.",
                            s.root
                        ),
                    },
                };
                let active = i == self.shells.active;

                // Renamed in place rather than in a window: the tab is small,
                // the thing being named is right there, and every terminal
                // and browser made this gesture mean this for twenty years.
                if let Some(r) = editing.as_mut().filter(|r| r.at == i) {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut r.text)
                            .id(egui::Id::new("shell-rename"))
                            .char_limit(crate::shells::NAME_LIMIT)
                            .hint_text(s.default_title())
                            .desired_width(120.0),
                    );
                    // Once, on the frame the edit opened. Asking every frame
                    // would make the field impossible to click away from.
                    if std::mem::take(&mut r.opened) {
                        field.request_focus();
                    }
                    let (enter, escape) = ui.input(|inp| {
                        (
                            inp.key_pressed(egui::Key::Enter),
                            inp.key_pressed(egui::Key::Escape),
                        )
                    });
                    // Escape before the lost-focus commit, and not merged with
                    // it: egui surrenders focus on Escape, so the two arrive on
                    // the same frame and testing focus first would save the
                    // edit you just abandoned.
                    if escape {
                        act = PanelAction::CancelRename;
                    } else if enter || field.lost_focus() {
                        act = PanelAction::CommitRename;
                    }
                    continue;
                }

                let tab = ui.selectable_label(active, s.title()).on_hover_text(tip);
                if tab.clicked() {
                    act = PanelAction::Select(i);
                }
                if tab.double_clicked() {
                    act = PanelAction::Rename(i);
                }
                tab.context_menu(|ui| {
                    if ui.button("Rename…").clicked() {
                        act = PanelAction::Rename(i);
                        ui.close();
                    }
                    if ui
                        .add_enabled(s.name.is_some(), egui::Button::new("Use the folder name"))
                        .on_hover_text(s.default_title())
                        .clicked()
                    {
                        act = PanelAction::ClearName(i);
                        ui.close();
                    }
                    if ui.button("Close").clicked() {
                        act = PanelAction::Close(i);
                        ui.close();
                    }
                });
                // On the active tab only, and beside the thing it closes —
                // there is a second × at the far right that hides the whole
                // panel, and two identical glyphs meaning different things
                // are only told apart by where they sit.
                if active
                    && ui
                        .add(
                            egui::Button::new(RichText::new(icon::HIDE).color(pal().dim))
                                .frame(false),
                        )
                        .on_hover_text("Close this shell — detaches from tmux, never kills it")
                        .clicked()
                {
                    act = PanelAction::Close(i);
                }
            }
            if ui
                .button("+")
                .on_hover_text(format!("A shell in {default_root}"))
                .clicked()
            {
                act = PanelAction::OpenIn(default_root.clone());
            }
            ui.menu_button(icon::EXPANDED, |ui| {
                ui.label(dim("open a shell in"));
                for r in &roots {
                    if ui.button(r.as_str()).clicked() {
                        act = PanelAction::OpenIn(r.clone());
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("A shell somewhere else — any worktree mogeung knows, or home");

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(
                    ui,
                    icon::HIDE,
                    "Hide the terminal — the shells keep running",
                    false,
                    None,
                )
                .clicked()
                {
                    act = PanelAction::Hide;
                }
                self.terminal_font_menu(ui);
                if let Some(i) = (!self.shells.tabs.is_empty()).then_some(self.shells.active) {
                    ui.separator();
                    let s = &self.shells.tabs[i];
                    if s.kind() == Some(crate::term::Kind::Bare) {
                        ui.label(
                            RichText::new("no tmux — this shell dies with the window")
                                .color(pal().amber),
                        )
                        .on_hover_text(
                            "Install tmux and the shells here persist across restarts, and \
                             stay reachable from a real terminal.",
                        );
                    }
                    if s.exited() {
                        if ui.button("New shell").clicked() {
                            act = PanelAction::Restart(i);
                        }
                        ui.label(RichText::new("the shell exited").color(pal().red));
                    } else if s.failed.is_some() {
                        if ui.button("Try again").clicked() {
                            act = PanelAction::Restart(i);
                        }
                        // The reason itself is in the body, once. Repeating it
                        // here would cost the header its whole width to say
                        // what is already on screen.
                        ui.label(RichText::new("could not start a shell").color(pal().red));
                    } else {
                        let hint = if self.pty_focus == Some(PtyPane::Shell) {
                            format!("keyboard goes to the shell — {leave} returns it")
                        } else {
                            format!("click the terminal — or press {leave} — to type")
                        };
                        ui.label(RichText::new(hint).weak());
                    }
                }
            });
        });
        // Back where it lives, before the actions below act on it. An index
        // past the end cannot happen — everything that moves the tabs ends the
        // edit — and if it ever did, the field would stop drawing and there
        // would be no way left to dismiss it.
        self.shells.rename = editing.filter(|r| r.at < self.shells.tabs.len());
        ui.separator();

        // The shortcut that opened the panel asked for the keyboard too, and
        // this is the first point at which there is something to give it to.
        let ready = self.shells.current().is_some_and(|s| s.term.is_some());
        if ready && std::mem::take(&mut self.shells.focus_wanted) {
            self.pty_focus = Some(PtyPane::Shell);
        }

        match self.shells.tabs.get_mut(self.shells.active) {
            Some(s) => match s.term.as_mut() {
                // Kept on screen after it exits rather than replaced: whatever
                // the shell printed on its way out is still in the grid, and
                // that text is the only explanation anyone gets.
                Some(term) => pty_body(ui, term, PtyPane::Shell, &mut self.pty_focus, font),
                None => {
                    ui.add_space(8.0);
                    ui.colored_label(
                        pal().red,
                        s.failed
                            .clone()
                            .unwrap_or_else(|| "the shell is not running".into()),
                    );
                }
            },
            None => {
                ui.add_space(8.0);
                ui.label(dim("no shell open — the + button starts one"));
            }
        }

        match act {
            PanelAction::None => {}
            PanelAction::Select(i) => {
                self.shells.select(i);
                // The keyboard follows the tab you clicked, not the shell you
                // were typing into a moment ago.
                self.release_pty(PtyPane::Shell);
            }
            PanelAction::OpenIn(root) => {
                self.shells.open_in(&root);
                self.shells.focus_wanted = true;
            }
            PanelAction::Close(i) => {
                self.shells.close(i);
                self.release_pty(PtyPane::Shell);
            }
            PanelAction::Restart(i) => self.shells.restart(i),
            PanelAction::Rename(i) => {
                // The field and the pty cannot both have the keyboard, and the
                // pty is holding it: without this you would type the new name
                // into the shell instead.
                self.release_pty(PtyPane::Shell);
                self.shells.begin_rename(i);
            }
            PanelAction::CommitRename => self.shells.commit_rename(),
            PanelAction::CancelRename => self.shells.cancel_rename(),
            PanelAction::ClearName(i) => self.shells.clear_name(i),
            PanelAction::Hide => {
                self.shells.open = false;
                self.shells.dirty = true;
                self.shells.cancel_rename();
                self.release_pty(PtyPane::Shell);
            }
        }
    }

    /// The font picker. `R-B32`.
    ///
    /// Here rather than in a settings window because this is the only place the
    /// setting means anything, and because the thing you want to do after
    /// choosing a font is look at a terminal drawn in it.
    fn terminal_font_menu(&mut self, ui: &mut egui::Ui) {
        // What is on screen, which is not the same question as what was
        // chosen: with nothing chosen the answer is whatever `PREFERRED`
        // found, and a menu that showed "default" there would be hiding the
        // only interesting part.
        let auto = crate::font::default_family().map(|f| f.family.clone());
        let auto_label = match &auto {
            Some(f) => format!("automatic — {f}"),
            None => "automatic — bundled monospace".to_string(),
        };
        let current = self
            .prefs
            .terminal_font
            .clone()
            .or(auto)
            .unwrap_or_else(|| "bundled monospace".into());
        ui.menu_button("Aa", |ui| {
            ui.set_min_width(240.0);
            ui.label(dim("terminal font"));
            ui.horizontal(|ui| {
                ui.label("size");
                let mut px = self.prefs.terminal_font_px;
                if ui
                    .add(egui::DragValue::new(&mut px).range(8.0..=32.0).speed(0.25))
                    .changed()
                {
                    self.prefs.terminal_font_px = px;
                    self.prefs_dirty = true;
                }
                ui.label(dim("pt · Ctrl+wheel zooms this pane alone"));
            });
            ui.separator();
            if ui
                .selectable_label(self.prefs.terminal_font.is_none(), auto_label)
                .on_hover_text(
                    "MesloLGS NF when it is installed — the font Powerlevel10k's own \
                     wizard sets up — and the bundled monospace otherwise, which carries \
                     no Powerline or Nerd Font glyphs.",
                )
                .clicked()
            {
                self.prefs.terminal_font = None;
                self.prefs_dirty = true;
                ui.close();
            }
            // Scanned once per process, so a font installed while mogeung is
            // running needs a restart to appear — the same deal every terminal
            // emulator offers, and the alternative is a directory walk per
            // frame.
            let families = crate::font::monospace_families();
            if families.is_empty() {
                ui.label(dim("no monospaced fonts found on this machine"));
            }
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for f in families {
                        let chosen = self.prefs.terminal_font.as_deref() == Some(f.as_str());
                        if ui.selectable_label(chosen, &f).clicked() {
                            self.prefs.terminal_font = Some(f);
                            self.prefs_dirty = true;
                            ui.close();
                        }
                    }
                });
        })
        .response
        .on_hover_text(format!(
            "Terminal font — {current}\n\nMonospaced families only: a terminal grid is laid \
             out from the width of one glyph, so a proportional font does not draw badly, it \
             draws wrongly."
        ));
    }

    /// The terminal family at this pane's zoom.
    ///
    /// `FontFamily::Name`, always — [`crate::font::definitions`] registers the
    /// name whether or not a font was chosen, precisely so this cannot ask for
    /// a family egui does not know. That is a panic inside the paint loop, not
    /// a fallback.
    fn terminal_font(&mut self, ui: &egui::Ui, pane: &str) -> egui::FontId {
        let z = self.pane_zoom(ui, pane);
        let px = (self.prefs.terminal_font_px * z).clamp(6.0, 48.0);
        egui::FontId::new(px, egui::FontFamily::Name(crate::font::FAMILY.into()))
    }

    /// Register the chosen font with egui, when and only when it changed.
    ///
    /// `set_fonts` rebuilds the glyph atlas, so this must not run per frame —
    /// and it takes effect on the *next* frame, which is why the first install
    /// happens in `App::new` rather than here. A family that does not exist yet
    /// is not a missing glyph, it is a panic.
    fn sync_terminal_font(&mut self, ctx: &egui::Context) {
        if self.font_installed.as_ref() == Some(&self.prefs.terminal_font) {
            return;
        }
        self.font_installed = Some(self.prefs.terminal_font.clone());
        if let Some(w) = crate::font::install(ctx, self.prefs.terminal_font.as_deref()) {
            self.errors.push(w);
        }
    }

    /// Show the terminal, and put the keyboard in it.
    ///
    /// Opening with no shells starts one: a panel that appears empty and asks
    /// you to press `+` has spent your keystroke on nothing.
    fn open_terminal_panel(&mut self) {
        self.shells.open = true;
        if self.shells.tabs.is_empty() {
            let root = self.default_shell_root();
            self.shells.open_in(&root);
        }
        self.shells.focus_wanted = true;
        self.shells.dirty = true;
    }

    fn toggle_terminal_panel(&mut self) {
        if self.shells.open {
            self.shells.open = false;
            self.shells.dirty = true;
            // A half-typed name in a panel you just put away is not a name,
            // and leaving it in flight means the field comes back holding the
            // keyboard's attention without having asked for it.
            self.shells.cancel_rename();
            self.release_pty(PtyPane::Shell);
        } else {
            self.open_terminal_panel();
        }
    }

    /// Where a new shell starts when you have not said.
    ///
    /// The selected session's worktree, because that is what you are looking
    /// at — and `$HOME` when there is no selection, which is the case the pane
    /// version could not express at all.
    fn default_shell_root(&self) -> String {
        self.selected_session()
            .map(|s| s.repo_root.clone().unwrap_or_else(|| s.cwd.clone()))
            .or_else(|| std::env::var("HOME").ok())
            .unwrap_or_else(|| ".".to_string())
    }

    /// The directories the `+` menu offers: every worktree mogeung knows, plus
    /// home. Sorted and deduplicated, with the selection first — it is the one
    /// you are most likely to want and should not be hunted for.
    fn known_roots(&self) -> Vec<String> {
        let first = self.default_shell_root();
        let mut out = vec![first.clone()];
        let mut rest: Vec<String> = self
            .sessions
            .values()
            .map(|s| s.repo_root.clone().unwrap_or_else(|| s.cwd.clone()))
            .chain(std::env::var("HOME").ok())
            .filter(|r| *r != first)
            .collect();
        rest.sort();
        rest.dedup();
        out.extend(rest);
        out
    }

    /// Take the keyboard back from one pty pane, and only that one.
    ///
    /// The `if` is the point. Both panes draw every frame when they are side
    /// by side in a split, so an unconditional clear here would let the Agent
    /// pane re-attaching yank focus out of a shell mid-command.
    fn release_pty(&mut self, which: PtyPane) {
        if self.pty_focus == Some(which) {
            self.pty_focus = None;
        }
    }

    /// The note on one turn, and the affordance to write one. `R-B35`.
    ///
    /// A mark and a note are the same object at two depths: a note with an
    /// empty body *is* a bookmark. That is one concept rather than two, and it
    /// means marking a turn and writing about it are the same gesture — you
    /// never have to decide which you are doing before you start.
    fn note_on_turn(&mut self, ui: &mut egui::Ui, session_id: &str, seq: u64) {
        let existing = self
            .notes
            .iter()
            .find(|n| n.session_id.as_deref() == Some(session_id) && n.seq == Some(seq))
            .cloned();
        let editing = self
            .note_draft
            .as_ref()
            .is_some_and(|(s, q, _)| s == session_id && *q == seq);

        if !editing {
            // Nothing here until there is something to say, or the pointer is
            // over the row: a transcript is for reading, and a column of
            // buttons down the side of it is a column of noise.
            let marked = existing.is_some();
            let hovered = ui.rect_contains_pointer(ui.max_rect());
            if !marked && !hovered {
                return;
            }
            ui.horizontal(|ui| {
                let (glyph, tip) = match &existing {
                    Some(n) if n.body.trim().is_empty() => {
                        (icon::FLAG, "bookmarked — click to write a note")
                    }
                    Some(_) => (icon::FLAG, "click to edit this note"),
                    None => (icon::FLAG, "bookmark this turn, or write a note on it"),
                };
                let tint = if marked { pal().amber } else { pal().dim };
                if ui::icon_button(ui, glyph, tip, marked, Some(tint)).clicked() {
                    self.note_draft = Some((
                        session_id.to_string(),
                        seq,
                        existing.as_ref().map(|n| n.body.clone()).unwrap_or_default(),
                    ));
                }
                if let Some(n) = &existing {
                    if !n.body.trim().is_empty() {
                        // The note itself, rendered where it was written. It is
                        // yours, so it reads as prose rather than as metadata.
                        ui.label(
                            RichText::new(crate::ui::truncate(n.body.trim(), 90))
                                .color(pal().amber)
                                .size(12.0),
                        );
                    }
                }
            });
            return;
        }

        // The editor, in place. Deliberately not a dialog: a note about a turn
        // is written while looking at the turn, and a modal would cover it.
        let mut save = false;
        let mut cancel = false;
        let mut discard = false;
        egui::Frame::NONE
            .fill(pal().bg_raised)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some((_, _, body)) = self.note_draft.as_mut() {
                    let field = ui.add(
                        egui::TextEdit::multiline(body)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .hint_text("what you want to remember about this turn"),
                    );
                    field.request_focus();
                    // Ctrl+Enter saves, Escape abandons — the two things hands
                    // already do in every other text box in this window.
                    if field.has_focus() {
                        let (enter, esc) = ui.input(|i| {
                            (
                                i.key_pressed(egui::Key::Enter) && i.modifiers.command,
                                i.key_pressed(egui::Key::Escape),
                            )
                        });
                        save |= enter;
                        cancel |= esc;
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if existing.is_some()
                            && ui
                                .button(RichText::new("Delete").color(pal().red))
                                .on_hover_text(
                                    "there is no undo — a note is the one thing here \
                                     nothing can recompute",
                                )
                                .clicked()
                        {
                            discard = true;
                        }
                        ui.label(dim("Ctrl+Enter saves · Esc cancels"));
                    });
                });
            });

        if save {
            if let Some((sid, q, body)) = self.note_draft.take() {
                // An emptied note is a deletion, not a blank one — except when
                // there was never a note, where it is a plain bookmark. The
                // difference is whether you cleared something or marked
                // something.
                match (existing.as_ref(), body.trim().is_empty()) {
                    (Some(n), true) => self.net.send(ClientMsg::NoteDelete { id: n.id.clone() }),
                    _ => self.net.send(ClientMsg::NoteSave {
                        id: existing.as_ref().map(|n| n.id.clone()).unwrap_or_default(),
                        body,
                        session_id: Some(sid),
                        seq: Some(q),
                        repo: None,
                    }),
                }
            }
        } else if discard {
            if let Some(n) = &existing {
                self.net.send(ClientMsg::NoteDelete { id: n.id.clone() });
            }
            self.note_draft = None;
        } else if cancel {
            self.note_draft = None;
        }
    }

    fn transcript_tab(&mut self, ui: &mut egui::Ui, s: &Session) {
        let z = self.pane_zoom(ui, "transcript");
        scale_text(ui, z);
        // `R-J5`. Kept apart: nothing has arrived yet, versus the transcript
        // really is empty. Flattened into one empty list, both said "no events
        // yet" — and the one that means "still loading" is the one you would
        // otherwise sit and stare at.
        let arrived = self.events.contains_key(&s.id);
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

        // R-F8: the session's subagents, from their nested transcripts.
        if !self.subagents_asked.contains(&s.id) {
            self.subagents_asked.insert(s.id.clone());
            self.net.send(ClientMsg::FetchSubagents {
                session_id: s.id.clone(),
            });
        }
        if let Some(nodes) = self.subagents.get(&s.id).filter(|n| !n.is_empty()) {
            egui::CollapsingHeader::new(dim(format!("subagents ({})", nodes.len())))
                .id_salt(("subagents", &s.id))
                .show(ui, |ui| {
                    for n in nodes {
                        ui.horizontal(|ui| {
                            ui.label(mono(format!("agent-{}", n.agent_id)).size(11.0));
                            ui.label(dim(format!("{} line(s)", n.lines)));
                            if let Some(a) = &n.last_activity {
                                ui.add(
                                    egui::Label::new(dim(truncate(a, 90))).truncate(),
                                );
                            }
                        });
                    }
                });
            ui.separator();
        }

        // `R-B36`. Searching *this* conversation, which `R-F1` cannot do —
        // that one searches across every session and answers with sessions.
        let mut jump_to: Option<u64> = None;
        ui.horizontal(|ui| {
            ui.label(dim("find"));
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.transcript_find)
                    .hint_text("in this transcript")
                    .desired_width(220.0),
            );
            let q = self.transcript_find.trim().to_string();
            if !q.is_empty() {
                // Every turn scored by the same scale, best first. The engine
                // that won is named, because a search that silently prefers
                // one is worse than one that says why it matched.
                let mut hits: Vec<(i32, u64, crate::search::Engine)> = events
                    .iter()
                    .filter_map(|ev| {
                        crate::search::best(&q, &event_text(&ev.kind))
                            .map(|h| (h.score, ev.seq, h.engine))
                    })
                    .collect();
                hits.sort_by(|a, b| b.0.cmp(&a.0));
                if hits.is_empty() {
                    ui.label(dim("no match"));
                } else {
                    ui.label(dim(format!(
                        "{} turn(s) · best is {}",
                        hits.len(),
                        hits[0].2.label()
                    )));
                    if ui.button("Go").on_hover_text("jump to the best match").clicked()
                        || (field.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    {
                        jump_to = Some(hits[0].1);
                    }
                }
            }
            if !self.transcript_find.is_empty() && ui.small_button(icon::HIDE).clicked() {
                self.transcript_find.clear();
            }
        });

        // A jump to a moment must be able to reach skipped history.
        if self.focus_event_ts.is_some() || jump_to.is_some() {
            self.transcript_limit = self.transcript_limit.max(events.len());
        }
        if let Some(seq) = jump_to {
            // Reuse the existing focus mechanism rather than growing a second
            // one: it already knows how to scroll a turn into the middle.
            self.focus_event_ts = events.iter().find(|e| e.seq == seq).map(|e| e.ts);
        }

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
                    ui.label(dim(if arrived {
                        "no events in this transcript yet — the session has not spoken"
                    } else {
                        "reading the transcript…"
                    }));
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
                    // R-F1/R-F9: land on the first event at or after the
                    // asked-for moment, once, then behave normally.
                    let focus_here = self
                        .focus_event_ts
                        .map(|t| ev.ts >= t)
                        .unwrap_or(false);
                    let seq = ev.seq;
                    let resp = ui
                        .scope(|ui| event_row(ui, ev, &mut self.md_cache, &self.prefs))
                        .response;
                    // `R-B35`, under the turn it is about.
                    self.note_on_turn(ui, &s.id, seq);
                    if focus_here {
                        resp.scroll_to_me(Some(egui::Align::Center));
                        self.focus_event_ts = None;
                    }
                }
                // A moment later than everything shown: settle at the end.
                if self.focus_event_ts.is_some() && !shown.is_empty() {
                    self.focus_event_ts = None;
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
            ui.label(RichText::new(err).color(pal().amber).size(12.0));
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
                    .selectable_label(self.prefs.side_by_side, icon::SIDE_BY_SIDE)
                    .on_hover_text("side by side (R-D6)")
                    .clicked()
                {
                    self.prefs.side_by_side = true;
                    self.prefs_dirty = true;
                }
                if ui
                    .selectable_label(!self.prefs.side_by_side, icon::UNIFIED)
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
        egui::Panel::left("files")
            .default_size(300.0)
            .resizable(true)
            .show(ui, |ui| {
            let focused = self.pane == Pane::Files || self.pane == Pane::Diff;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("FILES")
                        .size(11.0)
                        .color(if focused { pal().blue } else { pal().dim })
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
        // Both axes since `R-J2`: diff rows no longer wrap, so a long line is
        // reached by scrolling sideways. The git panes have always worked this
        // way; before this the two diff surfaces disagreed.
        egui::ScrollArea::both()
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
                        // The pane's width, not the available width: inside a
                        // horizontally-scrolling area the latter is effectively
                        // unbounded, and setting it would give every hunk a
                        // frame the width of the scroll extent. A minimum
                        // rather than an exact width, so a long diff line still
                        // widens the frame it sits in.
                        ui.set_min_width(ui.clip_rect().width() - 24.0);
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
                            ui.label(mono(&hunk.header).color(pal().dim));
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
                                        ui.small_button(RichText::new(label).color(pal().amber))
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
            ui.label(mono(&repo).color(pal().dim));
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
            ui.label(RichText::new("Nothing outstanding.").color(pal().green));
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
                row("source", s.source.label().to_string());
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

                self.verification_section(ui, s);
            });
    }

    /// Evidence, claims and the signal runner — pillar E's home. `R-E1`–`R-E5`.
    fn verification_section(&mut self, ui: &mut egui::Ui, s: &Session) {
        ui.add_space(10.0);
        ui.separator();
        ui.label(dim("verification"));

        if s.unverified() {
            ui.label(
                RichText::new("UNVERIFIED — edited files; no build/test/typecheck completed")
                    .size(11.5)
                    .color(pal().amber),
            );
        }
        if s.verify_runs.is_empty() {
            ui.label(dim("no build/test/typecheck commands seen in this session"));
        }
        for r in s.verify_runs.iter().rev().take(12) {
            let (mark, col, word) = match r.ok {
                Some(true) => (icon::READ, pal().green, "ok"),
                Some(false) => (icon::HIDE, pal().red, "failed"),
                None => ("…", pal().dim, "no result"),
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(mark).size(11.5).color(col));
                ui.label(dim(format!("{:>9}", r.kind.label())));
                ui.label(mono(truncate(&r.command, 90)).size(11.5));
                ui.label(RichText::new(word).size(10.5).color(col));
            });
        }

        if !s.claims.is_empty() {
            ui.add_space(6.0);
            ui.label(dim(format!("claims ({})", s.claims.len())));
            for c in s.claims.iter().rev().take(10) {
                ui.horizontal_wrapped(|ui| {
                    if c.contradicted {
                        ui.label(
                            RichText::new("CONTRADICTED").size(10.0).color(pal().red).strong(),
                        )
                        .on_hover_text(
                            "the run this claim matches ended in failure",
                        );
                    }
                    ui.label(RichText::new(truncate(&c.text, 140)).size(11.5));
                });
                // The heuristic's work, stated — never an implied verdict.
                ui.label(dim(format!("   {}", c.evidence)));
            }
        }

        let repo = s
            .repo_root
            .clone()
            .unwrap_or_else(|| s.cwd.clone());
        if repo.is_empty() {
            return;
        }
        ui.add_space(10.0);
        ui.label(dim("signal command — runs only when you click, never on its own"));

        let mut send_fetch = false;
        let mut send_set: Option<String> = None;
        let mut send_run = false;
        {
            let st = self.signals.entry(repo.clone()).or_default();
            if !st.fetched && !st.asked {
                st.asked = true;
                send_fetch = true;
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut st.edit)
                        .hint_text("e.g. cargo test --workspace")
                        .desired_width(340.0)
                        .font(egui::TextStyle::Monospace),
                );
                if ui.button("save").clicked() {
                    send_set = Some(st.edit.clone());
                }
                let can_run = st.command.is_some() && !st.running;
                let label = if st.running { "running…" } else { "run" };
                if ui.add_enabled(can_run, egui::Button::new(label)).clicked() {
                    send_run = true;
                }
            });
            if let Some(last) = &st.last {
                let (word, col) = match last.exit {
                    Some(0) => ("exited ok".to_string(), pal().green),
                    Some(c) => (format!("exited {c}"), pal().red),
                    None => ("did not run".to_string(), pal().red),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(word).size(11.5).color(col));
                    ui.label(dim(format!(
                        "{} · {}s · {}",
                        truncate(&last.command, 60),
                        last.secs,
                        last.started_at.format("%m-%d %H:%M")
                    )));
                });
                if !last.tail.is_empty() {
                    egui::CollapsingHeader::new(dim("output tail"))
                        .id_salt(("signal-tail", &repo))
                        .show(ui, |ui| {
                            ui.label(mono(&last.tail).size(10.5));
                        });
                }
                if last.coverage.is_empty() {
                    // R-E5: absent data says so; a made-up zero would be worse.
                    ui.label(dim("coverage on changed lines: no data"));
                } else {
                    ui.label(dim("coverage on changed lines"));
                    for fc in last.coverage.iter().take(20) {
                        let total = fc.changed_covered + fc.changed_missed;
                        let col = if fc.changed_missed == 0 { pal().green } else { pal().amber };
                        ui.horizontal(|ui| {
                            ui.label(mono(truncate(&fc.path, 60)).size(11.0));
                            ui.label(
                                RichText::new(format!(
                                    "{}/{total} covered",
                                    fc.changed_covered
                                ))
                                .size(11.0)
                                .color(col),
                            );
                        });
                    }
                }
            }
        }
        if send_fetch {
            self.net.send(ClientMsg::FetchSignal { repo: repo.clone() });
        }
        if let Some(command) = send_set {
            self.net.send(ClientMsg::SetSignalCommand {
                repo: repo.clone(),
                command,
            });
        }
        if send_run {
            self.net.send(ClientMsg::RunSignal {
                session_id: s.id.clone(),
            });
        }
    }

    /// Cross-session intelligence — pillars F and H in one pane, with pillar
    /// G's burn tables in the Analytics view. Session-independent: this pane
    /// reads across everything the daemon watches.
    fn insight_tab(&mut self, ui: &mut egui::Ui) {
        let z = self.pane_zoom(ui, "insight");
        scale_text(ui, z);

        ui.horizontal(|ui| {
            for v in InsightView::ALL {
                if ui
                    .selectable_label(self.insight.view == v, v.label())
                    .clicked()
                {
                    self.insight.view = v;
                }
            }
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| match self.insight.view {
                InsightView::Search => self.insight_search_view(ui),
                InsightView::Digest => self.insight_digest_view(ui),
                InsightView::Analytics => self.insight_analytics_view(ui),
                InsightView::Prompts => self.insight_prompts_view(ui),
                InsightView::Failures => self.insight_failures_view(ui),
                InsightView::Decisions => self.insight_decisions_view(ui),
                InsightView::File => self.insight_file_view(ui),
                InsightView::Docs => self.insight_docs_view(ui),
            });
    }

    /// `R-F1`. Enter to search — the corpus is tens of MB and a per-keystroke
    /// scan would punish typing.
    fn insight_search_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.insight.query)
                    .hint_text("search every transcript and all prompt history — Enter to run")
                    .desired_width(420.0),
            );
            let go = (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || ui.button("search").clicked();
            if go && !self.insight.query.trim().is_empty() {
                self.insight.search_pending = true;
                self.net.send(ClientMsg::InsightSearch {
                    query: self.insight.query.trim().to_string(),
                });
            }
            if self.insight.search_pending {
                ui.label(dim("searching…"));
            }
        });

        let mut jump: Option<(String, Option<chrono::DateTime<Utc>>)> = None;
        if let Some((q, r)) = &self.insight.results {
            ui.label(dim(format!(
                "{} hit(s) for \"{}\" across {} file(s){}{}",
                r.hits.len(),
                q,
                r.files_scanned,
                if r.truncated { " — capped, refine the query" } else { "" },
                if r.files_truncated > 0 {
                    " · some large files searched partially"
                } else {
                    ""
                },
            )));
            for h in &r.hits {
                let src = match h.source {
                    mogeung_core::insight::SearchSource::History => "hist",
                    mogeung_core::insight::SearchSource::Prompt => "you",
                    mogeung_core::insight::SearchSource::Transcript => "sess",
                };
                let when = h
                    .timestamp
                    .map(|t| t.format("%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                let row = ui.horizontal(|ui| {
                    ui.label(dim(format!("{src:>4}")));
                    ui.label(dim(format!(
                        "{} L{} {when}",
                        &h.session_id[..8.min(h.session_id.len())],
                        h.line
                    )));
                    ui.add(egui::Label::new(mono(&h.preview).size(11.5)).truncate())
                });
                let resp = row
                    .response
                    .interact(egui::Sense::click())
                    .on_hover_text(match h.source {
                        mogeung_core::insight::SearchSource::History => {
                            "click to copy the prompt"
                        }
                        _ => "click to open the transcript here",
                    });
                if resp.clicked() {
                    match h.source {
                        mogeung_core::insight::SearchSource::History => {
                            ui.ctx().copy_text(h.preview.clone());
                        }
                        _ => jump = Some((h.session_id.clone(), h.timestamp)),
                    }
                }
            }
        }
        if let Some((sid, ts)) = jump {
            if self.sessions.contains_key(&sid) {
                self.selected = Some(sid);
                self.focus_event_ts = ts;
                self.set_tab(Tab::Transcript);
            }
        }
    }

    /// `R-F3`. Evidence for one local day — counts, never narrative.
    fn insight_digest_view(&mut self, ui: &mut egui::Ui) {
        if self.insight.day.is_empty() {
            self.insight.day = chrono::Local::now().format("%Y-%m-%d").to_string();
        }
        let mut fetch = false;
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.insight.day)
                    .desired_width(110.0)
                    .font(egui::TextStyle::Monospace),
            );
            if ui.button("today").clicked() {
                self.insight.day = chrono::Local::now().format("%Y-%m-%d").to_string();
                fetch = true;
            }
            if ui.button("yesterday").clicked() {
                self.insight.day = (chrono::Local::now() - chrono::Duration::days(1))
                    .format("%Y-%m-%d")
                    .to_string();
                fetch = true;
            }
            if ui.button("load").clicked() {
                fetch = true;
            }
            if self.insight.digest_pending {
                ui.label(dim("reading…"));
            }
        });
        let stale = self
            .insight
            .digest
            .as_ref()
            .map(|d| d.day != self.insight.day)
            .unwrap_or(true);
        if fetch || (stale && !self.insight.digest_pending) {
            self.insight.digest_pending = true;
            self.net.send(ClientMsg::FetchDigest {
                day: self.insight.day.clone(),
            });
        }
        let Some(d) = &self.insight.digest else { return };
        if d.sessions.is_empty() {
            ui.label(dim(format!("no session activity on {}", d.day)));
            return;
        }
        ui.label(dim(
            "what the transcripts show — counts and files, never the agents' own summaries",
        ));
        for sd in &d.sessions {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(sd.title.clone().unwrap_or_else(|| sd.session_id.clone()))
                        .size(12.5)
                        .strong(),
                );
                ui.label(dim(&sd.repo));
            });
            ui.label(dim(format!(
                "  {} turn(s) · {} tool call(s) · {} error(s) · {} in / {} out",
                sd.turns,
                sd.tool_calls,
                sd.errors,
                tokens(sd.tokens_in),
                tokens(sd.tokens_out)
            )));
            if !sd.files_touched.is_empty() {
                ui.label(mono(truncate(&sd.files_touched.join("  "), 200)).size(10.5));
            }
        }
    }

    /// `R-F5` + `R-G3`. Prompt-history analytics beside the burn tables the
    /// usage scanner already computes — tokens, never dollars.
    fn insight_analytics_view(&mut self, ui: &mut egui::Ui) {
        if !self.insight.analytics_asked {
            self.insight.analytics_asked = true;
            self.net.send(ClientMsg::FetchAnalytics);
            self.net.send(ClientMsg::FetchUsage);
        }

        if let Some(u) = &self.usage {
            ui.label(RichText::new("token burn").strong().size(12.5));
            ui.label(dim(
                "from the transcripts' own usage records; the window estimate is derived \
                 from past limit hits, never a published quota",
            ));
            ui.label(mono(format!(
                "trailing 5h: {} in / {} out{}",
                tokens(u.window_tokens_in),
                tokens(u.window_tokens_out),
                u.window_fraction()
                    .map(|f| format!("  (≈{:.0}% of est. limit)", f * 100.0))
                    .unwrap_or_default()
            )));
            ui.add_space(4.0);
            ui.label(dim("by day"));
            for d in u.days.iter().take(14) {
                ui.label(mono(format!(
                    "  {}  {:>10} in  {:>9} out  · {} session(s)",
                    d.day,
                    tokens(d.tokens_in),
                    tokens(d.tokens_out),
                    d.sessions
                )));
            }
            ui.add_space(4.0);
            ui.label(dim("by repo"));
            for r in u.repos.iter().take(12) {
                ui.label(mono(format!(
                    "  {:<28} {:>10} in  {:>9} out",
                    truncate(&r.repo, 28),
                    tokens(r.tokens_in),
                    tokens(r.tokens_out)
                )));
            }
            ui.add_space(4.0);
            ui.label(dim("heaviest sessions"));
            for s in u.sessions.iter().take(8) {
                ui.label(mono(format!(
                    "  {}  {:<20} {:>9} out",
                    &s.session_id[..8.min(s.session_id.len())],
                    truncate(&s.repo, 20),
                    tokens(s.tokens_out)
                )));
            }
            if !u.limit_hits.is_empty() {
                ui.add_space(4.0);
                ui.label(dim("limit hits"));
                for h in u.limit_hits.iter().take(6) {
                    ui.label(mono(format!(
                        "  {}  {}{}",
                        h.at.format("%m-%d %H:%M"),
                        &h.session_id[..8.min(h.session_id.len())],
                        h.resets
                            .as_deref()
                            .map(|r| format!("  resets {r}"))
                            .unwrap_or_default()
                    )));
                }
            }
        }

        let Some(a) = &self.insight.analytics else { return };
        ui.add_space(8.0);
        ui.label(RichText::new("prompt history").strong().size(12.5));
        ui.label(dim("prompts per day"));
        for d in a.prompts_per_day.iter().rev().take(14) {
            ui.label(mono(format!("  {}  {:>4}", d.day, d.count)));
        }
        ui.add_space(4.0);
        ui.label(dim("by hour of day"));
        let max = a.hour_histogram.iter().copied().max().unwrap_or(1).max(1);
        for (h, n) in a.hour_histogram.iter().enumerate() {
            if *n == 0 {
                continue;
            }
            let width = 30usize * (*n as usize) / max as usize;
            ui.label(mono(format!("  {h:02}  {:<30} {n}", "=".repeat(width.max(1)))));
        }
    }

    /// `R-F6`.
    fn insight_prompts_view(&mut self, ui: &mut egui::Ui) {
        if !self.insight.prompts_asked {
            self.insight.prompts_asked = true;
            self.net.send(ClientMsg::FetchPromptLibrary);
        }
        let Some(clusters) = &self.insight.prompts else {
            ui.label(dim("reading prompt history…"));
            return;
        };
        if clusters.is_empty() {
            ui.label(dim("no prompt has been reused yet"));
            return;
        }
        ui.label(dim("most-reused prompts — click to copy"));
        for c in clusters.iter().take(50) {
            let row = ui.horizontal(|ui| {
                ui.label(RichText::new(format!("×{}", c.count)).size(11.5).color(pal().blue));
                ui.add(
                    egui::Label::new(RichText::new(truncate(&c.representative, 160)).size(11.5))
                        .truncate(),
                );
            });
            if row
                .response
                .interact(egui::Sense::click())
                .on_hover_text("copy the full prompt")
                .clicked()
            {
                ui.ctx().copy_text(c.representative.clone());
            }
        }
    }

    /// `R-F4`.
    fn insight_failures_view(&mut self, ui: &mut egui::Ui) {
        if !self.insight.failures_asked {
            self.insight.failures_asked = true;
            self.net.send(ClientMsg::FetchRecurring);
        }
        let Some(failures) = &self.insight.failures else {
            ui.label(dim("scanning transcripts…"));
            return;
        };
        if failures.is_empty() {
            ui.label(dim("no error shape recurs across sessions"));
            return;
        }
        ui.label(dim(
            "error shapes seen in two or more sessions — the normalised key is shown so the \
             grouping can be audited",
        ));
        for f in failures {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("×{} in {} session(s)", f.count, f.sessions.len()))
                    .size(11.5)
                    .color(pal().amber),
            );
            ui.label(mono(truncate(&f.example, 200)).size(11.0));
            ui.label(dim(format!("  key: {}", truncate(&f.normalized, 160))));
            ui.label(dim(format!(
                "  sessions: {}",
                f.sessions
                    .iter()
                    .map(|s| &s[..8.min(s.len())])
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }

    /// `R-F7`. Candidates for a human to skim — the pattern that matched is
    /// on every row, and the copy-out is a skeleton, not an ADR.
    fn insight_decisions_view(&mut self, ui: &mut egui::Ui) {
        let Some(s) = self.selected_session().cloned() else {
            ui.label(dim("select a session — decisions are read per transcript"));
            return;
        };
        if !self.insight.decisions_asked.contains(&s.id) {
            self.insight.decisions_asked.insert(s.id.clone());
            self.net.send(ClientMsg::FetchDecisions {
                session_id: s.id.clone(),
            });
        }
        let Some(cands) = self.insight.decisions.get(&s.id) else {
            ui.label(dim("reading transcript…"));
            return;
        };
        if cands.is_empty() {
            ui.label(dim("no decision-shaped sentences found"));
            return;
        }
        ui.horizontal(|ui| {
            ui.label(dim(format!(
                "{} candidate(s) in {} — pattern-matched, not judged",
                cands.len(),
                s.label()
            )));
            if ui
                .button("copy as ADR skeleton")
                .on_hover_text("a docs/decisions draft with these as raw material")
                .clicked()
            {
                let mut md = String::from(
                    "---\ntitle: <decision>\nstatus: draft\nupdated: <date>\n---\n\n\
                     # <NNNN> — <decision>\n\n## Context\n\n## Decision\n\n\
                     ## Raw material (pattern-extracted, verify before trusting)\n\n",
                );
                for c in cands {
                    md.push_str(&format!("- {} _(matched: {})_\n", c.text, c.pattern));
                }
                ui.ctx().copy_text(md);
            }
        });
        for c in cands {
            ui.add_space(3.0);
            ui.label(RichText::new(&c.text).size(11.5));
            ui.label(dim(format!("  L{} · matched: {}", c.line, c.pattern)));
        }
    }

    /// `R-F2`. Prompt-blame: which sessions touched a file, and what they
    /// were told. Heuristic, and each row names its match rule.
    fn insight_file_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.insight.file_query)
                    .hint_text("a file path (or suffix, e.g. src/state.rs) — Enter to look up")
                    .desired_width(420.0)
                    .font(egui::TextStyle::Monospace),
            );
            let go = (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || ui.button("who touched this").clicked();
            if go && !self.insight.file_query.trim().is_empty() {
                self.net.send(ClientMsg::FetchFileSessions {
                    path: self.insight.file_query.trim().to_string(),
                });
            }
        });
        let mut select: Option<String> = None;
        if let Some((path, entries)) = &self.insight.file_results {
            if entries.is_empty() {
                ui.label(dim(format!(
                    "no watched session's edit tools named {path} — sessions predating \
                     mogeung, or edits made outside an agent, are invisible here (A8)"
                )));
            }
            for e in entries {
                ui.add_space(4.0);
                let row = ui.horizontal(|ui| {
                    ui.label(RichText::new(&e.label).size(12.0).strong());
                    if let Some(at) = e.at {
                        ui.label(dim(at.format("%m-%d %H:%M").to_string()));
                    }
                });
                if let Some(p) = &e.prompt {
                    ui.label(RichText::new(format!("  \"{}\"", truncate(p, 160))).size(11.5));
                }
                ui.label(dim(format!("  {}", e.matched)));
                if row
                    .response
                    .interact(egui::Sense::click())
                    .on_hover_text("select this session")
                    .clicked()
                {
                    select = Some(e.session_id.clone());
                }
            }
        }
        if let Some(sid) = select {
            if self.sessions.contains_key(&sid) {
                self.selected = Some(sid);
            }
        }
    }

    /// `R-H1`–`R-H5`. Read-only from end to end: proposals and copy-out,
    /// never a write.
    fn insight_docs_view(&mut self, ui: &mut egui::Ui) {
        let repos = {
            let mut v: Vec<String> = self
                .sessions
                .values()
                .filter_map(|s| s.repo_root.clone())
                .collect();
            v.sort();
            v.dedup();
            v
        };
        if repos.is_empty() {
            ui.label(dim("no watched session is inside a git repo"));
            return;
        }
        if self.insight.docs_repo.is_empty() {
            self.insight.docs_repo = repos[0].clone();
        }
        let mut fetch = false;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("insight-docs-repo")
                .selected_text(truncate(&self.insight.docs_repo, 50))
                .show_ui(ui, |ui| {
                    for r in &repos {
                        if ui
                            .selectable_label(self.insight.docs_repo == *r, r)
                            .clicked()
                        {
                            self.insight.docs_repo = r.clone();
                            fetch = true;
                        }
                    }
                });
            if ui.button("scan").clicked() {
                fetch = true;
            }
            if self.insight.docs_pending {
                ui.label(dim("scanning…"));
            }
        });
        let stale = self
            .insight
            .docs
            .as_ref()
            .map(|(r, _)| r != &self.insight.docs_repo)
            .unwrap_or(true);
        if fetch || (stale && !self.insight.docs_pending) {
            self.insight.docs_pending = true;
            self.net.send(ClientMsg::FetchDocScan {
                repo: self.insight.docs_repo.clone(),
            });
        }
        let Some((_, inv)) = &self.insight.docs else { return };

        ui.label(dim(format!(
            "{} markdown file(s){} — proposals only; mogeung never touches them",
            inv.docs.len(),
            if inv.truncated { " (walk capped)" } else { "" }
        )));

        if !inv.stale.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("stale ({})", inv.stale.len())).strong().size(12.0));
            for st in inv.stale.iter().take(30) {
                ui.label(mono(format!("  {}", st.doc)).size(11.0));
                ui.label(dim(format!(
                    "    references {} — {} commit(s) since the doc's last edit",
                    st.referenced_path, st.commits_since
                )));
            }
        }

        if !inv.gc.is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("gc candidates ({})", inv.gc.len())).strong().size(12.0),
            );
            for g in inv.gc.iter().take(30) {
                ui.horizontal(|ui| {
                    let (word, col) = match g.action {
                        mogeung_core::docs::GcAction::Archive => ("archive", pal().amber),
                        mogeung_core::docs::GcAction::Merge => ("merge", pal().blue),
                        mogeung_core::docs::GcAction::Review => ("review", pal().dim),
                    };
                    ui.label(RichText::new(word).size(10.5).color(col));
                    ui.label(mono(&g.path).size(11.0));
                });
                ui.label(dim(format!("    {}", g.evidence)));
            }
        }

        if !inv.plans.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("plans vs evidence").strong().size(12.0));
            for p in &inv.plans {
                ui.label(mono(format!(
                    "  {} — {} checked / {} open",
                    p.doc, p.checked, p.unchecked
                ))
                .size(11.0));
                for it in p.items.iter().filter(|i| i.claimed_unevidenced).take(10) {
                    ui.label(
                        RichText::new(format!("    ☐ claimed, unevidenced: {}", truncate(&it.text, 120)))
                            .size(11.0)
                            .color(pal().amber),
                    );
                }
            }
        }

        if let Some(drift) = &inv.instruction_drift {
            if !drift.pairs.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("instruction files").strong().size(12.0));
                for pair in &drift.pairs {
                    ui.label(mono(format!(
                        "  {} vs {} — {:.0}% shared",
                        pair.a,
                        pair.b,
                        pair.shared_ratio * 100.0
                    ))
                    .size(11.0));
                    for l in pair.only_in_a.iter().take(8) {
                        ui.label(dim(format!("    only in {}: {}", pair.a, truncate(l, 110))));
                    }
                    for l in pair.only_in_b.iter().take(8) {
                        ui.label(dim(format!("    only in {}: {}", pair.b, truncate(l, 110))));
                    }
                }
                for f in &drift.files {
                    if ui
                        .button(format!("copy {} for hand-pasting", f.path))
                        .on_hover_text(
                            "reads the file from this machine; mogeung writes nothing anywhere",
                        )
                        .clicked()
                    {
                        let full = std::path::Path::new(&self.insight.docs_repo).join(&f.path);
                        match std::fs::read_to_string(&full) {
                            Ok(text) => ui.ctx().copy_text(text),
                            Err(_) => self.errors.push(format!(
                                "could not read {} from this machine — remote daemon?",
                                full.display()
                            )),
                        }
                    }
                }
            }
        }
    }
}

/// Shorten a path to its last directory plus filename.
///
/// `crates/mogeungd/src/state.rs` → `…/src/state.rs`. The tail is what
/// identifies a file; the leading directories are the same for most of the
/// list and just push the useful part off the edge. Full path is on hover.
/// The commit file tree (`R-D18`): directories as collapsible headers,
/// files as selectable rows colored the IntelliJ way — green added, blue
/// modified, red deleted, amber renamed. A click lands in `pick`; the
/// root "all files" row lives with the caller.
fn render_file_tree(
    ui: &mut egui::Ui,
    nodes: &[crate::gitview::TreeNode],
    files: &[mogeung_core::change::FileChange],
    focus_idx: Option<usize>,
    path: &str,
    pick: &mut Option<Option<usize>>,
) {
    use mogeung_core::change::FileStatus;
    for n in nodes {
        match n.file {
            None => {
                let full = format!("{path}/{}", n.label);
                egui::CollapsingHeader::new(dim(format!("{}/", n.label)))
                    .id_salt(("git-tree", &full))
                    .default_open(true)
                    .show(ui, |ui| {
                        render_file_tree(ui, &n.children, files, focus_idx, &full, pick);
                    });
            }
            Some(i) => {
                let f = &files[i];
                let color = match f.status {
                    FileStatus::Added => pal().green,
                    FileStatus::Modified => pal().blue,
                    FileStatus::Deleted => pal().red,
                    FileStatus::Renamed => pal().amber,
                };
                let row = ui.horizontal(|ui| {
                    let resp = ui.selectable_label(
                        focus_idx == Some(i),
                        RichText::new(&n.label).monospace().size(11.5).color(color),
                    );
                    ui.label(dim(format!("+{} −{}", f.insertions, f.deletions)));
                    if !f.hunks.is_empty() && f.hunks.iter().all(|h| h.reviewed) {
                        ui.label(RichText::new(icon::READ).size(10.0).color(pal().green))
                            .on_hover_text("every hunk marked read");
                    }
                    resp
                });
                if row.inner.on_hover_text(&f.path).clicked() {
                    *pick = Some(Some(i));
                }
            }
        }
    }
}

/// New-side line numbers a diff's hunks touched — the Editor's gutter data.
/// Parses the hunk header (`@@ -a,b +c,d @@`) and walks the signed lines,
/// the same arithmetic the daemon's coverage intersection uses. `R-B27`.
fn changed_lines_of(files: &[mogeung_core::FileChange]) -> Vec<u32> {
    let mut out = Vec::new();
    for f in files {
        for h in &f.hunks {
            let Some(start) = h
                .header
                .split('+')
                .nth(1)
                .and_then(|p| p.split([',', ' ', '@']).next())
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let mut line = start;
            for l in &h.lines {
                match l.as_bytes().first() {
                    Some(b'+') => {
                        out.push(line);
                        line += 1;
                    }
                    Some(b'-') => {}
                    _ => line += 1,
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The identifier around a char index, for click-to-highlight. `R-B28`.
fn word_at_char_index(content: &str, idx: usize) -> Option<String> {
    let chars: Vec<char> = content.chars().collect();
    let is_word = |c: &char| c.is_alphanumeric() || *c == '_';
    let at = chars.get(idx).filter(|c| is_word(c))?;
    let _ = at;
    let mut start = idx;
    while start > 0 && is_word(&chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx;
    while end + 1 < chars.len() && is_word(&chars[end + 1]) {
        end += 1;
    }
    let word: String = chars[start..=end].iter().collect();
    (word.chars().count() >= 2).then_some(word)
}

/// One line of it, whitespace collapsed. `R-F13`.
///
/// A search result is a row, and a matched transcript turn is prose with
/// newlines in it — pasted straight into a row it would be six rows, and the
/// list would stop being scannable at the first paragraph.
fn one_line(s: &str, n: usize) -> String {
    truncate(&s.split_whitespace().collect::<Vec<_>>().join(" "), n)
}

/// One result group of the search panel. `R-F13`.
fn search_group(ui: &mut egui::Ui, salt: &str, head: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(RichText::new(head).size(11.5).strong())
        .id_salt(("search-group", salt))
        .default_open(true)
        .show(ui, |ui| {
            // Truncate rather than wrap: a rail is narrow, and a result that
            // folded onto three lines would cost more room than the hit is
            // worth. The preview pane is where the whole of it lives.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            ui.spacing_mut().item_spacing.y = 1.0;
            body(ui);
        });
}

/// One result row: where it is, then what matched.
fn search_row(
    ui: &mut egui::Ui,
    picked: bool,
    scroll: bool,
    tag: &str,
    text: &str,
) -> egui::Response {
    let r = ui.selectable_label(
        picked,
        mono(format!("{tag}  {}", one_line(text, 240))).size(11.0),
    );
    if scroll {
        r.scroll_to_me(Some(egui::Align::Center));
    }
    r
}

/// A group's header: what it searched, and what state its answer is in.
///
/// The three states are kept apart deliberately (`R-J5`): still running,
/// answered with nothing, and never asked are three different things, and
/// flattening them is how a search comes to look broken.
fn group_head<T>(name: &str, g: &Group<T>, off: Option<&str>) -> String {
    if let Some(why) = off {
        return format!("{name} · {why}");
    }
    if g.running {
        return format!("{name} · searching…");
    }
    match g.count() {
        Some(n) => format!("{name} · {n} hit(s)"),
        None => format!("{name} · press Enter"),
    }
}

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
        (icon::READ, pal().green)
    } else {
        (icon::UNREAD, risk_color(risk))
    };
    job.append(&format!("{marker} "), 0.0, mono_fmt(11.0, marker_color));

    let (dir, base) = short_path(&f.path);
    let dim_text = if read { pal().read_dim } else { pal().dim };
    let base_text = if read { pal().read_base } else { pal().unread_base };
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
        Tok::Keyword => pal().syn_keyword,
        Tok::Str => pal().syn_string,
        Tok::Comment => pal().syn_comment,
        Tok::Number => pal().syn_number,
        Tok::Type => pal().syn_type,
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
        return Some(pal().conflict_bg);
    }
    match line.chars().next() {
        Some('+') => Some(pal().add_bg),
        Some('-') => Some(pal().del_bg),
        _ => None,
    }
}

fn line_fg(line: &str) -> Color32 {
    match line.chars().next() {
        Some('+') => pal().add_fg,
        Some('-') => pal().del_fg,
        _ => pal().ctx_fg,
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

    // Not wrapped, since `R-J2`: a wrapped line is of unknown height, and a row
    // of unknown height cannot be skipped without laying it out — which is the
    // whole cost being avoided. Long lines run off the side and the pane
    // scrolls to them, which is what a diff viewer does and what the panes
    // around this one already did (`8795688`).
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

        // Word-diff emphasis takes precedence: knowing *what changed* beats
        // knowing what is a keyword.
        if let Some(spans) = emphasis {
            for sp in spans {
                let mut t = RichText::new(&sp.text).monospace().size(size).color(fg);
                if sp.changed {
                    t = t.background_color(if line.starts_with('+') {
                        pal().add_emph
                    } else {
                        pal().del_emph
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

/// One diff row's height, known without laying the row out.
///
/// Exact rather than estimated, which is what makes `visible_range` safe:
/// diff rows hold a single line of monospace text and never wrap, so the row
/// height is the font's and nothing else. Vertical item spacing is zeroed by
/// the renderers, so `n` lines occupy exactly `n * row_height`.
fn diff_row_height(ui: &egui::Ui) -> f32 {
    let font = egui::FontId::monospace(diff_size(ui));
    ui.fonts_mut(|f| f.row_height(&font))
}

/// Which of `n` rows can be seen, given where the cursor sits inside the
/// enclosing scroll area. `R-J2`.
///
/// Two rows of margin either side, so a scroll that lands between frames
/// cannot show a gap. Returns an empty range when the block is entirely
/// off-screen — a diff whose file is scrolled past costs nothing at all.
fn visible_range(ui: &egui::Ui, n: usize, row_h: f32) -> std::ops::Range<usize> {
    let clip = ui.clip_rect();
    // An unbounded clip rect means nothing is clipping — draw everything, or a
    // measuring pass would silently render nothing.
    if !clip.is_finite() {
        return 0..n;
    }
    visible_window(ui.cursor().top(), clip.height(), clip.top(), n, row_h)
}

/// The arithmetic behind `visible_range`, without a `Ui` in the way.
///
/// `top` is where row zero sits, in the same coordinates as `view_top` — which
/// means it goes negative as the pane scrolls down past it.
fn visible_window(
    top: f32,
    view_height: f32,
    view_top: f32,
    n: usize,
    row_h: f32,
) -> std::ops::Range<usize> {
    if n == 0 || row_h <= 0.0 || !top.is_finite() {
        return 0..0;
    }
    let first = ((view_top - top) / row_h).floor() as i64 - 2;
    let last = ((view_top + view_height - top) / row_h).ceil() as i64 + 2;
    let n = n as i64;
    let start = first.clamp(0, n) as usize;
    let end = last.clamp(0, n) as usize;
    start..end.max(start)
}

/// Unified view, with word-level emphasis on runs that look like replacements.
///
/// Draws only the rows the pane can show, and reserves the rest as empty
/// space. Before `R-J2` this laid out every line of every hunk each frame —
/// 30ms on the largest commit in this repo, which is 33fps while scrolling the
/// diff of a session that touched a lot of files.
fn render_unified(ui: &mut egui::Ui, lines: &[String], syntax: bool, words: bool) {
    let partners = replacement_partners(lines, words);
    let row_h = diff_row_height(ui);
    ui.spacing_mut().item_spacing.y = 0.0;

    let range = visible_range(ui, lines.len(), row_h);
    if range.start > 0 {
        ui.add_space(range.start as f32 * row_h);
    }
    for i in range.clone() {
        // The word diff is computed for drawn lines only. Computing it for the
        // whole hunk was most of the old per-frame cost on a large replacement
        // run, and all of it was thrown away for lines nobody could see.
        let emphasis = partners.get(&i).map(|&j| word_emphasis(lines, i, j));
        styled_line(ui, &lines[i], syntax, emphasis.as_deref());
    }
    if range.end < lines.len() {
        ui.add_space((lines.len() - range.end) as f32 * row_h);
    }
}

/// The word-diff spans for line `i`, whose replacement counterpart is `j`.
fn word_emphasis(lines: &[String], i: usize, j: usize) -> Vec<crate::diff::Span> {
    let (del, add) = if lines[i].starts_with('-') { (i, j) } else { (j, i) };
    let (l, r) = crate::diff::word_diff(&lines[del], &lines[add]);
    if i == del { l } else { r }
}

fn render_side_by_side(ui: &mut egui::Ui, lines: &[String], syntax: bool, words: bool) {
    let rows = crate::diff::side_by_side(lines);
    let half = (ui.available_width() - 12.0).max(160.0) / 2.0;
    let row_h = diff_row_height(ui);
    ui.spacing_mut().item_spacing.y = 0.0;
    let range = visible_range(ui, rows.len(), row_h);
    if range.start > 0 {
        ui.add_space(range.start as f32 * row_h);
    }
    for row in &rows[range.clone()] {
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
    if range.end < rows.len() {
        ui.add_space((rows.len() - range.end) as f32 * row_h);
    }
}

/// Index → the index of its counterpart, for lines that are half of a
/// replacement pair.
///
/// A run of N removals followed by N additions is treated as N replacements;
/// anything lopsided is left alone, because pairing lines that are not really
/// counterparts produces noise rather than insight.
///
/// This used to return the word-diff spans themselves, computed for every pair
/// in the hunk. It now returns only the pairing, which is an index scan: the
/// spans are computed at draw time for the handful of lines actually on
/// screen. `R-J2`.
fn replacement_partners(lines: &[String], enabled: bool) -> HashMap<usize, usize> {
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
                out.insert(del_start + k, add_start + k);
                out.insert(add_start + k, del_start + k);
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
                            ui.label(badge("test", pal().green));
                        }
                        ui.label(mono(format!("{}:{}", r.path, r.line)).color(pal().dim));
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

/// The searchable text of one turn. `R-B36`.
///
/// Prose and prompts are what people look for; a tool call contributes its
/// name and its detail, because "when did it touch config.rs" is the other
/// half of what searching a transcript is for.
fn event_text(kind: &EventKind) -> String {
    match kind {
        EventKind::UserPrompt { text }
        | EventKind::AssistantText { text }
        | EventKind::Thinking { text } => text.clone(),
        other => format!("{other:?}"),
    }
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
                ui.label(badge("init", pal().dim));
                ui.label(dim(format!("{model} · {tool_count} tools · {cwd}")));
            });
        }
        EventKind::UserPrompt { text } => {
            message_header(ui, &time, "you", pal().blue, text);
            ui.indent(ev.seq, |ui| prose(ui, text, cache, prefs));
            ui.add_space(4.0);
        }
        EventKind::AssistantText { text } => {
            message_header(ui, &time, "agent", pal().green, text);
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
                ui.label(badge(name, pal().purple));
                ui.label(mono(truncate(summary, 150)));
            });
        }
        EventKind::ToolResult {
            is_error, preview, ..
        } => {
            let head = if *is_error { "error" } else { "result" };
            let color = if *is_error { pal().red } else { pal().dim };
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
                    if *is_error { pal().red } else { pal().green },
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
                NoticeLevel::Info => pal().dim,
                NoticeLevel::Warn => pal().amber,
                NoticeLevel::Error => pal().red,
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
                    RichText::new("Opens a real interactive claude in your terminal app.")
                        .size(12.5),
                );
                ui.label(dim(
                    "mogeung does not wrap the conversation — you drive it exactly as usual, and it shows up in the queue.",
                ));
                // Said here rather than left to be discovered. The flag is the
                // agent's own and mogeung only passes it, but it changes what
                // this button costs, and a permission prompt that never comes
                // is not something anyone should meet by surprise.
                ui.label(
                    RichText::new(
                        "Started in yolo mode: --dangerously-skip-permissions. The agent \
                         will not ask before editing files or running commands.",
                    )
                    .color(pal().amber)
                    .size(12.0),
                );
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
            // R-I4: against a remote daemon this would open a terminal on
            // the other machine — refuse rather than surprise.
            if self.remote_daemon() {
                self.errors.push(format!(
                    "remote daemon — launching a terminal would open it on {}",
                    self.other_machine()
                ));
            } else {
                self.net.send(ClientMsg::LaunchTerminal {
                    dir: self.launch_dir.trim().to_string(),
                    worktree: self.launch_worktree,
                });
            }
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
                                    ui.label(mono(&f.path).color(pal().dim));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("×").clicked() {
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
                        ui.label(RichText::new("All clear").size(48.0).color(pal().green));
                        ui.label(
                            RichText::new(format!(
                                "{} live session(s) working",
                                self.sessions.values().filter(|s| s.alive).count()
                            ))
                            .size(20.0)
                            .color(pal().dim),
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
                                                .color(pal().dim),
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
                                    .color(pal().dim),
                                );
                                if !s.collisions.is_empty() {
                                    ui.label(
                                        RichText::new("⚠ COLLISION").size(18.0).color(pal().red).strong(),
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
                                    Mode::Search => "search in files — ↩ runs it…",
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
                                None => Some("type a query and press ↩"),
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
                                ui.label(dim(format!("showing \"{answered}\" — ↩ searches again")));
                            }
                            if *truncated {
                                ui.label(
                                    RichText::new("cut short at the match cap — narrow the query")
                                        .size(11.0)
                                        .color(pal().amber),
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
                                    Mode::Actions => "⬆⬇ move   ↩ run   esc close",
                                    Mode::Files => "⬆⬇ move   ↩ open   esc close",
                                    Mode::Search => "⬆⬇ move   ↩ search / open   esc close",
                                })
                                .size(10.5)
                                .color(pal().dim),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("{} of {}",
                                            if len == 0 { 0 } else { self.palette.cursor + 1 },
                                            len))
                                            .size(10.5)
                                            .color(pal().dim),
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
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Theme:").size(12.5));
                    for m in crate::theme::Mode::ALL {
                        if ui
                            .selectable_value(&mut self.prefs.theme, m, m.label())
                            .changed()
                        {
                            self.prefs_dirty = true;
                        }
                    }
                    ui.label(dim(format!(
                        "· {} cycles",
                        self.keymap.describe(crate::keymap::Action::CycleTheme)
                    )));
                });
                ui.label(dim(
                    "system follows the desktop, and falls back to dark where the \
                     desktop will not say — which is most Linux sessions.",
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
                                    .color(pal().amber)
                                    .strong(),
                            );
                        } else {
                            ui.label(
                                RichText::new("⬆⬇ move   ↩ rebind   backspace reset   / search")
                                    .size(11.0)
                                    .color(pal().dim),
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
                    ui.label(RichText::new("Nothing unaccounted for.").color(pal().green));
                } else {
                    for a in &h.alerts {
                        let colour = if a.is_urgent() { pal().amber } else { pal().dim };
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
                        row("unknown type", h.lines_unknown, Some(pal().amber));
                        row("unreadable", h.lines_malformed, Some(pal().red));
                        ui.label(dim("total seen"));
                        ui.label(mono(h.lines_seen.to_string()));
                        ui.end_row();
                    });

                if !h.unknown_types.is_empty() {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Types mogeung does not understand").color(pal().amber).strong().size(12.5));
                    for (ty, n) in &h.unknown_types {
                        ui.label(mono(format!("  {ty}  ×{n}")).color(pal().amber));
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

                // R-I1: present-but-empty is information, absence is silence.
                if h.codex_present {
                    ui.add_space(10.0);
                    ui.label(RichText::new("Codex").strong().size(12.5));
                    match &h.codex_error {
                        Some(e) => {
                            ui.label(RichText::new(format!("  index unreadable: {e}"))
                                .size(11.5)
                                .color(pal().red));
                        }
                        None => {
                            ui.label(dim(format!(
                                "  present, watched — {} session(s)",
                                h.codex_threads
                            )));
                            ui.label(dim(
                                "  waiting/working derived from the rollout tail — a heuristic",
                            ));
                        }
                    }
                    for (kind, n) in &h.codex_unknown {
                        ui.label(RichText::new(format!("  unknown kind {kind} ×{n}"))
                            .size(11.5)
                            .color(pal().amber));
                    }
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
    use super::{group_head, keymap_row, may_toggle_hidden, one_line, short_path, Group};

    use super::{find_target, pty_chord, FindTarget, PtyChord, ScrollRequest, Tab};

    /// `Alt+F12` is a *toggle*, and a toggle you can only press once is not
    /// one: the same key that opens the terminal hands it the keyboard, so
    /// unless it is honoured from inside the shell, putting the panel away
    /// means reaching for the mouse — and the keystroke lands in your prompt
    /// as an escape sequence on the way past.
    ///
    /// The other half matters just as much: everything *else* must reach the
    /// pty. `Alt+1` switches tabs out here and may mean something to the
    /// program in there, and a terminal that swallows shortcuts is a terminal
    /// you cannot run a terminal program in.
    #[test]
    fn only_two_chords_are_taken_from_a_focused_terminal() {
        use crate::keymap::Action as A;
        assert_eq!(pty_chord(Some(A::ToggleTerminalFocus)), PtyChord::Leave);
        assert_eq!(pty_chord(Some(A::TabTerminal)), PtyChord::ToggleTerminal);
        assert_eq!(pty_chord(None), PtyChord::Pass, "an unbound key is typing");

        for a in A::ALL {
            if matches!(a, A::ToggleTerminalFocus | A::TabTerminal) {
                continue;
            }
            assert_eq!(
                pty_chord(Some(*a)),
                PtyChord::Pass,
                "{a:?} would be stolen from the shell"
            );
        }
    }

    /// `R-F13`. Four states, four different sentences.
    ///
    /// The one this exists to protect is the middle pair: *asked, and there
    /// is nothing* and *never asked* are not the same, and a group that
    /// flattened them into an empty list would answer "no hits" to a question
    /// nobody has pressed Enter on yet. That is how a search comes to look
    /// broken while working perfectly. `R-J5`, in a panel with three of them
    /// side by side and no room to explain itself.
    #[test]
    fn a_group_never_asked_does_not_claim_to_have_found_nothing() {
        let mut g: Group<u8> = Group::default();
        assert!(
            group_head("files", &g, None).contains("Enter"),
            "never asked must invite the question, not answer it"
        );

        g.asked();
        assert!(group_head("files", &g, None).contains("searching"));

        g.running = false;
        g.hits = Some(Vec::new());
        assert!(
            group_head("files", &g, None).contains("0 hit"),
            "answered-with-nothing is a finding, and reads as one"
        );

        g.hits = Some(vec![1, 2, 3]);
        assert!(group_head("files", &g, None).contains("3 hit"));

        // Switched off wins over everything: a group nobody asked for should
        // not report on a query it never ran.
        assert!(group_head("files", &g, Some("off")).contains("off"));

        // ...and being asked again forgets the old answer, rather than
        // showing yesterday's hits under today's query.
        g.asked();
        assert_eq!(g.count(), None);
    }

    /// `R-F13`. The rule that makes one Enter key mean two things safely:
    /// while the arrows are in the query box, Enter runs the search; once they
    /// have walked into the results, Enter opens the row. So "am I in the
    /// list" has to be exactly right at both boundaries.
    ///
    /// The boundary that matters most is the top. Off the top must **leave**
    /// the list rather than wrap, or there is no keyboard way back to the
    /// query you are in the middle of refining — you would have to reach for
    /// the mouse to edit the thing you are typing.
    #[test]
    fn the_search_arrows_can_always_get_back_to_the_query() {
        use super::search_move;
        // Down from the box enters at the top.
        assert_eq!(search_move(false, None, 5, true), (true, Some(0)));
        // ...and from a stale mouse selection, still at the top.
        assert_eq!(search_move(false, Some(3), 5, true), (true, Some(0)));
        assert_eq!(search_move(true, Some(0), 5, true), (true, Some(1)));
        // The bottom stops rather than wrapping to the top.
        assert_eq!(search_move(true, Some(4), 5, true), (true, Some(4)));
        assert_eq!(search_move(true, Some(2), 5, false), (true, Some(1)));
        // The top hands the arrows back, keeping what was selected.
        assert_eq!(search_move(true, Some(0), 5, false), (false, Some(0)));
        // Up while already in the box is not a way *into* the list.
        assert_eq!(search_move(false, Some(2), 5, false), (false, Some(2)));
        // Nothing to walk: never claim to be in a list that is not there, or
        // Enter would silently stop running searches.
        assert_eq!(search_move(true, Some(0), 0, true), (false, None));
        assert_eq!(search_move(false, None, 0, false), (false, None));
    }

    /// A result is a row. Transcript turns are prose with newlines in them,
    /// and pasted in raw one hit would take six lines of a narrow rail.
    #[test]
    fn a_search_row_is_one_line() {
        let messy = "first line\n\n   second   line\twith\ttabs\n";
        let out = one_line(messy, 200);
        assert!(!out.contains('\n'), "newlines survived into a row: {out:?}");
        assert_eq!(out, "first line second line with tabs");
        assert!(one_line(&"x".repeat(500), 40).chars().count() <= 41);
    }

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

    /// Ctrl+F follows the keyboard. With the Git pane focused it must reach
    /// the log filter, not yank the Editor forward — that misfire is exactly
    /// what got reported. Every other pane keeps the Editor find, including
    /// the Editor itself.
    #[test]
    fn find_goes_where_the_keyboard_is_aimed() {
        assert_eq!(find_target(Tab::Git), FindTarget::GitFilter);
        assert_eq!(find_target(Tab::Explorer), FindTarget::EditorFind);
        for other in [Tab::Changes, Tab::Transcript, Tab::Info, Tab::Debt, Tab::Agent] {
            assert_eq!(find_target(other), FindTarget::EditorFind);
        }
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

    /// `R-J2`. A diff must cost what is on screen, not what is in the file.
    ///
    /// Measured before this: 30ms per frame for the largest commit in this
    /// repo (7,399 lines), which is 33fps while scrolling. The renderer laid
    /// out every line of every hunk, visible or not, and computed a word diff
    /// for every replacement pair to throw almost all of it away.
    ///
    /// Drives the real renderer headlessly and counts the text it actually
    /// paints. A regression here is silent — the diff still looks right, it is
    /// merely slow — so the count is the only thing that catches it.
    #[test]
    fn a_diff_paints_only_the_rows_the_pane_can_show() {
        fn text_shapes(shapes: &[egui::epaint::ClippedShape]) -> usize {
            fn count(s: &egui::Shape) -> usize {
                match s {
                    egui::Shape::Text(_) => 1,
                    egui::Shape::Vec(v) => v.iter().map(count).sum(),
                    _ => 0,
                }
            }
            shapes.iter().map(|c| count(&c.shape)).sum()
        }

        let lines: Vec<String> = (0..4000)
            .map(|i| format!("+    let value_{i} = compute(i);"))
            .collect();
        let ctx = egui::Context::default();
        let mut painted = 0;
        let mut content_height = 0.0f32;
        for _ in 0..2 {
            // Twice: the first frame has no scroll state, and it is the second
            // that reports the geometry the user actually scrolls.
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let r = egui::ScrollArea::both()
                        .max_height(600.0)
                        .show(ui, |ui| super::render_unified(ui, &lines, false, true));
                    content_height = r.content_size.y;
                },
            );
            painted = text_shapes(&out.shapes);
        }

        // 600px of pane over a ~14px row is about 43 rows, plus the two rows
        // of margin either side. An order of magnitude of slack, because the
        // exact number moves with the default font and is not the point.
        assert!(
            painted < 200,
            "{painted} text shapes for 4000 lines — the renderer is drawing what \
             nobody can see"
        );
        assert!(painted > 10, "nothing was drawn at all: {painted}");

        // And the space skipped rows leave behind is real: the scrollbar must
        // describe the whole diff, not the visible slice.
        assert!(
            content_height > 4000.0 * 8.0,
            "content height {content_height} does not account for 4000 rows"
        );
    }

    /// The rows drawn must be the rows under the viewport. Scrolled to the
    /// bottom, the last line is on screen and the first is not — the failure
    /// this guards against is culling that always keeps the top of the list,
    /// which looks fine until you scroll.
    #[test]
    fn the_visible_window_follows_the_scroll() {
        let row_h = 14.0;
        let n = 1000;

        // Top of the list: the window starts at zero and covers the viewport.
        let top = super::visible_window(0.0, 600.0, 0.0, n, row_h);
        assert_eq!(top.start, 0);
        assert!(top.end >= 42 && top.end < 60, "{top:?}");

        // Scrolled past 500 rows: the window has moved with it, and the rows
        // above are not drawn.
        let mid = super::visible_window(-500.0 * row_h, 600.0, 0.0, n, row_h);
        assert!(mid.start >= 495 && mid.start <= 500, "{mid:?}");
        assert!(mid.end > mid.start && mid.end <= 550, "{mid:?}");

        // Entirely scrolled past: nothing to draw, and no panic on the
        // out-of-range arithmetic that gets there.
        let gone = super::visible_window(-100_000.0, 600.0, 0.0, n, row_h);
        assert!(gone.is_empty(), "{gone:?}");
        assert!(gone.end <= n, "range must stay inside the slice: {gone:?}");
    }

    /// Pairing is an index scan now; the spans come later, per drawn line.
    /// Both halves must still point at each other, or the word diff on screen
    /// compares a line with the wrong counterpart.
    #[test]
    fn replacement_runs_pair_both_ways_and_lopsided_ones_do_not_pair() {
        let lines: Vec<String> = ["-a", "-b", "+A", "+B", " ctx", "-only"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = super::replacement_partners(&lines, true);
        assert_eq!(p.get(&0), Some(&2));
        assert_eq!(p.get(&2), Some(&0));
        assert_eq!(p.get(&1), Some(&3));
        assert_eq!(p.get(&3), Some(&1));
        assert_eq!(p.get(&5), None, "a removal with no addition has no partner");
        assert!(super::replacement_partners(&lines, false).is_empty());

        // And the spans land on the right side: the deletion gets the left.
        let del = super::word_emphasis(&lines, 0, 2);
        let add = super::word_emphasis(&lines, 2, 0);
        assert!(!del.is_empty() && !add.is_empty());
        assert_ne!(
            del.iter().map(|s| s.text.clone()).collect::<String>(),
            add.iter().map(|s| s.text.clone()).collect::<String>(),
        );
    }
}


// ---------------------------------------------------------------------------
// Connections — which daemon this window is watching. `R-I7`.
// ---------------------------------------------------------------------------

impl App {
    /// Add, name, switch and forget daemons without a restart.
    ///
    /// The rule the layout follows: the list is written on change, not on
    /// every frame. Unlike the layout there is no drag to debounce, so a save
    /// per edit is exactly right.
    fn git_fetch_popup_entry(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        self.git_fetch_popup(&ctx);
    }

    fn connections_window(&mut self, root: &mut egui::Ui) {
        // The sync report is drawn here rather than in the Git tab: `Ctrl+T`
        // works from anywhere, so its answer must appear anywhere too.
        self.git_fetch_popup_entry(root);
        if !self.show_connections {
            return;
        }
        let ctx = root.ctx().clone();
        let mut open = true;
        let mut save = false;
        let mut switch: Option<crate::connections::Connection> = None;
        let mut remove: Option<usize> = None;
        // Taken for the frame, like the tab rename: the draft is state on
        // `self`, and the list below holds `self` borrowed to draw it.
        let mut draft_slot = self.conn_draft.take();
        let mut reopen: Option<(Option<usize>, crate::connections::Connection)> = None;
        let mut commit: Option<(Option<usize>, crate::connections::Connection)> = None;
        let mut cancel = false;
        // Subscribe for as long as the window is open, and merge whatever has
        // arrived before drawing, so a sighting shows on the frame it lands.
        if self.scan.is_none() {
            match mogeungd::discovery::Watcher::start() {
                Ok(w) => {
                    self.scan = Some(w);
                    self.scan_since = Some(std::time::Instant::now());
                }
                // No multicast here — a container, a locked-down interface.
                // Say so once rather than showing an empty list for ever.
                Err(e) => self.errors.push(format!("cannot browse the network: {e}")),
            }
        }
        if let Some(w) = &self.scan {
            for f in w.poll() {
                match self.scanned.iter_mut().find(|e| e.name == f.name) {
                    Some(existing) => existing.absorb(f),
                    None => self.scanned.push(f),
                }
            }
            self.scanned.sort_by(|a, b| a.name.cmp(&b.name));
            // Answers arrive between frames, and an idle window repaints about
            // once a second — too slow to feel like it is listening.
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        // Small shapes shared by the three lists below. Nested rather than
        // free functions: nothing else in the window draws a list like this,
        // and whoever changes a card should find it beside the rows.

        /// The Body size *in this scope*, after the `scale_text` below.
        ///
        /// The window's own sizes derive from it rather than being written
        /// down, for the reason `diff_size` does the same: `dim` and `mono`
        /// bake 12.0 into themselves, so a label built from them cannot be
        /// scaled by anything outside them — and a dialog that scales half its
        /// text looks worse than one that scales none of it.
        fn body_size(ui: &egui::Ui) -> f32 {
            ui.style()
                .text_styles
                .get(&egui::TextStyle::Body)
                .map(|f| f.size)
                .unwrap_or(13.0)
        }

        /// Secondary text — the `dim` of this window, but scalable.
        fn soft(ui: &egui::Ui, text: impl Into<String>) -> RichText {
            RichText::new(text.into())
                .color(pal().dim)
                .size(body_size(ui) - 1.0)
        }

        /// An address. Monospace so a host and port can be compared by eye.
        fn addr(ui: &egui::Ui, text: impl Into<String>) -> RichText {
            RichText::new(text.into())
                .monospace()
                .color(pal().dim)
                .size(body_size(ui) - 1.0)
        }

        fn section(ui: &mut egui::Ui, title: &str, note: &str) {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(title)
                        .strong()
                        .size(body_size(ui) - 1.0)
                        .color(pal().text_strong),
                );
                if !note.is_empty() {
                    ui.label(soft(ui, note));
                }
            });
            ui.add_space(3.0);
        }

        /// One daemon, as a card: name and buttons on top, address beneath.
        ///
        /// The old list was a run of `ui.horizontal`s, which gave a name, a
        /// URL, three suffixes and three buttons the same weight on one line —
        /// so the thing you pick by (the name) and the thing you are picking
        /// between (which machine) had to be read out of the middle of a
        /// sentence. The fill and the stroke say which one you are on without
        /// reading anything at all.
        fn card<R>(ui: &mut egui::Ui, live: bool, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
            let out = egui::Frame::NONE
                .fill(if live { pal().bg_raised } else { Color32::TRANSPARENT })
                .stroke(egui::Stroke::new(
                    1.0,
                    if live { pal().green } else { pal().border },
                ))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(10, 7))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    body(ui)
                });
            ui.add_space(4.0);
            out.inner
        }

        let current = self.net.url.clone();
        let local = crate::connections::Connection::local();
        let local_live = current.starts_with(&local.url);

        // What to call the daemon we are on, for the header. Its saved name if
        // it has one — the URL is the fallback, not the answer, because the
        // whole reason rows carry names is that a URL is a poor thing to
        // recognise a machine by under time pressure.
        let current_name = if local_live {
            local.label().to_string()
        } else {
            self.connections
                .list
                .iter()
                .find(|c| current.starts_with(&c.url))
                .map(|c| c.label().to_string())
                .unwrap_or_else(|| current.clone())
        };
        let current_detail = match &self.daemon_identity {
            Some(id) => format!(
                "{} on {} · mogeungd {} · pid {}",
                id.claude_home,
                id.label(),
                id.version,
                id.pid
            ),
            // Either the first snapshot has not landed or the daemon predates
            // R-I5. Say which is possible rather than leaving a blank line
            // where every other daemon shows its provenance.
            None => "identity not published — the first snapshot may still be in flight".into(),
        };

        // Our own daemon, seen from outside, is dropped before the count as
        // well as before the rows: offering it back would invite a second
        // connection to the machine we are already on, and reporting "1 found"
        // above an empty list is worse than either.
        let found: Vec<(&mogeungd::discovery::Found, String)> = self
            .scanned
            .iter()
            .filter(|f| !(f.machine_id.is_some() && f.machine_id == self.this_machine))
            .filter_map(|f| f.url().map(|u| (f, u)))
            .collect();
        let scan_note = if self.scan.is_none() {
            "not listening".to_string()
        } else if !found.is_empty() {
            format!("listening · {} found", found.len())
        } else if self.scan_says_waiting() {
            "listening…".to_string()
        } else {
            "listening · nothing yet".to_string()
        };

        egui::Window::new("Daemons")
            .open(&mut open)
            .default_width(560.0)
            .collapsible(false)
            .show(&ctx, |ui| {
                // A dialog you read across a desk and act on once, not body
                // text you sit in — and it was inheriting the panes' sizes,
                // which are tuned for density in a window full of diffs.
                // Scoped to this `Ui`, so nothing behind the dialog moves.
                scale_text(ui, 1.15);

                // --- Where you are ------------------------------------------
                // At the top, not the footnote it used to be at the bottom:
                // every row below asks where to go next, and none of them can
                // be answered without knowing where you already are.
                egui::Frame::NONE
                    .fill(pal().bg_raised)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon::DOT).color(if self.net.connected {
                                pal().green
                            } else {
                                pal().red
                            }));
                            ui.label(soft(ui, "watching"));
                            ui.label(RichText::new(&current_name).strong().size(body_size(ui) + 3.0));
                            if !self.net.connected {
                                ui.label(
                                    RichText::new(
                                        self.net.last_error.as_deref().unwrap_or("disconnected"),
                                    )
                                    .color(pal().red)
                                    .size(body_size(ui) - 1.0),
                                );
                            }
                        });
                        ui.label(soft(ui, &current_detail));
                        ui.label(addr(ui, crate::connections::redacted(&current)));
                    });

                ui.add_space(8.0);
                ui.label(soft(ui, 
                    "Switching keeps your layout and keymap; everything the old daemon \
                     said is dropped.",
                ));

                // --- This machine -------------------------------------------
                // Above everything and outside the saved list, carrying no
                // Edit or Forget: the point of it is that the machine you are
                // sitting in front of cannot be lost, and a row you can delete
                // is a row somebody deletes.
                section(ui, "THIS MACHINE", "");
                card(ui, local_live, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon::DOT).color(if local_live {
                            pal().green
                        } else {
                            pal().dim
                        }));
                        ui.label(RichText::new(local.label()).strong());
                        ui.label(soft(ui, match self.daemon_mode {
                            crate::daemon::Mode::Hosting if local_live => "hosted by this window",
                            crate::daemon::Mode::Attached { .. } if local_live => "already running",
                            _ => "where this window is running",
                        }));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !local_live && ui.button("Connect").clicked() {
                                switch = Some(local.clone());
                            }
                        });
                    });
                    ui.label(addr(ui, &local.url));
                });

                // --- Saved --------------------------------------------------
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("SAVED")
                            .strong()
                            .size(body_size(ui) - 1.0)
                            .color(pal().text_strong),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if draft_slot.is_none()
                            && ui
                                .button(format!("{} Add a daemon", icon::NEW_SESSION))
                                .clicked()
                        {
                            reopen = Some((None, Default::default()));
                        }
                    });
                });
                ui.add_space(3.0);

                // The form sits where the row it makes will land, rather than
                // at the far bottom of the window where the Add button that
                // opened it cannot see the result.
                if let Some((at, draft)) = draft_slot.as_mut() {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(match at {
                                Some(_) => "Edit daemon",
                                None => "New daemon",
                            })
                            .strong(),
                        );
                        ui.add_space(4.0);
                        egui::Grid::new("conn-draft")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(soft(ui, "Name"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.name)
                                        .hint_text("devbox")
                                        .desired_width(f32::INFINITY),
                                );
                                ui.end_row();
                                ui.label(soft(ui, "URL"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.url)
                                        .hint_text("ws://10.0.0.27:7717/ws")
                                        .desired_width(f32::INFINITY),
                                );
                                ui.end_row();
                                ui.label(soft(ui, "Token"));
                                let mut token = draft.token.clone().unwrap_or_default();
                                // Masked because a token on screen is a token
                                // in whatever recording or screenshot is
                                // running.
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut token)
                                            .password(true)
                                            .hint_text("only if the daemon requires one")
                                            .desired_width(f32::INFINITY),
                                    )
                                    .changed()
                                {
                                    draft.token = (!token.is_empty()).then_some(token);
                                }
                                ui.end_row();
                            });
                        if let Some(problem) = draft.problem() {
                            ui.label(
                                RichText::new(format!("{} {problem}", icon::WARN))
                                    .color(pal().amber)
                                    .size(body_size(ui) - 1.0),
                            );
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let ok = draft.problem().is_none();
                            if ui.add_enabled(ok, egui::Button::new("Save")).clicked() {
                                let mut conn = draft.clone();
                                conn.url = conn.url.trim().to_string();
                                conn.name = conn.name.trim().to_string();
                                commit = Some((*at, conn));
                            }
                            if ui.button("Cancel").clicked() {
                                cancel = true;
                            }
                        });
                        ui.label(soft(ui, 
                            "The token is stored in ~/.mogeung/connections.json, owner-readable \
                             only. It travels in clear text unless the URL is wss://.",
                        ));
                    });
                    ui.add_space(6.0);
                }

                if self.connections.list.is_empty() && draft_slot.is_none() {
                    ui.label(soft(ui, 
                        "Nothing saved yet — add one above, or take one off the network below.",
                    ));
                }

                for (i, c) in self.connections.list.iter().enumerate() {
                    let live = current.starts_with(&c.url);
                    card(ui, live, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon::DOT).color(if live {
                                pal().green
                            } else {
                                pal().dim
                            }));
                            ui.label(RichText::new(c.label()).strong());
                            if live {
                                ui.label(soft(ui, "connected"));
                            } else if self.connections.active == Some(i) {
                                // Which one you reached for last time. A hint
                                // and nothing more — no launch dials it, which
                                // is the whole change `R-I7` reverted to.
                                ui.label(soft(ui, "last used"));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui
                                    .button(icon::HIDE)
                                    .on_hover_text("Forget this daemon")
                                    .clicked()
                                {
                                    remove = Some(i);
                                }
                                if ui.button("Edit").clicked() {
                                    reopen = Some((Some(i), c.clone()));
                                }
                                if !live && ui.button("Connect").clicked() {
                                    switch = Some(c.clone());
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label(addr(ui, &c.url));
                            // Never the token itself, here or anywhere: this
                            // window is the thing people screen-share.
                            if c.token.is_some() {
                                ui.label(badge("token", pal().blue));
                            }
                        });
                    });
                }

                // --- On this network (R-I8) ---------------------------------
                // A wifi picker, not a search: the subscription runs while this
                // window is open and rows accumulate.
                section(ui, "ON THIS NETWORK", &scan_note);

                if found.is_empty() && !self.scan_says_waiting() {
                    ui.label(soft(ui, 
                        "A daemon appears only with --advertise, and only from a \
                         non-loopback --listen. Many networks drop multicast between hosts.",
                    ));
                }

                for (f, url) in &found {
                    card(ui, false, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(icon::DOT).color(pal().dim));
                            ui.label(RichText::new(&f.name).strong());
                            if f.needs_token {
                                ui.label(badge("token", pal().amber));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if self.connections.list.iter().any(|c| c.url == *url) {
                                    ui.label(soft(ui, "saved"));
                                } else if ui.button("Add").clicked() {
                                    // Add, never connect: this fills the form
                                    // and waits for a hand.
                                    reopen = Some((
                                        None,
                                        crate::connections::Connection {
                                            name: f.name.clone(),
                                            url: url.clone(),
                                            token: None,
                                        },
                                    ));
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label(addr(
                                ui,
                                f.addr().map(|a| a.to_string()).unwrap_or_default(),
                            ));
                            if f.addrs.len() > 1 {
                                ui.label(soft(ui, format!("+{}", f.addrs.len() - 1)))
                                    .on_hover_text(
                                        f.addrs
                                            .iter()
                                            .map(|a| a.to_string())
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    );
                            }
                            if let Some(home) = &f.claude_home {
                                ui.label(soft(ui, format!("· {home}")));
                            }
                        });
                    });
                }

                ui.add_space(10.0);
                ui.separator();
                ui.label(soft(ui, format!(
                    "Saved in {}",
                    crate::connections::Connections::path().display()
                )));
            });

        self.conn_draft = draft_slot;
        if cancel {
            self.conn_draft = None;
        }
        if let Some((at, conn)) = commit {
            // Editing a row's URL into one that already exists collapses the
            // two rather than leaving a duplicate behind.
            if let Some(old) = at {
                self.connections.remove(old);
            }
            self.connections.upsert(conn);
            self.conn_draft = None;
            save = true;
        }
        if let Some(next) = reopen {
            self.conn_draft = Some(next);
        }
        if let Some(i) = remove {
            self.connections.remove(i);
            save = true;
        }
        if let Some(conn) = switch {
            self.connections.active = self.connections.list.iter().position(|c| c.url == conn.url);
            self.switch_to(&conn, &ctx);
            save = true;
        }
        if save {
            if let Err(e) = self.connections.save() {
                self.errors.push(format!("could not save connections: {e}"));
            }
        }
        if !open {
            self.show_connections = false;
            self.conn_draft = None;
            // Stop querying the network the moment nobody is looking at the
            // answers. The rows are kept: reopening shows what was found, and
            // the subscription refreshes it.
            self.scan = None;
            self.scan_since = None;
        }
    }
}
