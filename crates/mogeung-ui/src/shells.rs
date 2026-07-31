//! The terminal panel's shells: which ones are open, and where they are rooted.
//! `R-B33`.
//!
//! # A shell is not a view of a session
//!
//! Every other pane in mogeung is *about* the selected session, and follows the
//! selection because that is what it means to be about something. The terminal
//! is not one of those. It is a shell — the place you type `cargo test`, or
//! `claude`, and starting a session from it is the point rather than a side
//! effect. Binding it to a session got that backwards twice over: it
//! disappeared when you moved the selection, and it could not exist at all
//! before there was a session to hang it on, which is precisely the moment you
//! want a shell.
//!
//! So the panel owns its shells, the selection does not move them, and closing
//! the panel hides it rather than ending anything.
//!
//! # What a shell is keyed by
//!
//! A worktree and an ordinal, which together name the tmux session
//! ([ADR-0011](../../../docs/decisions/0011-own-a-shell-never-an-agent.md)). The
//! worktree because it is the directory the commands are about; the ordinal
//! because one shell per directory is one too few the moment a build is running
//! in it.
//!
//! Ordinal 0 keeps the name it had when there was only ever one, so the shell
//! you left running before this module existed is the shell you get back.
//!
//! # What a tab is called
//!
//! By default the worktree's basename, which stops being an answer the moment
//! three of the four tabs say `mogeung`. So a tab can be renamed (`R-B34`), and
//! the name is **only the label**: the tmux session stays keyed by
//! `(worktree, ordinal)`. Renaming a tab and renaming the session look like the
//! same act and are not — a `tmux rename-session` would strand the shell under
//! a name this build never asks for again, which is the exact failure the
//! ordinal was designed around above.

use crate::prefs::{ShellRef, TerminalPanel};
use crate::term::Term;

/// One tab in the terminal panel.
pub struct Shell {
    pub root: String,
    /// Which shell in this worktree — 0 is the first, and names the tmux
    /// session the single-shell build used.
    pub ordinal: u32,
    /// A name you gave this tab, replacing the derived one. Display only: it
    /// never reaches tmux, so `tmux attach -t <session>` keeps working and a
    /// rename cannot lose a shell.
    pub name: Option<String>,
    /// Spawned lazily: a tab restored from the last run costs a tmux client
    /// only once you look at it. Stays alive when you switch away, because a
    /// pty without tmux behind it ([`crate::term::Kind::Bare`]) dies with the
    /// widget, and dropping it would make "switch tabs" mean "kill that shell"
    /// on exactly the machines that can least afford it.
    pub term: Option<Term>,
    /// A shell that would not start, remembered so it is not retried every
    /// frame — the same rule the Agent pane learned the hard way.
    pub failed: Option<String>,
}

impl Shell {
    /// What the tab says: your name for it, or the derived one.
    pub fn title(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.default_title())
    }

    /// The label a tab wears when you have not named it: the worktree's
    /// basename, plus the ordinal once there is more than one. Not the tmux
    /// session name, which is unique and unreadable — that belongs in the
    /// tooltip, where it is useful for `tmux attach`.
    pub fn default_title(&self) -> String {
        let base = std::path::Path::new(&self.root)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.root.clone());
        if self.ordinal == 0 {
            base
        } else {
            format!("{base} {}", self.ordinal + 1)
        }
    }

    pub fn session_name(&self) -> String {
        crate::term::shell_session_name(&self.root, self.ordinal)
    }

    /// What is behind this tab, once it has been spawned. `None` until then —
    /// a tab restored from the last run has no pty yet, and claiming it is
    /// under tmux before asking would be claiming the thing that matters most.
    pub fn kind(&self) -> Option<crate::term::Kind> {
        self.term.as_ref().map(|t| t.kind())
    }

    pub fn exited(&self) -> bool {
        self.term.as_ref().is_some_and(|t| t.exited())
    }
}

/// A tab being renamed in place. `R-B34`.
pub struct Rename {
    /// Which tab. Every path that moves the tabs under it cancels the edit
    /// rather than re-indexing — an index kept correct through a close is an
    /// index that will one day be kept incorrectly, and the cost of being
    /// wrong is renaming a shell you were not looking at.
    pub at: usize,
    pub text: String,
    /// Cleared after the first frame. The field asks for the keyboard once;
    /// asking every frame would make clicking away impossible.
    pub opened: bool,
}

