//! The daemons this window knows how to reach. `R-I7`.
//!
//! Before this, choosing a daemon meant a flag and a restart: `--url`, quit,
//! start again. That is tolerable when there is one daemon and it is on this
//! machine, and it stops being tolerable the moment there is a laptop and a dev
//! box and you move between them during a day.
//!
//! Client-side at `~/.mogeung/connections.json`, for the reason the keymap and
//! the prefs are ([ADR-0001](../../../docs/decisions/0001-rust-code-with-egui-ui.md)):
//! which daemons *you* have bookmarked is not daemon state, and a daemon should
//! not be able to tell a client about other daemons.
//!
//! # This file holds credentials
//!
//! A connection may carry the shared token its daemon requires, which is the
//! whole point — retyping a token is how tokens end up in shell history. So the
//! file is written `0600` and rewritten `0600`, and the token is never logged,
//! never put in a window title, and never shown in full in the UI.
//!
//! It is deliberately *not* merged into `prefs.json`. Prefs are boring view
//! state that could reasonably be copied between machines or pasted into a bug
//! report; this cannot.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Where a daemon on this machine listens unless told otherwise.
pub const DEFAULT_ADDR: &str = "127.0.0.1:7717";

/// What the always-present local row is called.
pub const LOCAL_NAME: &str = "LOCAL";

/// The websocket URL of the daemon on this machine.
pub fn local_url() -> String {
    format!("ws://{DEFAULT_ADDR}/ws")
}

/// One daemon, as this window remembers it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    /// What to call it in the list. Free text, because "devbox" beats
    /// `ws://10.0.0.4:7717/ws` when you are choosing under time pressure.
    pub name: String,
    /// The websocket URL, `ws://` or `wss://`.
    pub url: String,
    /// The shared token, when the daemon requires one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl Connection {
    /// The daemon on this machine — the row that is always in the list.
    ///
    /// Synthetic on purpose: it is never written to `connections.json`, never
    /// edited and never forgotten. The whole reason it exists is to be the one
    /// destination that cannot be lost, so a window pointed at an unreachable
    /// dev box is always one click from the machine it is running on.
    pub fn local() -> Self {
        Connection {
            name: LOCAL_NAME.into(),
            url: local_url(),
            token: None,
        }
    }

    /// Whether this names the local daemon, and so must not be stored.
    pub fn is_local(&self) -> bool {
        self.url.trim() == local_url()
    }

    /// The label a row wears: the name, or the URL when it has no name.
    pub fn label(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.url
        } else {
            &self.name
        }
    }

    /// The URL actually dialled, with the token attached as the daemon expects.
    ///
    /// Query parameter rather than a header because a websocket client cannot
    /// set one — the same reason `--token` works this way on the command line.
    pub fn dial_url(&self) -> String {
        match self.token.as_deref().filter(|t| !t.is_empty()) {
            None => self.url.clone(),
            Some(t) => {
                let sep = if self.url.contains('?') { '&' } else { '?' };
                format!("{}{sep}token={t}", self.url)
            }
        }
    }

    /// Whether this is worth attempting at all.
    ///
    /// Deliberately shallow: it rejects what cannot possibly work rather than
    /// predicting what will. A URL that parses but refuses the connection is
    /// the network's answer to give, and the status line already reports it.
    pub fn problem(&self) -> Option<&'static str> {
        let url = self.url.trim();
        if url.is_empty() {
            return Some("needs a URL");
        }
        if !(url.starts_with("ws://") || url.starts_with("wss://")) {
            return Some("must start with ws:// or wss://");
        }
        if url.len() <= "wss://".len() {
            return Some("needs a host after the scheme");
        }
        // Saving this would produce a second, *editable and forgettable* row
        // for the daemon that must always be reachable — and the first row it
        // duplicates would still be there, so the list would show one machine
        // twice with different affordances.
        if self.is_local() {
            return Some("LOCAL is always in the list");
        }
        None
    }
}

