/**
 * A writable editor owns the keyboard. Reported 2026-09-08: *"why in the
 * scratch pad editor I can't type the `j` chars, is that taken by some a
 * keymap?"* — and it was, by `queue.next`.
 *
 * `keymap.ts` had predicted this in a comment and then not been revisited when
 * it came true: *"It is read-only (ADR-0019)… **If a Monaco here ever becomes
 * editable, this has to become a per-editor check.**"* `R-L5`'s scratch files
 * made one editable on 2026-09-03 and the check was not written, so every bare
 * binding was stolen back from the editor.
 *
 * `j` and `k` are gone as bindings, which fixes the report. These tests are
 * about the **class** of bug rather than that instance: `[` is still bare, and
 * a bare `[` that cannot be typed into a Java scratch file is the same defect
 * one keystroke over.
 */

import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

class FakeSocket {
  static OPEN = 1;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  constructor(public url: string) {}
  send() {}
  close() {}
}

beforeAll(() => {
  vi.stubGlobal("WebSocket", FakeSocket);
});

function press(key: string) {
  act(() => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  });
}

/** A focused text box inside `wrapper`, which is what `focusOwns` reads. */
function focusInside(wrapper: HTMLElement): () => void {
  const box = document.createElement("textarea");
  wrapper.appendChild(box);
  document.body.appendChild(wrapper);
  box.focus();
  return () => wrapper.remove();
}

function writableEditor(): HTMLElement {
  const el = document.createElement("div");
  el.setAttribute("data-editor", "writable");
  // A writable editor is *also* a Monaco, which is the whole reason the order
  // of the checks in `focusOwns` matters.
  el.className = "monaco-editor";
  return el;
}

function readOnlyEditor(): HTMLElement {
  const el = document.createElement("div");
  el.className = "monaco-editor";
  return el;
}

describe("who owns a bare key", () => {
  beforeEach(() => {
    useStore.setState({ prefs: defaultPrefs(), paletteOpen: false });
    useStore.getState().setPrefs({ queueCollapsed: false });
  });

  /** The control: with nothing focused, a bare binding is the window's. */
  it("gives a bare key to the window when no editor has focus", async () => {
    const { default: App } = await import("@/App");
    render(<App />);

    press("[");

    expect(useStore.getState().prefs.queueCollapsed).toBe(true);
  });

  /** The bug, in the shape it will come back in. */
  it("does not steal a bare key from a writable editor", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    const done = focusInside(writableEditor());

    press("[");

    expect(useStore.getState().prefs.queueCollapsed).toBe(false);
    done();
  });

  /**
   * And the half that must not regress with the fix: a **viewer** still hands
   * bare letters back, or every shortcut in the window dies whenever the Code
   * pane has focus — which is the bug the viewer rule was written for.
   */
  it("still takes a bare key from a read-only editor", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    const done = focusInside(readOnlyEditor());

    press("[");

    expect(useStore.getState().prefs.queueCollapsed).toBe(true);
    done();
  });
});

describe("the queue's own navigation", () => {
  beforeEach(() => {
    useStore.setState({ prefs: defaultPrefs(), selected: null });
  });

  /**
   * `j` and `k` were removed on report rather than re-guarded: a bare letter is
   * given to whatever has focus, so it was never going to be typable and a
   * shortcut everywhere else at the same time.
   */
  it("no longer answers to a bare j or k", async () => {
    const { ACTIONS } = await import("@/lib/keymap");
    const next = ACTIONS.find((a) => a.id === "queue.next");
    const prev = ACTIONS.find((a) => a.id === "queue.prev");

    expect(next?.keys).toEqual(["ArrowDown"]);
    expect(prev?.keys).toEqual(["ArrowUp"]);
  });

  /** The arrows are in `VIEWER_KEYS`, so they were never the problem. */
  it("still answers to the arrows", async () => {
    const { ACTIONS } = await import("@/lib/keymap");
    expect(ACTIONS.find((a) => a.id === "queue.next")?.keys).toContain("ArrowDown");
  });
});
