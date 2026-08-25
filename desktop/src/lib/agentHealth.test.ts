/**
 * The Health window must not show the same CLI twice, and must not lose one.
 *
 * The daemon sends both shapes at once during the `R-I15` transition: the new
 * `agents` list *and* the four deprecated `codex_*` fields describing the same
 * install. A client that read both would render two Codex chips; one that read
 * only the old fields would never show Qwen at all.
 */

import { describe, expect, it } from "vitest";
import { agentColor, agentSlots } from "@/lib/agentHealth";
import type { Health } from "@/wire/types";

const base = (patch: Partial<Health> = {}): Health =>
  ({
    lines_seen: 0,
    alerts: [],
    versions_seen: [],
    ...patch,
  }) as Health;

describe("agentSlots", () => {
  it("reads the new list and ignores the deprecated fields describing the same install", () => {
    const health = base({
      agents: [
        { source: "codex", present: true, threads: 2, error: null, unknown: [] },
        { source: "qwen", present: true, threads: 1, error: null, unknown: [] },
      ],
      codex_present: true,
      codex_threads: 2,
    });
    const slots = agentSlots(health);
    expect(slots.map((a) => a.source)).toEqual(["codex", "qwen"]);
    expect(slots.filter((a) => a.source === "codex")).toHaveLength(1);
  });

  it("reconstructs a codex slot from an older daemon that sends no list", () => {
    const health = base({
      codex_present: true,
      codex_threads: 3,
      codex_error: "database is locked",
      codex_unknown: [["thought", 4]],
    });
    expect(agentSlots(health)).toEqual([
      {
        source: "codex",
        present: true,
        threads: 3,
        error: "database is locked",
        unknown: [["thought", 4]],
      },
    ]);
  });

  /// The distinction that makes the fallback safe to keep: an empty list is a
  /// real answer from a new daemon, not a missing one.
  it("treats an empty list as 'no other CLI', not as a missing field", () => {
    const health = base({ agents: [], codex_present: true, codex_threads: 9 });
    expect(agentSlots(health)).toEqual([]);
  });

  it("drops an install that is not present", () => {
    expect(agentSlots(base({ codex_present: false, codex_threads: 0 }))).toEqual([]);
  });
});

describe("agentColor", () => {
  it("gives each CLI its own colour, keyed by the label the daemon sends", () => {
    const colours = ["claude", "codex", "qwen"].map(agentColor);
    expect(new Set(colours).size).toBe(3);
  });

  /// The daemon can be newer than the window. An unknown CLI must render, not
  /// throw.
  it("falls back for a CLI this client has never heard of", () => {
    expect(agentColor("some-future-agent")).toBe("var(--fg-dim)");
  });
});
