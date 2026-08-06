import type { DockviewApi } from "dockview";
import { useStore } from "@/store";
import type { DockTool } from "@/store/prefs";

/**
 * The ids that are dock tools rather than panes.
 *
 * Kept here rather than imported from `BottomDock` on purpose: this module is
 * reached from the keymap and from `explorer.ts`, and pulling a React component
 * tree in behind them is how a cycle starts.
 */
const DOCK_TOOLS: readonly string[] = ["changes", "transcript", "git", "insight", "debt"];

/**
 * Bring a pane forward, adding it back if it was closed. The port of
 * `set_tab`: "show the Changes tab" has to mean *make it visible*, and with a
 * docking tree that includes restoring a pane you closed by accident.
 *
 * Lives here rather than in `App.tsx` so the keymap can call it without the two
 * modules importing each other — a cycle that resolves to `undefined` at module
 * init is exactly the kind of failure that only shows up in the browser.
 */
export function focusPane(api: DockviewApi | null, id: string, title: string): void {
  if (!api) return;
  const existing = api.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  api.addPanel({ id, component: id, title });
}

let dock: DockviewApi | null = null;

/** Set once, by `App` when dockview is ready. */
export function setDock(api: DockviewApi): void {
  dock = api;
}

/**
 * Raise a pane from outside the React tree that owns it — the top bar, the
 * palette, a notification. The alternative is threading a ref through every
 * component that might ever want to, which is how a prop ends up in twelve
 * places to be used in two.
 */
export function showPane(id: string, title: string): void {
  // Some surfaces are dock tools rather than panes, and which is which has
  // changed twice now — Git, Insight and Debt moved out of the centre, then
  // Changes and Transcript followed on 2026-08-06. Callers say *what to show*
  // and this decides where it lives, so a bookmark or a search hit does not
  // have to know: before this, `showPane("transcript", …)` on a moved pane
  // silently added a tab for a component that no longer exists.
  if (DOCK_TOOLS.includes(id as DockTool)) {
    const { prefs, setPrefs } = useStore.getState();
    if (prefs.dock !== id) setPrefs({ dock: id as DockTool });
    return;
  }
  focusPane(dock, id, title);
}

/**
 * Go to a marked turn: its session, the Transcript, that turn.
 *
 * The raise is the part that was missing. A bookmark clicked from the rail set
 * the session and the turn and stopped there, so with the Agent or Git tab
 * forward the click did nothing you could see — and a row that answers a click
 * with no visible change reads as broken, not as "the destination is behind
 * another tab".
 *
 * Order matters twice. `select` clears the focus fields when the session
 * changes, so it has to come first or the jump it is meant to set up is wiped
 * on the way. And the pane is raised **before** the seq is published, so the
 * Transcript's scroll effect runs against a pane that is on screen: a
 * virtualised list cannot scroll to an index it is not currently laying out.
 */
export function jumpToTurn(sessionId: string, seq: number | null): void {
  useStore.getState().select(sessionId);
  showPane("transcript", "Transcript");
  // By **seq**, not timestamp: a session this window has never opened has no
  // loaded events to take a timestamp from, and a seq stays pending until the
  // events arrive rather than silently missing.
  useStore.setState({ focusSeq: seq, highlightSeq: seq });
}
