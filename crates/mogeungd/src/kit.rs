//! Finding the memory and skills under `~/.claude`. `R-F14`, `R-F15`.
//!
//! The shapes and the parsing live in `mogeung_core::kit`; this is the half
//! that touches the filesystem — where to look, and what a request for a body
//! is allowed to open.
//!
//! **Read only**, like everything else that touches `~/.claude`.

use mogeung_core::kit::{decode_project, frontmatter, KitDoc, KitEntry, KitKind, KitScope, MAX_BODY};
use std::path::{Path, PathBuf};

/// How many entries a scan will return, so a marketplace with a thousand
/// plugins cannot turn one click into a message nobody can render.
const MAX_ENTRIES: usize = 2000;

/// How deep a walk goes looking for `SKILL.md`.
///
/// Measured rather than guessed, and the measurement is why this is not 6: a
/// plugin skill on this machine sits at
/// `plugins/marketplaces/<market>/plugins/<plugin>/skills/<skill>/SKILL.md`,
/// which is **seven** levels below the root. A cap of six found the six user
/// skills and silently none of the forty-four plugin ones — a list that looks
/// complete and is not, which is the failure mode this project spends a health
/// panel on. Eight leaves a level of headroom for a marketplace that nests one
/// deeper, and still refuses to walk a checkout to the bottom.
const SKILL_DEPTH: usize = 8;

/// Every root a skill can live under, with the scope it implies.
fn skill_roots(home: &Path) -> Vec<(PathBuf, KitScope)> {
    vec![
        (home.join("skills"), KitScope::User),
        // Plugins keep theirs under their own tree; the layout below the root
        // varies by marketplace, so this walks rather than assuming a depth.
        (home.join("plugins"), KitScope::Plugin),
    ]
}

fn memory_root(home: &Path) -> PathBuf {
    home.join("projects")
}

/// Every memory and skill on this machine, without bodies.
///
/// Sorted by kind and then by name so the list is stable between scans: a view
/// that reorders itself when a file's mtime changes is one you cannot keep your
/// place in.
pub fn scan(home: &Path) -> Vec<KitEntry> {
    let mut out = Vec::new();
    for (root, scope) in skill_roots(home) {
        collect_skills(&root, scope, &mut out);
    }
    collect_memories(&memory_root(home), &mut out);
    out.sort_by(|a, b| {
        (a.kind as u8, &a.name, &a.path)
            .partial_cmp(&(b.kind as u8, &b.name, &b.path))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(MAX_ENTRIES);
    out
}

/// Walk for `SKILL.md`, at whatever depth it turns up.
///
/// Bounded rather than unbounded: `~/.claude/plugins` holds caches and
/// checkouts, and a scan that follows every directory to the bottom is a scan
/// that reads a `node_modules` on the way past.
fn collect_skills(root: &Path, scope: KitScope, out: &mut Vec<KitEntry>) {
    walk(root, SKILL_DEPTH, &mut |path| {
        if path.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
            return;
        }
        // The skill's name is its directory — `skills/address-jira/SKILL.md` —
        // which is also what Claude Code calls it.
        let fallback = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("skill")
            .to_string();
        if let Some(entry) = read_entry(path, KitKind::Skill, scope, fallback, None) {
            out.push(entry);
        }
    });
}

/// `~/.claude/projects/<slug>/memory/*.md`.
fn collect_memories(root: &Path, out: &mut Vec<KitEntry>) {
    let Ok(projects) = std::fs::read_dir(root) else {
        return;
    };
    for project in projects.flatten() {
        let slug = project.file_name().to_string_lossy().to_string();
        let dir = project.path().join("memory");
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let fallback = path
                .file_stem()
                .and_then(|n| n.to_str())
                .unwrap_or("memory")
                .to_string();
            if let Some(entry) = read_entry(
                &path,
                KitKind::Memory,
                KitScope::Project,
                fallback,
                Some(decode_project(&slug)),
            ) {
                out.push(entry);
            }
        }
    }
}

/// Read one file's metadata. `None` only when the file cannot be read at all.
fn read_entry(
    path: &Path,
    kind: KitKind,
    scope: KitScope,
    fallback_name: String,
    project: Option<String>,
) -> Option<KitEntry> {
    let meta = std::fs::metadata(path).ok()?;
    // The head is enough for frontmatter, and reading 4 KiB of a file we are
    // only listing keeps a scan cheap when there are hundreds.
    let head = read_head(path, 4096);
    let (name, description) = frontmatter(&head);
    Some(KitEntry {
        kind,
        scope,
        name: name.unwrap_or(fallback_name),
        description: description.unwrap_or_default(),
        project,
        path: path.to_string_lossy().to_string(),
        bytes: meta.len(),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
    })
}

fn read_head(path: &Path, cap: usize) -> String {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut buf = vec![0u8; cap];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    // Lossy: a cap can land mid-character, and a listing must not fail for it.
    String::from_utf8_lossy(&buf).to_string()
}

