/**
 * The window's shape.
 *
 * Two ways of docking, and which one a thing uses is a decision rather than a
 * habit — [ADR-0017](../../docs/decisions/0017-the-rail-is-chrome.md):
 *
 * - **Panes** are views of a session. They live in the dockview tree, are
 *   draggable and splittable, and their arrangement is saved.
 * - **Chrome** is what must stay reachable whichever pane is forward: the
 *   Attention queue on the left, the terminal across the bottom, the
 *   tool-window rail on the right. None of it is a tile.
 *
 * The egui original had to declare edge panels before the central panel or
 * lose the argument about who gets the leftover space. Flexbox has no such
 * ordering trap — but the *shape* is deliberately identical, because the two
 * clients will run side by side against one daemon until this one reaches
 * parity, and a window that arranged itself differently would be a second
 * thing to learn rather than the same thing rewritten.
 */

import * as React from "react";
import { useEffect, useRef } from "react";
import {
  DockviewReact,
  type DockviewApi,
  type DockviewReadyEvent,
  type IDockviewPanelProps,
} from "dockview";
import "dockview/dist/styles/dockview.css";
import { TooltipProvider } from "@/ui/primitives";
import { useStore } from "@/store";
import { TopBar } from "@/ui/TopBar";
import { StatusBar } from "@/ui/StatusBar";
import { QueuePanel } from "@/ui/QueuePanel";
import { Rail } from "@/ui/Rail";
import { NoticesWindow, Toasts } from "@/ui/Notices";
import { Palette } from "@/ui/Palette";
import { HealthWindow } from "@/ui/HealthWindow";
import { KeymapWindow } from "@/ui/KeymapWindow";
import { ConnectionsWindow } from "@/ui/ConnectionsWindow";
import { PromptWindow } from "@/ui/PromptWindow";
import { LaunchWindow } from "@/ui/LaunchWindow";
import { AmbientWindow } from "@/ui/AmbientWindow";
import { LabelWindow } from "@/ui/LabelWindow";
import { SearchOverlay } from "@/ui/SearchOverlay";
import { ResizeGrip } from "@/ui/WindowControls";
import { useKeymap } from "@/lib/keymap";
import { CodePane } from "@/panes/CodePane";
import { AgentPane } from "@/panes/AgentPane";
import { TerminalPanel } from "@/ui/TerminalPanel";
import { ZoomPane } from "@/ui/ZoomPane";
import { BottomDock } from "@/ui/BottomDock";
import { dropOrphanHolds, setDock } from "@/lib/panes";
import { PaneScope } from "@/lib/paneScope";
import { PaneActions, PaneTab } from "@/ui/PaneChrome";
import { useNotifications } from "@/lib/notify";

/**
 * The panes, by the **kind** the saved layout stores as `component`.
 *
 * Two wrappers, and both are boundaries rather than decoration:
 *
 * - [`ZoomPane`] owns Ctrl+wheel, so the behaviour cannot drift between panes
 *   and the pane itself does not have to know it can be scaled. Its `name` is
 *   the kind, not the panel id — two Agent panes read at one size, because the
 *   factor describes *how you read agents*, not which tile you are in.
 * - [`PaneScope`] names the tile, so what is inside can be bound to a session
 *   the queue has not selected (`R-B49`). Without it every pane resolves
 *   `selected` and two Agent panes are two views of one session.
 */
const pane =
  (
    kind: string,
    Body: React.FunctionComponent,
    opts: { scale?: boolean } = {},
  ): React.FunctionComponent<IDockviewPanelProps> =>
  (props) => (
    <PaneScope id={props.api.id}>
      <ZoomPane name={kind} scale={opts.scale}>
        <Body />
      </ZoomPane>
    </PaneScope>
  );

const components: Record<string, React.FunctionComponent<IDockviewPanelProps>> = {
  // A terminal is scaled by its font, never by CSS — same reason as Monaco
  // below, and a stored zoom from before this rule must not still apply.
  agent: pane("agent", AgentPane, { scale: false }),
  // Monaco takes the factor as a font size instead — see `ZoomPane`'s `scale`.
  code: pane("code", CodePane, { scale: false }),
};

const LAYOUT_KEY = "mogeung.layout";

/**
 * Every pane that is **always** present, and its label.
 *
 * Two left on 2026-08-06 — Changes and Transcript are dock tools now — and
 * `code` is deliberately absent from this list even though it is still a pane:
 * see `syncCodePane`. Since `R-B49` the extra Agent slots are absent for the
 * same kind of reason: `agent:2` exists because you asked for it, and a window
 * that re-added it on every launch would be a split you cannot decline.
 */
const PANES: readonly (readonly [string, string])[] = [
  ["agent", "Agent"],
];

/**
 * Panes that used to live in the centre and now live in the bottom dock.
 *
 * A layout saved before the move still names them, and dockview would restore
 * tabs whose component no longer exists — so they are stripped on load. Without
 * this the first launch after upgrading shows four dead tabs.
 */
const MOVED_TO_DOCK = ["git", "info", "debt", "insight", "changes", "transcript"];

/**
 * How many files the selected session has open. `0` means the Code pane has
 * nothing to show and should not be a tab.
 */
function openFileCount(state: ReturnType<typeof useStore.getState>): number {
  const id = state.selected;
  return id ? (state.explorer[id]?.open.length ?? 0) : 0;
}

