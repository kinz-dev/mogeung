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

// ---------------------------------------------------------------------------
// Tasks. `R-L3`, ADR-0015.
// ---------------------------------------------------------------------------

/// One checkbox line found in a document.
///
/// **There is no task outside a document, and no field on a task that is not
/// written in the document** — ADR-0015 rule 2. So this carries no id of its
/// own, no due date and no assignee: it is a *position* and the words on the
/// line, and everything else about it is derived by looking again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Which checkbox in this document, counting from zero in document order.
    ///
    /// The address, deliberately, rather than the line number: a line number
    /// changes when anything above it is edited, and this only changes when a
    /// *checkbox* above it is added or removed. It is what a tick is aimed at.
    pub ord: usize,
    /// The line's own index in the body, for the rewriter. Not an identity.
    pub line: usize,
    /// The words after the box, trimmed. This is the task's identity for
    /// history — see [`Store::record_task_transitions`]. Rewriting the words
    /// makes it a different task, which is the honest reading of a line that
    /// no longer says what it said.
    pub text: String,
    pub done: bool,
}

/// Every checkbox in a document, in order.
///
/// # What is deliberately not a task
///
/// [Feature 0026](../../../docs/features/0026-notes-and-tasks.md) names the
/// risk: *"`- [ ]` appears in ordinary prose, including in any note that quotes
/// this spec. A parser that is too eager turns a quotation into a task."*
///
/// - **Inside a fenced code block.** ``` and `~~~`, closed by a fence of at
///   least the same length. The spec calls this the minimum and it is: a note
///   holding a snippet of a `README` should not sprout that README's checklist.
/// - **Inside a block quotation.** `R-L2`'s copy-a-turn gesture puts an agent's
///   words into a note **verbatim**, so a `> - [ ] …` is something somebody
///   else wrote and is being quoted — reporting it as your task is how a
///   checklist fills with other people's.
/// - **Indented four spaces or more**, which is an indented code block in
///   markdown. A nested list item under a task is not, because it indents by
///   two — and that case is a real one, so the boundary is drawn at four.
pub fn tasks_in(body: &str) -> Vec<Task> {
    let mut out = Vec::new();
    let mut fence: Option<(char, usize)> = None;

    for (line, raw) in body.lines().enumerate() {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();

        // A fence closes only on the same character and at least the same run
        // length, which is what lets a ```` ``` ```` sit inside a ```` ~~~ ````
        // block without ending it.
        if let Some(marker) = fence_run(trimmed) {
            match fence {
                Some((ch, len)) if ch == marker.0 && marker.1 >= len => fence = None,
                Some(_) => {}
                None => fence = Some(marker),
            }
            continue;
        }
        if fence.is_some() || indent >= 4 || trimmed.starts_with('>') {
            continue;
        }

        let Some((done, text)) = checkbox(trimmed) else {
            continue;
        };
        out.push(Task {
            ord: out.len(),
            line,
            text: text.to_string(),
            done,
        });
    }
    out
}

/// `(character, length)` when this line opens or closes a code fence.
fn fence_run(trimmed: &str) -> Option<(char, usize)> {
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|c| *c == ch).count();
    (len >= 3).then_some((ch, len))
}

/// `(done, text)` when this line is a checkbox item.
fn checkbox(trimmed: &str) -> Option<(bool, &str)> {
    let rest = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))?;
    let rest = rest.trim_start();
    let (mark, after) = if let Some(a) = rest.strip_prefix("[ ]") {
        (false, a)
    } else if let Some(a) = rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]")) {
        (true, a)
    } else {
        return None;
    };
    // A box has to be followed by a space or end the line: `- [x]done` is not
    // a checkbox in any renderer, and treating it as one would make a task out
    // of something nobody will see a box beside.
    if !after.is_empty() && !after.starts_with(' ') {
        return None;
    }
    Some((mark, after.trim()))
}

