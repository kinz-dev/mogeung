/**
 * The joins the list's own tests cannot make. `R-I16`.
 *
 * [`lib/connections.ts`](../lib/connections.ts) has its own tests for storage,
 * migration, ordering and the dialled URL. What is asserted here is the wiring
 * the panel adds on top, and each of these has a way of failing silently:
 *
 * - **The forget button is refused on the connection you are watching.** A
 *   window that forgets the daemon it is looking at has no way back to it.
 * - **The row's buttons do not fire the row.** Clicking ✕ or ▲ inside a
 *   clickable row must not also *switch daemons* — the same shape that folded a
 *   file in the diff pane once already, and it fails by doing the destructive
 *   thing on the way to the harmless one. Here it would drop the whole board.
 * - **A token is masked**, and never rendered into the address field beside it.
 * - **A browser tab says where the list actually is**, because a fallback that
 *   silently held a token somewhere with no mode bits is worse than one that
 *   refused.
 */

import { describe, expect, it, beforeEach, afterEach, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ConnectionsWindow } from "@/ui/ConnectionsWindow";
import { useStore } from "@/store";

const tauri = vi.hoisted(() => ({ on: false }));
vi.mock("@/lib/tauri", async (orig) => ({
  ...(await orig<typeof import("@/lib/tauri")>()),
  isTauri: () => tauri.on,
}));

/** The shell, standing in for `~/.mogeung/connections.json`. */
const file = vi.hoisted(() => ({ list: [] as unknown[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: { list?: unknown[] }) => {
    if (cmd === "connections_load") return file.list;
    if (cmd === "connections_save") {
      file.list = args?.list ?? [];
      return undefined;
    }
    throw new Error(`unexpected command ${cmd}`);
  }),
}));

const HERE = "ws://127.0.0.1:7717/ws";
const THERE = "ws://devbox:7717/ws";

async function open(list: unknown[]) {
  localStorage.setItem("mogeung.connections", JSON.stringify(list));
  useStore.setState({ showConnections: true, url: HERE } as never);
  render(<ConnectionsWindow />);
  // The list is a load rather than a value since ADR-0036. Waiting for the
  // load to land rather than for a count: `withCurrent` adds the address in
  // use, so the number of rows is not always the number passed in.
  await waitFor(() =>
    expect(screen.getAllByLabelText(/^address for /).length).toBeGreaterThan(0),
  );
}

const stored = (): { url: string; name: string }[] =>
  JSON.parse(localStorage.getItem("mogeung.connections") ?? "[]");

beforeEach(() => {
  localStorage.clear();
  tauri.on = false;
  file.list = [];
});
afterEach(() => {
  cleanup();
  useStore.setState({ showConnections: false });
});

describe("the connections panel", () => {
  it("refuses to forget the daemon you are watching", async () => {
    await open([{ id: "a", name: "here", url: HERE }]);
    expect(screen.getByTitle(/switch away before forgetting it/)).toBeDisabled();
  });

  it("forgets one you are not watching", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE },
    ]);
    fireEvent.click(screen.getByTitle("forget this daemon"));
    await waitFor(() => expect(stored()).toHaveLength(1));
    expect(stored()[0].url).toBe(HERE);
  });

  /**
   * The button is inside a row whose own click switches daemons and closes the
   * dialog, which would drop the entire board on the way to reordering.
   */
  it("does not switch daemons when a row's own button is pressed", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE },
    ]);
    fireEvent.click(screen.getByTitle("forget this daemon"));
    await waitFor(() => expect(stored()).toHaveLength(1));
    expect(useStore.getState().url).toBe(HERE);
    expect(useStore.getState().showConnections).toBe(true);
  });

  it("edits an address in place and writes it through", async () => {
    await open([{ id: "a", name: "here", url: HERE }]);
    fireEvent.change(screen.getByLabelText("address for here"), {
      target: { value: THERE },
    });
    await waitFor(() => expect(stored()[0].url).toBe(THERE));
  });

  it("orders the list, and stops at the ends", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE },
    ]);
    const up = screen.getAllByTitle("move up");
    expect(up[0]).toBeDisabled();
    fireEvent.click(up[1]);
    await waitFor(() => expect(stored().map((c) => c.name)).toEqual(["there", "here"]));
  });

  /** A shared secret must not be readable over your shoulder, or in a screenshot. */
  it("masks the token and keeps it out of the address", async () => {
    await open([{ id: "a", name: "there", url: THERE, token: "6f1c" }]);
    const token = screen.getByLabelText("token for there");
    expect(token).toHaveAttribute("type", "password");
    expect(screen.getByLabelText("address for there")).toHaveValue(THERE);
  });

  it("says the list is in the file when there is a shell", async () => {
    tauri.on = true;
    file.list = [{ id: "a", name: "here", url: HERE }];
    useStore.setState({ showConnections: true, url: HERE } as never);
    render(<ConnectionsWindow />);
    await waitFor(() =>
      expect(screen.getByText(/connections\.json/)).toBeInTheDocument(),
    );
  });

  /** A tab is a real client, and has to admit what it cannot protect. */
  it("warns that a browser tab is not protecting the token", async () => {
    await open([{ id: "a", name: "here", url: HERE }]);
    expect(screen.getByText(/browser tab/i)).toBeInTheDocument();
    expect(screen.getByText(/not protected by the file/i)).toBeInTheDocument();
  });
});