/// One file's text, if the path is one we published.
///
/// **The guard is the point.** This takes a path from the network — the daemon
/// binds loopback with no token, and beyond loopback with one (`R-I10`) — so
/// without it, "show me a skill" is "read any file this user can read". The
/// path is canonicalised first, which resolves `..` and follows symlinks, and
/// then has to still be under a root we scan. The same rule the file explorer
/// uses for a session's worktree, for the same reason.
pub fn read_doc(home: &Path, requested: &str) -> Result<KitDoc, String> {
    let path = std::fs::canonicalize(requested).map_err(|e| format!("{requested}: {e}"))?;
    let mut roots: Vec<PathBuf> = skill_roots(home).into_iter().map(|(p, _)| p).collect();
    roots.push(memory_root(home));
    let allowed = roots.iter().any(|root| {
        std::fs::canonicalize(root)
            .map(|r| path.starts_with(r))
            .unwrap_or(false)
    });
    if !allowed {
        return Err("that path is not a memory or a skill".into());
    }
    let raw = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let truncated = raw.len() > MAX_BODY;
    let cut = if truncated { &raw[..MAX_BODY] } else { &raw[..] };
    Ok(KitDoc {
        path: path.to_string_lossy().to_string(),
        body: String::from_utf8_lossy(cut).to_string(),
        truncated,
    })
}

/// Depth-limited walk, files only, symlinked directories not followed.
fn walk(dir: &Path, depth: usize, f: &mut impl FnMut(&Path)) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `file_type` rather than `metadata`: it does not follow the link, so a
        // symlink loop under `plugins` cannot walk for ever.
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(&path, depth - 1, f);
        } else if ft.is_file() {
            f(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn home() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mog-kit-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        write(
            &dir.join("skills/address-jira/SKILL.md"),
            "---\nname: address-jira\ndescription: Take a Jira issue\n---\n\n# body\n",
        );
        write(&dir.join("plugins/marketplaces/slack/skills/standup/SKILL.md"), "---\nname: standup\n---\n");
        write(
            &dir.join("projects/-home-kinz-projects-mogeung/memory/work-on-main.md"),
            "---\nname: work-on-main-directly\ndescription: \"commit straight to main\"\nmetadata:\n  type: feedback\n---\n\nbody\n",
        );
        write(&dir.join("projects/-home-kinz-projects-mogeung/memory/notes.txt"), "not markdown");
        dir
    }

    #[test]
    fn finds_skills_at_both_scopes_and_the_memories() {
        let dir = home();
        let found = scan(&dir);
        let skills: Vec<_> = found.iter().filter(|e| e.kind == KitKind::Skill).collect();
        let memories: Vec<_> = found.iter().filter(|e| e.kind == KitKind::Memory).collect();

        assert_eq!(skills.len(), 2, "user and plugin skills both count");
        assert!(skills.iter().any(|s| s.scope == KitScope::Plugin && s.name == "standup"));
        assert_eq!(memories.len(), 1, "only .md is a memory");
        assert_eq!(memories[0].name, "work-on-main-directly");
        assert_eq!(memories[0].description, "commit straight to main");
        assert_eq!(memories[0].project.as_deref(), Some("/home/kinz/projects/mogeung"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The bug this depth was measured for.
    ///
    /// A plugin skill sits seven levels under `plugins/`, and the first cap of
    /// six found every user skill and **none** of the forty-four plugin ones.
    /// A list that looks complete and is not is the exact failure this project
    /// keeps a health panel for, so the depth is pinned by a test rather than
    /// left as a number somebody once picked.
    #[test]
    fn finds_a_plugin_skill_at_its_real_depth() {
        let dir = home();
        write(
            &dir.join("plugins/marketplaces/official/plugins/frontend/skills/frontend-design/SKILL.md"),
            "---\nname: frontend-design\n---\n",
        );
        let found = scan(&dir);
        assert!(
            found.iter().any(|e| e.name == "frontend-design" && e.scope == KitScope::Plugin),
            "a skill seven levels down must still be found",
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A skill with no frontmatter is still a skill, named after its directory
    /// — which is what Claude Code calls it anyway.
    #[test]
    fn a_file_with_no_frontmatter_still_appears() {
        let dir = home();
        write(&dir.join("skills/bare/SKILL.md"), "# just a body\n");
        let found = scan(&dir);
        assert!(found.iter().any(|e| e.name == "bare" && e.description.is_empty()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_a_body_under_a_root() {
        let dir = home();
        let path = dir.join("skills/address-jira/SKILL.md");
        let doc = read_doc(&dir, path.to_str().unwrap()).expect("a skill under a root");
        assert!(doc.body.contains("# body"));
        assert!(!doc.truncated);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The guard, and the reason this endpoint exists in this shape: a path
    /// arrives over a socket that has no token on loopback. Without the check,
    /// "show me a skill" is "read any file I can read".
    #[test]
    fn refuses_a_path_outside_the_roots() {
        let dir = home();
        let outside = dir.join("secret.txt");
        write(&outside, "not yours");
        assert!(read_doc(&dir, outside.to_str().unwrap()).is_err());
        // ...including one that walks out and back in.
        let sneaky = dir.join("skills/../secret.txt");
        assert!(read_doc(&dir, sneaky.to_str().unwrap()).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_home_scans_to_nothing_rather_than_failing() {
        assert!(scan(Path::new("/nonexistent-mogeung-home")).is_empty());
    }
}
