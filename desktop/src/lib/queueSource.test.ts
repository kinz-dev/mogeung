/**
 * `source:` joins the field filters. `R-I15`.
 *
 * With one agent CLI the term would have been noise; with three in one queue,
 * "just the qwen ones" was a question that could not be asked at all. The
 * interesting case is that the wire value (`qwen_code`) is not the word anyone
 * would type, so both spellings have to match.
 */

import { describe, expect, it } from "vitest";
import { matchesFilter } from "@/lib/queue";
import type { Session, SessionSource } from "@/wire/types";

const session = (source: SessionSource, patch: Partial<Session> = {}): Session =>
  ({
    id: "s",
    title: "adding the outline",
    cwd: "/home/kinz/projects/immix",
    repo_root: "/home/kinz/projects/immix",
    git_branch: "main",
    touched_files: ["src/main.rs"],
    source,
    ...patch,
  }) as Session;

describe("filtering the queue by agent CLI", () => {
  it("matches the label a human would type, not just the wire value", () => {
    expect(matchesFilter(session("qwen_code"), undefined, "source:qwen")).toBe(true);
    expect(matchesFilter(session("qwen_code"), undefined, "source:qwen_code")).toBe(true);
  });

  it("excludes the other CLIs", () => {
    expect(matchesFilter(session("claude_code"), undefined, "source:qwen")).toBe(false);
    expect(matchesFilter(session("codex"), undefined, "source:qwen")).toBe(false);
    expect(matchesFilter(session("qwen_code"), undefined, "source:codex")).toBe(false);
  });

  /// `claude` must not match `claude_code` *and* every other session by
  /// accident — the haystack is the source, not the label.
  it("does not let a repo or title bleed into the source term", () => {
    const s = session("codex", { title: "port the qwen adapter" });
    expect(matchesFilter(s, undefined, "source:qwen")).toBe(false);
  });

  it("accepts agent: as a synonym", () => {
    expect(matchesFilter(session("codex"), undefined, "agent:codex")).toBe(true);
  });

  it("combines with the other field filters", () => {
    const s = session("qwen_code");
    expect(matchesFilter(s, undefined, "source:qwen branch:main")).toBe(true);
    expect(matchesFilter(s, undefined, "source:qwen branch:release")).toBe(false);
  });
});
