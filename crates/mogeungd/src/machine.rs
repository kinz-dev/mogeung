//! Which machine this process is on, and how to say so. `R-I5`.
//!
//! This lives in the daemon crate rather than in `mogeung-core` because
//! answering the question needs I/O — a file read, sometimes a subprocess — and
//! core is deliberately free of both. The window depends on this crate already,
//! so both ends still resolve identity through *one* implementation.
//!
//! That sharing is the point, not an accident. If the daemon and the window
//! computed a machine's identity by different routes, two processes on one
//! desk could disagree about whether they are on one desk — and the window
//! would refuse local actions on the machine it is running on.

use mogeung_core::wire::DaemonIdentity;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `~/.mogeung/machine-id` — written once, read forever.
fn id_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("machine-id")
}

/// A stable id for this machine-and-user, generated on first call.
///
/// Per *user* as well as per machine, because it lives under `$HOME`. Two users
/// on one box therefore read as two machines — which over-refuses (their
/// `~/.claude` directories really are different worlds) and never under-refuses.
///
/// `None` if it cannot be read or written. Callers must treat that as "not the
/// same machine" rather than as "probably fine".
///
/// **This is an identifier, not a secret.** It is published in the daemon's
/// identity to anyone allowed to connect, and knowing it grants nothing: it
/// decides whether a *client* enables actions that run on the client's own
/// machine. A daemon that lied about it could only persuade a window to open
/// an editor locally — which that window could already do — and a daemon you
/// have connected to can do considerably worse than that by design.
pub fn machine_id() -> Option<String> {
    let path = id_path();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let fresh = random_id()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    // Written first, then returned: an id we hand out but fail to persist would
    // change on the next run, and a client that cached it would quietly start
    // treating its own machine as somewhere else.
    std::fs::write(&path, format!("{fresh}\n")).ok()?;
    Some(fresh)
}

/// 16 bytes of `/dev/urandom` as hex.
fn random_id() -> Option<String> {
    let mut buf = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .ok()?
        .read_exact(&mut buf)
        .ok()?;
    Some(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// This machine's name, for display only.
///
/// Three sources in falling order of trust. `$HOSTNAME` is last despite being
/// cheapest: it is not exported by default in a non-interactive shell, so when
/// it *is* set it was often set by something else, and a stale value here would
/// mislabel a daemon in the UI.
pub fn hostname() -> Option<String> {
    if let Some(h) = read_trimmed(Path::new("/etc/hostname")) {
        return Some(h);
    }
    if let Ok(out) = std::process::Command::new("hostname").output() {
        if out.status.success() {
            let h = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !h.is_empty() {
                return Some(h);
            }
        }
    }
    std::env::var("HOSTNAME").ok().filter(|h| !h.is_empty())
}

fn read_trimmed(path: &Path) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Everything this daemon publishes about itself.
pub fn identity(claude_home: &Path, ssh_target: Option<String>) -> DaemonIdentity {
    DaemonIdentity {
        machine_id: machine_id(),
        host: hostname(),
        claude_home: claude_home.to_string_lossy().into_owned(),
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        ssh_target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Called twice in one process, or across two, the id must not move — a
    /// changing id means a machine that stops recognising itself.
    #[test]
    fn the_id_is_stable_across_calls() {
        let first = machine_id();
        let second = machine_id();
        assert!(first.is_some(), "no id could be established");
        assert_eq!(first, second);
    }

    #[test]
    fn a_fresh_id_is_hex_and_long_enough_not_to_collide() {
        let id = random_id().expect("urandom");
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "got {id}");
        assert_ne!(id, random_id().unwrap(), "two draws must differ");
    }

    /// The identity carries what the client needs to decide and to display.
    #[test]
    fn identity_reports_the_home_it_watches() {
        let id = identity(Path::new("/tmp/some-home"), None);
        assert_eq!(id.claude_home, "/tmp/some-home");
        assert_eq!(id.pid, std::process::id());
        assert!(!id.version.is_empty());
        assert!(id.ssh_target.is_none());
    }

    /// The comparison that gates every local-only action.
    #[test]
    fn identity_matches_only_a_known_equal_id() {
        let mut id = identity(Path::new("/tmp/h"), None);
        id.machine_id = Some("abc".into());

        assert!(id.is_same_machine_as(Some("abc")));
        assert!(!id.is_same_machine_as(Some("def")));
        // Either side unknown resolves to "not this machine" — refusing is a
        // message, acting on the wrong filesystem is a bug.
        assert!(!id.is_same_machine_as(None));
        id.machine_id = None;
        assert!(!id.is_same_machine_as(Some("abc")));
    }
}
