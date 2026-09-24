/**
 * The window's own update, from the offer to the restart, against a shell that
 * is faked but keeps the real contract: commands in, `update:log` and
 * `update:done` out. `R-J105`, feature 0049.
 *
 * What these pin is the ask's shape and the two places the plan changed it:
 * an offer, a confirmation, an install, and *Restart now* or *Later* — with
 * the offer coming from a commit rather than a timer, and nothing about a
 * password anywhere in the window.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { markUnsaved } from "@/lib/unsaved";
import { useStore } from "@/store";
import { resetUpdateForTests, useUpdate } from "@/store/update";
import { UpdateAlert, UpdateMark } from "@/ui/UpdateAlert";

const RUNNING = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

const shell = vi.hoisted(() => ({
  info: null as unknown,
  check: null as unknown,
  calls: [] as { cmd: string; args?: unknown }[],
  listeners: new Map<string, (e: { payload: unknown }) => void>(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: unknown) => {
    shell.calls.push({ cmd, args });
    if (cmd === "update_info") return shell.info;
    if (cmd === "update_check") return shell.check;
    return undefined;
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    shell.listeners.set(name, cb);
    return () => shell.listeners.delete(name);
  }),
}));

const emit = (name: string, payload: unknown) => act(() => shell.listeners.get(name)?.({ payload }));
const called = (cmd: string) => shell.calls.filter((c) => c.cmd === cmd);
const flush = () => act(async () => {});

function installed(overrides: Record<string, unknown> = {}) {
  return {
    commit: RUNNING,
    installed: true,
    record: null,
    check: true,
    branch: "main",
    interval: 60,
    source: "/home/k/mogeung",
    platform: "linux",
    ...overrides,
  };
}

function ahead(headTime = Date.now() / 1000 - 3600) {
  return {
    head: HEAD,
    head_time: headTime,
    behind: 2,
    commits: [
      { sha: HEAD, subject: "feat: the second thing" },
      { sha: "3333333ccccccccccccccccccccccccccccccccc", subject: "fix: the first thing" },
    ],
    error: null,
  };
}

async function mount() {
  render(
    <>
      <UpdateAlert />
      <UpdateMark />
    </>,
  );
  await flush();
  await flush();
}

beforeEach(() => {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  shell.calls = [];
  shell.listeners.clear();
  shell.info = installed();
  shell.check = ahead();
  localStorage.clear();
  resetUpdateForTests();
  useStore.setState({ daemonStatus: null } as never);
});

afterEach(() => {
  cleanup();
  resetUpdateForTests();
  markUnsaved("notes", null);
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("the offer", () => {
  it("a settled commit on main is offered, with what changed — and nothing is built until you say so", async () => {
    await mount();
    expect(screen.getByRole("dialog", { name: "Update available" })).toBeInTheDocument();
    const changed = screen.getByRole("list", { name: "what changed" });
    expect(within(changed).getByText(/the second thing/)).toBeInTheDocument();
    expect(within(changed).getByText(/the first thing/)).toBeInTheDocument();
    expect(screen.getByText(/system's own dialog, for dpkg — mogeung never sees it/)).toBeInTheDocument();
    expect(called("update_build")).toHaveLength(0);
    // And the status bar says how far behind.
    expect(screen.getByLabelText("this build")).toHaveTextContent("1111111 · 2 behind");
  });

  it("a commit still settling is shown in the status bar but not offered yet", async () => {
    shell.check = ahead(Date.now() / 1000 - 5);
    await mount();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByLabelText("this build")).toHaveTextContent("2 behind");
    // You can take it by hand.
    fireEvent.click(screen.getByLabelText("this build"));
    expect(screen.getByRole("dialog", { name: "Update available" })).toBeInTheDocument();
  });

  it("Not now is not asked again for the same commit, but is for the next one", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await act(() => useUpdate.getState().poll());
    expect(screen.queryByRole("dialog")).toBeNull();
    shell.check = { ...ahead(), head: "4444444ddddddddddddddddddddddddddddddddd" };
    await act(() => useUpdate.getState().poll());
    expect(screen.getByRole("dialog", { name: "Update available" })).toBeInTheDocument();
  });

  it("a build run from a checkout is never offered one, and does not even look", async () => {
    shell.info = installed({ installed: false });
    await mount();
    expect(called("update_check")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByLabelText("this build")).toHaveTextContent("dev 1111111");
  });

  it("looks once a minute, and stop checking stops the looking; the status bar starts it again", async () => {
    vi.useFakeTimers();
    try {
      await mount();
      const first = called("update_check").length;
      await act(async () => vi.advanceTimersByTime(60_000));
      expect(called("update_check")).toHaveLength(first + 1);

      fireEvent.click(screen.getByRole("button", { name: "stop checking" }));
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(localStorage.getItem("mogeung.update.check")).toBe("0");
      const stopped = called("update_check").length;
      await act(async () => vi.advanceTimersByTime(5 * 60_000));
      expect(called("update_check")).toHaveLength(stopped);

      fireEvent.click(screen.getByLabelText("this build"));
      await flush();
      expect(useUpdate.getState().enabled).toBe(true);
      expect(called("update_check").length).toBeGreaterThan(stopped);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("the update", () => {
  it("Update builds that commit, installs when it is built, and asks Restart now or Later", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    await flush();
    expect(called("update_build")[0].args).toEqual({ commit: HEAD });
    expect(screen.getByRole("dialog", { name: /Building 2222222/ })).toBeInTheDocument();

    emit("update:log", { stage: "build", line: "▸ building the daemon (release)…" });
    expect(screen.getByLabelText("update log")).toHaveTextContent("building the daemon");
    expect(called("update_install")).toHaveLength(0);

    emit("update:done", { stage: "build", ok: true, code: 0, cancelled: false, message: null });
    await flush();
    expect(called("update_install")).toHaveLength(1);
    expect(screen.getByRole("dialog", { name: "Installing" })).toBeInTheDocument();
    expect(screen.getByText(/Enter your password in the system's dialog/)).toBeInTheDocument();
    // Nowhere in the window is a password asked for.
    expect(document.querySelector("input[type=password]")).toBeNull();

    emit("update:done", { stage: "install", ok: true, code: 0, cancelled: false, message: null });
    await flush();
    expect(screen.getByRole("dialog", { name: "Update installed" })).toBeInTheDocument();
    expect(screen.getByText(/Agents keep running/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Restart now" }));
    await flush();
    expect(called("update_restart")).toHaveLength(1);
  });

  it("Later leaves a restart-to-update mark, which brings the question back", async () => {
    await mount();
    useUpdate.setState({ phase: "installed", target: HEAD, open: true });
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Later" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    fireEvent.click(screen.getByText("restart to update"));
    expect(screen.getByRole("dialog", { name: "Update installed" })).toBeInTheDocument();
    expect(called("update_restart")).toHaveLength(0);
  });

  it("a dismissed password dialog says so in words, and Try again starts over", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    await flush();
    emit("update:done", { stage: "build", ok: true, code: 0, cancelled: false, message: null });
    await flush();
    emit("update:done", {
      stage: "install",
      ok: false,
      code: 3,
      cancelled: false,
      message: "The password dialog was dismissed — nothing was installed.",
    });
    expect(screen.getByRole("dialog", { name: "The update did not finish" })).toBeInTheDocument();
    expect(screen.getByText(/dismissed — nothing was installed/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await flush();
    expect(called("update_build")).toHaveLength(2);
  });

  it("Cancel asks the shell to stop the build", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await flush();
    expect(called("update_cancel")).toHaveLength(1);
  });
});

describe("before a restart", () => {
  it("names a note with unsaved changes, and a daemon this window did not start", async () => {
    useStore.setState({ daemonStatus: { mode: "attached", pid: 4321, claude_home: null, token: null } } as never);
    markUnsaved("notes", "The note “Plans”");
    await mount();
    useUpdate.setState({ phase: "installed", target: HEAD, open: true });
    await flush();
    expect(screen.getByText(/The note “Plans” has unsaved changes/)).toBeInTheDocument();
    expect(screen.getByText(/daemon on this machine \(pid 4321\) was started separately/)).toBeInTheDocument();
  });
});
