/**
 * The Console tab's ledger, through the store. `R-D29`.
 *
 * A git command is a row when it is sent; its answer closes the row; a daemon
 * error lands on the latest row still waiting. Pinned here rather than only
 * in `lib/gitConsole.test.ts` because the wiring is the part a refactor of
 * `send` or `ingest` would lose quietly.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";

describe("the git console", () => {
  beforeEach(() => useStore.setState({ git: {} }));

  it("records a command as it is sent, and its answer as it lands", () => {
    const { send, ingest } = useStore.getState();
    send({ cmd: "git_log", session_id: "s1", skip: 0, limit: 100, all: true });
    let rows = useStore.getState().git.s1.console;
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ cmd: "git_log", args: "skip 0 · limit 100 · all", result: null, error: null });

    ingest({ ev: "git_commits", session_id: "s1", skip: 0, commits: [], done: true, rev: null, grep: null, author: null, path: null, pickaxe: null } as never);
    rows = useStore.getState().git.s1.console;
    expect(rows[0].result).toBe("0 commits — the end of history");
  });

  it("puts a refusal on the latest waiting command, whichever session sent it", () => {
    const { send, ingest } = useStore.getState();
    send({ cmd: "git_show", session_id: "s1", sha: "aaaa" });
    send({ cmd: "git_show", session_id: "s2", sha: "bbbb" });
    ingest({ ev: "error", message: "that is not a commit sha" } as never);
    expect(useStore.getState().git.s2.console[0].error).toBe("that is not a commit sha");
    expect(useStore.getState().git.s1.console[0].error).toBeNull();
  });

  it("ignores commands that are not git's", () => {
    useStore.getState().send({ cmd: "note_list" } as never);
    expect(useStore.getState().git).toEqual({});
  });
});
