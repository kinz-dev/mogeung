//! Doc-sprawl scanner. Pillar H (`R-H1`–`R-H5`), feature 0022.
//!
//! `scripts/check-docs.sh` and `scripts/gen-status.sh` are this module at toy
//! scale, and their choices are reused deliberately:
//!
//! - **Author dates, never committer dates.** Every rebase, cherry-pick and
//!   amend resets the committer date to now; a staleness check that cries
//!   wolf after a rebase teaches you to ignore it. So staleness compares
//!   `%at` on both sides, and "commits since" is counted by filtering author
//!   epochs in-process rather than trusting `--since` (which filters on the
//!   committer date).
//! - **Evidence, always.** Every classification and every GC proposal names
//!   the observation that produced it.
//! - **Zero write paths.** Every git call reads; every fs call reads. There
//!   is nothing here that could modify a watched repo, and nothing may be
//!   added that does (ADR-0003 applied to docs).
//!
//! Degradation over failure throughout: a git spawn that fails leaves the doc
//! listed with fs-mtime dates; a file that will not read still occupies its
//! inventory row; a path reference that does not exist simply does not count.

use mogeung_core::docs::{
    DocInfo, DocInventory, DocKind, GcAction, GcCandidate, InstructionDrift, InstructionFile,
    InstructionPair, PlanCheck, PlanItem, StaleDoc,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::git::run_git;

/// Walk cap. A repo with more markdown than this gets a truncated inventory
/// rather than an unbounded scan.
const MAX_FILES: usize = 2000;
/// Per-file read cap. Classification and extraction see at most this much;
/// the inventory row still records the true size.
const MAX_DOC_BYTES: usize = 1024 * 1024;
/// Path references consulted per doc for staleness.
const MAX_REFS_PER_DOC: usize = 50;
/// Outbound links recorded per doc.
const MAX_LINKS_PER_DOC: usize = 100;
/// Checklist items reported per doc; the checked/unchecked counts still
/// cover the whole doc.
const MAX_PLAN_ITEMS: usize = 200;
/// Author epochs fetched per path. Makes `commits_since` a floor when a path
/// churns harder than this, which the type documents.
const MAX_LOG_EPOCHS: usize = 500;
/// Drift sample lines per side of a pair.
const MAX_DRIFT_SAMPLES: usize = 20;
/// Sample lines and evidence fragments are clipped to this many chars.
const MAX_SAMPLE_CHARS: usize = 120;

// ---------------------------------------------------------------------------
// The one public entry point
// ---------------------------------------------------------------------------

/// Inventory every markdown file under `repo_root`. Never fails: a directory
/// that is not a git repo still inventories on fs dates, and an unreadable
/// file still gets a row.
pub fn scan(repo_root: &Path) -> DocInventory {
    let (rel_paths, mut truncated) = walk_markdown(repo_root);

    // Author-epoch lists per repo path, newest first. One git spawn per
    // distinct path, ever.
    let mut epochs: EpochCache = HashMap::new();

    let mut docs: Vec<DocInfo> = Vec::new();
    // Parallel to `docs`: (staleness path refs, checklist items).
    let mut extracts: Vec<(Vec<String>, Vec<(String, bool)>)> = Vec::new();

    for rel in &rel_paths {
        let abs = repo_root.join(rel);
        let bytes = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
        let raw = std::fs::read(&abs).unwrap_or_default();
        let clipped = raw.len() > MAX_DOC_BYTES;
        if clipped {
            truncated = true;
        }
        let end = raw.len().min(MAX_DOC_BYTES);
        let content = String::from_utf8_lossy(&raw[..end]);

        let (kind, evidence) = classify(rel, &content);
        let fm = frontmatter(&content);
        let title = fm.title.or_else(|| first_heading(&content));

        let (created, edited, dates_from_git) = doc_dates(repo_root, rel, &abs, &mut epochs);

        let base_dir = rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let (links_out, path_refs) = extract_references(repo_root, base_dir, rel, &content);
        let items = extract_plan_items(&content);

        docs.push(DocInfo {
            path: rel.clone(),
            kind,
            evidence,
            title,
            status: fm.status,
            updated: fm.updated,
            created_epoch: created,
            edited_epoch: edited,
            dates_from_git,
            bytes,
            links_out,
            links_in: 0,
            orphan: false,
        });
        extracts.push((path_refs, items));
    }

    // Link graph: inbound counts across the set, then the orphan flag.
    let index: HashMap<&str, usize> = docs
        .iter()
        .enumerate()
        .map(|(i, d)| (d.path.as_str(), i))
        .collect();
    let mut inbound = vec![0u32; docs.len()];
    for d in &docs {
        for target in &d.links_out {
            if target == &d.path {
                continue; // a self-link is not readership
            }
            if let Some(&i) = index.get(target.as_str()) {
                inbound[i] += 1;
            }
        }
    }
    for (i, d) in docs.iter_mut().enumerate() {
        d.links_in = inbound[i];
        d.orphan = inbound[i] == 0;
    }

    let stale = find_stale(repo_root, &docs, &extracts, &mut epochs);
    let plans = check_plans(repo_root, &docs, &extracts, &mut epochs);
    let gc = gc_candidates(&docs, &stale);
    let instruction_drift = build_drift(repo_root, &docs);

    DocInventory {
        docs,
        stale,
        gc,
        plans,
        instruction_drift,
        truncated,
        generated_at: Some(chrono::Utc::now()),
    }
}

// ---------------------------------------------------------------------------
// Walking (R-H1)
// ---------------------------------------------------------------------------

/// Every `*.md` under `root`: gitignore-aware, `.git` excluded, hidden dirs
/// included (a `.github/README.md` is still a doc), sorted, capped. The same
/// walk shape as `state::worktree_walk`.
fn walk_markdown(root: &Path) -> (Vec<String>, bool) {
    let mut files = Vec::new();
    let mut truncated = false;
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build();
    for entry in walker {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let is_md = entry
            .path()
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));
        if !is_md {
            continue;
        }
        if files.len() >= MAX_FILES {
            truncated = true;
            break;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let joined = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        files.push(joined);
    }
    files.sort();
    (files, truncated)
}

