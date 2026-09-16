/**
 * What a dropped file means. `R-J94`.
 *
 * Two questions, and neither is about the DOM: which paths a drop is carrying,
 * and which session opens one. The listener around them is three lines; these
 * are the decisions.
 */

import { describe, expect, it } from "vitest";
import { dropPaths, fileUrlToPath, inside, isFileDrag, parentDir, targetForDrop } from "@/lib/drop";
import type { Session } from "@/wire/types";

const session = (id: string, root: string, extra: Partial<Session> = {}): Session =>
  ({ id, repo_root: root, cwd: root, ...extra }) as unknown as Session;

const transfer = (data: Record<string, string>) => ({
  types: Object.keys(data),
  getData: (t: string) => data[t] ?? "",
});

describe("is this drag carrying files", () => {
  it("takes an OS file drag", () => {
    expect(isFileDrag(["Files", "text/uri-list", "text/plain"])).toBe(true);
    expect(isFileDrag(["text/uri-list"])).toBe(true);
  });

  /**
   * The one that matters. dockview drags tabs and groups with `text/plain`
   * and nothing else, and `R-J20` bought those gestures at the cost of the
   * shell's own drop handler — eating them here would undo that.
   */
  it("leaves a dockview tab drag alone", () => {
    expect(isFileDrag(["text/plain"])).toBe(false);
    expect(isFileDrag([])).toBe(false);
    expect(isFileDrag(undefined)).toBe(false);
  });
});

describe("the paths a drop is carrying", () => {
  it("reads a uri-list, decoding what the URL escaped", () => {
    expect(
      dropPaths(transfer({ "text/uri-list": "file:///home/kinz/my%20notes.md\r\nfile:///tmp/a.ts" })),
    ).toEqual(["/home/kinz/my notes.md", "/tmp/a.ts"]);
  });

  it("ignores the comment lines the format allows", () => {
    expect(dropPaths(transfer({ "text/uri-list": "# label\r\nfile:///tmp/a.ts" }))).toEqual(["/tmp/a.ts"]);
  });

  it("falls back to text/plain when that is all there is", () => {
    expect(dropPaths(transfer({ "text/plain": "file:///tmp/a.ts" }))).toEqual(["/tmp/a.ts"]);
    expect(dropPaths(transfer({ "text/plain": "/tmp/a.ts" }))).toEqual(["/tmp/a.ts"]);
  });

  /** Both flavours filled with the same file must not open it twice. */
  it("opens one file once, however many flavours name it", () => {
    expect(
      dropPaths(transfer({ "text/uri-list": "file:///tmp/a.ts", "text/plain": "file:///tmp/a.ts" })),
    ).toEqual(["/tmp/a.ts"]);
  });

  it("has nothing to say about a drag from a browser", () => {
    expect(dropPaths(transfer({ "text/uri-list": "https://example.com/a.ts" }))).toEqual([]);
    expect(dropPaths(null)).toEqual([]);
  });

  it("drops the host segment some sources write", () => {
    expect(fileUrlToPath("file://localhost/tmp/a.ts")).toBe("/tmp/a.ts");
    expect(fileUrlToPath("file:///tmp/a.ts")).toBe("/tmp/a.ts");
    expect(fileUrlToPath("file://%zz")).toBeNull();
  });
});

describe("which session opens the file", () => {
  const sessions = (...list: Session[]) => Object.fromEntries(list.map((s) => [s.id, s]));

  it("uses the session whose root holds it, and asks for it relative", () => {
    expect(
      targetForDrop("/home/kinz/projects/mogeung/src/main.rs", {
        sessions: sessions(session("a", "/home/kinz/projects/mogeung"), session("b", "/home/kinz/other")),
        selected: "b",
      }),
    ).toEqual({ session: "a", path: "src/main.rs", addDir: null });
  });

  /** A worktree inside a checkout: the nearer root is the one that means it. */
  it("prefers the nearest root when two hold the file", () => {
    expect(
      targetForDrop("/repo/sub/a.ts", {
        sessions: sessions(session("outer", "/repo"), session("inner", "/repo/sub")),
        selected: "outer",
      })?.session,
    ).toBe("inner");
  });

  it("breaks a tie on the session you are looking at", () => {
    expect(
      targetForDrop("/repo/a.ts", {
        sessions: sessions(session("one", "/repo"), session("two", "/repo")),
        selected: "two",
      })?.session,
    ).toBe("two");
  });

  /**
   * `R-J40` is the only door to a file outside a session's root, so a drop
   * from anywhere else asks for that door to be opened — and keeps the path
   * absolute, because that is the form an added folder is read through.
   */
  it("names the folder to admit when no session holds the file", () => {
    expect(
      targetForDrop("/etc/hosts", {
        sessions: sessions(session("a", "/repo")),
        selected: "a",
      }),
    ).toEqual({ session: "a", path: "/etc/hosts", addDir: "/etc" });
  });

  it("uses a workspace folder already added, without adding it again", () => {
    expect(
      targetForDrop("/shared/lib/a.ts", {
        sessions: sessions(session("a", "/repo")),
        extraRoots: () => ["/shared"],
        selected: "a",
      }),
    ).toEqual({ session: "a", path: "/shared/lib/a.ts", addDir: null });
  });

  it("has no answer when there is no session to open it in", () => {
    expect(targetForDrop("/etc/hosts", { sessions: {}, selected: null })).toBeNull();
  });

  /** `/reposaurus` is not inside `/repo`, whatever the prefix says. */
  it("matches a root by directory boundary, not by prefix", () => {
    expect(inside("/repo", "/reposaurus/a.ts")).toBe(false);
    expect(inside("/repo", "/repo/a.ts")).toBe(true);
    expect(parentDir("/a/b/c.txt")).toBe("/a/b");
    expect(parentDir("/a")).toBe("/");
  });
});
