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

/// Run a git command that **changes** the repository. `R-D19`.
///
/// A sibling to [`run_git`] rather than a flag on it, because the two want
/// opposite postures and mixing them is how a write comes to fail quietly.
/// Everything else in this file reads, and a read that fails degrades: an
/// empty log, no blame, a diff we could not compute. That is right for reading
/// — the pane stays usable and the health panel says what is missing.
///
/// It is wrong for writing. A stage that silently did nothing looks exactly
/// like a stage that worked until you commit and find the file absent, so
/// these fail loudly, and they fail in **git's own words**: the error is what
/// git wrote, verbatim, not a paraphrase wrapped around it. stderr when there
/// is any, and stdout otherwise — git splits refusals across both streams, and
/// the commonest one of all ("nothing to commit") arrives on stdout. `git` already writes
/// the best available sentence about why it refused, and every layer that
/// rewrites one makes it worse — "cannot switch branch" against git's own
/// "Your local changes to the following files would be overwritten by
/// checkout:" plus the list.
///
/// See [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md)
/// for what may be written at all: the working tree and the local repository,
/// never a remote.
pub fn run_git_write(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        // No stdin, deliberately. `commit` runs the repository's hooks and may
        // reach for a GPG passphrase, and either can decide to prompt. A
        // daemon has no terminal to prompt on, so an inherited stdin means a
        // thread blocked for ever on a question nobody will ever see. With
        // `/dev/null` the prompt gets EOF, git fails, and its complaint
        // reaches the window — slow and wrong beats silent and stuck.
        .stdin(std::process::Stdio::null())
        .current_dir(cwd)
        .output()
        .with_context(|| format!("failed to run git {args:?}"))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // stdout as the fallback, because git does not keep refusals on one
        // stream. `commit` with nothing staged exits 1 and writes "nothing to
        // commit, working tree clean" to *stdout* — which is the refusal a
        // user hits most often, and reading only stderr rendered it as the
        // empty message below.
        let stdout = String::from_utf8_lossy(&out.stdout);
        let said = match stderr.trim() {
            "" => stdout.trim(),
            s => s,
        };
        // Neither stream said anything. Rare, and it would otherwise produce
        // an empty error dialog, which reads as a bug in mogeung rather than
        // as a refusal by git.
        if said.is_empty() {
            bail!(
                "git {:?} failed with no message (exit {})",
                args,
                out.status.code().unwrap_or(-1)
            );
        }
        bail!("{said}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Stage the given worktree paths. `R-D19`.
pub fn stage(root: &Path, paths: &[String]) -> Result<()> {
    // `add` rather than `add --all`: the pathspecs are the whole instruction,
    // and a verb that could stage more than it was handed is one nobody can
    // predict from the list they were looking at when they clicked.
    run_git_write(root, &pathspec_args(&["add"], paths)).map(drop)
}

/// Unstage the given paths, leaving the working tree alone. `R-D19`.
pub fn unstage(root: &Path, paths: &[String]) -> Result<()> {
    // Before the first commit there is no HEAD to restore the index from, and
    // `restore --staged` says so rather than doing the obvious thing. That is
    // not an error worth showing: "unstage" in a fresh repository means "make
    // it untracked again", which is what `rm --cached` does.
    if head_sha(root).is_err() {
        return run_git_write(root, &pathspec_args(&["rm", "--cached", "-r", "-q"], paths))
            .map(drop);
    }
    run_git_write(root, &pathspec_args(&["restore", "--staged"], paths)).map(drop)
}

/// Throw away local changes to the given paths. `R-D19`.
///
/// **The only verb here with no undo.** Everything else this daemon can do to
/// a repository is recoverable from git's own history; this destroys work git
/// has never seen. The confirmation belongs in the UI, but the honesty belongs
/// here too: nothing in this function is clever, and it does exactly what the
/// two commands below do.
///
/// Tracked and untracked paths need different commands and are partitioned by
/// asking git rather than by guessing from the filesystem — a path can exist
/// and be untracked, or not exist and be tracked (that is a deletion, and
/// discarding it means bringing the file back).
///
/// The question is put to `ls-files` rather than to `status`, which is the
/// listing the pane is built from and therefore shaped for display: `status`
/// **collapses an untracked directory to one row**, so a file inside a folder
/// the agent just created never appears in it and would be classified as
/// tracked. `ls-files` answers the question actually being asked — is git
/// tracking this exact path — and answers it for the whole selection in one
/// call.
pub fn discard(root: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let listed = run_git(root, &pathspec_args(&["ls-files", "-z"], paths))?;
    let tracked: HashSet<&str> = listed.split('\0').filter(|s| !s.is_empty()).collect();

    let (kept, gone): (Vec<String>, Vec<String>) = paths.iter().cloned().partition(|p| {
        // Exactly tracked, or a directory with something tracked beneath it —
        // a selection can name a folder, and `ls-files` answers with the files
        // inside rather than the folder itself.
        let prefix = format!("{p}/");
        tracked.contains(p.as_str()) || tracked.iter().any(|t| t.starts_with(&prefix))
    });

    // Tracked first. If `clean` were to run first and then `restore` refused,
    // the untracked files would already be gone with nothing restored — the
    // worst half of the operation completed alone.
    if !kept.is_empty() {
        run_git_write(root, &pathspec_args(&["restore", "--staged", "--worktree"], &kept))?;
    }
    if !gone.is_empty() {
        // `-q` because the list of what was removed is on stdout and nobody
        // reads it; `--` is in `pathspec_args`, and is what keeps a file named
        // `-rf` from being read as flags.
        run_git_write(root, &pathspec_args(&["clean", "-f", "-q"], &gone))?;
    }
    Ok(())
}

/// Commit what is staged. `R-D20`.
///
/// Returns the new commit's sha, so the pane can select what it just made
/// rather than hunting for it in a log it has to re-fetch first.
///
/// **Only what is staged**, never `-a`. The staging list is the instruction,
/// and a commit verb that could include a file the user had deliberately left
/// unstaged would make the checkboxes above it a suggestion.
///
/// Hooks run. Skipping them with `--no-verify` would be a silent change of
/// meaning — a repository that refuses bad commits would start accepting them
/// from this window and only this window — so the honest failure is git's own,
/// which `run_git_write` returns verbatim.
pub fn commit(
    root: &Path,
    message: &str,
    amend: bool,
    trailers: &[(String, String)],
) -> Result<String> {
    // git refuses an empty message itself, and its refusal is the better
    // sentence. This one exists because a message of only whitespace passes
    // git's check and produces a commit with a blank subject line.
    if message.trim().is_empty() {
        bail!("a commit needs a message");
    }

    let mut args: Vec<String> = vec!["commit".into(), "-m".into(), message.into()];
    if amend {
        args.push("--amend".into());
    }
    // `--trailer` rather than appending to the message ourselves: git knows
    // where a trailer block goes when the message already has one, how to
    // separate it from the body, and what to do about `Signed-off-by`.
    // Requires git 2.32 (2021).
    for (k, v) in trailers {
        args.push("--trailer".into());
        args.push(format!("{k}={v}"));
    }

    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git_write(root, &borrowed)?;
    head_sha(root)
}

/// The trailer naming the session a commit's work came from. `R-D20`.
///
/// This is the reason committing from the pane is worth building rather than
/// shelling out to a terminal: mogeung knows which session produced which
/// hunks, and a terminal never can. `R-F2` prompt-blame reads it back — from a
/// line, to the commit, to the session, to the prompt that caused it.
///
/// A `Key: value` trailer rather than anything cleverer because it survives:
/// `git log`, `git show`, GitHub, `git interpret-trailers --parse`, and every
/// tool that has never heard of mogeung all keep it intact and ignore it.
pub const SESSION_TRAILER: &str = "Mogeung-Session";

/// Create a branch, and optionally move onto it. `R-D21`.
///
/// Created with `branch` rather than `switch -c` even when we are about to
/// switch, because the two refuse a bad name very differently: `git branch`
/// says *"'-evil' is not a valid branch name"* and points at
/// `git check-ref-format`, while `switch -c` parses the same string as flags
/// and answers *"unknown switch `e'"*. `--` does not help — git reads the
/// argument as options anyway. Two commands, and the user gets the sentence
/// that tells them what is wrong.
pub fn branch_create(root: &Path, name: &str, switch_to: bool) -> Result<()> {
    let name = check_ref(name)?;
    run_git_write(root, &["branch", name])?;
    if switch_to {
        run_git_write(root, &["switch", name])?;
    }
    Ok(())
}

/// Move the worktree onto an existing branch. `R-D21`.
///
/// Nothing here protects uncommitted work, because git already does: it
/// refuses a switch that would overwrite local changes, and its refusal names
/// every file. What git has no opinion about — and cannot detect — is that an
/// agent may be reading those files right now. That warning belongs in the
/// window, which is the only place that knows a session is live.
pub fn switch(root: &Path, name: &str) -> Result<()> {
    let name = check_ref(name)?;
    run_git_write(root, &["switch", name]).map(drop)
}

/// Shelve the working tree. `R-D21`.
///
/// `--include-untracked` is a choice the caller makes, not a default, because
/// the two behaviours differ in whether an agent's new files come along or are
/// left behind on the branch you are leaving.
pub fn stash_push(root: &Path, message: &str, include_untracked: bool) -> Result<()> {
    let mut args: Vec<&str> = vec!["stash", "push"];
    if include_untracked {
        args.push("--include-untracked");
    }
    let message = message.trim();
    if !message.is_empty() {
        args.push("-m");
        args.push(message);
    }
    run_git_write(root, &args).map(drop)
}

/// Restore a stash and remove it. `R-D21`.
pub fn stash_pop(root: &Path, index: u32) -> Result<()> {
    run_git_write(root, &["stash", "pop", &stash_ref(index)]).map(drop)
}

/// Throw a stash away without restoring it. `R-D21`.
///
/// The second verb here with no undo — `stash drop` prints the dropped
/// commit's sha and it stays reachable until gc, but recovering it means
/// knowing that, so the UI must ask.
pub fn stash_drop(root: &Path, index: u32) -> Result<()> {
    run_git_write(root, &["stash", "drop", &stash_ref(index)]).map(drop)
}

/// `stash@{n}`, built here rather than accepted from a client — the index is
/// a number, so there is no string from outside to spell a flag with.
fn stash_ref(index: u32) -> String {
    format!("stash@{{{index}}}")
}

/// A ref name a write verb may act on, or a refusal naming the reason.
///
/// The same rule the read side uses to scope a log ([`valid_ref_name`]), which
/// is deliberately narrower than git's own: everything a hostile client would
/// need — a leading `-`, `..`, `@{` — is outside it. Sharing the rule matters
/// more here than there, since reading a nonsense ref shows nothing and
/// writing one moves the worktree.
fn check_ref(name: &str) -> Result<&str> {
    let name = name.trim();
    if !valid_ref_name(name) {
        bail!(
            "{name:?} is not a branch name mogeung will use — letters, digits, \
             and . _ - / only, not starting with - . or /"
        );
    }
    Ok(name)
}

/// `git <verb> -- <paths>`, with the separator that makes a path a path.
///
/// Without `--`, a file called `-i` or `--hard` is an option. Agents produce
/// stranger filenames than people do, and this is a write verb: the failure is
/// not a confusing message but the wrong command running.
fn pathspec_args<'a>(verb: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    let mut args: Vec<&str> = verb.to_vec();
    args.push("--");
    args.extend(paths.iter().map(String::as_str));
    args
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
            } else if let Some(old) = line.strip_prefix("copy from ") {
                // A copy reads like a rename whose source survives; the
                // shapes have no separate status and do not need one.
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

use mogeung_core::wire::{
    BlameLine, BranchInfo, CommitDetail, CommitInfo, ReflogEntry, RefsInfo, RemoteInfo,
    StashInfo, StatusEntry, SubmoduleInfo, TagInfo, WorktreeInfo,
};

/// Blame stops here; past this a "who wrote this line" gutter is a memory
/// test. The truncated flag says so.
const MAX_BLAME_LINES: usize = 20_000;

/// A sha as an argument must look like a sha. The daemon is unauthenticated,
/// and `git show <client-supplied-string>` where the string may start with
/// `-` is how "read-only" quietly stops being true.
fn valid_sha(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// A sha, optionally suffixed `^` — "the parent of". The one revision
/// operator re-blame needs, and the only one allowed: still no way to spell
/// a flag.
fn valid_rev(s: &str) -> bool {
    valid_sha(s.strip_suffix('^').unwrap_or(s))
}

/// A ref name a client may scope the log to. Deliberately narrower than
/// what git itself would accept: branch and tag names in the wild are
/// `[A-Za-z0-9._/-]`, and everything a hostile client would need — a
/// leading `-`, `..`, `@{` — is outside that set or refused explicitly.
fn valid_ref_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 250
        && !s.starts_with(['-', '.', '/'])
        && !s.ends_with('/')
        && !s.contains("..")
        && !s.contains("@{")
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
}

/// What narrows a log page beyond its ref scope. All literal text — no
/// regex reaches git — and `path` doubles as file history because it
/// switches `--follow` on. `R-D12`.
#[derive(Debug, Default, Clone)]
pub struct LogFilter {
    pub grep: Option<String>,
    pub author: Option<String>,
    pub path: Option<String>,
    /// Pickaxe: commits that changed how often this literal occurs.
    pub pickaxe: Option<String>,
}

/// How a diff is cut: context width and whitespace sensitivity. `R-D14`.
#[derive(Debug, Clone, Copy)]
pub struct DiffOpts {
    pub context: u32,
    pub ignore_ws: bool,
}

impl Default for DiffOpts {
    fn default() -> Self {
        DiffOpts {
            context: 3,
            ignore_ws: false,
        }
    }
}

impl DiffOpts {
    pub fn from_wire(context: Option<u32>, ignore_ws: Option<bool>) -> Self {
        DiffOpts {
            // 400 lines of context is "the whole file" for most files and
            // a bounded answer for the rest.
            context: context.unwrap_or(3).min(400),
            ignore_ws: ignore_ws.unwrap_or(false),
        }
    }

    fn args(&self) -> Vec<String> {
        let mut a = vec![format!("--unified={}", self.context)];
        if self.ignore_ws {
            a.push("-w".to_string());
        }
        a
    }
}

/// A filter string a client may hand to git as `--grep=…`/`--author=…`.
/// Joined with `=` it cannot open a new argument, so the checks left are
/// sanity: a length that fits a filter box, and no control characters.
fn valid_filter_text(s: &str) -> bool {
    !s.is_empty() && s.len() <= 200 && !s.chars().any(|c| c.is_control())
}

/// One page of the log, newest first, scoped to `rev` (`None` = HEAD) and
/// narrowed by `filter`. Asks for one row past `limit`, so "was that the
/// end of history" costs no second call. Also returns, per commit, the
/// files it touched — fuel for the session-attribution heuristic, fetched
/// in the same process spawn.
pub fn log_page(
    cwd: &Path,
    skip: u32,
    limit: u32,
    rev: Option<&str>,
    filter: &LogFilter,
) -> Result<(Vec<CommitInfo>, Vec<Vec<String>>, bool)> {
    let limit = limit.clamp(1, 200);
    if let Some(r) = rev {
        if !valid_ref_name(r) {
            bail!("that is not a ref name");
        }
    }
    for f in [&filter.grep, &filter.author, &filter.pickaxe] {
        if let Some(f) = f {
            if !valid_filter_text(f) {
                bail!("that is not a filter");
            }
        }
    }
    if let Some(p) = &filter.path {
        if !valid_filter_text(p) || Path::new(p).is_absolute() || p.split('/').any(|s| s == "..")
        {
            bail!("{p} is not a path inside the session");
        }
    }
    // \x1f between fields, \x1e between records: subjects contain anything.
    let skip_arg = format!("--skip={skip}");
    let n_arg = format!("-n{}", limit + 1);
    let grep_arg = filter.grep.as_ref().map(|g| format!("--grep={g}"));
    let author_arg = filter.author.as_ref().map(|a| format!("--author={a}"));
    let pickaxe_arg = filter.pickaxe.as_ref().map(|x| format!("-S{x}"));
    let mut args = vec![
        "log",
        &skip_arg,
        &n_arg,
        "--name-only",
        "--format=%H%x1f%h%x1f%an%x1f%at%x1f%D%x1f%p%x1f%s%x1e",
    ];
    if let Some(g) = &grep_arg {
        // Literal and case-insensitive: a filter box is not a regex field,
        // and `--fixed-strings` covers --author the same way.
        args.extend(["--fixed-strings", "-i", g]);
    }
    if let Some(a) = &author_arg {
        if grep_arg.is_none() {
            args.extend(["--fixed-strings", "-i"]);
        }
        args.push(a);
    }
    if let Some(x) = &pickaxe_arg {
        // -S is literal by default: no regex ever reaches git from here.
        args.push(x);
    }
    if filter.path.is_some() {
        // One path exactly — --follow is only defined for one — and this
        // is what makes the filtered log double as file history.
        args.push("--follow");
    }
    if let Some(r) = rev {
        args.push(r);
    }
    args.push("--");
    if let Some(p) = &filter.path {
        args.push(p);
    }
    let out = run_git(cwd, &args)?;
    let (mut commits, mut files) = parse_log(&out);
    let done = commits.len() as u32 <= limit;
    commits.truncate(limit as usize);
    files.truncate(limit as usize);
    Ok((commits, files, done))
}

/// Parse `--format=…%x1e --name-only` output. Only the separators are
/// trusted: a line containing `\x1f` is a record's fields; every other
/// non-blank line is a filename belonging to the *previous* record, because
/// `--name-only` prints a commit's files after its format line.
fn parse_log(out: &str) -> (Vec<CommitInfo>, Vec<Vec<String>>) {
    let mut commits: Vec<CommitInfo> = Vec::new();
    let mut files: Vec<Vec<String>> = Vec::new();
    for chunk in out.split('\x1e') {
        let mut pre_files: Vec<String> = Vec::new();
        let mut fields: Option<&str> = None;
        for line in chunk.lines() {
            if fields.is_none() && line.contains('\x1f') {
                fields = Some(line);
            } else if fields.is_none() && !line.trim().is_empty() {
                pre_files.push(line.trim().to_string());
            }
        }
        // The files seen before this chunk's field line were printed under
        // the previous commit.
        if let Some(prev) = files.last_mut() {
            *prev = pre_files;
        }
        let Some(rec) = fields else { continue };
        let mut f = rec.split('\x1f');
        let Some(sha) = f.next().map(|s| s.trim().to_string()) else {
            continue;
        };
        if sha.is_empty() {
            continue;
        }
        commits.push(CommitInfo {
            sha,
            short: f.next().unwrap_or("").to_string(),
            author: f.next().unwrap_or("").to_string(),
            epoch: f.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            refs: f
                .next()
                .unwrap_or("")
                .split(", ")
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect(),
            parents: f
                .next()
                .unwrap_or("")
                .split_whitespace()
                .map(|s| s.to_string())
                .collect(),
            summary: f.next().unwrap_or("").to_string(),
            touches_session: false,
        });
        files.push(Vec::new());
    }
    (commits, files)
}

/// One commit's patch, in the same shapes the Changes tab renders — which is
/// what lets the Git pane reuse the entire diff pipeline for free — plus its
/// header (`R-D12`). The header is a second, `-s` call in the same daemon
/// trip; if it fails the diff still answers, wearing `None`.
pub fn show_commit(
    cwd: &Path,
    sha: &str,
    opts: DiffOpts,
    reviewed: &HashSet<String>,
) -> Result<(Vec<FileChange>, Option<CommitDetail>)> {
    if !valid_sha(sha) {
        bail!("that is not a commit sha");
    }
    let unified = opts.args();
    let mut args: Vec<&str> = vec!["show", "--no-color", "--no-ext-diff", "-M", "-C"];
    args.extend(unified.iter().map(String::as_str));
    args.extend(["--format=", sha, "--"]);
    let out = run_git_diff(cwd, &args)?;
    let mut detail = run_git(
        cwd,
        &[
            "show",
            "-s",
            "--format=%an%x1f%cn%x1f%at%x1f%ct%x1f%p%x1f%D%x1f%B",
            sha,
            "--",
        ],
    )
    .ok()
    .and_then(|d| parse_commit_detail(&d));
    // "Which branches contain this?" — the question the header exists to
    // answer when checking whether a fix has reached main. `R-D18`. A
    // separate call so its failure (or slowness on a huge repo) costs the
    // branch line, never the header.
    if let Some(d) = detail.as_mut() {
        if let Ok(out) = run_git(
            cwd,
            &[
                "branch",
                "-a",
                "--contains",
                sha,
                "--format=%(refname:short)",
            ],
        ) {
            d.branches = parse_containing_branches(&out);
        }
    }
    Ok((parse_unified(&out, reviewed), detail))
}

/// One branch name per line. A detached HEAD prints as
/// `(HEAD detached at …)` — parenthesised lines are states, not refs, and
/// are dropped.
fn parse_containing_branches(out: &str) -> Vec<String> {
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('('))
        .map(|l| l.to_string())
        .collect()
}

/// Parse the `-s` header record. `%B` is multiline and last, so `splitn`
/// keeps the body's newlines intact; a body containing the separator could
/// corrupt nothing past it.
fn parse_commit_detail(out: &str) -> Option<CommitDetail> {
    let mut f = out.splitn(7, '\x1f');
    Some(CommitDetail {
        author: f.next()?.trim_start_matches(['\n', '\r']).to_string(),
        committer: f.next()?.to_string(),
        epoch: f.next()?.trim().parse().unwrap_or(0),
        commit_epoch: f.next()?.trim().parse().unwrap_or(0),
        parents: f
            .next()?
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
        refs: f
            .next()?
            .split(", ")
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect(),
        message: f.next().unwrap_or("").trim_end().to_string(),
        // Filled by the separate `--contains` call in `show_commit`.
        branches: Vec::new(),
    })
}

/// The diff between two commits, `from` → `to`. Both ends are commits the
/// client picked off the log; the range is read the same way a commit is.
pub fn diff_range(cwd: &Path, from: &str, to: &str, opts: DiffOpts) -> Result<Vec<FileChange>> {
    if !valid_sha(from) || !valid_sha(to) {
        bail!("that is not a commit sha");
    }
    let unified = opts.args();
    let mut args: Vec<&str> = vec!["diff", "--no-color", "--no-ext-diff", "-M", "-C"];
    args.extend(unified.iter().map(String::as_str));
    args.extend([from, to, "--"]);
    let out = run_git_diff(cwd, &args)?;
    Ok(parse_unified(&out, &HashSet::new()))
}

/// What merging `branch` would bring to the current branch: the diff from
/// their merge base to the branch tip — three-dot semantics, with the two
/// ends resolved here so the answer can name real shas. `R-D15`.
pub fn compare_with_head(
    cwd: &Path,
    branch: &str,
    opts: DiffOpts,
) -> Result<(String, String, Vec<FileChange>)> {
    if !valid_ref_name(branch) {
        bail!("that is not a ref name");
    }
    let base = run_git(cwd, &["merge-base", "HEAD", branch])?
        .trim()
        .to_string();
    let tip = run_git(cwd, &["rev-parse", branch])?.trim().to_string();
    if !valid_sha(&base) || !valid_sha(&tip) {
        bail!("could not resolve the merge base");
    }
    let files = diff_range(cwd, &base, &tip, opts)?;
    Ok((base, tip, files))
}

/// The repo's refs in one answer. Several git calls, one wire round-trip —
/// a header line is not worth five.
pub fn refs(cwd: &Path) -> Result<RefsInfo> {
    let head = run_git(cwd, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let head_sha = run_git(cwd, &["rev-parse", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let branches = parse_branches(
        &run_git(
            cwd,
            &[
                "for-each-ref",
                "refs/heads",
                "--sort=-committerdate",
                "--count=200",
                "--format=%(refname:short)%1f%(objectname:short)%1f%(HEAD)%1f%(upstream:short)%1f%(upstream:track)%1f%(committerdate:unix)%1e",
            ],
        )
        .unwrap_or_default(),
    );
    let tags = parse_tags(
        &run_git(
            cwd,
            &[
                "for-each-ref",
                "refs/tags",
                "--sort=-creatordate",
                "--count=100",
                "--format=%(refname:short)%1f%(objectname:short)%1f%(*objectname:short)%1f%(creatordate:unix)%1e",
            ],
        )
        .unwrap_or_default(),
    );
    // Remote-tracking branches: same framing as local, no tracking
    // fields of their own, `origin/HEAD` symref skipped as noise.
    let remote_branches: Vec<BranchInfo> = parse_branches(
        &run_git(
            cwd,
            &[
                "for-each-ref",
                "refs/remotes",
                "--sort=-committerdate",
                "--count=200",
                "--format=%(refname:short)%1f%(objectname:short)%1f%(HEAD)%1f%(upstream:short)%1f%(upstream:track)%1f%(committerdate:unix)%1e",
            ],
        )
        .unwrap_or_default(),
    )
    .into_iter()
    .filter(|b| !b.name.ends_with("/HEAD"))
    .collect();
    let remotes = parse_remotes(&run_git(cwd, &["remote", "-v"]).unwrap_or_default());
    // FETCH_HEAD lives in the *common* dir, so a worktree session still
    // sees the fetch its main checkout ran. Reading an mtime is as far as
    // "remote status" goes — the daemon never fetches.
    let fetch_epoch = run_git(cwd, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()
        .and_then(|d| {
            std::fs::metadata(Path::new(d.trim()).join("FETCH_HEAD"))
                .and_then(|m| m.modified())
                .ok()
        })
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    Ok(RefsInfo {
        head,
        head_sha,
        branches,
        tags,
        remotes,
        fetch_epoch,
        remote_branches,
    })
}

fn parse_branches(out: &str) -> Vec<BranchInfo> {
    out.split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches(['\n', '\r']).split('\x1f');
            let name = f.next()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let sha = f.next()?.to_string();
            let current = f.next()? == "*";
            let upstream = Some(f.next()?.to_string()).filter(|s| !s.is_empty());
            // "[ahead 2, behind 1]", "[ahead 2]", "[behind 1]", "[gone]", "".
            let track = f.next().unwrap_or("");
            let grab = |key: &str| -> u32 {
                track
                    .split(|c: char| !c.is_ascii_alphanumeric())
                    .collect::<Vec<_>>()
                    .windows(2)
                    .find(|w| w[0] == key)
                    .and_then(|w| w[1].parse().ok())
                    .unwrap_or(0)
            };
            Some(BranchInfo {
                name,
                sha,
                current,
                upstream,
                ahead: grab("ahead"),
                behind: grab("behind"),
                epoch: f.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0),
            })
        })
        .collect()
}

fn parse_tags(out: &str) -> Vec<TagInfo> {
    out.split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches(['\n', '\r']).split('\x1f');
            let name = f.next()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let plain = f.next()?.to_string();
            // The deref field is non-empty exactly for annotated tags, and
            // is the commit; lightweight tags already point at one.
            let deref = f.next().unwrap_or("").to_string();
            Some(TagInfo {
                name,
                sha: if deref.is_empty() { plain } else { deref },
                epoch: f.next().and_then(|s| s.trim().parse().ok()).unwrap_or(0),
            })
        })
        .collect()
}

