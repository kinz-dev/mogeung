/**
 * The shells are tiles. `R-B56`.
 *
 * Asked 2026-09-28: *"I can organize the terminal windows freely in the bottom
 * panel like what I can do in the top claude code session panel"*, then the
 * same day: splits by chord and by right-click, *no* tab bar — *"it is not
 * useful in the terminal tab"* — and `exit` closing the split it ran in.
 *
 * Driven through the real dockview rather than a fake api, because what is
 * being pinned is that its layout and `scoped.shells` agree — and a fake would
 * agree with anything. `TerminalView` is stood in for: in jsdom it has no pty
 * and renders a notice, and what these need from it is which shell a split
 * holds and the `onExit` the panel hands it.
 */

import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { DockviewComponent, type DockviewApi } from "dockview";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import { readFileSync } from "node:fs";
import { SHELL_HUES, restoreShellLayout, shellHue, shellLayoutKey, splitShell, syncShells, type Shell } from "@/lib/shellDock";
import { ACTIONS, bindKeys, defaultKeys, formatChord } from "@/lib/keymap";

/** Each mounted shell's `onExit`, by pty id — the pty closing, simulated. */
const exits = new Map<string, (ranMs: number) => void>();
vi.mock("@/ui/Terminal", () => ({
  TerminalView: ({ id, onExit }: { id: string; onExit?: (ranMs: number) => void }) => {
    if (onExit) exits.set(id, onExit);
    return <div data-shell={id.replace("shell:", "")} />;
  },
}));

const { TerminalPanel } = await import("@/ui/TerminalPanel");

const MACHINE = "m1";

function withShells(shells: Shell[]) {
  useStore.setState({
    showTerminal: true,
    daemon: { machine_id: MACHINE } as never,
    machineId: MACHINE,
    prefs: { ...defaultPrefs(), keymap: {}, scoped: { [MACHINE]: { ...emptyScoped(), shells } } },
    send: () => {},
  } as never);
}

/**
 * A layout as dockview itself saves one — never written by hand, which would
 * pin dockview's schema rather than this panel's behaviour. Each inner list
 * is one group, left to right.
 */
function savedLayout(groups: string[][]): string {
  const host = document.createElement("div");
  const dv = new DockviewComponent(host, {
    createComponent: () => ({ element: document.createElement("div"), init: () => {} }) as never,
  });
  let prev: string | null = null;
  for (const group of groups) {
    group.forEach((name, i) => {
      const id = `shell:${name}`;
      dv.addPanel({
        id,
        component: "shell",
        title: name,
        ...(i > 0 ? { position: { referencePanel: `shell:${group[0]}` } } : prev ? { position: { referencePanel: prev, direction: "right" as const } } : {}),
      });
    });
    prev = `shell:${group[0]}`;
  }
  const json = JSON.stringify(dv.toJSON());
  dv.dispose();
  return json;
}

/** The shells each split shows, in DOM order. */
const splits = (container: HTMLElement) =>
  [...container.querySelectorAll(".dv-groupview")].map((g) =>
    [...g.querySelectorAll("[data-shell]")].map((el) => el.getAttribute("data-shell")),
  );

beforeEach(() => {
  cleanup();
  localStorage.clear();
  exits.clear();
});

