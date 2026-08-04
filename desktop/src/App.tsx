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
  type IDockviewPanelHeaderProps,
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
import { LabelWindow } from "@/ui/LabelWindow";
import { SearchOverlay } from "@/ui/SearchOverlay";
import { ResizeGrip } from "@/ui/WindowControls";
import { useKeymap } from "@/lib/keymap";
import { ChangesPane } from "@/panes/ChangesPane";
import { TranscriptPane } from "@/panes/TranscriptPane";
import { CodePane } from "@/panes/CodePane";
import { AgentPane } from "@/panes/AgentPane";
import { TerminalPanel } from "@/ui/TerminalPanel";
import { ZoomPane } from "@/ui/ZoomPane";
import { BottomDock } from "@/ui/BottomDock";
import { setDock } from "@/lib/panes";

/**
 * The panes, by the id the saved layout stores.
 *
 * Every one is wrapped in [`ZoomPane`] rather than each pane wiring its own
 * Ctrl+wheel: one boundary means the behaviour cannot drift between panes, and
 * the pane itself does not have to know it can be scaled.
 */
const pane =
  (
    id: string,
    Body: React.FunctionComponent,
    opts: { scale?: boolean } = {},
  ): React.FunctionComponent<IDockviewPanelProps> =>
  () => (
    <ZoomPane name={id} scale={opts.scale}>
      <Body />
    </ZoomPane>
  );

/**
 * A tab with no close button.
 *
 * These are fixed views of a session, not documents: there is no such thing as
 * "the Git tab you are finished with". Closing one only hides a view and leaves
 * you hunting for where it went — `ResetLayout` exists because a docking system
 * *can* be got into a state you cannot undo by hand, and a close button on
 * every tab is the fastest route there.
 *
 * A custom tab rather than CSS that hides the button: this also removes
 * dockview's middle-click-to-close, which the CSS would have left armed and
 * invisible.
 */
function PaneTab(props: IDockviewPanelHeaderProps) {
  const [title, setTitle] = React.useState(props.api.title ?? "");
  React.useEffect(() => {
    const sub = props.api.onDidTitleChange((e) => setTitle(e.title));
    return () => sub.dispose();
  }, [props.api]);
  return (
    <div className="dv-default-tab" title={title}>
      <span className="dv-default-tab-content">{title}</span>
    </div>
  );
}

const components: Record<string, React.FunctionComponent<IDockviewPanelProps>> = {
  changes: pane("changes", ChangesPane),
  transcript: pane("transcript", TranscriptPane),
  agent: pane("agent", AgentPane),
  // Monaco takes the factor as a font size instead — see `ZoomPane`'s `scale`.
  code: pane("code", CodePane, { scale: false }),
};

const LAYOUT_KEY = "mogeung.layout";

/** Every pane and its label, in the order the default layout lays them out. */
const PANES: readonly (readonly [string, string])[] = [
  ["changes", "Changes"],
  ["transcript", "Transcript"],
  ["code", "Code"],
  ["agent", "Agent"],
];

/**
 * Panes that used to live in the centre and now live in the bottom dock.
 *
 * A layout saved before the move still names them, and dockview would restore
 * tabs whose component no longer exists — so they are stripped on load. Without
 * this the first launch after upgrading shows four dead tabs.
 */
const MOVED_TO_DOCK = ["git", "info", "debt", "insight"];

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
      ...(id === "changes" ? {} : { position: { referencePanel: "changes" } }),
    });
  }
  api.getPanel("changes")?.api.setActive();
}

export default function App() {
  const theme = useStore((s) => s.prefs.theme);
  const dockRef = useRef<DockviewApi | null>(null);
  useKeymap(dockRef);

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
        <LabelWindow />
        <SearchOverlay />
        <NoticesWindow />
        <Toasts />
        <ResizeGrip />
      </div>
    </TooltipProvider>
  );
}
