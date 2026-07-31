//! User-editable key bindings.
//!
//! Three things wanted this to exist: pane-aware navigation (`Alt+1`/`Alt+2`),
//! rebinding, and import/export. All three need one thing first — actions have
//! to stop being spelled out inline in the event loop and become *data*.
//!
//! ## Where this lives
//!
//! On disk at `~/.mogeung/keymap.json`, **client-side**, not in the daemon.
//! That is not a violation of "the daemon is the product, every UI is a client
//! with no local authority" ([ADR-0001]) — a keymap is not daemon state. It
//! describes how *this* window turns keystrokes into commands, and a second
//! client (the web UI, a future TUI) would rightly have its own.
//!
//! ## Storage shape
//!
//! The file holds the **full effective map**, not just overrides, so export
//! produces something self-contained you can hand to someone else. Loading
//! merges over the defaults, so an action added in a later version appears
//! with its default binding instead of silently having none.

use egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Something the user can bind a key to.
///
/// Navigation actions are deliberately pane-agnostic: `Next` means "next thing
/// in whatever has focus". One binding that always does the obvious thing beats
/// three bindings you have to remember the context for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // -- panes
    /// Collapse the queue down to a strip, and back.
    ///
    /// The queue is the permanent furniture of this app, so it cannot be
    /// *closed* — but a wide diff sometimes wants the room, and giving it back
    /// should not mean dragging a splitter twice.
    ToggleQueuePanel,
    FocusQueue,
    FocusFiles,
    FocusDiff,
    // -- tabs in the detail panel
    TabChanges,
    TabTranscript,
    TabInfo,
    TabDebt,
    /// The session's own terminal, attached through tmux. `R-B18`.
    ///
    /// Called `Agent` rather than `Terminal` because a second pty pane is
    /// coming, and "Terminal" has to mean there what it means in every editor:
    /// a shell you own. The serde name stays `tab_terminal` so a keymap saved
    /// before the rename keeps its binding.
    #[serde(rename = "tab_terminal")]
    TabAgent,
    /// Show or hide the terminal panel — shells of your own. `R-B31`, `R-B33`.
    ///
    /// The one action in this group that is not a tab and not one-way. The
    /// terminal is a panel across the bottom rather than a pane in the tree
    /// (`R-B33`), and a key that gives a third of the window to a shell has to
    /// be the key that takes it back — which is what Alt+F12 and ``Ctrl+` ``
    /// do in the editors people arrive here from.
    ///
    /// The serde name is `tab_shell`, not `tab_terminal` — that one belongs to
    /// the Agent pane, which held the word first and cannot give up its stored
    /// name without dropping everyone's binding. The two are unrelated things
    /// that happen to have swapped labels once.
    #[serde(rename = "tab_shell")]
    TabTerminal,
    /// The session's worktree, read-only. `R-B24`.
    TabExplorer,
    /// Commits, uncommitted changes and diffs of the session's repo. `R-D10`.
    TabGit,
    TabInsight,
    NextTab,
    PrevTab,
    /// Cycle dark → light → system → dark. `R-J6`.
    CycleTheme,
    /// Toggle whether the keyboard belongs to the focused pty pane.
    ///
    /// One chord both ways — in and back out — because reaching the terminal
    /// should not need a mouse when leaving it does not. Needs its own action
    /// because the obvious key cannot be used: Escape belongs to Claude Code,
    /// where it interrupts a turn. A binding that stole it would make the
    /// agent uninterruptible from the pane that exists to interrupt it.
    ///
    /// Deliberately *not* per-pane: it means whichever terminal is in front of
    /// you — the shell in the panel if it is up, else the Agent pane. One chord
    /// for "give the terminal my keystrokes" is a thing you learn once — two
    /// would be two, and you would press the wrong one.
    #[serde(rename = "leave_terminal")]
    ToggleTerminalFocus,
    // -- scrolling the content pane
    PageDown,
    PageUp,
    ScrollTop,
    ScrollBottom,
    // -- navigation, relative to the focused pane
    Next,
    Prev,
    First,
    Activate,
    // -- queue
    /// Raise the terminal *application* the session runs in — iTerm2, or
    /// Terminal.app, or the tmux client — not a pane inside mogeung.
    ///
    /// The distinction was invisible while mogeung had one thing called a
    /// terminal. With two it is the difference between staying and leaving.
    #[serde(rename = "jump_to_terminal")]
    FocusTerminalApp,
    MarkAllRead,
    Snooze,
    FilterFocus,
    ClearFilter,
    HideSession,
    PinSession,
    /// Name the session yourself; the badge and `label:` filter follow.
    /// `R-B26`.
    LabelSession,
    // -- review
    ToggleRead,
    NextUnread,
    FlagHunk,
    // -- explorer (`R-B25`)
    /// Find a worktree file by name, fuzzily, in the palette.
    GoToFile,
    /// Find lines across the worktree, in the palette.
    SearchInFiles,
    /// The open explorer files, most recently used first.
    RecentFiles,
    NextFileTab,
    PrevFileTab,
    CloseFileTab,
    PinFileTab,
    /// Find within the file open in the Explorer's viewer. Client-side.
    FindInFile,
    /// Send the active file to the other side of the editor split.
    MoveFileTabSplit,
    /// Per-line authorship in the Editor's gutter. `R-D10`.
    ToggleAnnotate,
    /// Open the file selected in the Changes tab in the Explorer, revealed.
    OpenInExplorer,
    // -- windows
    /// Every action, by name, without knowing its key first.
    ///
    /// The one binding worth memorising, because it is the one that means you
    /// never have to memorise another.
    CommandPalette,
    /// Put the detail panes back to the single tab container they start as.
    ///
    /// The escape hatch a docking system cannot ship without: an arrangement
    /// you cannot undo by hand is one bad drag away from unusable.
    ResetLayout,
    ToggleAmbient,
    ToggleHealth,
    ToggleConnections,
    OpenKeymap,
    Rescan,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::ToggleQueuePanel,
        Action::FocusQueue,
        Action::FocusFiles,
        Action::FocusDiff,
        Action::TabChanges,
        Action::TabTranscript,
        Action::TabInfo,
        Action::TabDebt,
        Action::TabAgent,
        Action::TabTerminal,
        Action::TabExplorer,
        Action::TabGit,
        Action::TabInsight,
        Action::NextTab,
        Action::PrevTab,
        Action::ToggleTerminalFocus,
        Action::PageDown,
        Action::PageUp,
        Action::ScrollTop,
        Action::ScrollBottom,
        Action::Next,
        Action::Prev,
        Action::First,
        Action::Activate,
        Action::FocusTerminalApp,
        Action::MarkAllRead,
        Action::Snooze,
        Action::FilterFocus,
        Action::ClearFilter,
        Action::HideSession,
        Action::PinSession,
        Action::LabelSession,
        Action::ToggleRead,
        Action::NextUnread,
        Action::FlagHunk,
        Action::GoToFile,
        Action::SearchInFiles,
        Action::RecentFiles,
        Action::NextFileTab,
        Action::PrevFileTab,
        Action::CloseFileTab,
        Action::PinFileTab,
        Action::FindInFile,
        Action::MoveFileTabSplit,
        Action::ToggleAnnotate,
        Action::OpenInExplorer,
        Action::CommandPalette,
        Action::ResetLayout,
        Action::ToggleAmbient,
        Action::ToggleHealth,
        Action::ToggleConnections,
        Action::OpenKeymap,
        Action::CycleTheme,
        Action::Rescan,
    ];

    pub fn group(self) -> &'static str {
        match self {
            Action::ToggleQueuePanel
            | Action::FocusQueue
            | Action::FocusFiles
            | Action::FocusDiff => "Panes",
            Action::TabChanges
            | Action::TabTranscript
            | Action::TabInfo
            | Action::TabDebt
            | Action::TabAgent
            | Action::TabTerminal
            | Action::TabExplorer
            | Action::TabGit
            | Action::TabInsight
            | Action::NextTab
            | Action::PrevTab
            | Action::ToggleTerminalFocus => "Tabs",
            Action::PageDown
            | Action::PageUp
            | Action::ScrollTop
            | Action::ScrollBottom => "Scrolling",
            Action::Next | Action::Prev | Action::First | Action::Activate => "Navigation",
            Action::FocusTerminalApp
            | Action::MarkAllRead
            | Action::Snooze
            | Action::FilterFocus
            | Action::ClearFilter
            | Action::HideSession
            | Action::PinSession
            | Action::LabelSession => "Queue",
            Action::ToggleRead | Action::NextUnread | Action::FlagHunk => "Review",
            Action::GoToFile
            | Action::SearchInFiles
            | Action::RecentFiles
            | Action::NextFileTab
            | Action::PrevFileTab
            | Action::CloseFileTab
            | Action::PinFileTab
            | Action::FindInFile
            | Action::MoveFileTabSplit
            | Action::ToggleAnnotate
            | Action::OpenInExplorer => "Editor",
            Action::CommandPalette
            | Action::ResetLayout
            | Action::ToggleAmbient
            | Action::ToggleHealth
            | Action::ToggleConnections
            | Action::OpenKeymap
            | Action::CycleTheme
            | Action::Rescan => "Windows",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::ToggleQueuePanel => "Collapse or expand the queue",
            Action::FocusQueue => "Focus the session queue",
            Action::FocusFiles => "Focus the file list",
            Action::FocusDiff => "Focus the diff",
            Action::TabChanges => "Show the Changes tab",
            Action::TabTranscript => "Show the Transcript tab",
            Action::TabInfo => "Show the Info tab",
            Action::TabDebt => "Show the Debt tab",
            Action::TabAgent => "Show the Agent tab — the session's own terminal, attached",
            Action::TabTerminal => "Show or hide the terminal — your own shells, at the bottom",
            Action::TabExplorer => "Show the Editor tab — browse and read the session's files",
            Action::TabGit => "Show the Git tab — commits, changes and diffs",
            Action::TabInsight => "Show the Insight tab — cross-session search, digest, analytics, docs",
            Action::NextTab => "Next tab",
            Action::PrevTab => "Previous tab",
            Action::ToggleTerminalFocus => "Type into the terminal, or give the keyboard back",
            Action::PageDown => "Scroll the transcript or diff down a page",
            Action::PageUp => "Scroll the transcript or diff up a page",
            Action::ScrollTop => "Jump to the top",
            Action::ScrollBottom => "Jump to the bottom",
            Action::Next => "Next item in the focused pane",
            Action::Prev => "Previous item in the focused pane",
            Action::First => "First item in the focused pane",
            Action::Activate => "Open — the terminal app for a session, the file for a row",
            Action::FocusTerminalApp => "Switch to the terminal app the session runs in",
            Action::MarkAllRead => "Mark the whole diff read",
            Action::Snooze => "Snooze or wake the session",
            Action::FilterFocus => "Focus the queue filter",
            Action::ClearFilter => "Clear the filter / close overlays",
            Action::HideSession => "Hide the session from the queue (reversible)",
            Action::PinSession => "Pin or unpin the session at the top",
            Action::LabelSession => "Label the session — name it yourself",
            Action::ToggleRead => "Mark the selected hunk read or unread",
            Action::NextUnread => "Jump to the next unread hunk",
            Action::FlagHunk => "Flag the selected hunk for a follow-up prompt",
            Action::GoToFile => "Go to file — find a worktree file by name",
            Action::SearchInFiles => "Search in files — find lines across the worktree",
            Action::RecentFiles => "Recent files — the open tabs, latest first",
            Action::NextFileTab => "Next open file in the Editor",
            Action::PrevFileTab => "Previous open file in the Editor",
            Action::CloseFileTab => "Close the active Editor file",
            Action::PinFileTab => "Pin or unpin the active Editor file",
            Action::FindInFile => "Find — the Git log filter when the Git pane is focused, else in the Editor's open file",
            Action::MoveFileTabSplit => "Open the file on the other side, side by side",
            Action::ToggleAnnotate => "Toggle git blame in the Editor's gutter",
            Action::OpenInExplorer => "Open the selected changed file in the Editor",
            Action::CommandPalette => "Command palette — run anything by name",
            Action::ResetLayout => "Reset the pane layout",
            Action::ToggleAmbient => "Show or hide the ambient board",
            Action::ToggleHealth => "Show or hide the health panel",
            Action::ToggleConnections => "Choose which daemon this window watches",
            Action::OpenKeymap => "Open this keyboard settings window",
            Action::CycleTheme => "Theme — dark, light, or follow the desktop",
            Action::Rescan => "Rescan sessions now",
        }
    }
}

