/**
 * A large diff opens as a list, not as every hunk at once. `R-J85`.
 *
 * Reported 2026-08-29: *"the Changes pane is very slow… it shows all the files
 * all expanded"*. Nothing in this list is virtualised, so a session whose diff
 * base is a few days back puts every file and every one of their hunks into
 * the DOM in one go — 280 files was a real one on this machine.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { FileChange } from "@/wire/types";
import { DiffList } from "@/ui/DiffView";

const file = (n: number): FileChange =>
  ({
    path: `src/f${n}.rs`,
    old_path: null,
    status: "modified",
    insertions: 1,
    deletions: 0,
    hunks: [
      {
        anchor: `a${n}`,
        header: `@@ -1 +1 @@ f${n}`,
        lines: [`+line in f${n}`],
        insertions: 1,
        deletions: 0,
        flags: [],
        score: 0,
        reviewed: false,
      },
    ],
    flags: [],
    score: 0,
    truncated: false,
  }) as unknown as FileChange;

const many = (n: number) => Array.from({ length: n }, (_, i) => file(i));

beforeEach(() => {
  useStore.setState({ prefs: defaultPrefs(), send: () => {} } as never);
});

describe("how a diff opens", () => {
  // A diff line is drawn as several elements — the `+` marker is its own —
  // so the body is matched against the rendered text rather than one node.
  const body = (c: HTMLElement) => c.textContent ?? "";

  /** A handful of files is a diff you came to read. Nothing changes. */
  it("expands a small diff, as it always did", () => {
    const { container } = render(<DiffList files={many(3)} sessionId="s1" />);
    expect(body(container)).toContain("line in f0");
    expect(body(container)).toContain("line in f2");
  });

  /**
   * Past the threshold every file is collapsed. The header row is still the
   * whole scannable answer — path, ±, risk — which is what you are reading at
   * that size.
   */
  it("collapses a large one, and still lists every file", () => {
    const { container } = render(<DiffList files={many(40)} sessionId="s1" />);

    // Every path is on screen…
    expect(screen.getByText("src/f0.rs")).toBeInTheDocument();
    expect(screen.getByText("src/f39.rs")).toBeInTheDocument();
    // …and not one hunk body is.
    expect(body(container)).not.toContain("line in f0");
    expect(body(container)).not.toContain("line in f39");
  });

  /**
   * The boundary, pinned. A silent drift here would either bring back the
   * stall or start collapsing diffs people expect to read.
   */
  it("draws the line at twelve files", () => {
    const twelve = render(<DiffList files={many(12)} sessionId="s1" />);
    expect(body(twelve.container)).toContain("line in f0");
    twelve.unmount();

    const thirteen = render(<DiffList files={many(13)} sessionId="s1" />);
    expect(body(thirteen.container)).not.toContain("line in f0");
  });
});
