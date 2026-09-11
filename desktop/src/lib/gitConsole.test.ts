import { describe, expect, it } from "vitest";
import { answerFor, attachError, attachResult, CONSOLE_CAP, consoleText, describeCmd, gitSessionOf, pushCmd } from "@/lib/gitConsole";
import type { ClientMsg, ServerMsg } from "@/wire/types";

describe("describeCmd", () => {
  it("names the arguments, drops the session and the nulls, and says a flag by its name", () => {
    const msg: ClientMsg = { cmd: "git_log", session_id: "s1", skip: 0, limit: 100, rev: null, grep: "PROJ-1", author: null, path: null, pickaxe: null, all: true, since: null, until: null };
    expect(describeCmd(msg)).toBe("skip 0 · limit 100 · grep PROJ-1 · all");
    expect(describeCmd({ cmd: "git_stage", session_id: "s1", paths: ["a.rs"] })).toBe("paths a.rs");
    expect(describeCmd({ cmd: "git_stage", session_id: "s1", paths: ["a.rs", "b.rs"] })).toBe("paths 2 items");
    expect(describeCmd({ cmd: "git_refs", session_id: "s1" })).toBe("");
  });
});

describe("gitSessionOf", () => {
  it("is the session of a git command or answer, and nothing else", () => {
    expect(gitSessionOf({ cmd: "git_refs", session_id: "s1" })).toBe("s1");
    expect(gitSessionOf({ ev: "git_commits", session_id: "s1" })).toBe("s1");
    expect(gitSessionOf({ cmd: "note_list" })).toBeNull();
    expect(gitSessionOf({ ev: "error" })).toBeNull();
  });
});

describe("the ledger", () => {
  const log: ClientMsg = { cmd: "git_log", session_id: "s1", skip: 0, limit: 100 };
  const show: ClientMsg = { cmd: "git_show", session_id: "s1", sha: "fa64b61" };

  it("closes the latest waiting row of the command an answer answers", () => {
    let rows = pushCmd([], log, 1);
    rows = pushCmd(rows, show, 2);
    rows = pushCmd(rows, log, 3);
    const a = answerFor({ ev: "git_commits", session_id: "s1", skip: 0, commits: [], done: true } as ServerMsg)!;
    rows = attachResult(rows, a.cmds, a.summary);
    expect(rows.map((r) => r.result)).toEqual([null, null, "0 commits — the end of history"]);
    const b = answerFor({ ev: "git_commit_diff", session_id: "s1", sha: "fa64b61", files: [], detail: null } as ServerMsg)!;
    rows = attachResult(rows, b.cmds, b.summary);
    expect(rows[1].result).toBe("0 files · +0 −0 · no header");
    // An answer nobody is waiting for changes nothing.
    expect(attachResult(rows, ["git_refs"], "x")).toEqual(rows);
  });

  it("lands a refusal on the latest waiting row, whichever it was", () => {
    let rows = pushCmd([], log, 1);
    rows = pushCmd(rows, show, 2);
    rows = attachError(rows, "that is not a commit sha");
    expect(rows[1].error).toBe("that is not a commit sha");
    expect(rows[0].error).toBeNull();
    rows = attachError(rows, "and again");
    expect(rows[0].error).toBe("and again");
  });

  it("closes a write verb with the status it re-broadcasts", () => {
    let rows = pushCmd([], { cmd: "git_stage", session_id: "s1", paths: ["a"] }, 1);
    const a = answerFor({ ev: "git_local_changes", session_id: "s1", entries: [] } as unknown as ServerMsg)!;
    rows = attachResult(rows, a.cmds, a.summary);
    expect(rows[0].result).toBe("0 entries");
  });

  it("keeps the cap", () => {
    let rows: ReturnType<typeof pushCmd> = [];
    for (let i = 0; i < CONSOLE_CAP + 5; i++) rows = pushCmd(rows, log, i);
    expect(rows).toHaveLength(CONSOLE_CAP);
    expect(rows[0].at).toBe(5);
  });

  it("reads as text", () => {
    const rows = attachError(pushCmd([], show, 0), "no");
    expect(consoleText(rows)).toBe("00:00:00  git_show sha fa64b61\n        error: no");
  });
});