/// A key that is itself a modifier, and therefore never a binding on its own.
///
/// egui 0.35 began reporting modifier presses as `Key`s in their own right
/// (`AltLeft`, `SuperRight`, …). Two things must not happen as a result: the
/// rebind capture must not end on one (or every chord saves as its bare
/// modifier — found live), and a stored binding naming one must not load as
/// valid (or a file polluted by that capture bug fires actions on a bare Alt).
pub fn is_modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

/// A key plus modifiers, stored as text so the file stays readable and does not
/// depend on egui's internal enum layout.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Binding(pub String);

impl Binding {
    pub fn new(mods: Modifiers, key: Key) -> Self {
        let mut parts: Vec<&str> = Vec::new();
        // Fixed order, so the same chord always produces the same string —
        // otherwise two identical bindings could fail to compare equal.
        if mods.ctrl {
            parts.push("Ctrl");
        }
        if mods.alt {
            parts.push("Alt");
        }
        if mods.shift {
            parts.push("Shift");
        }
        if mods.mac_cmd || (mods.command && !mods.ctrl) {
            parts.push("Cmd");
        }
        let mut s = parts.join("+");
        if !s.is_empty() {
            s.push('+');
        }
        s.push_str(key.name());
        Binding(s)
    }

    /// Parse back into something matchable.
    ///
    /// **Every token must be understood.** An earlier version ignored tokens it
    /// did not recognise, so `Ctl+J` — a plausible hand-edit typo — parsed
    /// happily as a bare `J`: it would fire on the wrong key and validation
    /// would call it fine. Anything unrecognised now invalidates the whole
    /// binding, which is the only way "this shortcut does nothing" can be
    /// distinguished from "this shortcut is wrong".
    pub fn parse(&self) -> Option<(Modifiers, Key)> {
        let mut mods = Modifiers::NONE;
        let mut key = None;
        for raw in self.0.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                return None; // "Ctrl+", "", "A++B"
            }
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "alt" | "option" => mods.alt = true,
                "shift" => mods.shift = true,
                "cmd" | "command" | "super" => {
                    mods.mac_cmd = true;
                    mods.command = true;
                }
                _ => {
                    if key.is_some() {
                        return None; // two keys in one chord
                    }
                    let k = Key::from_name(token)?;
                    if is_modifier_key(k) {
                        return None; // "AltLeft" is a modifier, not a binding
                    }
                    key = Some(k);
                }
            }
        }
        key.map(|k| (mods, k))
    }

    pub fn is_valid(&self) -> bool {
        self.parse().is_some()
    }
}

