/**
 * Your own shells, across the bottom. `R-B31`, `R-B33`.
 *
 * A panel rather than a pane, and that was learnt the hard way: `R-B31` shipped
 * it as a pane and the next day's use said the pane was wrong — it followed the
 * selection, vanished when the selection moved, and could not exist at all
 * before a session did, which is exactly when you want a shell to start one.
 *
 * Each shell is `tmux new-session -A`, so it is the **same** shell across
 * restarts: a build still running from an hour ago is still there, and closing
 * the window detaches instead of killing. A `claude` started in one of these is
 * a session mogeung observes like any other rather than one it owns —
 * [ADR-0011](../../docs/decisions/0011-own-a-shell-never-an-agent.md).
 *
 * **The shells are tiles since `R-B56`**, asked 2026-09-28: *"I want to change
 * it to dockable panel like those we have in the AI session at the top"*. One
 * shell at a time behind a tab strip meant a build and its log could not be
 * watched together; now the panel holds its own dockview, and every shell is
 * a split of it — tmux's shape, with no tab bar, asked for the same day. It
 * is a second dockview rather than the centre's, so nothing crosses between
 * the two. See [`shellDock.ts`](../lib/shellDock.ts) for which store owns
 * what, and for why a split never holds two shells.
 */


import * as React from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  DockviewReact,
  type DockviewApi,
  type DockviewReadyEvent,
  type IDockviewPanelProps,
} from "dockview";
import { Plus, ChevronDown } from "lucide-react";
import { useStore } from "@/store";
import { Chip, Dim, IconButton } from "@/ui/primitives";
import { MenuItem, MenuSeparator } from "@/ui/Menu";
import { TerminalView } from "@/ui/Terminal";
import { CommandBox } from "@/ui/CommandBox";
import { cursorToPlaceholder } from "@/lib/command";
import { ptyWrite } from "@/lib/tauri";
import { hostLabel, reachFor, shellArgs, spawnAs, type Reach } from "@/lib/tmux";
import { base } from "@/lib/format";
import { useChord } from "@/lib/keymap";
import { PaneScope } from "@/lib/paneScope";
import {
  openShell,
  restoreShellLayout,
  setShellDock,
  shellForSelection,
  shellLayoutKey,
  shellOfPane,
  shellPaneId,
  splitShell,
  syncShells,
  takeDeferredSplit,
  type Shell,
} from "@/lib/shellDock";

const MIN_HEIGHT = 120;
/** Shorter than this, an exit is a failure to start rather than an `exit`. */
const QUICK_EXIT_MS = 2000;

const shellTitle = (s: Shell) => (s[1] ? base(s[1]) : s[0]);

/** The shell a panel id names, read live — the layout never carries it. */
function useShell(name: string | null): Shell | null {
  return useStore((s) => (name ? (s.scoped().shells.find(([n]) => n === name) ?? null) : null));
}

/** Where a shell runs: where it was opened, else wherever the daemon is. */
function useReach(shell: Shell | null): Reach | null {
  const daemon = useStore((s) => s.daemon);
  const machineId = useStore((s) => s.machineId);
  const daemonReach = useMemo(() => reachFor(daemon, machineId), [daemon, machineId]);
  return shell?.[2] ? { kind: "ssh", dest: shell[2] } : daemonReach;
}

/**
 * Removes the *split*, not the session. tmux keeps it; opening a shell with
 * the same name attaches straight back.
 */
function closeShell(name: string) {
  const { scoped, setScoped } = useStore.getState();
  setScoped({ shells: scoped().shells.filter(([n]) => n !== name) });
}

/**
 * Split and close, for one shell, in the terminal's own right-click menu under
 * Copy and Paste. `R-B56`. The split is of *this* shell, the one under the
 * mouse, which need not be the one in front.
 */
function ShellMenuItems({ name }: { name: string }) {
  const right = useChord("terminal.split_right");
  const down = useChord("terminal.split_down");
  return (
    <>
      <MenuItem onSelect={() => splitShell("right", name)}>
        Split right {right && <span className="text-[var(--dim)]">{right}</span>}
      </MenuItem>
      <MenuItem onSelect={() => splitShell("below", name)}>
        Split down {down && <span className="text-[var(--dim)]">{down}</span>}
      </MenuItem>
      <MenuSeparator />
      <MenuItem onSelect={() => closeShell(name)}>
        Close <span className="text-[var(--dim)]">— the tmux session keeps running</span>
      </MenuItem>
    </>
  );
}

