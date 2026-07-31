//! The write verbs, against real temporary repositories — roadmap `R-D19`.
//!
//! Every other git test in this tree reads a fixture. These cannot: the whole
//! question is what the repository looks like *afterwards*, and a verb that
//! stages nothing is indistinguishable from one that stages correctly until
//! you ask git. So each test builds a repo, acts on it, and asks git.
//!
//! [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md) is
//! the fence these live behind: the working tree and the local repository may
//! be written, a remote may not. Nothing here has a network in it, which is
//! also why they are free to run.

use mogeungd::git;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A repository with one commit, unless `empty` — some verbs behave differently
/// before the first commit, and that is a state real repositories pass through.
fn repo(tag: &str, empty: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mogeung-gitwrite-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git_in(&dir, &["init", "-q", "-b", "main"]);
    // Identity and signing are per-repo so the test never depends on, or
    // touches, whatever the machine running it has configured.
    git_in(&dir, &["config", "user.email", "t@example.com"]);
    git_in(&dir, &["config", "user.name", "t"]);
    git_in(&dir, &["config", "commit.gpgsign", "false"]);
    if !empty {
        std::fs::write(dir.join("kept.txt"), "one\n").unwrap();
        git_in(&dir, &["add", "kept.txt"]);
        git_in(&dir, &["commit", "-q", "-m", "first"]);
    }
    dir
}

fn status(dir: &Path) -> String {
    git_in(dir, &["status", "--porcelain"])
}

fn p(s: &str) -> Vec<String> {
    vec![s.to_string()]
}

#[test]
fn staging_moves_a_file_into_the_index() {
    let dir = repo("stage", false);
    std::fs::write(dir.join("new.txt"), "hello\n").unwrap();
    assert_eq!(status(&dir), "?? new.txt\n");

    git::stage(&dir, &p("new.txt")).unwrap();
    assert_eq!(status(&dir), "A  new.txt\n");

    std::fs::remove_dir_all(&dir).ok();
}

/// Staging a deletion is the case that catches an implementation using
/// `git add` on a path that is not there any more.
#[test]
fn staging_a_deleted_file_records_the_deletion() {
    let dir = repo("stage-del", false);
    std::fs::remove_file(dir.join("kept.txt")).unwrap();
    git::stage(&dir, &p("kept.txt")).unwrap();
    assert_eq!(status(&dir), "D  kept.txt\n");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn unstaging_leaves_the_working_tree_alone() {
    let dir = repo("unstage", false);
    std::fs::write(dir.join("kept.txt"), "one\ntwo\n").unwrap();
    git::stage(&dir, &p("kept.txt")).unwrap();
    assert_eq!(status(&dir), "M  kept.txt\n");

    git::unstage(&dir, &p("kept.txt")).unwrap();
    assert_eq!(status(&dir), " M kept.txt\n", "still modified, no longer staged");
    assert_eq!(
        std::fs::read_to_string(dir.join("kept.txt")).unwrap(),
        "one\ntwo\n",
        "unstage must never touch the file itself"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Before the first commit there is no HEAD to restore the index from, and
/// `git restore --staged` refuses. A repository someone has just `git init`-ed
/// is exactly where a first staging mistake happens.
#[test]
fn unstaging_works_in_a_repository_with_no_commits() {
    let dir = repo("unstage-empty", true);
    std::fs::write(dir.join("new.txt"), "hello\n").unwrap();
    git::stage(&dir, &p("new.txt")).unwrap();
    assert_eq!(status(&dir), "A  new.txt\n");

    git::unstage(&dir, &p("new.txt")).unwrap();
    assert_eq!(status(&dir), "?? new.txt\n", "untracked again, not deleted");
    assert!(dir.join("new.txt").exists(), "the file itself survives");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn discarding_a_tracked_file_restores_it_from_head() {
    let dir = repo("discard-tracked", false);
    std::fs::write(dir.join("kept.txt"), "ruined\n").unwrap();
    git::discard(&dir, &p("kept.txt")).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("kept.txt")).unwrap(), "one\n");
    assert_eq!(status(&dir), "", "nothing left to see");
    std::fs::remove_dir_all(&dir).ok();
}

/// Discarding a staged change has to clear the index too, or the file comes
/// back on disk while the old edit stays staged — a state nobody asked for and
/// the pane would render as a change that will not go away.
#[test]
fn discarding_clears_the_index_as_well_as_the_worktree() {
    let dir = repo("discard-staged", false);
    std::fs::write(dir.join("kept.txt"), "ruined\n").unwrap();
    git::stage(&dir, &p("kept.txt")).unwrap();
    git::discard(&dir, &p("kept.txt")).unwrap();
    assert_eq!(status(&dir), "");
    assert_eq!(std::fs::read_to_string(dir.join("kept.txt")).unwrap(), "one\n");
    std::fs::remove_dir_all(&dir).ok();
}

/// The destructive one. An untracked file has no version to restore, so
/// discarding it means deleting it — which is what the confirmation in the UI
/// has to say out loud.
#[test]
fn discarding_an_untracked_file_deletes_it() {
    let dir = repo("discard-untracked", false);
    std::fs::write(dir.join("scratch.txt"), "temp\n").unwrap();
    git::discard(&dir, &p("scratch.txt")).unwrap();
    assert!(!dir.join("scratch.txt").exists());
    assert_eq!(status(&dir), "");
    std::fs::remove_dir_all(&dir).ok();
}

/// Both kinds in one click, which is the normal case after an agent has run:
/// edits to files it knew about, plus files it invented.
#[test]
fn discarding_a_mixed_selection_handles_both_kinds() {
    let dir = repo("discard-mixed", false);
    std::fs::write(dir.join("kept.txt"), "ruined\n").unwrap();
    std::fs::write(dir.join("scratch.txt"), "temp\n").unwrap();
    git::discard(&dir, &["kept.txt".into(), "scratch.txt".into()]).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("kept.txt")).unwrap(), "one\n");
    assert!(!dir.join("scratch.txt").exists());
    assert_eq!(status(&dir), "");
    std::fs::remove_dir_all(&dir).ok();
}