impl std::fmt::Display for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The whole map. Several bindings may point at one action; a binding points at
/// exactly one action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keymap {
    /// Action → its bindings. A `BTreeMap` so the file has a stable order and
    /// two exports of the same map are byte-identical.
    ///
    /// `serde(default)` so a hand-written file containing only the one binding
    /// somebody wanted to change is valid, and everything else falls back.
    #[serde(default)]
    pub bindings: BTreeMap<Action, Vec<Binding>>,
}

impl Default for Keymap {
    fn default() -> Self {
        let mut b: BTreeMap<Action, Vec<Binding>> = BTreeMap::new();
        let mut set = |a: Action, keys: &[&str]| {
            b.insert(a, keys.iter().map(|k| Binding(k.to_string())).collect());
        };

        set(Action::ToggleQueuePanel, &["OpenBracket"]);
        set(Action::FocusQueue, &["Alt+1"]);
        set(Action::FocusFiles, &["Alt+2"]);
        set(Action::FocusDiff, &["Alt+3"]);

        // Both vim keys and arrows, because muscle memory differs and there is
        // no reason to make anyone choose.
        // Mnemonic single letters: the four tabs are the thing you switch
        // between constantly, and c/t/i/d are unclaimed.
        set(Action::TabChanges, &["C"]);
        set(Action::TabTranscript, &["T"]);
        set(Action::TabInfo, &["I"]);
        set(Action::TabDebt, &["D"]);
        // E as in tErminal, kept although the pane now displays as "Agent",
        // for the same reason X below stayed with the Editor: a binding that
        // moves under trained hands costs more than a mnemonic that lags a
        // rename. `A` is therefore still free.
        set(Action::TabAgent, &["E"]);
        // Not a letter, unlike every tab above it, and deliberately so. Both
        // of these are what the editors people already have bound it to —
        // Alt+F12 is IntelliJ's, Ctrl+` is VS Code's — and a shell is the one
        // pane here that arrives with an existing reflex attached. The bare
        // letters stay with the panes that are mogeung's own idea.
        //
        // Two defaults rather than one because the reflex splits cleanly by
        // editor and neither camp should have to look this up.
        set(Action::TabTerminal, &["Alt+F12", "Ctrl+Backquote"]);
        set(Action::TabExplorer, &["X"]);
        // V as in VCS — g means "first" and G is taken with it.
        set(Action::TabGit, &["V"]);
        set(Action::TabInsight, &["Y"]);
        set(Action::NextTab, &["Ctrl+Tab"]);
        set(Action::PrevTab, &["Ctrl+Shift+Tab"]);

        // Read while the terminal holds the keyboard, so it must be a chord
        // Claude Code will never want. Escape, Ctrl+C and Ctrl+D all belong to
        // the agent; Alt+Cmd+M does not belong to anyone. The same is true of
        // a shell, so this one chord covers both pty panes.
        set(Action::ToggleTerminalFocus, &["Alt+Cmd+M"]);

        set(Action::PageDown, &["PageDown"]);
        set(Action::PageUp, &["PageUp"]);
        set(Action::ScrollTop, &["Home"]);
        set(Action::ScrollBottom, &["End"]);

        set(Action::Next, &["J", "Down"]);
        set(Action::Prev, &["K", "Up"]);
        set(Action::First, &["G"]);
        set(Action::Activate, &["Enter"]);

        // O as in "over there" — this one leaves mogeung for the terminal app.
        set(Action::FocusTerminalApp, &["O"]);
        set(Action::MarkAllRead, &["R"]);
        set(Action::Snooze, &["S"]);
        set(Action::FilterFocus, &["Slash"]);
        set(Action::ClearFilter, &["Escape"]);
        set(Action::HideSession, &["H"]);
        set(Action::PinSession, &["P"]);
        // L as in label, and the last honest letter of it still free.
        set(Action::LabelSession, &["L"]);

        set(Action::ToggleRead, &["Space"]);
        set(Action::NextUnread, &["N"]);
        set(Action::FlagHunk, &["F"]);

        // The chords every editor taught: Ctrl+P finds a file, Ctrl+Shift+F
        // searches contents. Both with Cmd twins for macOS hands.
        set(Action::GoToFile, &["Ctrl+P", "Cmd+P"]);
        set(Action::SearchInFiles, &["Ctrl+Shift+F", "Cmd+Shift+F"]);
        set(Action::RecentFiles, &["Ctrl+E", "Cmd+E"]);
        // Alt+arrows walk the file tabs; Ctrl+Tab already walks the panes.
        set(Action::NextFileTab, &["Alt+Right"]);
        set(Action::PrevFileTab, &["Alt+Left"]);
        set(Action::CloseFileTab, &["Ctrl+W"]);
        set(Action::PinFileTab, &["Alt+P"]);
        set(Action::FindInFile, &["Ctrl+F", "Cmd+F"]);
        // S as in split/side. Bare S is snooze; Alt+S was free.
        set(Action::MoveFileTabSplit, &["Alt+S"]);
        set(Action::ToggleAnnotate, &["Alt+B"]);
        set(Action::OpenInExplorer, &["Alt+X"]);

        // Two bindings on purpose: `Cmd+K` is the palette everywhere on this
        // platform, and `Ctrl+K` is the same reflex for anyone arriving from
        // Linux. Neither is claimed by anything else.
        set(Action::CommandPalette, &["Cmd+K", "Ctrl+K"]);

        // Deliberately awkward. Resetting the layout throws away an
        // arrangement you may have spent a while on, so it should be reachable
        // and not adjacent to anything you press often.
        set(Action::ResetLayout, &["Alt+0"]);

        set(Action::ToggleAmbient, &["Alt+A"]);
        set(Action::ToggleHealth, &["Alt+H"]);
        set(Action::ToggleConnections, &["Alt+D"]);
        set(Action::OpenKeymap, &["Alt+K"]);
        set(Action::Rescan, &["Alt+R"]);
        // Alt+T rather than a bare letter: switching theme mid-review is a
        // once-in-a-while act, and the letters belong to triage.
        set(Action::CycleTheme, &["Alt+T"]);

        Keymap { bindings: b }
    }
}

