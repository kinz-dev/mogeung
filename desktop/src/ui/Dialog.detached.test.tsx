/**
 * A dialog in a window of its own. `R-J128`, and `R-J130` after a first look.
 *
 * Asked 2026-10-11: *"make the new 'New Session' or 'Teams' a real popup
 * dialog that can move around out of the current mogeung application. (But
 * make it a modal popup, only allow one copy)"*. Pinned here, against a fake
 * of what the shell's `window.open` hands back and of what the shell then
 * announces about it:
 *
 * - **Where the panel is drawn.** In the window of its own, with the styles
 *   and the theme carried across, and not here — here is a backdrop that
 *   names the window and brings it forward on a click.
 * - **One copy.** Showing the dialog again raises its window rather than
 *   opening a second one; showing it when none is open sets the store flag
 *   that mounts it.
 * - **Modal from either side.** Escape in either window closes it.
 * - **The shell's close button**, which `popout.rs` routes back as
 *   `popout:close`, closes the dialog, and the dialog then closes the OS
 *   window — and hands back the size it was left at.
 * - **A window closed from outside** — a browser's own button — closes the
 *   dialog too, so the flag and the window never disagree.
 * - **The zoom keys inside it** step the dialogs' own zoom, a CSS zoom on
 *   that page, and leave the main window's alone.
 * - **A refusal** keeps the dialog here, as it always was.
 *
 * And after the first look (`R-J130`), the empty window that was left behind:
 *
 * - **Closed before its page loads**, the window is still closed through the
 *   shell, by the label the shell announced — never with the page's own
 *   `close()`, which leaves an OS window standing.
 * - **A StrictMode re-run** keeps the one window rather than closing it and
 *   opening another, which is what `tauri dev` did on every open.
 * - **A page that never loads** leaves the dialog here and closes the window
 *   that would have held it.
 *
 * Every one of these fails against the old `Dialog`, which knew one window;
 * the last three fail against the first cut of this one.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode, useState } from "react";
import { Dialog } from "./Dialog";
import { onPopoutCloseRequest } from "@/lib/popout";
import { listenForDialogWindows, openDialogWindows, showDialog, tuning } from "@/lib/dialogWindow";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

const shellCalls: { cmd: string; args: unknown }[] = [];
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args: unknown) => {
    shellCalls.push({ cmd, args });
  }),
}));

/** What the shell says to the main window, by event name. */
const heard = new Map<string, (e: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    heard.set(name, cb);
    return () => heard.delete(name);
  }),
}));

const raised: string[] = [];
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: {
    getByLabel: vi.fn(async (label: string) => ({
      unminimize: async () => {},
      setFocus: async () => {
        raised.push(label);
      },
    })),
  },
}));

type Fake = Window & { closed: boolean; innerWidth: number; innerHeight: number; focus: ReturnType<typeof vi.fn>; close: ReturnType<typeof vi.fn> };

let handed: Fake[] = [];
let labels = 0;

/**
 * What the shell's `window.open` hands back: a window that loads shortly
 * (unless told not to), told its label by the page — and, the way `popout.rs`
 * does, announced to the opener with the URL that was asked for.
 */
function shellWindow(url: string, { loads = true } = {}): Fake {
  const label = `popout-${++labels}`;
  const target = Object.assign(new EventTarget(), {
    document: document.implementation.createHTMLDocument("popout"),
    close: vi.fn(),
    focus: vi.fn(),
    closed: false,
    innerWidth: 900,
    innerHeight: 650,
    __MOGEUNG_WINDOW_LABEL__: label,
  }) as unknown as Fake;
  setTimeout(() => heard.get("popout:opened")?.({ payload: { label, url } }));
  if (loads) setTimeout(() => target.dispatchEvent(new Event("load")), 2);
  handed.push(target);
  return target;
}

const settle = () => act(async () => void (await new Promise((r) => setTimeout(r, 10))));

const SIZE = { width: 1000, height: 700 };

/** A dialog that leaves the page when closed, the way the store flag unmounts the real ones. */
function Host({ onClosed, onResized }: { onClosed: () => void; onResized?: (s: { width: number; height: number }) => void }) {
  const [open, setOpen] = useState(true);
  if (!open) return null;
  return (
    <Dialog
      title="Teams"
      subtitle="a team of seats"
      onClose={() => {
        setOpen(false);
        onClosed();
      }}
      detached={{ name: "teams", size: SIZE, onResized }}
    >
      <button>inside</button>
    </Dialog>
  );
}

