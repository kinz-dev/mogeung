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

/// How deep a scratch tree may go. `R-L9`.
///
/// A limit rather than none, because every path here is walked and joined and
/// a pathological depth is the cheap way to make that expensive. Eight is far
/// past what anyone will nest a scratchpad.
const MAX_DEPTH: usize = 8;

/// Check a **relative path** of one or more segments. `R-L9`.
///
/// This replaces *"a name is a bare file name"* as the shape of the rule, and
/// [ADR-0035](../../../docs/decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)'s
/// second amendment is where that is argued. The rule is now *"a path is a
/// sequence of bare names"* — which is the same guarantee said once per
/// segment, plus a depth cap:
///
/// - **`..` cannot be spelled**, because a segment is checked by
///   [`check_name`] and that refuses a leading dot outright.
/// - **An absolute path cannot be spelled**, because an empty first segment is
///   refused and `/` is the separator being split on.
/// - **A backslash is not a separator here**, and is refused by `check_name`'s
///   character set rather than treated as one — this daemon runs on Unix, and
///   quietly accepting a Windows separator would mean two spellings of one
///   path.
///
/// It is deliberately **not** the only fence. [`resolve`] joins and then checks
/// containment against the real directory, which is what catches the thing no
/// amount of string checking can: a symlink inside the scratch directory
/// pointing out of it.
pub fn check_path(path: &str) -> Result<()> {
    if path.is_empty() || path.len() > 512 {
        bail!("a scratch path must be 1–512 characters");
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() > MAX_DEPTH {
        bail!("a scratch path may be at most {MAX_DEPTH} folders deep");
    }
    for segment in segments {
        check_name(segment)?;
    }
    Ok(())
}

/// Join a checked path to the scratch directory, and prove the result is
/// inside it. `R-L9`.
///
/// **The check that string rules cannot make.** `check_path` refuses every
/// spelling of an escape, and a symlink is not a spelling: a directory created
/// here by hand — or by an agent with a shell — pointing at `$HOME` would let
/// an ordinary-looking `notes/secrets.txt` write outside the scratch folder.
/// So the joined path is canonicalized and required to be under the
/// canonical scratch directory.
///
/// A path that does not exist yet cannot be canonicalized, so its **parent** is
/// checked instead and the final segment appended — the parent is what a
/// symlink would have to be.
pub fn resolve(dir: &Path, path: &str) -> Result<PathBuf> {
    check_path(path)?;
    let root = std::fs::canonicalize(dir)
        .with_context(|| format!("resolving {}", dir.display()))?;
    let joined = root.join(path);

    let checked = match std::fs::canonicalize(&joined) {
        Ok(real) => real,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = joined
                .parent()
                .ok_or_else(|| anyhow::anyhow!("{path} has no parent"))?;
            let real_parent = std::fs::canonicalize(parent)
                .with_context(|| format!("{path} is not somewhere that exists"))?;
            let leaf = joined
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("{path} names no file"))?;
            real_parent.join(leaf)
        }
        Err(e) => return Err(e).with_context(|| format!("resolving {path}")),
    };

    if !checked.starts_with(&root) {
        bail!("{path} is not inside the scratch folder");
    }
    Ok(checked)
}

/// The extension a new file gets: short, alphanumeric, no dot.
pub fn check_ext(ext: &str) -> Result<()> {
    if ext.is_empty() || ext.len() > 12 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("a scratch file extension must be 1–12 letters or digits");
    }
    Ok(())
}

/// Every scratch file, newest first, as a path relative to the scratch
/// directory. `R-L5`; folders since `R-L9`.
///
/// Walks rather than reads one level, because a file in a folder is still a
/// scratch file and a panel that lists only the root would hide what you just
/// made. Depth-capped by the same `MAX_DEPTH` the path check uses, and
/// **symlinked directories are not followed** — a link is the one way a walk
/// leaves the tree, and the panel is a view of this folder.
pub fn list(dir: &Path) -> Result<Vec<String>> {
    let mut entries: Vec<(std::time::SystemTime, String)> = Vec::new();
    walk(dir, "", 1, &mut entries)?;
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(entries.into_iter().map(|(_, n)| n).collect())
}

