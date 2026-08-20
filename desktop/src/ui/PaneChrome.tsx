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
import { Anchor, Columns2, Folder, FolderOpen, GitBranch, X } from "lucide-react";
import { useStore, togglePaneHold } from "@/store";
import { paneKind } from "@/lib/paneScope";
import { closeAgentPane, parseFilePaneId, splitAgent } from "@/lib/panes";
import { sessionLabel } from "@/wire/types";
import { dirTail } from "@/lib/format";
import { Chip, Dim, IconButton } from "@/ui/primitives";
import { hostLabel, reachFor } from "@/lib/tmux";
import { closeFile, revealInFiles } from "@/lib/explorer";
import { ContextMenu, MenuItem, MenuLabel, MenuSeparator } from "@/ui/Menu";
import { writeClipboard } from "@/lib/clipboard";

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
  const file = fileOf(paneId);
  // The file's **own** session, which is not necessarily the selected one — a
  // file pane stays put when the queue moves (`R-B53`), so asking `selected`
  // for its root would name the wrong repository the moment you clicked away.
  const fileRoot = useStore((s) => {
    if (!file) return null;
    const owner = s.sessions[file.session];
    return owner ? (owner.repo_root || owner.cwd) : null;
  });
  if (file) {
    return {
      text: file.name,
      held: false,
      // **The whole path, since `R-J24`.** It used to be the repo-relative one,
      // which is the half you already have: the tab says the basename and the
      // strip under it says the rest of the relative path. What neither said is
      // *which checkout* — and with two agents in two worktrees of the same
      // repo, `src/lib.rs` is the ambiguous half of the answer.
      hint: `${fullPath(fileRoot, file.path)} — read-only, and bound to the session that opened it`,
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
 * A file pane's own file, from its id. `R-B53`.
 *
 * No lookup into the store at all: the pane *is* the file, so the tab can name
 * it without asking anything. The basename alone, because the row under the tab
 * carries the path and a tab wide enough for `desktop/src/ui/PaneChrome.tsx`
 * leaves no room for the pane beside it.
 */
/** Absolute where the session's root is known, relative where it is not — a
 *  path that pretends to be absolute is worse than one that admits it is not. */
export function fullPath(root: string | null, path: string): string {
  return root ? `${root.replace(/\/$/, "")}/${path}` : path;
}

function fileOf(paneId: string): { name: string; path: string; session: string; rev: string | null } | null {
  const ref = parseFilePaneId(paneId);
  if (!ref) return null;
  const cut = ref.path.lastIndexOf("/");
  return { name: cut === -1 ? ref.path : ref.path.slice(cut + 1), path: ref.path, session: ref.session, rev: ref.rev };
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
  const file = fileOf(props.api.id);
  const root = useStore((s) => {
    if (!file) return null;
    const owner = s.sessions[file.session];
    return owner ? (owner.repo_root || owner.cwd) : null;
  });

  const tab = (
    <div className="dv-default-tab" title={hint}>
      {held && <Anchor className="mr-1 h-3 w-3 shrink-0 text-[var(--blue)]" aria-label="held" />}
      <span className="dv-default-tab-content">{text}</span>
      {/*
        **Only a file closes.** Every other pane in the centre is permanent —
        a view you can lose is a view you have to rediscover, which is why
        `is_tab_closable` was argued over back in feature 0006. A file is the
        exception and always was: it is a document you are finished with, and
        `R-B53` made that a real tab rather than a row in a strip.
      */}
      {file && (
        <button
          type="button"
          title={`close ${file.name}`}
          aria-label={`close ${file.name}`}
          onClick={(e) => {
            e.stopPropagation();
            closeFile(file.session, file.path, file.rev);
          }}
          className="ml-1 shrink-0 opacity-60 outline-none transition-opacity duration-[var(--dur-fast)] hover:opacity-100 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--ring)]"
        >
          <X className="h-3 w-3" />
        </button>
      )}
    </div>
  );

  // **Only a file gets a menu.** `R-J24`. The other tabs have nothing to copy
  // — an Agent pane's "path" is a tmux target and a dock tool has none at all
  // — and a menu that opens with one greyed item is worse than no menu.
  if (!file) return tab;

  return (
    <ContextMenu trigger={tab}>
      <MenuLabel>{file.name}</MenuLabel>
      <MenuItem onSelect={() => void copyPath(fullPath(root, file.path), "full path")}>
        Copy full path
      </MenuItem>
      <MenuItem onSelect={() => void copyPath(file.path, "path")}>Copy path in the repo</MenuItem>
      <MenuItem onSelect={() => void copyPath(file.name, "file name")}>Copy file name</MenuItem>
      <MenuSeparator />
      {/* The other half of `R-J25`: the tree can follow you continuously, and
          this is the same gesture for someone who does not want it to. */}
      <MenuItem onSelect={() => revealInFiles(file.session, file.path)}>Reveal in Files</MenuItem>
      <MenuSeparator />
      <MenuItem onSelect={() => closeFile(file.session, file.path, file.rev)}>Close</MenuItem>
    </ContextMenu>
  );
}

/**
 * Copy, and say so. `R-J24`.
 *
 * The notice is not decoration: a clipboard write is the one action in this
 * window with no visible result at all, so without it the only way to know
 * whether the menu did anything is to go and paste somewhere.
 */
async function copyPath(text: string, what: string): Promise<void> {
  const { pushNotice, pushError } = useStore.getState();
  try {
    await writeClipboard(text);
    pushNotice(`${what} copied — ${text}`);
  } catch (e) {
    pushError(`could not copy: ${String(e)}`);
  }
}

/**
 * The session a pane is showing: the one it is held on, else the selected one.
 *
 * Both header components need this and both must answer it the same way, or a
 * held pane would name one session on the left of its header and another on the
 * right. `R-B49` is what lets the two differ at all.
 */
function usePaneSession(paneId: string | null) {
  const held = useStore((s) => (paneId ? (s.scoped().paneHold[paneId] ?? null) : null));
  const selected = useStore((s) => s.selected);
  const id = held ?? selected;
  const session = useStore((s) => (id ? (s.sessions[id] ?? null) : null));
  return { held, selected, id, session };
}

/**
 * The directory the session was started in, on the left of the pane header.
 * Asked for 2026-08-07, left-aligned by name.
 *
 * `cwd`, not `repo_root`: the question is where you ran `claude`, and the two
 * differ whenever a session was started in a subdirectory — which is exactly
 * the case where you want to be told.
 *
 * Shortened from the front, because the tail of a path is the part that
 * identifies it, with the whole thing on hover. Dockview renders this container
 * immediately after the tabs, so it costs the pane no vertical room and sits
 * where reading starts.
 */
export function PaneCwd(props: IDockviewHeaderActionsProps) {
  const paneId = props.activePanel?.id ?? null;
  const { session } = usePaneSession(paneId);
  if (!paneId || paneKind(paneId) !== "agent" || !session?.cwd) return null;
  return (
    <div className="flex h-full min-w-0 items-center pl-1.5">
      <Dim
        className="flex min-w-0 items-center gap-1 font-mono text-2xs"
        title={`started in ${session.cwd}`}
      >
        <Folder className="h-3 w-3 shrink-0" />
        <span className="max-w-[20rem] truncate">{dirTail(session.cwd)}</span>
      </Dim>
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

  const { held, selected, session } = usePaneSession(paneId);
  const daemon = useStore((s) => s.daemon);
  const machineId = useStore((s) => s.machineId);
  const send = useStore((s) => s.send);

  const reach = React.useMemo(() => reachFor(daemon, machineId), [daemon, machineId]);
  const host = session?.tmux_target && reach ? hostLabel(reach) : null;

  if (kind !== "agent" || !paneId) return null;

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
      {/*
        The branch this session is working on. Asked for 2026-08-07.

        **Only when there is one.** `git_branch` is `null` for a directory that
        is not a repository *and* for a detached HEAD, and both are states where
        naming a branch would be a small lie — so the row simply has one fewer
        thing on it rather than saying `(none)`.

        Truncated with the whole name on hover: a branch called
        `feature/ENG-441-the-long-one` would otherwise push the controls off a
        narrow pane, and the controls are the reason this row exists.
      */}
      {session?.git_branch && (
        <Dim
          className="hidden min-w-0 items-center gap-1 text-2xs sm:flex"
          title={`on branch ${session.git_branch}`}
        >
          <GitBranch className="h-3 w-3 shrink-0" />
          <span className="max-w-[14rem] truncate">{session.git_branch}</span>
        </Dim>
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
      {/*
        **The folder, in the machine's own file manager.** Asked for
        2026-08-19, and placed here — beside the anchor — at the same ask.

        A handoff, which is what pillar K asks for rather than an exception to
        it: this window reads a worktree and never writes it, so moving,
        renaming or opening a file with something else belongs to an
        application that can. The daemon runs it, because the folder is on the
        daemon's machine and a path is not the same answer on two of them.

        Disabled without a `cwd` rather than hidden: the control is per group
        and a button that comes and goes is one you stop reaching for.
      */}
      <IconButton
        title={
          session?.cwd
            ? `show ${session.cwd} in the file manager`
            : "no folder to show — this pane has no session"
        }
        disabled={!session?.cwd}
        onClick={() => session && send({ cmd: "open_folder", session_id: session.id })}
      >
        <FolderOpen className="h-3.5 w-3.5" />
      </IconButton>
      {/*
        **No ceiling since `R-J35`**, asked for 2026-08-20: *"just open the
        panel and let the user move around himself to fit his own screen"*. The
        cost the old limit named is real and is now yours to see — each pane is
        a live `tmux attach` and a pty — but a number here applied the same
        limit to a laptop and to a 49-inch screen.
      */}
      <IconButton title="another Agent pane, beside this one — it arrives anchored" onClick={() => splitAgent()}>
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
        **Every Agent pane closes, slot 1 included**, since 2026-08-19. Only the
        extra ones did before, and the asymmetry was reported as a bug on the
        day it bit: split, and the pane you want rid of is as often the first as
        the second — but the first refused, so the way back to one agent was to
        close the *other* one and re-aim what was left.

        A close is a detach either way, and the pane comes back four ways —
        `Alt+A`, the palette, `Alt+0`, or the next launch — so there is nothing
        left for the exception to protect. See `closeAgentPane`.
      */}
      <IconButton title="close this Agent pane — it detaches, it never kills" onClick={() => closeAgentPane(paneId)}>
        <X className="h-3.5 w-3.5" />
      </IconButton>
    </div>
  );
}
