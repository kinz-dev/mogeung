//! Git plumbing: worktrees, and turning a run into a risk-ordered Change.
//!
//! We shell out to `git` rather than link a library. On worktrees especially,
//! the CLI is the definition of correct behaviour, and this is not a hot path
//! at v0.1 scale. CONCEPT.md flags `gix` for later if it ever hurts.

use anyhow::{anyhow, bail, Context, Result};
use mogeung_core::change::{Change, FileChange, FileStatus, Hunk, RiskFlag};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Skip diffing any single file larger than this. Generated bundles and
/// vendored blobs otherwise drown the review queue.
const MAX_FILE_BYTES: u64 = 512 * 1024;
/// Hard cap on untracked files we will render, to keep a runaway agent from
/// producing an unusable UI.
const MAX_UNTRACKED: usize = 200;

pub fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {args:?}"))?;
    if !out.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Like `run_git`, but tolerates the exit-code-1 that `git diff` uses to mean
/// "there were differences".
fn run_git_diff(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {args:?}"))?;
    match out.status.code() {
        Some(0) | Some(1) => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
        _ => bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        ),
    }
}

pub fn is_repo(path: &Path) -> bool {
    run_git(path, &["rev-parse", "--git-dir"]).is_ok()
}

pub fn repo_root(path: &Path) -> Result<PathBuf> {
    let out = run_git(path, &["rev-parse", "--show-toplevel"])?;
    Ok(PathBuf::from(out.trim()))
}