/** One shell, as a tile. */
function ShellPane(props: IDockviewPanelProps) {
  // A hidden shell holds no pty, the same rule as a background Agent tab
  // (`R-J58`).
  const [visible, setVisible] = useState(props.api.isVisible);
  useEffect(() => {
    const d = props.api.onDidVisibilityChange((e) => setVisible(e.isVisible));
    return () => d.dispose();
  }, [props.api]);
  const name = shellOfPane(props.api.id);
  const shell = useShell(name);
  const reach = useReach(shell);

  if (!shell) return null;
  if (!reach) {
    return (
      <div className="flex h-full items-center justify-center bg-[var(--bg-panel)] px-6 text-center">
        <Dim className="text-xs">
          that daemon is on another machine and has no <code className="font-mono">ssh_target</code> —
          a shell here would open on the wrong filesystem
        </Dim>
      </div>
    );
  }
  return (
    <PaneScope id={props.api.id} visible={visible}>
      <div className="h-full bg-[var(--bg-panel)]">
        <TerminalView
          id={shellPaneId(shell[0])}
          command={spawnAs(reach, shellArgs(shell[0], shell[1]))}
          menu={<ShellMenuItems name={shell[0]} />}
          // `exit` closes the split — reported the same day: *"the empty tab
          // didn't close itself and I have to use the mouse"*. Except a shell
          // that dies as it opens: that is a failure (no tmux, ssh refused),
          // and closing it would hide the one line that says so.
          onExit={(ran) => {
            if (ran >= QUICK_EXIT_MS) closeShell(shell[0]);
          }}
        />
      </div>
    </PaneScope>
  );
}

/**
 * A tab that draws nothing, for a tab bar that is never shown. Not removable:
 * a layout saved while shells had tabs names `props.defaultTabComponent` on
 * every panel, and without one dockview refuses the whole layout — which
 * `restoreShellLayout` then degrades to one flat row. Found in a browser the
 * day the tab bar went.
 */
function NoTab() {
  return null;
}

function ShellsEmpty() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-1 bg-[var(--bg-panel)]">
      <Dim className="text-xs">no shells open</Dim>
      <Dim className="text-2xs opacity-70">a shell here is yours — mogeung never starts an agent in one</Dim>
    </div>
  );
}

const components = { shell: ShellPane };

export function TerminalPanel() {
  const open = useStore((s) => s.showTerminal);
  // Its own component so that each show is a fresh tree: hiding unmounts the
  // dockview, and an api held across that would be a disposed one.
  return open ? <OpenTerminalPanel /> : null;
}