fn walk(
    dir: &Path,
    prefix: &str,
    depth: usize,
    out: &mut Vec<(std::time::SystemTime, String)>,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // A dotfile, or anything else we would refuse on the wire, is not ours
        // to list: offering a name the verbs would reject is a row that cannot
        // be clicked.
        if check_name(&name).is_err() {
            continue;
        }
        // `symlink_metadata`, so a link is judged as a link rather than as
        // whatever it points at. Following one would walk out of the tree.
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(link) = entry.path().symlink_metadata() else { continue };
        if link.file_type().is_symlink() {
            continue;
        }
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if meta.is_dir() {
            walk(&entry.path(), &path, depth + 1, out)?;
        } else if meta.is_file() {
            out.push((meta.modified().unwrap_or(std::time::UNIX_EPOCH), path));
        }
    }
    Ok(())
}

/// Every folder in the tree, so the panel can show one you have not yet put a
/// file in. `R-L9`.
pub fn folders(dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    walk_folders(dir, "", 1, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_folders(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<String>) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if check_name(&name).is_err() {
            continue;
        }
        let Ok(link) = entry.path().symlink_metadata() else { continue };
        if link.file_type().is_symlink() || !link.is_dir() {
            continue;
        }
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        out.push(path.clone());
        walk_folders(&entry.path(), &path, depth + 1, out)?;
    }
    Ok(())
}

/// Make a folder, and every folder above it. `R-L9`.
pub fn mkdir(dir: &Path, path: &str) -> Result<()> {
    let target = resolve(dir, path)?;
    if target.exists() {
        bail!("{path} is already here");
    }
    std::fs::create_dir_all(&target).with_context(|| format!("creating {path}"))?;
    Ok(())
}

/// Remove a folder and everything in it. `R-L9`.
///
/// Recursive, and the most destructive verb this daemon has. Three things stand
/// between it and a mistake: [`resolve`] proves the target is inside the
/// scratch directory *after* following symlinks, the target must be a directory
/// rather than a file, and the window asks first, naming how many files go with
/// it.
pub fn rmdir(dir: &Path, path: &str) -> Result<()> {
    let target = resolve(dir, path)?;
    if !target.exists() {
        return Ok(());
    }
    // A symlink to a directory reports `is_dir()` through `metadata`, so the
    // link's own type is what decides — otherwise removing a link here would
    // recurse into whatever it points at.
    let link = target
        .symlink_metadata()
        .with_context(|| format!("reading {path}"))?;
    if link.file_type().is_symlink() {
        bail!("{path} is a link, not a folder here");
    }
    if !link.is_dir() {
        bail!("{path} is not a folder");
    }
    std::fs::remove_dir_all(&target).with_context(|| format!("removing {path}"))?;
    Ok(())
}

/// Make a new empty file at the root and answer with its name.
pub fn create(dir: &Path, ext: &str) -> Result<String> {
    create_in(dir, None, ext)
}

