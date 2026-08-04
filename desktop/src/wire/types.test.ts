/**
 * The wire types are a hand-mirror of `mogeung-core`, so the helpers on them
 * have to mean what the Rust means. `unverified` is the one worth pinning: it
 * decides whether a session is marked as having edited files nobody checked,
 * and getting it backwards would be a quiet, confident lie.
 */

import { describe, expect, it } from "vitest";
import { unverified, type Session, type VerifyRun } from "@/wire/types";

const session = (patch: Partial<Session>): Session =>
  ({
    id: "s",
    touched_files: [],
    verify_runs: [],
    ...patch,
  }) as Session;

const run = (ok: boolean | null): VerifyRun => ({ kind: "test", command: "cargo test", ok }) as VerifyRun;

describe("unverified", () => {
  it("is true when files were edited and nothing ever reported a result", () => {
    expect(unverified(session({ touched_files: ["a.rs"] }))).toBe(true);
  });

  /**
   * The distinction the Rust comment insists on: "no completed check" is not
   * "no passing check". A session whose tests failed **has** been verified —
   * badly — and calling it unverified would hide the more interesting fact.
   */
  it("is false when a check completed, even if it failed", () => {
    expect(unverified(session({ touched_files: ["a.rs"], verify_runs: [run(false)] }))).toBe(false);
  });

  it("is still true when a command ran but never reported", () => {
    expect(unverified(session({ touched_files: ["a.rs"], verify_runs: [run(null)] }))).toBe(true);
  });

  it("is false when nothing was edited — there is nothing to verify", () => {
    expect(unverified(session({ touched_files: [] }))).toBe(false);
  });
});
