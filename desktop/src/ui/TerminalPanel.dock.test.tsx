/**
 * The shells are tiles. `R-B56`.
 *
 * Asked 2026-09-28: *"I can organize the terminal windows freely in the bottom
 * panel like what I can do in the top claude code session panel"*. The strip
 * this replaced showed one shell and a row of names, so the four about
 * arrangement fail against it — checked, by running them there: there was no
 * tree to restore a split into, and no tab that closing could go through.
 *
 * Driven through the real dockview rather than a fake api, because what is
 * being pinned is that its layout, its tabs and `scoped.shells` agree — and a
 * fake would agree with anything.
 */

import { act } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { DockviewComponent, type DockviewApi } from "dockview";
import { useStore } from "@/store";
import { defaultPrefs, emptyScoped } from "@/store/prefs";
import { TerminalPanel } from "@/ui/TerminalPanel";
import { restoreShellLayout, shellLayoutKey, syncShells, type Shell } from "@/lib/shellDock";

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
 * Two shells split left and right, plus one the preferences no longer name —
 * a shell closed on another client. Built by dockview and serialised rather
 * than written by hand: a hand-written layout would pin dockview's schema, not
 * this panel's behaviour.
 */
function splitLayout(): string {
  const host = document.createElement("div");
  const dv = new DockviewComponent(host, {
    createComponent: () => ({ element: document.createElement("div"), init: () => {} }) as never,
  });
  dv.addPanel({ id: "shell:mog-0", component: "shell", title: "a" });
  dv.addPanel({ id: "shell:mog-1", component: "shell", title: "b", position: { direction: "right" } });
  dv.addPanel({ id: "shell:mog-9", component: "shell", title: "gone", position: { referencePanel: "shell:mog-1" } });
  const json = JSON.stringify(dv.toJSON());
  dv.dispose();
  return json;
}

beforeEach(() => {
  cleanup();
  localStorage.clear();
});

describe("the terminal panel as a dock", () => {
  it("shows every shell as a tab, not only the one in front", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    const tabs = [...container.querySelectorAll(".dv-tab")].map((t) => t.textContent);
    expect(tabs).toEqual(["alpha", "beta"]);
  });

  it("restores a split, and drops a shell the preferences no longer name", () => {
    localStorage.setItem(shellLayoutKey(MACHINE), splitLayout());
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    const { container } = render(<TerminalPanel />);
    // Two groups: the split survived the round trip.
    expect(container.querySelectorAll(".dv-groupview")).toHaveLength(2);
    // The stale one is gone from the tree, not merely hidden.
    expect(screen.queryByText("gone")).not.toBeInTheDocument();
  });

  it("closing a tab removes the shell from the list, which is what detaches it", () => {
    withShells([
      ["mog-0", "/w/alpha"],
      ["mog-1", "/w/beta"],
    ]);
    render(<TerminalPanel />);
    act(() => {
      fireEvent.click(screen.getByLabelText("close mog-0"));
    });
    expect(useStore.getState().scoped().shells).toEqual([["mog-1", "/w/beta"]]);
    expect(screen.queryByText("alpha")).not.toBeInTheDocument();
  });

  it("a new shell arrives as a tab, and is the one in front", () => {
    withShells([["mog-0", "/w/alpha"]]);
    render(<TerminalPanel />);
    act(() => {
      fireEvent.click(screen.getByTitle(/a new shell/));
    });
    expect(useStore.getState().scoped().shells.map(([n]) => n)).toEqual(["mog-0", "mog-1"]);
    const tabs = document.querySelectorAll(".dv-tab");
    expect(tabs).toHaveLength(2);
    expect(tabs[1].classList.contains("dv-active-tab")).toBe(true);
  });

  it("says so when there are none", () => {
    withShells([]);
    render(<TerminalPanel />);
    expect(screen.getByText("no shells open")).toBeInTheDocument();
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
    expect(() => syncShells({ ...api, getPanel: () => undefined, addPanel: () => {} } as never, [], String)).not.toThrow();
  });
});