/// A URL with any token blanked, for anything that reaches a screen.
///
/// [`Connection::dial_url`] puts the token in the query string, and the URL the
/// window is actually connected to is that one — so every place that renders
/// "which daemon am I on" was rendering the secret with it. Two of them had
/// shipped: the connection dot's tooltip, and the footer of the Daemons window.
/// Both are on screen while somebody is asking for help, which is exactly when
/// a window gets shared.
///
/// The parameter is kept rather than dropped. A URL that silently loses its
/// query would read as the URL you typed, and "this daemon wants a token" is
/// worth seeing; the value is not.
pub fn redacted(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let kept: Vec<String> = query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, _)) if k.eq_ignore_ascii_case("token") => format!("{k}=…"),
            _ => pair.to_string(),
        })
        .collect();
    format!("{base}?{}", kept.join("&"))
}

/// Every remembered daemon, and which one was last chosen.
///
/// `list` holds the *saved* daemons only. [`Connection::local`] is not in it
/// and never will be — the window draws it above these, always.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Connections {
    #[serde(default)]
    pub list: Vec<Connection>,
    /// Index into `list`, recording the last daemon switched to in the window.
    /// `None` means LOCAL, which is also where every launch starts.
    ///
    /// **This no longer decides what the window connects to at start-up.** It
    /// used to, and the result was a window that silently kept dialling a dev
    /// box you were no longer at — with the machine you were actually sitting
    /// in front of nowhere in the list. Start-up is always LOCAL now; this is
    /// kept for the window to show what you last picked.
    #[serde(default)]
    pub active: Option<usize>,
}

impl Connections {
    pub fn path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".mogeung").join("connections.json")
    }

    /// Load, or an empty list. A broken file is reported and then ignored —
    /// losing your bookmarks is annoying, and refusing to open the window over
    /// it would be worse.
    pub fn load() -> (Self, Option<String>) {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return (Self::default(), None);
        };
        match serde_json::from_str::<Self>(&text) {
            Ok(c) => (c.repaired(), None),
            Err(e) => (
                Self::default(),
                Some(format!("{} is unreadable ({e}) — starting empty", path.display())),
            ),
        }
    }

    /// An `active` that points nowhere is a file someone hand-edited, or one
    /// written by a build that indexed differently. Drop the pointer rather
    /// than panicking on it later.
    ///
    /// A stored row naming the local daemon is dropped too. Files written
    /// before LOCAL existed can hold one — it was the obvious thing to add by
    /// hand when the list started empty — and leaving it would show this
    /// machine twice, once forgettable and once not.
    fn repaired(mut self) -> Self {
        let before = self.list.len();
        self.list.retain(|c| !c.is_local());
        // Indices moved under the pointer. It only records the last choice, so
        // dropping it costs a highlight rather than a connection.
        if self.list.len() != before {
            self.active = None;
        }
        if self.active.is_some_and(|i| i >= self.list.len()) {
            self.active = None;
        }
        self
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        restrict(&path);
        Ok(())
    }

    /// Remember a connection, keeping the list a set by URL.
    ///
    /// Matching on URL rather than name: two rows pointing at one daemon are a
    /// mistake however they are labelled, and re-adding a daemon you already
    /// have should update its token rather than grow a duplicate.
    pub fn upsert(&mut self, conn: Connection) -> usize {
        match self.list.iter().position(|c| c.url == conn.url) {
            Some(i) => {
                self.list[i] = conn;
                i
            }
            None => {
                self.list.push(conn);
                self.list.len() - 1
            }
        }
    }

    /// Forget one, keeping `active` pointing at the same connection it did.
    pub fn remove(&mut self, at: usize) {
        if at >= self.list.len() {
            return;
        }
        self.list.remove(at);
        self.active = match self.active {
            // The current one went: nothing is current now. The window keeps
            // whatever socket it already has — forgetting a bookmark is not a
            // reason to disconnect from a daemon you are watching.
            Some(a) if a == at => None,
            Some(a) if a > at => Some(a - 1),
            other => other,
        };
    }
}

