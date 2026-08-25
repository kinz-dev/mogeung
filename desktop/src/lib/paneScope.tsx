/**
 * Which session a pane is looking at. `R-B49`.
 *
 * Every pane used to read `selected` directly, so "the Agent pane" meant "the
 * selected session's agent" and there could only ever be one. Two agents on
 * screen at once needs a pane that is aimed somewhere the selection is not —
 * so a pane declares *which pane it is*, and the binding is resolved from
 * there ([`usePaneBinding`](../store/index.ts)).
 *
 * **This module deliberately imports nothing but React.** The resolution lives
 * in the store, and the store cannot import a module that imports the store —
 * `useSelectedSession` is defined there, so the context has to be reachable
 * from underneath it. Keeping this a leaf is what makes that safe rather than
 * a cycle that resolves to `undefined` at module init.
 */

import { createContext, useContext, type ReactNode } from "react";

/**
 * `null` means "no pane declared this" — the palette, the rail, a dialog. Those
 * follow the selection, which is the behaviour every caller had before this
 * existed, so nothing outside the tile tree changes.
 */
const PaneIdContext = createContext<string | null>(null);

/**
 * Whether the tile is actually on screen. `true` by default, so everything
 * outside the dockview tree — the terminal panel, the dock tools, overlays —
 * keeps behaving as if this context did not exist.
 *
 * What reads it: `TerminalView`, which detaches its pty while hidden. A
 * background Agent tab is a live `tmux attach` whose TUI redraws its spinner
 * continuously; xterm parsed that stream at full rate behind the tab.
 * `R-J58`.
 */
const PaneVisibleContext = createContext<boolean>(true);

/** Wraps one tile, naming it, so what is inside can be bound elsewhere. */
export function PaneScope({
  id,
  visible = true,
  children,
}: {
  id: string;
  visible?: boolean;
  children: ReactNode;
}) {
  return (
    <PaneIdContext.Provider value={id}>
      <PaneVisibleContext.Provider value={visible}>{children}</PaneVisibleContext.Provider>
    </PaneIdContext.Provider>
  );
}

export function usePaneId(): string | null {
  return useContext(PaneIdContext);
}

export function usePaneVisible(): boolean {
  return useContext(PaneVisibleContext);
}

/**
 * The base pane behind a numbered slot: `agent:2` → `agent`.
 *
 * Slots are numbered rather than keyed by session on purpose — see
 * `nextAgentSlot` in [`panes.ts`](panes.ts). Everything that keys off *what
 * kind of pane this is* (the component to render, the zoom entry, the tab's
 * shape) goes through here rather than string-matching the id in five places.
 */
export function paneKind(id: string): string {
  const cut = id.indexOf(":");
  return cut === -1 ? id : id.slice(0, cut);
}
