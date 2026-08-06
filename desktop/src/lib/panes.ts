import type { DockviewApi } from "dockview";
import { useStore } from "@/store";
import { paneKind } from "@/lib/paneScope";
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
  // `component` is the *kind*, `id` is the identity, and since `R-B49` they are
  // no longer the same string: `agent:2` renders with the `agent` component.
  api.addPanel({ id, component: paneKind(id), title });
}

/**
 * The next free Agent slot, or `null` when the ceiling is reached. `R-B49`.
 *
 * **Numbered, never keyed by session.** The layout is persisted, and a panel id
 * naming a session restores a tab pointing at something that ended three days
 * ago — a dead tab you have to identify before you can close it. A number plus
 * a hold keeps the *arrangement* durable while leaving the *binding* free to be
 * dropped, which is the whole reason the two are separate ideas.
 *
 * The ceiling is not arithmetic shyness. Every visible Agent pane is a live
 * `tmux attach` and a pty, and unlike a hidden tab it cannot be free — so the
 * limit is stated here rather than discovered on the day someone splits nine
 * times and wonders why the fans came on.
 */
export const MAX_AGENT_PANES = 4;

export function nextAgentSlot(api: DockviewApi | null): string | null {
  if (!api) return null;
  // Slot 1 is the bare `agent`, because it predates numbering and a layout
  // saved before this feature must keep working untouched.
  for (let n = 1; n <= MAX_AGENT_PANES; n++) {
    const id = n === 1 ? "agent" : `agent:${n}`;
    // Gaps are filled rather than skipped: close the middle pane of three and
    // the next split reuses that number instead of climbing to the ceiling.
    if (!api.getPanel(id)) return id;
  }
  return null;
}

let dock: DockviewApi | null = null;

/** Set once, by `App` when dockview is ready. */
export function setDock(api: DockviewApi): void {
  dock = api;
}

/** The tree itself, for callers that need to read it rather than command it. */
export function getDock(): DockviewApi | null {
  return dock;
}

/**
 * Put another Agent pane beside this one.
 *
 * Beside, not on top of: the ask was two agents *side by side*, and a split
 * that opened as a second tab in the same group would answer it by hiding one
 * of them. `direction: "right"` against the active group is what makes the
 * default gesture produce the arrangement the feature exists for.
 */
export function splitAgent(): void {
  if (!dock) return;
  const id = nextAgentSlot(dock);
  if (!id) return;
  const active = dock.activeGroup;
  dock.addPanel({
    id,
    component: "agent",
    title: "Agent",
    position: active ? { referenceGroup: active, direction: "right" } : undefined,
  });
}

/**
 * Send an Agent pane away, and let go of whatever it was holding.
 *
 * The hold has to go with it or the entry outlives the pane that could clear
 * it: split again into the same slot number and the new pane would arrive
 * already held on a session you chose last week, which looks like the split
 * ignoring your selection.
 *
 * The base `agent` pane is never closed — every other pane in the centre is
 * permanent (`PaneTab` has no close button for exactly this reason), and a
 * window with no Agent pane at all is one you can only recover from with a
 * shortcut you have to remember.
 */
export function closeAgentPane(id: string): void {
  if (!dock || id === "agent") return;
  dropHold(id);
  dock.getPanel(id)?.api.close();
}

/** Let go of one pane's session, if it was holding one. */
function dropHold(id: string): void {
  const { scoped, setScoped } = useStore.getState();
  if (!scoped().paneHold[id]) return;
  const next = { ...scoped().paneHold };
  delete next[id];
  setScoped({ paneHold: next });
}

/** Every hold whose pane is not in this layout, dropped in one write. */
export function dropOrphanHolds(api: DockviewApi | null): void {
  if (!api) return;
  const { scoped, setScoped } = useStore.getState();
  const hold = scoped().paneHold;
  const next: Record<string, string> = {};
  let changed = false;
  for (const [paneId, sessionId] of Object.entries(hold)) {
    if (api.getPanel(paneId)) next[paneId] = sessionId;
    else changed = true;
  }
  if (changed) setScoped({ paneHold: next });
}

/**
 * The way back. `R-B49` — and it is the feature's safety valve rather than a
 * convenience.
 *
 * The egui client shipped this with the tile tree ([feature 0006](../../../docs/features/0006-dockable-panes.md))
 * and the port dropped it, which was survivable only while the centre held one
 * pane that could not be split. Now that a gesture can put four Agent panes on
 * screen, an arrangement you cannot undo by hand is reachable again, and the
 * reason the original existed comes back with it.
 *
 * Holds go too. A layout reset that left three panes moored to sessions you
 * cannot see would be a reset that did not reset.
 */
export function resetLayout(): void {
  if (!dock) return;
  useStore.getState().setScoped({ paneHold: {} });
  dock.clear();
  dock.addPanel({ id: "agent", component: "agent", title: "Agent" });
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
