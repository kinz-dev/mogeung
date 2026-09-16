/**
 * The operations, as rules. `R-D31`.
 *
 * The popup is a list and a keyboard; everything that can be *wrong* about it
 * is here — which digit runs what, what refuses and in whose words, and where
 * each entry actually sends you. All of it is assertable without a DOM, which
 * is the argument for the list being a module at all.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { GIT_OPS, opForDigit, opsContext, targetPath, type OpsContext } from "@/lib/gitOps";
import { useStore, emptyGit } from "@/store";
import type { ClientMsg, Session } from "@/wire/types";

const sent: ClientMsg[] = [];

const ctx = (over: Partial<OpsContext> = {}): OpsContext => ({
  session: "s1",
  repoRoot: "/repo",
  selectedPath: null,
  filePath: null,
  head: "main",
  ...over,
});

const session = (over: Partial<Session> = {}): Session =>
  ({ id: "s1", cwd: "/repo", repo_root: "/repo", title: "the session", alive: true, ...over }) as unknown as Session;

const op = (id: string) => GIT_OPS.find((o) => o.id === id)!;

beforeEach(() => {
  sent.length = 0;
  useStore.setState({
    selected: "s1",
    sessions: { s1: session() },
    git: { s1: { ...emptyGit() } },
    gitTab: "log",
    gitMore: "reflog",
    gitPopup: null,
    commitFocus: 0,
    activePane: null,
    notices: [],
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
});

describe("the numbered list", () => {
  /**
   * The numbering *is* the feature: `Alt+\`` then `1` is two keystrokes with
   * no reading in between, and that only holds while the digits mean what they
   * meant yesterday. IntelliJ's own order, which is what the ask showed.
   */
  it("numbers the entries the way the popup it came from does", () => {
    expect(GIT_OPS.filter((o) => o.digit).map((o) => [o.digit, o.id])).toEqual([
      ["1", "commit"],
      ["2", "commit_file"],
      ["3", "rollback"],
      ["4", "history"],
      ["5", "annotate"],
      ["6", "diff"],
      ["7", "branches"],
      ["8", "push"],
      ["9", "stash"],
      ["0", "unstash"],
    ]);
  });

  it("gives every digit exactly one entry, and the rest none", () => {
    const digits = GIT_OPS.map((o) => o.digit).filter(Boolean);
    expect(new Set(digits).size).toBe(digits.length);
    expect(opForDigit("7")?.id).toBe("branches");
    expect(opForDigit("4")?.id).toBe("history");
    expect(opForDigit("x")).toBeUndefined();
  });
});

describe("what refuses, and why", () => {
  it("says which of the two things is missing — a session, or a repository", () => {
    expect(op("commit").unavailable(ctx({ session: null }))).toMatch(/no session/);
    expect(op("commit").unavailable(ctx({ repoRoot: null }))).toMatch(/not in a git repository/);
    expect(op("commit").unavailable(ctx())).toBeNull();
  });

  /**
   * The one entry that is unavailable in a perfect repository, and the reason
   * it is drawn at all: an IntelliJ user will press `8`.
   * [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md).
   */
  it("refuses push always, in the product's words rather than the situation's", () => {
    for (const c of [ctx(), ctx({ session: null }), ctx({ repoRoot: null })]) {
      expect(op("push").unavailable(c)).toMatch(/never publishes/);
    }
    expect(op("push").digit).toBe("8");
  });

  it("wants a file for the three entries that act on one", () => {
    for (const id of ["commit_file", "rollback", "annotate"]) {
      expect(op(id).unavailable(ctx()), id).toMatch(/no file is selected/);
      expect(op(id).unavailable(ctx({ selectedPath: "a.rs" })), id).toBeNull();
      expect(op(id).unavailable(ctx({ filePath: "b.rs" })), id).toBeNull();
    }
  });

  /** Git's selection wins: it is the more deliberate of the two. */
  it("prefers the file Local changes has selected over the one you are reading", () => {
    expect(targetPath(ctx({ selectedPath: "a.rs", filePath: "b.rs" }))).toBe("a.rs");
    expect(targetPath(ctx({ filePath: "b.rs" }))).toBe("b.rs");
    expect(targetPath(ctx())).toBeNull();
  });

  it("wants a branch before it will copy one", () => {
    expect(op("copy_branch").unavailable(ctx({ head: null }))).toMatch(/no branch/);
    expect(op("copy_branch").unavailable(ctx())).toBeNull();
  });
});