const zoomOf = (w: Fake) => (w.document.documentElement.style as unknown as { zoom: string }).zoom;

beforeEach(async () => {
  handed = [];
  labels = 0;
  shellCalls.length = 0;
  raised.length = 0;
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  useStore.setState({ prefs: defaultPrefs(), showTeams: false, showLaunch: false });
  vi.spyOn(window, "open").mockImplementation((url) => shellWindow(String(url)));
  await listenForDialogWindows();
});

afterEach(async () => {
  cleanup();
  // A dialog lets its window go a tick after leaving; give it the tick.
  await settle();
  vi.restoreAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  document.documentElement.setAttribute("data-theme", "dark");
  document.head.querySelector("#probe")?.remove();
  expect(openDialogWindows()).toEqual([]);
});

describe("a dialog in a window of its own", () => {
  it("draws the panel there, styled and themed, and keeps a backdrop here that names the window", async () => {
    document.documentElement.setAttribute("data-theme", "light");
    const probe = document.createElement("style");
    probe.id = "probe";
    probe.textContent = ".probe { color: red }";
    document.head.appendChild(probe);

    render(<Host onClosed={() => {}} />);
    // The URL says which dialog asked, so the shell's announcement can be told from a pop-out's.
    expect(window.open).toHaveBeenCalledWith("/popout.html?dialog=teams", "mogeung-teams", "width=1000,height=700");
    await settle();

    const popup = handed[0];
    const there = within(popup.document.body);
    // By attribute: a document made by `createHTMLDocument` has no window, and
    // the accessible-name query wants one.
    expect(popup.document.querySelector('[role="dialog"]')?.getAttribute("aria-label")).toBe("Teams");
    expect(there.queryByText("inside")).not.toBeNull();
    expect(popup.document.title).toBe("Teams · mogeung");
    expect(popup.document.documentElement.getAttribute("data-theme")).toBe("light");
    expect([...popup.document.head.querySelectorAll("style")].some((s) => s.textContent?.includes(".probe"))).toBe(true);

    // Here: no panel, a backdrop that says where it went, and a click that raises.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(/is open in a window of its own/)).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("dialog-backdrop"));
    expect(popup.focus).toHaveBeenCalled();
    await settle();
    expect(raised).toEqual(["popout-1"]);
  });

  it("is one copy: shown again it comes forward, and shown when closed it opens", async () => {
    useStore.setState({ showTeams: true });
    render(<Host onClosed={() => useStore.setState({ showTeams: false })} />);
    await settle();
    expect(handed).toHaveLength(1);

    showDialog("teams");
    await settle();
    expect(window.open).toHaveBeenCalledTimes(1);
    expect(handed[0].focus).toHaveBeenCalled();
    expect(raised).toEqual(["popout-1"]);
    expect(useStore.getState().showTeams).toBe(true);

    cleanup();
    await settle();
    expect(openDialogWindows()).toEqual([]);
    useStore.setState({ showTeams: false });
    showDialog("teams");
    expect(useStore.getState().showTeams).toBe(true);
    // The flag mounts the dialog, and the dialog opens the window — not `showDialog`.
    expect(window.open).toHaveBeenCalledTimes(1);
  });

  it("closes on Escape typed in either window", async () => {
    const closed = vi.fn();
    render(<Host onClosed={closed} />);
    await settle();
    act(() => void handed[0].dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
    expect(closed).toHaveBeenCalledTimes(1);
    await settle();

    const again = vi.fn();
    render(<Host onClosed={again} />);
    await settle();
    act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })));
    expect(again).toHaveBeenCalledTimes(1);
  });

  it("closes when the shell's close button is pressed, then closes the window and hands back its size", async () => {
    const closed = vi.fn();
    const resized = vi.fn();
    render(<Host onClosed={closed} onResized={resized} />);
    await settle();
    const popup = handed[0];
    popup.innerWidth = 1280;
    popup.innerHeight = 820;

    act(() => onPopoutCloseRequest({ groups: [] }, "popout-1"));
    expect(closed).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/window of its own/)).toBeNull();
    await settle();
    expect(resized).toHaveBeenCalledWith({ width: 1280, height: 820 });
    expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-1" } }]);
    // Through the shell, never the page: see the module notes for what that leaves.
    expect(popup.close).not.toHaveBeenCalled();
  });

  it("closes when the window is closed from outside", async () => {
    const closed = vi.fn();
    render(<Host onClosed={closed} />);
    await settle();
    handed[0].closed = true;
    await waitFor(() => expect(closed).toHaveBeenCalledTimes(1), { timeout: 2000 });
    expect(screen.queryByText(/window of its own/)).toBeNull();
  });

  it("steps the dialogs' own zoom on Ctrl+= inside it, leaving the main window's alone", async () => {
    useStore.getState().setPrefs({ appZoom: 1.21 });
    render(<Host onClosed={() => {}} />);
    await settle();
    // Opens at the main window's zoom, having none of its own yet — as the page's own zoom.
    expect(zoomOf(handed[0])).toBe("1.21");

    act(() => void handed[0].dispatchEvent(new KeyboardEvent("keydown", { key: "=", ctrlKey: true })));
    await settle();
    const { prefs } = useStore.getState();
    expect(prefs.dialogZoom).toBeCloseTo(1.331, 3);
    expect(prefs.appZoom).toBe(1.21);
    expect(zoomOf(handed[0])).toBe(String(prefs.dialogZoom));
    expect((document.documentElement.style as unknown as { zoom: string }).zoom).toBeFalsy();

    act(() => void handed[0].dispatchEvent(new KeyboardEvent("keydown", { key: "0", ctrlKey: true })));
    await settle();
    expect(useStore.getState().prefs.dialogZoom).toBe(1);
    expect(zoomOf(handed[0])).toBe("1");
  });

  it("stays here when no window can be opened", () => {
    vi.spyOn(window, "open").mockImplementation(() => null);
    render(<Host onClosed={() => {}} />);
    expect(screen.getByRole("dialog", { name: "Teams" })).toBeInTheDocument();
    expect(screen.getByText("inside")).toBeInTheDocument();
    expect(screen.queryByText(/window of its own/)).toBeNull();
  });
});

