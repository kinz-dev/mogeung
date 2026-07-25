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
use std::collections::BTreeSet;
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
    #[serde(default)]
    pub scope: Scope,
    /// Temporarily reveal hidden sessions, so unhiding is possible.
    /// Not persisted — it is a peek, not a setting.
    #[serde(skip)]
    pub reveal_hidden: bool,

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
}

fn yes() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            hidden: BTreeSet::new(),
            pinned: BTreeSet::new(),
            scope: Scope::default(),
            reveal_hidden: false,
            group_by_repo: false,
            auto_select: false,
            preview_on_select: true,
            hide_reviewed: false,
            hide_noise: true,
            syntax: true,
            word_diff: true,
            side_by_side: false,
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

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<Prefs>("not json").is_err());
        assert!(serde_json::from_str::<Prefs>("{}").is_ok());
    }
}
