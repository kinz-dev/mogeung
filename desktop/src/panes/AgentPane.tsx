/**
 * The session's own terminal, attached through tmux. `R-B18`.
 *
 * **Attached, never spawned.** This pane runs `tmux attach`; tmux already owns
 * the conversation and hands over a view of it. A `claude` started in iTerm2 is
 * owned by iTerm2 and cannot be attached to at all, which is what
 * `scripts/yolomo` exists to fix — and why a session without a `tmux_target`
 * gets a sentence explaining that rather than a broken pane.
 *
 * Closing this pane detaches. The agent keeps working. See
 * [ADR-0010](../../docs/decisions/0010-attach-a-terminal-never-own-one.md).
 */

import { useMemo } from "react";
import { useStore, useSelectedSession } from "@/store";
import { Chip, Dim, Empty, PaneHeader } from "@/ui/primitives";
import { TerminalView } from "@/ui/Terminal";
import { attachArgs, hostLabel, reachFor, spawnAs } from "@/lib/tmux";

export function AgentPane() {
  const s = useSelectedSession();
  const daemon = useStore((st) => st.daemon);
  const machineId = useStore((st) => st.machineId);
  const send = useStore((st) => st.send);

  const reach = useMemo(() => reachFor(daemon, machineId), [daemon, machineId]);

  const command = useMemo(() => {
    if (!s?.tmux_target || !reach) return null;
    return spawnAs(reach, attachArgs(s.tmux_target));
  }, [s?.tmux_target, reach]);

  if (!s) return <Empty>select a session</Empty>;

  const host = reach ? hostLabel(reach) : null;

  return (
    // Stays `--bg-panel`: `PaneHeader` paints no surface of its own, so this is
    // the header's background too, and the darker one belongs to the terminal
    // alone — chrome above, another machine's output below. `TerminalView`
    // carries `--terminal-bg` itself, which also covers the shell panel.
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <PaneHeader title="Agent" hint="attached through tmux — closing this pane detaches, it never kills">
        {s.tmux_target && <Dim className="font-mono text-2xs">{s.tmux_target}</Dim>}
        {host && (
          <Chip color="var(--amber)" title="tmux runs on that machine, reached over ssh">
            {host}
          </Chip>
        )}
        <button
          type="button"
          onClick={() => send({ cmd: "focus_terminal", session_id: s.id })}
          className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] rounded-sm border border-[var(--border)] px-1.5 py-px text-2xs hover:border-[var(--border-hover)]"
          title="raise the terminal application this session runs in — it moves your window, then you type"
        >
          raise its window
        </button>
      </PaneHeader>

      <div className="min-h-0 flex-1">
        <TerminalView
          id={`agent:${s.id}`}
          command={command}
          refusal={
            !s.tmux_target ? (
              <div className="max-w-md space-y-2">
                <div className="text-sm font-semibold text-[var(--text-strong)]">
                  This session is not running under tmux.
                </div>
                <p className="text-xs text-[var(--dim)]">
                  A terminal owns the pty of whatever it started, and nothing else can attach to it.
                  mogeung can point you at this session, but cannot host it.
                </p>
                <p className="text-xs text-[var(--dim)]">
                  Start sessions with <code className="font-mono">yolomo</code> instead of{" "}
                  <code className="font-mono">yolo</code> and this pane becomes live.
                </p>
              </div>
            ) : (
              <div className="max-w-md space-y-2">
                <div className="text-sm font-semibold text-[var(--text-strong)]">
                  That daemon is on another machine, and has not been told how to reach it.
                </div>
                <p className="text-xs text-[var(--dim)]">
                  tmux owns this session over there, so the pane would have to run{" "}
                  <code className="font-mono">ssh -t … tmux …</code>. Set{" "}
                  <code className="font-mono">ssh_target</code> in that daemon's config and it will
                  attach. A guess would open a shell on the wrong filesystem, so it refuses instead.
                </p>
              </div>
            )
          }
        />
      </div>
    </div>
  );
}
