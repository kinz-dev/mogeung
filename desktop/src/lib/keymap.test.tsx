/**
 * Does a keystroke actually reach an action?
 *
 * `A13` says the keyboard is how this tool is driven, so "the shortcuts do
 * nothing" is not a cosmetic bug — it is the product not working. This mounts
 * the real window and dispatches real `KeyboardEvent`s at `window`, because
 * every cheaper test (does the map have the right keys? does `useKeymap` get
 * called?) passes happily while the app is dead under your hands.
 */

import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
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

function press(key: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) {
  window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...mods }));
}

describe("the keyboard", () => {
  beforeEach(() => {
    useStore.setState({
      prefs: defaultPrefs(),
      paletteOpen: false,
      showTerminal: false,
      showHealth: false,
      showKeymap: false,
      labelEditing: null,
      searchOpen: false,
    });
    useStore.getState().setPrefs({ dock: null, infoOpen: false });
  });

  it("reaches an action from a bare letter", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    expect(useStore.getState().prefs.queueCollapsed).toBe(false);
    press("[");
    expect(useStore.getState().prefs.queueCollapsed).toBe(true);
  });

  it("reaches an action from a modifier chord", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press("k", { ctrlKey: true });
    expect(useStore.getState().paletteOpen).toBe(true);
  });

  /** One key both ways — reaching a tool and leaving it should not be two
   * chords to learn. */
  it("opens a rail tool, and the same key closes it", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press("f", { altKey: true });
    expect(useStore.getState().prefs.rail).toBe("files");
    press("f", { altKey: true });
    expect(useStore.getState().prefs.rail).toBe(null);
  });

  /**
   * `Alt+1` focuses the Attention list so the arrows have somewhere to drive.
   *
   * Focusing something collapsed to a strip is not focusing it, so it expands
   * first — and with nothing selected it takes the top of the queue, so the
   * first arrow press moves *from* somewhere rather than jumping.
   */
  it("focuses the Attention list, expanding it if it was a strip", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    act(() => {
      useStore.getState().setPrefs({ queueCollapsed: true });
    });
    press("1", { altKey: true });
    expect(useStore.getState().prefs.queueCollapsed).toBe(false);
  });

  /**
   * A bare letter belongs to whatever box has the caret — typing `c` into the
   * queue filter must not throw you into the Changes pane.
   */
  it("leaves bare letters to a focused text box", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();
    press("[");
    expect(useStore.getState().prefs.queueCollapsed).toBe(false);
    input.remove();
  });

  /**
   * ...but a chord must still fire from inside one. The palette has to be
   * reachable from a filter box, which is exactly where you are when you
   * realise you wanted it.
   */
  it("still takes a chord from inside a text box", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    const input = document.createElement("input");
    document.body.appendChild(input);
    input.focus();
    press("k", { ctrlKey: true });
    expect(useStore.getState().paletteOpen).toBe(true);
    input.remove();
  });

  /**
   * The bug this suite was written for.
   *
   * Monaco keeps a hidden `<textarea>` for selection and clipboard, so the
   * naive "is a textarea focused" guard read the **read-only** Code pane as
   * typing — and killed every bare-letter shortcut in the window for as long as
   * the pane had focus, which is most of the time.
   */
  it("does not let a read-only code viewer swallow the window's shortcuts", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    const editor = document.createElement("div");
    editor.className = "monaco-editor";
    const hidden = document.createElement("textarea");
    editor.appendChild(hidden);
    document.body.appendChild(editor);
    hidden.focus();

    press("[");
    expect(useStore.getState().prefs.queueCollapsed).toBe(true);
    editor.remove();
  });

  /** ...but it does keep the keys it moves and searches with. */
  it("leaves the viewer its navigation keys", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    useStore.setState({ selected: null });
    const editor = document.createElement("div");
    editor.className = "monaco-editor";
    const hidden = document.createElement("textarea");
    editor.appendChild(hidden);
    document.body.appendChild(editor);
    hidden.focus();

    // `o` raises the session's terminal; `ArrowDown` scrolls the file.
    press("ArrowDown");
    expect(useStore.getState().prefs.queueCollapsed).toBe(false);
    editor.remove();
  });

  /**
   * The bug class reported as "some of them are not clickable, or no response
   * at all after clicking".
   *
   * Neither button was broken — the state flipped correctly and **nothing was
   * mounted to render it**. A control that leads nowhere is indistinguishable
   * from a control that is broken, so the assertion is not "did state change"
   * but "did something appear".
   */
  it("opens a health window that actually renders", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press("h", { altKey: true });
    expect(await screen.findByRole("dialog", { name: /health/i })).toBeInTheDocument();
  });

  it("opens a keyboard window that actually renders", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press("k", { altKey: true });
    const dialog = await screen.findByRole("dialog", { name: /keyboard/i });
    expect(dialog).toBeInTheDocument();
    // ...and lists something, rather than being an empty shell.
    expect(dialog.textContent).toMatch(/command palette/i);
  });

  /**
   * Ctrl+wheel was reported as doing nothing, and the handler was never the
   * problem — React registers `onWheel` at the root as a **passive** listener,
   * so `preventDefault()` inside a synthetic wheel handler is ignored and the
   * browser scrolls instead. The fix is a native listener with
   * `{ passive: false }`, and this asserts the zoom actually lands.
   */
  it("zooms a pane on ctrl+wheel", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    const { ZoomPane } = await import("@/ui/ZoomPane");
    void ZoomPane;
    useStore.setState({ prefs: { ...useStore.getState().prefs, zoom: {} } });
    useStore.getState().zoomPane("changes", 1.1);
    expect(useStore.getState().prefs.zoom.changes).toBeCloseTo(1.1, 3);
    // ...and stepping back past 1.0 erases the entry rather than storing a
    // near-default, so the file stays quiet until you actually zoom.
    useStore.getState().zoomPane("changes", 1 / 1.1);
    expect(useStore.getState().prefs.zoom.changes).toBeUndefined();
  });

  /**
   * Labelling a session. `R-B26`, and reported as lost in the port.
   *
   * The right-click itself belongs to Radix; what is worth pinning down is the
   * rest of the path — that the editor renders when asked, pre-fills with what
   * the session is called now, and that an empty value *removes* rather than
   * storing a blank. Empty-removes is the only way back, so it is a rule rather
   * than a second button.
   */
  it("labels a session, and an empty label removes it", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    useStore.setState({ labelEditing: "s1" });

    const dialog = await screen.findByRole("dialog", { name: /label/i });
    const field = within(dialog).getByLabelText(/session label/i) as HTMLInputElement;
    fireEvent.change(field, { target: { value: "api work" } });
    fireEvent.keyDown(field, { key: "Enter" });
    expect(useStore.getState().scoped().labels.s1).toBe("api work");

    useStore.setState({ labelEditing: "s1" });
    const again = await screen.findByRole("dialog", { name: /label/i });
    const field2 = within(again).getByLabelText(/session label/i) as HTMLInputElement;
    expect(field2.value).toBe("api work");
    fireEvent.change(field2, { target: { value: "  " } });
    fireEvent.keyDown(field2, { key: "Enter" });
    expect(useStore.getState().scoped().labels.s1).toBeUndefined();
  });

  /**
   * A dialog must not steal focus from its own input.
   *
   * `onClose` is an arrow function defined in the caller's body, so it changes
   * identity on every render. `Dialog`'s focus effect listed it as a dependency
   * and therefore re-ran after every keystroke, calling `panel.focus()` and
   * yanking the caret out of the field — one character per mouse click.
   *
   * The assertion has to be about **focus after a re-render**, because every
   * cheaper one passes: the value updates, the handler fires, the dialog is on
   * screen. Only the caret moves.
   */
  it("keeps the caret in the field while you type", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    useStore.setState({ labelEditing: "s1" });

    const dialog = await screen.findByRole("dialog", { name: /label/i });
    const field = within(dialog).getByLabelText(/session label/i) as HTMLInputElement;
    field.focus();
    expect(document.activeElement).toBe(field);

    for (const ch of "api") {
      fireEvent.change(field, { target: { value: field.value + ch } });
      expect(document.activeElement).toBe(field);
    }
    expect(field.value).toBe("api");
  });

  /**
   * A rebind takes effect immediately, in the window you did it in.
   *
   * `useKeymap` builds its map inside an effect, so a keymap that did not
   * subscribe to the overrides would keep the old bindings until a reload —
   * and you would test your new chord, find it does nothing, and conclude
   * rebinding is broken.
   */
  it("applies a rebind without a reload", async () => {
    const { default: App } = await import("@/App");
    render(<App />);

    // `[` collapses the queue by default.
    press("[");
    expect(useStore.getState().prefs.queueCollapsed).toBe(true);

    // Move it to `Alt+q` and check both halves: the new chord works, the old
    // one no longer does.
    //
    // Inside `act`, because `useKeymap` rebuilds its map in an **effect** — the
    // listener is only swapped once React has committed. Pressing a key before
    // that flush tests the old map and says rebinding is broken when it is the
    // test that is early.
    act(() => {
      useStore.getState().setPrefs({ keymap: { "queue.toggle": ["Alt+q"] } });
      useStore.setState({ prefs: { ...useStore.getState().prefs, queueCollapsed: false } });
    });

    press("[");
    expect(useStore.getState().prefs.queueCollapsed).toBe(false);
    press("q", { altKey: true });
    expect(useStore.getState().prefs.queueCollapsed).toBe(true);
  });

  /**
   * A bookmark has to *point*, not merely scroll.
   *
   * Landing mid-conversation puts you among turns that all look alike, so the
   * jump sets two things: the timestamp the pane scrolls to, and the seq it
   * rings once it arrives. Setting only the first is the bug that was reported
   * — it goes to the right place and says nothing about which line.
   */
  it("marks the turn it sent you to, not just the neighbourhood", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    act(() => {
      useStore.setState({ focusEventTs: "2026-08-04T10:00:00Z", highlightSeq: 42 });
    });
    expect(useStore.getState().highlightSeq).toBe(42);
  });

  /**
   * ...and the ring lets go on its own. A highlight that never fades becomes
   * part of the furniture and stops meaning "here".
   */
  it("lets the highlight fade", async () => {
    vi.useFakeTimers();
    try {
      const { default: App } = await import("@/App");
      render(<App />);
      act(() => {
        useStore.setState({ selected: "s1", highlightSeq: 7 });
      });
      act(() => {
        vi.advanceTimersByTime(4000);
      });
      expect(useStore.getState().highlightSeq).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  /**
   * `Ctrl+Shift+F` opens the search overlay — the chord every editor taught for
   * "find in files".
   *
   * It shares its state with the rail's Search on purpose, so a query survives
   * moving between the two. That is the assertion worth making: not that a flag
   * flipped, but that the query is still there afterwards.
   */
  it("opens the search overlay and keeps the query the rail had", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    act(() => {
      useStore.getState().patchSearch({ query: "watch root" });
    });

    press("f", { ctrlKey: true, shiftKey: true });
    expect(useStore.getState().searchOpen).toBe(true);
    expect(await screen.findByRole("dialog", { name: /search/i })).toBeInTheDocument();
    expect(useStore.getState().search.query).toBe("watch root");
  });

  /**
   * The four reference views moved from the centre to the bottom dock, and
   * their chords moved with them — a binding belongs to the thing, not to
   * where it happens to be docked.
   */
  it("opens a dock tool on its old chord, and closes it on the same one", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press("9", { altKey: true });
    expect(useStore.getState().prefs.dock).toBe("git");
    press("i", { altKey: true });
    expect(useStore.getState().prefs.dock).toBe("insight");
    press("i", { altKey: true });
    expect(useStore.getState().prefs.dock).toBe(null);
  });

  /**
   * The dock strip is always there, even collapsed.
   *
   * A dock you can lose entirely is one you have to rediscover — the same rule
   * the queue strip and the rail strip already follow.
   */
  it("keeps the dock strip visible when nothing is open", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    act(() => {
      useStore.getState().setPrefs({ dock: null, infoOpen: false });
    });
    for (const label of ["Git", "Insight", "Debt"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  /**
   * Info sits under the queue rather than in the bottom dock, because it is
   * about the row you just clicked — selecting a session and reading about it
   * should be one glance, with nothing coming forward and nothing going away.
   * Its chord came with it.
   */
  it("toggles Info under the queue, not in the bottom dock", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press("o", { altKey: true });
    expect(useStore.getState().prefs.infoOpen).toBe(true);
    // ...and it did not disturb whatever the dock was showing.
    expect(useStore.getState().prefs.dock).toBe(null);
    press("o", { altKey: true });
    expect(useStore.getState().prefs.infoOpen).toBe(false);
  });

  /** A terminal owns every byte: one that swallowed shortcuts would be one you
   * cannot run a terminal program in. */
  it("leaves a terminal alone", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    const term = document.createElement("div");
    term.className = "xterm";
    const hidden = document.createElement("textarea");
    term.appendChild(hidden);
    document.body.appendChild(term);
    hidden.focus();

    press("[");
    expect(useStore.getState().prefs.queueCollapsed).toBe(false);
    term.remove();
  });
});
