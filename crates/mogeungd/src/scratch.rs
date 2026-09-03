//! Scratch files. `R-L5`, [ADR-0035](../../../docs/decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md).
//!
//! A scratch file is a *file* — `scratch-3.java`, `scratch-1.sql` — that the
//! Code pane opens writable and saves as you type. It is not a note: a note
//! is markdown the daemon owns in its store and mirrors one way
//! ([`crate::notes`]); a scratch file has no row anywhere and the file **is**
//! the thing, which is what makes it useful to anything else on the machine.
//!
//! # Where they live, and why only there
//!
//! `~/.mogeung/scratch`. The editor has been read-only since it shipped
//! ([ADR-0019](../../../docs/decisions/0019-a-viewer-not-an-editor.md)) and
//! pillar K says it never writes a worktree file. This module does not move
//! that line — it is the one directory the daemon already owns, and every
//! name that arrives over the wire is checked to be a bare file name inside
//! it. A path, a dotfile or a `..` is refused before anything touches the
//! filesystem, so the write verb the window gained cannot be aimed anywhere
//! but here.
//!
//! # Naming
//!
//! The daemon picks the name, never the window: `scratch-<n>.<ext>` with the
//! first free `n`. Two windows creating at once cannot collide, because the
//! file is created with `create_new` and the loser retries with the next
//! number.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// The default home, beside the notes mirror.
pub fn default_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("scratch")
}

/// Past this the "scratch file" is something else, and the window's editor
/// would be the wrong tool for it anyway.
const CAP: u64 = 4 * 1024 * 1024;

/// Refuse anything that is not a bare file name inside the directory.
///
/// Checked on every verb rather than only on create, because a name that
/// arrives in a `ScratchWrite` was typed by whatever is on the other end of
/// the socket, not by this daemon.
pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 128 {
        bail!("a scratch file name must be 1–128 characters");
    }
    if name.starts_with('.') {
        bail!("a scratch file name cannot start with a dot");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        bail!("a scratch file name may only contain letters, digits, `-`, `_` and `.`");
    }
    Ok(())
}

/// The extension a new file gets: short, alphanumeric, no dot.
pub fn check_ext(ext: &str) -> Result<()> {
    if ext.is_empty() || ext.len() > 12 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("a scratch file extension must be 1–12 letters or digits");
    }
    Ok(())
}

/// Every scratch file, newest first — the one you just made is the one you
/// most likely want back.
pub fn list(dir: &Path) -> Result<Vec<String>> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut entries: Vec<(std::time::SystemTime, String)> = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // A directory or a dotfile someone put here by hand is not ours to
        // list, and a name we would refuse on the wire is not one to offer.
        if check_name(&name).is_err() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        entries.push((modified, name));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(entries.into_iter().map(|(_, n)| n).collect())
}

/// Make a new empty file and answer with its name.
pub fn create(dir: &Path, ext: &str) -> Result<String> {
    check_ext(ext)?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for n in 1..10_000u32 {
        let name = format!("scratch-{n}.{ext}");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&name))
        {
            Ok(_) => return Ok(name),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("creating {name}")),
        }
    }
    bail!("ten thousand scratch files with that extension — time to tidy up");
}

/// The whole file.
pub fn read(dir: &Path, name: &str) -> Result<String> {
    check_name(name)?;
    let path = dir.join(name);
    let meta = std::fs::metadata(&path).with_context(|| format!("{name} is not here"))?;
    if !meta.is_file() {
        bail!("{name} is not a file");
    }
    if meta.len() > CAP {
        bail!("{name} is larger than a scratch file should be");
    }
    let bytes = std::fs::read(&path).with_context(|| format!("reading {name}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Replace the whole file, atomically: a crash mid-write leaves the old
/// content rather than half the new.
pub fn write(dir: &Path, name: &str, content: &str) -> Result<()> {
    check_name(name)?;
    if content.len() as u64 > CAP {
        bail!("{name} would be larger than a scratch file should be");
    }
    let path = dir.join(name);
    // Only a file that exists: a write is an edit, and create is the one
    // verb that mints a name. Without this a window could create files with
    // names the daemon never chose.
    if !path.is_file() {
        bail!("{name} is not a scratch file here — create one first");
    }
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, content).with_context(|| format!("writing {name}"))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("replacing {name}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mogeung-scratch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_name_is_a_bare_file_name_and_nothing_else() {
        assert!(check_name("scratch-1.java").is_ok());
        assert!(check_name("notes_2.md").is_ok());
        for bad in ["", "../x", "a/b", ".hidden", "..", "a\\b", "with space.txt", "x\0y"] {
            assert!(check_name(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(check_name(&"a".repeat(129)).is_err());
    }

    #[test]
    fn create_picks_the_first_free_number_and_never_reuses_one() {
        let d = fresh("create");
        assert_eq!(create(&d, "java").unwrap(), "scratch-1.java");
        assert_eq!(create(&d, "java").unwrap(), "scratch-2.java");
        // A different extension counts from one again: the name says the
        // language, and `scratch-1.sql` beside `scratch-1.java` is fine.
        assert_eq!(create(&d, "sql").unwrap(), "scratch-1.sql");
        // A gap is filled rather than skipped, so the numbers stay small.
        std::fs::remove_file(d.join("scratch-1.java")).unwrap();
        assert_eq!(create(&d, "java").unwrap(), "scratch-1.java");
        assert!(create(&d, "").is_err());
        assert!(create(&d, "a/b").is_err());
    }

    #[test]
    fn write_then_read_round_trips_and_a_write_needs_a_created_file() {
        let d = fresh("rw");
        let name = create(&d, "py").unwrap();
        assert_eq!(read(&d, &name).unwrap(), "");
        write(&d, &name, "print('hi')\n").unwrap();
        assert_eq!(read(&d, &name).unwrap(), "print('hi')\n");
        // The window may not mint names: an unknown name is an error, not a
        // new file — and after it nothing is on disk.
        assert!(write(&d, "made-up.txt", "x").is_err());
        assert!(!d.join("made-up.txt").exists());
        // Nor may it reach outside, whatever the name spells.
        assert!(write(&d, "../escape.txt", "x").is_err());
        assert!(read(&d, "../../etc/passwd").is_err());
        // The temp file from an atomic write does not linger.
        assert!(list(&d).unwrap().iter().all(|n| !n.ends_with(".tmp")));
    }

    #[test]
    fn list_is_newest_first_and_skips_what_is_not_ours() {
        let d = fresh("list");
        assert_eq!(list(&d).unwrap(), Vec::<String>::new());
        let a = create(&d, "md").unwrap();
        let b = create(&d, "md").unwrap();
        std::fs::create_dir_all(d.join("a-dir")).unwrap();
        std::fs::write(d.join(".dotfile"), "x").unwrap();
        // Touch `a` so it is newest again: the order is by modification, not
        // by number.
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&d, &a, "newer").unwrap();
        assert_eq!(list(&d).unwrap(), vec![a, b]);
    }
}
