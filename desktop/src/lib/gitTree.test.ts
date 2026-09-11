import { describe, expect, it } from "vitest";
import { fileTree, hostUrl, refTree, visible } from "@/lib/gitTree";
import type { BranchInfo, FileChange, RefsInfo, TagInfo } from "@/wire/types";

const file = (path: string, ins = 1, del = 0): FileChange => ({
  path,
  old_path: null,
  status: "modified",
  insertions: ins,
  deletions: del,
  hunks: [],
  flags: [],
  score: 0,
  truncated: false,
});

describe("fileTree", () => {
  it("loses no file, puts directories before files, and flattens corridors", () => {
    const rows = fileTree([
      file("STATUS.md"),
      file("crates/mogeungd/src/notes.rs", 144, 6),
      file("crates/mogeungd/src/state.rs", 10, 6),
      file("desktop/src/ui/tools/TasksTool.tsx", 40, 3),
      file("desktop/src/wire/types.ts", 4),
    ]);
    expect(rows.filter((r) => r.kind === "file")).toHaveLength(5);
    // `crates/mogeungd/src` has no files of its own and one child at each
    // step, so it is one row, and it counts what is beneath it.
    const crates = rows.find((r) => r.kind === "dir" && r.label === "crates/mogeungd/src");
    expect(crates).toMatchObject({ depth: 0, count: 2, insertions: 154, deletions: 12, path: "crates/mogeungd/src" });
    // `desktop/src` has two children, so it stops there; `ui/tools` is a
    // corridor beneath it.
    const desktop = rows.find((r) => r.kind === "dir" && r.label === "desktop/src");
    expect(desktop).toMatchObject({ depth: 0, count: 2 });
    expect(rows.find((r) => r.label === "ui/tools")).toMatchObject({ depth: 1, count: 1 });
    // Directories first at each level; the root file last.
    expect(rows[0].kind).toBe("dir");
    expect(rows[rows.length - 1]).toMatchObject({ kind: "file", label: "STATUS.md", depth: 0 });
    const types = rows.find((r) => r.label === "types.ts");
    expect(types).toMatchObject({ depth: 2, path: "desktop/src/wire/types.ts" });
  });

  it("handles an empty diff and a root-only diff", () => {
    expect(fileTree([])).toEqual([]);
    expect(fileTree([file("a"), file("b")]).map((r) => r.label)).toEqual(["a", "b"]);
  });
});

const branch = (name: string, extra: Partial<BranchInfo> = {}): BranchInfo => ({
  name,
  sha: "abc1234",
  current: false,
  upstream: null,
  ahead: 0,
  behind: 0,
  epoch: 0,
  ...extra,
});
const tag = (name: string): TagInfo => ({ name, sha: "abc1234", epoch: 0 });

const refs: RefsInfo = {
  head: "main",
  head_sha: "abc1234",
  branches: [branch("main", { current: true, upstream: "origin/main", ahead: 12 }), branch("claude/issue-303"), branch("claude/issue-302"), branch("git-fetch")],
  tags: [tag("v0.2.0"), tag("v0.1.0")],
  remotes: [{ name: "origin", url: "git@github.com:kinz-dev/mogeung.git" }],
  fetch_epoch: null,
  remote_branches: [branch("origin/main"), branch("origin/claude/issue-303"), branch("origin/ci/blacksmith-runners")],
};

describe("refTree", () => {
  it("groups HEAD, Local, one node per remote, and Tags, on slashes", () => {
    const rows = refTree(refs, "", []);
    const shape = rows.map((r) => `${" ".repeat(r.depth)}${r.kind}:${r.label}`);
    expect(shape).toEqual([
      "head:main",
      "group:Local",
      " dir:claude",
      // The daemon's order — most recently committed first — is kept.
      "  branch:issue-303",
      "  branch:issue-302",
      " branch:main",
      " branch:git-fetch",
      "group:Remote",
      " remote:origin",
      "  dir:ci",
      "   branch:blacksmith-runners",
      "  dir:claude",
      "   branch:issue-303",
      "  branch:main",
      "group:Tags",
      " tag:v0.2.0",
      " tag:v0.1.0",
    ]);
    // A leaf carries the ref a click scopes to, and a group its count.
    expect(rows.find((r) => r.label === "issue-303" && r.depth === 2)?.ref).toBe("claude/issue-303");
    expect(rows.find((r) => r.kind === "remote")).toMatchObject({ count: 3, key: "remote/origin" });
    expect(rows.find((r) => r.kind === "group" && r.label === "Local")?.count).toBe(4);
  });

  it("leads with the current branch, then favourites", () => {
    const rows = refTree({ ...refs, branches: [branch("zed"), branch("alpha", { current: true }), branch("fav")] }, "", ["fav"]);
    const local = rows.filter((r) => r.kind === "branch" && r.key.startsWith("local/")).map((r) => r.label);
    expect(local).toEqual(["alpha", "fav", "zed"]);
    expect(rows.find((r) => r.label === "fav")?.favourite).toBe(true);
  });

  it("narrows to a query and keeps the ancestors of a match", () => {
    const rows = refTree(refs, "303", []);
    const shape = rows.map((r) => `${" ".repeat(r.depth)}${r.kind}:${r.label}`);
    expect(shape).toEqual([
      "group:Local",
      " dir:claude",
      "  branch:issue-303",
      "group:Remote",
      " remote:origin",
      "  dir:claude",
      "   branch:issue-303",
    ]);
  });

  it("names a detached HEAD by its sha", () => {
    const rows = refTree({ ...refs, head: null }, "", []);
    expect(rows[0]).toMatchObject({ kind: "head", label: "detached at abc1234", ref: "abc1234" });
  });
});

describe("visible", () => {
  it("hides everything beneath a collapsed node until its depth returns", () => {
    const rows = refTree(refs, "", []);
    const shown = visible(rows, new Set(["local/claude", "remote"]), (r) => r.key);
    const shape = shown.map((r) => `${" ".repeat(r.depth)}${r.label}`);
    expect(shape).toEqual(["main", "Local", " claude", " main", " git-fetch", "Remote", "Tags", " v0.2.0", " v0.1.0"]);
  });
});

describe("hostUrl", () => {
  it("recognises the three hosts over ssh and https, and nothing else", () => {
    expect(hostUrl("git@github.com:kinz-dev/mogeung.git", "abc")).toBe("https://github.com/kinz-dev/mogeung/commit/abc");
    expect(hostUrl("https://gitlab.com/org/repo", "abc")).toBe("https://gitlab.com/org/repo/commit/abc");
    expect(hostUrl("ssh://git@bitbucket.org/org/repo.git", "abc")).toBe("https://bitbucket.org/org/repo/commits/abc");
    expect(hostUrl("https://git.example.com/org/repo.git", "abc")).toBeNull();
    expect(hostUrl("/srv/git/repo.git", "abc")).toBeNull();
  });
});
