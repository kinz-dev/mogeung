/**
 * The arcade: the one thing in this window nobody is told about.
 *
 * It opens when you type the first six digits of π — `314159` — with nothing
 * that takes text focused, and it is in neither the shortcuts window nor the
 * palette (see [`lib/mentalMath.ts`](../lib/mentalMath.ts) for why it is not
 * an `Action`). Since 2026-09-24 the digits open a **picker** rather than a
 * game, because there are more than one: the mental-math drill it started as,
 * and — the same day — all six of tradermath.org's market games: the Trading
 * Game, the Market Making Dice Game, the ETF Arbitrage Game, the Card Game,
 * the Fermi Estimation Game and the Event Contracts Game. Asked for in those
 * words: *"the code 314159 will now popup a game screen, where user can select
 * Mental maths or Trading Game to start and follow the pages"* — and then
 * *"please port all of the 3 remaining game over"*.
 *
 * Since the same day it opens in **a window of its own** (see
 * [`lib/arcadeWindow.ts`](../lib/arcadeWindow.ts)): the main window keeps only
 * the way in — π — and the bridge that tells the arcade's page the theme and
 * which session is waiting. `ArcadeWindow` is that page. Where no window can
 * be opened, the room opens over the main window as it used to, rather than π
 * doing nothing.
 *
 * The arcade owns the room — the whole surface, focus in and back out, and the
 * one line that keeps an eye on your sessions. Each game owns its own pages
 * inside it, and Escape walks back through them: a game's first page returns
 * here, and Escape here closes.
 */

import { useEffect, useRef, useState } from "react";
import { channel, openArcadeWindow, raiseThisWindow, type ArcadeMsg, type Waiting } from "@/lib/arcadeWindow";
import { focusOwns } from "@/lib/keymap";
import { SECRET, SECRET_GAP_MS, secretStep } from "@/lib/mentalMath";
import { useStore, type AppState } from "@/store";
import { needsHuman, sessionLabel, type SessionId } from "@/wire/types";
import { Room } from "@/ui/ArcadeRoom";

/**
 * The main window's half: π opens the arcade's window, and the bridge keeps
 * that window told. Renders nothing — unless no window could be opened.
 */
export function Arcade({ seed }: { seed?: number } = {}) {
  const [inline, setInline] = useState(false);
  useSecret(!inline, () => {
    if (!openArcadeWindow()) setInline(true);
  });
  useBridge();
  const waiting = useStore(waitingOf);
  if (!inline) return null;
  const goTo = (id: SessionId) => {
    useStore.getState().select(id);
    setInline(false);
  };
  return <Room onClose={() => setInline(false)} seed={seed} waiting={waiting} onGo={goTo} overlay />;
}

let lastWaiting: { queue: unknown; sessions: unknown; value: Waiting } | null = null;

/** The waiting line's facts, from the store. Memoised so a selector is stable. */
function waitingOf(s: Pick<AppState, "queue" | "sessions">): Waiting {
  if (lastWaiting && lastWaiting.queue === s.queue && lastWaiting.sessions === s.sessions) return lastWaiting.value;
  const waiting = s.queue.filter((i) => needsHuman(i.reason));
  const first = waiting[0];
  const session = first && s.sessions[first.session_id];
  const value: Waiting = {
    count: waiting.length,
    first: first ? { id: first.session_id, label: session ? sessionLabel(session) : "" } : null,
  };
  lastWaiting = { queue: s.queue, sessions: s.sessions, value };
  return value;
}

/**
 * The main window's end of the channel: answer an arcade that says hello,
 * tell it whenever the waiting line or the theme changes, and take a click on
 * that line as "show me this session".
 */
function useBridge() {
  useEffect(() => {
    const c = channel();
    if (!c) return;
    let said = "";
    const send = (force = false) => {
      const msg: ArcadeMsg = {
        t: "state",
        theme: document.documentElement.getAttribute("data-theme"),
        waiting: waitingOf(useStore.getState()),
      };
      const text = JSON.stringify(msg);
      if (!force && text === said) return;
      said = text;
      c.postMessage(msg);
    };
    c.onmessage = (e: MessageEvent<ArcadeMsg>) => {
      const m = e.data;
      if (m.t === "hello") send(true);
      else if (m.t === "go") {
        useStore.getState().select(m.id);
        void raiseThisWindow();
      }
    };
    const unsubscribe = useStore.subscribe((s, prev) => {
      if (s.queue !== prev.queue || s.sessions !== prev.sessions) send();
    });
    const themes = new MutationObserver(() => send());
    themes.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      unsubscribe();
      themes.disconnect();
      c.close();
    };
  }, []);
}

/**
 * Listens for π. Only where a bare key belongs to the window — the same rule
 * the window's own bare bindings obey — so the digits typed into a terminal,
 * a filter box or a scratch file are only ever digits.
 */
function useSecret(armed: boolean, onFound: () => void) {
  const found = useRef(onFound);
  found.current = onFound;
  useEffect(() => {
    if (!armed) return;
    let progress = "";
    let at = 0;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat) return;
      const digit = /^\d$/.test(e.key) && !e.ctrlKey && !e.altKey && !e.metaKey;
      if (!digit || focusOwns(e.key)) {
        progress = "";
        return;
      }
      const now = Date.now();
      progress = secretStep(now - at > SECRET_GAP_MS ? "" : progress, e.key);
      at = now;
      if (progress === SECRET) {
        progress = "";
        e.preventDefault();
        found.current();
      }
    };
    // Capture, like the keymap: a pane that stops propagation must not be
    // able to hide the sequence from this.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [armed]);
}