describe("the terminal panel as a dock", () => {
  it("gives every shell a split of its own", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    expect(splits(container)).toEqual([["mog-0"], ["mog-1"]]);
  });

  /**
   * `R-B64`: the header is back, drawn as a grab strip that opens on hover —
   * the no-tab-bar split (`R-B56`) had nothing to drag it by. The strip is CSS
   * and is seen in a browser; what is pinned here is that there is a tab to
   * grab, named for its shell.
   */
  it("gives every split a tab to drag it by", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    const bars = [...container.querySelectorAll<HTMLElement>(".dv-tabs-and-actions-container")];
    expect(bars).toHaveLength(2);
    for (const bar of bars) expect(bar.style.display).not.toBe("none");
    expect([...container.querySelectorAll(".dv-tab")].map((t) => t.textContent)).toEqual(["alpha", "beta"]);
  });

  /** Detaches, as the right-click Close does — never leaves a shell listed but gone from the tree. */
  it("closing a tab takes its shell off the list", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    const close = container.querySelectorAll<HTMLElement>(".dv-default-tab-action")[0];
    act(() => {
      close.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
      close.click();
    });
    expect(useStore.getState().scoped().shells).toEqual([["mog-1", "/w/beta"]]);
    expect(splits(container)).toEqual([["mog-1"]]);
  });

  it("restores a split, and drops a shell the preferences no longer name", () => {
    localStorage.setItem(shellLayoutKey(MACHINE), savedLayout([["mog-0"], ["mog-1"], ["mog-9"]]));
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    expect(splits(container)).toEqual([["mog-0"], ["mog-1"]]);
  });

  /**
   * The shape, not only the count. A restore that fails falls back to an empty
   * tree and `syncShells` then lays every shell out in one row — which the
   * row-only test above cannot tell from success. Found in a browser the day
   * the tab component was removed: dockview refused to deserialize without
   * one, and every arrangement came back flat.
   */
  it("restores a stacked split as stacked", () => {
    localStorage.setItem(shellLayoutKey(MACHINE), savedLayout([["mog-0"], ["mog-1"]]));
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    // Stack mog-1's split below mog-0 and save that, through the real panel.
    const first = render(<TerminalPanel />);
    act(() => splitShell("below", "mog-0"));
    const saved = () => localStorage.getItem(shellLayoutKey(MACHINE)) ?? "";
    first.unmount();
    expect(saved()).toContain("mog-2");
    const { container } = render(<TerminalPanel />);
    const stacked = container.querySelector(".dv-split-view-container.dv-vertical");
    expect(stacked?.querySelectorAll("[data-shell]")).toHaveLength(2);
    expect([...stacked!.querySelectorAll("[data-shell]")].map((e) => e.getAttribute("data-shell"))).toEqual([
      "mog-0",
      "mog-2",
    ]);
  });

  /**
   * A layout saved by the build before this one — shells with tabs — names
   * `props.defaultTabComponent` on every panel. Without a default tab
   * component dockview refuses it whole, and the arrangement comes back flat.
   */
  it("restores a layout saved while shells still had tabs", () => {
    const old = JSON.parse(savedLayout([["mog-0"], ["mog-1"]]));
    for (const p of Object.values(old.panels) as { tabComponent?: string }[]) p.tabComponent = "props.defaultTabComponent";
    // Stack the second split under the first, the shape that shows a fallback.
    old.grid.root = { type: "branch", size: old.grid.root.size, data: [{ type: "branch", data: old.grid.root.data }] };
    localStorage.setItem(shellLayoutKey(MACHINE), JSON.stringify(old));
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    const stacked = container.querySelector(".dv-split-view-container.dv-vertical");
    expect(stacked?.querySelectorAll("[data-shell]")).toHaveLength(2);
  });

  /** With a tab to click, a second shell in a split is reachable again — so it stays a tab. `R-B64`. */
  it("keeps two shells saved in one split as tabs", () => {
    localStorage.setItem(shellLayoutKey(MACHINE), savedLayout([["mog-0", "mog-1"]]));
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    expect(container.querySelectorAll(".dv-groupview")).toHaveLength(1);
    expect(container.querySelectorAll(".dv-tab")).toHaveLength(2);
  });

  it("a new shell opens beside the one in front, and is the one in front", () => {
    withShells([["mog-0", "/w/alpha"]]);
    const { container } = render(<TerminalPanel />);
    act(() => {
      screen.getByTitle(/a new shell/).click();
    });
    expect(useStore.getState().scoped().shells.map(([n]) => n)).toEqual(["mog-0", "mog-1"]);
    expect(splits(container)).toEqual([["mog-0"], ["mog-1"]]);
    expect(container.querySelectorAll(".dv-groupview")[1].classList.contains("dv-active-group")).toBe(true);
  });

  it("says so when there are none", () => {
    withShells([]);
    render(<TerminalPanel />);
    expect(screen.getByText("no shells open")).toBeInTheDocument();
  });
});

