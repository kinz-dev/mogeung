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

/// Rename a scratch file. `R-L7`.
///
/// **This is the verb that hands the window naming power**, which
/// [ADR-0035](../../../docs/decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)
/// deliberately withheld: its rule 1 says *"the daemon mints every name"*, so
/// that a window could not place a file of its choosing even inside this
/// directory. A rename is exactly that power, and the fences are what make it
/// bearable — `check_name` still governs the target, so no separator, no
/// leading dot and no `..` can be spelled; and an existing target is refused
/// rather than replaced, so a rename can never destroy a file you did not name.
///
/// It cannot create, either: the source has to be a file that is already here.
pub fn rename(dir: &Path, name: &str, to: &str) -> Result<()> {
    check_name(name)?;
    check_name(to)?;
    if name == to {
        return Ok(());
    }
    let from = dir.join(name);
    if !from.is_file() {
        bail!("{name} is not a scratch file here");
    }
    let target = dir.join(to);
    // Checked rather than clobbered. `std::fs::rename` replaces silently on
    // Unix, and silently replacing a file the user did not name is the one
    // outcome a rename must not have.
    if target.exists() {
        bail!("{to} is already here — pick another name");
    }
    std::fs::rename(&from, &target).with_context(|| format!("renaming {name} to {to}"))?;
    Ok(())
}

/// Delete a scratch file. `R-L7`.
///
/// ADR-0035 said deleting one was `rm` and meant it: *"they are the user's, on
/// the user's disk, in a folder any tool can open — and it is also the limit."*
/// The limit moved on 2026-09-09 (see that ADR's amendment). What did not move
/// is where it may point: `check_name` first, and the join is to this directory
/// only, so the verb cannot be aimed at anything else on the machine.
///
/// A file that is already gone is **not** an error. Two windows listing the
/// same directory will race, and "delete something that is not there" is the
/// outcome you asked for.
pub fn delete(dir: &Path, name: &str) -> Result<()> {
    check_name(name)?;
    let path = dir.join(name);
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() {
        bail!("{name} is not a file");
    }
    std::fs::remove_file(&path).with_context(|| format!("deleting {name}"))?;
    Ok(())
}

/// Copy a scratch file to a new, daemon-minted name. `R-L7`.
///
/// The name is **minted here**, not asked for, which keeps ADR-0035's rule 1
/// intact for the one operation that creates a file: duplicating
/// `query.sql` gives `scratch-<n>.sql`, the same shape [`create`] produces.
pub fn duplicate(dir: &Path, name: &str) -> Result<String> {
    check_name(name)?;
    let from = dir.join(name);
    if !from.is_file() {
        bail!("{name} is not a scratch file here");
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("txt");
    check_ext(ext)?;
    let fresh = create(dir, ext)?;
    let bytes = std::fs::read(&from).with_context(|| format!("reading {name}"))?;
    std::fs::write(dir.join(&fresh), bytes).with_context(|| format!("writing {fresh}"))?;
    Ok(fresh)
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

    // -- Managing the files rather than their contents. `R-L7`.

    #[test]
    fn a_rename_moves_the_file_and_keeps_the_content() {
        let d = fresh("rename");
        std::fs::create_dir_all(&d).unwrap();
        let name = create(&d, "sql").unwrap();
        write(&d, &name, "select 1").unwrap();

        rename(&d, &name, "query.sql").unwrap();

        assert!(!d.join(&name).exists(), "the old name is gone");
        assert_eq!(read(&d, "query.sql").unwrap(), "select 1");
    }

    /// `std::fs::rename` replaces silently on Unix, and silently destroying a
    /// file the user did not name is the one outcome a rename must not have.
    #[test]
    fn a_rename_onto_an_existing_file_is_refused() {
        let d = fresh("clobber");
        std::fs::create_dir_all(&d).unwrap();
        let a = create(&d, "sql").unwrap();
        let b = create(&d, "sql").unwrap();
        write(&d, &a, "keep me").unwrap();
        write(&d, &b, "and me").unwrap();

        assert!(rename(&d, &b, &a).is_err());
        assert_eq!(read(&d, &a).unwrap(), "keep me");
        assert_eq!(read(&d, &b).unwrap(), "and me");
    }

    /// The verb that hands the window a name is the verb most worth fencing.
    #[test]
    fn a_rename_cannot_spell_a_path() {
        let d = fresh("escape");
        std::fs::create_dir_all(&d).unwrap();
        let name = create(&d, "txt").unwrap();

        for bad in ["../escaped.txt", "sub/dir.txt", ".hidden", "with space"] {
            assert!(rename(&d, &name, bad).is_err(), "{bad} should be refused");
        }
        assert!(d.join(&name).is_file(), "the file did not move");
    }

    #[test]
    fn renaming_to_the_same_name_is_a_no_op() {
        let d = fresh("same");
        std::fs::create_dir_all(&d).unwrap();
        let name = create(&d, "txt").unwrap();
        write(&d, &name, "body").unwrap();

        rename(&d, &name, &name).unwrap();

        assert_eq!(read(&d, &name).unwrap(), "body");
    }

    #[test]
    fn a_delete_removes_it_from_the_list() {
        let d = fresh("delete");
        std::fs::create_dir_all(&d).unwrap();
        let a = create(&d, "txt").unwrap();
        let b = create(&d, "txt").unwrap();

        delete(&d, &a).unwrap();

        let names = list(&d).unwrap();
        assert!(!names.contains(&a));
        assert!(names.contains(&b));
    }

    /// Two windows list one directory and will race. Deleting something that
    /// is already gone is the outcome that was asked for.
    #[test]
    fn deleting_what_is_not_there_is_not_an_error() {
        let d = fresh("gone");
        std::fs::create_dir_all(&d).unwrap();
        assert!(delete(&d, "never-existed.txt").is_ok());
    }

    #[test]
    fn a_delete_cannot_be_aimed_outside_the_directory() {
        let d = fresh("aim");
        std::fs::create_dir_all(&d).unwrap();
        let outside = d.parent().unwrap().join("mogeung-scratch-bystander.txt");
        std::fs::write(&outside, "not yours").unwrap();

        assert!(delete(&d, "../mogeung-scratch-bystander.txt").is_err());

        assert!(outside.is_file(), "a file outside the directory was deleted");
        std::fs::remove_file(&outside).ok();
    }

    /// The copy gets a name the **daemon** minted, which is how ADR-0035's
    /// rule 1 survives the one operation here that creates a file.
    #[test]
    fn a_duplicate_is_a_fresh_daemon_minted_name_with_the_same_content() {
        let d = fresh("dup");
        std::fs::create_dir_all(&d).unwrap();
        let name = create(&d, "java").unwrap();
        write(&d, &name, "class A {}").unwrap();

        let copy = duplicate(&d, &name).unwrap();

        assert_ne!(copy, name);
        assert!(copy.starts_with("scratch-"), "{copy} was not minted here");
        assert!(copy.ends_with(".java"), "{copy} lost the extension");
        assert_eq!(read(&d, &copy).unwrap(), "class A {}");
        assert_eq!(read(&d, &name).unwrap(), "class A {}", "the original is untouched");
    }
}
