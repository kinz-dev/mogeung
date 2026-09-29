/**
 * Reaching the shells from the keyboard. `R-B60`, `R-B61`.
 *
 * Asked 2026-09-28: *"Terminal windows navigation by keyboard. (use Alt +
 * arrows) to switch the focus"*, and *"When the terminal panel is toggle (Alt +
 * F12), if there is no terminal, please create a new one using the current
 * selected claude session's folder … If there are already opened terminal, No
 * need to auto create a new one."*
 *
 * Through the real keymap listener and the real dockview: what is pinned is
 * that a press typed in a shell is the window's and one typed anywhere else is
 * not, and a fake of either would agree with anything.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, renderHook } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import { type Shell } from "@/lib/shellDock";
import { ACTIONS, bindKeys, defaultKeys, useKeymap } from "@/lib/keymap";

vi.mock("@/ui/Terminal", () => ({
  // Focusable, as xterm's hidden textarea is — that is what a press comes from.
  TerminalView: ({ id }: { id: string }) => (
    <textarea className="xterm-helper-textarea" data-shell={id.replace("shell:", "")} />
  ),
}));

const { TerminalPanel } = await import("@/ui/TerminalPanel");

const MACHINE = "m1";

function withShells(shells: Shell[], selected: { id: string; repo_root: string } | null = null) {
  useStore.setState({
    showTerminal: false,
    daemon: { machine_id: MACHINE } as never,
    machineId: MACHINE,
    selected: selected?.id ?? null,
    sessions: selected ? { [selected.id]: { id: selected.id, repo_root: selected.repo_root, cwd: selected.repo_root } } : {},
    focusTerminal: null,
    prefs: { ...defaultPrefs(), keymap: {}, scoped: { [MACHINE]: { ...emptyScoped(), shells } } },
    send: () => {},
  } as never);
}

const shells = () => useStore.getState().scoped().shells;
const toggle = () => act(() => ACTIONS.find((a) => a.id === "terminal.toggle")!.run(null));

beforeEach(() => {
  cleanup();
  localStorage.clear();
});

describe("showing the panel", () => {
  it("with no shell, opens one in the selected session's worktree and gives it the keyboard", () => {
    withShells([], { id: "s1", repo_root: "/w/alpha" });
    toggle();
    expect(useStore.getState().showTerminal).toBe(true);
    expect(shells()).toEqual([["mog-0", "/w/alpha"]]);
    expect(useStore.getState().focusTerminal?.id).toBe("shell:mog-0");
  });

  /** An empty root is tmux's `-c ""` — the pty's own directory, which is home. */
  it("with no shell and nothing selected, opens one at home", () => {
    withShells([]);
    toggle();
    expect(shells()).toEqual([["mog-0", ""]]);
  });

  it("with a shell already, opens nothing new", () => {
    withShells([["mog-3", "/w/beta"]], { id: "s1", repo_root: "/w/alpha" });
    toggle();
    expect(useStore.getState().showTerminal).toBe(true);
    expect(shells()).toEqual([["mog-3", "/w/beta"]]);
  });

  it("hiding it opens nothing, even with no shell", () => {
    withShells([]);
    useStore.setState({ showTerminal: true });
    toggle();
    expect(useStore.getState().showTerminal).toBe(false);
    expect(shells()).toEqual([]);
  });
});

describe("moving between shells with Alt+arrow", () => {
  // jsdom lays nothing out, so each split is given the rectangle its place in
  // the row would have: 100px wide, left to right in DOM order.
  beforeEach(() => {
    vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
      const groups = [...document.querySelectorAll(".dv-groupview")];
      const i = groups.indexOf(this);
      return { left: i * 100, right: i * 100 + 100, top: 0, bottom: 100, width: 100, height: 100, x: i * 100, y: 0 } as DOMRect;
    });
    vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => {
      cb(0);
      return 0;
    });
  });
  afterEach(() => vi.restoreAllMocks());

  function mount() {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    useStore.setState({ showTerminal: true });
    renderHook(() => useKeymap({ current: null }));
    return render(<TerminalPanel />);
  }

  const shellEl = (name: string) => document.querySelector<HTMLElement>(`[data-shell="${name}"]`)!;
  const activeShell = () => document.querySelector(".dv-active-group [data-shell]")?.getAttribute("data-shell");
  const press = (from: Element, key: string) => {
    const e = new KeyboardEvent("keydown", { key, code: key, altKey: true, bubbles: true, cancelable: true });
    from.dispatchEvent(e);
    return e;
  };

  /** Where the keyboard is, as a click into a shell leaves it. */
  const into = (name: string) => act(() => shellEl(name).focus());

  it("moves to the shell that way, keyboard and all", () => {
    mount();
    into("mog-1");
    expect(activeShell()).toBe("mog-1");
    const e = press(shellEl("mog-1"), "ArrowLeft");
    expect(e.defaultPrevented).toBe(true);
    expect(activeShell()).toBe("mog-0");
    expect(document.activeElement).toBe(shellEl("mog-0"));
  });

  /** Nothing that way is a wall — and the arrow still never reaches the pty. */
  it("swallows the press at the edge, and stays put", () => {
    mount();
    into("mog-1");
    const e = press(shellEl("mog-1"), "ArrowRight");
    expect(e.defaultPrevented).toBe(true);
    expect(activeShell()).toBe("mog-1");
  });

  /** `Alt+←` is a word jump in a text box and a keystroke in an Agent pane. */
  it("leaves the press alone anywhere but a shell", () => {
    mount();
    into("mog-1");
    const box = document.createElement("input");
    document.body.appendChild(box);
    const e = press(box, "ArrowLeft");
    expect(e.defaultPrevented).toBe(false);
    expect(activeShell()).toBe("mog-1");
    box.remove();
  });
});

/** `R-B63`: a new shell by chord, as `+` opens one. */
describe("Alt+Shift+N", () => {
  const action = () => ACTIONS.find((a) => a.id === "terminal.new")!;

  it.each(["other", "mac"] as const)("fires on the %s spelling", (platform) => {
    let fired = 0;
    const off = bindKeys(window, { [defaultKeys(action(), platform)[0]]: () => fired++ });
    // `code` is what a Mac's spelling matches: `⌥⇧N` composes a character.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "N", code: "KeyN", altKey: true, shiftKey: true, bubbles: true }));
    off();
    expect(fired).toBe(1);
  });

  it("opens a shell in the selected session's worktree beside those already open, panel down or not", () => {
    withShells([["mog-0", "/w/beta"]], { id: "s1", repo_root: "/w/alpha" });
    act(() => action().run(null));
    expect(shells()).toEqual([
      ["mog-0", "/w/beta"],
      ["mog-1", "/w/alpha"],
    ]);
    expect(useStore.getState().showTerminal).toBe(true);
    expect(useStore.getState().focusTerminal?.id).toBe("shell:mog-1");
  });
});
