/**
 * Start a session — in your terminal, not in mogeung. `R-B2`.
 *
 * This is the one place mogeung causes an agent to exist, and the shape is what
 * keeps it inside [ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md):
 * the daemon opens **a real terminal window** in a directory and nothing more.
 * The conversation is not wrapped, not proxied and not readable except the way
 * every other session is — through the files Claude Code writes. Closing mogeung
 * leaves it running, exactly like a session you started yourself.
 *
 * The `--dangerously-skip-permissions` warning is stated up front rather than
 * left to be discovered. The flag is the agent's own and the daemon only passes
 * it, but it changes what pressing this costs, and a permission prompt that
 * never comes is not something to meet by surprise.
 */

import { useMemo, useState } from "react";
import { Rocket } from "lucide-react";
import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { Button, Checkbox, Dim, Input, Mono, Row } from "@/ui/primitives";

export function LaunchWindow() {
  const open = useStore((s) => s.showLaunch);
  const sessions = useStore((s) => s.sessions);
  const send = useStore((s) => s.send);
  const [dir, setDir] = useState("");
  const [worktree, setWorktree] = useState(false);

  // Somewhere you have already worked is nearly always where you want to work
  // again, and typing a path is the slowest part of this window.
  const repos = useMemo(() => {
    const seen = new Set<string>();
    for (const s of Object.values(sessions)) if (s.repo_root) seen.add(s.repo_root);
    return [...seen].sort();
  }, [sessions]);

  if (!open) return null;
  const close = () => useStore.setState({ showLaunch: false });

  const go = () => {
    if (!dir.trim()) return;
    send({ cmd: "launch_terminal", dir: dir.trim(), worktree });
    close();
  };

  return (
    <Dialog title="New session" subtitle="opens a real interactive claude in your terminal" onClose={close}>
      <div className="min-w-[30rem]">
        <Dim className="block text-2xs">
          mogeung does not wrap the conversation — you drive it exactly as usual, and it shows
          up in the queue like any other session.
        </Dim>
        <div className="mt-1 text-2xs text-[var(--amber)]">
          Started in yolo mode: <Mono>--dangerously-skip-permissions</Mono>. The agent will not
          ask before editing files or running commands.
        </div>

        <div className="mt-2">
          <Dim className="mb-0.5 block text-2xs">directory</Dim>
          <Input value={dir} mono onChange={setDir} placeholder="~/projects/foo" onKeyDown={(e) => e.key === "Enter" && go()} />
        </div>

        {repos.length > 0 && (
          <>
            <Dim className="mt-2 mb-0.5 block text-2xs">recent repos</Dim>
            <div className="max-h-40 overflow-y-auto">
              {repos.map((r) => (
                <Row key={r} selected={dir === r} onClick={() => setDir(r)} className="px-1 py-0.5">
                  <Mono className="truncate text-2xs">{r}</Mono>
                </Row>
              ))}
            </div>
          </>
        )}

        <div className="mt-2">
          <Checkbox
            checked={worktree}
            onChange={setWorktree}
            label="in a fresh git worktree"
            title="a new branch mogeung/<timestamp> in its own checkout, so this session cannot collide with the one already running there"
          />
        </div>

        <div className="mt-3 flex items-center gap-2">
          <Button variant="solid" onClick={go} disabled={!dir.trim()}>
            <Rocket size={11} /> open a terminal
          </Button>
          <Dim className="text-2xs">
            it opens where you are watching — the daemon's machine, not necessarily this one
          </Dim>
        </div>
      </div>
    </Dialog>
  );
}