describe("the window that was left behind (R-J130)", () => {
  it("closed before its page loads, is still closed through the shell by the label it announced", async () => {
    vi.spyOn(window, "open").mockImplementation((url) => shellWindow(String(url), { loads: false }));
    render(<Host onClosed={() => {}} />);
    // Gone before `load` — and before the shell has even announced the label.
    cleanup();
    await settle();
    expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-1" } }]);
    expect(handed[0].close).not.toHaveBeenCalled();
    expect(openDialogWindows()).toEqual([]);
  });

  it("keeps the one window across a StrictMode re-run", async () => {
    render(
      <StrictMode>
        <Host onClosed={() => {}} />
      </StrictMode>,
    );
    await settle();
    expect(window.open).toHaveBeenCalledTimes(1);
    expect(shellCalls).toEqual([]);
    expect(within(handed[0].document.body).queryByText("inside")).not.toBeNull();
    expect(screen.getByText(/is open in a window of its own/)).toBeInTheDocument();

    cleanup();
    await settle();
    expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-1" } }]);
  });

  it("stays here when the page never loads, and closes the window that would have held it", async () => {
    const was = tuning.loadTimeoutMs;
    tuning.loadTimeoutMs = 30;
    try {
      vi.spyOn(window, "open").mockImplementation((url) => shellWindow(String(url), { loads: false }));
      render(<Host onClosed={() => {}} />);
      expect(screen.queryByRole("dialog")).toBeNull();
      await waitFor(() => expect(screen.getByRole("dialog", { name: "Teams" })).toBeInTheDocument());
      expect(screen.getByText("inside")).toBeInTheDocument();
      await settle();
      expect(shellCalls).toEqual([{ cmd: "popout_close", args: { label: "popout-1" } }]);
      expect(handed[0].close).not.toHaveBeenCalled();
      expect(openDialogWindows()).toEqual([]);
    } finally {
      tuning.loadTimeoutMs = was;
    }
  });
});