/**
 * Each shell wears a colour of its own, and its split's outline wears the
 * colour of the tab in front. `R-B68`, asked 2026-09-29: *"I want it auto
 * change the outline color as well as the tab color, so that I know which tab
 * I am in"* — one blue for every front tab said *a* tab was in front, not
 * which. The tab says which colour; `index.css` paints it, as with the tags.
 */
describe("a shell's colour", () => {
  it("is carried on each shell's tab, and differs between shells", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    const hues = [...container.querySelectorAll(".dv-default-tab")].map((t) => t.getAttribute("data-shell-hue"));
    expect(hues).toEqual([shellHue("mog-0"), shellHue("mog-1")]);
    expect(hues[0]).not.toBe(hues[1]);
  });

  /** From the name, never the position: closing a shell must not repaint the rest. */
  it("belongs to the shell, not to where it sits", () => {
    expect(shellHue("mog-7")).toBe(SHELL_HUES[7 % SHELL_HUES.length]);
    expect(new Set(SHELL_HUES.map((_, i) => shellHue(`mog-${i}`))).size).toBe(SHELL_HUES.length);
    expect(SHELL_HUES).toContain(shellHue("build"));
  });

  it("has a rule for each colour: the tab's own, and the outline it gives its split", () => {
    const css = readFileSync("src/index.css", "utf8");
    for (const hue of SHELL_HUES) {
      expect(css).toContain(`.dv-default-tab[data-shell-hue="${hue}"]`);
      expect(css).toContain(`.dv-active-tab .dv-default-tab[data-shell-hue="${hue}"]`);
    }
  });
});

/**
 * `exit` closes the split. Reported 2026-09-28: *"When I exit a terminal, the
 * empty tab didn't close itself and I have to use the mouse to close it."*
 */
describe("when a shell ends", () => {
  it("closes its split and takes it off the list", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    act(() => exits.get("shell:mog-0")!(60_000));
    expect(useStore.getState().scoped().shells).toEqual([["mog-1", "/w/beta"]]);
    expect(splits(container)).toEqual([["mog-1"]]);
  });

  it("leaves the panel up while another shell is still running", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    render(<TerminalPanel />);
    act(() => exits.get("shell:mog-0")!(60_000));
    expect(useStore.getState().showTerminal).toBe(true);
  });

  /** `R-B62`: an empty panel is a row of *no shells open* nobody asked to keep. */
  it("puts the panel away when the last one ends", () => {
    withShells([["mog-0", "/w/alpha"]]);
    const { container } = render(<TerminalPanel />);
    act(() => exits.get("shell:mog-0")!(60_000));
    expect(useStore.getState().scoped().shells).toEqual([]);
    expect(useStore.getState().showTerminal).toBe(false);
    expect(container.querySelector(".dv-groupview")).toBeNull();
  });

  /** A shell that dies as it opens failed to start; its notice is the only word on why. */
  it("keeps a shell that died as it opened", () => {
    withShells([["mog-0", "/w/alpha"]]);
    const { container } = render(<TerminalPanel />);
    act(() => exits.get("shell:mog-0")!(300));
    expect(useStore.getState().scoped().shells).toEqual([["mog-0", "/w/alpha"]]);
    expect(splits(container)).toEqual([["mog-0"]]);
  });
});

describe("syncShells", () => {
  it("survives an unreadable saved layout", () => {
    const calls: string[] = [];
    const api = {
      panels: [],
      clear: () => calls.push("clear"),
      fromJSON: () => {
        throw new Error("schema");
      },
    } as unknown as DockviewApi;
    restoreShellLayout(api, "{not json");
    restoreShellLayout(api, "{}");
    expect(calls).toEqual(["clear", "clear"]);
    expect(() =>
      syncShells({ ...api, groups: [], getPanel: () => undefined, addPanel: () => {} } as never, [], String),
    ).not.toThrow();
  });
});

