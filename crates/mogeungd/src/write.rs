//! The one write into a worktree: a human's save in the Code pane, in a
//! team's worktree, under two refusals. `R-P9`, feature 0051.
//!
//! [ADR-0019](../../../docs/decisions/0019-a-viewer-not-an-editor.md)'s
//! 2026-10-09 amendment narrowed *a viewer, not an editor* to every worktree
//! that is not a team's, and this module is the narrow part. Three things
//! make it safe where the ADR's own objection — the concurrent writer — made
//! it unsafe everywhere else:
//!
//! 1. **Only a worktree the daemon made** (fence 1). The caller resolves the
//!    seat from the session's tag, and the launch record says whether its
//!    directory is a worktree the daemon created; a seat in the team's own
//!    checkout gets no write, and no other session has a record at all. The
//!    path is then canonicalised and must stay under that directory —
//!    `state.rs`'s containment rule, pointed at one root.
//! 2. **The file must be the one you opened** (fence 3). Every `FileContent`
//!    carries the hash of the bytes it served; a save carries it back, and a
//!    file whose hash has moved — an agent wrote under you — is refused with
//!    the current text, so the window can show the difference. There is no
//!    override for this: you reload and edit again.
//! 3. **No seat may be busy in it** (fence 3), unless you say so on that
//!    save. The daemon knows the writers: the seat whose worktree it is, and
//!    whether it is mid-turn from the CLI's own registry.
//!
//! The write itself is a rename over the file, so a seat reading it sees the
//! old bytes or the new and never half of each.

use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The hash a `FileContent` carries and a `FileWrite` returns: sha256 of the
/// bytes, as hex. Of the **whole** file, truncated or not, so a save against
/// a file the viewer cut short still fails the comparison it should.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Why a save was not made. The window says which, and offers what fits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The file on disk is not the one the save was based on.
    Moved,
    /// A seat is mid-turn in this worktree and the save did not say *anyway*.
    Busy { seat: String },
}

impl Refusal {
    pub fn sentence(&self) -> String {
        match self {
            Refusal::Moved => "the file changed on disk since you opened it — an agent wrote under you; \
                               reload it and make your edit again"
                .to_string(),
            Refusal::Busy { seat } => format!(
                "seat `{seat}` is mid-turn in this worktree — a save now lands under a working agent; \
                 wait for it to stop, or save anyway"
            ),
        }
    }
}

/// Judge one save. `on_disk` is the current bytes' hash (or `None` for a file
/// that does not exist yet), `base` the hash the save was made from, `busy`
/// the seat mid-turn in this worktree if there is one.
pub fn judge(on_disk: Option<&str>, base: &str, busy: Option<&str>, force: bool) -> Option<Refusal> {
    match on_disk {
        // A new file: a base of nothing is the only honest base.
        None if !base.is_empty() => return Some(Refusal::Moved),
        Some(h) if h != base => return Some(Refusal::Moved),
        _ => {}
    }
    match busy {
        Some(seat) if !force => Some(Refusal::Busy { seat: seat.to_string() }),
        _ => None,
    }
}

/// `rel` inside `root`, canonicalised, or refused. A file that does not exist
/// yet is resolved by its parent, which must.
pub fn resolve_for_write(root: &Path, rel: &str) -> Result<std::path::PathBuf> {
    if rel.is_empty() || Path::new(rel).is_absolute() {
        bail!("{rel:?} is not a path inside the worktree");
    }
    if Path::new(rel).components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        bail!("{rel} is not a path inside the worktree");
    }
    let root = root.canonicalize().map_err(|e| anyhow!("cannot open the worktree: {e}"))?;
    let joined = root.join(rel);
    let parent = joined
        .parent()
        .ok_or_else(|| anyhow!("{rel} has no parent"))?
        .canonicalize()
        .map_err(|e| anyhow!("cannot open {rel}'s directory: {e}"))?;
    if !parent.starts_with(&root) {
        bail!("{rel} is not a path inside the worktree");
    }
    let full = parent.join(joined.file_name().ok_or_else(|| anyhow!("{rel} names no file"))?);
    // `.git` is never written: a worktree's `.git` is a file pointing into
    // the repository, and a save there is a repository write by another door.
    if full.components().any(|c| c.as_os_str() == ".git") {
        bail!("{rel} is under .git, which is never written");
    }
    Ok(full)
}

/// Write whole, by rename, so a reader sees one version or the other.
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| anyhow!("no directory to write in"))?;
    let tmp = dir.join(format!(
        ".mogeung-write-{}-{}",
        std::process::id(),
        path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
    ));
    std::fs::write(&tmp, content)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_of_the_whole_file_and_stable() {
        assert_eq!(hash_bytes(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hash_bytes(b"a"), hash_bytes(b"a"));
        assert_ne!(hash_bytes(b"a"), hash_bytes(b"b"));
    }

    /// Fence 3, as a table: a moved file is refused whatever you say, a busy
    /// seat is refused until you say *anyway*, and a new file saves against a
    /// base of nothing only.
    #[test]
    fn a_moved_file_is_always_refused_and_a_busy_seat_until_you_say_so() {
        let h = hash_bytes(b"as opened");
        assert_eq!(judge(Some(&h), &h, None, false), None);
        assert_eq!(judge(Some(&h), &hash_bytes(b"older"), None, false), Some(Refusal::Moved));
        assert_eq!(judge(Some(&h), &hash_bytes(b"older"), None, true), Some(Refusal::Moved), "no override for a moved file");
        assert_eq!(judge(Some(&h), &h, Some("builder"), false), Some(Refusal::Busy { seat: "builder".into() }));
        assert_eq!(judge(Some(&h), &h, Some("builder"), true), None, "said so");
        assert_eq!(judge(None, "", None, false), None, "a new file against nothing");
        assert_eq!(judge(None, &h, None, false), Some(Refusal::Moved), "a file that vanished");
        assert!(Refusal::Busy { seat: "b".into() }.sentence().contains("save anyway"));
        assert!(Refusal::Moved.sentence().contains("reload"));
    }

    /// Containment for a path that may not exist yet: resolved by its parent,
    /// and `..`, absolute paths and anything under `.git` refused.
    #[test]
    fn a_write_path_stays_inside_the_worktree() {
        let root = std::env::temp_dir().join(format!("mogeung-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let ok = resolve_for_write(&root, "src/new.rs").unwrap();
        assert!(ok.ends_with("src/new.rs"));
        assert!(resolve_for_write(&root, "../escape").is_err());
        assert!(resolve_for_write(&root, "/etc/passwd").is_err());
        assert!(resolve_for_write(&root, "").is_err());
        assert!(resolve_for_write(&root, "nope/deeper.rs").is_err(), "a parent that does not exist");
        assert!(resolve_for_write(&root, ".git/config").is_err());
        write_atomic(&ok, "fn main() {}\n").unwrap();
        assert_eq!(std::fs::read_to_string(&ok).unwrap(), "fn main() {}\n");
        assert!(!root.join("src").read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".mogeung-write")));
        std::fs::remove_dir_all(&root).ok();
    }
}
