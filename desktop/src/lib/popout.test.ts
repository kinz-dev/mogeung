/**
 * Popping a group out, now that dockview owns the window. `R-B55`,
 * [ADR-0037](../../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md)
 * and its 2026-09-08 amendment.
 *
 * There is far less to test here than there was, and that is the point: the
 * hand-over dance the first cut needed — close the pane, open a window, hear it
 * die, put the pane back — is all gone, because the group in the other window
 * *is* the group that left. What is left to get wrong is the two things
 * dockview does not do for us.
 *
 * - **The theme.** dockview copies the opener's stylesheets into the popout
 *   document, so every rule arrives. It does not copy `data-theme` on `<html>`,
 *   which is where all of those rules get their colours — so a popout without
 *   this renders in the *wrong palette* rather than unstyled, which is much
 *   easier to mistake for a design decision than for a bug.
 * - **The refusal.** `window.open` is opt-in in a Tauri webview and the shell
 *   has to have said yes (`popout.rs`). If it has not, the pane must stay put
 *   and say so, rather than leaving a control that appears to do nothing.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createDockview, type DockviewApi } from "dockview";
import { useStore } from "@/store";
import {
  forgetPopout,
  labelOf,
  mirrorTheme,
  onPopoutCloseRequest,
  popInGroup,
  popOutPane,
  retheme,
  trackPopout,
  watchPopouts,
} from "@/lib/popout";

const shellCalls = vi.hoisted(() => [] as { cmd: string; args?: unknown }[]);
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    shellCalls.push({ cmd, args });
  }),
}));

/** A stand-in for the popout's `Window`, which jsdom will not open for us. */
function fakeWindow() {
  const doc = document.implementation.createHTMLDocument("popout");
  return { document: doc } as unknown as Window;
}

function fakeApi(opts: { has: boolean; result?: boolean; throws?: boolean }) {
  // Typed with its parameters, not just its result: the assertion below reads
  // `calls[0][1]`, and a `vi.fn(async () => …)` gives `calls` an empty tuple
  // type, so the test compiles only by accident. `npm test` never typechecks —
  // `tsc` caught this one and vitest was perfectly happy.
  const addPopoutGroup = vi.fn(async (_item: unknown, _options?: { popoutUrl?: string }) => {
    if (opts.throws) throw new Error("no");
    return opts.result ?? true;
  });
  return {
    api: {
      getPanel: (id: string) => (opts.has ? ({ id } as never) : undefined),
      addPopoutGroup,
    } as unknown as DockviewApi,
    addPopoutGroup,
  };
}

beforeEach(() => {
  useStore.setState({ notices: [] } as never);
  document.documentElement.setAttribute("data-theme", "dark");
});

describe("carrying the theme into the popout document", () => {
  it("copies data-theme onto the new document", () => {
    const w = fakeWindow();
    mirrorTheme(w);
    expect(w.document.documentElement.getAttribute("data-theme")).toBe("dark");
  });

  it("re-applies it to every tracked window when the theme changes", () => {
    const a = fakeWindow();
    const b = fakeWindow();
    trackPopout(a);
    trackPopout(b);

    document.documentElement.setAttribute("data-theme", "light");
    retheme();

    expect(a.document.documentElement.getAttribute("data-theme")).toBe("light");
    expect(b.document.documentElement.getAttribute("data-theme")).toBe("light");
  });

  it("stops writing to a window that has closed", () => {
    const w = fakeWindow();
    trackPopout(w);
    forgetPopout(w);

    document.documentElement.setAttribute("data-theme", "light");
    retheme();

    expect(w.document.documentElement.getAttribute("data-theme")).toBe("dark");
  });
});

describe("asking dockview for a popout", () => {
  it("pops the group the pane is in", async () => {
    const { api, addPopoutGroup } = fakeApi({ has: true });

    await expect(popOutPane(api, "agent")).resolves.toBe(true);

    expect(addPopoutGroup).toHaveBeenCalledTimes(1);
    // Same-origin, and ours: dockview's default is `/popout.html`, but naming
    // it here is what stops a Vite base-path change silently pointing the
    // window at a page that does not exist.
    expect(addPopoutGroup.mock.calls[0][1]).toMatchObject({ popoutUrl: "/popout.html" });
  });

  it("does nothing when the pane is not there", async () => {
    const { api, addPopoutGroup } = fakeApi({ has: false });
    await expect(popOutPane(api, "agent")).resolves.toBe(false);
    expect(addPopoutGroup).not.toHaveBeenCalled();
  });

  /**
   * The failure that would otherwise be silent: a shell that has not opted in
   * to `window.open` refuses, and a button that appears to do nothing is the
   * worst way to find that out.
   */
  it("says so when the window cannot be opened", async () => {
    const { api } = fakeApi({ has: true, result: false });

    await expect(popOutPane(api, "agent")).resolves.toBe(false);

    const said = useStore.getState().notices.map((n) => n.text).join(" ");
    expect(said).toMatch(/could not open another/i);
  });

  it("says so when the attempt throws rather than returning false", async () => {
    const { api } = fakeApi({ has: true, throws: true });

    await expect(popOutPane(api, "agent")).resolves.toBe(false);

    const said = useStore.getState().notices.map((n) => n.text).join(" ");
    expect(said).toMatch(/could not open another/i);
  });
});

