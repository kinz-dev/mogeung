/**
 * The window's own update, as state: what it runs, where the branch is, and
 * how far through an update it has got. `R-J105`, feature 0049.
 *
 * Its own small store rather than a slice of the main one, because none of it
 * is about sessions or the daemon — it is about the program itself, and it
 * must keep working whichever daemon the window is talking to.
 *
 * The phases run one way: **idle** → **available** (a settled new commit) →
 * **building** → **installing** → **installed** (restart when you like), with
 * **failed** reachable from either middle step. Every step past *available* is
 * a click: nothing is built or installed on its own.
 */

import { create } from "zustand";
import {
  behindText,
  onUpdateEvents,
  settled,
  updateBuild,
  updateCancel,
  updateCheck,
  updateInfo,
  updateInstall,
  updateRestart,
  type Done,
  type UpdateCheck,
  type UpdateInfo,
} from "@/lib/update";
import { isTauri } from "@/lib/tauri";

export type Phase = "idle" | "available" | "building" | "installing" | "installed" | "failed";

/** This window's own off switch, beside `update_check` in the config file. */
const ENABLED_KEY = "mogeung.update.check";
/** Enough of a build's output to see why it failed, and no more. */
const LOG_LINES = 400;

export interface UpdateState {
  info: UpdateInfo | null;
  check: UpdateCheck | null;
  /** When the branch's current head was first seen, for "still for two minutes". */
  seen: { head: string; at: number } | null;
  phase: Phase;
  /** The commit being built, installed, or installed and waiting for a restart. */
  target: string | null;
  log: string[];
  message: string | null;
  /** A head you said *not now* to; offered again only when the branch moves on. */
  dismissed: string | null;
  open: boolean;
  enabled: boolean;
  start: () => Promise<void>;
  poll: (now?: number) => Promise<void>;
  show: () => void;
  notNow: () => void;
  update: () => Promise<void>;
  cancel: () => Promise<void>;
  later: () => void;
  restart: () => Promise<void>;
  setEnabled: (on: boolean) => void;
}

function readEnabled(): boolean {
  try {
    return localStorage.getItem(ENABLED_KEY) !== "0";
  } catch {
    return true;
  }
}

let timer: ReturnType<typeof setInterval> | null = null;
let started = false;

/** Poll on the interval the shell names, only when there is something to poll for. */
function schedule() {
  const { info, enabled, poll } = useUpdate.getState();
  if (timer) clearInterval(timer);
  timer = null;
  if (!info || !info.installed || !info.check || !enabled) return;
  timer = setInterval(() => void poll(), info.interval * 1000);
  void poll();
}

function onDone(d: Done) {
  if (!d.ok) {
    useUpdate.setState({ phase: "failed", message: d.message, open: true });
    return;
  }
  if (d.stage === "build") {
    useUpdate.setState({ phase: "installing" });
    updateInstall().catch((e) => useUpdate.setState({ phase: "failed", message: String(e), open: true }));
    return;
  }
  useUpdate.setState({ phase: "installed", open: true, message: null });
  // The record `install.sh` just wrote says what is installed now.
  updateInfo()
    .then((info) => useUpdate.setState({ info }))
    .catch(() => undefined);
}

export const useUpdate = create<UpdateState>((set, get) => ({
  info: null,
  check: null,
  seen: null,
  phase: "idle",
  target: null,
  log: [],
  message: null,
  dismissed: null,
  open: false,
  enabled: readEnabled(),

  start: async () => {
    if (started || !isTauri()) return;
    started = true;
    await onUpdateEvents(
      (_stage, line) => set((s) => ({ log: [...s.log, line].slice(-LOG_LINES) })),
      onDone,
    );
    try {
      set({ info: await updateInfo() });
    } catch {
      return;
    }
    schedule();
  },

  poll: async (now = Date.now()) => {
    let check: UpdateCheck;
    try {
      check = await updateCheck();
    } catch (e) {
      set({ check: { head: null, head_time: null, commits: [], behind: null, error: String(e) } });
      return;
    }
    const { seen, info, phase, dismissed } = get();
    const nextSeen = check.head && check.head !== seen?.head ? { head: check.head, at: now } : seen;
    set({ check, seen: nextSeen });
    if (!info || phase !== "idle" || !check.head || check.head === dismissed) return;
    if (settled(info.commit, check, nextSeen?.at ?? now, now)) set({ phase: "available", target: check.head, open: true });
  },

  show: () => {
    const { phase, check, info } = get();
    // From the status bar: an update the alert was waiting to settle can be
    // taken now, by hand — the wait is for interrupting you, not for you.
    if (phase === "idle" && info && check?.head && check.head !== info.commit) {
      set({ phase: "available", target: check.head, open: true });
      return;
    }
    set({ open: true });
  },

  notNow: () => {
    const { target, phase } = get();
    if (phase === "available") set({ phase: "idle", dismissed: target, open: false });
    else set({ open: false });
  },

  update: async () => {
    const { target } = get();
    if (!target) return;
    set({ phase: "building", log: [], message: null, open: true });
    try {
      await updateBuild(target);
    } catch (e) {
      set({ phase: "failed", message: String(e) });
    }
  },

  cancel: async () => {
    await updateCancel().catch(() => undefined);
  },

  later: () => set({ open: false }),

  restart: async () => {
    try {
      await updateRestart();
    } catch (e) {
      set({ message: String(e) });
    }
  },

  setEnabled: (on) => {
    try {
      localStorage.setItem(ENABLED_KEY, on ? "1" : "0");
    } catch {
      // Remembered for this run only.
    }
    // Turning it off withdraws an offer on screen; it never stops an update
    // already under way, which you started and can cancel.
    if (!on && get().phase === "available") set({ phase: "idle", open: false });
    set({ enabled: on });
    schedule();
  },
}));

/** For the status bar: what this build is, and how it stands against the branch. */
export function standing(s: Pick<UpdateState, "info" | "check">): string | null {
  if (!s.info) return null;
  return behindText(s.info.commit, s.check, s.info.branch);
}

/** For tests: forget that `start` ran and stop polling. */
export function resetUpdateForTests() {
  if (timer) clearInterval(timer);
  timer = null;
  started = false;
  useUpdate.setState({
    info: null,
    check: null,
    seen: null,
    phase: "idle",
    target: null,
    log: [],
    message: null,
    dismissed: null,
    open: false,
    enabled: true,
  });
}
