/**
 * When an update is worth interrupting you for, and how the window says where
 * it stands. `R-J105`.
 */

import { describe, expect, it } from "vitest";
import { SETTLE_MS, behindText, settled, short, type UpdateCheck } from "@/lib/update";

const check = (head: string | null, headTime: number | null, behind: number | null = 1): UpdateCheck => ({
  head,
  head_time: headTime,
  commits: [],
  behind,
  error: null,
});

describe("when to offer", () => {
  const now = 1_800_000_000_000;

  it("not when the branch is where this build is", () => {
    expect(settled("abc", check("abc", 0), now - SETTLE_MS * 10, now)).toBe(false);
    expect(settled("abc", check(null, null), now - SETTLE_MS * 10, now)).toBe(false);
  });

  it("not while the branch is still moving — a burst of commits is one offer, not five", () => {
    // Committed ten seconds ago, first seen ten seconds ago.
    expect(settled("abc", check("def", now / 1000 - 10), now - 10_000, now)).toBe(false);
  });

  it("once it has been still for two minutes, by this window's watch or by its commit time", () => {
    expect(settled("abc", check("def", now / 1000 - 10), now - SETTLE_MS, now)).toBe(true);
    // A commit made an hour ago is settled the moment it is seen.
    expect(settled("abc", check("def", now / 1000 - 3600), now, now)).toBe(true);
  });
});

describe("the status bar's words", () => {
  it("says how far behind, or that it is up to date, or that it is not on the branch at all", () => {
    expect(behindText("abc", check("abc", 0, 0), "main")).toBe("up to date with main");
    expect(behindText("abc", check("def", 0, 1), "main")).toBe("1 commit behind main");
    expect(behindText("abc", check("def", 0, 7), "main")).toBe("7 commits behind main");
    expect(behindText("abc", check("def0123456", 0, null), "main")).toBe("not on main — main is def0123");
    expect(behindText("abc", null, "main")).toBeNull();
  });

  it("shortens a commit to seven, and says unknown for none", () => {
    expect(short("0123456789abcdef")).toBe("0123456");
    expect(short("")).toBe("unknown");
    expect(short(null)).toBe("unknown");
  });
});