pub fn head_sha(path: &Path) -> Result<String> {
    Ok(run_git(path, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// Directory holding all mogeung-managed worktrees.
pub fn worktree_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".mogeung").join("worktrees")
}

/// Create a dedicated worktree + branch for a run.
///
/// Worktree-per-run is what makes parallel agents safe: two runs on the same
/// repo never see each other's edits. Cost is disk and the caller's
/// responsibility to clean up (see `remove_worktree`).
pub fn add_worktree(repo: &Path, branch: &str, short_id: &str) -> Result<PathBuf> {
    let repo_name = repo
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".into());
    let dir = worktree_root().join(&repo_name).join(short_id);
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dir_s = dir.to_string_lossy().to_string();
    run_git(repo, &["worktree", "add", "-b", branch, &dir_s, "HEAD"])
        .with_context(|| format!("creating worktree at {dir_s}"))?;
    Ok(dir)
}

pub fn remove_worktree(repo: &Path, worktree: &Path) -> Result<()> {
    let s = worktree.to_string_lossy().to_string();
    run_git(repo, &["worktree", "remove", "--force", &s])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Risk heuristics
// ---------------------------------------------------------------------------

/// Flags derived from where a file lives. Cheap, and catches most of what
/// actually matters: infra, CI, migrations, secrets, dependencies.
pub fn path_flags(path: &str) -> Vec<RiskFlag> {
    let p = path.to_ascii_lowercase();
    let mut f = Vec::new();
    let has = |needle: &str| p.contains(needle);

    if has("auth") || has("login") || has("session") || has("passwd") || has("permission") {
        f.push(RiskFlag::Auth);
    }
    if has(".env") || has("secret") || has("credential") || has("token") || has(".pem") {
        f.push(RiskFlag::Secrets);
    }
    if has("migration") || has("migrate") || has("schema.") || has(".sql") {
        f.push(RiskFlag::Migration);
    }
    if has("billing") || has("payment") || has("invoice") || has("charge") || has("stripe") {
        f.push(RiskFlag::Money);
    }
    if has(".github/") || has("gitlab-ci") || has("jenkinsfile") || has(".circleci") {
        f.push(RiskFlag::CiConfig);
    }
    if has("infra/") || has("terraform") || has("dockerfile") || has("k8s") || has("helm") {
        f.push(RiskFlag::Infra);
    }

    let dep_manifest = [
        "cargo.toml",
        "package.json",
        "go.mod",
        "requirements.txt",
        "pyproject.toml",
        "gemfile",
        "pom.xml",
        "build.gradle",
    ];
    if dep_manifest.iter().any(|m| p.ends_with(m)) {
        f.push(RiskFlag::Dependency);
    }

    let noise = [
        "cargo.lock",
        "package-lock.json",
        "yarn.lock",
        "pnpm-lock.yaml",
        "poetry.lock",
        "go.sum",
        "gemfile.lock",
    ];
    let noisy_dir = p.contains("/generated/")
        || p.contains("/__snapshots__/")
        || p.contains("/fixtures/")
        || p.contains("/vendor/")
        || p.contains("/node_modules/")
        || p.ends_with(".min.js")
        || p.ends_with(".pb.go")
        || p.ends_with("_generated.go");
    if noise.iter().any(|m| p.ends_with(m)) || noisy_dir {
        f.push(RiskFlag::Noise);
    }
    f
}

fn is_test_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("/test") || p.contains("/spec") || p.starts_with("test") || {
        let base = p.rsplit('/').next().unwrap_or("");
        base.starts_with("test_")
            || base.ends_with("_test.go")
            || base.ends_with("_test.py")
            || base.ends_with(".test.ts")
            || base.ends_with(".test.js")
            || base.ends_with(".spec.ts")
            || base.ends_with("_spec.rb")
    }
}

/// Flags derived from the content of a hunk. Only added/removed lines are
/// inspected — context lines are not part of the change.
fn content_flags(path: &str, lines: &[String]) -> Vec<RiskFlag> {
    let mut f = HashSet::new();
    let mut removed_test_lines = 0u32;
    let mut removed = 0u32;
    let mut added = 0u32;

    for line in lines {
        let (is_add, is_del) = (line.starts_with('+'), line.starts_with('-'));
        if !is_add && !is_del {
            continue;
        }
        if is_add {
            added += 1;
        } else {
            removed += 1;
        }
        let body = line[1..].trim().to_ascii_lowercase();

        if body.contains("unsafe ") || body.contains("unsafe{") {
            f.insert(RiskFlag::UnsafeCode);
        }
        if body.contains("unwrap()")
            || body.contains("expect(")
            || body.contains("panic!")
            || body.contains("catch")
            || body.contains("except")
            || body.contains("rescue")
            || body.contains("try ")
        {
            f.insert(RiskFlag::ErrorHandling);
        }
        if body.contains("spawn")
            || body.contains("thread")
            || body.contains("mutex")
            || body.contains("rwlock")
            || body.contains("goroutine")
            || body.contains("async ")
            || body.contains("await ")
        {
            f.insert(RiskFlag::Concurrency);
        }
        if body.contains("http://")
            || body.contains("https://")
            || body.contains("fetch(")
            || body.contains("requests.")
            || body.contains("reqwest")
            || body.contains("curl ")
        {
            f.insert(RiskFlag::NetworkIo);
        }
        if body.contains("api_key")
            || body.contains("apikey")
            || body.contains("password")
            || body.contains("secret")
            || body.contains("bearer ")
        {
            f.insert(RiskFlag::Secrets);
        }
        if body.starts_with("pub fn")
            || body.starts_with("pub struct")
            || body.starts_with("export function")
            || body.starts_with("export class")
            || body.starts_with("public ")
        {
            f.insert(RiskFlag::PublicApi);
        }
        if is_del && is_test_path(path) {
            removed_test_lines += 1;
        }
    }

    // Deleting tests is one of the few things an agent does that is almost
    // always worth a human look, so it gets its own flag.
    if removed_test_lines >= 3 {
        f.insert(RiskFlag::DeletedTest);
    }
    if removed >= 30 && added * 3 < removed {
        f.insert(RiskFlag::LargeDeletion);
    }
    f.into_iter().collect()
}

fn score_of(flags: &[RiskFlag]) -> i32 {
    flags.iter().map(|f| f.weight()).sum()
}

/// Collapse a diff line to what a reviewer would call "the same code".
///
/// Leading indentation and runs of internal whitespace are flattened. A
/// reformat, a re-indent, or a change of tabs to spaces then produces the same
/// anchor, so hunks you have already read do not come back unread. `R-D2`.
///
/// Deliberately *not* normalised away: the `+`/`-` sign, punctuation, string
/// contents, and case. Whitespace is the only thing a formatter is guaranteed
/// to be free to move; anything more aggressive would start silently marking
/// genuinely different code as already-reviewed, which is the one failure this
/// system must never have.
fn normalize_for_anchor(line: &str) -> String {
    let sign = &line[..1];
    let body = &line[1..];
    let mut out = String::with_capacity(body.len() + 1);
    out.push_str(sign);
    let mut space = false;
    for c in body.chars() {
        if c.is_whitespace() {
            space = !out.is_empty() && out.len() > 1;
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    out
}

/// Content hash that survives the hunk moving within its file, and survives
/// reformatting.
///
/// Only the path and the added/removed lines feed the hash — not line numbers
/// and not context. That is what makes a review mark stick when the agent
/// edits elsewhere in the same file.
fn anchor_of(path: &str, lines: &[String]) -> String {
    let mut h = Sha256::new();
    h.update(path.as_bytes());
    h.update(b"\n");
    for l in lines {
        if l.starts_with('+') || l.starts_with('-') {
            h.update(normalize_for_anchor(l).as_bytes());
            h.update(b"\n");
        }
    }
    format!("{:x}", h.finalize())[..16].to_string()
}

/// Pick a diff base that includes work the session committed before we saw it.
///
/// `HEAD`-when-first-seen is wrong whenever mogeung starts *after* an agent has
/// already committed: those commits are inside the base, so the work is
/// invisible and the session looks like it did nothing. Instead, walk back to
/// the last commit made before the session started. `R-D7`.
///
/// Falls back to `HEAD` when the repo has no commit that old, which is the
/// previous behaviour and still the only safe answer.
pub fn base_for_session(cwd: &Path, started_at: chrono::DateTime<chrono::Utc>) -> Result<String> {
    let before = started_at.to_rfc3339();
    let out = run_git(
        cwd,
        &[
            "rev-list",
            "-1",
            "--before",
            &before,
            "HEAD",
        ],
    )
    .unwrap_or_default();
    let sha = out.trim();
    if sha.is_empty() {
        return head_sha(cwd);
    }
    Ok(sha.to_string())
}

// ---------------------------------------------------------------------------
// Unified diff parsing
// ---------------------------------------------------------------------------

fn parse_unified(diff: &str, reviewed: &HashSet<String>) -> Vec<FileChange> {
    let mut files: Vec<FileChange> = Vec::new();
    let mut cur: Option<FileChange> = None;
    let mut hunk: Option<Hunk> = None;

    // Close the open hunk into the open file.
    fn flush_hunk(cur: &mut Option<FileChange>, hunk: &mut Option<Hunk>, reviewed: &HashSet<String>) {
        if let (Some(f), Some(mut h)) = (cur.as_mut(), hunk.take()) {
            h.anchor = anchor_of(&f.path, &h.lines);
            h.insertions = h.lines.iter().filter(|l| l.starts_with('+')).count() as u32;
            h.deletions = h.lines.iter().filter(|l| l.starts_with('-')).count() as u32;
            let mut flags = content_flags(&f.path, &h.lines);
            // A file-level flag applies to every hunk in it.
            for pf in &f.flags {
                if !flags.contains(pf) {
                    flags.push(*pf);
                }
            }
            h.score = score_of(&flags);
            h.flags = flags;
            h.reviewed = reviewed.contains(&h.anchor);
            f.insertions += h.insertions;
            f.deletions += h.deletions;
            f.hunks.push(h);
        }
    }

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            flush_hunk(&mut cur, &mut hunk, reviewed);
            if let Some(f) = cur.take() {
                files.push(f);
            }
            // "a/path b/path" — take the b-side, tolerating spaces by splitting
            // on the " b/" separator.
            let path = rest
                .split_once(" b/")
                .map(|(_, b)| b.to_string())
                .unwrap_or_else(|| rest.to_string());
            let flags = path_flags(&path);
            cur = Some(FileChange {
                path,
                old_path: None,
                status: FileStatus::Modified,
                insertions: 0,
                deletions: 0,
                hunks: Vec::new(),
                score: score_of(&flags),
                flags,
                truncated: false,
            });
        } else if let Some(f) = cur.as_mut() {
            if line.starts_with("new file mode") {
                f.status = FileStatus::Added;
            } else if line.starts_with("deleted file mode") {
                f.status = FileStatus::Deleted;
            } else if let Some(old) = line.strip_prefix("rename from ") {
                f.status = FileStatus::Renamed;
                f.old_path = Some(old.to_string());
            } else if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
                f.truncated = true;
            } else if line.starts_with("@@") {
                flush_hunk(&mut cur, &mut hunk, reviewed);
                hunk = Some(Hunk {
                    anchor: String::new(),
                    header: line.to_string(),
                    lines: Vec::new(),
                    insertions: 0,
                    deletions: 0,
                    flags: Vec::new(),
                    score: 0,
                    reviewed: false,
                });
            } else if let Some(h) = hunk.as_mut() {
                // "\ No newline at end of file" is metadata, not content.
                if !line.starts_with('\\') {
                    h.lines.push(line.to_string());
                }
            }
        }
    }
    flush_hunk(&mut cur, &mut hunk, reviewed);
    if let Some(f) = cur.take() {
        files.push(f);
    }

    // A file scores as its riskiest hunk, so one dangerous change cannot be
    // averaged away by surrounding boilerplate.
    for f in &mut files {
        let max_hunk = f.hunks.iter().map(|h| h.score).max().unwrap_or(0);
        f.score = f.score.max(max_hunk);
    }
    files
}