/// The terminal panel: its shells, its height, and whether it is up at all.
#[derive(Default)]
pub struct Shells {
    /// Hidden until asked for. The default on a fresh install is closed —
    /// a panel that opens itself has taken a third of the window from the
    /// thing you actually launched mogeung to look at.
    pub open: bool,
    pub height: f32,
    pub tabs: Vec<Shell>,
    pub active: usize,
    /// The shortcut hands over the keyboard as well as opening the panel, but
    /// it fires a frame before the pty exists — and the pty may never exist.
    /// Taking focus there directly would point the keyboard at nothing whenever
    /// a shell failed to start, and a window that answers no keys looks hung.
    pub focus_wanted: bool,
    /// The tab currently being renamed, if any. Not persisted — an edit half
    /// typed when the window closed is not a name.
    pub rename: Option<Rename>,
    /// Set when the shape changed and the preferences need writing. Never
    /// written from inside the widget that changed it.
    pub dirty: bool,
}

/// The default panel height, and the floor a drag may take it to.
pub const DEFAULT_HEIGHT: f32 = 260.0;
pub const MIN_HEIGHT: f32 = 120.0;

impl Shells {
    /// The panel's geometry and the watched machine's tab list, which live in
    /// two files since `R-I11` — the height is about this window, the tabs are
    /// about worktrees on the machine the daemon is watching.
    pub fn from_prefs(p: &TerminalPanel, tabs: &[ShellRef]) -> Self {
        Shells {
            open: p.open,
            height: sane_height(p.height),
            tabs: tabs
                .iter()
                .map(|s| Shell {
                    root: s.root.clone(),
                    ordinal: s.ordinal,
                    name: s.name.as_deref().and_then(clean_name),
                    term: None,
                    failed: None,
                })
                .collect(),
            active: p.active,
            focus_wanted: false,
            rename: None,
            dirty: false,
        }
        .repaired()
    }

    pub fn to_prefs(&self) -> (TerminalPanel, Vec<ShellRef>) {
        let panel = TerminalPanel {
            open: self.open,
            height: sane_height(self.height),
            active: self.active,
            ..Default::default()
        };
        let tabs = self
            .tabs
            .iter()
            .map(|s| ShellRef {
                root: s.root.clone(),
                ordinal: s.ordinal,
                name: s.name.clone(),
            })
            .collect();
        (panel, tabs)
    }

    /// Clamp `active` into the tabs that exist. A hand-edited or half-written
    /// preferences file is the ordinary case here, not the exotic one.
    fn repaired(mut self) -> Self {
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        self
    }

    pub fn current(&self) -> Option<&Shell> {
        self.tabs.get(self.active)
    }

    /// Open a shell in `root`, and make it the visible one.
    ///
    /// Always a *new* shell, never a switch to an existing one: this is the `+`
    /// button, and a `+` that sometimes adds nothing is a button you stop
    /// trusting. Two shells in one worktree is the case it exists for.
    pub fn open_in(&mut self, root: &str) -> usize {
        let ordinal = self.free_ordinal(root);
        self.rename = None;
        self.tabs.push(Shell {
            root: root.to_string(),
            ordinal,
            name: None,
            term: None,
            failed: None,
        });
        self.active = self.tabs.len() - 1;
        self.dirty = true;
        self.active
    }

    /// The lowest ordinal not currently open for this worktree.
    ///
    /// Lowest rather than next, so closing a tab and opening another lands back
    /// on the same tmux session — which is the shell you just closed, with the
    /// build you left running still in it. A monotonic counter would strand it
    /// under a name nothing reaches again.
    fn free_ordinal(&self, root: &str) -> u32 {
        let taken: Vec<u32> = self
            .tabs
            .iter()
            .filter(|s| s.root == root)
            .map(|s| s.ordinal)
            .collect();
        (0..).find(|n| !taken.contains(n)).unwrap_or(0)
    }