describe("where each entry sends you", () => {
  it("opens the tool window on the tab the verb lives on", () => {
    op("commit").run(ctx());
    expect(useStore.getState().gitTab).toBe("local");
    expect(useStore.getState().prefs.dock).toBe("git");
    expect(useStore.getState().commitFocus).toBe(1);

    op("stash").run(ctx());
    expect(useStore.getState().gitTab).toBe("stash");

    op("worktrees").run(ctx());
    expect(useStore.getState().gitTab).toBe("more");
    expect(useStore.getState().gitMore).toBe("worktrees");

    op("reflog").run(ctx());
    expect(useStore.getState().gitMore).toBe("reflog");
  });

  /** *Commit File…* is two verbs in git, and the popup owes you both. */
  it("stages the file before asking for a commit message", () => {
    op("commit_file").run(ctx({ selectedPath: "a.rs" }));
    expect(sent[0]).toEqual({ cmd: "git_stage", session_id: "s1", paths: ["a.rs"] });
    expect(useStore.getState().gitTab).toBe("local");
    expect(useStore.getState().commitFocus).toBe(1);
  });

  /**
   * `R-D19`'s rule is that a discard names the file first. The popup asks the
   * caller for a confirmation rather than sending anything — the assertion is
   * that **nothing goes on the wire** until the answer comes back.
   */
  it("sends no discard until its confirmation has been answered", () => {
    const asked = op("rollback").run(ctx({ selectedPath: "a.rs" }));
    expect(asked?.ask).toBe("confirm");
    expect(sent).toEqual([]);
    if (asked?.ask === "confirm") {
      expect(asked.body).toContain("a.rs");
      asked.then();
    }
    expect(sent).toEqual([{ cmd: "git_discard", session_id: "s1", paths: ["a.rs"] }]);
  });

  it("scopes the log to the file for history, and clears the scope without one", () => {
    op("history").run(ctx({ selectedPath: "src/a.rs" }));
    expect(sent.at(-1)).toMatchObject({ cmd: "git_log", session_id: "s1", path: "src/a.rs" });
    expect(useStore.getState().gitTab).toBe("log");

    op("history").run(ctx());
    expect(sent.at(-1)).toMatchObject({ cmd: "git_log", path: null });
  });

  /** Blame is a per-path toggle now, and turning it on has to *show* you it. */
  it("toggles the blame gutter for the file, by path", () => {
    op("annotate").run(ctx({ selectedPath: "a.rs" }));
    expect(useStore.getState().scoped().editorBlame).toEqual(["a.rs"]);
    op("annotate").run(ctx({ selectedPath: "a.rs" }));
    expect(useStore.getState().scoped().editorBlame).toEqual([]);
  });

  it("asks the popup for the branches list rather than doing anything itself", () => {
    expect(op("branches").run(ctx())).toEqual({ ask: "branches" });
    expect(sent).toEqual([]);
  });

  it("fetches without merging anything, which is the only outbound call there is", () => {
    op("fetch").run(ctx());
    expect(sent).toEqual([{ cmd: "git_fetch", session_id: "s1" }]);
  });

  it("does nothing at all for push", () => {
    op("push").run(ctx());
    expect(sent).toEqual([]);
  });
});

describe("the context it reads", () => {
  it("takes the repository and the file from the selected session", () => {
    useStore.setState({
      git: { s1: { ...emptyGit(), selectedPath: "a.rs", refs: { head: "feature/x", head_sha: "abc", branches: [], tags: [], remotes: [], fetch_epoch: null, remote_branches: [] } } },
    } as never);
    expect(opsContext()).toMatchObject({ session: "s1", repoRoot: "/repo", selectedPath: "a.rs", head: "feature/x" });
  });

  /** The Code pane in front is the fallback target — `R-J25`'s mirror. */
  it("falls back to the file pane that is forward", () => {
    useStore.setState({ activePane: "file:s1::src/main.rs" } as never);
    expect(opsContext().filePath).toBe("src/main.rs");
    useStore.setState({ activePane: "agent" } as never);
    expect(opsContext().filePath).toBeNull();
  });

  it("has no repository when the session is not in one", () => {
    useStore.setState({ sessions: { s1: session({ repo_root: null }) } } as never);
    expect(opsContext().repoRoot).toBeNull();
  });
});
