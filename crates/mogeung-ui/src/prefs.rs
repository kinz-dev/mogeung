//! View preferences that survive a restart.
//!
//! Client-side at `~/.mogeung/prefs.json`, for the same reason the keymap is
//! ([ADR-0001](../../../docs/decisions/0001-rust-core-with-egui-ui.md)): none
//! of this is daemon state. Which sessions *you* have hidden in *this* window
//! says nothing about the sessions themselves, and a second client should not
//! inherit it.
//!
//! ## Hiding is not forgetting
//!
//! `ClientMsg::ForgetSession` already exists and is destructive — it stops
//! tracking a session and drops its review marks. Hiding does neither. It is a
//! view filter, reversible from the panel, and the daemon never hears about it.
//! Keeping them separate matters: "get this out of my way" and "throw away what
//! I have read" must not be one button.
//!
//! ## Two files, because two different things were in one. `R-I11`
//!
//! [`ADR-0013`](../../../docs/decisions/0013-one-window-one-daemon.md) settled
//! that a window watches one daemon, which means watching two machines means
//! two windows — so this file stopped being something only one process writes.
//! Whole-file `write` with no merge means the last one to save wins, and the
//! other window's pins and labels are simply gone.
//!
//! Scoping the *whole* file per daemon would have been the obvious fix and the
//! wrong one: you would then set your theme, your keymap and your window size
//! again for every machine you watch. Those describe **this window**. So the
//! split is by what the state is *about*:
//!
//! - `~/.mogeung/prefs.json` — the window. Theme, layout, fonts, zoom,
//!   geometry, which filters are on. Shared by every window, and two windows
//!   can still race here. That race costs a setting you can see and redo, and
//!   is not worth a lock file.
//! - `~/.mogeung/state/<machine>.json` — [`Scoped`], everything keyed by a
//!   session id or a path on the watched machine. This is what was actually
//!   being lost, and it is the half that was never shareable anyway: a session
//!   id from the dev box means nothing on the laptop, and `~/projects/mogeung`
//!   means *different files* on each.
//!
//! `<machine>` is `R-I5`'s `machine_id`, not the URL — an `ssh -L` tunnel makes
//! a remote daemon answer on `127.0.0.1`, and keying on the address would file
//! the dev box's pins under the laptop.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Which sessions the queue shows at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Only sessions asking for something. The default, and the product's whole
    /// thesis: the queue answers "where do I look", not "what exists".
    #[default]
    NeedsYou,
    /// Everything currently running, busy or not.
    Live,
    /// Everything mogeung knows about, including finished and reviewed.
    All,
}

impl Scope {
    pub const ALL: [Scope; 3] = [Scope::NeedsYou, Scope::Live, Scope::All];

    pub fn label(self) -> &'static str {
        match self {
            Scope::NeedsYou => "needs you",
            Scope::Live => "live",
            Scope::All => "all",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Scope::NeedsYou => "waiting, blocked, failed, stalled, or unreviewed",
            Scope::Live => "every session still running",
            Scope::All => "everything, including finished and reviewed",
        }
    }
}