fn parse_remotes(out: &str) -> Vec<RemoteInfo> {
    let mut seen = HashSet::new();
    out.lines()
        .filter_map(|line| {
            // "origin\thttps://… (fetch)" — one row per remote, fetch URL.
            let (name, rest) = line.split_once('\t')?;
            let url = rest.strip_suffix(" (fetch)")?;
            if !seen.insert(name.to_string()) {
                return None;
            }
            Some(RemoteInfo {
                name: name.to_string(),
                url: url.trim().to_string(),
            })
        })
        .collect()
}

/// The stash list. `%gs` is the reflog subject — "WIP on main: …" or the
/// message `stash push -m` gave it.
pub fn stashes(cwd: &Path) -> Result<Vec<StashInfo>> {
    let out = run_git(cwd, &["stash", "list", "--format=%at%x1f%gs%x1e"])?;
    Ok(parse_stashes(&out))
}

fn parse_stashes(out: &str) -> Vec<StashInfo> {
    out.split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches(['\n', '\r']).split('\x1f');
            let epoch: i64 = f.next()?.trim().parse().ok()?;
            let message = f.next().unwrap_or("").to_string();
            Some((epoch, message))
        })
        .enumerate()
        .map(|(i, (epoch, message))| StashInfo {
            index: i as u32,
            message,
            epoch,
        })
        .collect()
}

