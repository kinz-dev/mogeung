//! Where the connection list rests. `R-I16`,
//! [ADR-0036](../../../docs/decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md).
//!
//! The list is the **client's** — no wire family, no daemon code, nothing
//! served. Which daemons you watch is client-side taste, exactly like the
//! keymap and the layout, and a remote daemon has no business holding the
//! addresses of its peers.
//!
//! What moved here from `localStorage` is only the *storage*, and the reason is
//! the token. The daemon accepts a shared token as `Authorization: Bearer …`
//! or `?token=…`, and until `R-I16` the window had no field for one — so the
//! only way to reach a token-gated daemon was to type the token into the
//! address, which put a shared secret into the webview's unencrypted
//! key-value store. This file is `0600`, which is what `R-I7`'s row said in
//! the first place.
//!
//! It is deliberately **this** machine's file and not the daemon's: the list
//! is about where this window can go, so reading it from a dev box you are
//! connected to would answer the wrong question.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One daemon you can switch to.
///
/// `id` is the identity, not `url`: two entries may name the same daemon by two
/// routes — a tunnel and a hostname — and the URL has to stay editable, which
/// it cannot be while it is also the key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub url: String,
    /// The shared token for a non-loopback bind (`R-I10`). Its own field, so
    /// it never has to be smuggled through the address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The tunnel command you use, **recorded and never run**. A panel that
    /// ran `ssh` would be a window with a shell verb, which is the line
    /// ADR-0008 drew.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// `~/.mogeung/connections.json`, beside the notes mirror and the scratch
/// directory. Same `HOME` idiom the daemon's own paths use.
pub fn default_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("connections.json")
}

/// Read the list, degrading to empty rather than failing.
///
/// A hand-editable file with one bad row must not cost the whole list, and a
/// file that is not there yet is the ordinary first-run case rather than an
/// error. The same posture the transcript parsers take, for the same reason.
pub fn load_from(path: &Path) -> Vec<Connection> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<Connection>>(&raw).unwrap_or_default()
}

/// Write the list `0600`, through a temporary file in the same directory.
///
/// Two things this does that a plain `write` does not. **Atomic**: a crash
/// half-way through leaves the previous list intact rather than a truncated
/// one, and a rename within a directory is the only cheap way to get that.
/// **`0600` on every write, not only on create**: the mode is a property of
/// the file, so a list restored from a backup arrives `0644` and would stay
/// that way for ever if we only set it the first time.
pub fn save_to(path: &Path, list: &[Connection]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let body = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    restrict(&tmp)?;
    std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))?;
    // Again after the rename: on a filesystem that does not carry the mode
    // through, the temporary file's `0600` is not necessarily what landed.
    restrict(path)
}

/// Owner read/write and nothing else.
#[cfg(unix)]
fn restrict(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("could not secure {}: {e}", path.display()))
}

/// Windows is not a target (`R-I3` descoped it), and this keeps the module
/// compiling rather than pretending the guarantee holds there.
#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn connections_load() -> Result<Vec<Connection>, String> {
    Ok(load_from(&default_path()))
}

#[tauri::command]
pub async fn connections_save(list: Vec<Connection>) -> Result<(), String> {
    save_to(&default_path(), &list)
}

/// Is a saved daemon alive, and will it let us in? `R-I20`.
///
/// The top bar's switcher shows this beside every saved connection. It is
/// asked **here**, in the shell that owns the file, rather than in the
/// webview, for two reasons: the tokens never have to leave the `0600` file
/// to answer it (ADR-0036), and a plain `GET /api/health` is the cheapest
/// honest question — where a WebSocket, the only thing the webview could open
/// across origins, would be answered with the whole snapshot.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Liveness {
    pub id: String,
    /// `up` — answered, and the token worked. `refused` — answered, and turned
    /// the token away. `down` — nothing answered. `unknown` — not a thing this
    /// can ask (a `wss://` address behind a TLS proxy).
    pub state: &'static str,
    pub host: Option<String>,
    pub version: Option<String>,
    /// Why, in words, when it is not `up`.
    pub detail: Option<String>,
}

/// Host, port and whether it is TLS, out of a `ws://` or `wss://` address.
pub fn target_of(url: &str) -> Option<(String, u16, bool)> {
    let (rest, tls) = if let Some(r) = url.strip_prefix("ws://") {
        (r, false)
    } else if let Some(r) = url.strip_prefix("wss://") {
        (r, true)
    } else {
        return None;
    };
    let authority = rest.split(['/', '?']).next()?;
    if authority.is_empty() {
        return None;
    }
    let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        let port = after.strip_prefix(':').and_then(|p| p.parse().ok());
        (h.to_string(), port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()),
            None => (authority.to_string(), None),
        }
    };
    Some((host, port.unwrap_or(if tls { 443 } else { 80 }), tls))
}

