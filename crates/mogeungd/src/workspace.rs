//! Folders you added to a session's workspace, by hand. `R-J40`.
//!
//! A session can read and write outside the directory it was started in — a
//! sibling repository in the same workspace is the ordinary case, and 64 of
//! 270 transcripts on the author's machine did exactly that. mogeung's file
//! surface could not follow: [`crate::state::AppState::session_root`] answers
//! one path and `resolve_inside` refuses everything outside it, which is the
//! rule that stops an unauthenticated localhost port becoming *read any file
//! by asking politely*.
//!
//! **This is the manual half, and it is manual on purpose.** `/add-dir`, the
//! launch flag and the paths an agent has already touched are all discoverable
//! (`R-J39` measures each), and none of them is *prospective*: they can only
//! tell you where an agent has already been. A folder you add yourself is a
//! folder you can add before the work starts, and — the part that matters for
//! a read boundary — it is a folder **you** authorised rather than one the
//! daemon inferred.
//!
//! # Keyed by repository, not by session
//!
//! Sessions are disposable: `/clear` ends one, a crash ends one, and tomorrow
//! is a new one. The folder is not. So the key is the session's **repository
//! root** (its `cwd` when it is not a repository), which means a workspace
//! outlives every session in it and is shared by all of them at once —
//! including two agents working in the same checkout.
//!
//! # The file
//!
//! `~/.mogeung/workspaces.json`, one object, `{ "<repo root>": ["<path>", …] }`
//! — small enough to read, edit and delete by hand, which is deliberate: it is
//! in your home directory, and a file there that you cannot understand is a
//! file you cannot undo. A malformed one degrades to *no extra folders* rather
//! than taking the daemon down, the same rule every parser here follows.

use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn store_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("workspaces.json")
}

/// Every workspace on this machine: repository root → the folders added to it.
///
/// `BTreeMap` so the file is written in a stable order — a store that reorders
/// itself on every save is one you cannot read a diff of.
pub type Workspaces = BTreeMap<String, Vec<String>>;

/// Read the store, or an empty one.
///
/// **Every failure is the same answer.** Missing, unreadable, malformed, or
/// holding something that is not an object of arrays: the workspace is simply
/// empty, because the alternative — refusing to serve files because a
/// convenience file has a stray comma — is a worse day than losing the extra
/// folders until you fix it.
pub fn load(path: &Path) -> Workspaces {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return Workspaces::new(),
    };
    serde_json::from_str::<Workspaces>(&raw).unwrap_or_default()
}

/// Write the store, creating `~/.mogeung` if this is the first one.
pub fn save(path: &Path, all: &Workspaces) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = serde_json::to_string_pretty(all)?;
    // Written whole through a neighbour and renamed, so a crash mid-write
    // leaves the old workspace rather than half of the new one. A rename
    // within a directory is atomic on every filesystem this runs on.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// What a folder must be before it can join a workspace.
///
/// Four refusals, each of which would otherwise become a confusing tree:
///
/// - a **relative** path, because the store is read by a daemon whose working
///   directory is nobody's business but its own;
/// - a path that **does not exist**, or is not a directory;
/// - the workspace's **own root**, or anything already inside it — the tree
///   already shows those, and a second copy of a subtree is a bug report
///   waiting to happen;
/// - a path that is already **in the list**, which is a no-op wearing the
///   clothes of an action.
///
/// Canonicalised on the way in, for `R-J27`'s reason: `/home/me/proj` reached
/// through a symlink and the same directory reached directly are one folder,
/// and a store holding both would list every file twice.
pub fn admit(root: &str, existing: &[String], candidate: &str) -> Result<String> {
    let p = Path::new(candidate);
    if !p.is_absolute() {
        return Err(anyhow!("{candidate} is not an absolute path"));
    }
    let full = p
        .canonicalize()
        .map_err(|e| anyhow!("cannot add {candidate}: {e}"))?;
    if !full.is_dir() {
        return Err(anyhow!("{candidate} is not a directory"));
    }
    let full_s = full.to_string_lossy().to_string();
    let root_c = Path::new(root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(root));
    if full == root_c || full.starts_with(&root_c) {
        return Err(anyhow!(
            "{full_s} is already inside this session's own folder"
        ));
    }
    if existing.iter().any(|e| e == &full_s) {
        return Err(anyhow!("{full_s} is already in this workspace"));
    }
    Ok(full_s)
}