/// Client state that belongs to one watched machine. `R-I11`.
///
/// Everything in here is keyed by a session id or by a path *on that machine*.
/// Session ids are unique, so merging two machines' would not collide — it
/// would just accumulate entries that can never match anything again. Paths
/// are the real hazard: `editor_wrap` and the shell roots are worktree paths,
/// and the same checkout path on a laptop and a dev box is the normal case,
/// not an edge one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scoped {
    /// Sessions to keep out of the queue. Reversible, never destructive.
    #[serde(default)]
    pub hidden: BTreeSet<String>,
    /// Sessions to keep at the top regardless of rank.
    #[serde(default)]
    pub pinned: BTreeSet<String>,
    /// Session → the name the user gave it. `R-B26`. One label per session —
    /// it is a name, not a tag system.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Paths whose Editor tab wraps long lines — per file, because wrap is a
    /// property of prose files, not a mode you live in. `R-B29`.
    #[serde(default)]
    pub editor_wrap: BTreeSet<String>,
    /// Bookmarks: `(session, path, 1-based line)`, insertion order — which
    /// *is* the jump list. `R-B29`.
    #[serde(default)]
    pub bookmarks: Vec<(String, String, u64)>,
    /// Which shells the terminal panel holds. `R-B33`.
    ///
    /// Here rather than in [`TerminalPanel`] with the panel's own geometry,
    /// because a shell is rooted in a worktree *on the watched machine* while
    /// "the panel is 300pt tall" is about this window. Restoring one machine's
    /// tabs against another's daemon would have opened shells in directories
    /// that happen to share a name.
    #[serde(default)]
    pub shells: Vec<ShellRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
    /// The half of this that belongs to the daemon we are watching. Never in
    /// `prefs.json` — [`Prefs::adopt_origin`] loads and saves it separately.
    #[serde(skip)]
    pub scoped: Scoped,
    /// Which machine `scoped` was loaded for, so a switch can save it back
    /// where it came from rather than over whatever came next.
    #[serde(skip)]
    origin: Option<String>,
    /// What a pre-`R-I11` `prefs.json` held, read once so it can be moved into
    /// the first machine's [`Scoped`] and then never written again.
    ///
    /// Kept as a distinct field rather than loaded into `scoped` directly: at
    /// load time we do not yet know which machine we are talking to, and
    /// guessing would file the whole history under the wrong one.
    #[serde(default, flatten)]
    legacy: Legacy,
    #[serde(default)]
    pub scope: Scope,
    /// Temporarily reveal hidden sessions, so unhiding is possible.
    /// Not persisted — it is a peek, not a setting.
    #[serde(skip)]
    pub reveal_hidden: bool,

    /// Queue collapsed to a strip. Persisted, because a collapsed panel that
    /// came back on every launch would be a setting that does not stick.
    #[serde(default)]
    pub queue_collapsed: bool,

    #[serde(default)]
    pub group_by_repo: bool,
    #[serde(default)]
    pub auto_select: bool,
    #[serde(default = "yes")]
    pub preview_on_select: bool,

    #[serde(default)]
    pub hide_reviewed: bool,
    #[serde(default = "yes")]
    pub hide_noise: bool,
    #[serde(default = "yes")]
    pub syntax: bool,
    #[serde(default = "yes")]
    pub word_diff: bool,
    #[serde(default)]
    pub side_by_side: bool,

    /// Render assistant and human messages as Markdown in the transcript.
    #[serde(default = "yes")]
    pub markdown: bool,
    /// Show thinking blocks at all.
    #[serde(default = "yes")]
    pub show_thinking: bool,

    /// Per-pane content zoom, keyed by pane ("editor", "diff", "git",
    /// "transcript", "agent", "terminal") — Ctrl+wheel over a pane, not the global
    /// Ctrl+=/Ctrl+- which scales the whole window. Only non-default
    /// levels are stored, so the file stays quiet until you zoom.
    #[serde(default)]
    pub zoom: BTreeMap<String, f32>,

    /// Dark, light, or whatever the desktop says. `R-J6`.
    #[serde(default)]
    pub theme: crate::theme::Mode,

    /// The font family the terminal panes draw in. `R-B32`.
    ///
    /// A family name as the system knows it — "MesloLGS NF", "JetBrains Mono" —
    /// resolved against the installed fonts by [`crate::font`].
    ///
    /// `None` is *automatic*, not "bundled": it resolves through
    /// [`crate::font::PREFERRED`], which prefers MesloLGS NF where it is
    /// installed and falls back to the bundled monospace where it is not. The
    /// bundled font carries no Powerline or Nerd Font glyphs, so a prompt built
    /// out of them is a row of boxes — and the whole point of a default is that
    /// the common case does not have to find this setting first.
    ///
    /// Terminal-only, deliberately. The diff, the transcript and the Editor are
    /// monospace too and want the font that is known to carry mogeung's icons.
    #[serde(default)]
    pub terminal_font: Option<String>,
    /// The terminal's base size in points, before the pane's own zoom.
    #[serde(default = "default_terminal_px")]
    pub terminal_font_px: f32,

    /// The terminal panel: up or not, how tall, and which shells are in it.
    /// `R-B33`. Persisted because each tab re-attaches to a tmux session that
    /// outlived the window — a panel that forgot its tabs would leave them
    /// running and unreachable from here.
    #[serde(default)]
    pub terminal_panel: TerminalPanel,

    /// Where the window was, last time it closed. `R-J1`.
    ///
    /// Here rather than in eframe's own persistence, which stores a second
    /// copy of view state in a second format: two stores holding the same kind
    /// of thing is how they drift.
    #[serde(default)]
    pub window: Option<Window>,
}

/// A remembered window: outer position and inner size, in logical points.
///
/// Position is optional and size is not, because they fail differently. A size
/// is always usable; a position belongs to a monitor arrangement that may not
/// exist at the next launch, and restoring it blindly puts the window
/// somewhere you cannot reach it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
}

/// The smallest window we will restore to, matching `--min-inner-size`. A
/// stored size below this came from a corrupt file or a resize we mis-sampled,
/// and honouring it would open a window too small to use.
pub const MIN_SIZE: (f32, f32) = (900.0, 600.0);

impl Window {
    /// The stored geometry, minus anything unusable.
    ///
    /// Returns the size only when the position cannot be trusted: a window in
    /// the wrong place is a nuisance you fix by dragging it, and a window
    /// off-screen is one you cannot fix at all. `monitor` is the total desktop
    /// area in logical points, or `None` when the platform will not say — in
    /// which case the position is kept, since we have no grounds to drop it.
    pub fn usable(&self, monitor: Option<(f32, f32)>) -> Option<Window> {
        if !self.width.is_finite() || !self.height.is_finite() {
            return None;
        }
        if self.width < MIN_SIZE.0 || self.height < MIN_SIZE.1 {
            return None;
        }
        let mut out = *self;
        if let (Some(x), Some(y)) = (self.x, self.y) {
            let visible = match monitor {
                // A title bar needs to be on-screen to be grabbed, so the
                // test is that the window's top-left corner sits inside the
                // desktop with room to grab — not that the whole window fits,
                // which would refuse a deliberately oversized window.
                Some((w, h)) => {
                    x.is_finite()
                        && y.is_finite()
                        && x > -self.width + 120.0
                        && y >= 0.0
                        && x < w - 120.0
                        && y < h - 40.0
                }
                None => x.is_finite() && y.is_finite(),
            };
            if !visible {
                out.x = None;
                out.y = None;
            }
        } else {
            out.x = None;
            out.y = None;
        }
        Some(out)
    }
}

/// The machine-relative fields as a pre-`R-I11` `prefs.json` wrote them.
///
/// Flattened, so an old file deserialises straight into it and a migrated one
/// writes nothing back — every field skips serialising when empty, which is
/// what it is once [`Prefs::adopt_origin`] has moved it out.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Legacy {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    hidden: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pinned: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    labels: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    editor_wrap: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bookmarks: Vec<(String, String, u64)>,
}