fn is_loopback_host(host: &str) -> bool {
    host == "127.0.0.1" || host == "localhost" || host == "::1"
}

/// One health check, with a deadline on every step. Degrades to `down` or
/// `unknown` with a reason; never panics on what a stranger sends back.
pub fn liveness(id: &str, url: &str, token: Option<&str>, timeout: std::time::Duration) -> Liveness {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    let answer = |state: &'static str, detail: Option<String>| Liveness {
        id: id.to_string(),
        state,
        host: None,
        version: None,
        detail,
    };
    let Some((host, port, tls)) = target_of(url) else {
        return answer("unknown", Some("not a ws:// address".into()));
    };
    if tls {
        return answer("unknown", Some("behind TLS — the switcher only asks plain ws:// daemons".into()));
    }
    let addr = match (host.as_str(), port).to_socket_addrs().ok().and_then(|mut a| a.next()) {
        Some(a) => a,
        None => return answer("down", Some(format!("{host} does not resolve"))),
    };
    let mut sock = match TcpStream::connect_timeout(&addr, timeout) {
        Ok(s) => s,
        Err(e) => return answer("down", Some(format!("nothing answered on {host}:{port} — {e}"))),
    };
    let _ = sock.set_read_timeout(Some(timeout));
    let _ = sock.set_write_timeout(Some(timeout));
    let auth = token
        .filter(|t| !t.is_empty())
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!("GET /api/health HTTP/1.0\r\nHost: {host}\r\n{auth}\r\n");
    if sock.write_all(req.as_bytes()).is_err() {
        return answer("down", Some("the connection closed before it could be asked".into()));
    }
    let mut body = String::new();
    let _ = sock.take(64 * 1024).read_to_string(&mut body);
    let status = body
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse::<u16>().ok());
    match status {
        Some(200) => {
            let json = body.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
            let v: serde_json::Value = serde_json::from_str(json.trim()).unwrap_or_default();
            let daemon = v.get("daemon");
            Liveness {
                id: id.to_string(),
                state: "up",
                host: daemon.and_then(|d| d.get("host")).and_then(|h| h.as_str()).map(str::to_string),
                version: v.get("version").and_then(|h| h.as_str()).map(str::to_string),
                detail: None,
            }
        }
        Some(401) => answer(
            "refused",
            Some(if token.is_some_and(|t| !t.is_empty()) {
                "answering, and the token was refused".into()
            } else {
                "answering, and it wants a token".into()
            }),
        ),
        Some(code) => answer("unknown", Some(format!("answered {code}, which is not a mogeung daemon's answer"))),
        None => answer("unknown", Some("answered with something that is not HTTP".into())),
    }
}

