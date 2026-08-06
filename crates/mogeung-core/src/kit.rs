//! What Claude Code has been *told* — the memory it saved and the skills it can
//! run. `R-F14`, `R-F15`.
//!
//! Everything else this daemon reads is a record of what an agent *did*:
//! transcripts, diffs, registries. This is the other half, and it is the half
//! you edit. A skill is instructions Claude Code will follow next time; a
//! memory is something it decided to remember about you or a project. Both are
//! markdown files under `~/.claude`, both are easy to accumulate and hard to
//! review, and Claude Code shows you their *names* while you are trying to work
//! out what they say.
//!
//! **Read only, and it matters more here than anywhere else.** These files
//! change what an agent does. `CLAUDE.md` says never write to `~/.claude`, and
//! a view that could edit a skill would be a view that could change every
//! session on the machine from a loopback port with no token
//! ([ADR-0001](../../../docs/decisions/0001-daemon-owns-state.md)).
//!
//! The formats are undocumented, like every other format here: frontmatter that
//! may be absent, keys that may be missing, a body that may be empty. Nothing
//! in this module returns an error for a file it cannot understand — it returns
//! what it could read and lets the entry stand on its filename.

use serde::{Deserialize, Serialize};

/// Which kind of instruction a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KitKind {
    /// A file Claude Code wrote to remember something. `memory/*.md`.
    Memory,
    /// A `SKILL.md` — instructions with a name and a trigger.
    Skill,
}

/// Where a file came from, which is what decides who it applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KitScope {
    /// `~/.claude/skills` — yours, everywhere.
    User,
    /// Installed with a plugin. Yours to read, theirs to update.
    Plugin,
    /// Written against one project, and only loaded there.
    Project,
}

/// One memory or skill, without its body.
///
/// The body is fetched per file rather than sent with the list: this machine
/// has fifty skills and sixty-three memories, and the honest reason is that the
/// number only grows — a list that carries every body is a list that gets
/// slower the longer you use the tool it describes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitEntry {
    pub kind: KitKind,
    pub scope: KitScope,
    /// From the frontmatter when it has one, else the filename or directory.
    pub name: String,
    /// The frontmatter's `description`, empty when there is none.
    pub description: String,
    /// For a memory, the project it belongs to, decoded back to a path.
    pub project: Option<String>,
    /// Absolute, and the only thing the body request accepts.
    pub path: String,
    pub bytes: u64,
    /// Epoch seconds, or `None` when the filesystem would not say.
    pub modified: Option<i64>,
}

/// A file's text, as far as it was read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitDoc {
    pub path: String,
    pub body: String,
    /// The file was longer than `MAX_BODY` and this is the front of it. Said
    /// out loud rather than left for you to notice at the bottom.
    pub truncated: bool,
}

/// How much of one file is sent. Generous — these are documents people read —
/// and finite, because nothing stops one being a megabyte.
pub const MAX_BODY: usize = 256 * 1024;

/// `name` and `description` out of YAML frontmatter, if there is any.
///
/// A deliberately small parser rather than a YAML dependency: the two keys are
/// scalars at the top level, and the alternative is taking on a parser to read
/// two strings out of a file whose shape nobody promised us. Anything it does
/// not understand — no frontmatter, a missing key, a nested block, a quoted
/// value — comes back as `None` rather than as an error.
pub fn frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let (mut name, mut description) = (None, None);
    for line in lines {
        let trimmed = line.trim_end();
        if trimmed.trim() == "---" {
            break;
        }
        // Only top-level keys: an indented line belongs to a nested block
        // (`metadata:` in a memory file) and its `name:` is not this file's.
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = unquote(value.trim());
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => {}
        }
    }
    (name, description)
}

fn unquote(v: &str) -> String {
    let bytes = v.as_bytes();
    if bytes.len() >= 2 && (v.starts_with('"') && v.ends_with('"') || v.starts_with('\'') && v.ends_with('\'')) {
        return v[1..v.len() - 1].to_string();
    }
    v.to_string()
}

/// Turn `~/.claude/projects/-home-kinz-projects-mogeung` back into a path.
///
/// Claude Code encodes a project directory by replacing every separator with a
/// dash, which is **lossy**: a directory whose own name contains a dash is
/// indistinguishable from a separator. So this is presentation only — it is
/// never resolved, opened, or compared against a real path — and a name that
/// decodes wrongly is a label that reads oddly rather than a file that opens
/// somewhere it should not.
pub fn decode_project(slug: &str) -> String {
    if let Some(rest) = slug.strip_prefix('-') {
        format!("/{}", rest.replace('-', "/"))
    } else {
        slug.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_two_keys_it_wants() {
        let (name, desc) = frontmatter("---\nname: address-jira\ndescription: Take a Jira issue\n---\n\n# body");
        assert_eq!(name.as_deref(), Some("address-jira"));
        assert_eq!(desc.as_deref(), Some("Take a Jira issue"));
    }

    #[test]
    fn unquotes_a_quoted_description() {
        let (_, desc) = frontmatter("---\nname: n\ndescription: \"In mogeung, commit straight to main\"\n---\n");
        assert_eq!(desc.as_deref(), Some("In mogeung, commit straight to main"));
    }

    /// A memory file's `metadata:` block has its own `type:` and, in some
    /// shapes, its own `name:`. Reading a nested key as the file's own would
    /// label every memory after whatever the block happened to contain.
    #[test]
    fn ignores_keys_inside_a_nested_block() {
        let text = "---\nname: real-name\ndescription: d\nmetadata:\n  name: not-this\n  type: feedback\n---\n";
        let (name, _) = frontmatter(text);
        assert_eq!(name.as_deref(), Some("real-name"));
    }

    /// Degrade, never panic — the rule every parser here follows, because the
    /// format is undocumented and changes without warning.
    #[test]
    fn a_file_with_nothing_to_read_says_so_rather_than_failing() {
        assert_eq!(frontmatter(""), (None, None));
        assert_eq!(frontmatter("# just a heading\n"), (None, None));
        assert_eq!(frontmatter("---\n"), (None, None));
        assert_eq!(frontmatter("---\nname:\n---\n"), (None, None));
        // An unterminated block stops at the end of the file rather than
        // running off it.
        assert_eq!(frontmatter("---\nname: x\n").0.as_deref(), Some("x"));
    }

    #[test]
    fn decodes_a_project_slug_for_reading() {
        assert_eq!(decode_project("-home-kinz-projects-mogeung"), "/home/kinz/projects/mogeung");
        assert_eq!(decode_project("plain"), "plain");
    }

    /// The encoding is lossy and this documents which way it fails: a real
    /// directory named `perf-test` comes back as two segments. It is a label,
    /// never a path to open, and this test exists so that stays deliberate.
    #[test]
    fn decoding_is_lossy_and_only_ever_a_label() {
        assert_eq!(decode_project("-home-kinz-perf-test"), "/home/kinz/perf/test");
    }
}
