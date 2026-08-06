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
 * What a pane's tab should read, whether it is held, and what to say on hover.
 *
 * A held pane names the session it is held on even when that session has gone
 * — `#dead` beats a tab that reverts to saying `Agent`, because the second one
 * looks like the pane came loose rather than like the thing it watched ended.
 *
 * **A tab names the thing in it, not the kind of thing it is.** `Agent` and
 * `Code` were fine while the centre held one of each; the moment a pane can be
 * one of several, a tab that says what *sort* of pane it is has stopped
 * identifying it. The hover text is where the kind survives, along with the
 * detail the tab has no width for.
 */
export interface PaneTitle {
  text: string;
  held: boolean;
  hint: string;
}

export function usePaneTitle(paneId: string, fallback: string): PaneTitle {
  const kind = paneKind(paneId);
  const held = useStore((s) => s.scoped().paneHold[paneId] ?? null);
  const selected = useStore((s) => s.selected);
  const id = held ?? selected;
  const session = useStore((s) => (id ? (s.sessions[id] ?? null) : null));
  const label = useStore((s) => (id ? (s.scoped().labels[id] ?? null) : null));
  const code = useCodeFile(kind === "code" ? id : null);

  if (kind === "code") {
    return {
      text: code ? code.name : "Code",
      held: !!held,
      hint: code ? `${code.path} — read-only` : "Code — read-only, always",
    };
  }
  if (kind !== "agent") return { text: fallback, held: false, hint: fallback };

  const attached = "the agent, attached through tmux — closing this pane detaches, it never kills";
  if (!id) return { text: "Agent", held: false, hint: attached };
  if (!session) {
    return {
      text: held ? `${id.slice(0, 8)} — ended` : "Agent",
      held: !!held,
      hint: held ? `held on ${id}, which has ended — drop the anchor to follow the queue again` : attached,
    };
  }
  const name = label ?? sessionLabel(session);
  return {
    text: name,
    held: !!held,
    hint: held ? `held on ${name} — it will not follow the queue` : `${name} — ${attached}`,
  };
}

/**
 * The file the Code pane is actually showing, by basename.
 *
 * **The focused group's active tab**, not the first open file: the pane splits
 * internally (`R-B25`), and with two files side by side inside it the tab has
 * to name the one you are in or it is worse than saying `Code` — a label that
 * points at the wrong half is a label you learn to distrust.
 *
 * The basename alone. The row under the inner tab strip already carries the
 * full path, and a tab wide enough for `desktop/src/ui/PaneChrome.tsx` is a tab
 * that leaves no room for the pane beside it.
 */
function useCodeFile(sessionId: string | null): { name: string; path: string } | null {
  // Two primitive selectors rather than one returning an object: a selector
  // that mints `{name, path}` is never equal to itself, which zustand reads as
  // a change on every store tick — the identity trap `scoped()` documents, and
  // this tab re-renders on every keystroke that touches explorer state.
  const path = useStore((s) => {
    const st = sessionId ? s.explorer[sessionId] : null;
    if (!st) return null;
    const index = st.active[st.focus];
    return index === null ? null : (st.open[index]?.path ?? null);
  });
  if (!path) return null;
  const cut = path.lastIndexOf("/");
  return { name: cut === -1 ? path : path.slice(cut + 1), path };
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

  const { text, held, hint } = usePaneTitle(props.api.id, fallback);
  return (
    <div className="dv-default-tab" title={hint}>
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