/// One stash's diff. The argument is built here from a number — a client
/// cannot spell `stash@{…}` at the daemon, only an index.
pub fn stash_show(cwd: &Path, index: u32, opts: DiffOpts) -> Result<Vec<FileChange>> {
    let spec = format!("stash@{{{index}}}");
    let unified = opts.args();
    let mut args: Vec<&str> = vec!["stash", "show", "-p", "--no-color"];
    args.extend(unified.iter().map(String::as_str));
    args.push(&spec);
    let out = run_git_diff(cwd, &args)?;
    Ok(parse_unified(&out, &HashSet::new()))
}

/// Submodule paths and their state, `git submodule status` parsed leniently.
pub fn submodules(cwd: &Path) -> Result<Vec<SubmoduleInfo>> {
    let out = run_git(cwd, &["submodule", "status"])?;
    Ok(parse_submodules(&out))
}

fn parse_submodules(out: &str) -> Vec<SubmoduleInfo> {
    out.lines()
        .filter_map(|line| {
            if line.len() < 3 {
                return None;
            }
            // "[ +-U]<sha> <path>( (<describe>))?"
            let (state, rest) = line.split_at(1);
            let mut parts = rest.trim().splitn(2, ' ');
            let sha = parts.next()?.to_string();
            if !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            let rest = parts.next().unwrap_or("");
            let (path, note) = match rest.rsplit_once(" (") {
                Some((p, n)) => (p.to_string(), n.trim_end_matches(')').to_string()),
                None => (rest.to_string(), String::new()),
            };
            Some(SubmoduleInfo {
                path,
                sha: sha.chars().take(8).collect(),
                state: state.to_string(),
                note,
            })
        })
        .collect()
}