// ---------------------------------------------------------------------------
// Classification (R-H1)
// ---------------------------------------------------------------------------

/// What is this file, and — always — why do we think so. Name rules first,
/// then path rules, then content shape; the first hit wins, so the cheap
/// certain signals outrank the fuzzy ones.
pub fn classify(rel: &str, content: &str) -> (DocKind, String) {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".md").unwrap_or(&lower);
    let path_lower = rel.to_ascii_lowercase();

    if matches!(lower.as_str(), "claude.md" | "agents.md" | "gemini.md") {
        return (
            DocKind::Instruction,
            format!("{name} is an agent-instruction file name"),
        );
    }
    if lower == "readme.md" {
        return (DocKind::Readme, "named README.md".into());
    }
    if matches!(lower.as_str(), "changelog.md" | "history.md" | "news.md") {
        return (DocKind::Changelog, format!("named {name}"));
    }
    if lower == "status.md" {
        return (
            DocKind::Generated,
            "named STATUS.md — the derived-status convention".into(),
        );
    }
    if path_lower.contains("docs/decisions/") || path_lower.split('/').any(|c| c == "adr") {
        return (DocKind::Adr, "lives in a decisions/ADR directory".into());
    }
    if path_lower.contains("docs/features/")
        || path_lower.contains("docs/specs/")
        || path_lower.split('/').any(|c| c == "rfcs")
    {
        return (DocKind::Spec, "lives in a features/specs/rfcs directory".into());
    }
    if let Some((line_no, line)) = generated_marker(content) {
        return (
            DocKind::Generated,
            format!("line {line_no} says {line:?}"),
        );
    }
    if numbered_name(&lower) && adr_shape(content) {
        return (
            DocKind::Adr,
            "numbered NNNN- name with a Status/Superseded section".into(),
        );
    }
    if stem == "plan"
        || stem == "todo"
        || stem == "tasks"
        || stem.starts_with("plan-")
        || stem.starts_with("todo-")
        || stem.ends_with("-plan")
        || stem.ends_with("-todo")
    {
        return (DocKind::Plan, format!("named {name}"));
    }
    if stem == "note" || stem == "notes" || stem == "scratch" || path_lower.contains("/notes/") {
        return (DocKind::Note, format!("named {name} or lives under notes/"));
    }
    let (boxes, bullets) = checklist_stats(content);
    if boxes >= 5 || (boxes >= 3 && boxes * 2 >= bullets) {
        return (
            DocKind::Plan,
            format!("checklist-heavy: {boxes} checkbox items across {bullets} bullets"),
        );
    }
    (
        DocKind::Unknown,
        "no name, path or content-shape rule matched".into(),
    )
}

/// A "this file is derived" marker in the first 15 lines. Only the head of
/// the file counts: prose *about* generated files further down must not
/// reclassify the doc discussing them.
fn generated_marker(content: &str) -> Option<(usize, String)> {
    for (i, line) in content.lines().take(15).enumerate() {
        let l = line.to_ascii_lowercase();
        if l.contains("generated by")
            || l.contains("do not edit")
            || l.contains("auto-generated")
            || l.contains("autogenerated")
        {
            return Some((i + 1, clip(line.trim(), 80)));
        }
    }
    None
}

/// `NNNN-*.md` — three or four leading digits then a dash, the numbering
/// convention ADRs and specs share.
fn numbered_name(lower: &str) -> bool {
    let Some((head, _)) = lower.split_once('-') else {
        return false;
    };
    (3..=4).contains(&head.len()) && head.chars().all(|c| c.is_ascii_digit())
}

/// The body shape that distinguishes an ADR from an equally-numbered spec: a
/// Status section, or a superseded marker. Deliberately case-sensitive on
/// `Status:` so a lowercase frontmatter `status:` (the spec convention here)
/// does not read as ADR shape.
fn adr_shape(content: &str) -> bool {
    content.lines().any(|l| {
        let t = l.trim();
        t.starts_with("## Status") || t.starts_with("**Status") || t.starts_with("Status:")
    }) || content.contains("Superseded by")
}

/// `(checkbox items, bullet lines)` — the raw material of "checklist-heavy".
fn checklist_stats(content: &str) -> (usize, usize) {
    let mut boxes = 0;
    let mut bullets = 0;
    for line in content.lines() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) else {
            continue;
        };
        bullets += 1;
        if rest.starts_with("[ ]") || rest.starts_with("[x]") || rest.starts_with("[X]") {
            boxes += 1;
        }
    }
    (boxes, bullets)
}