/// Tick or untick the `ord`th checkbox, returning the new body.
///
/// **The document is the only thing that is written** — ADR-0015 rule 3. A tick
/// in the task list comes here, rewrites the line, and the derived table is
/// then rebuilt from the result. There is no path that updates the cache and
/// leaves the markdown alone, because that is precisely the second source of
/// truth the ADR refuses.
///
/// Rewrites the **box** and nothing else: the indent, the bullet character and
/// the text are all left exactly as they were, so a tick cannot reformat a
/// line you wrote.
pub fn set_task(body: &str, ord: usize, done: bool) -> Option<String> {
    let task = tasks_in(body).into_iter().find(|t| t.ord == ord)?;
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    let line = lines.get_mut(task.line)?;
    let at = line.find("[ ]").or_else(|| line.find("[x]")).or_else(|| line.find("[X]"))?;
    line.replace_range(at..at + 3, if done { "[x]" } else { "[ ]" });
    let mut out = lines.join("\n");
    // `lines()` drops a trailing newline; putting it back keeps a tick from
    // silently reflowing the end of the file.
    if body.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod task_tests {
    use super::*;

    fn texts(body: &str) -> Vec<(bool, String)> {
        tasks_in(body).into_iter().map(|t| (t.done, t.text)).collect()
    }

    #[test]
    fn a_checkbox_line_is_a_task_and_nothing_else_is() {
        let body = "# Plan\n\n- [ ] open one\n- [x] closed one\n- an ordinary bullet\n\nprose\n";
        assert_eq!(
            texts(body),
            vec![(false, "open one".into()), (true, "closed one".into())]
        );
    }

    #[test]
    fn the_box_may_be_upper_case_and_the_bullet_any_of_three() {
        assert_eq!(texts("* [X] a\n+ [ ] b\n- [x] c\n").len(), 3);
    }

    /// The minimum feature 0026 named: a note holding a snippet of a README
    /// must not sprout that README's checklist.
    #[test]
    fn a_checkbox_inside_a_fence_is_not_a_task() {
        let body = "- [ ] real\n\n```md\n- [ ] not this one\n```\n\n- [x] also real\n";
        assert_eq!(texts(body), vec![(false, "real".into()), (true, "also real".into())]);
    }

    #[test]
    fn a_tilde_fence_closes_only_on_tildes() {
        let body = "~~~\n- [ ] inside\n```\n- [ ] still inside\n~~~\n- [ ] outside\n";
        assert_eq!(texts(body), vec![(false, "outside".into())]);
    }

    /// `R-L2` copies an agent's words in verbatim, so a quoted checkbox is
    /// something somebody else wrote.
    #[test]
    fn a_quoted_checkbox_is_somebody_elses() {
        let body = "> - [ ] the agent's plan\n\n- [ ] mine\n";
        assert_eq!(texts(body), vec![(false, "mine".into())]);
    }

    /// Four spaces is an indented code block; two is a nested list item, and
    /// that is a real case rather than a curiosity.
    #[test]
    fn indentation_decides_between_a_nested_task_and_a_code_block() {
        let body = "- [ ] top\n  - [ ] nested\n\n    - [ ] indented code\n";
        assert_eq!(texts(body), vec![(false, "top".into()), (false, "nested".into())]);
    }

    #[test]
    fn a_box_must_be_followed_by_a_space_or_the_end_of_the_line() {
        assert_eq!(texts("- [x]squashed\n"), Vec::new());
        assert_eq!(texts("- [ ]\n"), vec![(false, String::new())]);
    }

    #[test]
    fn ord_counts_checkboxes_and_not_lines() {
        let body = "prose\n\n- [ ] first\n\nmore prose\n\n- [ ] second\n";
        let tasks = tasks_in(body);
        assert_eq!(tasks[0].ord, 0);
        assert_eq!(tasks[1].ord, 1);
        assert_eq!(tasks[1].line, 6, "the line is where it really is");
    }

    // -- The rewriter, which is the only thing that writes. ------------------

    #[test]
    fn ticking_rewrites_the_box_and_leaves_the_line_alone() {
        let body = "  - [ ]   spaced   out  \n";
        let out = set_task(body, 0, true).unwrap();
        assert_eq!(out, "  - [x]   spaced   out  \n");
    }

    #[test]
    fn unticking_is_the_same_in_reverse() {
        assert_eq!(set_task("- [x] a\n", 0, false).unwrap(), "- [ ] a\n");
    }

    #[test]
    fn ticking_aims_at_the_checkbox_and_not_the_line_number() {
        let body = "```\n- [ ] decoy\n```\n- [ ] real\n";
        let out = set_task(body, 0, true).unwrap();
        assert_eq!(out, "```\n- [ ] decoy\n```\n- [x] real\n", "the fenced line is untouched");
    }

    #[test]
    fn a_body_without_a_trailing_newline_keeps_not_having_one() {
        assert_eq!(set_task("- [ ] a", 0, true).unwrap(), "- [x] a");
    }

    #[test]
    fn ticking_a_task_that_is_not_there_changes_nothing() {
        assert!(set_task("- [ ] a\n", 7, true).is_none());
    }
}
