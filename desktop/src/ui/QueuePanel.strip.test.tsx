/**
 * The collapsed Attention list says what each session is doing. `R-J113`.
 *
 * Asked 2026-09-25: *"when I collapse it, I see some colorful icon in for each
 * of the session. But there is no way for me to tell the status (like WAITING,
 * RUNNING) … may be with little RED circle or little GREEN/AMBER circle"*.
 *
 * The chip's colour is the session's — your tag, or its name's hash — and
 * stays so. The status is a dot of its own, in the colour the expanded row's
 * badge wears, so collapsing the list changes how much it says and never what
 * a colour means.
 */

import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import { TooltipProvider } from "@/ui/primitives";
import { QueuePanel, reasonColor } from "@/ui/QueuePanel";
import type { AttentionReason, Session } from "@/wire/types";

const session = (id: string, title: string): Session =>
  ({
    id,
    title,
    cwd: "/tmp/repo",
    repo_root: "/tmp/repo",
    alive: true,
    tmux_target: "work:0.0",
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
  }) as unknown as Session;

const ROWS: [string, string, AttentionReason][] = [
  ["w", "the waiting one", "awaiting_input"],
  ["a", "the asking one", "awaiting_permission"],
  ["r", "the running one", "running"],
  ["s", "the stalled one", "stalled"],
  ["i", "the idle one", "idle"],
];

function strip(tags: Record<string, string> = {}) {
  useStore.setState({
    prefs: { ...defaultPrefs(), scope: "all", queueCollapsed: true },
    sessions: Object.fromEntries(ROWS.map(([id, title]) => [id, session(id, title)])),
    queue: ROWS.map(([id, , reason], n) => ({ session_id: id, reason, score: 10 - n, detail: "" })),
    selected: null,
  } as never);
  useStore.getState().setScoped({ ...emptyScoped(), tags });
  render(
    <TooltipProvider>
      <QueuePanel />
    </TooltipProvider>,
  );
}

const chip = (title: RegExp) => screen.getByRole("button", { name: title });
const dot = (title: RegExp) => within(chip(title)).getByText("", { selector: "[data-status]" });

beforeEach(() => cleanup());
afterEach(() => {
  useStore.setState({ sessions: {}, queue: [] } as never);
  localStorage.clear();
});

describe("the collapsed Attention list", () => {
  it("names each chip's status, for a reader and a screen reader alike", () => {
    strip();
    expect(chip(/the waiting one — WAITING/)).toBeInTheDocument();
    expect(chip(/the running one — running/)).toBeInTheDocument();
    expect(chip(/the idle one — idle/)).toBeInTheDocument();
  });

  it("wears the expanded row's colour for each status: red waiting, green running, amber stalled", () => {
    strip();
    for (const [, title, reason] of ROWS) {
      const d = dot(new RegExp(title));
      expect(d.dataset.status).toBe(reason);
      expect(d.style.background).toBe(reasonColor(reason));
    }
    expect(dot(/the waiting one/).style.background).toBe("var(--red)");
    expect(dot(/the running one/).style.background).toBe("var(--green)");
    expect(dot(/the stalled one/).style.background).toBe("var(--amber)");
  });

  /** Your tag still colours the chip; the status is the dot, not a repaint. */
  it("keeps the chip your tag's colour, and puts the status beside it", () => {
    strip({ w: "blue" });
    expect(chip(/the waiting one/).style.background).toBe("var(--blue)");
    expect(dot(/the waiting one/).style.background).toBe("var(--red)");
  });
});