// ---------------------------------------------------------------------------
// Frontmatter and headings
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Frontmatter {
    title: Option<String>,
    status: Option<String>,
    updated: Option<String>,
}

/// The three keys the doc system here cares about, parsed leniently from a
/// leading `---` block. Bounded at 40 lines: an unclosed fence must not turn
/// the whole body into frontmatter.
fn frontmatter(content: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    let mut lines = content.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return fm;
    }
    for line in lines.take(40) {
        if line.trim_end() == "---" {
            break;
        }
        let grab = |v: &str| {
            let t = v.trim().trim_matches('"').trim_matches('\'');
            (!t.is_empty()).then(|| clip(t, MAX_SAMPLE_CHARS))
        };
        if let Some(v) = line.strip_prefix("title:") {
            fm.title = grab(v);
        } else if let Some(v) = line.strip_prefix("status:") {
            fm.status = grab(v);
        } else if let Some(v) = line.strip_prefix("updated:") {
            fm.updated = grab(v);
        }
    }
    fm
}

fn first_heading(content: &str) -> Option<String> {
    content.lines().find_map(|l| {
        l.strip_prefix("# ")
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| clip(t, MAX_SAMPLE_CHARS))
    })
}

// ---------------------------------------------------------------------------
// Reference extraction
// ---------------------------------------------------------------------------

/// `(links_out, path_refs)` for one doc:
///
/// - `links_out`: `.md` targets of markdown links, resolved relative to the
///   doc's directory, existing in the repo. The link graph.
/// - `path_refs`: existing non-`.md` repo paths the doc names, from links
///   *and* inline code spans. The staleness inputs — deliberately excluding
///   doc-to-doc links, because "another doc changed after this one" is the
///   graph's business, not drift from covered code.
///
/// Only paths that actually exist count, on both lists: a mention of a path
/// that is not in the repo is prose, not a reference.
fn extract_references(
    root: &Path,
    base_dir: &str,
    self_rel: &str,
    content: &str,
) -> (Vec<String>, Vec<String>) {
    let mut links = Vec::new();
    let mut refs = Vec::new();
    let mut seen_links = HashSet::new();
    let mut seen_refs = HashSet::new();

    let mut consider = |raw: &str, base: &str| {
        let Some(resolved) = resolve_rel(base, raw) else {
            return;
        };
        if resolved == self_rel || !root.join(&resolved).exists() {
            return;
        }
        if resolved.to_ascii_lowercase().ends_with(".md") {
            if links.len() < MAX_LINKS_PER_DOC && seen_links.insert(resolved.clone()) {
                links.push(resolved);
            }
        } else if refs.len() < MAX_REFS_PER_DOC && seen_refs.insert(resolved.clone()) {
            refs.push(resolved);
        }
    };

    for target in link_targets(content) {
        consider(&target, base_dir);
    }
    for span in code_span_candidates(content) {
        // Code spans conventionally spell repo-root-relative paths.
        consider(&span, "");
    }
    (links, refs)
}

/// Targets of `](…)` markdown links, fragments stripped, web/mail schemes
/// dropped. No markdown parser: the separator is all we trust.
fn link_targets(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (idx, _) in content.match_indices("](") {
        let rest = &content[idx + 2..];
        let Some(end) = rest.find(')') else { continue };
        let target = rest[..end].split('#').next().unwrap_or("").trim();
        if target.is_empty()
            || target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("mailto:")
            || target.starts_with("//")
        {
            continue;
        }
        out.push(target.to_string());
    }
    out
}

/// Inline `` `…` `` spans that look like a path: one token, path characters
/// only, containing a `/` or a `.`. Splitting on backticks makes fenced
/// blocks yield multi-line segments, which the single-token rule discards.
fn code_span_candidates(content: &str) -> Vec<String> {
    content
        .split('`')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 200
                && !s.contains(char::is_whitespace)
                && (s.contains('/') || s.contains('.'))
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
        })
        .map(str::to_string)
        .collect()
}

/// Resolve `target` against `base_dir` (both repo-relative) into a
/// normalized repo-relative path, or `None` when it escapes the repo or is
/// not a sane path. The same lexical containment `git.rs` applies to
/// client-supplied paths: no absolutes, no `..` past the root, and nothing
/// that could ever spell a git flag — every git call here also uses `--`.
fn resolve_rel(base_dir: &str, target: &str) -> Option<String> {
    if target.is_empty()
        || target.len() > 240
        || target.starts_with('/')
        || target.contains('\\')
        || target.chars().any(|c| c.is_control())
    {
        return None;
    }
    let mut stack: Vec<&str> = if base_dir.is_empty() {
        Vec::new()
    } else {
        base_dir.split('/').collect()
    };
    for comp in target.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                stack.pop()?;
            }
            c => stack.push(c),
        }
    }
    if stack.is_empty() || stack.iter().any(|c| c.starts_with('-')) {
        return None;
    }
    Some(stack.join("/"))
}

// ---------------------------------------------------------------------------
// Git dates — author dates, degrading to fs mtime
// ---------------------------------------------------------------------------

/// Per-path author epochs, newest first. `None`: git itself failed (not a
/// repo, no git). `Some(empty)`: git answered and has never seen the path.
type EpochCache = HashMap<String, Option<Vec<i64>>>;

