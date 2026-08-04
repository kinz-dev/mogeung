/**
 * The git replies that had nowhere to land.
 *
 * `git_conflict_stages` arrived at a `case` whose whole body was `break` — the
 * daemon answered, the client dropped it, and the pane showed the empty state
 * for ever. These pin the arrival of each answer *and* what it clears, because
 * the second half is what stops a conflict view and a commit diff being on
 * screen at the same time claiming to describe the same file.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";
import type { CommitDetail, FileChange } from "@/wire/types";

const SESSION = "s1";
const files: FileChange[] = [];

describe("the git answers", () => {
  beforeEach(() => useStore.setState({ git: {} }));

  it("keeps the three stages of a conflicted file", () => {
    useStore.getState().ingest({
      ev: "git_conflict_stages",
      session_id: SESSION,
      path: "src/main.rs",
      base: "b",
      ours: "o",
      theirs: "t",
      truncated: false,
    });
    expect(useStore.getState().git[SESSION].conflict).toEqual({
      path: "src/main.rs",
      base: "b",
      ours: "o",
      theirs: "t",
      truncated: false,
    });
  });

  it("labels a range diff with the two ends it compared", () => {
    useStore.getState().ingest({
      ev: "git_range_diff",
      session_id: SESSION,
      from: "aaaaaaaaaaaa",
      to: "bbbbbbbbbbbb",
      files,
    });
    expect(useStore.getState().git[SESSION].diffLabel).toBe("aaaaaaaa..bbbbbbbb");
  });

  /**
   * A commit's message header belongs to that commit. Leaving it above a range
   * diff would attribute the range to whatever you last clicked.
   */
  it("clears the commit header and any conflict when a range arrives", () => {
    useStore.getState().patchGit(SESSION, {
      detail: {
        author: "a",
        committer: "a",
        epoch: 0,
        commit_epoch: 0,
        parents: [],
        refs: [],
        message: "old",
        branches: [],
      } as CommitDetail,
      conflict: { path: "p", base: "", ours: "", theirs: "", truncated: false },
    });
    useStore.getState().ingest({
      ev: "git_range_diff",
      session_id: SESSION,
      from: "a",
      to: "b",
      files,
    });
    const git = useStore.getState().git[SESSION];
    expect(git.detail).toBeNull();
    expect(git.conflict).toBeNull();
  });

  it("clears a conflict when a stash diff arrives", () => {
    useStore.getState().patchGit(SESSION, {
      conflict: { path: "p", base: "", ours: "", theirs: "", truncated: false },
    });
    useStore.getState().ingest({ ev: "git_stash_diff", session_id: SESSION, index: 0, files });
    expect(useStore.getState().git[SESSION].conflict).toBeNull();
  });
});