/// Every saved connection, checked at once. This machine's own daemon keeps
/// no token in the list — it lives in `config.toml` — so a loopback entry with
/// none is asked with that one.
fn status_all(list: Vec<Connection>) -> Vec<Liveness> {
    let local_token = mogeung_core::config::Config::load().0.token;
    let timeout = std::time::Duration::from_millis(1500);
    let handles: Vec<_> = list
        .into_iter()
        .map(|c| {
            let token = c.token.clone().or_else(|| {
                target_of(&c.url)
                    .filter(|(h, _, _)| is_loopback_host(h))
                    .and(local_token.clone())
            });
            std::thread::spawn(move || liveness(&c.id, &c.url, token.as_deref(), timeout))
        })
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

#[tauri::command]
pub async fn connections_status() -> Vec<Liveness> {
    let list = load_from(&default_path());
    tauri::async_runtime::spawn_blocking(move || status_all(list))
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The address a switcher has to take apart — IPv6 brackets, a query with
    /// a token in it, no port at all.
    #[test]
    fn an_address_comes_apart_into_host_port_and_tls() {
        assert_eq!(target_of("ws://10.0.0.39:7717/ws"), Some(("10.0.0.39".into(), 7717, false)));
        assert_eq!(target_of("ws://devbox:7717/ws?token=abc"), Some(("devbox".into(), 7717, false)));
        assert_eq!(target_of("ws://[fd00::1]:7800/ws"), Some(("fd00::1".into(), 7800, false)));
        assert_eq!(target_of("wss://mogeung.example/ws"), Some(("mogeung.example".into(), 443, true)));
        assert_eq!(target_of("ws://devbox/ws"), Some(("devbox".into(), 80, false)));
        assert_eq!(target_of("http://devbox:7717"), None);
        assert_eq!(target_of("ws:///ws"), None);
    }

    /// A tiny server that answers every connection with `reply`, and hands
    /// back the request it saw so a test can check the header went out.
    fn serve_once(reply: &'static str) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
                let _ = s.write_all(reply.as_bytes());
            }
        });
        (format!("ws://127.0.0.1:{port}/ws"), rx)
    }

    #[test]
    fn a_daemon_that_answers_is_up_and_says_who_it_is() {
        let (url, seen) = serve_once(
            "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true,\"version\":\"0.1.0\",\"daemon\":{\"host\":\"devbox\"}}",
        );
        let l = liveness("a", &url, Some("s3cret"), std::time::Duration::from_secs(2));
        assert_eq!((l.state, l.host.as_deref(), l.version.as_deref()), ("up", Some("devbox"), Some("0.1.0")));
        assert!(seen.recv().unwrap().contains("Authorization: Bearer s3cret"), "the token went as a header");
    }

    #[test]
    fn a_daemon_that_turns_the_token_away_is_refused_not_down() {
        let (url, _) = serve_once("HTTP/1.0 401 Unauthorized\r\n\r\nmissing or wrong token");
        let l = liveness("a", &url, Some("wrong"), std::time::Duration::from_secs(2));
        assert_eq!(l.state, "refused");
        assert!(l.detail.unwrap().contains("refused"));
        let (url, _) = serve_once("HTTP/1.0 401 Unauthorized\r\n\r\n");
        assert!(liveness("a", &url, None, std::time::Duration::from_secs(2)).detail.unwrap().contains("wants a token"));
    }

    #[test]
    fn nothing_listening_is_down_and_says_where() {
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let l = liveness("a", &format!("ws://127.0.0.1:{port}/ws"), None, std::time::Duration::from_millis(500));
        assert_eq!(l.state, "down");
        assert!(l.detail.unwrap().contains(&port.to_string()));
    }

    #[test]
    fn something_that_is_not_a_daemon_is_unknown_and_tls_is_not_asked() {
        let (url, _) = serve_once("SSH-2.0-OpenSSH_9.6\r\n");
        assert_eq!(liveness("a", &url, None, std::time::Duration::from_secs(2)).state, "unknown");
        assert_eq!(liveness("a", "wss://x.example/ws", None, std::time::Duration::from_millis(100)).state, "unknown");
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mog-conn-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("connections.json")
    }

    fn one(id: &str) -> Connection {
        Connection {
            id: id.into(),
            name: "dev box".into(),
            url: "ws://devbox:7717/ws".into(),
            token: Some("6f1c".into()),
            note: None,
        }
    }

    #[test]
    fn a_list_survives_a_round_trip() {
        let path = scratch("round");
        save_to(&path, &[one("a"), one("b")]).unwrap();
        assert_eq!(load_from(&path), vec![one("a"), one("b")]);
        std::fs::remove_file(&path).ok();
    }

    /// The whole reason the file moved out of `localStorage`.
    #[cfg(unix)]
    #[test]
    fn the_file_holding_a_token_is_not_readable_by_anyone_else() {
        use std::os::unix::fs::PermissionsExt;
        let path = scratch("mode");
        save_to(&path, &[one("a")]).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "a file holding a shared token must be 0600");
        std::fs::remove_file(&path).ok();
    }

    /// A list restored from a backup arrives `0644`. Setting the mode only on
    /// create would leave it that way for ever.
    #[cfg(unix)]
    #[test]
    fn rewriting_a_loose_file_tightens_it_again() {
        use std::os::unix::fs::PermissionsExt;
        let path = scratch("retighten");
        std::fs::write(&path, "[]").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        save_to(&path, &[one("a")]).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_file(&path).ok();
    }

    /// Hand-editable, so it will be hand-broken. An empty list is recoverable;
    /// an error at startup is not.
    #[test]
    fn a_broken_file_reads_as_an_empty_list() {
        let path = scratch("broken");
        std::fs::write(&path, "{not json at all").unwrap();
        assert_eq!(load_from(&path), Vec::new());
        std::fs::remove_file(&path).ok();
    }

    /// First run. Not an error.
    #[test]
    fn a_missing_file_reads_as_an_empty_list() {
        let path = scratch("missing").with_file_name("definitely-absent.json");
        assert_eq!(load_from(&path), Vec::new());
    }

    /// An older file predates the token and note fields; it must still load.
    #[test]
    fn a_file_without_the_new_fields_still_loads() {
        let path = scratch("older");
        std::fs::write(
            &path,
            r#"[{"id":"a","name":"dev box","url":"ws://devbox:7717/ws"}]"#,
        )
        .unwrap();
        let list = load_from(&path);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].token, None);
        assert_eq!(list[0].note, None);
        std::fs::remove_file(&path).ok();
    }

    /// A token is a secret; it must not be in the file under a key that a
    /// glance would miss, nor absent when it was set.
    #[test]
    fn an_entry_without_a_token_does_not_write_a_null() {
        let path = scratch("nulls");
        let bare = Connection {
            id: "a".into(),
            name: "local".into(),
            url: "ws://localhost:7717/ws".into(),
            token: None,
            note: None,
        };
        save_to(&path, &[bare]).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("token"), "an absent token writes no key: {raw}");
        std::fs::remove_file(&path).ok();
    }
}