fn author_epochs<'c>(
    root: &Path,
    rel: &str,
    cache: &'c mut EpochCache,
) -> &'c Option<Vec<i64>> {
    if !cache.contains_key(rel) {
        let fetched = if safe_rel(rel) {
            let n = format!("-n{MAX_LOG_EPOCHS}");
            run_git(root, &["log", &n, "--format=%at", "--", rel])
                .ok()
                .map(|out| out.lines().filter_map(|l| l.trim().parse().ok()).collect())
        } else {
            None
        };
        cache.insert(rel.to_string(), fetched);
    }
    &cache[rel]
}

/// Belt-and-braces validation before a path reaches a git argv. Everything
/// here already came from our own walk or `resolve_rel`, but the git.rs rule
/// stands: validate at the boundary anyway.
fn safe_rel(rel: &str) -> bool {
    !rel.is_empty()
        && rel.len() <= 240
        && !rel.starts_with(['-', '/'])
        && !rel.split('/').any(|c| c == "..")
        && !rel.chars().any(|c| c.is_control())
}

/// `(created, edited, dates_from_git)` for one doc. Git author dates when
/// history knows the file; fs mtime for both when it does not (untracked
/// file, or no repo at all) — a fresh `PLAN.md` dropped by an agent has no
/// commits yet and still deserves dates.
fn doc_dates(
    root: &Path,
    rel: &str,
    abs: &Path,
    cache: &mut EpochCache,
) -> (Option<i64>, Option<i64>, bool) {
    if let Some(epochs) = author_epochs(root, rel, cache) {
        if let Some(&edited) = epochs.first() {
            // `--follow` so a renamed doc keeps its original birthday; if the
            // add-commit query fails, the oldest epoch in the window is the
            // honest fallback.
            let created = if safe_rel(rel) {
                run_git(
                    root,
                    &["log", "--diff-filter=A", "--follow", "-1", "--format=%at", "--", rel],
                )
                .ok()
                .and_then(|out| out.trim().parse().ok())
            } else {
                None
            }
            .or_else(|| epochs.last().copied());
            return (created, Some(edited), true);
        }
    }
    let mtime = mtime_epoch(abs);
    (mtime, mtime, false)
}

fn mtime_epoch(abs: &Path) -> Option<i64> {
    std::fs::metadata(abs)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

// ---------------------------------------------------------------------------
// Staleness (R-H2)
// ---------------------------------------------------------------------------

/// Docs whose referenced paths moved after the doc's own last edit. Only
/// docs with git-backed dates take part: comparing an fs mtime against git
/// author dates would mix clocks and manufacture false alarms.
fn find_stale(
    root: &Path,
    docs: &[DocInfo],
    extracts: &[(Vec<String>, Vec<(String, bool)>)],
    cache: &mut EpochCache,
) -> Vec<StaleDoc> {
    let mut stale = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        if !doc.dates_from_git {
            continue;
        }
        let Some(doc_edited) = doc.edited_epoch else {
            continue;
        };
        for referenced in &extracts[i].0 {
            let Some(epochs) = author_epochs(root, referenced, cache) else {
                continue;
            };
            let Some(&last) = epochs.first() else {
                continue;
            };
            if last > doc_edited {
                let commits_since =
                    epochs.iter().filter(|&&e| e > doc_edited).count() as u32;
                stale.push(StaleDoc {
                    doc: doc.path.clone(),
                    referenced_path: referenced.clone(),
                    doc_edited_epoch: doc_edited,
                    path_edited_epoch: last,
                    commits_since,
                });
            }
        }
    }
    stale
}

// ---------------------------------------------------------------------------
// Plan evidence (R-H4)
// ---------------------------------------------------------------------------

/// Checklist items with their continuation lines joined — the spec
/// convention here wraps items, and the path an item names is often on the
/// wrapped line.
fn extract_plan_items(content: &str) -> Vec<(String, bool)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        let item = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .and_then(|rest| {
                rest.strip_prefix("[ ]")
                    .map(|b| (false, b))
                    .or_else(|| rest.strip_prefix("[x]").map(|b| (true, b)))
                    .or_else(|| rest.strip_prefix("[X]").map(|b| (true, b)))
            });
        let Some((checked, body)) = item else {
            i += 1;
            continue;
        };
        let mut text = body.trim().to_string();
        let mut j = i + 1;
        while j < lines.len() {
            let l = lines[j];
            let lt = l.trim_start();
            if lt.is_empty()
                || !l.starts_with([' ', '\t'])
                || lt.starts_with("- ")
                || lt.starts_with("* ")
                || lt.starts_with('#')
            {
                break;
            }
            text.push(' ');
            text.push_str(lt.trim_end());
            j += 1;
        }
        out.push((clip(&text, 300), checked));
        i = j.max(i + 1);
    }
    out
}