    /// Close a tab. Detaches; it does not kill.
    ///
    /// Dropping the [`Term`] ends mogeung's tmux client and nothing else, so
    /// the shell and whatever is running in it stay reachable from any
    /// terminal. That is the property the whole design is built on, and it is
    /// why there is no "are you sure" here.
    pub fn close(&mut self, at: usize) {
        if at >= self.tabs.len() {
            return;
        }
        self.rename = None;
        self.tabs.remove(at);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        } else if at < self.active {
            self.active -= 1;
        }
        self.dirty = true;
    }

    pub fn select(&mut self, at: usize) {
        if at < self.tabs.len() && at != self.active {
            self.rename = None;
            self.active = at;
            self.dirty = true;
        }
    }

    /// Start renaming a tab, seeded with the name it already wears.
    ///
    /// Seeded rather than empty: renaming `mogeung 2` to `mogeung tests` is
    /// the ordinary case, and an empty field makes you retype what was there.
    /// The tab is selected first, so you are always renaming the shell you can
    /// see — a rename applied to a background tab reads as a bug even when it
    /// did exactly what you asked.
    pub fn begin_rename(&mut self, at: usize) {
        let Some(s) = self.tabs.get(at) else {
            return;
        };
        let text = s.title();
        self.select(at);
        self.rename = Some(Rename {
            at,
            text,
            opened: true,
        });
    }

    /// Take the edited name. Blank clears it, so the tab goes back to naming
    /// its worktree rather than becoming an unlabelled one — and so does
    /// typing the derived name back, which is not an override, just agreement.
    pub fn commit_rename(&mut self) {
        let Some(r) = self.rename.take() else {
            return;
        };
        let Some(s) = self.tabs.get_mut(r.at) else {
            return;
        };
        let name = clean_name(&r.text).filter(|n| *n != s.default_title());
        if s.name != name {
            s.name = name;
            self.dirty = true;
        }
    }

    pub fn cancel_rename(&mut self) {
        self.rename = None;
    }

    /// Drop a tab's name, putting the derived one back.
    pub fn clear_name(&mut self, at: usize) {
        if let Some(s) = self.tabs.get_mut(at) {
            if s.name.take().is_some() {
                self.dirty = true;
            }
        }
    }

    /// Spawn the visible shell if it does not exist yet, and drain every
    /// shell's pty events.
    ///
    /// Polling all of them, not just the visible one: a shell whose process
    /// exited while you were looking at another tab must say so when you come
    /// back, and an undrained channel is how that gets missed.
    pub fn tick(&mut self, ctx: &egui::Context, reach: &crate::term::Reach) {
        for s in &mut self.tabs {
            if let Some(t) = s.term.as_mut() {
                t.poll();
            }
        }
        let Some(s) = self.tabs.get_mut(self.active) else {
            return;
        };
        if s.term.is_some() || s.failed.is_some() {
            return;
        }
        match Term::shell(ctx, &s.root, s.ordinal, reach) {
            Ok(t) => s.term = Some(t),
            Err(e) => s.failed = Some(e.to_string()),
        }
    }

    /// Drop every live pty, keeping the tabs themselves.
    ///
    /// For switching daemons (`R-I7`): a running pane holds a shell on the
    /// machine we are leaving, rooted at a path the next one need not have.
    /// Dropping the view **detaches** rather than kills — tmux still owns the
    /// session over there ([ADR-0011]), so a build left running keeps running
    /// and switching back re-attaches to it.
    ///
    /// The tabs stay because they are this window's arrangement, not the
    /// daemon's, and each re-spawns through the current reach when it is next
    /// shown.
    pub fn detach_all(&mut self) {
        for s in &mut self.tabs {
            s.term = None;
            s.failed = None;
        }
        self.rename = None;
    }

    /// Start the visible shell again after it exited, or after a failure.
    pub fn restart(&mut self, at: usize) {
        if let Some(s) = self.tabs.get_mut(at) {
            s.term = None;
            s.failed = None;
        }
    }
}

/// A typed or stored tab name, or `None` if it is not one.
///
/// Control characters go because a pasted name can carry a newline or an escape
/// sequence, and the length is capped because the tab bar is a single row: a
/// paste of a whole command line would push every other tab off the end of it.
/// Applied on the way *in* from preferences too — the file is hand-editable,
/// and a name that could not be typed should not be loadable either.
fn clean_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(NAME_LIMIT)
        .collect();
    let cleaned = cleaned.trim_end().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// How long a tab name may be. Generous for a label, short of a sentence.
