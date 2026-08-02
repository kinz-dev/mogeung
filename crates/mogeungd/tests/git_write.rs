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

// -- Commit. `R-D20`. ---------------------------------------------------------

fn log_subjects(dir: &Path) -> Vec<String> {
    git_in(dir, &["log", "--format=%s"])
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn committing_records_only_what_was_staged() {
    let dir = repo("commit", false);
    std::fs::write(dir.join("staged.txt"), "in\n").unwrap();
    std::fs::write(dir.join("loose.txt"), "out\n").unwrap();
    git::stage(&dir, &p("staged.txt")).unwrap();

    let sha = git::commit(&dir, "add the staged one", false, &[]).unwrap();

    assert_eq!(log_subjects(&dir), ["add the staged one", "first"]);
    assert_eq!(sha.len(), 40, "the new sha comes back: {sha}");
    // The unstaged file is untouched and still uncommitted — the checkboxes
    // are the instruction, not a suggestion.
    assert_eq!(status(&dir), "?? loose.txt\n");
    let named = git_in(&dir, &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(named.trim(), "staged.txt");

    std::fs::remove_dir_all(&dir).ok();
}

/// The distinctive part, and the reason this is worth building rather than
/// shelling out: a terminal cannot know which session produced the work.
/// `R-F2` prompt-blame reads this back.
#[test]
fn the_session_trailer_lands_where_git_puts_trailers() {
    let dir = repo("trailer", false);
    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    git::stage(&dir, &p("a.txt")).unwrap();
    git::commit(
        &dir,
        "subject line\n\nA body paragraph.",
        false,
        &[(git::SESSION_TRAILER.into(), "sess-abc-123".into())],
    )
    .unwrap();

    let body = git_in(&dir, &["log", "-1", "--format=%B"]);
    assert!(body.contains("subject line"), "{body}");
    assert!(body.contains("A body paragraph."), "the body survives: {body}");
    assert!(
        body.contains("Mogeung-Session: sess-abc-123"),
        "trailer formatted by git, not spliced by us: {body}"
    );
    // Parseable as a trailer by anything that speaks the convention, which is
    // what makes it useful to a tool that has never heard of mogeung.
    let parsed = git_in(&dir, &["log", "-1", "--format=%(trailers:key=Mogeung-Session,valueonly)"]);
    assert_eq!(parsed.trim(), "sess-abc-123");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn amending_replaces_the_tip_rather_than_adding_to_it() {
    let dir = repo("amend", false);
    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    git::stage(&dir, &p("a.txt")).unwrap();
    git::commit(&dir, "first try", false, &[]).unwrap();
    assert_eq!(log_subjects(&dir), ["first try", "first"]);

    std::fs::write(dir.join("b.txt"), "y\n").unwrap();
    git::stage(&dir, &p("b.txt")).unwrap();
    git::commit(&dir, "second thoughts", true, &[]).unwrap();

    assert_eq!(log_subjects(&dir), ["second thoughts", "first"], "no new commit");
    let named = git_in(&dir, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(named.contains("a.txt") && named.contains("b.txt"), "{named}");

    std::fs::remove_dir_all(&dir).ok();
}

/// Amending in a repository with no commits: git refuses, and its refusal is
/// the sentence worth showing.
#[test]
fn amending_with_nothing_to_amend_is_gits_refusal() {
    let dir = repo("amend-empty", true);
    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    git::stage(&dir, &p("a.txt")).unwrap();
    let e = git::commit(&dir, "nope", true, &[]).unwrap_err().to_string();
    assert!(e.to_lowercase().contains("amend"), "git's words: {e}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A message of only whitespace passes git's own empty check and produces a
/// commit with a blank subject — one of the few places worth refusing ahead
/// of git rather than after it.
#[test]
fn a_blank_message_is_refused_before_git_sees_it() {
    let dir = repo("blank-msg", false);
    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    git::stage(&dir, &p("a.txt")).unwrap();

    for blank in ["", "   ", "\n\n", " \t\n "] {
        let e = git::commit(&dir, blank, false, &[]).unwrap_err().to_string();
        assert!(e.contains("needs a message"), "{blank:?} → {e}");
    }
    assert_eq!(log_subjects(&dir), ["first"], "nothing was committed");

    std::fs::remove_dir_all(&dir).ok();
}

/// Nothing staged is git's call, not ours — "no changes added to commit" is
/// more useful than anything we would write, and the pane shows it verbatim.
#[test]
fn committing_nothing_is_gits_refusal() {
    let dir = repo("nothing-staged", false);
    let e = git::commit(&dir, "empty", false, &[]).unwrap_err().to_string();
    assert!(
        e.contains("nothing to commit") || e.contains("no changes added"),
        "git's words: {e}"
    );
    assert_eq!(log_subjects(&dir), ["first"]);
    std::fs::remove_dir_all(&dir).ok();
}

/// A message starting with a dash must not be read as an option. `-m` takes
/// the next argument whatever it looks like, and this pins that it stays so.
#[test]
fn a_message_that_looks_like_a_flag_is_still_a_message() {
    let dir = repo("dashy-msg", false);
    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    git::stage(&dir, &p("a.txt")).unwrap();
    git::commit(&dir, "--amend all the things", false, &[]).unwrap();
    assert_eq!(log_subjects(&dir), ["--amend all the things", "first"]);
    std::fs::remove_dir_all(&dir).ok();
}

/// A pre-commit hook that refuses must stop the commit, and say why. Skipping
/// hooks would mean a repository that rejects bad commits everywhere except
/// from this window.
#[test]
fn a_hook_that_refuses_stops_the_commit() {
    let dir = repo("hook", false);
    let hooks = dir.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'the hook says no' >&2\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    git::stage(&dir, &p("a.txt")).unwrap();
    let e = git::commit(&dir, "should not land", false, &[]).unwrap_err().to_string();
    assert!(e.contains("the hook says no"), "the hook's own words: {e}");
    assert_eq!(log_subjects(&dir), ["first"], "nothing was committed");

    std::fs::remove_dir_all(&dir).ok();
}

/// A hook that reads stdin would block a daemon thread for ever on a question
/// nobody can see, because a daemon has no terminal. `run_git_write` gives it
/// `/dev/null`, so it gets EOF and the commit fails loudly instead of hanging.
#[test]
fn a_hook_that_prompts_fails_instead_of_hanging() {
    let dir = repo("hook-prompt", false);
    let hooks = dir.join(".git/hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\nread answer || exit 1\n[ \"$answer\" = y ] || exit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    git::stage(&dir, &p("a.txt")).unwrap();
    // The assertion that matters is that this returns at all.
    assert!(git::commit(&dir, "should not hang", false, &[]).is_err());
    assert_eq!(log_subjects(&dir), ["first"]);

    std::fs::remove_dir_all(&dir).ok();
}

// -- Branches and stashes. `R-D21`. -------------------------------------------

fn branch_now(dir: &Path) -> String {
    git_in(dir, &["rev-parse", "--abbrev-ref", "HEAD"]).trim().to_string()
}

#[test]
fn creating_a_branch_can_leave_you_where_you_were() {
    let dir = repo("branch-create", false);
    git::branch_create(&dir, "feature/a", false).unwrap();
    assert_eq!(branch_now(&dir), "main", "created, not switched to");
    assert!(git_in(&dir, &["branch", "--list"]).contains("feature/a"));

    git::branch_create(&dir, "feature/b", true).unwrap();
    assert_eq!(branch_now(&dir), "feature/b", "created and switched to");

    std::fs::remove_dir_all(&dir).ok();
}

/// `switch -c` reads a leading-dash name as flags and answers "unknown switch
/// `e'". Creating with `branch` first means the user gets git's real sentence
/// about branch names — and our own check refuses it before either.
#[test]
fn a_branch_name_that_could_be_a_flag_is_refused_with_a_reason() {
    let dir = repo("branch-evil", false);
    for bad in ["-evil", "--force", "..", "a..b", "main@{1}", "/leading", "trailing/", ""] {
        let e = git::branch_create(&dir, bad, false).unwrap_err().to_string();
        assert!(
            e.contains("not a branch name mogeung will use"),
            "{bad:?} → {e}"
        );
    }
    // And the ordinary ones still work, including slashes and dots.
    for good in ["feature/x", "release-1.2", "user_name/thing"] {
        git::branch_create(&dir, good, false).unwrap();
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn switching_moves_the_worktree() {
    let dir = repo("switch", false);
    git::branch_create(&dir, "other", false).unwrap();
    git::switch(&dir, "other").unwrap();
    assert_eq!(branch_now(&dir), "other");
    git::switch(&dir, "main").unwrap();
    assert_eq!(branch_now(&dir), "main");
    std::fs::remove_dir_all(&dir).ok();
}

/// Git refuses a switch that would lose work, and its refusal names the files.
/// mogeung must not paper over that with a sentence of its own.
#[test]
fn a_switch_that_would_lose_work_is_gits_refusal() {
    let dir = repo("switch-dirty", false);
    // A commit on `other` that touches the same file, so switching back with
    // local edits would have to overwrite them.
    git::branch_create(&dir, "other", true).unwrap();
    std::fs::write(dir.join("kept.txt"), "from other\n").unwrap();
    git::stage(&dir, &p("kept.txt")).unwrap();
    git::commit(&dir, "other's version", false, &[]).unwrap();
    git::switch(&dir, "main").unwrap();

    std::fs::write(dir.join("kept.txt"), "uncommitted edit\n").unwrap();
    let e = git::switch(&dir, "other").unwrap_err().to_string();
    assert!(e.contains("kept.txt"), "git names the file: {e}");
    assert_eq!(branch_now(&dir), "main", "and did not move");
    assert_eq!(
        std::fs::read_to_string(dir.join("kept.txt")).unwrap(),
        "uncommitted edit\n",
        "the edit survives"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_stash_round_trips() {
    let dir = repo("stash", false);
    std::fs::write(dir.join("kept.txt"), "work in progress\n").unwrap();
    git::stash_push(&dir, "wip", false).unwrap();

    assert_eq!(status(&dir), "", "the tree is clean again");
    assert_eq!(std::fs::read_to_string(dir.join("kept.txt")).unwrap(), "one\n");
    assert!(git_in(&dir, &["stash", "list"]).contains("wip"));

    git::stash_pop(&dir, 0).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("kept.txt")).unwrap(),
        "work in progress\n"
    );
    assert_eq!(git_in(&dir, &["stash", "list"]).trim(), "", "popping removes it");

    std::fs::remove_dir_all(&dir).ok();
}

/// Untracked files are left behind by a plain stash, which is a surprise
/// worth pinning: the tree is *not* clean afterwards.
#[test]
fn untracked_files_stash_only_when_asked() {
    let dir = repo("stash-untracked", false);
    std::fs::write(dir.join("new.txt"), "fresh\n").unwrap();

    git::stash_push(&dir, "without", false).unwrap();
    assert_eq!(status(&dir), "?? new.txt\n", "left behind");

    git::stash_push(&dir, "with", true).unwrap();
    assert_eq!(status(&dir), "", "taken along");
    git::stash_pop(&dir, 0).unwrap();
    assert!(dir.join("new.txt").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dropping_a_stash_removes_the_right_one() {
    let dir = repo("stash-drop", false);
    for n in ["first", "second", "third"] {
        std::fs::write(dir.join("kept.txt"), format!("{n}\n")).unwrap();
        git::stash_push(&dir, n, false).unwrap();
    }
    // Newest is stash@{0}, so index 1 is "second".
    git::stash_drop(&dir, 1).unwrap();
    let list = git_in(&dir, &["stash", "list"]);
    assert!(list.contains("third") && list.contains("first"), "{list}");
    assert!(!list.contains("second"), "{list}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn popping_a_stash_that_is_not_there_is_gits_refusal() {
    let dir = repo("stash-missing", false);
    let e = git::stash_pop(&dir, 7).unwrap_err().to_string();
    assert!(e.to_lowercase().contains("stash"), "git's words: {e}");
    std::fs::remove_dir_all(&dir).ok();
}

// -- Conflict resolution. `R-D22`. --------------------------------------------

/// A repo mid-merge, with `conflicted.txt` unresolvable by git.
fn conflicted(tag: &str) -> PathBuf {
    let dir = repo(tag, false);
    std::fs::write(dir.join("conflicted.txt"), "base\n").unwrap();
    git_in(&dir, &["add", "conflicted.txt"]);
    git_in(&dir, &["commit", "-q", "-m", "base"]);

    git_in(&dir, &["switch", "-q", "-c", "theirs"]);
    std::fs::write(dir.join("conflicted.txt"), "their version\n").unwrap();
    git_in(&dir, &["commit", "-q", "-am", "theirs"]);

    git_in(&dir, &["switch", "-q", "main"]);
    std::fs::write(dir.join("conflicted.txt"), "our version\n").unwrap();
    git_in(&dir, &["commit", "-q", "-am", "ours"]);

    // Expected to fail — that is the point.
    let _ = Command::new("git")
        .args(["merge", "theirs"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        status(&dir).contains("UU conflicted.txt"),
        "the fixture must actually conflict: {}",
        status(&dir)
    );
    dir
}

#[test]
fn taking_ours_keeps_this_branchs_version_and_stages_it() {
    let dir = conflicted("resolve-ours");
    git::resolve(&dir, "conflicted.txt", git::Side::Ours).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("conflicted.txt")).unwrap(),
        "our version\n"
    );
    // Empty, not `M `: taking ours reproduces HEAD exactly, so once the index
    // is no longer unmerged there is nothing left that differs from it. The
    // property that matters is that the conflict is gone.
    assert!(!status(&dir).contains("UU"), "still unmerged: {}", status(&dir));
    assert_eq!(
        git_in(&dir, &["diff", "--name-only", "--diff-filter=U"]).trim(),
        "",
        "no unmerged paths remain"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn taking_theirs_keeps_the_incoming_version_and_stages_it() {
    let dir = conflicted("resolve-theirs");
    git::resolve(&dir, "conflicted.txt", git::Side::Theirs).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("conflicted.txt")).unwrap(),
        "their version\n"
    );
    assert_eq!(status(&dir), "M  conflicted.txt\n");
    std::fs::remove_dir_all(&dir).ok();
}

/// Resolved by hand elsewhere: the file on disk is already right, and all
/// that is missing is telling git so.
#[test]
fn marking_resolved_stages_whatever_is_on_disk() {
    let dir = conflicted("resolve-mine");
    std::fs::write(dir.join("conflicted.txt"), "a blend of both\n").unwrap();
    git::resolve(&dir, "conflicted.txt", git::Side::Mine).unwrap();
    assert_eq!(status(&dir), "M  conflicted.txt\n");
    assert_eq!(
        std::fs::read_to_string(dir.join("conflicted.txt")).unwrap(),
        "a blend of both\n",
        "the hand-made resolution is untouched"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// In git a conflict is resolved by *staging* the result. A verb that wrote
/// the file but left the index unmerged would show a conflict that looks
/// fixed and is not — and `git commit` would still refuse.
#[test]
fn resolving_leaves_the_merge_committable() {
    let dir = conflicted("resolve-commit");
    git::resolve(&dir, "conflicted.txt", git::Side::Ours).unwrap();
    git::commit(&dir, "take ours", false, &[]).unwrap();
    assert_eq!(status(&dir), "", "the merge is done");
    assert!(
        !std::fs::read_to_string(dir.join("conflicted.txt"))
            .unwrap()
            .contains("<<<<<<<"),
        "no markers survived into the commit"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Markers left in the file are committed happily once the index says
/// resolved — which is exactly why `Mine` stages what is on disk rather than
/// checking it. Pinned so nobody later mistakes that for a bug and "fixes" it
/// into a validator that would refuse legitimate content.
#[test]
fn marking_resolved_does_not_inspect_the_content() {
    let dir = conflicted("resolve-markers");
    let with_markers = std::fs::read_to_string(dir.join("conflicted.txt")).unwrap();
    assert!(with_markers.contains("<<<<<<<"));
    git::resolve(&dir, "conflicted.txt", git::Side::Mine).unwrap();
    assert_eq!(status(&dir), "M  conflicted.txt\n");
    std::fs::remove_dir_all(&dir).ok();
}

// -- Fetch. `R-D25`, ADR-0014. ------------------------------------------------

/// A repo with a real remote — a second local repository, so the test needs no
/// network and still exercises the actual code path.
fn with_remote(tag: &str) -> (PathBuf, PathBuf) {
    let upstream = repo(&format!("{tag}-up"), false);
    let dir = std::env::temp_dir().join(format!("mogeung-gitwrite-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = Command::new("git")
        .args(["clone", "-q", upstream.to_str().unwrap(), dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    git_in(&dir, &["config", "user.email", "t@example.com"]);
    git_in(&dir, &["config", "user.name", "t"]);
    (dir, upstream)
}

#[test]
fn fetching_with_nothing_new_says_so_rather_than_nothing() {
    let (dir, up) = with_remote("fetch-quiet");
    let r = git::fetch(&dir).unwrap();
    assert!(r.updates.is_empty(), "{:?}", r.updates);
    assert_eq!((r.ahead, r.behind), (0, 0));
    assert!(r.upstream.is_some(), "a clone tracks its origin");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&up).ok();
}

/// The case that prompted ADR-0014: the remote moved and the local ref did
/// not, so `behind` was zero and wrong until something fetched.
#[test]
fn fetching_learns_how_far_behind_you_are() {
    let (dir, up) = with_remote("fetch-behind");
    for n in ["second", "third"] {
        std::fs::write(up.join(format!("{n}.txt")), "x\n").unwrap();
        git_in(&up, &["add", "-A"]);
        git_in(&up, &["commit", "-q", "-m", n]);
    }
    // Before the fetch, git itself believes nothing has changed.
    let stale = git_in(&dir, &["rev-list", "--count", "@{upstream}..HEAD"]);
    assert_eq!(stale.trim(), "0");

    let r = git::fetch(&dir).unwrap();
    assert_eq!(r.behind, 2, "the two upstream commits are now visible");
    assert_eq!(r.ahead, 0);
    assert!(
        r.updates.iter().any(|l| l.contains("main")),
        "git named the ref it moved: {:?}",
        r.updates
    );

    // And it fetched only: the working tree was not merged into.
    assert!(!dir.join("second.txt").exists(), "fetch must never merge");

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&up).ok();
}

#[test]
fn fetching_reports_both_directions_when_diverged() {
    let (dir, up) = with_remote("fetch-diverged");
    std::fs::write(up.join("theirs.txt"), "x\n").unwrap();
    git_in(&up, &["add", "-A"]);
    git_in(&up, &["commit", "-q", "-m", "theirs"]);

    std::fs::write(dir.join("mine.txt"), "y\n").unwrap();
    git::stage(&dir, &p("mine.txt")).unwrap();
    git::commit(&dir, "mine", false, &[]).unwrap();

    let r = git::fetch(&dir).unwrap();
    assert_eq!((r.ahead, r.behind), (1, 1), "diverged both ways");

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&up).ok();
}

/// A remote that is not there must fail loudly and quickly. The constraint
/// that matters is the one this cannot assert directly — that it *returns*
/// rather than sitting on a credential prompt for ever.
#[test]
fn a_dead_remote_fails_instead_of_hanging() {
    let dir = repo("fetch-dead", false);
    git_in(
        &dir,
        &["remote", "add", "origin", "/nonexistent/path/to/nowhere.git"],
    );
    let e = git::fetch(&dir).unwrap_err().to_string();
    assert!(!e.is_empty(), "git said something");
    std::fs::remove_dir_all(&dir).ok();
}

/// A branch with no upstream is an ordinary state, not a failure, and the
/// report has to distinguish "0 behind" from "there is nothing to be behind".
#[test]
fn a_branch_with_no_upstream_reports_no_upstream() {
    let dir = repo("fetch-no-upstream", false);
    let r = git::fetch(&dir).unwrap();
    assert!(r.upstream.is_none());
    assert_eq!((r.ahead, r.behind), (0, 0));
    std::fs::remove_dir_all(&dir).ok();
}