/// Evidence-check every doc that carries a checklist. Honest about limits:
/// an item naming no existing repo path is `verifiable: false` and gets no
/// verdict; only a checked item whose named paths saw **no** commit since
/// the doc was created is flagged `claimed_unevidenced`.
fn check_plans(
    root: &Path,
    docs: &[DocInfo],
    extracts: &[(Vec<String>, Vec<(String, bool)>)],
    cache: &mut EpochCache,
) -> Vec<PlanCheck> {
    let mut plans = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        let raw_items = &extracts[i].1;
        if raw_items.is_empty() {
            continue;
        }
        let checked_total = raw_items.iter().filter(|(_, c)| *c).count() as u32;
        let unchecked_total = raw_items.len() as u32 - checked_total;
        let git_dates = doc.dates_from_git && doc.created_epoch.is_some();

        let mut items = Vec::new();
        for (text, checked) in raw_items.iter().take(MAX_PLAN_ITEMS) {
            // Paths named in the item itself, root-relative by convention.
            let (mut paths, mut refs) = extract_references(root, "", &doc.path, text);
            paths.append(&mut refs); // .md and non-.md both count as evidence targets
            let verifiable = !paths.is_empty() && git_dates;
            let mut claimed_unevidenced = false;
            if *checked && verifiable {
                let created = doc.created_epoch.unwrap_or(i64::MAX);
                let touched = paths.iter().any(|p| {
                    author_epochs(root, p, cache)
                        .as_ref()
                        .is_some_and(|v| v.iter().any(|&e| e > created))
                });
                claimed_unevidenced = !touched;
            }
            items.push(PlanItem {
                text: text.clone(),
                checked: *checked,
                paths,
                verifiable,
                claimed_unevidenced,
            });
        }
        plans.push(PlanCheck {
            doc: doc.path.clone(),
            checked: checked_total,
            unchecked: unchecked_total,
            items,
        });
    }
    plans
}

// ---------------------------------------------------------------------------
// GC candidates (R-H3) — proposals only, forever
// ---------------------------------------------------------------------------

/// Kinds that never become orphan GC candidates: a README is an entry point,
/// an ADR is an immutable record (check-docs.sh only *warns* on unlinked
/// ADRs), an instruction file is read by agents rather than linked, and a
/// generated file is owned by its generator.
fn gc_exempt(kind: DocKind) -> bool {
    matches!(
        kind,
        DocKind::Readme | DocKind::Adr | DocKind::Instruction | DocKind::Generated
    )
}

/// Root-level names the repo's own rule allows (`check-docs.sh` section 1,
/// generalised — plus `GEMINI.md`, an instruction file by the same logic).
fn root_allowed(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "readme.md" | "changelog.md" | "claude.md" | "agents.md" | "gemini.md" | "status.md"
    )
}

fn gc_candidates(docs: &[DocInfo], stale: &[StaleDoc]) -> Vec<GcCandidate> {
    let mut gc = Vec::new();

    // Orphans: nothing links here and the kind is not exempt.
    for d in docs.iter().filter(|d| d.orphan && !gc_exempt(d.kind)) {
        gc.push(GcCandidate {
            path: d.path.clone(),
            action: GcAction::Archive,
            evidence: "no other markdown file links to it".into(),
        });
    }

    // Stale docs: one candidate per doc, evidence naming the drift.
    let mut seen_stale = HashSet::new();
    for s in stale {
        if !seen_stale.insert(s.doc.as_str()) {
            continue;
        }
        let n = stale.iter().filter(|x| x.doc == s.doc).count();
        gc.push(GcCandidate {
            path: s.doc.clone(),
            action: GcAction::Review,
            evidence: format!(
                "{n} referenced path(s) changed after its last edit, e.g. {}",
                s.referenced_path
            ),
        });
    }

    // Near-duplicate titles: normalised match, READMEs excluded (every
    // subdirectory README legitimately re-titles its area).
    let mut by_title: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, d) in docs.iter().enumerate() {
        if d.kind == DocKind::Readme {
            continue;
        }
        if let Some(t) = &d.title {
            let norm = normalize_title(t);
            if norm.len() >= 4 {
                by_title.entry(norm).or_default().push(i);
            }
        }
    }
    let mut dup_groups: Vec<&Vec<usize>> = by_title.values().filter(|v| v.len() > 1).collect();
    dup_groups.sort_by_key(|v| v[0]);
    for group in dup_groups {
        for &i in group {
            let others: Vec<&str> = group
                .iter()
                .filter(|&&j| j != i)
                .map(|&j| docs[j].path.as_str())
                .collect();
            gc.push(GcCandidate {
                path: docs[i].path.clone(),
                action: GcAction::Merge,
                evidence: format!(
                    "title {:?} also carried by {}",
                    docs[i].title.as_deref().unwrap_or(""),
                    others.join(", ")
                ),
            });
        }
    }

    // Root strays: the repo's own "no markdown at the root" rule.
    for d in docs.iter().filter(|d| !d.path.contains('/')) {
        if !root_allowed(&d.path.to_ascii_lowercase()) {
            gc.push(GcCandidate {
                path: d.path.clone(),
                action: GcAction::Archive,
                evidence:
                    "markdown at the repo root outside README/CHANGELOG/STATUS/instruction files"
                        .into(),
            });
        }
    }
    gc
}

