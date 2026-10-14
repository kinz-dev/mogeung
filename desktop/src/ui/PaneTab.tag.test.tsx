/**
 * A session's colour tag, on its tabs. `R-J106`, asked 2026-09-25: *"color
 * the tab header to the same color selected in the Attention list … so that
 * user can easily tell what is the session"*.
 *
 * The tab says which tag; `index.css` paints it. So these pin both halves:
 * every tab that speaks for a session names that session's tag — the Agent
 * pane by what it shows, a file or a diff by the session that opened it — and
 * every tag the queue offers has a rule to paint it with.
 */

import { readFileSync } from "node:fs";
import type { IDockviewPanelHeaderProps } from "dockview";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { TAGS } from "@/lib/tags";
import { diffPaneId, filePaneId } from "@/lib/panes";
import { scratchPaneId } from "@/lib/scratch";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import { PaneTab } from "@/ui/PaneChrome";
import type { AttentionItem, Session } from "@/wire/types";

const session = (id: string): Session =>
  ({
    id,
    title: `session ${id}`,
    cwd: "/tmp/repo",
    repo_root: "/tmp/repo",
    alive: true,
    touched_files: [],
    collisions: [],
    verify_runs: [],
    claims: [],
    last_event_at: new Date().toISOString(),
  }) as unknown as Session;

function tagged(tags: Record<string, string>, paneHold: Record<string, string> = {}) {
  useStore.setState({ prefs: { ...defaultPrefs(), scoped: { unknown: { ...emptyScoped(), tags, paneHold } } } });
}

/** Render the real tab with the least of dockview's panel API it reads. */
function tab(id: string) {
  const api = { id, title: "Agent", onDidTitleChange: () => ({ dispose() {} }), close: vi.fn() };
  const props = { api, containerApi: {}, params: {} } as unknown as IDockviewPanelHeaderProps;
  render(<PaneTab {...props} />);
  return () => document.querySelector(".dv-default-tab")!;
}

beforeEach(() => {
  cleanup();
  useStore.setState({ selected: "s1", sessions: { s1: session("s1"), s2: session("s2") }, queue: [] });
  tagged({});
});

describe("a tab wears its session's colour", () => {
  it("the Agent pane: the session it shows, and it changes when the selection does", () => {
    tagged({ s1: "green", s2: "red" });
    const el = tab("agent");
    expect(el().getAttribute("data-tag")).toBe("green");
    expect(el().getAttribute("title")).toMatch(/tagged Green$/);
    act(() => useStore.setState({ selected: "s2" }));
    expect(el().getAttribute("data-tag")).toBe("red");
  });

  it("a held Agent pane: the session it is held on, not the selection", () => {
    tagged({ s1: "green", s2: "red" }, { agent: "s2" });
    const el = tab("agent");
    expect(el().getAttribute("data-tag")).toBe("red");
  });

  it("a file or a diff: the session that opened it, whichever is selected", () => {
    tagged({ s1: "green", s2: "purple" });
    const file = tab(filePaneId("s2", "src/lib.rs", null));
    expect(file().getAttribute("data-tag")).toBe("purple");
    cleanup();
    const diff = tab(diffPaneId("s2", "abc1234", "src/lib.rs"));
    expect(diff().getAttribute("data-tag")).toBe("purple");
  });

  it("follows the tag as you change it in the queue, and goes when you take it off", () => {
    const el = tab("agent");
    expect(el().hasAttribute("data-tag")).toBe(false);
    act(() => tagged({ s1: "amber" }));
    expect(el().getAttribute("data-tag")).toBe("amber");
    act(() => tagged({}));
    expect(el().hasAttribute("data-tag")).toBe(false);
  });

  it("no colour for a tab that speaks for no session, or for a tag this build does not know", () => {
    tagged({ s1: "green" });
    const scratch = tab(scratchPaneId("notes"));
    expect(scratch().hasAttribute("data-tag")).toBe(false);
    cleanup();
    tagged({ s1: "chartreuse" });
    const agent = tab("agent");
    expect(agent().hasAttribute("data-tag")).toBe(false);
  });
});

describe("the stylesheet paints every tag", () => {
  it("has a rule for each tag the queue offers, in the queue's own two values", () => {
    const css = readFileSync("src/index.css", "utf8");
    const missing: string[] = [];
    for (const t of TAGS) {
      const rule = new RegExp(
        String.raw`\.dv-tab:has\(\.dv-default-tab\[data-tag="${t.id}"\]\)\s*\{\s*--tab-tag:\s*${t.color.replace(/[()]/g, "\\$&")};\s*--tab-tag-bg:\s*${t.bg.replace(/[()]/g, "\\$&")};`,
      );
      if (!rule.test(css)) missing.push(t.id);
    }
    expect(missing).toEqual([]);
  });
});

describe("an Agent tab says what its session is doing, after its name", () => {
  const ranked = (session_id: string, reason: AttentionItem["reason"]): AttentionItem =>
    ({ session_id, reason, score: 50, detail: "" }) as AttentionItem;

  it("the queue's own word, and it follows the queue and the selection", () => {
    useStore.setState({ queue: [ranked("s1", "stalled"), ranked("s2", "awaiting_input")] });
    const el = tab("agent");
    expect(el().textContent).toBe("session s1STALLED");
    expect(el().getAttribute("title")).toMatch(/· STALLED$/);
    act(() => useStore.setState({ queue: [ranked("s1", "running"), ranked("s2", "awaiting_input")] }));
    expect(el().textContent).toBe("session s1running");
    act(() => useStore.setState({ selected: "s2" }));
    expect(el().textContent).toBe("session s2WAITING");
  });

  it("nothing for a session the queue does not rank, or on a file's tab", () => {
    useStore.setState({ queue: [ranked("s2", "failed")] });
    const agent = tab("agent");
    expect(agent().textContent).toBe("session s1");
    cleanup();
    const file = tab(filePaneId("s2", "src/lib.rs", null));
    expect(file().textContent).not.toMatch(/FAILED/);
  });
});
