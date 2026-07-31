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
        None
    }
}

/// Every remembered daemon, and which one is current.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Connections {
    #[serde(default)]
    pub list: Vec<Connection>,
    /// Index into `list`. Out of range means "none of them" — which happens
    /// when the window was started with `--url`, and after a removal.
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
    fn repaired(mut self) -> Self {
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

    pub fn active(&self) -> Option<&Connection> {
        self.list.get(self.active?)
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
        assert!(c.active().is_none());
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
