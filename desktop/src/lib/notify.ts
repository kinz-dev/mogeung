/**
 * Telling you a session needs you when you are not looking. `R-C1`.
 *
 * A port of `crates/mogeungd/src/notify.rs`, and deliberately a faithful one:
 * delivery is the easy half, and the hard half — **not being annoying** — was
 * reasoned out once there and should not be re-derived differently here. The
 * rule is the transition *into* needing you, once, per session; never a state
 * that is merely continuing, or every snapshot would re-announce every waiting
 * session.
 *
 * ## Who speaks
 *
 * Two processes can post the same banner, and one of them must not. `mogeungd`
 * notifies when it is run with `--notify`; a **hosted** daemon is started with
 * it off, on the reasoning written into `src-tauri/src/daemon.rs` — the window
 * is right there. That leaves the case nobody covers: this window hosting a
 * daemon while you are working in another application, which is precisely when
 * a queue nobody can see is worth interrupting for.
 *
 * So the window speaks only when it hosts, and only while it is unfocused. A
 * daemon you attached to is someone else's to announce, and if you want banners
 * from a daemon that outlives the window, `mogeungd --notify` is the thing that
 * gives them to you.
 */

import { useEffect, useRef } from "react";
import type { AttentionItem, AttentionReason, SessionId } from "@/wire/types";
import { needsHuman, sessionLabel } from "@/wire/types";
import { isTauri } from "@/lib/tauri";
import { useStore } from "@/store";

export interface Announcement {
  sessionId: SessionId;
  title: string;
  body: string;
}

/** The same words the daemon uses, so the two cannot describe one state twice. */
function titleFor(reason: AttentionReason): string | null {
  switch (reason) {
    case "awaiting_permission":
      return "Needs your approval";
    case "awaiting_input":
      return "Waiting for you";
    case "failed":
      return "Session failed";
    case "needs_review":
      return "Changes to review";
    case "stalled":
      return "Session has gone quiet";
    default:
      // `rate_limited` counts as needing you but has no line here, exactly as
      // in the Rust: the queue shows it, and there is nothing you can do about
      // a limit by being interrupted.
      return null;
  }
}

/**
 * What to say about this queue, given what has already been said.
 *
 * Pure, like `Notifier::diff`: it returns the announcements *and* the next
 * seen-state and sends nothing, which is what makes the interesting question —
 * when do we speak? — testable with no desktop and no permissions.
 *
 * `next` is rebuilt from the queue rather than merged into, so a session that
 * drops out entirely is forgotten and is announced again if it comes back
 * needing you later.
 */
export function diffQueue(
  announced: ReadonlyMap<SessionId, AttentionReason>,
  queue: readonly AttentionItem[],
  labelOf: (id: SessionId) => string,
): { out: Announcement[]; next: Map<SessionId, AttentionReason> } {
  const out: Announcement[] = [];
  const next = new Map<SessionId, AttentionReason>();

  for (const item of queue) {
    next.set(item.session_id, item.reason);
    if (!needsHuman(item.reason)) continue;
    // Already told you about this exact state.
    if (announced.get(item.session_id) === item.reason) continue;
    const title = titleFor(item.reason);
    if (!title) continue;
    out.push({
      sessionId: item.session_id,
      title,
      body: `${labelOf(item.session_id)} — ${item.detail}`,
    });
  }

  return { out, next };
}

/**
 * Ask the OS, once, and remember the answer for this run.
 *
 * Requested when you turn banners on rather than at startup: a permission
 * prompt on first launch, for a feature that is off by default, is exactly the
 * overstep the daemon's `--notify` flag exists to avoid.
 */
export async function ensurePermission(): Promise<boolean> {
  if (!isTauri()) return false;
  const { isPermissionGranted, requestPermission } = await import(
    "@tauri-apps/plugin-notification"
  );
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

/** Post one banner. Best-effort: a notifier that throws must not break a scan. */
export async function deliver(n: Announcement): Promise<void> {
  if (!isTauri()) return;
  try {
    const { sendNotification } = await import("@tauri-apps/plugin-notification");
    sendNotification({ title: `mogeung — ${n.title}`, body: n.body });
  } catch {
    // A refused or unavailable notification service is ordinary — the same
    // posture `notify.rs` takes with a missing `notify-send`.
  }
}

/**
 * What the bell is actually going to do, in three states rather than two.
 *
 * Asked on 2026-08-07 — *"I tried to enable it, but I didn't see any behaviour
 * changes"* — and the honest answer was that in the usual setup the toggle
 * **cannot** do anything: `start.sh` runs the daemon as its own process, so the
 * window is *attached*, and an attached window deliberately stays silent
 * (see the note at the top of this file). The tooltip said so; the icon did
 * not, and a control that looks switched on while being unable to act is a
 * control that teaches you not to trust it.
 *
 * - `off` — nothing will be posted.
 * - `here` — this window hosts the daemon and will post, while unfocused.
 * - `elsewhere` — banners are on, but the **daemon** is the one that announces.
 *   Which it does when run with `--notify`, as `scripts/start.sh` does by
 *   default. Nothing is broken; this window simply is not the speaker.
 */
export type BellState = "off" | "here" | "elsewhere";

export function bellState(notify: boolean, hosting: boolean): BellState {
  if (!notify) return "off";
  return hosting ? "here" : "elsewhere";
}

/**
 * Watch the queue and announce what changes. Called once, by `App`.
 *
 * The seen-state is advanced **even while silent**, which is the difference
 * between a notifier and a backlog: without it, turning banners on — or coming
 * back to the window after an hour — would fire one for every session that has
 * been waiting all along, and the first thing you would do is turn them off
 * again.
 */
export function useNotifications(): void {
  const queue = useStore((s) => s.queue);
  const sessions = useStore((s) => s.sessions);
  const enabled = useStore((s) => s.prefs.notify);
  const hosting = useStore((s) => s.daemonStatus?.mode === "hosting");
  const announced = useRef(new Map<SessionId, AttentionReason>());

  useEffect(() => {
    const { out, next } = diffQueue(announced.current, queue, (id) => {
      const s = sessions[id];
      return s ? sessionLabel(s) : id.slice(0, 8);
    });
    announced.current = next;

    if (!enabled || !isTauri()) return;
    // Only the hosting window speaks — see the note at the top of this file.
    if (!hosting) return;
    // Not while you are looking at it. The queue is chrome: it is already on
    // screen, already re-ranked, and a banner for what you can see is the
    // noise that teaches people to dismiss banners.
    if (typeof document !== "undefined" && document.hasFocus()) return;

    for (const n of out) void deliver(n);
  }, [queue, sessions, enabled, hosting]);
}
