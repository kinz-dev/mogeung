/**
 * Stashes: the list, the selected one's files and diff through the same
 * inspector a commit uses, and — since `R-D28` — push, pop and drop.
 * Drop asks first: a dropped stash is only in the reflog, and only for a
 * while. Push takes untracked files along by default, because an agent's
 * new files are exactly the ones in the way.
 */

import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { Button, Checkbox, Dim, Empty, Input, Mono } from "@/ui/primitives";
import { Dialog } from "@/ui/Dialog";
import { ContextMenu, MenuItem, MenuSeparator } from "@/ui/Menu";
import { showStash, stashDrop, stashPop, stashPush } from "@/lib/gitActions";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import { row as rowCls, rowSelected } from "@/ui/styles";
import { CommitInspector } from "./CommitInspector";
import { Splitter, useDragWidth } from "./Dropdown";

export function StashTab({ id }: { id: string }) {
  const stashes = useStore((s) => s.git[id]?.stashes ?? null);
  const label = useStore((s) => s.git[id]?.diffLabel ?? null);
  const changed = useStore((s) => s.git[id]?.status?.filter((e) => e.state !== "!!").length ?? 0);
  const send = useStore((s) => s.send);
  const saved = useStore((s) => s.prefs.gitSidebarWidth);
  const setPrefs = useStore((s) => s.setPrefs);
  const [width, onDrag] = useDragWidth(saved, 220, 900, (px) => setPrefs({ gitSidebarWidth: px }));
  const [message, setMessage] = useState("");
  const [untracked, setUntracked] = useState(true);
  const [dropping, setDropping] = useState<{ index: number; message: string } | null>(null);

  useEffect(() => {
    if (stashes === null) send({ cmd: "git_stashes", session_id: id });
  }, [stashes, id, send]);

  return (
    <div className="flex h-full min-h-0">
      <div className="flex shrink-0 flex-col" style={{ width }}>
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
          <span className="text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">Stashes</span>
          {stashes && <Dim className="text-2xs tabular-nums">{stashes.length}</Dim>}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto py-0.5">
          {!stashes ? (
            <Empty>reading stashes…</Empty>
          ) : stashes.length === 0 ? (
            <Empty hint="stash the working tree below, or from the terminal">nothing stashed</Empty>
          ) : (
            stashes.map((st) => {
              const key = `stash@{${st.index}}`;
              return (
                <ContextMenu
                  key={st.index}
                  trigger={
                    <div
                      role="option"
                      aria-selected={label === key}
                      tabIndex={-1}
                      onClick={() => showStash(id, st.index)}
                      className={cn(rowCls, "flex items-start gap-2 px-2 py-1", label === key && rowSelected)}
                    >
                      <Mono className="shrink-0 text-2xs text-[var(--amber)]">{key}</Mono>
                      <span className="min-w-0 flex-1 text-xs leading-4 break-words">{st.message}</span>
                      <Dim className="shrink-0 text-2xs tabular-nums">{stamp(st.epoch)}</Dim>
                    </div>
                  }
                >
                  <MenuItem onSelect={() => showStash(id, st.index)}>Show</MenuItem>
                  <MenuItem onSelect={() => stashPop(id, st.index)}>Pop — restore it, and drop it from the list</MenuItem>
                  <MenuSeparator />
                  <MenuItem danger onSelect={() => setDropping({ index: st.index, message: st.message })}>
                    Drop…
                  </MenuItem>
                </ContextMenu>
              );
            })
          )}
        </div>
        <div className="flex shrink-0 flex-col gap-1.5 border-t border-[var(--border)] p-2">
          <Input value={message} onChange={setMessage} placeholder="stash message — optional" ariaLabel="stash message" />
          <Checkbox checked={untracked} onChange={setUntracked} label="include untracked files" title="--include-untracked: a plain stash leaves new files behind, and an agent's new files are the ones in the way" />
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              disabled={changed === 0}
              title={changed === 0 ? "nothing to stash — the working tree is clean" : "stash the working tree"}
              onClick={() => {
                stashPush(id, message.trim(), untracked);
                setMessage("");
              }}
            >
              Stash all
            </Button>
            <Dim className="text-2xs">{changed} change{changed === 1 ? "" : "s"}</Dim>
          </div>
        </div>
      </div>
      <Splitter onMouseDown={onDrag} />
      <div className="flex min-w-0 flex-1 flex-col">
        {label?.startsWith("stash@") ? (
          <CommitInspector id={id} onBack={() => {}} />
        ) : (
          <Empty hint="its files show here as a tree, and one file's diff on selection">pick a stash</Empty>
        )}
      </div>
      {dropping && (
        <Dialog title={`Drop stash@{${dropping.index}}?`} subtitle="a dropped stash lives on only in the reflog, and only for a while" onClose={() => setDropping(null)}>
          <div className="flex flex-col gap-3 px-3 py-3">
            <span className="text-xs">{dropping.message}</span>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                className="border-[var(--red)] text-[var(--red)]"
                onClick={() => {
                  stashDrop(id, dropping.index);
                  setDropping(null);
                }}
              >
                Drop it
              </Button>
              <Button variant="outline" onClick={() => setDropping(null)}>
                Keep it
              </Button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
}
