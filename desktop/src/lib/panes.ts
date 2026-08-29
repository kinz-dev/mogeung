import type { DockviewApi } from "dockview";
import { useStore } from "@/store";
import { paneKind } from "@/lib/paneScope";
import { pickNeighbour, type Box, type Direction } from "@/lib/spatial";
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
 * The next free Agent slot. `R-B49`.
 *
 * **Numbered, never keyed by session.** The layout is persisted, and a panel id
 * naming a session restores a tab pointing at something that ended three days
 * ago — a dead tab you have to identify before you can close it. A number plus
 * a hold keeps the *arrangement* durable while leaving the *binding* free to be
 * dropped, which is the whole reason the two are separate ideas.
 *
 * **There is no ceiling since 2026-08-20** (`R-J35`). There was one, at four,
 * and the reason it gave is still true — every *visible* Agent pane is a live
 * `tmux attach` and a pty, and unlike a hidden tab it cannot be free. What was
 * wrong was who got to weigh that: a number picked here applies the same limit
 * to a laptop and to a 49-inch screen, and it refused a fifth pane on a machine
 * that could plainly carry one. The cost is now the user's to see and to
 * arrange around, which is the same trade the whole docking tree already makes.
 *
 * `null` only where every function here answers `null` — before dockview is
 * ready. The loop terminates on the first free number, and the tree holds
 * finitely many panels, so it always finds one.
 */
export function nextAgentSlot(api: DockviewApi | null): string | null {
  if (!api) return null;
  // Slot 1 is the bare `agent`, because it predates numbering and a layout
  // saved before this feature must keep working untouched.
  for (let n = 1; ; n++) {
    const id = n === 1 ? "agent" : `agent:${n}`;
    // Gaps are filled rather than skipped: close the middle pane of three and
    // the next pane reuses that number instead of climbing past it.
    if (!api.getPanel(id)) return id;
  }
}

/**
 * The Agent panes currently in the tree, in slot order.
 *
 * Read off the tree rather than counted up to a limit — there is no limit to
 * count to. Sorted numerically and not by string, or `agent:10` would file
 * itself between `agent` and `agent:2`.
 */
export function agentSlots(api: DockviewApi | null): string[] {
  if (!api) return [];
  return (api.panels ?? [])
    .map((p) => p.id)
    .filter((id) => paneKind(id) === "agent")
    .sort((a, b) => slotNumber(a) - slotNumber(b));
}