/// Make a new empty file in `folder` (or at the root) and answer with its path.
/// `R-L9`.
///
/// **The name is still the daemon's**, which is the rule folders had to leave
/// standing: the window may say *where*, and never *what*. The number is the
/// first free one **in that folder**, so two folders each have a `scratch-1`
/// and neither had to consult the other.
pub fn create_in(dir: &Path, folder: Option<&str>, ext: &str) -> Result<String> {
    check_ext(ext)?;
    let target_dir = match folder {
        Some(f) => {
            let resolved = resolve(dir, f)?;
            if !resolved.is_dir() {
                bail!("there is no folder called {f}");
            }
            resolved
        }
        None => {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            std::fs::canonicalize(dir).with_context(|| format!("resolving {}", dir.display()))?
        }
    };
    for n in 1..10_000u32 {
        let name = format!("scratch-{n}.{ext}");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target_dir.join(&name))
        {
            Ok(_) => {
                return Ok(match folder {
                    Some(f) => format!("{f}/{name}"),
                    None => name,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("creating {name}")),
        }
    }
    bail!("ten thousand scratch files with that extension — time to tidy up");
}

/// The whole file.
pub fn read(dir: &Path, name: &str) -> Result<String> {
    let path = resolve(dir, name)?;
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
    let path = resolve(dir, name)?;
    if content.len() as u64 > CAP {
        bail!("{name} would be larger than a scratch file should be");
    }
    // Only a file that exists: a write is an edit, and create is the one
    // verb that mints a name. Without this a window could create files with
    // names the daemon never chose.
    if !path.is_file() {
        bail!("{name} is not a scratch file here — create one first");
    }
    // Beside the file rather than at the root, or a write into a folder would
    // rename across directories — which is still atomic here but need not be
    // on every filesystem. The leading dot keeps it out of `list`.
    let tmp = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("scratch")
    ));
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
    if name == to {
        return Ok(());
    }
    let from = resolve(dir, name)?;
    if !from.is_file() {
        bail!("{name} is not a scratch file here");
    }
    let target = resolve(dir, to)?;
    // Since `R-L9` a rename is also a **move**: `to` may name another folder,
    // and that folder has to exist — creating one implicitly would make a typo
    // into a new directory rather than an error.
    if let Some(parent) = target.parent() {
        if !parent.is_dir() {
            bail!("there is no folder for {to}");
        }
    }
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
    let path = resolve(dir, name)?;
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
    let from = resolve(dir, name)?;
    if !from.is_file() {
        bail!("{name} is not a scratch file here");
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("txt");
    check_ext(ext)?;
    // Minted **beside the original**, not at the root: a copy that jumped out
    // of its folder would be a copy you then have to go and find.
    let folder = name.rsplit_once('/').map(|(d, _)| d);
    let fresh = create_in(dir, folder, ext)?;
    let bytes = std::fs::read(&from).with_context(|| format!("reading {name}"))?;
    std::fs::write(resolve(dir, &fresh)?, bytes).with_context(|| format!("writing {fresh}"))?;
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
    ///
    /// **Renamed and re-argued on 2026-09-09.** It used to be called
    /// `a_rename_cannot_spell_a_path`, and that name became a lie when `R-L9`
    /// made a path the ordinary thing a rename spells. What has to stay true is
    /// narrower and is what this now says: a rename cannot leave the scratch
    /// folder, cannot make a hidden file, and cannot invent a folder on its way.
    #[test]
    fn a_rename_cannot_leave_the_scratch_folder() {
        let d = fresh("escape");
        std::fs::create_dir_all(&d).unwrap();
        let name = create(&d, "txt").unwrap();

        for bad in ["../escaped.txt", "/etc/passwd", ".hidden", "with space", "a/../../b.txt"] {
            assert!(rename(&d, &name, bad).is_err(), "{bad} should be refused");
        }
        // A path into a folder that does not exist is refused too — but as a
        // missing folder rather than as a bad name, which is a different error
        // and a different message.
        assert!(rename(&d, &name, "sub/dir.txt").is_err());

        assert!(d.join(&name).is_file(), "the file did not move");

        // And the one that must now succeed, or the fence has been drawn too
        // tightly to be useful.
        mkdir(&d, "sub").unwrap();
        rename(&d, &name, "sub/dir.txt").unwrap();
        assert!(d.join("sub/dir.txt").is_file());
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

    // -- Folders. `R-L9`. The fence changed shape here, so it is tested hardest.

    #[test]
    fn a_path_is_a_sequence_of_bare_names() {
        assert!(check_path("a.txt").is_ok());
        assert!(check_path("notes/a.txt").is_ok());
        assert!(check_path("a/b/c/d.sql").is_ok());
    }

    /// The rule `..` used to be caught by is gone; a segment check has to do
    /// the same work once per segment.
    #[test]
    fn a_path_cannot_climb_out() {
        for bad in [
            "../escaped.txt",
            "notes/../../escaped.txt",
            "..",
            "notes/..",
            "/etc/passwd",
            "notes//a.txt",
            "",
            ".hidden/a.txt",
            "notes/.hidden",
            "notes\\a.txt",
        ] {
            assert!(check_path(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn a_path_may_not_be_absurdly_deep() {
        let deep = (0..9).map(|_| "d").collect::<Vec<_>>().join("/") + "/a.txt";
        assert!(check_path(&deep).is_err());
    }

    /// **The check no string rule can make.** A directory symlink inside the
    /// scratch folder pointing out of it turns an innocent-looking path into a
    /// write anywhere the daemon can reach.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_folder_cannot_be_written_through() {
        let d = fresh("symlink");
        std::fs::create_dir_all(&d).unwrap();
        let outside = d.parent().unwrap().join("mogeung-scratch-outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, d.join("escape")).unwrap();

        // The name is impeccable; the folder it names is not.
        assert!(resolve(&d, "escape/secrets.txt").is_err());
        assert!(write(&d, "escape/secrets.txt", "no").is_err());
        assert!(rmdir(&d, "escape").is_err(), "a link is not a folder to remove");

        std::fs::remove_dir_all(&outside).ok();
    }

    /// And a link must not be walked into, or the panel lists files that are
    /// not in this tree at all.
    #[cfg(unix)]
    #[test]
    fn the_listing_does_not_follow_a_symlink_out() {
        let d = fresh("walklink");
        std::fs::create_dir_all(&d).unwrap();
        let outside = d.parent().unwrap().join("mogeung-scratch-elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("theirs.txt"), "not yours").unwrap();
        std::os::unix::fs::symlink(&outside, d.join("elsewhere")).unwrap();
        create(&d, "txt").unwrap();

        let names = list(&d).unwrap();

        assert!(names.iter().all(|n| !n.contains("theirs")), "{names:?}");
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn a_folder_can_be_made_and_listed() {
        let d = fresh("mkdir");
        std::fs::create_dir_all(&d).unwrap();

        mkdir(&d, "notes").unwrap();
        mkdir(&d, "notes/deep").unwrap();

        assert_eq!(folders(&d).unwrap(), vec!["notes", "notes/deep"]);
        assert!(mkdir(&d, "notes").is_err(), "making one twice is an error");
    }

    #[test]
    fn a_file_can_be_made_inside_a_folder_and_is_listed_by_its_path() {
        let d = fresh("inside");
        std::fs::create_dir_all(&d).unwrap();
        mkdir(&d, "sql").unwrap();

        let name = create_in(&d, Some("sql"), "sql").unwrap();

        assert_eq!(name, "sql/scratch-1.sql");
        write(&d, &name, "select 1").unwrap();
        assert_eq!(read(&d, &name).unwrap(), "select 1");
        assert!(list(&d).unwrap().contains(&"sql/scratch-1.sql".to_string()));
    }

    /// Two folders each get their own numbering, because the first free `n` is
    /// asked of the folder rather than of the tree.
    #[test]
    fn numbering_is_per_folder() {
        let d = fresh("numbering");
        std::fs::create_dir_all(&d).unwrap();
        mkdir(&d, "a").unwrap();
        mkdir(&d, "b").unwrap();

        assert_eq!(create_in(&d, Some("a"), "txt").unwrap(), "a/scratch-1.txt");
        assert_eq!(create_in(&d, Some("b"), "txt").unwrap(), "b/scratch-1.txt");
    }

    /// Moving is renaming to another folder, which is why there is no separate
    /// verb for it.
    #[test]
    fn a_rename_across_folders_is_a_move() {
        let d = fresh("move");
        std::fs::create_dir_all(&d).unwrap();
        mkdir(&d, "kept").unwrap();
        let name = create(&d, "java").unwrap();
        write(&d, &name, "class A {}").unwrap();

        rename(&d, &name, &format!("kept/{name}")).unwrap();

        assert_eq!(read(&d, &format!("kept/{name}")).unwrap(), "class A {}");
        assert!(read(&d, &name).is_err(), "the old path is gone");
    }

    /// A typo in the folder should be an error, not a new folder.
    #[test]
    fn a_move_into_a_folder_that_is_not_there_is_refused() {
        let d = fresh("nofolder");
        std::fs::create_dir_all(&d).unwrap();
        let name = create(&d, "txt").unwrap();

        assert!(rename(&d, &name, "typo/a.txt").is_err());
        assert!(read(&d, &name).is_ok(), "the file did not move");
    }

    #[test]
    fn removing_a_folder_takes_what_is_in_it() {
        let d = fresh("rmdir");
        std::fs::create_dir_all(&d).unwrap();
        mkdir(&d, "old").unwrap();
        create_in(&d, Some("old"), "txt").unwrap();
        let kept = create(&d, "txt").unwrap();

        rmdir(&d, "old").unwrap();

        assert_eq!(folders(&d).unwrap(), Vec::<String>::new());
        assert!(read(&d, &kept).is_ok(), "the file outside it survived");
    }

    #[test]
    fn removing_a_file_as_though_it_were_a_folder_is_refused() {
        let d = fresh("notafolder");
        std::fs::create_dir_all(&d).unwrap();
        let name = create(&d, "txt").unwrap();

        assert!(rmdir(&d, &name).is_err());
        assert!(read(&d, &name).is_ok());
    }

    /// A copy stays where its original was, or it is a copy you have to find.
    #[test]
    fn a_duplicate_lands_beside_its_original() {
        let d = fresh("dupfolder");
        std::fs::create_dir_all(&d).unwrap();
        mkdir(&d, "sql").unwrap();
        let name = create_in(&d, Some("sql"), "sql").unwrap();
        write(&d, &name, "select 1").unwrap();

        let copy = duplicate(&d, &name).unwrap();

        assert!(copy.starts_with("sql/"), "{copy} left its folder");
        assert_eq!(read(&d, &copy).unwrap(), "select 1");
    }
}