/// Lowercased alphanumeric words, single-spaced — enough to catch "Plan",
/// "PLAN" and "plan." colliding without fuzzy matching's false positives.
fn normalize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut space = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.extend(c.to_lowercase());
        } else {
            space = true;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Instruction drift (R-H5)
// ---------------------------------------------------------------------------

/// Compare the root-level instruction files pairwise. `None` below two —
/// one file has nothing to drift from. The data for a side-by-side; the UI
/// offers copy-out, and mogeung writes nothing.
fn build_drift(root: &Path, docs: &[DocInfo]) -> Option<InstructionDrift> {
    let roots: Vec<&DocInfo> = docs
        .iter()
        .filter(|d| d.kind == DocKind::Instruction && !d.path.contains('/'))
        .collect();
    if roots.len() < 2 {
        return None;
    }
    let contents: Vec<Vec<String>> = roots
        .iter()
        .map(|d| {
            let raw = std::fs::read(root.join(&d.path)).unwrap_or_default();
            let end = raw.len().min(MAX_DOC_BYTES);
            distinct_lines(&String::from_utf8_lossy(&raw[..end]))
        })
        .collect();
    let files = roots
        .iter()
        .zip(&contents)
        .map(|(d, lines)| InstructionFile {
            path: d.path.clone(),
            bytes: d.bytes,
            edited_epoch: d.edited_epoch,
            distinct_lines: lines.len() as u32,
        })
        .collect();
    let mut pairs = Vec::new();
    for i in 0..roots.len() {
        for j in i + 1..roots.len() {
            pairs.push(compare_pair(
                &roots[i].path,
                &contents[i],
                &roots[j].path,
                &contents[j],
            ));
        }
    }
    Some(InstructionDrift { files, pairs })
}

/// Distinct trimmed non-empty lines, in first-occurrence order — the unit of
/// instruction comparison. Order is kept so drift samples read like the file.
fn distinct_lines(content: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if !t.is_empty() && seen.insert(t.to_string()) {
            out.push(t.to_string());
        }
    }
    out
}

fn compare_pair(a: &str, a_lines: &[String], b: &str, b_lines: &[String]) -> InstructionPair {
    let set_a: HashSet<&String> = a_lines.iter().collect();
    let set_b: HashSet<&String> = b_lines.iter().collect();
    let shared = a_lines.iter().filter(|l| set_b.contains(l)).count();
    let union = set_a.len() + set_b.len() - shared;
    let only = |lines: &[String], other: &HashSet<&String>| -> (u32, Vec<String>) {
        let all: Vec<&String> = lines.iter().filter(|l| !other.contains(l)).collect();
        let sample = all
            .iter()
            .take(MAX_DRIFT_SAMPLES)
            .map(|l| clip(l, MAX_SAMPLE_CHARS))
            .collect();
        (all.len() as u32, sample)
    };
    let (only_in_a_total, only_in_a) = only(a_lines, &set_b);
    let (only_in_b_total, only_in_b) = only(b_lines, &set_a);
    InstructionPair {
        a: a.to_string(),
        b: b.to_string(),
        shared_lines: shared as u32,
        shared_ratio: if union == 0 {
            1.0
        } else {
            shared as f32 / union as f32
        },
        only_in_a_total,
        only_in_b_total,
        only_in_a,
        only_in_b,
    }
}

// ---------------------------------------------------------------------------