function slotNumber(id: string): number {
  const cut = id.indexOf(":");
  return cut === -1 ? 1 : Number(id.slice(cut + 1)) || 1;
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
 * One open file, one pane. `R-B53`.
 *
 * The window ran **two** tab systems until 2026-08-07: dockview's, and the Code
 * pane's own file strip with its own two-way split. Collapsing them into one
 * means a file can sit beside an agent, above it, or in its own group — the
 * arrangement `R-B49` gave agents, applied to files — and it deletes the strip,
 * the `group`/`active`/`focus` machinery, and the hidden-header workaround that
 * existed only because the Code pane had three rows of naming.
 *
 * **A file pane is bound to the session that opened it**, chosen deliberately
 * on 2026-08-07 over "close and reopen on every selection change". A pane that
 * emptied itself when you clicked another session would make reading one repo
 * while an agent works in another impossible, which is the arrangement two
 * agents at once exists to support. The cost is that panes accumulate and you
 * close them yourself — which is why, unlike every other pane in the centre,
 * these have a close button.
 *
 * The id carries session, revision and path because the pane needs all three to
 * render and dockview gives it nothing else. That makes the id **unfit for the
 * saved layout**, which is handled where layouts are loaded: `file:` panels are
 * stripped on restore. Nothing is lost — `explorer` is not persisted either, so
 * a restart has no open files to restore in the first place.
 */
const FILE_PREFIX = "file:";

export function filePaneId(session: string, path: string, rev: string | null): string {
  return `${FILE_PREFIX}${session}:${rev ?? ""}:${path}`;
}

/** The three fields back out of an id, or `null` if it is not a file pane. */
export function parseFilePaneId(id: string): { session: string; path: string; rev: string | null } | null {
  if (!id.startsWith(FILE_PREFIX)) return null;
  const rest = id.slice(FILE_PREFIX.length);
  const a = rest.indexOf(":");
  if (a < 0) return null;
  const b = rest.indexOf(":", a + 1);
  if (b < 0) return null;
  // The path is whatever is left, colons and all — split from the left exactly
  // twice, because a session id and a sha cannot contain one and a path can.
  return { session: rest.slice(0, a), rev: rest.slice(a + 1, b) || null, path: rest.slice(b + 1) };
}

export function showFilePane(session: string, path: string, rev: string | null): void {
  if (!dock) return;
  const id = filePaneId(session, path, rev);
  const existing = dock.getPanel(id);
  if (existing) {
    existing.api.setActive();
    return;
  }
  // Beside the active group rather than inside it only when the active group is
  // an **agent**: a second file joins the files, which is the arrangement
  // anyone opening two of them expects.
  const active = dock.activeGroup;
  const activeIsFile = active?.activePanel && parseFilePaneId(active.activePanel.id) !== null;
  dock.addPanel({
    id,
    component: "file",
    title: path,
    ...(active && !activeIsFile ? { position: { referenceGroup: active, direction: "right" as const } } : {}),
  });
}

export function closeFilePane(session: string, path: string, rev: string | null): void {
  dock?.getPanel(filePaneId(session, path, rev))?.api.close();
}

/** Every file pane currently open, for callers reconciling with the store. */
export function filePanes(api: DockviewApi | null): string[] {
  return (api?.panels ?? []).map((p) => p.id).filter((id) => id.startsWith(FILE_PREFIX));
}

/**
 * The tabs sitting in the same group as this one, in tab order, itself
 * included. `R-J47`.
 *
 * **Ids, not panels, and read in one go.** Closing a tab mutates the group's
 * own array, so a caller that iterated the live list would skip every second
 * pane — the classic remove-while-iterating bug, and here it would look like
 * "close all" leaving half the tabs behind rather than like a crash.
 *
 * The **group** and not the whole tree, which is the line every editor draws:
 * two Agent panes split side by side are two groups, and a gesture on one of
 * them must not reach across the sash into the arrangement you deliberately
 * built beside it.
 */
export function groupPanes(id: string): string[] {
  const panel = dock?.getPanel(id);
  if (!panel) return [];
  // `group` is non-optional in dockview's types and absent in the hand-built
  // panels the tests use; a pane with no group is a group of one.
  return panel.group?.panels?.map((p) => p.id) ?? [id];
}

/**
 * Another Agent pane, **as a tab in the group you are in** and anchored on the
 * session it opens with. `R-J35`, `R-J44`.
 *
 * **A tab, not a split, since 2026-08-24** — asked for directly, and it
 * reverses what this function was built to do. `R-J35` opened beside, because
 * the ask then was two agents *side by side* and a second tab would answer it
 * by hiding one of them. That reasoning was right and is no longer the common
 * case: with sessions started from the window rather than from a terminal
 * (`R-J45`), panes are opened far more often than they are compared, and every
 * `direction: "right"` narrowed the two agents already on screen to make room
 * for a third nobody had asked to see yet.
 *
 * Side by side is not gone, it has moved to where it is chosen: dockview drags
 * a tab out to any edge, which is the same gesture the file panes have always
 * used. The trade is one drag when you want the split, against no drag — and
 * no re-widening — when you do not.
 *
 * Omitting `direction` is what says *within*: dockview reads
 * `position.direction || "within"`, so `{ referenceGroup }` alone is the tab.
 * With no active group there is no `position` at all and the panel lands
 * wherever dockview puts a first one, which is unchanged.
 *
 * **The anchor is the default since 2026-08-20**, asked for directly. A new
 * pane used to arrive unheld, which meant two panes both following the queue:
 * the next click moved *both* of them and the arrangement you had just built
 * collapsed back into two views of one session. So the second pane was only
 * ever one manual click away from being useful, and everybody made that click
 * every time.
 *
 * **The pane you opened *from* is untouched**, and that is what keeps the queue
 * working. A window whose every pane was moored would answer a queue click by
 * opening another (see `revealSession`), so the one pane that came back with
 * the layout stays free to follow the selection unless you moor it yourself.
 *
 * Nothing is written when there is no selection: an anchor needs a session, and
 * holding a pane on "nothing" is not a state — it is the absence of a hold.
 */
export function addAgentPane(): string | null {
  if (!dock) return null;
  const id = nextAgentSlot(dock);
  if (!id) return null;
  const active = dock.activeGroup;
  dock.addPanel({
    id,
    component: "agent",
    title: "Agent",
    position: active ? { referenceGroup: active } : undefined,
  });
  const { selected, scoped, setScoped } = useStore.getState();
  if (selected) setScoped({ paneHold: { ...scoped().paneHold, [id]: selected } });
  return id;
}

/**
 * Put a session on screen, wherever it belongs. `R-J31`.
 *
 * The queue used to call `select` directly, which is only half an answer: a
 * selection is what the *unheld* panes follow, and once every pane on screen is
 * held there is nothing left to follow it. Clicking a queue row then changed
 * the dock, the rail and the status bar while the centre — the part you were
 * looking at — stayed exactly as it was. Reported 2026-08-09 with one pane, the
 * worst version of it: the app appeared to ignore the click entirely.
 *
 * Three cases, in order, and the order is the design:
 *
 * 1. **A pane is already held on this session.** Raise it. This is the case
 *    that must come first — a session you have deliberately moored somewhere is
 *    a session with a home, and a second pane for it would be two views of one
 *    agent, which is what `R-B49` exists *not* to make.
 * 2. **Some pane is unheld.** It follows the selection, so selecting is the
 *    whole job. Opening another here would grow the layout on every click,
 *    which is how you end up with a strip of tabs by lunchtime.
 3. **Every pane is held and none of them holds this one.** Open another —
 *    a tab beside the one you are on since `R-J44`, not a split. It arrives
 *    held on this session, because `select` has already run and an anchor is
 *    what a new pane comes with since `R-J35` — which is also what it wanted
 *    before, by a longer route: it used to arrive unheld and follow the
 *    selection into place.
 *
 * Case 3 no longer has a ceiling to hit (`R-J35`), so the only way it answers
 * nothing is a window with no dockview — which is case 0, above.
 */
export function revealSession(sessionId: string): void {
  const api = dock;
  const { select, scoped, pushNotice } = useStore.getState();
  if (!api) {
    select(sessionId);
    return;
  }

  const slots = agentSlots(api);
  const hold = scoped().paneHold;

  const holder = slots.find((id) => hold[id] === sessionId);
  if (holder) {
    // `select` first: activating the pane fires the handler in `App.tsx` that
    // writes the hold back to the selection, and doing our own write after that
    // one would be the same value twice.
    select(sessionId);
    api.getPanel(holder)?.api.setActive();
    return;
  }

  select(sessionId);
  const free = slots.filter((id) => !hold[id]);
  if (free.length > 0) {
    // **Raise it, too.** `R-J50`, asked 2026-08-25: *"even all tab is opened,
    // when someone click on the ATTENTION's session, bring the selected panel
    // up"*. This case answered the click by selecting and stopping there, on
    // the reasoning that an unheld pane follows the selection and therefore
    // needs no help — true of what the pane *shows*, and silent about whether
    // you can see it. With a file tab forward in the same group, the pane that
    // just changed session is behind it, so the click moved the dock, the rail
    // and the status bar and left the centre showing a file. That is `R-J31`'s
    // complaint again, one layer down, and it got commoner with every tab
    // `R-B53` and `R-J44` made it easy to open.
    raiseTab(api, free);
    return;
  }

  if (!addAgentPane()) {
    pushNotice("there is nowhere to put this session — the window has no panes yet");
  }
}

/**
 * Bring one of these tabs forward, unless one of them already is. `R-J50`.
 *
 * **Unless**, because raising a tab is not free: `setActive` activates the
 * group as well, which moves the keyboard, and a queue click that yanked focus
 * out of the file you were reading — to show you a pane that was already in
 * front of you — would be a worse answer than doing nothing. So a pane that is
 * the forward tab of its own group is taken as *shown*, split off in its own
 * group or not, and the click is already answered.
 *
 * A panel with no group is left alone for the same reason: not knowing whether
 * it is visible is not a reason to steal focus.
 */
function raiseTab(api: DockviewApi, ids: readonly string[]): void {
  const forward = (id: string) => {
    const group = api.getPanel(id)?.group;
    return !group || group.activePanel?.id === id;
  };
  if (ids.some(forward)) return;
  api.getPanel(ids[0])?.api.setActive();
}

/**
 * Send an Agent pane away, and let go of whatever it was holding.
 *
 * The hold has to go with it or the entry outlives the pane that could clear
 * it: open into the same slot number again and the new pane would arrive
 * already held on a session you chose last week, which looks like the gesture
 * ignoring your selection.
 *
 * **Every Agent pane closes, slot 1 included.** It did not until 2026-08-19,
 * on the reasoning that a window with no Agent pane is one you can only
 * recover from with a shortcut you have to remember — but the price of that
 * was worse and was being paid every day: open a second, decide the *first*
 * pane is the one you are done with, and the only way to be left with one
 * agent again is to close the other and re-aim it. A pane that is special for a reason you
 * cannot see reads as a bug, and it was reported as one.
 *
 * What made it safe is that the shortcut is no longer the only way back:
 * `Alt+A` raises the Agent pane and re-adds it when it is gone, the same
 * action is in the palette under its name, `Alt+0` resets the layout, and
 * `PANES` in [`App.tsx`](../App.tsx) re-adds slot 1 on the next launch. Four
 * ways back, only one of which you have to remember.
 */
export function closeAgentPane(id: string): void {
  if (!dock) return;
  dropHold(id);
  dock.getPanel(id)?.api.close();
}

/**
 * Close the Agent panes anchored to sessions that have just ended.
 *
 * **The anchor does not protect the pane, and that is the point.** A held pane
 * whose session ends used to stay exactly where it was, showing *"that session
 * has ended — drop the anchor to follow the queue again"*, and the only way out
 * was to drop the anchor by hand and then close it. Asked for directly: the
 * agent is gone, so the pane should go with it, anchored or not.
 *
 * Only **held** panes. An unheld pane is not bound to anything — it shows the
 * selection, so it has already moved on and closing it would take away a view
 * you never pointed at this session in the first place.
 *
 * Two things this deliberately does not do. It does not close a pane you
 * anchored to a session that was *already* finished: the caller reports
 * transitions, so reading a session you deliberately opened is undisturbed. And
 * it does not spare slot 1 — a pane that is special for a reason you cannot see
 * reads as a bug (see [`closeAgentPane`]), and there are four ways back to an
 * Agent pane.
 */
export function closePanesFor(sessionIds: readonly string[]): void {
  if (!dock || sessionIds.length === 0) return;
  const ended = new Set(sessionIds);
  const hold = useStore.getState().scoped().paneHold;
  for (const [paneId, sessionId] of Object.entries(hold)) {
    if (!ended.has(sessionId)) continue;
    if (paneKind(paneId) !== "agent") continue;
    closeAgentPane(paneId);
  }
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
 * Move to the pane that way. `R-B52`.
 *
 * The geometry is `spatial.ts`'s and is tested there; this is the part that
 * needs dockview — turning groups into rectangles, and then *landing* in the
 * one that wins.
 *
 * **Landing means the terminal, not the tab.** Activating the group alone would
 * leave the keyboard where it was, so the next thing you typed would go to the
 * pane you just left — the exact failure this exists to prevent, running the
 * other way. Focus goes to the xterm input when the pane has one and to the
 * `[data-focus-host]` otherwise, so moving between agents leaves you able to
 * type at the one you arrived at. Chords keep working from there, so the next
 * move is another keystroke rather than a mouse.
 */
export function movePane(dir: Direction): void {
  const api = dock;
  const active = api?.activeGroup;
  if (!api || !active) return;

  const boxes: Box[] = api.groups.map((g) => {
    const r = g.element.getBoundingClientRect();
    return { id: g.id, left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  });

  const nextId = pickNeighbour(boxes, active.id, dir);
  const next = nextId ? api.groups.find((g) => g.id === nextId) : null;
  if (!next) return;

  next.api.setActive();
  // After activation, and after the render it may cause: a group that was not
  // showing cannot be focused into.
  requestAnimationFrame(() => {
    const el = next.element;
    // xterm types through a hidden textarea; that is the thing that takes focus.
    const term = el.querySelector<HTMLElement>(".xterm-helper-textarea");
    const host = el.querySelector<HTMLElement>("[data-focus-host]");
    (term ?? host ?? el).focus();
  });
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
/**
 * Go to a moment by **time**: its session, the Transcript, that turn. `R-O4`.
 *
 * By timestamp rather than by seq, because a citation comes from the transcript
 * file and a seq is assigned by the daemon on ingest — the two do not
 * correspond, and a citation that scrolled to the wrong turn would be worse
 * than one that did nothing. This is the route the search panel already takes
 * for a turn in another session.
 *
 * Same ordering rule as [`jumpToTurn`]: select first (it clears the focus
 * fields), raise the pane second, publish the moment last.
 */
export function jumpToMoment(sessionId: string, timestamp: string): void {
  useStore.getState().select(sessionId);
  showPane("transcript", "Transcript");
  useStore.setState({ focusEventTs: timestamp });
}

export function jumpToTurn(sessionId: string, seq: number | null): void {
  useStore.getState().select(sessionId);
  showPane("transcript", "Transcript");
  // By **seq**, not timestamp: a session this window has never opened has no
  // loaded events to take a timestamp from, and a seq stays pending until the
  // events arrive rather than silently missing.
  useStore.setState({ focusSeq: seq, highlightSeq: seq });
}