impl Keymap {
    pub fn path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".mogeung").join("keymap.json")
    }

    /// Load from disk, filling in anything missing from the defaults.
    ///
    /// Merging rather than replacing matters across versions: a keymap saved
    /// today must not leave an action added next month permanently unbound and
    /// invisible.
    pub fn load() -> (Self, Option<String>) {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };
        match serde_json::from_str::<Keymap>(&text) {
            Ok(mut km) => {
                let defaults = Self::default();
                for (action, keys) in defaults.bindings {
                    km.bindings.entry(action).or_insert(keys);
                }
                let bad = km.invalid();
                let warning = (!bad.is_empty()).then(|| {
                    format!(
                        "{} keymap binding(s) name keys mogeung does not know: {}",
                        bad.len(),
                        bad.iter()
                            .map(|(_, b)| b.0.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                });
                (km, warning)
            }
            // A broken file must not cost you a working keyboard.
            Err(e) => (
                Self::default(),
                Some(format!("{} is unreadable ({e}) — using defaults", path.display())),
            ),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, self.to_json()?).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let mut km: Keymap = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let defaults = Self::default();
        for (action, keys) in defaults.bindings {
            km.bindings.entry(action).or_insert(keys);
        }
        Ok(km)
    }

    /// Which action, if any, this chord triggers.
    pub fn action_for(&self, mods: Modifiers, key: Key) -> Option<Action> {
        for (action, keys) in &self.bindings {
            for b in keys {
                if let Some((m, k)) = b.parse() {
                    if k == key && same_mods(m, mods) {
                        return Some(*action);
                    }
                }
            }
        }
        None
    }

    /// The action a binding is already taken by, ignoring `except`.
    pub fn conflict(&self, binding: &Binding, except: Action) -> Option<Action> {
        self.bindings
            .iter()
            .filter(|(a, _)| **a != except)
            .find(|(_, keys)| keys.contains(binding))
            .map(|(a, _)| *a)
    }

    pub fn set(&mut self, action: Action, bindings: Vec<Binding>) {
        self.bindings.insert(action, bindings);
    }

    pub fn reset(&mut self, action: Action) {
        let d = Self::default();
        if let Some(keys) = d.bindings.get(&action) {
            self.bindings.insert(action, keys.clone());
        }
    }

    pub fn bindings_for(&self, action: Action) -> &[Binding] {
        self.bindings.get(&action).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Bindings that name a key egui does not know.
    ///
    /// These are the worst kind of failure for a keymap: the file loads, the
    /// window opens, the shortcut is listed in the settings — and pressing it
    /// does nothing, with no way to tell that from "the action is broken". A
    /// hand-edited or hand-ported map is exactly where a typo like `Ctl+J` or
    /// `Cmd+Return` turns up, so import says so out loud.
    pub fn invalid(&self) -> Vec<(Action, Binding)> {
        self.bindings
            .iter()
            .flat_map(|(a, keys)| keys.iter().map(move |b| (*a, b.clone())))
            .filter(|(_, b)| !b.is_valid())
            .collect()
    }

    /// Human-readable, for showing a shortcut next to a menu item.
    pub fn describe(&self, action: Action) -> String {
        let keys = self.bindings_for(action);
        if keys.is_empty() {
            return "unbound".into();
        }
        keys.iter().map(|b| b.0.clone()).collect::<Vec<_>>().join(" / ")
    }
}

/// Compare only the modifiers that matter.
///
/// egui sets `command` *and* `mac_cmd` on macOS, and `command` mirrors `ctrl`
/// elsewhere; comparing `Modifiers` directly would make bindings behave
/// differently per platform for no reason the user could see.
fn same_mods(a: Modifiers, b: Modifiers) -> bool {
    let cmd = |m: Modifiers| m.mac_cmd || (m.command && !m.ctrl);
    a.ctrl == b.ctrl && a.alt == b.alt && a.shift == b.shift && cmd(a) == cmd(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_default_binding() {
        // An action with no default is one the user can never discover.
        let km = Keymap::default();
        for a in Action::ALL {
            assert!(
                !km.bindings_for(*a).is_empty(),
                "{a:?} ({}) has no default binding",
                a.label()
            );
        }
    }

    #[test]
    fn every_default_binding_parses() {
        let km = Keymap::default();
        for (action, keys) in &km.bindings {
            for b in keys {
                assert!(b.is_valid(), "{action:?} has unparseable binding {b}");
            }
        }
    }

    #[test]
    fn no_two_actions_share_a_default_binding() {
        // A conflict in the shipped defaults means one of them silently never
        // fires, and which one depends on map iteration order.
        let km = Keymap::default();
        let mut seen: BTreeMap<&str, Action> = BTreeMap::new();
        for (action, keys) in &km.bindings {
            for b in keys {
                if let Some(prev) = seen.insert(&b.0, *action) {
                    panic!("{} is bound to both {prev:?} and {action:?}", b.0);
                }
            }
        }
    }

    #[test]
    fn round_trips_through_the_binding_string() {
        let cases = [
            (Modifiers::NONE, Key::J),
            (Modifiers::ALT, Key::Num1),
            (Modifiers { ctrl: true, shift: true, ..Default::default() }, Key::K),
            (Modifiers::NONE, Key::Slash),
            (Modifiers::NONE, Key::Escape),
        ];
        for (mods, key) in cases {
            let b = Binding::new(mods, key);
            let (m2, k2) = b.parse().unwrap_or_else(|| panic!("{b} did not parse"));
            assert_eq!(k2, key, "{b}");
            assert!(same_mods(m2, mods), "{b}: modifiers lost");
        }
    }

    #[test]
    fn lookup_respects_modifiers() {
        let km = Keymap::default();
        // Alt+1 focuses the queue; a bare 1 is not bound to anything.
        assert_eq!(
            km.action_for(Modifiers::ALT, Key::Num1),
            Some(Action::FocusQueue)
        );
        assert_eq!(km.action_for(Modifiers::NONE, Key::Num1), None);
        // And a plain J must not fire when Alt is held.
        assert_eq!(km.action_for(Modifiers::NONE, Key::J), Some(Action::Next));
        assert_eq!(km.action_for(Modifiers::ALT, Key::J), None);
    }

    #[test]
    fn a_saved_map_survives_a_round_trip() {
        let mut km = Keymap::default();
        km.set(Action::Snooze, vec![Binding("Ctrl+Z".into())]);
        let json = km.to_json().unwrap();
        let back = Keymap::from_json(&json).unwrap();
        assert_eq!(back.bindings_for(Action::Snooze), km.bindings_for(Action::Snooze));
        assert_eq!(back.bindings, km.bindings);
    }

    /// The reason the file stores a full map and loading merges: a keymap
    /// exported by an older build must not leave newer actions unbound.
    #[test]
    fn loading_an_old_map_fills_in_actions_it_never_heard_of() {
        let partial = r#"{ "bindings": { "snooze": ["Ctrl+Z"] } }"#;
        let km = Keymap::from_json(partial).unwrap();
        assert_eq!(km.bindings_for(Action::Snooze)[0].0, "Ctrl+Z");
        for a in Action::ALL {
            assert!(!km.bindings_for(*a).is_empty(), "{a:?} left unbound");
        }
    }

    #[test]
    fn a_typo_in_a_binding_is_reported_rather_than_silently_dead() {
        // The realistic hand-edit mistakes: a misspelled modifier and a key
        // name from a different toolkit.
        let km = Keymap::from_json(
            r#"{ "bindings": { "snooze": ["Ctl+J"], "next": ["Cmd+Frobnicate"] } }"#,
        )
        .unwrap();
        let bad: Vec<String> = km.invalid().into_iter().map(|(_, b)| b.0).collect();
        assert!(bad.contains(&"Ctl+J".to_string()), "got {bad:?}");
        assert!(bad.contains(&"Cmd+Frobnicate".to_string()), "got {bad:?}");
    }

    /// A modifier on its own is not a chord. The capture bug that motivated
    /// this saved `AltLeft` the instant Alt went down; a file holding one
    /// must load as *invalid* (loud) rather than fire on a bare Alt press.
    #[test]
    fn a_bare_modifier_never_parses_as_a_binding() {
        for bad in ["AltLeft", "ControlRight", "ShiftLeft", "SuperRight", "Ctrl+AltLeft"] {
            assert!(
                !Binding(bad.to_string()).is_valid(),
                "{bad:?} should not be a valid binding"
            );
        }
    }

    /// The specific shapes that used to slip through.
    #[test]
    fn a_malformed_binding_never_parses_as_something_else() {
        for bad in [
            "Ctl+J",      // misspelled modifier — used to parse as bare J
            "Comand+K",   // misspelled modifier
            "Ctrl+",      // dangling separator
            "",           // empty
            "Ctrl+J+K",   // two keys
            "Frobnicate", // not a key
            "Meta+J",     // not a modifier we know
        ] {
            assert!(
                !Binding(bad.to_string()).is_valid(),
                "{bad:?} should not parse"
            );
        }
    }

    /// egui accepts friendly aliases, and hand-edited files will use them.
    /// Worth pinning: rejecting these would look like the parser being fussy.
    #[test]
    fn common_aliases_are_accepted() {
        for ok in ["Cmd+Return", "Esc", "Ctrl+Down", "Alt+Space", "Shift+Enter"] {
            assert!(Binding(ok.to_string()).is_valid(), "{ok:?} should parse");
        }
    }

    #[test]
    fn the_defaults_contain_nothing_invalid() {
        assert!(Keymap::default().invalid().is_empty());
    }

    #[test]
    fn garbage_json_is_an_error_not_a_panic() {
        assert!(Keymap::from_json("not json").is_err());
        assert!(Keymap::from_json("{}").is_ok(), "an empty object is just defaults");
    }

    #[test]
    fn conflicts_are_detected_but_not_against_yourself() {
        let km = Keymap::default();
        let j = Binding("J".into());
        assert_eq!(km.conflict(&j, Action::Snooze), Some(Action::Next));
        // Rebinding an action to a key it already owns is not a conflict.
        assert_eq!(km.conflict(&j, Action::Next), None);
    }

    /// The three terminal actions were renamed when the Terminal tab became
    /// the Agent tab; their names *on disk* were not. Dropping the
    /// `#[serde(rename)]` that holds that line would hand every existing user
    /// the default binding back and their own rebinding would vanish — the
    /// exact failure this module keeps insisting is the worst kind, because a
    /// shortcut that quietly reverted looks identical to one that broke.
    #[test]
    fn a_keymap_saved_before_the_agent_rename_keeps_its_bindings() {
        let km = Keymap::from_json(
            r#"{ "bindings": {
                "tab_terminal": ["Alt+9"],
                "leave_terminal": ["Alt+8"],
                "jump_to_terminal": ["Alt+7"]
            } }"#,
        )
        .unwrap();
        assert_eq!(km.bindings_for(Action::TabAgent)[0].0, "Alt+9");
        assert_eq!(km.bindings_for(Action::ToggleTerminalFocus)[0].0, "Alt+8");
        assert_eq!(km.bindings_for(Action::FocusTerminalApp)[0].0, "Alt+7");
        // And what we write back is still readable by a build from before the
        // rename, so exporting a map does not strand the person you sent it to.
        let json = km.to_json().unwrap();
        for name in ["tab_terminal", "leave_terminal", "jump_to_terminal"] {
            assert!(json.contains(name), "{name} is missing from the export");
        }
    }

    /// The terminal panel's two chords are the ones the user's editor already
    /// taught them, and both have to survive: dropping either sends half the
    /// people who reach for a terminal to a key that does nothing, and they
    /// will conclude it does not exist rather than that it moved.
    ///
    /// Also the thing they must never become: a chord that reaches the panel
    /// while a pty pane holds the keyboard would be swallowed, so neither may
    /// collide with `ToggleTerminalFocus`.
    #[test]
    fn the_terminal_answers_to_both_editors_reflexes() {
        let km = Keymap::default();
        for chord in ["Alt+F12", "Ctrl+Backquote"] {
            let (mods, key) = Binding(chord.into()).parse().expect("{chord} must parse");
            assert_eq!(
                km.action_for(mods, key),
                Some(Action::TabTerminal),
                "{chord} no longer shows the terminal"
            );
        }
        assert_ne!(
            km.bindings_for(Action::ToggleTerminalFocus),
            km.bindings_for(Action::TabTerminal),
            "the chord that shows the terminal must not be the one that leaves it"
        );
    }

    #[test]
    fn resetting_restores_the_default() {
        let mut km = Keymap::default();
        km.set(Action::Next, vec![Binding("Ctrl+Q".into())]);
        km.reset(Action::Next);
        assert_eq!(km.bindings_for(Action::Next), Keymap::default().bindings_for(Action::Next));
    }
}