/// Where HEAD has been — the read-only recovery map. Freeform subjects,
/// so only the separators are trusted. `R-D15`.
pub fn reflog(cwd: &Path) -> Result<Vec<ReflogEntry>> {
    let out = run_git(
        cwd,
        &["reflog", "-n", "100", "--format=%h%x1f%gd%x1f%gs%x1e"],
    )?;
    Ok(parse_reflog(&out))
}

fn parse_reflog(out: &str) -> Vec<ReflogEntry> {
    out.split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches(['\n', '\r']).split('\x1f');
            let sha = f.next()?.trim().to_string();
            if sha.is_empty() {
                return None;
            }
            Some(ReflogEntry {
                sha,
                selector: f.next().unwrap_or("").to_string(),
                summary: f.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// `git worktree list --porcelain`, parsed leniently: blocks separated by
/// blank lines, each opened by a `worktree <path>` line. `R-D15`.
pub fn worktrees(cwd: &Path) -> Result<Vec<WorktreeInfo>> {
    let out = run_git(cwd, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktrees(&out))
}

fn parse_worktrees(out: &str) -> Vec<WorktreeInfo> {
    let mut list = Vec::new();
    let mut cur: Option<WorktreeInfo> = None;
    for line in out.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(w) = cur.take() {
                list.push(w);
            }
            cur = Some(WorktreeInfo {
                path: path.to_string(),
                sha: String::new(),
                branch: None,
            });
        } else if let Some(sha) = line.strip_prefix("HEAD ") {
            if let Some(w) = cur.as_mut() {
                w.sha = sha.chars().take(8).collect();
            }
        } else if let Some(branch) = line.strip_prefix("branch ") {
            if let Some(w) = cur.as_mut() {
                w.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string(),
                );
            }
        }
        // "detached", "bare", "locked …" and anything future are noted by
        // their absence — a branchless row reads as detached.
    }
    if let Some(w) = cur.take() {
        list.push(w);
    }
    list
}

