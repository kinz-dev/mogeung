/**
 * The tab, and the controls beside it. `R-B49`.
 *
 * Before this the Agent pane wore two headers: dockview's tab, 30px and
 * uppercase, saying `AGENT` — and its own `PaneHeader`, 28px and uppercase,
 * saying `AGENT` again with three controls on the right. 58px of chrome per
 * pane for one word twice, and splitting the centre made each half pay all of
 * it.
 *
 * So there is one header now. The tab says **which session**, because with two
 * agents up "Agent | Agent" identifies neither, and the controls move into the
 * group's right actions, which is per-group and therefore lands correctly
 * whether the two panes are tabbed together or split apart.
 */

import * as React from "react";
import type { IDockviewHeaderActionsProps, IDockviewPanelHeaderProps } from "dockview";
import { Anchor, Columns2, X } from "lucide-react";
import { useStore, togglePaneHold } from "@/store";
import { paneKind } from "@/lib/paneScope";
import { closeAgentPane, nextAgentSlot, splitAgent } from "@/lib/panes";
import { sessionLabel } from "@/wire/types";
import { Chip, Dim, IconButton } from "@/ui/primitives";
import { hostLabel, reachFor } from "@/lib/tmux";

/**
 * What a pane's tab should read, and whether it is held.
 *
 * A held pane names the session it is held on even when that session has gone
 * — `#dead` beats a tab that reverts to saying `Agent`, because the second one
 * looks like the pane came loose rather than like the thing it watched ended.
 */
export function usePaneTitle(paneId: string, fallback: string): { text: string; held: boolean } {
  const kind = paneKind(paneId);
  const held = useStore((s) => s.scoped().paneHold[paneId] ?? null);
  const selected = useStore((s) => s.selected);
  const id = held ?? selected;
  const session = useStore((s) => (id ? (s.sessions[id] ?? null) : null));
  const label = useStore((s) => (id ? (s.scoped().labels[id] ?? null) : null));

  // Only the Agent pane is bindable in a way worth naming on its tab. `Code`
  // says what it is; a file name there would fight the editor's own tab strip.
  if (kind !== "agent") return { text: fallback, held: false };
  if (!id) return { text: "Agent", held: false };
  if (!session) return { text: held ? `${id.slice(0, 8)} — ended` : "Agent", held: !!held };
  return { text: label ?? sessionLabel(session), held: !!held };
}

/**
 * A tab with no close button, and — since `R-B49` — a name that can change.
 *
 * The decision of *what kind of tab this is* is taken from the panel id rather
 * than from a registered `tabComponent`, on purpose: `tabComponent` is a
 * property of the saved layout, so a window restored from a file written before
 * this feature would come back with default tabs and stale titles. The id is
 * always there.
 */
export function PaneTab(props: IDockviewPanelHeaderProps) {
  const [fallback, setFallback] = React.useState(props.api.title ?? "");
  React.useEffect(() => {
    const sub = props.api.onDidTitleChange((e) => setFallback(e.title));
    return () => sub.dispose();
  }, [props.api]);

  const { text, held } = usePaneTitle(props.api.id, fallback);
  return (
    <div className="dv-default-tab" title={held ? `held on ${text} — it will not follow the queue` : text}>
      {held && <Anchor className="mr-1 h-3 w-3 shrink-0 text-[var(--blue)]" aria-label="held" />}
      <span className="dv-default-tab-content">{text}</span>
    </div>
  );
}

/**
 * The controls for whichever pane is forward in this group.
 *
 * Rendered per group, so a split shows one set per half and a tabbed pair shows
 * the active tab's — which is the behaviour you would have to write by hand if
 * these lived inside the pane.
 */
export function PaneActions(props: IDockviewHeaderActionsProps) {
  const paneId = props.activePanel?.id ?? null;
  const kind = paneId ? paneKind(paneId) : null;

  const held = useStore((s) => (paneId ? (s.scoped().paneHold[paneId] ?? null) : null));
  const selected = useStore((s) => s.selected);
  const id = held ?? selected;
  const session = useStore((s) => (id ? (s.sessions[id] ?? null) : null));
  const daemon = useStore((s) => s.daemon);
  const machineId = useStore((s) => s.machineId);
  const send = useStore((s) => s.send);

  const reach = React.useMemo(() => reachFor(daemon, machineId), [daemon, machineId]);
  const host = session?.tmux_target && reach ? hostLabel(reach) : null;

  if (kind !== "agent" || !paneId) return null;

  const canSplit = nextAgentSlot(props.containerApi) !== null;

  return (
    <div className="flex h-full items-center gap-1 pr-1">
      {session?.tmux_target && (
        <Dim className="hidden font-mono text-2xs sm:block" title="the tmux target this pane is attached to">
          {session.tmux_target}
        </Dim>
      )}
      {host && (
        <Chip color="var(--amber)" title="tmux runs on that machine, reached over ssh">
          {host}
        </Chip>
      )}
      <IconButton
        title={
          held
            ? "let this pane follow the queue again"
            : "hold this pane on this session, so clicking the queue moves the others"
        }
        active={!!held}
        disabled={!held && !selected}
        onClick={() => togglePaneHold(paneId)}
      >
        <Anchor className="h-3.5 w-3.5" />
      </IconButton>
      <IconButton
        title={canSplit ? "another Agent pane, beside this one" : "four Agent panes is the ceiling"}
        disabled={!canSplit}
        onClick={() => splitAgent()}
      >
        <Columns2 className="h-3.5 w-3.5" />
      </IconButton>
      {session && (
        <button
          type="button"
          onClick={() => send({ cmd: "focus_terminal", session_id: session.id })}
          className="rounded-sm border border-[var(--border)] px-1.5 py-px text-2xs outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:border-[var(--border-hover)] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--ring)]"
          title="raise the terminal application this session runs in — it moves your window, then you type"
        >
          raise
        </button>
      )}
      {/*
        Only the extra panes close, and only from here. The base `agent` pane is
        permanent for the reason `PaneTab` has no close button at all: a pane
        you can lose is a pane you have to rediscover, and a split you cannot
        undo is worse than no split.
      */}
      {paneId !== "agent" && (
        <IconButton title="close this Agent pane — it detaches, it never kills" onClick={() => closeAgentPane(paneId)}>
          <X className="h-3.5 w-3.5" />
        </IconButton>
      )}
    </div>
  );
}