function OpenTerminalPanel() {
  const scoped = useStore((s) => s.scoped());
  const layoutKey = useStore((s) => shellLayoutKey(s.daemon?.machine_id));
  const [height, setHeight] = useState(260);
  const [api, setApi] = useState<DockviewApi | null>(null);
  const [active, setActive] = useState<string | null>(null);
  /** The machine whose layout the tree holds — restored once per machine. */
  const restoredFor = useRef<string | null>(null);
  const keyRef = useRef(layoutKey);
  keyRef.current = layoutKey;

  const askChord = useChord("terminal.command_box");
  const shells = scoped.shells;

  // The tree follows the list — after a restore, and on every change to it.
  useEffect(() => {
    if (!api) return;
    if (restoredFor.current !== layoutKey) {
      restoredFor.current = layoutKey;
      let saved: string | null = null;
      try {
        saved = localStorage.getItem(layoutKey);
      } catch {
        /* no storage is no arrangement, not no shells */
      }
      restoreShellLayout(api, saved);
    }
    syncShells(api, shells, shellTitle);
    const front = api.activePanel ? shellOfPane(api.activePanel.id) : null;
    setActive(front);
    // A chord pressed while the panel was down: now there is a front to split.
    const split = takeDeferredSplit();
    if (split) splitShell(split, front);
  }, [api, layoutKey, shells]);

  // Saved on change, never mid-drag — the window's own layout's rule — and
  // flushed when the panel is put away, which unmounts the tree.
  useEffect(() => {
    if (!api) return;
    let pending: number | null = null;
    const save = () => {
      pending = null;
      try {
        localStorage.setItem(keyRef.current, JSON.stringify(api.toJSON()));
      } catch {
        /* a full quota costs the arrangement, not the shells */
      }
    };
    const onLayout = api.onDidLayoutChange(() => {
      if (pending !== null) window.clearTimeout(pending);
      pending = window.setTimeout(save, 400);
    });
    const onActive = api.onDidActivePanelChange((p) => setActive(p ? shellOfPane(p.id) : null));
    // The keymap's splits reach the tree through here, as the centre's
    // actions reach theirs through `setDock`.
    setShellDock(api);
    return () => {
      setShellDock(null);
      onLayout.dispose();
      onActive.dispose();
      // Always, not only with a save pending: whatever the tree holds now is
      // what the next show should restore.
      if (pending !== null) window.clearTimeout(pending);
      save();
    };
  }, [api]);

  const current = useShell(active);
  const reach = useReach(current);
  const host = reach ? hostLabel(reach) : null;

  const onDrag = (e: React.MouseEvent) => {
    const startY = e.clientY;
    const startH = height;
    let latest = startH;
    const move = (ev: MouseEvent) => {
      latest = Math.min(window.innerHeight - 200, Math.max(MIN_HEIGHT, startH - (ev.clientY - startY)));
      setHeight(latest);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  // A split to the right of the shell in front. See `shellForSelection` for
  // where it is rooted — the selected session's worktree, not the shell's.
  const addShell = () => openShell(shellForSelection(shells));

  return (
    <div className="flex shrink-0 flex-col border-t border-[var(--border)]" style={{ height }}>
      <div onMouseDown={onDrag} className="h-1 cursor-row-resize hover:bg-[var(--blue)]" title="drag to resize" />

      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-[var(--border)] px-1">
        <Dim className="px-1 text-2xs uppercase tracking-wide">Terminal</Dim>
        <IconButton title="a new shell, rooted in the selected session's worktree" onClick={addShell}>
          <Plus size={13} />
        </IconButton>
        {host && (
          <Chip color="var(--amber)" title="the shell in front runs on that machine, over ssh">
            {host}
          </Chip>
        )}
        {/*
          The hint, and it reads the **binding** rather than a string. A tooltip
          that names a chord somebody has rebound is worse than one that says
          nothing — and on a Mac a hard-coded `Alt+…` would lie to everyone.
          `useChord` is the same hook the dock strip and the rail use, added
          after four tooltips were found naming keys that had moved.
        */}
        <Dim className="ml-auto hidden text-2xs sm:block">
          {askChord ? `${askChord} — ask for a command` : ""}
        </Dim>
        <div>
          <IconButton
            title="hide the panel — the shells keep running (Ctrl+`)"
            onClick={() => useStore.setState({ showTerminal: false })}
          >
            <ChevronDown size={13} />
          </IconButton>
        </div>
      </div>

      {/*
        Above the terminal rather than over it: an overlay on the canvas would
        cover the line you are asking about, and `R-O12`'s harness already
        refused the version that drew *into* the line. With shells side by side
        it writes into the one in front — the last one you clicked.
      */}
      <CommandBox
        repo={current ? current[1] : null}
        onAccept={(command, run) => {
          // Straight into this window's own pty — no daemon, no tmux
          // paste-buffer, because mogeung holds this one.
          if (!current) return;
          const id = shellPaneId(current[0]);
          // `\r`, not `\n`: a pty's line discipline is what turns a carriage
          // return into "the user pressed Enter", and it is what xterm sends
          // for that key. `run` is `Alt+Enter` and nothing else — plain Enter
          // still only writes.
          // The command, then — when it carries a `<placeholder>` — the
          // keystrokes that leave the cursor exactly where the placeholder was,
          // with the rest of the line intact. Hunting for `<file>` with arrow
          // keys was the most common friction in using this.
          const keys = run ? `${command}\r` : command + cursorToPlaceholder(command);
          void ptyWrite(id, keys, "command-box").catch(() => {});
          // And the keyboard follows the text. Reported 2026-08-29: focus went
          // back to the attention list, so the command was in the shell and the
          // cursor was not, which is the one place it needed to be.
          useStore.setState({ focusTerminal: { id, nonce: Date.now() } });
        }}
      />

      <div className="min-h-0 flex-1 bg-[var(--bg-panel)]">
        <DockviewReact
          components={components}
          defaultTabComponent={NoTab}
          watermarkComponent={ShellsEmpty}
          // A shell has no business floating over the panel or leaving the
          // window: the centre's popouts exist for sessions (`R-B55`), and a
          // shell in a window of its own is what a terminal app is for.
          disableFloatingGroups
          onReady={(e: DockviewReadyEvent) => setApi(e.api)}
          className="dockview-theme-mogeung h-full"
        />
      </div>
    </div>
  );
}