/// Cap per conflict stage — three of these travel in one message.
const MAX_STAGE_BYTES: usize = 256 * 1024;

/// A conflicted file's three stages: base, ours, theirs. A stage the
/// merge did not produce (both-added has no base) comes back empty rather
/// than erroring — the view's job is to show what exists. `R-D16`.
pub fn conflict_stages(cwd: &Path, rel: &str) -> Result<(String, String, String, bool)> {
    if Path::new(rel).is_absolute() || rel.split('/').any(|p| p == "..") {
        bail!("{rel} is not a path inside the session");
    }
    let mut truncated = false;
    let mut stage = |n: u32| -> String {
        let spec = format!(":{n}:{rel}");
        let mut text = run_git(cwd, &["show", &spec]).unwrap_or_default();
        if text.len() > MAX_STAGE_BYTES {
            let mut end = MAX_STAGE_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            truncated = true;
        }
        text
    };
    let (base, ours, theirs) = (stage(1), stage(2), stage(3));
    Ok((base, ours, theirs, truncated))
}

/// Cap for revision-tab bodies, the explorer's own cap: past this the tab
/// shows the head of the file and says so.
const MAX_REV_FILE_BYTES: usize = 512 * 1024;

/// One file's content at a revision — `git show sha:path`, the read-only
/// answer to "what did this look like before the agent rewrote it".
pub fn file_at_rev(cwd: &Path, rev: &str, rel: &str) -> Result<(String, bool)> {
    if !valid_rev(rev) {
        bail!("that is not a revision");
    }
    // The path is historical: it may not exist in the worktree at all, so
    // containment is lexical — no absolute paths, no `..`.
    if Path::new(rel).is_absolute() || rel.split('/').any(|p| p == "..") {
        bail!("{rel} is not a path inside the session");
    }
    let spec = format!("{rev}:{rel}");
    let out = run_git(cwd, &["show", &spec])?;
    let truncated = out.len() > MAX_REV_FILE_BYTES;
    let mut content = out;
    if truncated {
        // Cut on a char boundary; a revision tab is a reading aid, not an
        // archive download.
        let mut end = MAX_REV_FILE_BYTES;
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        content.truncate(end);
    }
    Ok((content, truncated))
}

