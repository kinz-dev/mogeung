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
