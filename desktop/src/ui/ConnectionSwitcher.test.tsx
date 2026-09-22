/**
 * The top bar's switcher. `R-I20`.
 *
 * What each test would catch failing quietly:
 *
 * - a row whose dot says *up* for a daemon that is down — the one thing the
 *   switcher was asked for;
 * - a choice that dials the address without its token, which is `R-J102`'s
 *   refused-by-your-own-daemon one layer up;
 * - a browser tab showing every daemon as *checking* for ever.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ConnectionSwitcher } from "@/ui/ConnectionSwitcher";
import { useStore } from "@/store";

const tauri = vi.hoisted(() => ({ on: true }));
vi.mock("@/lib/tauri", async (orig) => ({
  ...(await orig<typeof import("@/lib/tauri")>()),
  isTauri: () => tauri.on,
}));

const shell = vi.hoisted(() => ({
  list: [] as unknown[],
  status: [] as unknown[],
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "connections_load") return shell.list;
    if (cmd === "connections_status") return shell.status;
    if (cmd === "lan_peers") return [];
    throw new Error(`unexpected command ${cmd}`);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const HERE = "ws://127.0.0.1:7717/ws";

const SAVED = [
  { id: "l", name: "this laptop", url: HERE },
  { id: "m", name: "mac-m4p", url: "ws://10.0.0.39:7717/ws", token: "theirs" },
  { id: "k", name: "mac-m1p", url: "ws://10.0.0.22:7717/ws", token: "old" },
  { id: "s", name: "spark", url: "ws://10.0.0.183:7717/ws", token: "x" },
];
const STATUS = [
  { id: "l", state: "up", host: "kinz-xps14", version: "0.1.0", detail: null },
  { id: "m", state: "up", host: "Kinzs-Mac-mini.local", version: "0.1.0", detail: null },
  { id: "k", state: "refused", host: null, version: null, detail: "answering, and the token was refused" },
  { id: "s", state: "down", host: null, version: null, detail: "nothing answered" },
];

async function openMenu() {
  render(<ConnectionSwitcher />);
  const trigger = await screen.findByRole("button", { name: "switch daemon" });
  await waitFor(() => expect(trigger).toHaveTextContent("this laptop"));
  fireEvent.keyDown(trigger, { key: "Enter" });
}

beforeEach(() => {
  tauri.on = true;
  shell.list = SAVED;
  shell.status = STATUS;
  useStore.setState({ url: HERE, conn: "open" } as never);
});
afterEach(() => cleanup());

describe("switching daemons from the top bar", () => {
  it("names the daemon being watched on the trigger", async () => {
    render(<ConnectionSwitcher />);
    await waitFor(() => expect(screen.getByRole("button", { name: "switch daemon" })).toHaveTextContent("this laptop"));
  });

  it("says, per saved daemon, whether it is up, refusing the token, or down", async () => {
    await openMenu();
    await waitFor(() => expect(document.querySelector('[data-connection="mac-m4p"]')).toHaveAttribute("data-liveness", "up"));
    expect(document.querySelector('[data-connection="mac-m1p"]')).toHaveAttribute("data-liveness", "refused");
    expect(document.querySelector('[data-connection="spark"]')).toHaveAttribute("data-liveness", "down");
    expect(screen.getByText(/up — Kinzs-Mac-mini\.local · 0\.1\.0/)).toBeInTheDocument();
    expect(screen.getByText(/the token was refused/)).toBeInTheDocument();
    expect(screen.getByText(/down — nothing answered/)).toBeInTheDocument();
    expect(screen.getByText("watching")).toBeInTheDocument();
  });

  it("switches with the saved token composed in", async () => {
    const setUrl = vi.fn();
    useStore.setState({ setUrl } as never);
    await openMenu();
    fireEvent.click(await screen.findByText("mac-m4p"));
    expect(setUrl).toHaveBeenCalledWith("ws://10.0.0.39:7717/ws?token=theirs");
  });

  it("does nothing when the daemon already being watched is chosen", async () => {
    const setUrl = vi.fn();
    useStore.setState({ setUrl } as never);
    await openMenu();
    await screen.findByText("mac-m4p");
    fireEvent.click(document.querySelector('[data-connection="this laptop"]')!);
    expect(setUrl).not.toHaveBeenCalled();
  });

  it("opens the Connections panel from its last row", async () => {
    useStore.setState({ showConnections: false } as never);
    await openMenu();
    fireEvent.click(await screen.findByText(/manage connections/));
    expect(useStore.getState().showConnections).toBe(true);
  });

  it("says a browser tab cannot check, rather than checking for ever", async () => {
    tauri.on = false;
    localStorage.setItem("mogeung.connections", JSON.stringify(SAVED));
    await openMenu();
    expect((await screen.findAllByText(/only the desktop window can check/)).length).toBe(SAVED.length);
    localStorage.removeItem("mogeung.connections");
  });
});