/**
 * Split right and split down. `R-B56`, asked 2026-09-28 with the chords named:
 * *Alt+Shift+"-"* and *Alt+Shift+"|"*, plus a right-click menu and no button.
 */
describe("splitting a shell", () => {
  it("puts a new shell beside the one in front, in the same folder and on the same machine", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta", "devbox"],
    ]);
    const { container } = render(<TerminalPanel />);
    // Recorded as it is sent: the new terminal takes the signal and clears it
    // on mount, so reading the store afterwards may find nothing.
    const asked: string[] = [];
    const off = useStore.subscribe((st) => st.focusTerminal && asked.push(st.focusTerminal.id));
    // mog-1 is in front: it was added last.
    act(() => splitShell("right"));
    off();
    expect(useStore.getState().scoped().shells).toEqual([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta", "devbox"],
      ["mog-2", "/w/beta", "devbox"],
    ]);
    expect(splits(container)).toEqual([["mog-0"], ["mog-1"], ["mog-2"]]);
    // And the keyboard goes with it — a split you then have to click into is
    // half a split.
    expect(asked).toContain("shell:mog-2");
  });

  it("goes the way it was asked", () => {
    withShells([["mog-0", "/w/alpha"]]);
    const { container } = render(<TerminalPanel />);
    act(() => splitShell("below"));
    const [top, bottom] = [...container.querySelectorAll(".dv-groupview")];
    expect(splits(container)).toEqual([["mog-0"], ["mog-1"]]);
    // Stacked, not side by side: the two groups sit in a vertical branch.
    const branch = top.closest(".dv-split-view-container");
    expect(branch?.classList.contains("dv-vertical")).toBe(true);
    expect(bottom.closest(".dv-split-view-container")).toBe(branch);
  });

  /** What the right-click menu does: the shell under the mouse, not the one in front. */
  it("splits a named shell rather than the one in front", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    act(() => splitShell("right", "mog-0"));
    expect(useStore.getState().scoped().shells.at(-1)).toEqual(["mog-2", "/w/alpha"]);
    expect(splits(container)).toEqual([["mog-0"], ["mog-2"], ["mog-1"]]);
  });

  it("with the panel down, brings it up and splits the shell the layout has in front", () => {
    localStorage.setItem(shellLayoutKey(MACHINE), savedLayout([["mog-0"], ["mog-1"]]));
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    useStore.setState({ showTerminal: false });
    const { container } = render(<TerminalPanel />);
    act(() => splitShell("below"));
    expect(useStore.getState().showTerminal).toBe(true);
    // mog-1 is the restored layout's active panel — the last one it added.
    expect(useStore.getState().scoped().shells.at(-1)).toEqual(["mog-2", "/w/beta"]);
    expect(splits(container)).toEqual([["mog-0"], ["mog-1"], ["mog-2"]]);
  });

  it("with no shell at all, opens one", () => {
    withShells([]);
    useStore.setState({ showTerminal: false });
    render(<TerminalPanel />);
    act(() => splitShell("right"));
    expect(useStore.getState().scoped().shells.map(([n]) => n)).toEqual(["mog-0"]);
  });
});

describe("the split chords", () => {
  const press = (key: string, code: string) =>
    window.dispatchEvent(new KeyboardEvent("keydown", { key, code, altKey: true, shiftKey: true, bubbles: true }));

  /** Through tinykeys' own matcher, with the key a US layout reports under Shift. */
  it.each([
    ["terminal.split_down", "_", "Minus"],
    ["terminal.split_right", "|", "Backslash"],
  ])("%s fires on Alt+Shift+%s", (id, key, code) => {
    const action = ACTIONS.find((a) => a.id === id)!;
    let fired = 0;
    const off = bindKeys(window, { [defaultKeys(action, "other")[0]]: () => fired++ });
    press(key, code);
    off();
    expect(fired).toBe(1);
  });

  it("reads as the keys you press", () => {
    expect(formatChord("Alt+Shift+Minus", "other")).toBe("Alt+Shift+−");
    expect(formatChord("Alt+Shift+Backslash", "other")).toBe("Alt+Shift+\\");
  });
});