/// Char-boundary-safe clip, ellipsis when anything was dropped.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    /// Unique scratch root per test, the usage.rs/canary.rs pattern — no
    /// tempfile dep, tag unique per test.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("mogeung-docscan-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const T1: i64 = 1_600_000_000;
    const T2: i64 = T1 + 86_400;
    const T3: i64 = T2 + 86_400;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn init_repo(dir: &Path) {
        git(dir, &["init", "-q"]);
    }

    /// Commit everything, author *and* committer dates pinned to `epoch`.
    fn commit_at(dir: &Path, msg: &str, epoch: i64) {
        git(dir, &["add", "-A"]);
        let date = format!("{epoch} +0000");
        let out = Command::new("git")
            .args(["-c", "commit.gpgsign=false", "commit", "-q", "-m", msg])
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git commit: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    fn doc<'a>(inv: &'a DocInventory, path: &str) -> &'a DocInfo {
        inv.docs
            .iter()
            .find(|d| d.path == path)
            .unwrap_or_else(|| panic!("{path} missing from inventory"))
    }

    // -- classification: every kind, with its evidence ----------------------

    #[test]
    fn classify_every_kind_with_evidence() {
        let cases: &[(&str, &str, DocKind, &str)] = &[
            ("README.md", "# hi", DocKind::Readme, "README"),
            ("docs/README.md", "# docs", DocKind::Readme, "README"),
            ("CHANGELOG.md", "# log", DocKind::Changelog, "CHANGELOG"),
            ("CLAUDE.md", "rules", DocKind::Instruction, "instruction"),
            ("AGENTS.md", "rules", DocKind::Instruction, "instruction"),
            ("STATUS.md", "# Status", DocKind::Generated, "STATUS.md"),
            (
                "docs/OUT.md",
                "# Out\n<!-- generated by tool -->\n",
                DocKind::Generated,
                "line 2",
            ),
            (
                "docs/decisions/0003-observe.md",
                "# ADR",
                DocKind::Adr,
                "decisions",
            ),
            (
                "0007-choice.md",
                "# Choice\n\n## Status\n\nAccepted\n",
                DocKind::Adr,
                "Status",
            ),
            (
                "docs/features/0022-doc-inventory.md",
                "---\ntitle: x\n---\n",
                DocKind::Spec,
                "features",
            ),
            ("PLAN.md", "do things", DocKind::Plan, "named PLAN.md"),
            ("TODO.md", "later", DocKind::Plan, "named TODO.md"),
            (
                "docs/steps.md",
                "- [ ] a\n- [x] b\n- [ ] c\n- [ ] d\n- [x] e\n",
                DocKind::Plan,
                "checklist-heavy",
            ),
            ("NOTES.md", "stray thought", DocKind::Note, "NOTES"),
            ("docs/misc.md", "just prose", DocKind::Unknown, "no name"),
        ];
        for (path, content, want, evidence_hint) in cases {
            let (kind, evidence) = classify(path, content);
            assert_eq!(kind, *want, "{path} classified as {kind:?}");
            assert!(
                evidence.contains(evidence_hint),
                "{path}: evidence {evidence:?} should mention {evidence_hint:?}"
            );
        }
    }

    // -- lifecycle dates from real commits ----------------------------------

    #[test]
    fn dates_come_from_git_author_history() {
        let s = Scratch::new("dates");
        let dir = s.path();
        init_repo(dir);
        write(dir, "docs/a.md", "# A\nfirst\n");
        commit_at(dir, "add a", T1);
        write(dir, "docs/a.md", "# A\nsecond\n");
        commit_at(dir, "edit a", T2);

        let inv = scan(dir);
        let a = doc(&inv, "docs/a.md");
        assert!(a.dates_from_git);
        assert_eq!(a.created_epoch, Some(T1), "created is the add commit");
        assert_eq!(a.edited_epoch, Some(T2), "edited is the last commit");
        assert!(!inv.truncated);
    }

    // -- staleness fires only when the referenced path moved after the doc --

    #[test]
    fn staleness_triggers_only_after_referenced_path_moves() {
        let s = Scratch::new("stale");
        let dir = s.path();
        init_repo(dir);
        write(dir, "src/lib.rs", "fn a() {}\n");
        commit_at(dir, "code", T1);
        write(dir, "docs/design.md", "# Design\n\nCovers `src/lib.rs`.\n");
        commit_at(dir, "doc", T2);

        let inv = scan(dir);
        assert!(
            inv.stale.is_empty(),
            "doc is newer than the code it references — not stale"
        );

        write(dir, "src/lib.rs", "fn a() {}\nfn b() {}\n");
        commit_at(dir, "code moved", T3);
        let inv = scan(dir);
        assert_eq!(inv.stale.len(), 1);
        let st = &inv.stale[0];
        assert_eq!(st.doc, "docs/design.md");
        assert_eq!(st.referenced_path, "src/lib.rs");
        assert_eq!(st.doc_edited_epoch, T2);
        assert_eq!(st.path_edited_epoch, T3);
        assert_eq!(st.commits_since, 1);
        // And the stale doc surfaces as a Review GC candidate.
        assert!(inv
            .gc
            .iter()
            .any(|c| c.path == "docs/design.md"
                && c.action == GcAction::Review
                && c.evidence.contains("src/lib.rs")));
    }

    // -- orphan detection through the link graph ----------------------------

    #[test]
    fn orphans_are_docs_nothing_links_to() {
        let s = Scratch::new("orphan");
        let dir = s.path();
        init_repo(dir);
        write(dir, "docs/a.md", "# A\n\nSee [b](b.md).\n");
        write(dir, "docs/b.md", "# B\n\nno links out\n");
        commit_at(dir, "docs", T1);

        let inv = scan(dir);
        let a = doc(&inv, "docs/a.md");
        let b = doc(&inv, "docs/b.md");
        assert_eq!(a.links_out, vec!["docs/b.md".to_string()]);
        assert_eq!(b.links_in, 1);
        assert!(!b.orphan);
        assert!(a.orphan, "nothing links to a.md");
        assert!(
            inv.gc
                .iter()
                .any(|c| c.path == "docs/a.md" && c.action == GcAction::Archive),
            "orphaned non-readme doc proposed for archive"
        );
        assert!(
            !inv.gc.iter().any(|c| c.path == "docs/b.md"),
            "linked doc is no GC candidate"
        );
    }

    // -- plan evidence: checked box, no commits behind it --------------------

    #[test]
    fn checked_item_without_commits_is_claimed_unevidenced() {
        let s = Scratch::new("plan");
        let dir = s.path();
        init_repo(dir);
        write(dir, "src/thing.rs", "fn t() {}\n");
        write(
            dir,
            "docs/plan-work.md",
            "# Plan\n\n- [x] rewrite `src/thing.rs`\n- [x] think about naming\n- [ ] ship it\n",
        );
        commit_at(dir, "plan and code together", T1);

        let inv = scan(dir);
        let plan = inv
            .plans
            .iter()
            .find(|p| p.doc == "docs/plan-work.md")
            .expect("plan doc extracted");
        assert_eq!(plan.checked, 2);
        assert_eq!(plan.unchecked, 1);

        let rewrite = &plan.items[0];
        assert!(rewrite.checked && rewrite.verifiable);
        assert_eq!(rewrite.paths, vec!["src/thing.rs".to_string()]);
        assert!(
            rewrite.claimed_unevidenced,
            "no commit touched src/thing.rs after the plan was created"
        );

        let vague = &plan.items[1];
        assert!(
            !vague.verifiable && !vague.claimed_unevidenced,
            "an item naming no path gets no verdict"
        );

        // A later commit touching the path turns the claim evidenced.
        write(dir, "src/thing.rs", "fn t() {}\nfn u() {}\n");
        commit_at(dir, "actually did it", T2);
        let inv = scan(dir);
        let plan = inv.plans.iter().find(|p| p.doc == "docs/plan-work.md").unwrap();
        assert!(!plan.items[0].claimed_unevidenced);
    }

    // -- instruction drift ---------------------------------------------------

    #[test]
    fn instruction_drift_compares_root_files_pairwise() {
        let s = Scratch::new("drift");
        let dir = s.path();
        write(dir, "CLAUDE.md", "shared rule\nonly for claude\n");
        write(dir, "AGENTS.md", "shared rule\nonly for agents\n");

        let inv = scan(dir);
        let drift = inv.instruction_drift.expect("two files, drift present");
        assert_eq!(drift.files.len(), 2);
        assert_eq!(drift.pairs.len(), 1);
        let p = &drift.pairs[0];
        assert_eq!((p.a.as_str(), p.b.as_str()), ("AGENTS.md", "CLAUDE.md"));
        assert_eq!(p.shared_lines, 1);
        assert!((p.shared_ratio - 1.0 / 3.0).abs() < 1e-6, "1 shared of 3 union");
        assert_eq!(p.only_in_a, vec!["only for agents".to_string()]);
        assert_eq!(p.only_in_b, vec!["only for claude".to_string()]);
    }

    #[test]
    fn single_instruction_file_reports_no_drift() {
        let s = Scratch::new("nodrift");
        let dir = s.path();
        write(dir, "CLAUDE.md", "rules\n");
        assert!(scan(dir).instruction_drift.is_none());
    }

    // -- no git at all: fs fallback ------------------------------------------

    #[test]
    fn repo_without_git_still_inventories() {
        let s = Scratch::new("nogit");
        let dir = s.path();
        write(dir, "README.md", "# Top\n");
        write(dir, "docs/note-thing.md", "# Something\n\nSee `src/x.rs`.\n");

        let inv = scan(dir);
        assert_eq!(inv.docs.len(), 2);
        let n = doc(&inv, "docs/note-thing.md");
        assert!(!n.dates_from_git, "no git — dates are fs mtime");
        assert!(n.created_epoch.is_some() && n.edited_epoch.is_some());
        assert!(inv.stale.is_empty(), "no git history, no staleness verdicts");
    }

    // -- GC: root strays and duplicate titles --------------------------------

    #[test]
    fn gc_flags_root_strays_and_duplicate_titles() {
        let s = Scratch::new("gc");
        let dir = s.path();
        init_repo(dir);
        write(dir, "README.md", "# Repo\n\n[a](docs/a.md) [b](docs/b.md)\n");
        write(dir, "DROPPING.md", "# Agent Dropping\n");
        write(dir, "docs/a.md", "# Same Title\n");
        write(dir, "docs/b.md", "# Same title.\n");
        commit_at(dir, "all", T1);

        let inv = scan(dir);
        assert!(
            inv.gc.iter().any(|c| c.path == "DROPPING.md"
                && c.action == GcAction::Archive
                && c.evidence.contains("repo root")),
            "root stray proposed for archive: {:?}",
            inv.gc
        );
        assert!(
            !inv.gc.iter().any(|c| c.path == "README.md"),
            "README is allowed at the root"
        );
        let merges: Vec<_> = inv
            .gc
            .iter()
            .filter(|c| c.action == GcAction::Merge)
            .collect();
        assert_eq!(merges.len(), 2, "both duplicate-titled docs proposed for merge");
        assert!(merges.iter().any(|c| c.path == "docs/a.md" && c.evidence.contains("docs/b.md")));
    }

    // -- gitignored markdown stays out of the inventory ----------------------

    #[test]
    fn gitignored_files_are_not_inventoried() {
        let s = Scratch::new("ignored");
        let dir = s.path();
        init_repo(dir);
        write(dir, ".gitignore", "build/\n");
        write(dir, "docs/kept.md", "# Kept\n");
        write(dir, "build/out.md", "# Generated output\n");
        commit_at(dir, "base", T1);

        let inv = scan(dir);
        assert!(inv.docs.iter().any(|d| d.path == "docs/kept.md"));
        assert!(
            !inv.docs.iter().any(|d| d.path == "build/out.md"),
            "gitignore-aware walk skips build/"
        );
    }

    // -- pure pieces ----------------------------------------------------------

    #[test]
    fn reference_extraction_requires_existence_and_containment() {
        let s = Scratch::new("refs");
        let dir = s.path();
        write(dir, "src/lib.rs", "fn a() {}\n");
        write(dir, "docs/other.md", "# Other\n");

        let content = "See `src/lib.rs`, [other](other.md), `src/gone.rs`,\n\
                       [web](https://example.com/x.md), `../../etc/passwd`,\n\
                       and `cargo test` which is a command, not a path.\n";
        let (links, refs) = extract_references(dir, "docs", "docs/me.md", content);
        assert_eq!(links, vec!["docs/other.md".to_string()]);
        assert_eq!(refs, vec!["src/lib.rs".to_string()]);
    }

    #[test]
    fn plan_items_join_wrapped_continuation_lines() {
        let items = extract_plan_items(
            "- [x] a checked item that wraps onto the\n      next line naming `src/x.rs`\n- [ ] short\n",
        );
        assert_eq!(items.len(), 2);
        assert!(items[0].0.contains("`src/x.rs`"), "wrapped path joined: {:?}", items[0].0);
        assert!(items[0].1);
        assert!(!items[1].1);
    }
}
