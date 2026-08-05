/**
 * `tag:` joins the field filters, so "which ones did I mark red" is a query
 * rather than a scroll. The interesting case is the absent tag: `tag:none` has
 * to mean *untagged* rather than matching nothing, or there is no way to ask
 * the opposite question.
 */

import { describe, expect, it } from "vitest";
import { matchesFilter } from "@/ui/QueuePanel";
import type { Session } from "@/wire/types";

const session = (patch: Partial<Session> = {}): Session =>
  ({
    id: "s",
    title: "adding the outline",
    cwd: "/home/kinz/projects/immix",
    repo_root: "/home/kinz/projects/immix",
    git_branch: "main",
    touched_files: ["src/main.rs"],
    ...patch,
  }) as Session;

describe("the queue filter", () => {
  it("matches a tag by its colour name", () => {
    expect(matchesFilter(session(), undefined, "tag:red", "red")).toBe(true);
    expect(matchesFilter(session(), undefined, "tag:red", "blue")).toBe(false);
  });

  it("treats an untagged session as tag:none", () => {
    expect(matchesFilter(session(), undefined, "tag:none", undefined)).toBe(true);
    expect(matchesFilter(session(), undefined, "tag:none", "red")).toBe(false);
  });

  it("combines with the other fields rather than replacing them", () => {
    expect(matchesFilter(session(), undefined, "repo:immix tag:red", "red")).toBe(true);
    expect(matchesFilter(session(), undefined, "repo:other tag:red", "red")).toBe(false);
  });

  /** The existing fields still mean what they meant — this is a port of
   *  `filter.rs` and the new field must not have shifted any of them. */
  it("still matches repo, branch, file and label", () => {
    expect(matchesFilter(session(), "mine", "branch:main")).toBe(true);
    expect(matchesFilter(session(), "mine", "file:main.rs")).toBe(true);
    expect(matchesFilter(session(), "mine", "label:mine")).toBe(true);
    expect(matchesFilter(session(), "mine", "outline")).toBe(true);
  });
});