/// Owner-only, best effort.
///
/// Unix only, and failure is silent by design: on a filesystem that cannot
/// express the mode there is nothing useful to say and nothing the user could
/// do about it. The file is still written.
#[cfg(unix)]
fn restrict(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(name: &str, url: &str) -> Connection {
        Connection {
            name: name.into(),
            url: url.into(),
            token: None,
        }
    }

    #[test]
    fn a_token_rides_the_url_as_a_query_parameter() {
        let mut c = conn("box", "ws://box:7717/ws");
        assert_eq!(c.dial_url(), "ws://box:7717/ws");
        c.token = Some("s3cret".into());
        assert_eq!(c.dial_url(), "ws://box:7717/ws?token=s3cret");
    }

    /// A URL that already carries a query must gain `&token=`, not a second
    /// `?` — which would make the whole thing unparseable and fail in a way
    /// that looks like a wrong token rather than a malformed URL.
    #[test]
    fn a_url_with_a_query_keeps_only_one_question_mark() {
        let c = Connection {
            name: "x".into(),
            url: "ws://box:7717/ws?debug=1".into(),
            token: Some("t".into()),
        };
        assert_eq!(c.dial_url(), "ws://box:7717/ws?debug=1&token=t");
    }

    /// An empty token is what a cleared text field leaves behind, and it must
    /// not become `?token=` — which a daemon would compare against its real
    /// token and reject, reporting an auth failure for a field you emptied.
    #[test]
    fn an_emptied_token_field_does_not_reach_the_url() {
        let c = Connection {
            name: "x".into(),
            url: "ws://box/ws".into(),
            token: Some(String::new()),
        };
        assert_eq!(c.dial_url(), "ws://box/ws");
    }

    #[test]
    fn only_impossible_urls_are_refused() {
        assert!(conn("a", "ws://box:7717/ws").problem().is_none());
        assert!(conn("a", "wss://box/ws").problem().is_none());
        assert!(conn("a", "").problem().is_some());
        assert!(conn("a", "http://box/ws").problem().is_some());
        assert!(conn("a", "box:7717").problem().is_some());
        assert!(conn("a", "wss://").problem().is_some());
    }

    #[test]
    fn a_row_with_no_name_shows_its_url() {
        assert_eq!(conn("", "ws://box/ws").label(), "ws://box/ws");
        assert_eq!(conn("  ", "ws://box/ws").label(), "ws://box/ws");
        assert_eq!(conn("devbox", "ws://box/ws").label(), "devbox");
    }

    /// Re-adding a daemon updates it rather than growing a second row for the
    /// same machine — usually because a token was rotated.
    #[test]
    fn adding_a_url_that_is_already_known_replaces_it() {
        let mut c = Connections::default();
        c.upsert(conn("box", "ws://box/ws"));
        let at = c.upsert(Connection {
            name: "box".into(),
            url: "ws://box/ws".into(),
            token: Some("new".into()),
        });
        assert_eq!(c.list.len(), 1, "no duplicate for one URL");
        assert_eq!(at, 0);
        assert_eq!(c.list[0].token.as_deref(), Some("new"));
    }

    /// Removing a row above the active one must not leave `active` pointing at
    /// its neighbour — which would silently rename the daemon you are watching.
    #[test]
    fn removing_an_earlier_row_keeps_the_active_one_active() {
        let mut c = Connections::default();
        c.upsert(conn("a", "ws://a/ws"));
        c.upsert(conn("b", "ws://b/ws"));
        c.upsert(conn("c", "ws://c/ws"));
        c.active = Some(2);

        c.remove(0);
        assert_eq!(c.active, Some(1));
        assert_eq!(c.list[c.active.unwrap()].name, "c", "still watching c");
    }

    #[test]
    fn removing_the_active_row_leaves_nothing_active() {
        let mut c = Connections::default();
        c.upsert(conn("a", "ws://a/ws"));
        c.upsert(conn("b", "ws://b/ws"));
        c.active = Some(1);
        c.remove(1);
        assert_eq!(c.active, None);
    }

    #[test]
    fn removing_a_later_row_changes_nothing() {
        let mut c = Connections::default();
        c.upsert(conn("a", "ws://a/ws"));
        c.upsert(conn("b", "ws://b/ws"));
        c.active = Some(0);
        c.remove(1);
        assert_eq!(c.active, Some(0));
    }

    /// A hand-edited file can point `active` past the end. Repair rather than
    /// carry an index that panics the first time something indexes with it.
    #[test]
    fn an_active_index_past_the_end_is_dropped_on_load() {
        let broken = Connections {
            list: vec![conn("a", "ws://a/ws")],
            active: Some(7),
        };
        assert_eq!(broken.repaired().active, None);
    }

    /// The row that must always exist, and must always mean this machine.
    #[test]
    fn local_names_the_default_port_on_this_machine() {
        let local = Connection::local();
        assert_eq!(local.url, "ws://127.0.0.1:7717/ws");
        assert_eq!(local.label(), "LOCAL");
        assert!(local.token.is_none(), "the local daemon needs no token");
        assert!(local.is_local());
        assert!(!conn("devbox", "ws://10.0.0.27:7717/ws").is_local());
    }

    /// Typing the local URL into the add form would produce a second row for
    /// the machine already at the top of the list — and unlike that one, this
    /// one would carry Edit and Forget. Refuse it where every other bad URL is
    /// refused, so the message lands next to the field.
    #[test]
    fn the_local_url_cannot_be_saved_as_a_row() {
        let mut typed = conn("my laptop", &local_url());
        assert_eq!(typed.problem(), Some("LOCAL is always in the list"));
        // Whitespace is what a paste leaves behind, and must not slip past.
        typed.url = format!("  {}  ", local_url());
        assert_eq!(typed.problem(), Some("LOCAL is always in the list"));
        // A different port on this machine is somebody's real second daemon.
        assert!(conn("other", "ws://127.0.0.1:9999/ws").problem().is_none());
    }

    /// Files written before LOCAL existed can hold a hand-added localhost row.
    /// Loading one must not show this machine twice.
    #[test]
    fn a_stored_local_row_is_dropped_on_load() {
        let stored = Connections {
            list: vec![
                conn("localhost", &local_url()),
                conn("devbox", "ws://10.0.0.27:7717/ws"),
            ],
            active: Some(0),
        };
        let fixed = stored.repaired();
        assert_eq!(fixed.list.len(), 1);
        assert_eq!(fixed.list[0].name, "devbox");
        assert_eq!(
            fixed.active, None,
            "the pointer indexed the row that went; it must not slide onto its neighbour"
        );
    }

    /// The dialled URL is what the window shows when it says which daemon it
    /// is on, and it is the one string in the client that carries the secret.
    #[test]
    fn a_token_never_survives_into_something_shown() {
        let dialled = Connection {
            name: "devbox".into(),
            url: "wss://dev.example.com/ws".into(),
            token: Some("s3cret".into()),
        }
        .dial_url();
        let shown = redacted(&dialled);
        assert!(!shown.contains("s3cret"), "{shown}");
        assert_eq!(shown, "wss://dev.example.com/ws?token=…");
    }

    #[test]
    fn redacting_leaves_everything_that_is_not_the_token() {
        assert_eq!(redacted("ws://box/ws"), "ws://box/ws");
        assert_eq!(redacted("ws://box/ws?debug=1"), "ws://box/ws?debug=1");
        // Order is whatever the URL had; only the one value goes.
        assert_eq!(
            redacted("ws://box/ws?debug=1&token=abc&trace=2"),
            "ws://box/ws?debug=1&token=…&trace=2"
        );
        // A hand-written URL need not use the casing dial_url does.
        assert_eq!(redacted("ws://box/ws?Token=abc"), "ws://box/ws?Token=…");
    }

    #[test]
    fn connections_round_trip_through_the_stored_form() {
        let mut c = Connections::default();
        c.upsert(Connection {
            name: "devbox".into(),
            url: "wss://dev.example.com/ws".into(),
            token: Some("s3cret".into()),
        });
        c.active = Some(0);
        let json = serde_json::to_string(&c).unwrap();
        let back: Connections = serde_json::from_str(&json).unwrap();
        assert_eq!(back.list, c.list);
        assert_eq!(back.active, Some(0));

        // A file written before tokens were optional, and one written by a
        // build that never had an `active`.
        let older: Connections =
            serde_json::from_str(r#"{"list":[{"name":"a","url":"ws://a/ws"}]}"#).unwrap();
        assert_eq!(older.list.len(), 1);
        assert!(older.list[0].token.is_none());
        assert_eq!(older.active, None);
    }
}