pub const NAME_LIMIT: usize = 32;

/// A stored height that will not open a panel too short to hold a prompt, and
/// will not swallow the window either.
fn sane_height(h: f32) -> f32 {
    if !h.is_finite() {
        return DEFAULT_HEIGHT;
    }
    h.clamp(MIN_HEIGHT, 2000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shells(roots: &[(&str, u32)]) -> Shells {
        Shells {
            tabs: roots
                .iter()
                .map(|(r, o)| Shell {
                    root: r.to_string(),
                    ordinal: *o,
                    name: None,
                    term: None,
                    failed: None,
                })
                .collect(),
            ..Default::default()
        }
    }

    /// Type into the rename field and accept it, as the tab bar does.
    fn rename(s: &mut Shells, at: usize, typed: &str) {
        s.begin_rename(at);
        s.rename.as_mut().expect("editing").text = typed.to_string();
        s.commit_rename();
    }

    /// Two shells in one worktree is the case the `+` button exists for, and
    /// they must not collide onto one tmux session — which is what would
    /// happen if the ordinal were ignored.
    #[test]
    fn a_second_shell_in_one_worktree_gets_its_own_session() {
        let mut s = Shells::default();
        s.open_in("/home/k/repo");
        s.open_in("/home/k/repo");
        assert_eq!(s.tabs.len(), 2);
        assert_ne!(s.tabs[0].session_name(), s.tabs[1].session_name());
        assert_eq!(s.active, 1, "the new shell is the one you are looking at");
    }

    /// Closing a tab and opening another in the same worktree must land back on
    /// the shell just closed — with whatever was left running still in it.
    /// A monotonic counter would strand that session under a name nothing ever
    /// asks for again.
    #[test]
    fn a_reopened_shell_returns_to_the_session_it_left() {
        let mut s = Shells::default();
        s.open_in("/home/k/repo");
        s.open_in("/home/k/repo");
        let second = s.tabs[1].session_name();
        s.close(1);
        s.open_in("/home/k/repo");
        assert_eq!(s.tabs[1].session_name(), second);
    }

    /// The first shell in a worktree keeps the name the single-shell build
    /// used, or upgrading strands every shell anyone had open.
    #[test]
    fn the_first_shell_keeps_the_name_it_had_before_tabs_existed() {
        let s = shells(&[("/home/k/repo", 0)]);
        assert_eq!(
            s.tabs[0].session_name(),
            crate::term::shell_session_name("/home/k/repo", 0)
        );
        assert!(!s.tabs[0].session_name().ends_with("-1"));
    }

    /// Closing a tab before the active one must not leave the selection
    /// pointing at a different shell — the one thing that would make closing a
    /// background tab dangerous.
    #[test]
    fn closing_a_tab_keeps_you_looking_at_the_same_shell() {
        let mut s = shells(&[("/a", 0), ("/b", 0), ("/c", 0)]);
        s.active = 2;
        // Closing a tab *before* the active one shifts the index under it.
        s.close(0);
        assert_eq!(s.current().map(|t| t.root.as_str()), Some("/c"));

        // Closing one *after* it must not move the selection at all.
        s.active = 0;
        s.close(1);
        assert_eq!(s.current().map(|t| t.root.as_str()), Some("/b"));
    }

    /// Closing the last tab must leave a coherent state, not an index into
    /// nothing.
    #[test]
    fn closing_every_tab_leaves_an_empty_panel() {
        let mut s = shells(&[("/a", 0)]);
        s.close(0);
        assert!(s.tabs.is_empty());
        assert!(s.current().is_none());
        s.close(0); // out of range, and not a panic
    }

    /// The panel's shape survives a restart — that is what makes the tabs worth
    /// having, since each one re-attaches to a tmux session that outlived the
    /// window.
    #[test]
    fn the_panel_round_trips_through_the_stored_form() {
        let mut s = Shells::default();
        s.open_in("/home/k/repo");
        s.open_in("/home/k/other");
        s.open_in("/home/k/repo");
        s.open = true;
        s.height = 320.0;
        s.select(1);

        let (panel, tabs) = s.to_prefs();
        let back = Shells::from_prefs(&panel, &tabs);
        assert!(back.open);
        assert_eq!(back.height, 320.0);
        assert_eq!(back.active, 1);
        let names: Vec<String> = back.tabs.iter().map(|t| t.session_name()).collect();
        let before: Vec<String> = s.tabs.iter().map(|t| t.session_name()).collect();
        assert_eq!(names, before, "restored tabs must reach the same sessions");
        assert!(back.tabs.iter().all(|t| t.term.is_none()), "spawned lazily");
    }

    /// A preferences file naming a tab that is not there — hand-edited, or
    /// written by a build that stored one more — must not index out of bounds.
    #[test]
    fn a_stored_selection_past_the_end_is_repaired() {
        let p = TerminalPanel {
            open: true,
            height: f32::NAN,
            active: 7,
            ..Default::default()
        };
        let tabs = vec![ShellRef { root: "/a".into(), ordinal: 0, name: None }];
        let s = Shells::from_prefs(&p, &tabs);
        assert_eq!(s.active, 0);
        assert_eq!(s.height, DEFAULT_HEIGHT, "a nonsense height is refused");

        let empty = Shells::from_prefs(&TerminalPanel { active: 3, ..p }, &[]);
        assert_eq!(empty.active, 0);
        assert!(empty.current().is_none());
    }

    /// A height below the floor would open a panel with no room for a prompt;
    /// one above the ceiling comes from a corrupt file, not a drag.
    #[test]
    fn a_stored_height_is_clamped_to_something_usable() {
        assert_eq!(sane_height(10.0), MIN_HEIGHT);
        assert_eq!(sane_height(99_000.0), 2000.0);
        assert_eq!(sane_height(300.0), 300.0);
    }

    /// The tab label is the directory, not the tmux name — and the ordinal
    /// only shows up once it distinguishes something.
    #[test]
    fn tab_titles_name_the_worktree_and_number_only_the_extras() {
        let s = shells(&[("/home/k/mogeung", 0), ("/home/k/mogeung", 1)]);
        assert_eq!(s.tabs[0].title(), "mogeung");
        assert_eq!(s.tabs[1].title(), "mogeung 2");

        // A root with no basename still gives a label rather than an empty tab.
        let odd = shells(&[("/", 0)]);
        assert!(!odd.tabs[0].title().is_empty());
    }

    /// `R-B34`. A name replaces the label and nothing else — most of all not
    /// the tmux session, which is what reaches the shell from a real terminal
    /// and what the next launch re-attaches to.
    #[test]
    fn renaming_a_tab_changes_the_label_and_not_the_session() {
        let mut s = shells(&[("/home/k/mogeung", 0), ("/home/k/mogeung", 1)]);
        let sessions: Vec<String> = s.tabs.iter().map(|t| t.session_name()).collect();

        rename(&mut s, 1, "tests");
        assert_eq!(s.tabs[1].title(), "tests");
        assert_eq!(s.tabs[0].title(), "mogeung", "the other tab is untouched");
        assert_eq!(
            s.tabs.iter().map(|t| t.session_name()).collect::<Vec<_>>(),
            sessions,
            "a rename must not move a shell to a different tmux session"
        );
        assert!(s.dirty, "a name is worth writing to disk");
    }

    /// Clearing the name — an empty field, or the menu item — puts the derived
    /// one back rather than leaving a tab with nothing written on it.
    #[test]
    fn a_cleared_name_falls_back_to_the_worktree() {
        let mut s = shells(&[("/home/k/mogeung", 1)]);
        rename(&mut s, 0, "tests");
        rename(&mut s, 0, "   ");
        assert_eq!(s.tabs[0].name, None);
        assert_eq!(s.tabs[0].title(), "mogeung 2");

        rename(&mut s, 0, "tests");
        s.clear_name(0);
        assert_eq!(s.tabs[0].title(), "mogeung 2");
        // And typing the derived name back is agreement, not an override —
        // storing it would pin a label that is already what it says.
        rename(&mut s, 0, "mogeung 2");
        assert_eq!(s.tabs[0].name, None);
    }

    /// The edit is seeded with what the tab already says, and Escape leaves
    /// that alone — a rename you backed out of must not have happened.
    #[test]
    fn an_abandoned_rename_changes_nothing() {
        let mut s = shells(&[("/home/k/mogeung", 0)]);
        s.begin_rename(0);
        assert_eq!(
            s.rename.as_ref().map(|r| r.text.as_str()),
            Some("mogeung"),
            "seeded with the current label, not blank"
        );
        s.rename.as_mut().unwrap().text = "something else".into();
        s.cancel_rename();
        assert_eq!(s.tabs[0].title(), "mogeung");
        assert!(!s.dirty);

        // Committing with nothing in flight is a no-op, not a panic — the tab
        // bar sends a commit on lost focus, which can arrive twice.
        s.commit_rename();
        assert_eq!(s.tabs[0].title(), "mogeung");
    }

    /// The edit names a tab by index, so anything that moves the tabs under it
    /// must end it. Otherwise closing a tab to the left of the one you were
    /// renaming applies your name to a different shell.
    #[test]
    fn moving_the_tabs_abandons_a_rename_in_flight() {
        let mut s = shells(&[("/a", 0), ("/b", 0), ("/c", 0)]);
        s.begin_rename(2);
        s.close(0);
        assert!(s.rename.is_none());
        assert_eq!(s.tabs.iter().filter(|t| t.name.is_some()).count(), 0);

        s.begin_rename(0);
        s.select(1);
        assert!(s.rename.is_none());

        s.begin_rename(0);
        s.open_in("/d");
        assert!(s.rename.is_none());
    }

    /// Renaming a background tab brings it to the front first: a name applied
    /// to a shell you cannot see reads as a bug even when it is what you asked
    /// for.
    #[test]
    fn renaming_a_tab_selects_it() {
        let mut s = shells(&[("/a", 0), ("/b", 0)]);
        s.begin_rename(1);
        assert_eq!(s.active, 1);
        assert!(s.rename.is_some(), "selecting must not cancel the edit it started");
    }

    /// A name is pasted as often as typed, and a paste carries whatever was on
    /// the clipboard: a newline would break the row, and a command line would
    /// push every other tab off the end of it.
    #[test]
    fn a_pasted_name_cannot_break_the_tab_bar() {
        let mut s = shells(&[("/a", 0)]);
        rename(&mut s, 0, "  cargo\ttest\nsecond line  ");
        let name = s.tabs[0].name.clone().expect("named");
        assert!(!name.contains('\n') && !name.contains('\t'), "{name:?}");
        assert_eq!(name.chars().count(), name.trim().chars().count());

        rename(&mut s, 0, &"x".repeat(400));
        assert_eq!(s.tabs[0].title().chars().count(), NAME_LIMIT);
    }

    /// Names survive a restart — a tab called `tests` that came back as
    /// `mogeung 2` would be a setting that quietly does not stick.
    #[test]
    fn names_round_trip_through_the_stored_form() {
        let mut s = shells(&[("/home/k/repo", 0), ("/home/k/repo", 1)]);
        rename(&mut s, 1, "tests");
        let (panel, tabs) = s.to_prefs();
        let back = Shells::from_prefs(&panel, &tabs);
        assert_eq!(back.tabs[1].title(), "tests");
        assert_eq!(back.tabs[0].title(), "repo");
        assert!(back.rename.is_none(), "a half-typed name is not a name");
    }

    /// The preferences file is hand-editable, so a name that could not be
    /// typed must not be loadable either.
    #[test]
    fn a_hand_written_name_is_cleaned_on_the_way_in() {
        let p = TerminalPanel {
            open: true,
            height: DEFAULT_HEIGHT,
            active: 0,
            ..Default::default()
        };
        let tabs = vec![
            ShellRef { root: "/a".into(), ordinal: 0, name: Some("  ".into()) },
            ShellRef {
                root: "/b".into(),
                ordinal: 0,
                name: Some("a\nname".into()),
            },
        ];
        let s = Shells::from_prefs(&p, &tabs);
        assert_eq!(s.tabs[0].name, None, "whitespace is not a name");
        assert_eq!(s.tabs[1].name.as_deref(), Some("aname"));
    }
}