/// The folders of one workspace, dropping any that have since gone away.
///
/// **Missing is reported, not silently dropped from the file.** A folder you
/// added and then moved should say so on screen (`R-J5`) — but it must not be
/// served, because a root that does not resolve turns every path check under
/// it into an error with no useful message. So the store keeps it and this
/// answers only what is real; [`missing`] is what the client is told about.
pub fn live(all: &Workspaces, root: &str) -> Vec<String> {
    all.get(root)
        .map(|v| v.iter().filter(|p| Path::new(p).is_dir()).cloned().collect())
        .unwrap_or_default()
}

/// The folders of one workspace that are no longer directories.
pub fn missing(all: &Workspaces, root: &str) -> Vec<String> {
    all.get(root)
        .map(|v| {
            v.iter()
                .filter(|p| !Path::new(p).is_dir())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mogeung-ws-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn a_missing_store_is_an_empty_workspace_not_an_error() {
        assert!(load(&tmp().join("nope.json")).is_empty());
    }

    /// The rule every parser here follows: a file we cannot read costs the
    /// feature, never the daemon.
    #[test]
    fn a_malformed_store_degrades_rather_than_failing() {
        let p = tmp().join("bad.json");
        std::fs::write(&p, "{ this is not json").unwrap();
        assert!(load(&p).is_empty());
        std::fs::write(&p, r#"{"/repo": "not-a-list"}"#).unwrap();
        assert!(load(&p).is_empty());
    }

    #[test]
    fn a_workspace_survives_the_round_trip() {
        let p = tmp().join("round.json");
        let mut all = Workspaces::new();
        all.insert("/repo/a".into(), vec!["/repo/b".into()]);
        save(&p, &all).unwrap();
        assert_eq!(load(&p), all);
        // Written whole and renamed, so no half-file is ever left behind.
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    fn a_folder_must_be_absolute_and_real() {
        assert!(admit("/repo", &[], "relative/path").is_err());
        assert!(admit("/repo", &[], "/definitely/not/here/at/all").is_err());
    }

    #[test]
    fn a_folder_inside_the_session_is_refused_with_a_reason() {
        let dir = tmp().join("inside");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let root = dir.to_string_lossy().to_string();
        let err = admit(&root, &[], &dir.join("sub").to_string_lossy()).unwrap_err();
        assert!(err.to_string().contains("already inside"), "{err}");
        let err = admit(&root, &[], &root).unwrap_err();
        assert!(err.to_string().contains("already inside"), "{err}");
    }

    #[test]
    fn the_same_folder_twice_is_refused_rather_than_stored_twice() {
        let dir = tmp().join("dup");
        std::fs::create_dir_all(&dir).unwrap();
        let full = dir.canonicalize().unwrap().to_string_lossy().to_string();
        let err = admit("/elsewhere", std::slice::from_ref(&full), &full).unwrap_err();
        assert!(err.to_string().contains("already in this workspace"), "{err}");
    }

    /// `R-J27`'s lesson, applied before the path is stored rather than after:
    /// one directory reached two ways is one folder.
    #[test]
    fn a_symlinked_folder_is_stored_as_what_it_resolves_to() {
        let base = tmp().join("link");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("real")).unwrap();
        let link = base.join("alias");
        let _ = std::os::unix::fs::symlink(base.join("real"), &link);
        let stored = admit("/elsewhere", &[], &link.to_string_lossy()).unwrap();
        assert!(stored.ends_with("/real"), "{stored}");
    }

    #[test]
    fn a_folder_that_has_gone_away_is_reported_rather_than_served() {
        let mut all = Workspaces::new();
        let real = tmp().to_string_lossy().to_string();
        all.insert("/repo".into(), vec![real.clone(), "/gone/for/ever".into()]);
        assert_eq!(live(&all, "/repo"), vec![real]);
        assert_eq!(missing(&all, "/repo"), vec!["/gone/for/ever".to_string()]);
    }
}
