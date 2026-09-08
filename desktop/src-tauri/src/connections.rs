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

#[cfg(test)]
mod tests {
    use super::*;

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