// --- popping back in (`R-J108`) ------------------------------------------------

/**
 * Asked 2026-09-25: *"After popout, there is no easy way to pop back in. I
 * have to drag and drop the pane and then manually close the empty pop-up
 * window."*
 *
 * Two faults, and these pin both against a **real dockview** — the claim worth
 * testing is that dockview's own way home runs, not that an event was sent:
 *
 * - **The way home.** dockview returns a popout group when the popout document
 *   raises `beforeunload`, which a shell window never does (`popout.rs`). So
 *   `popInGroup` raises it, and the group is back where it came from.
 * - **The empty window.** dockview then calls `window.close()`, which the
 *   shell's webview does not honour. `watchPopouts` closes it through the shell
 *   once its group has gone — however it went, including dragged back by hand.
 */
describe("popping back in", () => {
  type Opened = Window & { close: ReturnType<typeof vi.fn> };
  let opened: Opened[] = [];
  let label = 0;

  /** What the shell's `window.open` hands dockview: a document, and its label. */
  function shellWindow(): Opened {
    const target = Object.assign(new EventTarget(), {
      document: document.implementation.createHTMLDocument("popout"),
      close: vi.fn(),
      focus: () => {},
      innerWidth: 400,
      innerHeight: 300,
      __MOGEUNG_WINDOW_LABEL__: `popout-${++label}`,
    }) as unknown as Opened;
    setTimeout(() => target.dispatchEvent(new Event("load")));
    opened.push(target);
    return target;
  }

  const component = () => {
    const el = document.createElement("div");
    return { element: el, init: () => {} };
  };

  async function dockWithTwo() {
    const host = document.createElement("div");
    document.body.appendChild(host);
    const api = createDockview(host, { createComponent: component });
    api.layout(1000, 800);
    api.addPanel({ id: "agent", component: "x" });
    api.addPanel({ id: "notes", component: "x", position: { direction: "right" } });
    return api;
  }

  const groupOf = (api: DockviewApi, id: string) => api.getPanel(id)!.group;
  const settle = () => new Promise((r) => setTimeout(r, 0));

  beforeEach(() => {
    opened = [];
    label = 0;
    shellCalls.length = 0;
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    vi.spyOn(window, "open").mockImplementation(() => shellWindow());
  });

  afterEach(() => {
    vi.restoreAllMocks();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
    document.body.innerHTML = "";
  });

  it("the button's way home puts the pane back where it was, and closes the window it leaves", async () => {
    const api = await dockWithTwo();
    watchPopouts(api);
    await expect(popOutPane(api, "notes")).resolves.toBe(true);
    await settle();
    const out = groupOf(api, "notes");
    expect(out.api.location.type).toBe("popout");

    expect(popInGroup(out)).toBe(true);
    await settle();

    const home = groupOf(api, "notes");
    expect(home.api.location.type).toBe("grid");
    expect(home.api.isVisible).toBe(true);
    // Back beside the agent, not somewhere new.
    expect(api.groups.filter((g) => g.api.location.type === "grid")).toHaveLength(2);
    expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-1" } }]);
  });

  it("dragging the pane back by hand closes the emptied window too", async () => {
    const api = await dockWithTwo();
    watchPopouts(api);
    await popOutPane(api, "notes");
    await settle();

    api.getPanel("notes")!.api.moveTo({ group: groupOf(api, "agent"), position: "center" });
    await settle();

    expect(api.groups.some((g) => g.api.location.type === "popout")).toBe(false);
    expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-1" } }]);
  });

  it("the window's own close button sends its group home first, rather than losing it", async () => {
    const api = await dockWithTwo();
    watchPopouts(api);
    await popOutPane(api, "notes");
    await settle();

    onPopoutCloseRequest(api, "popout-1");
    await settle();

    expect(groupOf(api, "notes").api.location.type).toBe("grid");
    expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-1" } }]);
  });

  it("a close for a window that holds no group closes it straight away", async () => {
    const api = await dockWithTwo();
    onPopoutCloseRequest(api, "popout-9");
    await settle();
    expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-9" } }]);
  });

  it("closes nothing while the pane is still out, and never raises beforeunload on the main window", async () => {
    const api = await dockWithTwo();
    watchPopouts(api);
    await popOutPane(api, "notes");
    await settle();
    api.getPanel("agent")!.api.setActive();
    await settle();
    expect(shellCalls).toEqual([]);

    const unload = vi.fn();
    window.addEventListener("beforeunload", unload);
    expect(popInGroup(groupOf(api, "agent"))).toBe(false);
    window.removeEventListener("beforeunload", unload);
    expect(unload).not.toHaveBeenCalled();
  });

  it("reads a window's label, and nothing from a window without one", () => {
    expect(labelOf({ __MOGEUNG_WINDOW_LABEL__: "popout-3" } as unknown as Window)).toBe("popout-3");
    expect(labelOf({ __MOGEUNG_WINDOW_LABEL__: 3 } as unknown as Window)).toBeNull();
    expect(labelOf(null)).toBeNull();
    const gone = Object.defineProperty({}, "__MOGEUNG_WINDOW_LABEL__", {
      get() {
        throw new Error("cross-origin");
      },
    });
    expect(labelOf(gone as Window)).toBeNull();
  });
});
