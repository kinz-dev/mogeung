/**
 * The joins the list's own tests cannot make. `R-I16`, reshaped by `R-I19`.
 *
 * [`lib/connections.ts`](../lib/connections.ts) has its own tests for storage,
 * migration, ordering, the dialled URL and the probe. What is asserted here is
 * the wiring the panel adds on top, and each of these has a way of failing
 * silently:
 *
 * - **The forget button is refused on the connection you are watching.** A
 *   window that forgets the daemon it is looking at has no way back to it.
 * - **Forgetting, reordering and testing do not switch daemons.** Only
 *   *connect* does, and it says so; anything else that dropped the board on
 *   the way to a harmless thing would be the diff pane's fold bug again.
 * - **A token is masked**, never rendered into the address beside it, shown
 *   only while the eye is pressed, and masked again when you look elsewhere.
 * - **Test asks who is there and switches nothing.**
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

/** The other end of Test, standing in for a daemon that answers. */
const probe = vi.hoisted(() => ({
  answer: { ok: true, host: "devbox", version: "0.1.0", queued: 3 } as unknown,
  asked: [] as string[],
}));
vi.mock("@/lib/connections", async (orig) => ({
  ...(await orig<typeof import("@/lib/connections")>()),
  probeConnection: vi.fn(async (url: string) => {
    probe.asked.push(url);
    return probe.answer;
  }),
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
    // The panel also asks what the LAN has said (`R-I17`); nothing, here.
    if (cmd === "lan_peers") return [];
    throw new Error(`unexpected command ${cmd}`);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const HERE = "ws://127.0.0.1:7717/ws";
const THERE = "ws://devbox:7717/ws";

async function open(list: unknown[]) {
  localStorage.setItem("mogeung.connections", JSON.stringify(list));
  useStore.setState({ showConnections: true, url: HERE } as never);
  render(<ConnectionsWindow />);
  // The list is a load rather than a value since ADR-0036; the form lands on
  // the connection being watched, so one address field is the sign it did.
  await waitFor(() => expect(screen.getAllByLabelText(/^address for /).length).toBeGreaterThan(0));
}

/** Click a row on the left; the form on the right becomes that connection. */
// Anchored, because "here" is inside "there".
const pick = (name: string) => fireEvent.click(screen.getByRole("option", { name: new RegExp(`^${name}\\b`) }));

const stored = (): { url: string; name: string }[] =>
  JSON.parse(localStorage.getItem("mogeung.connections") ?? "[]");

beforeEach(() => {
  localStorage.clear();
  tauri.on = false;
  file.list = [];
  probe.asked = [];
  probe.answer = { ok: true, host: "devbox", version: "0.1.0", queued: 3 };
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

  it("forgets one you are not watching, and neither switches nor closes", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE },
    ]);
    pick("there");
    fireEvent.click(screen.getByTitle("forget this daemon"));
    await waitFor(() => expect(stored()).toHaveLength(1));
    expect(stored()[0].url).toBe(HERE);
    expect(useStore.getState().url).toBe(HERE);
    expect(useStore.getState().showConnections).toBe(true);
  });

  it("edits an address in place and writes it through", async () => {
    await open([{ id: "a", name: "here", url: HERE }]);
    fireEvent.change(screen.getByLabelText("address for here"), { target: { value: THERE } });
    await waitFor(() => expect(stored()[0].url).toBe(THERE));
  });

  it("orders the list, and stops at the ends", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE },
    ]);
    expect(screen.getByTitle("move up")).toBeDisabled();
    pick("there");
    fireEvent.click(screen.getByTitle("move up"));
    await waitFor(() => expect(stored().map((c) => c.name)).toEqual(["there", "here"]));
  });

  /** A shared secret must not be readable over your shoulder, or in a screenshot. */
  it("masks the token, shows it only while the eye is pressed, and keeps it out of the address", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE, token: "6f1c" },
    ]);
    pick("there");
    const token = screen.getByLabelText("token for there");
    expect(token).toHaveAttribute("type", "password");
    expect(screen.getByLabelText("address for there")).toHaveValue(THERE);
    fireEvent.click(screen.getByTitle(/show the token/));
    expect(screen.getByLabelText("token for there")).toHaveAttribute("type", "text");
    // Looking elsewhere masks it again: a look, not a setting.
    pick("here");
    pick("there");
    expect(screen.getByLabelText("token for there")).toHaveAttribute("type", "password");
  });

  it("asks a daemon who it is on test, and switches nothing", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE, token: "6f1c" },
    ]);
    pick("there");
    fireEvent.click(screen.getByTitle(/without switching/));
    expect(await screen.findByText(/answered — devbox · 0\.1\.0 · 3 in its queue/)).toBeInTheDocument();
    // Dialled with the token, the way connect would.
    expect(probe.asked[0]).toContain("token=6f1c");
    expect(useStore.getState().url).toBe(HERE);
  });

  it("says before dialling that a LAN entry has no token saved", async () => {
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE },
    ]);
    pick("there");
    fireEvent.click(screen.getByTitle(/without switching/));
    expect(await screen.findByText(/no token saved/)).toBeInTheDocument();
    expect(probe.asked).toEqual([]);
  });

  it("says what a failed test could mean", async () => {
    probe.answer = { ok: false, reason: "refused — unreachable, or the token was wrong" };
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE, token: "wrong" },
    ]);
    pick("there");
    fireEvent.click(screen.getByTitle(/without switching/));
    expect(await screen.findByText(/refused — unreachable/)).toBeInTheDocument();
  });

  it("connects only from the connect button, and closes", async () => {
    const setUrl = vi.fn();
    useStore.setState({ setUrl } as never);
    await open([
      { id: "a", name: "here", url: HERE },
      { id: "b", name: "there", url: THERE },
    ]);
    pick("there");
    expect(setUrl).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("connect"));
    expect(setUrl).toHaveBeenCalledWith(THERE);
    expect(useStore.getState().showConnections).toBe(false);
  });

  it("starts a new connection blank and selected", async () => {
    await open([{ id: "a", name: "here", url: HERE }]);
    fireEvent.click(screen.getByTitle("a connection you will type in"));
    expect(screen.getByLabelText("address for new connection")).toHaveValue("");
    expect(screen.getByText("connect")).toBeDisabled();
  });

  it("says the list is in the file when there is a shell", async () => {
    tauri.on = true;
    file.list = [{ id: "a", name: "here", url: HERE }];
    useStore.setState({ showConnections: true, url: HERE } as never);
    render(<ConnectionsWindow />);
    await waitFor(() => expect(screen.getByText(/connections\.json/)).toBeInTheDocument());
  });

  /** A tab is a real client, and has to admit what it cannot protect. */
  it("warns that a browser tab is not protecting the token", async () => {
    await open([{ id: "a", name: "here", url: HERE }]);
    expect(screen.getByText(/browser tab/i)).toBeInTheDocument();
    expect(screen.getByText(/not protected by the file/i)).toBeInTheDocument();
  });
});