/// A filename with a space in it, which plain porcelain quotes and `-z` does
/// not. `discard` partitions the selection by matching these strings against
/// `git status` output, so a name git spells differently on the way out lands
/// in the wrong half — the untracked file survives a discard that reported
/// success.
#[test]
fn a_name_with_a_space_is_discarded_like_any_other() {
    let dir = repo("spacey", false);
    std::fs::write(dir.join("a file.txt"), "temp\n").unwrap();
    std::fs::create_dir(dir.join("some dir")).unwrap();
    std::fs::write(dir.join("some dir/inner.txt"), "temp\n").unwrap();

    git::discard(&dir, &["a file.txt".into(), "some dir/inner.txt".into()]).unwrap();
    assert!(!dir.join("a file.txt").exists());
    assert!(!dir.join("some dir/inner.txt").exists());
    assert_eq!(status(&dir), "");

    std::fs::remove_dir_all(&dir).ok();
}

/// A file whose name starts with a dash is an option unless `--` separates it.
/// Agents produce stranger names than people do, and on a write verb the
/// failure is not a confusing message — it is a different command running.
#[test]
fn a_path_that_looks_like_a_flag_is_still_a_path() {
    let dir = repo("dashy", false);
    std::fs::write(dir.join("--hard"), "not a flag\n").unwrap();
    git::stage(&dir, &p("--hard")).unwrap();
    assert_eq!(status(&dir), "A  --hard\n");

    git::unstage(&dir, &p("--hard")).unwrap();
    git::discard(&dir, &p("--hard")).unwrap();
    assert!(!dir.join("--hard").exists());

    std::fs::remove_dir_all(&dir).ok();
}

/// Git's own words reach the caller. A paraphrase would throw away the list of
/// files, the hint, and everything else git says better than we would.
#[test]
fn a_refusal_arrives_in_gits_own_words() {
    let dir = repo("refusal", false);
    let e = git::stage(&dir, &p("nothing-here.txt")).unwrap_err().to_string();
    assert!(e.contains("nothing-here.txt"), "{e}");
    assert!(e.contains("did not match"), "git's sentence, not ours: {e}");
    assert!(!e.contains("failed:"), "no wrapper around it: {e}");
    std::fs::remove_dir_all(&dir).ok();
}

/// Staging nothing must not become staging everything. An empty selection is
/// what an unchecked list produces, and `git add --` with no pathspec is a
/// no-op rather than a wildcard — this pins that.
#[test]
fn an_empty_selection_changes_nothing() {
    let dir = repo("empty-sel", false);
    std::fs::write(dir.join("new.txt"), "hello\n").unwrap();
    git::stage(&dir, &[]).unwrap();
    assert_eq!(status(&dir), "?? new.txt\n", "still untracked");
    git::discard(&dir, &[]).unwrap();
    assert!(dir.join("new.txt").exists(), "discard of nothing destroys nothing");
    std::fs::remove_dir_all(&dir).ok();
}
