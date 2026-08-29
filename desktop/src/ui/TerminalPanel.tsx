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
 */

import { useMemo, useState } from "react";
import { Plus, X, ChevronDown } from "lucide-react";
import { useStore, useSelectedSession } from "@/store";
import { Chip, Dim, IconButton } from "@/ui/primitives";
import { TerminalView } from "@/ui/Terminal";
import { CommandBox } from "@/ui/CommandBox";
import { ptyWrite } from "@/lib/tauri";
import { hostLabel, reachFor, shellArgs, shellSessionName, spawnAs } from "@/lib/tmux";
import { cn } from "@/lib/cn";
import { base } from "@/lib/format";
import { useChord } from "@/lib/keymap";

const MIN_HEIGHT = 120;

export function TerminalPanel() {
  const open = useStore((s) => s.showTerminal);
  const scoped = useStore((s) => s.scoped());
  const setScoped = useStore((s) => s.setScoped);
  const daemon = useStore((s) => s.daemon);
  const machineId = useStore((s) => s.machineId);
  const session = useSelectedSession();
  const [active, setActive] = useState(0);
  const [height, setHeight] = useState(260);

  const reach = useMemo(() => reachFor(daemon, machineId), [daemon, machineId]);
  const askChord = useChord("terminal.command_box");
  const shells = scoped.shells;

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

  if (!open) return null;

  const addShell = () => {
    // Rooted in the selected session's worktree when there is one, else home.
    // `-c` is read only on creation, so an existing shell keeps wherever you
    // left it — the behaviour every terminal has.
    const root = session?.repo_root ?? session?.cwd ?? "";
    // Names are reused across clients on purpose: the egui window's `mog-0` and
    // this one's are two views of one tmux session, not two shells.
    const used = new Set(shells.map(([name]) => name));
    let n = 0;
    while (used.has(shellSessionName(n))) n += 1;
    setScoped({ shells: [...shells, [shellSessionName(n), root]] });
    setActive(shells.length);
  };

  const closeShell = (i: number) => {
    // Removes the *tab*, not the session. tmux keeps it; adding a tab with the
    // same name attaches straight back.
    setScoped({ shells: shells.filter((_, j) => j !== i) });
    setActive((a) => Math.max(0, Math.min(a, shells.length - 2)));
  };

  const current = shells[active];
  const host = reach ? hostLabel(reach) : null;

  return (
    <div className="flex shrink-0 flex-col border-t border-[var(--border)]" style={{ height }}>
      <div onMouseDown={onDrag} className="h-1 cursor-row-resize hover:bg-[var(--blue)]" title="drag to resize" />

      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-[var(--border)] px-1">
        {shells.map(([name, root], i) => (
          <div
            key={name}
            onClick={() => setActive(i)}
            className={cn(
              "group flex cursor-default items-center gap-1 rounded-sm px-2 py-0.5 text-xs",
              active === i
                ? "bg-[var(--bg-faint)] text-[var(--text-strong)]"
                : "text-[var(--dim)] hover:bg-[var(--bg-faint)]",
            )}
            title={
              host
                ? `${name} — a tmux session on ${host}. It survives mogeung closing, and \`tmux attach -t ${name}\` reaches it from any terminal there.`
                : `${name} — a tmux session in ${root}. It survives mogeung closing, and \`tmux attach -t ${name}\` reaches it from any terminal.`
            }
          >
            <span>{root ? base(root) : name}</span>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                closeShell(i);
              }}
              className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] opacity-0 group-hover:opacity-100 hover:text-[var(--text-strong)]"
              title="close the tab — the tmux session keeps running"
            >
              <X size={10} />
            </button>
          </div>
        ))}
        <IconButton title="a new shell, rooted in the selected session's worktree" onClick={addShell}>
          <Plus size={13} />
        </IconButton>
        {host && (
          <Chip color="var(--amber)" title="these shells run on that machine, over ssh">
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
        refused the version that drew *into* the line.
      */}
      <CommandBox
        repo={current ? current[1] : null}
        onAccept={(command) => {
          // Straight into this window's own pty — no daemon, no tmux
          // paste-buffer, because mogeung holds this one. And **no Enter**:
          // ADR-0003's amendment one level down.
          if (current) void ptyWrite(`shell:${current[0]}`, command, "command-box").catch(() => {});
        }}
      />

      <div className="min-h-0 flex-1 bg-[var(--bg-panel)]">
        {!current ? (
          <div className="flex h-full flex-col items-center justify-center gap-1">
            <Dim className="text-xs">no shells open</Dim>
            <Dim className="text-2xs opacity-70">
              a shell here is yours — mogeung never starts an agent in one
            </Dim>
          </div>
        ) : !reach ? (
          <div className="flex h-full items-center justify-center px-6 text-center">
            <Dim className="text-xs">
              that daemon is on another machine and has no <code className="font-mono">ssh_target</code> —
              a shell here would open on the wrong filesystem
            </Dim>
          </div>
        ) : (
          <TerminalView
            key={current[0]}
            id={`shell:${current[0]}`}
            command={spawnAs(reach, shellArgs(current[0], current[1]))}
          />
        )}
      </div>
    </div>
  );
}