/**
 * The Code pane exists **only while a file is open**. Asked for 2026-08-06.
 *
 * It is the one pane with nothing to say by itself: the others always describe
 * the selected session, where an empty Code tab is a promise of a file that
 * is not there. So it is added when a file is opened — from the tree, from
 * `Ctrl+P`, from a diff row, all of which go through `openFile` — and closed
 * again when the last tab goes.
 *
 * `Alt+C` still opens it deliberately, and that escape hatch is intentional:
 * this hides a pane with nothing in it, it does not forbid one.
 */
function syncCodePane(api: DockviewApi | null, open: number): void {
  if (!api) return;
  const existing = api.getPanel("code");
  if (open > 0 && !existing) {
    api.addPanel({ id: "code", component: "code", title: "Code" });
  } else if (open === 0 && existing) {
    existing.api.close();
  }
}

/**
 * The arrangement you get before you have made one.
 *
 * One tab group holding everything, Changes forward — the same default the
 * egui client shipped, and for the same reason: a docking system that opens
 * pre-split is a docking system you have to undo before you can use it.
 */
function defaultLayout(api: DockviewApi): void {
  for (const [id, title] of PANES) {
    api.addPanel({
      id,
      component: id,
      title,
      ...(id === "agent" ? {} : { position: { referencePanel: "agent" } }),
    });
  }
  api.getPanel("agent")?.api.setActive();
}

export default function App() {
  const theme = useStore((s) => s.prefs.theme);
  // The count, not the array: a selector returning the tabs would re-run this
  // on every keystroke that touches explorer state.
  const openFiles = useStore(openFileCount);
  const dockRef = useRef<DockviewApi | null>(null);
  useKeymap(dockRef);
  useNotifications();

  // The theme is an attribute on the root, so the CSS variables switch without
  // a re-render of anything that reads them. `system` follows the desktop.
  useEffect(() => {
    const root = document.documentElement;
    const apply = () => {
      const resolved =
        theme === "system"
          ? window.matchMedia("(prefers-color-scheme: light)").matches
            ? "light"
            : "dark"
          : theme;
      root.setAttribute("data-theme", resolved);
    };
    apply();
    if (theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);

  useEffect(() => syncCodePane(dockRef.current, openFiles), [openFiles]);

  const onReady = (event: DockviewReadyEvent) => {
    dockRef.current = event.api;
    setDock(event.api);
    const saved = localStorage.getItem(LAYOUT_KEY);
    let restored = false;
    if (saved) {
      try {
        event.api.fromJSON(JSON.parse(saved));
        restored = event.api.panels.length > 0;
      } catch {
        // An unreadable layout degrades to the default rather than to an
        // empty window — the rule `layout.rs` already followed. Losing an
        // arrangement is a nuisance; losing every pane is a broken app.
        restored = false;
      }
    }
    // Before anything else: drop tabs for panes that have moved out of the
    // centre, or a saved layout resurrects them as empty groups.
    for (const id of MOVED_TO_DOCK) event.api.getPanel(id)?.api.close();

    if (!restored) defaultLayout(event.api);

    // Every pane, always present. Now that tabs cannot be closed, a layout
    // saved *before* that change can still be missing one — and a pane you can
    // only reach by remembering its shortcut is a pane you have lost.
    for (const [id, title] of PANES) {
      if (!event.api.getPanel(id)) event.api.addPanel({ id, component: id, title });
    }

    // The effect above has already run once by now, against a `dockRef` that
    // was still null — so a saved layout naming `code` with no file open would
    // otherwise restore an empty tab and keep it.
    syncCodePane(event.api, openFileCount(useStore.getState()));

    // A hold belongs to a pane, so a hold whose pane is not in the restored
    // layout belongs to nothing. Left behind it is worse than untidy: split
    // into that slot number again and the new pane arrives already held on a
    // session chosen last week, which reads as the split ignoring your
    // selection. `closeAgentPane` clears its own; this catches the layouts that
    // lost a pane some other way.
    dropOrphanHolds(event.api);

    // Written on change rather than on a timer, and never mid-drag: dockview
    // fires while a sash moves, and saving each frame would write the file
    // sixty times a second.
    let pending: number | null = null;
    event.api.onDidLayoutChange(() => {
      if (pending !== null) window.clearTimeout(pending);
      pending = window.setTimeout(() => {
        try {
          localStorage.setItem(LAYOUT_KEY, JSON.stringify(event.api.toJSON()));
        } catch {
          /* a full quota costs the arrangement, not the session */
        }
      }, 400);
    });
  };

  return (
    <TooltipProvider>
      <div className="relative flex h-full flex-col bg-[var(--bg)]">
        <TopBar />
        <div className="flex min-h-0 flex-1">
          <QueuePanel />
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="min-h-0 flex-1">
              <DockviewReact
                components={components}
                defaultTabComponent={PaneTab}
                // Per **group**, which is what makes a split correct without a
                // line of layout-aware code: two Agent panes side by side are
                // two groups and get one set of controls each, and tabbed
                // together they are one group showing the active tab's.
                rightHeaderActionsComponent={PaneActions}
                onReady={onReady}
                className="dockview-theme-mogeung h-full"
              />
            </div>
            <TerminalPanel />
            <BottomDock />
          </div>
          <Rail />
        </div>
        <StatusBar />
        <Palette dock={dockRef} />
        <HealthWindow />
        <KeymapWindow />
      <ConnectionsWindow />
      <PromptWindow />
      <LaunchWindow />
      <AmbientWindow />
        <LabelWindow />
        <SearchOverlay />
        <NoticesWindow />
        <Toasts />
        <ResizeGrip />
      </div>
    </TooltipProvider>
  );
}