/// The terminal panel's stored shape. `R-B33`.
///
/// Geometry only, since `R-I11`: which shells are in it moved to
/// [`Scoped::shells`], because a shell is rooted in a worktree on the *watched*
/// machine while the panel's height is a fact about this window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalPanel {
    /// On demand: closed on a fresh install, and whatever you left it at after.
    #[serde(default)]
    pub open: bool,
    #[serde(default = "default_panel_height")]
    pub height: f32,
    #[serde(default)]
    pub active: usize,
    /// Where the tab list used to live. Read from an old file and moved once;
    /// empty afterwards, so it stops being written.
    ///
    /// `pub` only so `..Default::default()` works where the panel is rebuilt.
    /// Nothing should read it but [`Prefs::take_legacy`].
    #[serde(default, rename = "shells", skip_serializing_if = "Vec::is_empty")]
    pub legacy_shells: Vec<ShellRef>,
}

impl Default for TerminalPanel {
    fn default() -> Self {
        TerminalPanel {
            open: false,
            height: default_panel_height(),
            active: 0,
            legacy_shells: Vec::new(),
        }
    }
}

/// One shell in the panel: where it is rooted, and which one it is there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellRef {
    pub root: String,
    /// 0 is the first shell in that worktree, and names the tmux session the
    /// single-shell build used — see `term::shell_session_name`.
    #[serde(default)]
    pub ordinal: u32,
    /// What you called this tab, if you called it anything. `R-B34`.
    ///
    /// A label and nothing else — the tmux session is still named by `root`
    /// and `ordinal`, so a file written by a build that had never heard of
    /// names still restores the same shells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A machine id, made safe to be a filename.
///
/// `machine_id` is 32 hex characters and needs none of this. The fallback path
/// does: when a daemon publishes no identity the window keys on its URL, and
/// `ws://box:7717/ws` contains separators that would silently become
/// directories. Anything outside `[A-Za-z0-9._-]` becomes `-`, and the result
/// is capped, so a long URL cannot produce a filename the filesystem refuses.
fn slug(origin: &str) -> String {
    let cleaned: String = origin
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    // A name of nothing but separators, or nothing at all, would collide with
    // every other such name — and `..` would be worse than a collision.
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn default_panel_height() -> f32 {
    crate::shells::DEFAULT_HEIGHT
}

fn default_terminal_px() -> f32 {
    14.0
}

fn yes() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            scoped: Scoped::default(),
            origin: None,
            legacy: Legacy::default(),
            scope: Scope::default(),
            reveal_hidden: false,
            queue_collapsed: false,
            group_by_repo: false,
            auto_select: false,
            preview_on_select: true,
            hide_reviewed: false,
            hide_noise: true,
            syntax: true,
            word_diff: true,
            side_by_side: false,
            markdown: true,
            show_thinking: true,
            zoom: BTreeMap::new(),
            theme: crate::theme::Mode::default(),
            terminal_font: None,
            terminal_font_px: default_terminal_px(),
            terminal_panel: TerminalPanel::default(),
            window: None,
        }
    }
}

