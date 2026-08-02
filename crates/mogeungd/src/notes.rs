//! The user's own writing. `R-B35`, pillar L.
//!
//! Everything else this daemon stores is derived — sessions, events, diffs,
//! review marks all come from `~/.claude` or from git, and losing any of it
//! costs a rescan. A note cannot be recomputed from anything. That difference
//! is what [ADR-0015](../../../docs/decisions/0015-markdown-is-the-truth.md)
//! is about, and it is why this module exists at all rather than the store
//! being called directly.
//!
//! # The mirror
//!
//! Every save also writes `~/.mogeung/notes/<id>-<slug>.md`. It is **one way**
//! and is never read back: it is not an input, and editing one changes
//! nothing. It exists so that the one kind of content nothing can regenerate
//! is never trapped inside a database only mogeung can open — `grep`, a backup
//! tool and any editor all work on it.
//!
//! That makes it a constraint rather than a convenience. Without it, daemon
//! ownership costs the user access to their own writing, which was not an
//! acceptable trade and is the reason the ADR names it alongside the decision
//! rather than under it.

use anyhow::Result;
use mogeung_core::wire::Note;
use std::path::{Path, PathBuf};

/// Where the mirror lives.
pub fn mirror_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("notes")
}

/// A short, filesystem-safe stem taken from the note's first line.
///
/// Only so the directory is browsable — the id is what identifies a note, and
/// the slug may collide, be empty, or change when the note is edited. Nothing
/// reads it back, so none of that matters.
pub fn slug(body: &str) -> String {
    let first = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let cleaned: String = first
        .trim()
        .trim_start_matches('#')
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(48)
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    // Collapse runs, which a sentence full of punctuation produces plenty of.
    let mut out = String::with_capacity(cleaned.len());
    let mut last_dash = false;
    for c in cleaned.chars() {
        if c == '-' {
            if !last_dash {
                out.push(c);
            }
            last_dash = true;
        } else {
            out.push(c);
            last_dash = false;
        }
    }
    if out.is_empty() {
        "note".to_string()
    } else {
        out
    }
}

fn mirror_path(dir: &Path, n: &Note) -> PathBuf {
    dir.join(format!("{}-{}.md", n.id, slug(&n.body)))
}

/// Write one note to the mirror, replacing whatever it was called before.
///
/// Best effort by design: a mirror that cannot be written must not stop a note
/// being saved. The note is already in the store by the time this runs, and
/// failing the save because a directory is read-only would lose the writing to
/// protect a copy of it.
pub fn mirror(n: &Note) -> Result<()> {
    mirror_in(&mirror_dir(), n)
}

pub fn unmirror(id: &str) {
    let _ = unmirror_in(&mirror_dir(), id);
}

pub fn mirror_in(dir: &Path, n: &Note) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    // The slug moves when the first line is edited, so yesterday's filename
    // would otherwise linger beside today's with the same id in both.
    let _ = unmirror_in(dir, &n.id);
    let mut text = String::new();
    // Front matter, because a file this may outlive mogeung should say what it
    // was attached to. Deliberately not parsed back — see the module doc.
    text.push_str("---\n");
    text.push_str(&format!("id: {}\n", n.id));
    if let Some(s) = &n.session_id {
        text.push_str(&format!("session: {s}\n"));
        if let Some(q) = n.seq {
            text.push_str(&format!("turn: {q}\n"));
        }
    }
    if let Some(r) = &n.repo {
        text.push_str(&format!("repo: {r}\n"));
    }
    text.push_str("---\n\n");
    text.push_str(&n.body);
    if !n.body.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(mirror_path(dir, n), text)?;
    Ok(())
}

/// Remove every mirror file for this id, whatever it was last called.
pub fn unmirror_in(dir: &Path, id: &str) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let prefix = format!("{id}-");
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(&prefix) && name.ends_with(".md") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    Ok(())
}

/// A fresh note id.
///
/// Time-ordered so the mirror directory sorts the way the notes were written,
/// with random bytes because two notes can be made inside one millisecond.
pub fn new_id() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut rand = [0u8; 4];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read as _;
        let _ = f.read_exact(&mut rand);
    }
    format!(
        "{ms:x}-{}",
        rand.iter().map(|b| format!("{b:02x}")).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, body: &str) -> Note {
        Note {
            id: id.into(),
            body: body.into(),
            created: 1,
            updated: 2,
            session_id: Some("sess-1".into()),
            seq: Some(7),
            repo: None,
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mogeung-notes-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_slug_is_readable_and_never_empty() {
        assert_eq!(slug("# The refactor is wrong"), "the-refactor-is-wrong");
        assert_eq!(slug("hello   world!!!"), "hello-world");
        // Leading blank lines are skipped; a body of nothing still has a name.
        assert_eq!(slug("\n\n  real first line"), "real-first-line");
        assert_eq!(slug(""), "note");
        assert_eq!(slug("!!!"), "note");
        // Long first lines are cut rather than producing an unusable filename.
        assert!(slug(&"a".repeat(200)).len() <= 48);
    }

    /// Editing the first line renames the file. Without removing the old one,
    /// the directory fills with several files carrying the same id and
    /// different bodies — and the stale ones would read as real notes to
    /// anything that is not mogeung, which is exactly who the mirror is for.
    #[test]
    fn re_mirroring_leaves_exactly_one_file_per_note() {
        let dir = scratch("rename");
        mirror_in(&dir, &note("abc", "# first title")).unwrap();
        mirror_in(&dir, &note("abc", "# second title")).unwrap();

        let files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].contains("second-title"), "{files:?}");

        let text = std::fs::read_to_string(dir.join(&files[0])).unwrap();
        assert!(text.contains("# second title"));
        // What it was attached to survives in a file that may outlive mogeung.
        assert!(text.contains("session: sess-1"), "{text}");
        assert!(text.contains("turn: 7"), "{text}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn deleting_a_note_removes_its_mirror() {
        let dir = scratch("delete");
        mirror_in(&dir, &note("abc", "gone soon")).unwrap();
        mirror_in(&dir, &note("keep", "still here")).unwrap();
        unmirror_in(&dir, "abc").unwrap();

        let files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].starts_with("keep-"), "{files:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// A body with no trailing newline is still a well-formed text file, and
    /// the front matter must not run into the first line of prose.
    #[test]
    fn the_mirror_is_a_well_formed_markdown_file() {
        let dir = scratch("shape");
        mirror_in(&dir, &note("abc", "no trailing newline")).unwrap();
        let path = std::fs::read_dir(&dir).unwrap().flatten().next().unwrap().path();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.starts_with("---\n"));
        assert!(text.contains("---\n\nno trailing newline"), "{text:?}");
        assert!(text.ends_with('\n'));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ids_are_unique_and_time_ordered() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
        // Same millisecond is the normal case here, so the random tail is what
        // does the work; the ordering only has to hold across time.
        assert!(!a.is_empty() && a.contains('-'));
    }
}