/// The repo's uncommitted state, porcelain v1 parsed leniently — a code we
/// do not recognise still lists, it just wears its raw `XY`. `--ignored`
/// adds `!!` rows (directories collapsed), which clients use to *dim*, not
/// to list.
/// `-z` rather than line-oriented, which is not a detail. `R-D19`.
///
/// Plain `--porcelain` C-quotes any path it considers unusual — a space is
/// enough for surrounding quotes, and a non-ASCII byte becomes an octal escape,
/// so `café.txt` arrives as `caf\303\251.txt`. Stripping the quotes got the
/// first case right and left the second wrong, which was survivable while this
/// was only ever *displayed*.
///
/// It stops being survivable now that the same strings are pathspecs handed
/// back to git by [`discard`], which partitions on exact matches: a filename
/// git spelled differently on the way out than the way in is a path the verb
/// classifies wrongly. `-z` emits paths raw, NUL-separated, and quotes nothing.
pub fn status(cwd: &Path) -> Result<Vec<StatusEntry>> {
    let out = run_git(cwd, &["status", "--porcelain", "-z", "--ignored"])?;
    Ok(parse_status(&out))
}

fn parse_status(out: &str) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let mut records = out.split('\0');
    while let Some(rec) = records.next() {
        // The trailing NUL leaves one empty record; a short one is not a row.
        if rec.len() < 4 {
            continue;
        }
        let (code, rest) = rec.split_at(2);
        // Exactly one space separates the code from the path — `trim_start`
        // here would eat the first character of a file whose name begins with
        // one, which `-z` is otherwise careful to preserve.
        let path = rest.strip_prefix(' ').unwrap_or(rest);
        let x = code.chars().next().unwrap_or(' ');
        let y = code.chars().nth(1).unwrap_or(' ');
        // A rename or copy spends a second record on the source path. Under
        // `-z` there is no ` -> ` to split on, and the record must be consumed
        // or the old name is read as a row of its own.
        if x == 'R' || x == 'C' || y == 'R' || y == 'C' {
            let _ = records.next();
        }
        // The porcelain's unmerged table: any U, or both sides added or
        // both deleted.
        let conflicted = x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D');
        entries.push(StatusEntry {
            path: path.to_string(),
            staged: x != ' ' && x != '?' && x != '!',
            unstaged: y != ' ' && y != '!',
            state: code.to_string(),
            conflicted,
        });
    }
    entries
}