impl Prefs {
    pub fn path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".mogeung").join("prefs.json")
    }

    /// Load, tolerating anything. A corrupt preferences file must never stop
    /// the window opening — the worst case is that you set your options again.
    pub fn load() -> (Self, Option<String>) {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };
        match serde_json::from_str::<Prefs>(&text) {
            Ok(p) => (p, None),
            Err(e) => (
                Self::default(),
                Some(format!(
                    "{} is unreadable ({e}) — using defaults",
                    path.display()
                )),
            ),
        }
    }

    /// Where every machine's [`Scoped`] state lives.
    fn state_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".mogeung").join("state")
    }

    /// Point this window's [`Scoped`] state at a machine.
    ///
    /// Called when a daemon publishes its identity — at launch, and again on
    /// every switch — because that is the first moment the machine is known.
    /// Before it, `scoped` is empty, which is correct rather than merely safe:
    /// there are no sessions on screen to hide or pin yet either.
    ///
    /// Returns a message worth showing, never an error worth stopping for.
    /// Losing view state is annoying; refusing to display a queue over it
    /// would be worse.
    pub fn adopt_origin(&mut self, origin: &str) -> Option<String> {
        self.adopt_origin_in(&Self::state_dir(), origin)
    }

    /// [`Prefs::adopt_origin`] against a named directory.
    ///
    /// Split out so the tests can exercise the part that matters — that
    /// switching machines saves one and loads the other — against a temporary
    /// directory. Reaching `$HOME` from a test would either write to a real
    /// person's state or need the environment mutated under every other test
    /// in the process.
    fn adopt_origin_in(&mut self, dir: &std::path::Path, origin: &str) -> Option<String> {
        if self.origin.as_deref() == Some(origin) {
            return None;
        }
        // Back where it came from, not over whatever comes next.
        let carried = self.origin.take();
        let mut note = carried.and_then(|old| self.write_scoped_in(dir, &old).err());

        let path = dir.join(format!("{}.json", slug(origin)));
        let existing = std::fs::read_to_string(&path).ok();
        self.scoped = match existing.as_deref() {
            Some(text) => match serde_json::from_str(text) {
                Ok(s) => s,
                Err(e) => {
                    note = Some(format!("{} is unreadable ({e}) — starting empty", path.display()));
                    Scoped::default()
                }
            },
            // No file for this machine yet. If an old `prefs.json` is still
            // carrying the pre-split state, this is where it lands — and it
            // lands on the *first* machine adopted, which `R-I7` guarantees is
            // LOCAL. Migrating into a remote would file the laptop's history
            // under the dev box.
            None => self.take_legacy(),
        };
        self.origin = Some(origin.to_string());
        note
    }

    /// Whether a machine has been adopted yet. Until it has, `scoped` is empty
    /// and nothing should be written into it — an edit made now would be saved
    /// against the first daemon to answer, which need not be the one it was
    /// about.
    pub fn origin(&self) -> Option<&str> {
        self.origin.as_deref()
    }

    /// Drain a pre-`R-I11` file's machine-relative state into one [`Scoped`].
    ///
    /// Draining rather than copying is what makes the migration happen once:
    /// what is left behind is empty, every legacy field skips serialising when
    /// empty, and the next `save` therefore writes a `prefs.json` that no
    /// longer carries any of it.
    fn take_legacy(&mut self) -> Scoped {
        let legacy = std::mem::take(&mut self.legacy);
        Scoped {
            hidden: legacy.hidden,
            pinned: legacy.pinned,
            labels: legacy.labels,
            editor_wrap: legacy.editor_wrap,
            bookmarks: legacy.bookmarks,
            // Nested one level down in the old file, so it does not ride the
            // flattened struct with the rest.
            shells: std::mem::take(&mut self.terminal_panel.legacy_shells),
        }
    }

    fn write_scoped_in(&self, dir: &std::path::Path, origin: &str) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.json", slug(origin)));
        let json = serde_json::to_string_pretty(&self.scoped).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        // Both halves or neither: they are edited by the same gestures, and a
        // window that saved its theme but not its pins would look like it had
        // saved.
        match &self.origin {
            Some(o) => self.write_scoped_in(&Self::state_dir(), o),
            None => Ok(()),
        }
    }

    pub fn is_hidden(&self, id: &str) -> bool {
        self.scoped.hidden.contains(id)
    }

    pub fn is_pinned(&self, id: &str) -> bool {
        self.scoped.pinned.contains(id)
    }

    /// Hide a session. Un-pins it too — a pinned-but-hidden session is a
    /// contradiction that would otherwise sit in the file forever.
    pub fn hide(&mut self, id: &str) {
        self.scoped.hidden.insert(id.to_string());
        self.scoped.pinned.remove(id);
    }

    pub fn unhide(&mut self, id: &str) {
        self.scoped.hidden.remove(id);
    }

    pub fn toggle_pin(&mut self, id: &str) {
        if !self.scoped.pinned.remove(id) {
            self.scoped.pinned.insert(id.to_string());
            // Pinning something hidden means you want to see it.
            self.scoped.hidden.remove(id);
        }
    }

    pub fn label(&self, id: &str) -> Option<&str> {
        self.scoped.labels.get(id).map(String::as_str)
    }

    /// Set or clear in one door: saving an empty label removes it, so the
    /// editor needs no separate delete affordance and the file never holds a
    /// label that renders as nothing.
    pub fn set_label(&mut self, id: &str, label: &str) {
        let label = label.trim();
        if label.is_empty() {
            self.scoped.labels.remove(id);
        } else {
            self.scoped.labels.insert(id.to_string(), label.to_string());
        }
    }

    /// The zoom factor for one pane; 1.0 unless the user set one.
    pub fn zoom_of(&self, pane: &str) -> f32 {
        self.zoom.get(pane).copied().unwrap_or(1.0)
    }

    /// Set a pane's zoom, clamped to what stays readable. Near-1.0 lands
    /// back on the default and leaves the file — reset by zooming back.
    pub fn set_zoom(&mut self, pane: &str, z: f32) {
        let z = z.clamp(0.5, 2.5);
        if (z - 1.0).abs() < 0.05 {
            self.zoom.remove(pane);
        } else {
            self.zoom.insert(pane.to_string(), z);
        }
    }

    /// `/clear` in Claude Code keeps the process but mints a fresh session
    /// id, so a hand-applied label lands on a dead id while the same work
    /// carries on under a new one. The live registry is per-*pid*, which
    /// makes succession a fact rather than a guess: a dead session and a
    /// live one sharing a pid are the same conversation. Labels and pins
    /// follow it.
    ///
    /// `sessions` is `(id, alive, pid, started_epoch, cwd)` for every
    /// known session. The cwd must match as well as the pid: pids are
    /// reused by the OS eventually, and a label jumping onto an unrelated
    /// session that happened to inherit a number would be worse than the
    /// bug this fixes. Conservative on purpose: a label never overwrites
    /// one the successor was given by hand, and nothing is invented —
    /// only moved. Returns whether anything changed, so the caller can
    /// mark the prefs dirty.
    pub fn migrate_succession(
        &mut self,
        sessions: &[(String, bool, Option<u32>, i64, String)],
    ) -> bool {
        let mut changed = false;
        for (succ_id, alive, pid, _, cwd) in sessions {
            if !alive {
                continue;
            }
            let Some(pid) = pid else { continue };
            // The latest dead session on the same pid *and* cwd is the
            // immediate predecessor — /clear twice makes a chain, and
            // label state walks it one hop per pass.
            let pred = sessions
                .iter()
                .filter(|(id, alive, p, _, c)| {
                    !alive && id != succ_id && p.as_ref() == Some(pid) && c == cwd
                })
                .max_by_key(|(_, _, _, started, _)| *started)
                .map(|(id, _, _, _, _)| id.clone());
            let Some(pred_id) = pred else { continue };
            if self.label(succ_id).is_none() {
                if let Some(label) = self.label(&pred_id).map(str::to_string) {
                    self.scoped.labels.remove(&pred_id);
                    self.scoped.labels.insert(succ_id.clone(), label);
                    changed = true;
                }
            }
            if self.scoped.pinned.contains(&pred_id) && !self.scoped.pinned.contains(succ_id) {
                self.scoped.pinned.remove(&pred_id);
                self.scoped.pinned.insert(succ_id.clone());
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_state(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mogeung-prefs-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A `prefs.json` as every build before `R-I11` wrote one.
    const LEGACY: &str = r#"{
        "hidden": ["dead-one"],
        "pinned": ["keep-me"],
        "labels": {"keep-me": "the refactor"},
        "editor_wrap": ["/home/kinz/notes.md"],
        "bookmarks": [["keep-me", "/home/kinz/src/main.rs", 42]],
        "theme": "dark",
        "terminal_panel": {
            "open": true,
            "height": 300.0,
            "active": 1,
            "shells": [{"root": "/home/kinz/projects/mogeung", "ordinal": 0}]
        }
    }"#;

    /// Upgrading must not read as "someone else cleared my pins". The whole
    /// history moves to the first machine adopted, which `R-I7` guarantees is
    /// the local one.
    #[test]
    fn an_old_prefs_file_migrates_whole_into_the_first_machine() {
        let dir = temp_state("migrate");
        let mut p: Prefs = serde_json::from_str(LEGACY).unwrap();
        assert!(p.scoped.hidden.is_empty(), "nothing is scoped before a machine is known");

        p.adopt_origin_in(&dir, "machine-aaa");

        assert!(p.is_hidden("dead-one"));
        assert!(p.is_pinned("keep-me"));
        assert_eq!(p.label("keep-me"), Some("the refactor"));
        assert!(p.scoped.editor_wrap.contains("/home/kinz/notes.md"));
        assert_eq!(p.scoped.bookmarks.len(), 1);
        assert_eq!(p.scoped.shells.len(), 1, "terminal tabs migrate too");
        // Window-level settings stay where they were.
        assert!(p.terminal_panel.open);
        assert_eq!(p.terminal_panel.height, 300.0);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Migration must happen once. If the legacy fields kept being written,
    /// every later launch would re-import them over whatever the machine's own
    /// file had since become — quietly resurrecting hidden sessions.
    #[test]
    fn a_migrated_prefs_file_stops_carrying_the_machine_half() {
        let dir = temp_state("once");
        let mut p: Prefs = serde_json::from_str(LEGACY).unwrap();
        p.adopt_origin_in(&dir, "machine-aaa");

        let written = serde_json::to_string(&p).unwrap();
        for gone in ["dead-one", "keep-me", "notes.md", "the refactor", "projects/mogeung"] {
            assert!(!written.contains(gone), "{gone} is still in prefs.json: {written}");
        }
        // And what belongs to the window is still there.
        assert!(written.contains("terminal_panel"));

        // Re-reading what we just wrote must not resurrect anything.
        let mut again: Prefs = serde_json::from_str(&written).unwrap();
        again.adopt_origin_in(&dir, "machine-bbb");
        assert!(again.scoped.hidden.is_empty(), "a second machine inherits nothing");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The defect `R-I11` exists for: two windows on two machines, each
    /// keeping its own pins, neither overwriting the other.
    #[test]
    fn switching_machines_saves_one_and_loads_the_other() {
        let dir = temp_state("switch");
        let mut p = Prefs::default();

        p.adopt_origin_in(&dir, "laptop");
        p.hide("session-on-laptop");
        p.set_label("session-on-laptop", "the laptop one");

        p.adopt_origin_in(&dir, "devbox");
        assert!(!p.is_hidden("session-on-laptop"), "the devbox starts clean");
        p.hide("session-on-devbox");

        p.adopt_origin_in(&dir, "laptop");
        assert!(p.is_hidden("session-on-laptop"), "the laptop's state came back");
        assert!(!p.is_hidden("session-on-devbox"), "and did not gain the devbox's");
        assert_eq!(p.label("session-on-laptop"), Some("the laptop one"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Snapshots arrive continuously and a reconnect sends another. Re-adopting
    /// the machine we are already on must not touch the disk, and above all
    /// must not reload — an in-memory pin made since the last save would be
    /// read back over.
    #[test]
    fn re_adopting_the_same_machine_changes_nothing() {
        let dir = temp_state("idempotent");
        let mut p = Prefs::default();
        p.adopt_origin_in(&dir, "laptop");
        p.hide("just-hidden");

        p.adopt_origin_in(&dir, "laptop");
        assert!(p.is_hidden("just-hidden"), "an unsaved change survived the no-op");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A daemon that publishes no `machine_id` is keyed by its URL, and a URL
    /// is full of characters that would turn a filename into a path.
    #[test]
    fn an_origin_never_escapes_its_filename() {
        assert_eq!(slug("ws://box:7717/ws"), "ws---box-7717-ws");
        assert_eq!(slug("a1b2c3d4e5f6"), "a1b2c3d4e5f6");
        // Traversal: no separator survives, so the result cannot leave the
        // state directory however the origin was chosen.
        assert_eq!(slug("../../etc/passwd"), "..-..-etc-passwd");
        assert!(!slug("../../etc/passwd").contains(std::path::MAIN_SEPARATOR));
        // `..` is the one that would still name a directory on its own.
        assert_eq!(slug(".."), "unknown");
        assert_eq!(slug("."), "unknown");
        assert_eq!(slug(""), "unknown");
        assert_eq!(slug("///"), "unknown");
        // Two different machines never share a file.
        assert_ne!(slug("machine-a"), slug("machine-b"));
    }

    /// The whole point of `R-J1`: geometry survives the round trip through the
    /// file, rather than being remembered only in memory.
    #[test]
    fn geometry_round_trips_through_the_stored_form() {
        let mut p = Prefs::default();
        assert!(p.window.is_none(), "a fresh install has no remembered window");
        p.window = Some(Window {
            width: 1600.0,
            height: 1000.0,
            x: Some(120.0),
            y: Some(64.0),
        });
        let back: Prefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.window, p.window);
    }

    /// A file written before `R-J1` has no `window` key at all, and must still
    /// load — the same tolerance every other field here has.
    #[test]
    fn a_prefs_file_from_before_geometry_still_loads() {
        let old = r#"{"hidden":[],"pinned":[],"scope":"needs_you"}"#;
        let p: Prefs = serde_json::from_str(old).unwrap();
        assert!(p.window.is_none());
    }

    /// The case that loses a window: it was closed on a second monitor that is
    /// no longer attached, so the stored position names a place that does not
    /// exist. Size is still worth keeping — only the position is suspect.
    #[test]
    fn a_position_off_the_monitor_is_dropped_and_the_size_kept() {
        let w = Window {
            width: 1600.0,
            height: 1000.0,
            x: Some(3000.0),
            y: Some(200.0),
        };
        let got = w.usable(Some((1920.0, 1080.0))).expect("size stays usable");
        assert_eq!((got.width, got.height), (1600.0, 1000.0));
        assert_eq!((got.x, got.y), (None, None), "unreachable position dropped");

        // And the ordinary case is left alone.
        let ok = Window { x: Some(80.0), y: Some(40.0), ..w };
        assert_eq!(ok.usable(Some((1920.0, 1080.0))), Some(ok));
    }

    /// Partly off-screen to the left is normal and must survive — a window
    /// whose title bar is still grabbable is where the user put it. Fully off
    /// to the left is not.
    #[test]
    fn a_window_hanging_off_an_edge_is_kept_while_its_title_bar_is_reachable() {
        let base = Window { width: 1000.0, height: 700.0, x: Some(-60.0), y: Some(0.0) };
        let monitor = Some((1920.0, 1080.0));
        assert!(base.usable(monitor).unwrap().x.is_some(), "still grabbable");

        let gone = Window { x: Some(-1000.0), ..base };
        assert_eq!(gone.usable(monitor).unwrap().x, None, "entirely off-screen");
    }

    /// Nonsense in the file must not open a window nobody can use. A size
    /// below the minimum is refused outright rather than clamped: the stored
    /// value is not trustworthy, so the built-in default is the better answer.
    #[test]
    fn an_impossible_size_falls_back_to_the_default() {
        for bad in [
            Window { width: 10.0, height: 10.0, x: None, y: None },
            Window { width: f32::NAN, height: 900.0, x: None, y: None },
            Window { width: 1400.0, height: f32::INFINITY, x: None, y: None },
        ] {
            assert_eq!(bad.usable(Some((1920.0, 1080.0))), None, "{bad:?}");
        }
    }

    /// `R-J6`. The theme has to survive a restart — a preference that resets
    /// every launch is not a preference — and a file written before the theme
    /// existed must still load, defaulting to the dark it was drawn in.
    #[test]
    fn the_theme_round_trips_and_an_older_file_defaults_to_dark() {
        let mut p = Prefs::default();
        assert_eq!(p.theme, crate::theme::Mode::Dark, "dark stays the default");
        p.theme = crate::theme::Mode::Light;
        let back: Prefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.theme, crate::theme::Mode::Light);

        let older: Prefs = serde_json::from_str(r#"{"hidden":[],"scope":"live"}"#).unwrap();
        assert_eq!(older.theme, crate::theme::Mode::Dark);
    }

    /// A size with no position is a whole answer, not a broken one.
    ///
    /// This is what Wayland gives us: `outer_position()` is unsupported there,
    /// so there is never a position to store and the compositor places the
    /// window itself. Treating a positionless geometry as unusable would mean
    /// the window forgot its size on every Linux desktop running Wayland —
    /// which is the machine this was written on.
    #[test]
    fn a_size_without_a_position_is_still_worth_restoring() {
        let w = Window { width: 1180.0, height: 742.0, x: None, y: None };
        assert_eq!(w.usable(Some((1920.0, 1080.0))), Some(w));
        assert_eq!(w.usable(None), Some(w));
    }

    /// With no monitor reported — a platform that will not say, which is the
    /// state at startup before the window exists — a stored position is kept.
    /// Dropping it there would break the ordinary case to guard the rare one.
    #[test]
    fn an_unknown_monitor_keeps_the_stored_position() {
        let w = Window { width: 1400.0, height: 900.0, x: Some(3000.0), y: Some(10.0) };
        assert_eq!(w.usable(None), Some(w));
    }

    #[test]
    fn defaults_are_the_opinionated_ones() {
        let p = Prefs::default();
        // The queue exists to answer "where do I look", so it starts filtered.
        assert_eq!(p.scope, Scope::NeedsYou);
        assert!(p.preview_on_select);
        assert!(p.hide_noise);
        assert!(p.scoped.hidden.is_empty());
    }

    #[test]
    fn hiding_and_unhiding_round_trip() {
        let mut p = Prefs::default();
        p.hide("abc");
        assert!(p.is_hidden("abc"));
        p.unhide("abc");
        assert!(!p.is_hidden("abc"));
    }

    #[test]
    fn a_session_cannot_be_both_pinned_and_hidden() {
        let mut p = Prefs::default();
        p.toggle_pin("abc");
        assert!(p.is_pinned("abc"));

        p.hide("abc");
        assert!(p.is_hidden("abc"));
        assert!(!p.is_pinned("abc"), "hiding must drop the pin");

        p.toggle_pin("abc");
        assert!(p.is_pinned("abc"));
        assert!(!p.is_hidden("abc"), "pinning must reveal it");
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_everything_else() {
        // What a hand-edit or an older version produces.
        let mut p: Prefs = serde_json::from_str(r#"{ "hidden": ["a", "b"] }"#).unwrap();
        // A top-level `hidden` is a pre-`R-I11` file, so it arrives as legacy
        // and reaches the queue only once a machine is known.
        let dir = temp_state("partial");
        p.adopt_origin_in(&dir, "machine");
        assert_eq!(p.scoped.hidden.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
        assert!(p.preview_on_select, "missing fields must fall back, not default to false");
        assert!(p.markdown);
        assert!(p.hide_noise);
        assert_eq!(p.scope, Scope::NeedsYou);
    }

    /// Each half survives through its own file, and — the point of `R-I11` —
    /// the machine half is not in `prefs.json` at all, because that is the file
    /// a second window overwrites.
    #[test]
    fn each_half_survives_a_round_trip_through_its_own_file() {
        let dir = temp_state("roundtrip");
        let mut p = Prefs::default();
        p.adopt_origin_in(&dir, "machine");
        p.hide("gone");
        p.toggle_pin("kept");
        p.scope = Scope::All;
        p.side_by_side = true;

        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("gone"), "a session id must not be in prefs.json");
        assert!(!json.contains("kept"));

        let mut back: Prefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back.scope, Scope::All, "the window half rides prefs.json");
        assert!(back.side_by_side);
        assert!(!back.is_hidden("gone"), "and carries none of the machine half");

        p.write_scoped_in(&dir, "machine").unwrap();
        back.adopt_origin_in(&dir, "machine");
        assert!(back.is_hidden("gone"), "which comes back from the machine's own file");
        assert!(back.is_pinned("kept"));

        std::fs::remove_dir_all(&dir).ok();
    }

    /// `reveal_hidden` is a peek, not a setting: persisting it would mean a
    /// restart quietly undid every hide.
    #[test]
    fn revealing_hidden_sessions_is_not_persisted() {
        let mut p = Prefs::default();
        p.reveal_hidden = true;
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("reveal"), "reveal_hidden leaked into the file");
        let back: Prefs = serde_json::from_str(&json).unwrap();
        assert!(!back.reveal_hidden);
    }

    /// Zoom is clamped, near-default self-erases, and unknown panes read
    /// as 1.0 — the file only ever holds deliberate levels.
    #[test]
    fn pane_zoom_clamps_and_default_erases() {
        let mut p = Prefs::default();
        assert_eq!(p.zoom_of("diff"), 1.0);
        p.set_zoom("diff", 1.5);
        assert_eq!(p.zoom_of("diff"), 1.5);
        assert_eq!(p.zoom_of("editor"), 1.0, "panes zoom independently");
        p.set_zoom("diff", 9.0);
        assert_eq!(p.zoom_of("diff"), 2.5, "clamped, not trusted");
        p.set_zoom("diff", 0.1);
        assert_eq!(p.zoom_of("diff"), 0.5);
        p.set_zoom("diff", 1.02);
        assert!(p.zoom.is_empty(), "near-1.0 must erase the entry, not store it");
    }

    /// The `/clear` case: same pid, new session id — the label and pin
    /// must follow the work, not die with the old id.
    #[test]
    fn labels_and_pins_follow_a_cleared_session_to_its_successor() {
        let mut p = Prefs::default();
        p.set_label("old", "api-work");
        p.toggle_pin("old");
        let sessions = vec![
            ("old".to_string(), false, Some(4242), 100, "/repo".to_string()),
            ("new".to_string(), true, Some(4242), 200, "/repo".to_string()),
        ];
        assert!(p.migrate_succession(&sessions));
        assert_eq!(p.label("new"), Some("api-work"));
        assert_eq!(p.label("old"), None, "the dead id must not keep a shadow copy");
        assert!(p.is_pinned("new"));
        assert!(!p.is_pinned("old"));
        // Idempotent: a second pass over the same facts moves nothing.
        assert!(!p.migrate_succession(&sessions));
    }

    /// A label given to the successor by hand wins over inheritance.
    #[test]
    fn succession_never_overwrites_a_hand_applied_label() {
        let mut p = Prefs::default();
        p.set_label("old", "stale-name");
        p.set_label("new", "fresh-name");
        let sessions = vec![
            ("old".to_string(), false, Some(1), 100, "/repo".to_string()),
            ("new".to_string(), true, Some(1), 200, "/repo".to_string()),
        ];
        p.migrate_succession(&sessions);
        assert_eq!(p.label("new"), Some("fresh-name"));
        assert_eq!(p.label("old"), Some("stale-name"), "the loser stays put, not deleted");
    }

    /// Two `/clear`s before a scan: the *latest* predecessor is the one
    /// whose state carries — and only pid-sharers are ever considered.
    #[test]
    fn succession_picks_the_latest_predecessor_and_ignores_strangers() {
        let mut p = Prefs::default();
        p.set_label("first", "renamed-early");
        p.set_label("second", "current-name");
        p.set_label("other-pid", "unrelated");
        let sessions = vec![
            ("first".to_string(), false, Some(7), 100, "/repo".to_string()),
            ("second".to_string(), false, Some(7), 200, "/repo".to_string()),
            ("other-pid".to_string(), false, Some(9), 300, "/repo".to_string()),
            ("third".to_string(), true, Some(7), 400, "/repo".to_string()),
            ("no-pid".to_string(), true, None, 500, "/repo".to_string()),
        ];
        p.migrate_succession(&sessions);
        assert_eq!(p.label("third"), Some("current-name"));
        assert_eq!(p.label("first"), Some("renamed-early"), "only the immediate predecessor moves");
        assert_eq!(p.label("other-pid"), Some("unrelated"));
        assert_eq!(p.label("no-pid"), None);
    }

    /// A reused pid in a different directory is a stranger, not a
    /// successor — the OS hands pid numbers out again eventually.
    #[test]
    fn a_reused_pid_in_another_directory_inherits_nothing() {
        let mut p = Prefs::default();
        p.set_label("old", "api-work");
        let sessions = vec![
            ("old".to_string(), false, Some(4242), 100, "/repo-a".to_string()),
            ("new".to_string(), true, Some(4242), 200, "/repo-b".to_string()),
        ];
        assert!(!p.migrate_succession(&sessions));
        assert_eq!(p.label("old"), Some("api-work"));
        assert_eq!(p.label("new"), None);
    }

    /// Two live sessions never trade state — succession requires a death.
    #[test]
    fn succession_requires_a_dead_predecessor() {
        let mut p = Prefs::default();
        p.set_label("a", "mine");
        let sessions = vec![
            ("a".to_string(), true, Some(3), 100, "/repo".to_string()),
            ("b".to_string(), true, Some(3), 200, "/repo".to_string()),
        ];
        assert!(!p.migrate_succession(&sessions));
        assert_eq!(p.label("a"), Some("mine"));
        assert_eq!(p.label("b"), None);
    }

    #[test]
    fn labels_set_replace_remove_and_round_trip() {
        let dir = temp_state("labels");
        let mut p = Prefs::default();
        // Before a machine is adopted there is nowhere for a label to live —
        // and adopting one replaces whatever is in memory with what is on
        // disk, so it has to come first.
        p.adopt_origin_in(&dir, "machine");
        p.set_label("a", "risky one");
        assert_eq!(p.label("a"), Some("risky one"));
        p.set_label("a", "  safe now  ");
        assert_eq!(p.label("a"), Some("safe now"), "replacing trims and overwrites");

        p.write_scoped_in(&dir, "machine").unwrap();
        let mut back = Prefs::default();
        back.adopt_origin_in(&dir, "machine");
        assert_eq!(back.label("a"), Some("safe now"));
        std::fs::remove_dir_all(&dir).ok();

        p.set_label("a", "   ");
        assert_eq!(p.label("a"), None, "an empty label is a removal");
        assert!(p.scoped.labels.is_empty(), "removal must not leave an empty entry behind");
    }

    /// `R-B32`/`R-B33`. Both settings have to survive a restart, and a file
    /// written before either existed must still load — the terminal's font is
    /// not a thing anyone wants to choose twice a day, and a panel that forgot
    /// its tabs would leave their tmux sessions running with nothing pointing
    /// at them.
    #[test]
    fn the_terminal_font_and_panel_round_trip_and_older_files_still_load() {
        let mut p = Prefs::default();
        assert_eq!(p.terminal_font, None, "the bundled monospace until asked");
        assert!(!p.terminal_panel.open, "on demand, so closed on a fresh install");

        p.terminal_font = Some("MesloLGS NF".into());
        p.terminal_font_px = 15.0;
        p.terminal_panel = TerminalPanel {
            open: true,
            height: 300.0,
            active: 1,
            ..Default::default()
        };
        p.scoped.shells = vec![
            ShellRef { root: "/home/k/repo".into(), ordinal: 0, name: None },
            ShellRef {
                root: "/home/k/repo".into(),
                ordinal: 1,
                name: Some("tests".into()),
            },
        ];
        let back: Prefs = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back.terminal_font.as_deref(), Some("MesloLGS NF"));
        assert_eq!(back.terminal_font_px, 15.0);
        assert_eq!(back.terminal_panel, p.terminal_panel);

        let older: Prefs = serde_json::from_str(r#"{"hidden":[],"scope":"live"}"#).unwrap();
        assert_eq!(older.terminal_font, None);
        assert_eq!(older.terminal_font_px, 14.0, "not zero — an unreadable terminal");
        assert_eq!(older.terminal_panel, TerminalPanel::default());

        // A shell written before ordinals or names existed is the first one,
        // unnamed — not a parse error that would take the whole file down.
        // It arrives on the legacy field, which is where `R-I11` reads it from.
        let one: TerminalPanel =
            serde_json::from_str(r#"{"open":true,"shells":[{"root":"/a"}]}"#).unwrap();
        assert_eq!(one.legacy_shells[0].ordinal, 0);
        assert_eq!(one.legacy_shells[0].name, None);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<Prefs>("not json").is_err());
        assert!(serde_json::from_str::<Prefs>("{}").is_ok());
    }
}
