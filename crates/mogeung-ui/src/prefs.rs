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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prefs {
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
    /// "transcript", "terminal") — Ctrl+wheel over a pane, not the global
    /// Ctrl+=/Ctrl+- which scales the whole window. Only non-default
    /// levels are stored, so the file stays quiet until you zoom.
    #[serde(default)]
    pub zoom: BTreeMap<String, f32>,

    /// Paths whose Editor tab wraps long lines — per file, because wrap is a
    /// property of prose files, not a mode you live in. `R-B29`.
    #[serde(default)]
    pub editor_wrap: BTreeSet<String>,
    /// Bookmarks: `(session, path, 1-based line)`, insertion order — which
    /// *is* the jump list. `R-B29`.
    #[serde(default)]
    pub bookmarks: Vec<(String, String, u64)>,

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

fn yes() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            hidden: BTreeSet::new(),
            pinned: BTreeSet::new(),
            labels: BTreeMap::new(),
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
            editor_wrap: BTreeSet::new(),
            bookmarks: Vec::new(),
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

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    }

    pub fn is_hidden(&self, id: &str) -> bool {
        self.hidden.contains(id)
    }

    pub fn is_pinned(&self, id: &str) -> bool {
        self.pinned.contains(id)
    }

    /// Hide a session. Un-pins it too — a pinned-but-hidden session is a
    /// contradiction that would otherwise sit in the file forever.
    pub fn hide(&mut self, id: &str) {
        self.hidden.insert(id.to_string());
        self.pinned.remove(id);
    }

    pub fn unhide(&mut self, id: &str) {
        self.hidden.remove(id);
    }

    pub fn toggle_pin(&mut self, id: &str) {
        if !self.pinned.remove(id) {
            self.pinned.insert(id.to_string());
            // Pinning something hidden means you want to see it.
            self.hidden.remove(id);
        }
    }

    pub fn label(&self, id: &str) -> Option<&str> {
        self.labels.get(id).map(String::as_str)
    }

    /// Set or clear in one door: saving an empty label removes it, so the
    /// editor needs no separate delete affordance and the file never holds a
    /// label that renders as nothing.
    pub fn set_label(&mut self, id: &str, label: &str) {
        let label = label.trim();
        if label.is_empty() {
            self.labels.remove(id);
        } else {
            self.labels.insert(id.to_string(), label.to_string());
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
                    self.labels.remove(&pred_id);
                    self.labels.insert(succ_id.clone(), label);
                    changed = true;
                }
            }
            if self.pinned.contains(&pred_id) && !self.pinned.contains(succ_id) {
                self.pinned.remove(&pred_id);
                self.pinned.insert(succ_id.clone());
                changed = true;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(p.hidden.is_empty());
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
        let p: Prefs = serde_json::from_str(r#"{ "hidden": ["a", "b"] }"#).unwrap();
        assert_eq!(p.hidden.len(), 2);
        assert!(p.preview_on_select, "missing fields must fall back, not default to false");
        assert!(p.markdown);
        assert!(p.hide_noise);
        assert_eq!(p.scope, Scope::NeedsYou);
    }

    #[test]
    fn survives_a_round_trip_through_json() {
        let mut p = Prefs::default();
        p.hide("gone");
        p.toggle_pin("kept");
        p.scope = Scope::All;
        p.side_by_side = true;

        let json = serde_json::to_string(&p).unwrap();
        let back: Prefs = serde_json::from_str(&json).unwrap();
        assert!(back.is_hidden("gone"));
        assert!(back.is_pinned("kept"));
        assert_eq!(back.scope, Scope::All);
        assert!(back.side_by_side);
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
        let mut p = Prefs::default();
        p.set_label("a", "risky one");
        assert_eq!(p.label("a"), Some("risky one"));
        p.set_label("a", "  safe now  ");
        assert_eq!(p.label("a"), Some("safe now"), "replacing trims and overwrites");

        let json = serde_json::to_string(&p).unwrap();
        let back: Prefs = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label("a"), Some("safe now"));

        p.set_label("a", "   ");
        assert_eq!(p.label("a"), None, "an empty label is a removal");
        assert!(p.labels.is_empty(), "removal must not leave an empty entry behind");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<Prefs>("not json").is_err());
        assert!(serde_json::from_str::<Prefs>("{}").is_ok());
    }
}
