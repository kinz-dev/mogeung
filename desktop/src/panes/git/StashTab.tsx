/**
 * Stashes: the list, and the selected one's files and diff through the same
 * inspector a commit uses. `R-D26`. Pop and drop are `R-D28`.
 */

import { useEffect } from "react";
import { useStore } from "@/store";
import { Dim, Empty, Mono } from "@/ui/primitives";
import { showStash } from "@/lib/gitActions";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import { row as rowCls, rowSelected } from "@/ui/styles";
import { CommitInspector } from "./CommitInspector";
import { Splitter, useDragWidth } from "./Dropdown";

export function StashTab({ id }: { id: string }) {
  const stashes = useStore((s) => s.git[id]?.stashes ?? null);
  const label = useStore((s) => s.git[id]?.diffLabel ?? null);
  const send = useStore((s) => s.send);
  const saved = useStore((s) => s.prefs.gitSidebarWidth);
  const setPrefs = useStore((s) => s.setPrefs);
  const [width, onDrag] = useDragWidth(saved, 220, 900, (px) => setPrefs({ gitSidebarWidth: px }));

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
            <Empty hint="git stash from the terminal puts one here — pushing from this window is R-D28">nothing stashed</Empty>
          ) : (
            stashes.map((st) => {
              const key = `stash@{${st.index}}`;
              return (
                <div
                  key={st.index}
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
              );
            })
          )}
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
    </div>
  );
}
