import { describe, expect, it } from "vitest";
import {
  emptyQuery,
  hidesGraph,
  inForce,
  looksLikeSha,
  parseSearch,
  patchText,
  presetOf,
  presetRange,
  searchText,
  signatureWord,
} from "@/lib/gitFilter";
import type { FileChange } from "@/wire/types";

describe("looksLikeSha", () => {
  it("is a whole hex word of seven or more", () => {
    expect(looksLikeSha("fa64b61")).toBe(true);
    expect(looksLikeSha(" FA64B6145616 ")).toBe(true);
    expect(looksLikeSha("fa64b6")).toBe(false);
    expect(looksLikeSha("fa64b61 fix")).toBe(false);
    expect(looksLikeSha("deadbeef the word")).toBe(false);
    expect(looksLikeSha("")).toBe(false);
  });
});

describe("parseSearch", () => {
  it("pulls the prefixed fields out and leaves the message", () => {
    expect(parseSearch("R-D26 author:keith path:crates/mogeungd find:log_args the log")).toEqual({
      grep: "R-D26 the log",
      author: "keith",
      path: "crates/mogeungd",
      pickaxe: "log_args",
    });
  });
  it("treats an empty prefix as a word, and round-trips through searchText", () => {
    expect(parseSearch("author: alone")).toEqual({ grep: "author: alone", author: "", path: "", pickaxe: "" });
    const q = { grep: "fix", author: "kinz", path: "docs", pickaxe: "" };
    expect(parseSearch(searchText(q))).toEqual(q);
    expect(searchText({ grep: "", author: "", path: "", pickaxe: "" })).toBe("");
  });
});

describe("inForce and hidesGraph", () => {
  const now = new Date(2026, 8, 11, 10, 0, 0);
  it("says nothing for the default view", () => {
    expect(inForce(emptyQuery, now)).toEqual([]);
    expect(hidesGraph(emptyQuery)).toBe(false);
  });
  it("names every filter, and only text filters hide the graph", () => {
    const q = { ...emptyQuery, rev: "main", all: false, grep: "PROJ-1", author: "keith", path: "docs", pickaxe: "x", ...presetRange("week", now) };
    expect(inForce(q, now)).toEqual(["branch main", "message ~ PROJ-1", "author ~ keith", "docs", "-S x", "last 7 days"]);
    expect(hidesGraph(q)).toBe(true);
    expect(hidesGraph({ ...emptyQuery, rev: "main", all: false, ...presetRange("today", now) })).toBe(false);
  });
  it("labels a range that is not a preset by its days", () => {
    const q = { ...emptyQuery, since: Date.UTC(2026, 0, 1) / 1000, until: Date.UTC(2026, 0, 31) / 1000 };
    expect(inForce(q, now)).toEqual(["2026-01-01 → 2026-01-31"]);
    expect(inForce({ ...emptyQuery, since: Date.UTC(2026, 0, 1) / 1000 }, now)).toEqual(["since 2026-01-01"]);
  });
});

describe("presetRange", () => {
  const now = new Date(2026, 8, 11, 10, 30, 0);
  it("is local midnights, and recognisable afterwards", () => {
    const today = presetRange("today", now);
    expect(today.since).toBe(Math.floor(new Date(2026, 8, 11).getTime() / 1000));
    expect(today.until).toBeNull();
    const y = presetRange("yesterday", now);
    expect(y.since).toBe(Math.floor(new Date(2026, 8, 10).getTime() / 1000));
    expect(y.until).toBe(today.since! - 1);
    expect(presetOf(y.since, y.until, now)).toBe("yesterday");
    expect(presetOf(null, null, now)).toBe("any");
    expect(presetOf(1, 2, now)).toBeNull();
  });
});

describe("signatureWord", () => {
  it("words the letters and is quiet about nothing", () => {
    expect(signatureWord("N")).toEqual({ word: "unsigned", bad: false });
    expect(signatureWord("G")).toEqual({ word: "signed", bad: false });
    expect(signatureWord("B")).toEqual({ word: "bad signature", bad: true });
    expect(signatureWord("N\n")).toEqual({ word: "unsigned", bad: false });
    expect(signatureWord("")).toBeNull();
    expect(signatureWord(undefined)).toBeNull();
  });
});

describe("patchText", () => {
  const file = (over: Partial<FileChange>): FileChange => ({
    path: "a.rs",
    old_path: null,
    status: "modified",
    insertions: 1,
    deletions: 1,
    hunks: [{ anchor: "x", header: "@@ -1,2 +1,2 @@", lines: [" keep", "-old", "+new"], insertions: 1, deletions: 1, flags: [], score: 0, reviewed: false }],
    flags: [],
    score: 0,
    truncated: false,
    ...over,
  });
  it("rebuilds a unified diff with /dev/null ends and rename lines", () => {
    expect(patchText([file({})])).toBe("diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n");
    expect(patchText([file({ status: "added" })])).toContain("--- /dev/null\n+++ b/a.rs\n");
    expect(patchText([file({ status: "deleted" })])).toContain("--- a/a.rs\n+++ /dev/null\n");
    expect(patchText([file({ status: "renamed", old_path: "z.rs" })])).toContain("diff --git a/z.rs b/a.rs\nrename from z.rs\nrename to a.rs\n--- a/z.rs\n+++ b/a.rs\n");
    expect(patchText([])).toBe("");
  });
});
