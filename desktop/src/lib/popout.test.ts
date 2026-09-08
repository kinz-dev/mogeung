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

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DockviewApi } from "dockview";
import { useStore } from "@/store";
import { forgetPopout, mirrorTheme, popOutPane, retheme, trackPopout } from "@/lib/popout";

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
