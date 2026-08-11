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
 *
 * **It no longer draws its own header.** `R-B49` merged that row into the
 * dockview tab, which now names the session rather than repeating the word
 * `Agent`, and moved the controls into the group's actions — see
 * [`PaneChrome`](../ui/PaneChrome.tsx). What is left here is the terminal and
 * the four sentences that explain why there sometimes is not one — and which of
 * them is chosen matters more than any of them reads: a session that has
 * **ended** used to be told it was not running under tmux, because both facts
 * arrive as a `tmux_target` of `null`.
 */

import { useMemo } from "react";
import { usePaneBinding, useSelectedSession } from "@/store";
import { Empty } from "@/ui/primitives";
import { TerminalView } from "@/ui/Terminal";
import { attachArgs, reachFor, spawnAs } from "@/lib/tmux";
import { usePaneId } from "@/lib/paneScope";
import { useStore } from "@/store";

export function AgentPane() {
  const s = useSelectedSession();
  const { id, held } = usePaneBinding();
  const paneId = usePaneId() ?? "agent";
  const daemon = useStore((st) => st.daemon);
  const machineId = useStore((st) => st.machineId);

  const reach = useMemo(() => reachFor(daemon, machineId), [daemon, machineId]);

  const command = useMemo(() => {
    if (!s?.tmux_target || !reach) return null;
    return spawnAs(reach, attachArgs(s.tmux_target));
  }, [s?.tmux_target, reach]);

  // A **held** pane with nothing to show is not an empty pane, and saying
  // "select a session" here would be a lie twice over: you did select one, and
  // clicking another will not fill this pane because it is held. The session it
  // was watching has ended, and the way out is to let go — so it says that, and
  // stays where it is rather than quietly adopting whatever is selected now.
  if (!s && held) {
    return (
      <Empty hint={`held on ${id?.slice(0, 8) ?? "a session"} — drop the anchor in the header to follow the queue again`}>
        that session has ended
      </Empty>
    );
  }
  if (!s) return <Empty>select a session</Empty>;

  return (
    // Stays `--bg-panel`: the darker surface belongs to the terminal alone —
    // chrome above, another machine's output below. `TerminalView` carries
    // `--terminal-bg` itself, which also covers the shell panel.
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <div className="min-h-0 flex-1">
        <TerminalView
          // Keyed by **pane and session**, not by session alone. Two panes can
          // legitimately show one session — hold one on it and leave the other
          // following — and a shared pty id would mean two xterms writing to one
          // pty, with the first unmount closing it under the second. tmux is
          // happy to hand the same session to two clients; this is what asks it
          // to. `agent:<sid>` is still what slot one produces, so nothing that
          // existed before this change is renamed.
          id={`${paneId}:${s.id}`}
          command={command}
          refusal={
            // **Dead first, and the order is the whole fix.** A session that has
            // ended has no pane, so `tmux_target` is `null` for it too (see
            // `state.rs`, which clears the target rather than leaving a stale
            // one) — and the tmux branch below therefore used to answer for it,
            // blaming the launcher for a session that simply finished. That
            // reads as a broken pane and sends you to check tmux, which is the
            // wrong place: reported 2026-08-11 against a **held** pane, where
            // it is worst, because the sentence is fixed on screen and no
            // change of selection will replace it.
            //
            // The empty state above catches only the held session that has left
            // the store entirely. This is the same fact one step earlier, while
            // the session is still known and merely over.
            !s.alive ? (
              <div className="max-w-md space-y-2">
                <div className="text-sm font-semibold text-[var(--text-strong)]">
                  That session has ended.
                </div>
                <p className="text-xs text-[var(--dim)]">
                  A pty dies with the process that owned it, so there is nothing left to attach to.
                  Everything it did is still here — the transcript, its diff and the review.
                </p>
                {held && (
                  <p className="text-xs text-[var(--dim)]">
                    This pane is <strong>held</strong> on it, so picking another session will not
                    change it. Drop the anchor in the header to follow the queue again.
                  </p>
                )}
              </div>
            ) : !s.tmux_target ? (
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