/// Compute the net change a run produced, ordered for reading.
///
/// Covers three sources: committed work since `base_sha`, uncommitted edits,
/// and untracked new files (which plain `git diff` would miss entirely — and
/// which is exactly what an agent creating new modules produces).
pub fn compute_change(cwd: &Path, base_sha: Option<&str>, reviewed: &HashSet<String>) -> Change {
    match compute_change_inner(cwd, base_sha, reviewed) {
        Ok(c) => c,
        Err(e) => Change {
            error: Some(e.to_string()),
            ..Default::default()
        },
    }
}

fn compute_change_inner(
    cwd: &Path,
    base_sha: Option<&str>,
    reviewed: &HashSet<String>,
) -> Result<Change> {
    if !cwd.exists() {
        return Err(anyhow!("working directory no longer exists: {}", cwd.display()));
    }
    let base = base_sha.ok_or_else(|| anyhow!("run has no base commit recorded"))?;

    // Tracked: working tree vs the commit the run started from. This picks up
    // both committed and uncommitted work in one pass.
    let tracked = run_git_diff(
        cwd,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "-M",
            "--unified=3",
            base,
            "--",
        ],
    )?;
    let mut files = parse_unified(&tracked, reviewed);

    // Untracked: diff each against /dev/null so new files are reviewable too.
    let untracked = run_git(cwd, &["ls-files", "--others", "--exclude-standard"])?;
    let paths: Vec<&str> = untracked.lines().filter(|l| !l.trim().is_empty()).collect();
    let shown = paths.len().min(MAX_UNTRACKED);
    for rel in paths.iter().take(shown) {
        let abs = cwd.join(rel);
        let too_big = std::fs::metadata(&abs).map(|m| m.len() > MAX_FILE_BYTES).unwrap_or(false);
        if too_big {
            let flags = path_flags(rel);
            files.push(FileChange {
                path: rel.to_string(),
                old_path: None,
                status: FileStatus::Added,
                insertions: 0,
                deletions: 0,
                hunks: Vec::new(),
                score: score_of(&flags),
                flags,
                truncated: true,
            });
            continue;
        }
        let d = run_git_diff(
            cwd,
            &["diff", "--no-color", "--no-index", "--", "/dev/null", rel],
        )
        .unwrap_or_default();
        for mut f in parse_unified(&d, reviewed) {
            f.path = rel.to_string();
            f.status = FileStatus::Added;
            files.push(f);
        }
    }

    let mut change = Change {
        insertions: files.iter().map(|f| f.insertions).sum(),
        deletions: files.iter().map(|f| f.deletions).sum(),
        files,
        error: None,
    };
    if paths.len() > shown {
        change.error = Some(format!(
            "{} untracked files not shown (cap {})",
            paths.len() - shown,
            MAX_UNTRACKED
        ));
    }

    // Risk order, not alphabetical order. This is B2.
    change
        .files
        .sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
    Ok(change)
}