/// One uncommitted file against `HEAD` — or against nothing, when it is
/// untracked and `HEAD` has never heard of it.
pub fn diff_file(cwd: &Path, rel: &str, opts: DiffOpts) -> Result<Vec<FileChange>> {
    let tracked = run_git(cwd, &["ls-files", "--error-unmatch", "--", rel]).is_ok();
    let out = if tracked {
        let unified = opts.args();
        let mut args: Vec<&str> = vec!["diff", "--no-color", "--no-ext-diff", "-M"];
        args.extend(unified.iter().map(String::as_str));
        args.extend(["HEAD", "--", rel]);
        run_git_diff(cwd, &args)?
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

/// Per-line authorship, of the worktree file (`rev: None`) or of the file
/// as it stood at a revision. Uncommitted lines come back with git's
/// all-zeros sha and its "Not Committed Yet" author, unchanged — renaming
/// them is the client's editorial decision, not the daemon's.
pub fn blame(cwd: &Path, rel: &str, rev: Option<&str>) -> Result<(Vec<BlameLine>, bool)> {
    let out = match rev {
        None => run_git(cwd, &["blame", "--porcelain", "--", rel])?,
        Some(r) => {
            if !valid_rev(r) {
                bail!("that is not a revision");
            }
            run_git(cwd, &["blame", "--porcelain", r, "--", rel])?
        }
    };
    Ok(parse_blame(&out))
}

fn parse_blame(out: &str) -> (Vec<BlameLine>, bool) {
    // Porcelain prints full commit details once per commit; later lines of
    // the same commit carry only the header. Remember what each sha said.
    let mut known: std::collections::HashMap<String, (String, i64, String)> =
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
                let (author, epoch, summary) = known.get(&sha).cloned().unwrap_or_default();
                lines.push(BlameLine {
                    sha: sha.chars().take(8).collect(),
                    author,
                    epoch,
                    summary,
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
        } else if let Some(s) = line.strip_prefix("summary ") {
            if let Some(sha) = &cur {
                known.entry(sha.clone()).or_default().2 = s.to_string();
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
        let out =
            "aaa111\x1faaa\x1fkeith\x1f1722000000\x1fHEAD -> main, tag: v1\x1fp1 p2\x1ffix: a \"quoted\" thing\x1e\n\
             bbb222\x1fbbb\x1fclaude\x1f1722000100\x1f\x1fp3\x1fsubject with \x7f and spaces\x1e";
        let (commits, _) = parse_log(out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].short, "aaa");
        assert_eq!(commits[0].epoch, 1722000000);
        assert!(commits[0].summary.contains("quoted"));
        assert_eq!(commits[0].refs, vec!["HEAD -> main", "tag: v1"]);
        assert_eq!(commits[0].parents, vec!["p1", "p2"]);
        assert!(commits[1].refs.is_empty());
        assert!(parse_log("").0.is_empty());
        assert!(parse_log("garbage with no separators").0.len() <= 1);
    }

    /// `--name-only` prints a commit's files *after* its format line, so
    /// they land at the head of the next \x1e chunk — and must attach to
    /// the commit that produced them, not the one that follows.
    #[test]
    fn log_files_attach_to_the_commit_that_touched_them() {
        let out = "aaa111\x1faaa\x1fkeith\x1f1722000000\x1f\x1fp1\x1ffirst\x1e\n\n\
                   src/a.rs\nsrc/b.rs\n\n\
                   bbb222\x1fbbb\x1fkeith\x1f1722000100\x1f\x1f\x1fsecond\x1e\n\n\
                   docs/c.md\n";
        let (commits, files) = parse_log(out);
        assert_eq!(commits.len(), 2);
        assert_eq!(files[0], vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(files[1], vec!["docs/c.md"], "the tail chunk's files belong to the last commit");
    }

    /// The for-each-ref framing: current-branch marker, tracking counts
    /// pulled out of "[ahead 2, behind 1]", and a branch with no upstream.
    #[test]
    fn branch_parsing_reads_tracking_state() {
        let out = "main\x1fabc123\x1f*\x1forigin/main\x1f[ahead 2, behind 1]\x1f1722000000\x1e\n\
                   fix/x\x1fdef456\x1f \x1f\x1f\x1f1722000100\x1e\n\
                   gone-br\x1f987fed\x1f \x1forigin/gone-br\x1f[gone]\x1f1722000200\x1e";
        let bs = parse_branches(out);
        assert_eq!(bs.len(), 3);
        assert!(bs[0].current);
        assert_eq!(bs[0].ahead, 2);
        assert_eq!(bs[0].behind, 1);
        assert_eq!(bs[0].upstream.as_deref(), Some("origin/main"));
        assert!(!bs[1].current);
        assert_eq!(bs[1].upstream, None);
        assert_eq!((bs[1].ahead, bs[1].behind), (0, 0));
        assert_eq!((bs[2].ahead, bs[2].behind), (0, 0), "[gone] is not a count");
        assert!(parse_branches("").is_empty());
    }

    /// Annotated tags carry the commit in the deref field; lightweight tags
    /// only in the plain one. Clicking either must land on a commit.
    #[test]
    fn tag_parsing_dereferences_annotated_tags() {
        let out = "v1.0\x1ftag0bj00\x1fcommit11\x1f1722000000\x1e\n\
                   light\x1fcommit22\x1f\x1f1722000100\x1e";
        let ts = parse_tags(out);
        assert_eq!(ts[0].sha, "commit11", "annotated tag must dereference");
        assert_eq!(ts[1].sha, "commit22");
    }

    #[test]
    fn remote_parsing_dedupes_fetch_and_push() {
        let out = "origin\thttps://github.com/x/y.git (fetch)\n\
                   origin\thttps://github.com/x/y.git (push)\n\
                   fork\tgit@github.com:z/y.git (fetch)\n";
        let rs = parse_remotes(out);
        assert_eq!(rs.len(), 2);
        assert_eq!(rs[0].name, "origin");
        assert_eq!(rs[1].url, "git@github.com:z/y.git");
    }

    #[test]
    fn stash_parsing_indexes_in_list_order() {
        let out = "1722000100\x1fWIP on main: abc fix the thing\x1e\n\
                   1722000000\x1fOn fix/x: stashed by hand\x1e";
        let ss = parse_stashes(out);
        assert_eq!(ss.len(), 2);
        assert_eq!(ss[0].index, 0, "stash@{{0}} is the newest, first in the list");
        assert_eq!(ss[1].index, 1);
        assert!(ss[1].message.contains("stashed by hand"));
    }

    /// `git submodule status` prefixes: in-sync, out-of-date, uninitialised.
    #[test]
    fn submodule_parsing_keeps_the_state_prefix() {
        let out = " aaaa111122223333aaaa111122223333aaaa1111 vendor/lib (v1.2.0)\n\
                   +bbbb111122223333aaaa111122223333aaaa1111 vendor/other (heads/main)\n\
                   -cccc111122223333aaaa111122223333aaaa1111 vendor/uninit\n";
        let subs = parse_submodules(out);
        assert_eq!(subs.len(), 3);
        assert_eq!(subs[0].state, " ");
        assert_eq!(subs[0].path, "vendor/lib");
        assert_eq!(subs[0].note, "v1.2.0");
        assert_eq!(subs[1].state, "+");
        assert_eq!(subs[2].state, "-");
        assert_eq!(subs[2].note, "");
        assert!(parse_submodules("").is_empty());
    }

    /// The porcelain codes that actually occur. NUL-separated, because that is
    /// what `-z` produces and what the parser is now written against — under
    /// `-z` a rename spends a *second record* on its source path rather than
    /// an ` -> ` arrow.
    #[test]
    fn status_parsing_reads_the_common_codes() {
        let out = " M crates/a.rs\0M  crates/b.rs\0MM crates/c.rs\0?? new.txt\0R  new.rs\0old.rs\0";
        let entries = parse_status(out);
        assert_eq!(entries.len(), 5, "the rename's source is not a row of its own");
        let by = |p: &str| entries.iter().find(|e| e.path == p).unwrap();
        assert!(!by("crates/a.rs").staged);
        assert!(by("crates/a.rs").unstaged);
        assert!(by("crates/b.rs").staged);
        assert!(!by("crates/b.rs").unstaged);
        assert!(by("crates/c.rs").staged && by("crates/c.rs").unstaged);
        assert!(!by("new.txt").staged, "untracked is not staged");
        assert!(by("new.txt").unstaged);
        assert_eq!(by("new.rs").state, "R ", "a rename lists under its new name");
        assert!(entries.iter().all(|e| e.path != "old.rs"), "the source is consumed");
        assert!(entries.iter().all(|e| !e.conflicted));
    }

    /// The unmerged table: any `U`, both-added, both-deleted. And ignored
    /// rows, which are neither staged nor unstaged — dimming data, not a
    /// change.
    #[test]
    fn status_parsing_marks_conflicts_and_ignored() {
        let out = "UU src/merge.rs\0AA both.rs\0DD gone.rs\0AU theirs.rs\0!! target/\0 M ok.rs\0";
        let entries = parse_status(out);
        let by = |p: &str| entries.iter().find(|e| e.path == p).unwrap();
        for p in ["src/merge.rs", "both.rs", "gone.rs", "theirs.rs"] {
            assert!(by(p).conflicted, "{p} must read as conflicted");
        }
        assert!(!by("ok.rs").conflicted);
        let ignored = by("target/");
        assert_eq!(ignored.state, "!!");
        assert!(!ignored.staged && !ignored.unstaged, "ignored is not a change");
    }

    /// The names line-oriented porcelain spelled differently on the way out
    /// than they exist on disk. Display could survive that; `R-D19`'s write
    /// verbs cannot, because they hand these strings back to git as pathspecs
    /// and `discard` partitions on an exact match.
    #[test]
    fn status_parsing_keeps_unusual_names_exactly_as_they_are() {
        // Quoted by plain porcelain for the space, octal-escaped for the
        // accent, and neither under `-z`.
        let out = "?? with space.txt\0?? café.txt\0 M dir/a b/c.rs\0?? --hard\0";
        let entries = parse_status(out);
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["with space.txt", "café.txt", "dir/a b/c.rs", "--hard"]);
        assert!(
            entries.iter().all(|e| !e.path.contains('"') && !e.path.contains('\\')),
            "no quoting or escaping survives into a pathspec: {paths:?}"
        );
    }

    /// Porcelain blame repeats a commit's details only once; every later
    /// line of the same commit must still get the remembered author.
    #[test]
    fn blame_parsing_fills_in_repeated_commits() {
        let out = "\
aaaa111122223333aaaa111122223333aaaa1111 1 1 2
author keith
author-time 1722000000
summary fix: the thing
\tfn one() {}
aaaa111122223333aaaa111122223333aaaa1111 2 2
\tfn two() {}
0000000000000000000000000000000000000000 3 3 1
author Not Committed Yet
author-time 1722000200
summary Version of src/x.rs from src/x.rs
\tfn three() {}
";
        let (lines, truncated) = parse_blame(out);
        assert!(!truncated);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].author, "keith");
        assert_eq!(lines[0].summary, "fix: the thing");
        assert_eq!(lines[1].author, "keith", "the repeat must inherit the details");
        assert_eq!(lines[1].summary, "fix: the thing");
        assert_eq!(lines[1].sha, lines[0].sha);
        assert_eq!(lines[2].author, "Not Committed Yet");
        assert!(lines[2].sha.chars().all(|c| c == '0'));
        assert_eq!(parse_blame("").0.len(), 0, "an empty file blames to nothing");
    }

    /// `-C` copies read as renames whose source survives.
    #[test]
    fn copies_read_as_renames() {
        let diff = "\
diff --git a/src/a.rs b/src/b.rs
similarity index 95%
copy from src/a.rs
copy to src/b.rs
--- a/src/a.rs
+++ b/src/b.rs
@@ -1,2 +1,2 @@
 x
+y
";
        let files = parse_unified(diff, &HashSet::new());
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].old_path.as_deref(), Some("src/a.rs"));
    }

    /// Reflog subjects are freeform; only the separators are trusted.
    #[test]
    fn reflog_parsing_reads_selector_and_summary() {
        let out = "abc123\x1fHEAD@{0}\x1fcheckout: moving from main to fix/x\x1e\n\
                   def456\x1fHEAD@{1}\x1fcommit: weird \"subject\" with \x7f\x1e";
        let entries = parse_reflog(out);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].selector, "HEAD@{0}");
        assert!(entries[1].summary.contains("weird"));
        assert!(parse_reflog("").is_empty());
    }

    /// Worktree porcelain: blocks with a path, a HEAD, and a branch — or
    /// no branch at all, which reads as detached.
    #[test]
    fn worktree_parsing_reads_blocks_and_detached() {
        let out = "worktree /home/k/repo\nHEAD aaaa111122223333aaaa111122223333aaaa1111\nbranch refs/heads/main\n\n\
                   worktree /home/k/.mogeung/worktrees/repo/xyz\nHEAD bbbb111122223333aaaa111122223333aaaa1111\ndetached\n";
        let w = parse_worktrees(out);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].path, "/home/k/repo");
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert_eq!(w[0].sha, "aaaa1111");
        assert_eq!(w[1].branch, None, "no branch line reads as detached");
        assert!(parse_worktrees("").is_empty());
    }

    /// Diff options: context is clamped, whitespace adds exactly `-w`.
    #[test]
    fn diff_opts_clamp_and_spell_their_args() {
        let d = DiffOpts::from_wire(None, None);
        assert_eq!(d.args(), vec!["--unified=3"]);
        let d = DiffOpts::from_wire(Some(10_000), Some(true));
        assert_eq!(d.args(), vec!["--unified=400", "-w"]);
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

    /// The one revision operator allowed is a trailing `^` — still nothing
    /// that could spell a flag or a range.
    #[test]
    fn revs_allow_exactly_one_trailing_parent_hat() {
        assert!(valid_rev("aaa111"));
        assert!(valid_rev("aaa111^"));
        assert!(!valid_rev("^aaa111"));
        assert!(!valid_rev("aaa111^^"));
        assert!(!valid_rev("aaa111..bbb222"));
        assert!(!valid_rev("--output=x^"));
        assert!(!valid_rev("^"));
    }

    /// The `-s` header record: multiline body preserved, empty body fine,
    /// annotated fields split only on the separator.
    #[test]
    fn commit_detail_keeps_the_body_multiline() {
        let out = "keith\x1fGitHub\x1f1722000000\x1f1722000100\x1fp1 p2\x1fHEAD -> main, tag: v1\x1f\
                   feat: the subject\n\nA body paragraph.\n\nAnother, with = signs and -- dashes.\n";
        let d = parse_commit_detail(out).unwrap();
        assert_eq!(d.author, "keith");
        assert_eq!(d.committer, "GitHub");
        assert_eq!(d.epoch, 1722000000);
        assert_eq!(d.commit_epoch, 1722000100);
        assert_eq!(d.parents, vec!["p1", "p2"]);
        assert_eq!(d.refs, vec!["HEAD -> main", "tag: v1"]);
        assert!(d.message.starts_with("feat: the subject\n\nA body paragraph."));
        assert!(d.message.ends_with("dashes."), "trailing newline trimmed");

        let bare = parse_commit_detail("a\x1fa\x1f0\x1f0\x1f\x1f\x1fsubject only").unwrap();
        assert_eq!(bare.message, "subject only");
        assert!(bare.parents.is_empty(), "a root commit has no parents");
        assert!(parse_commit_detail("").is_none(), "a truncated record degrades to no header");
    }

    /// `branch -a --contains` answers one name per line; a detached HEAD's
    /// parenthesised state line is not a branch. `R-D18`.
    #[test]
    fn containing_branches_drop_the_detached_state_line() {
        let out = "main\norigin/main\n(HEAD detached at abc1234)\n  feature/x\n\n";
        assert_eq!(
            parse_containing_branches(out),
            vec!["main", "origin/main", "feature/x"]
        );
        assert!(parse_containing_branches("").is_empty());
    }

    /// Filter text is joined with `=` so it cannot open a new argument;
    /// what dies here is the nonsense: control characters and novels.
    #[test]
    fn filter_text_refuses_control_characters_and_novels() {
        assert!(valid_filter_text("fix: the parser"));
        assert!(valid_filter_text("--looks-like-a-flag")); // harmless after --grep=
        assert!(!valid_filter_text(""));
        assert!(!valid_filter_text("a\x1bb"));
        assert!(!valid_filter_text("a\nb"));
        assert!(!valid_filter_text(&"a".repeat(201)));
    }

    /// Ref names widen the argument surface; hostile shapes die here.
    #[test]
    fn ref_names_that_could_be_arguments_are_refused() {
        assert!(valid_ref_name("main"));
        assert!(valid_ref_name("origin/main"));
        assert!(valid_ref_name("fix/issue-42_v1.2"));
        assert!(!valid_ref_name(""));
        assert!(!valid_ref_name("-D"));
        assert!(!valid_ref_name("--all"));
        assert!(!valid_ref_name("a..b"));
        assert!(!valid_ref_name("a@{1}"));
        assert!(!valid_ref_name(".hidden"));
        assert!(!valid_ref_name("/abs"));
        assert!(!valid_ref_name("trailing/"));
        assert!(!valid_ref_name("has space"));
        assert!(!valid_ref_name("semi;colon"));
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
