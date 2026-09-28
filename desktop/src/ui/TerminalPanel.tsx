/**
 * Your own shells, across the bottom. `R-B31`, `R-B33`.
 *
 * A panel rather than a pane, and that was learnt the hard way: `R-B31` shipped
 * it as a pane and the next day's use said the pane was wrong — it followed the
 * selection, vanished when the selection moved, and could not exist at all
 * before a session did, which is exactly when you want a shell to start one.
 *
 * Each tab is `tmux new-session -A`, so it is the **same** shell across
 * restarts: a build still running from an hour ago is still there, and closing
 * the window detaches instead of killing. A `claude` started in one of these is
 * a session mogeung observes like any other rather than one it owns —
 * [ADR-0011](../../docs/decisions/0011-own-a-shell-never-an-agent.md).
 *
 * **The shells are tiles since `R-B56`**, asked 2026-09-28: *"I want to change
 * it to dockable panel like those we have in the AI session at the top"*. One
 * shell at a time behind a tab strip meant a build and its log could not be
 * watched together; now the panel holds its own dockview, so shells tab, split
 * and rearrange by drag exactly as the Agent panes do — and only among
 * themselves, because a second dockview refuses a drop from the first. See
 * [`shellDock.ts`](../lib/shellDock.ts) for which store owns what.
 */


import * as React from "react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  DockviewReact,
  type DockviewApi,
  type DockviewReadyEvent,
  type IDockviewPanelHeaderProps,
  type IDockviewPanelProps,
} from "dockview";
import { Plus, X, ChevronDown } from "lucide-react";
import { useStore, useSelectedSession } from "@/store";
import { Chip, Dim, IconButton } from "@/ui/primitives";
import { TerminalView } from "@/ui/Terminal";
import { CommandBox } from "@/ui/CommandBox";
import { cursorToPlaceholder } from "@/lib/command";
import { ptyWrite } from "@/lib/tauri";
import { hostLabel, reachFor, shellArgs, shellSessionName, spawnAs, type Reach } from "@/lib/tmux";
import { base } from "@/lib/format";
import { useChord } from "@/lib/keymap";
import { PaneScope } from "@/lib/paneScope";
import {
  restoreShellLayout,
  shellLayoutKey,
  shellOfPane,
  shellPaneId,
  syncShells,
  type Shell,
} from "@/lib/shellDock";

const MIN_HEIGHT = 120;

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
 * Removes the *tab*, not the session. tmux keeps it; adding a tab with the
 * same name attaches straight back.
 */
function closeShell(name: string) {
  const { scoped, setScoped } = useStore.getState();
  setScoped({ shells: scoped().shells.filter(([n]) => n !== name) });
}

/** One shell, as a tile. */
function ShellPane(props: IDockviewPanelProps) {
  // A background tab holds no pty, the same rule as a background Agent tab
  // (`R-J58`) — and the same cost the strip had, which mounted only the shell
  // in front.
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
        <TerminalView id={shellPaneId(shell[0])} command={spawnAs(reach, shellArgs(shell[0], shell[1]))} />
      </div>
    </PaneScope>
  );
}

/** A shell's tab: its folder, and a close that detaches rather than kills. */
function ShellTab(props: IDockviewPanelHeaderProps) {
  const name = shellOfPane(props.api.id);
  const shell = useShell(name);
  const reach = useReach(shell);
  if (!name) return null;
  const where = reach ? hostLabel(reach) : null;
  const root = shell?.[1] ?? "";
  return (
    <div
      className="dv-default-tab group"
      title={
        where
          ? `${name} — a tmux session on ${where}. It survives mogeung closing, and \`tmux attach -t ${name}\` reaches it from any terminal there.`
          : `${name} — a tmux session in ${root}. It survives mogeung closing, and \`tmux attach -t ${name}\` reaches it from any terminal.`
      }
    >
      <span className="dv-default-tab-content">{shell ? shellTitle(shell) : name}</span>
      <button
        type="button"
        aria-label={`close ${name}`}
        title="close the tab — the tmux session keeps running"
        // Not a drag: dockview starts one on pointerdown anywhere in the tab.
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          closeShell(name);
        }}
        className="ml-1 shrink-0 opacity-0 outline-none transition-opacity duration-[var(--dur-fast)] group-hover:opacity-60 hover:!opacity-100 focus-visible:opacity-100 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--ring)]"
      >
        <X className="h-3 w-3" />
      </button>
    </div>
  );
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
  const setScoped = useStore((s) => s.setScoped);
  const machineId = useStore((s) => s.machineId);
  const layoutKey = useStore((s) => shellLayoutKey(s.daemon?.machine_id));
  const session = useSelectedSession();
  const [height, setHeight] = useState(260);
  const [api, setApi] = useState<DockviewApi | null>(null);
  const [active, setActive] = useState<string | null>(null);
  /** The machine whose layout the tree holds — restored once per machine. */
  const restoredFor = useRef<string | null>(null);
  /** A shell just added, to bring forward once its panel exists. */
  const reveal = useRef<string | null>(null);
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
    if (reveal.current) {
      api.getPanel(shellPaneId(reveal.current))?.api.setActive();
      reveal.current = null;
    }
    setActive(api.activePanel ? shellOfPane(api.activePanel.id) : null);
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
    return () => {
      onLayout.dispose();
      onActive.dispose();
      if (pending !== null) {
        window.clearTimeout(pending);
        save();
      }
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

  const addShell = () => {
    // Rooted in the selected session's worktree when there is one, else home.
    // `-c` is read only on creation, so an existing shell keeps wherever you
    // left it — the behaviour every terminal has.
    const root = session?.repo_root ?? session?.cwd ?? "";
    // A session on another machine (`R-I22`) has its worktree *there*, so the
    // shell goes there too — or, with no way to reach it, opens here at home
    // rather than `cd`-ing into a path this filesystem does not have.
    const o = session?.origin;
    const away = o && o.machine_id !== machineId;
    // Names are reused across clients on purpose: the egui window's `mog-0` and
    // this one's are two views of one tmux session, not two shells.
    const used = new Set(shells.map(([name]) => name));
    let n = 0;
    while (used.has(shellSessionName(n))) n += 1;
    const name = shellSessionName(n);
    const entry: Shell = !away ? [name, root] : o.ssh_target ? [name, root, o.ssh_target] : [name, ""];
    // It joins the group you are in, as a tab — drag it to an edge to split.
    reveal.current = name;
    setScoped({ shells: [...shells, entry] });
  };

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
          defaultTabComponent={ShellTab}
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