// ---------------------------------------------------------------------------
// The Git view (R-D10) — read-only, permanently
// ---------------------------------------------------------------------------
//
// Nothing below mutates a repository, and nothing may be added below that
// does. Staging, committing, checkout — all of it stays in the terminal;
// mogeung driving the repo is the observer trap one layer down.

use mogeung_core::wire::{BlameLine, CommitInfo, StatusEntry};

/// Blame stops here; past this a "who wrote this line" gutter is a memory
/// test. The truncated flag says so.
const MAX_BLAME_LINES: usize = 20_000;

/// A sha as an argument must look like a sha. The daemon is unauthenticated,
/// and `git show <client-supplied-string>` where the string may start with
/// `-` is how "read-only" quietly stops being true.
fn valid_sha(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// One page of the log, newest first. Asks for one row past `limit`, so
/// "was that the end of history" costs no second call.
pub fn log_page(cwd: &Path, skip: u32, limit: u32) -> Result<(Vec<CommitInfo>, bool)> {
    let limit = limit.clamp(1, 200);
    // \x1f between fields, \x1e between records: subjects contain anything.
    let out = run_git(
        cwd,
        &[
            "log",
            &format!("--skip={skip}"),
            &format!("-n{}", limit + 1),
            "--format=%H%x1f%h%x1f%an%x1f%at%x1f%s%x1e",
        ],
    )?;
    let mut commits = parse_log(&out);
    let done = commits.len() as u32 <= limit;
    commits.truncate(limit as usize);
    Ok((commits, done))
}

fn parse_log(out: &str) -> Vec<CommitInfo> {
    out.split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches(['\n', '\r']).split('\x1f');
            let sha = f.next()?.trim().to_string();
            if sha.is_empty() {
                return None;
            }
            Some(CommitInfo {
                sha,
                short: f.next()?.to_string(),
                author: f.next()?.to_string(),
                epoch: f.next()?.parse().unwrap_or(0),
                summary: f.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// One commit's patch, in the same shapes the Changes tab renders — which is
/// what lets the Git pane reuse the entire diff pipeline for free.
pub fn show_commit(cwd: &Path, sha: &str) -> Result<Vec<FileChange>> {
    if !valid_sha(sha) {
        bail!("that is not a commit sha");
    }
    let out = run_git_diff(
        cwd,
        &[
            "show", "--no-color", "--no-ext-diff", "-M", "--unified=3", "--format=", sha, "--",
        ],
    )?;
    Ok(parse_unified(&out, &HashSet::new()))
}

/// The repo's uncommitted state, porcelain v1 parsed leniently — a code we
/// do not recognise still lists, it just wears its raw `XY`.
pub fn status(cwd: &Path) -> Result<Vec<StatusEntry>> {
    let out = run_git(cwd, &["status", "--porcelain"])?;
    Ok(parse_status(&out))
}

fn parse_status(out: &str) -> Vec<StatusEntry> {
    out.lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let (code, rest) = line.split_at(2);
            let path = rest.trim_start();
            // "old -> new" for renames; the new name is the one you can open.
            let path = path.rsplit(" -> ").next().unwrap_or(path);
            let x = code.chars().next().unwrap_or(' ');
            let y = code.chars().nth(1).unwrap_or(' ');
            Some(StatusEntry {
                path: path.trim_matches('"').to_string(),
                staged: x != ' ' && x != '?',
                unstaged: y != ' ',
                state: code.to_string(),
            })
        })
        .collect()
}

/// One uncommitted file against `HEAD` — or against nothing, when it is
/// untracked and `HEAD` has never heard of it.
pub fn diff_file(cwd: &Path, rel: &str) -> Result<Vec<FileChange>> {
    let tracked = run_git(cwd, &["ls-files", "--error-unmatch", "--", rel]).is_ok();
    let out = if tracked {
        run_git_diff(
            cwd,
            &["diff", "--no-color", "--no-ext-diff", "-M", "--unified=3", "HEAD", "--", rel],
        )?
    } else {
        run_git_diff(cwd, &["diff", "--no-color", "--no-index", "--", "/dev/null", rel])
            .unwrap_or_default()
    };
    let mut files = parse_unified(&out, &HashSet::new());
    if !tracked {
        for f in &mut files {
            f.path = rel.to_string();
            f.status = FileStatus::Added;
        }
    }
    Ok(files)
}

/// Per-line authorship of the worktree file. Uncommitted lines come back
/// with git's all-zeros sha and its "Not Committed Yet" author, unchanged —
/// renaming them is the client's editorial decision, not the daemon's.
pub fn blame(cwd: &Path, rel: &str) -> Result<(Vec<BlameLine>, bool)> {
    let out = run_git(cwd, &["blame", "--porcelain", "--", rel])?;
    Ok(parse_blame(&out))
}

fn parse_blame(out: &str) -> (Vec<BlameLine>, bool) {
    // Porcelain prints full commit details once per commit; later lines of
    // the same commit carry only the header. Remember what each sha said.
    let mut known: std::collections::HashMap<String, (String, i64)> =
        std::collections::HashMap::new();
    let mut lines: Vec<BlameLine> = Vec::new();
    let mut cur: Option<String> = None;
    let mut truncated = false;

    for line in out.lines() {
        if line.starts_with('\t') {
            // The content line ends one blamed line.
            if let Some(sha) = cur.take() {
                if lines.len() >= MAX_BLAME_LINES {
                    truncated = true;
                    break;
                }
                let (author, epoch) = known.get(&sha).cloned().unwrap_or_default();
                lines.push(BlameLine {
                    sha: sha.chars().take(8).collect(),
                    author,
                    epoch,
                });
            }
            continue;
        }
        // "<40-hex> orig final [count]" opens a line's record.
        let first = line.split(' ').next().unwrap_or("");
        if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            cur = Some(first.to_string());
            known.entry(first.to_string()).or_default();
        } else if let Some(a) = line.strip_prefix("author ") {
            if let Some(sha) = &cur {
                known.entry(sha.clone()).or_default().0 = a.to_string();
            }
        } else if let Some(t) = line.strip_prefix("author-time ") {
            if let Some(sha) = &cur {
                known.entry(sha.clone()).or_default().1 = t.trim().parse().unwrap_or(0);
            }
        }
    }
    (lines, truncated)
}

// ---------------------------------------------------------------------------
// Blast radius (R-D9)
// ---------------------------------------------------------------------------

/// Cap on grep hits, so a change to a name like `new` cannot produce a
/// thousand-row table nobody reads.
const MAX_REFERENCES: usize = 60;

/// Pull probable symbol names out of a hunk's added and removed lines.
///
/// Pattern-matching on declaration keywords across the handful of languages we
/// see most. It is not a parser and will miss plenty; every symbol it *does*
/// find is one the reviewer can act on, and a miss costs nothing beyond a
/// smaller table.
pub fn symbols_in(lines: &[String]) -> Vec<String> {
    const DECL: &[&str] = &[
        "fn ", "func ", "def ", "class ", "struct ", "enum ", "trait ", "interface ",
        "type ", "impl ", "function ",
    ];
    let mut out: Vec<String> = Vec::new();

    for line in lines {
        if !(line.starts_with('+') || line.starts_with('-')) {
            continue;
        }
        let body = line[1..].trim_start();
        // Strip visibility and async/export noise before matching.
        let body = body
            .trim_start_matches("pub ")
            .trim_start_matches("export ")
            .trim_start_matches("default ")
            .trim_start_matches("async ")
            .trim_start_matches("static ")
            .trim_start_matches("const ");

        for kw in DECL {
            let Some(rest) = body.strip_prefix(kw) else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            // Single letters are almost always generics or noise.
            if name.len() >= 3 && !out.contains(&name) {
                out.push(name);
            }
            break;
        }
    }
    out.sort();
    out.truncate(12);
    out
}

/// Find where the given symbols are referenced, excluding their own file.
///
/// **Textual, not semantic.** `git grep -w` over tracked files only, so it
/// respects `.gitignore` and never wanders into `node_modules`.
pub fn find_references(
    repo: &Path,
    symbols: &[String],
    exclude_path: &str,
) -> (Vec<mogeung_core::review::Reference>, bool) {
    let mut refs = Vec::new();
    let mut truncated = false;

    for sym in symbols {
        if refs.len() >= MAX_REFERENCES {
            truncated = true;
            break;
        }
        let out = run_git_diff(repo, &["grep", "-n", "-w", "--", sym]).unwrap_or_default();
        for line in out.lines() {
            // "path:line:text"
            let mut parts = line.splitn(3, ':');
            let (Some(path), Some(num), Some(text)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            if path == exclude_path {
                continue;
            }
            if refs.len() >= MAX_REFERENCES {
                truncated = true;
                break;
            }
            refs.push(mogeung_core::review::Reference {
                path: path.to_string(),
                line: num.parse().unwrap_or(0),
                text: text.trim().chars().take(160).collect(),
                symbol: sym.clone(),
                is_test: is_test_path(path),
            });
        }
    }
    (refs, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
diff --git a/src/auth.rs b/src/auth.rs
index 111..222 100644
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -1,3 +1,4 @@
 fn a() {}
+let password = \"hunter2\";
 fn b() {}
diff --git a/Cargo.lock b/Cargo.lock
index 333..444 100644
--- a/Cargo.lock
+++ b/Cargo.lock
@@ -1,2 +1,3 @@
 x
+y
";

    #[test]
    fn parses_files_and_hunks() {
        let files = parse_unified(SAMPLE, &HashSet::new());
        assert_eq!(files.len(), 2);
        let auth = files.iter().find(|f| f.path == "src/auth.rs").unwrap();
        assert_eq!(auth.hunks.len(), 1);
        assert_eq!(auth.hunks[0].insertions, 1);
        assert_eq!(auth.hunks[0].deletions, 0);
    }

    #[test]
    fn risk_puts_secrets_above_lockfiles() {
        let files = parse_unified(SAMPLE, &HashSet::new());
        let auth = files.iter().find(|f| f.path == "src/auth.rs").unwrap();
        let lock = files.iter().find(|f| f.path == "Cargo.lock").unwrap();
        assert!(auth.score > lock.score);
        assert!(lock.flags.contains(&RiskFlag::Noise));
        assert!(auth.flags.contains(&RiskFlag::Auth));
    }

    #[test]
    fn anchor_ignores_line_numbers_and_context() {
        // Same added line, different position and different surrounding context.
        let a = parse_unified(SAMPLE, &HashSet::new());
        let moved = SAMPLE.replace("@@ -1,3 +1,4 @@", "@@ -80,3 +90,4 @@").replace(" fn b() {}", " fn zzz() {}");
        let b = parse_unified(&moved, &HashSet::new());
        assert_eq!(a[0].hunks[0].anchor, b[0].hunks[0].anchor);
    }

    #[test]
    fn rewriting_a_hunk_changes_its_anchor() {
        let a = parse_unified(SAMPLE, &HashSet::new());
        let edited = SAMPLE.replace("hunter2", "hunter3");
        let b = parse_unified(&edited, &HashSet::new());
        assert_ne!(a[0].hunks[0].anchor, b[0].hunks[0].anchor);
    }

    #[test]
    fn reviewed_marks_are_applied_by_anchor() {
        let first = parse_unified(SAMPLE, &HashSet::new());
        let anchor = first[0].hunks[0].anchor.clone();
        let seen: HashSet<String> = [anchor].into_iter().collect();
        let again = parse_unified(SAMPLE, &seen);
        assert!(again[0].hunks[0].reviewed);
        assert!(!again[1].hunks[0].reviewed);
    }

    /// R-D2. Reformatting is the single most common cause of a hunk you have
    /// already read coming back unread, and it carries no new information.
    #[test]
    fn reindenting_does_not_make_a_hunk_unread() {
        let a = parse_unified(SAMPLE, &HashSet::new());
        let reindented = SAMPLE.replace(
            "+let password = \"hunter2\";",
            "+        let password   =  \"hunter2\";",
        );
        let b = parse_unified(&reindented, &HashSet::new());
        assert_eq!(
            a[0].hunks[0].anchor, b[0].hunks[0].anchor,
            "whitespace-only change produced a different anchor"
        );
    }

    /// The other half of R-D2, and the more important one: normalisation must
    /// not go so far that different code collides. A collision would silently
    /// mark unread code as reviewed.
    #[test]
    fn normalisation_stops_at_whitespace() {
        let base = parse_unified(SAMPLE, &HashSet::new());
        for changed in [
            SAMPLE.replace("hunter2", "hunter3"),          // string contents
            SAMPLE.replace("let password", "let Password"), // case
            SAMPLE.replace("+let password", "-let password"), // sign
        ] {
            let other = parse_unified(&changed, &HashSet::new());
            assert_ne!(
                base[0].hunks[0].anchor, other[0].hunks[0].anchor,
                "anchor collided on a change that is not whitespace"
            );
        }
    }

    #[test]
    fn symbols_are_pulled_from_declarations() {
        let lines: Vec<String> = vec![
            "+pub fn compute_change(x: u8) {}".into(),
            "+    def handle_request(self):".into(),
            "-export function renderQueue() {".into(),
            "+struct Session {".into(),
            " fn untouched_context() {}".into(), // context line, not a change
            "+let x = 1;".into(),                // not a declaration
            "+fn ab() {}".into(),                // too short to be useful
        ];
        let got = symbols_in(&lines);
        assert!(got.contains(&"compute_change".to_string()));
        assert!(got.contains(&"handle_request".to_string()));
        assert!(got.contains(&"renderQueue".to_string()));
        assert!(got.contains(&"Session".to_string()));
        assert!(!got.contains(&"untouched_context".to_string()));
        assert!(!got.contains(&"ab".to_string()));
    }

    /// Field separators over format guessing: a subject line can contain
    /// anything printable, so only the \x1f/\x1e framing is trustworthy.
    #[test]
    fn log_parsing_survives_hostile_subjects() {
        let out = "aaa111\x1faaa\x1fkeith\x1f1722000000\x1ffix: a \"quoted\" thing\x1e\n\
                   bbb222\x1fbbb\x1fclaude\x1f1722000100\x1fsubject with \x7f and spaces\x1e";
        let commits = parse_log(out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].short, "aaa");
        assert_eq!(commits[0].epoch, 1722000000);
        assert!(commits[0].summary.contains("quoted"));
        assert!(parse_log("").is_empty());
        assert!(parse_log("garbage with no separators").len() <= 1);
    }

    /// The porcelain codes that actually occur, including the rename arrow
    /// and the quoted-path case.
    #[test]
    fn status_parsing_reads_the_common_codes() {
        let out = " M crates/a.rs\nM  crates/b.rs\nMM crates/c.rs\n?? new.txt\nR  old.rs -> new.rs\n";
        let entries = parse_status(out);
        assert_eq!(entries.len(), 5);
        let by = |p: &str| entries.iter().find(|e| e.path == p).unwrap();
        assert!(!by("crates/a.rs").staged);
        assert!(by("crates/a.rs").unstaged);
        assert!(by("crates/b.rs").staged);
        assert!(!by("crates/b.rs").unstaged);
        assert!(by("crates/c.rs").staged && by("crates/c.rs").unstaged);
        assert!(!by("new.txt").staged, "untracked is not staged");
        assert!(by("new.txt").unstaged);
        assert_eq!(by("new.rs").state, "R ", "a rename lists under its new name");
    }

    /// Porcelain blame repeats a commit's details only once; every later
    /// line of the same commit must still get the remembered author.
    #[test]
    fn blame_parsing_fills_in_repeated_commits() {
        let out = "\
aaaa111122223333aaaa111122223333aaaa1111 1 1 2
author keith
author-time 1722000000
\tfn one() {}
aaaa111122223333aaaa111122223333aaaa1111 2 2
\tfn two() {}
0000000000000000000000000000000000000000 3 3 1
author Not Committed Yet
author-time 1722000200
\tfn three() {}
";
        let (lines, truncated) = parse_blame(out);
        assert!(!truncated);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].author, "keith");
        assert_eq!(lines[1].author, "keith", "the repeat must inherit the details");
        assert_eq!(lines[1].sha, lines[0].sha);
        assert_eq!(lines[2].author, "Not Committed Yet");
        assert!(lines[2].sha.chars().all(|c| c == '0'));
        assert_eq!(parse_blame("").0.len(), 0, "an empty file blames to nothing");
    }

    /// The read-only guarantee starts at argument hygiene: a "sha" that
    /// could be parsed as a flag must be refused before git ever sees it.
    #[test]
    fn shas_that_are_not_shas_are_refused() {
        assert!(valid_sha("aaa111"));
        assert!(valid_sha(&"a".repeat(40)));
        assert!(!valid_sha(""));
        assert!(!valid_sha("--output=/tmp/x"));
        assert!(!valid_sha("HEAD"));
        assert!(!valid_sha(&"a".repeat(41)));
    }

    #[test]
    fn deleting_tests_is_flagged() {
        let diff = "\
diff --git a/tests/foo_test.go b/tests/foo_test.go
--- a/tests/foo_test.go
+++ b/tests/foo_test.go
@@ -1,5 +1,1 @@
-func TestA(t *testing.T) {}
-func TestB(t *testing.T) {}
-func TestC(t *testing.T) {}
 x
";
        let files = parse_unified(diff, &HashSet::new());
        assert!(files[0].hunks[0].flags.contains(&RiskFlag::DeletedTest));
    }
}
