/**
 * One file of one commit's diff, as a pane in the centre. `R-D30`.
 *
 * The Git tool window lives in the dock and the dock has a height; a diff
 * worth a long read can leave it here, keeping the read marks because this
 * is the same `DiffList`, and leave the window under `R-B55` because a pane
 * is a pane. The id carries session, revision and path (`lib/panes.ts`);
 * the files come from the store's small cache of revision diffs, filled by
 * every commit or range diff that arrives, and are asked for once when the
 * cache does not have them — a pane opened from the inspector never has to
 * ask.
 */

import { useEffect, useMemo, useRef } from "react";
import { useStore } from "@/store";
import { usePaneId } from "@/lib/paneScope";
import { parseDiffPaneId } from "@/lib/panes";
import { Dim, Empty, Mono } from "@/ui/primitives";
import { DiffList } from "@/ui/DiffView";

export function DiffPane() {
  const paneId = usePaneId();
  const ref = paneId ? parseDiffPaneId(paneId) : null;
  const key = ref ? `${ref.session}:${ref.rev}` : "";
  const files = useStore((s) => (key ? s.revDiffs[key] : undefined));
  const send = useStore((s) => s.send);
  const asked = useRef(false);

  useEffect(() => {
    if (!ref || files || asked.current) return;
    asked.current = true;
    const range = ref.rev.split("..");
    if (range.length === 2) send({ cmd: "git_diff_range", session_id: ref.session, from: range[0], to: range[1] });
    else send({ cmd: "git_show", session_id: ref.session, sha: ref.rev });
  }, [ref, files, send]);

  const shown = useMemo(() => {
    if (!files) return [];
    return ref && ref.path !== "*" ? files.filter((f) => f.path === ref.path) : files;
  }, [files, ref]);

  if (!ref) return <Empty>no diff</Empty>;
  if (!files) return <Empty hint="asked once; the answer lands here">reading {ref.rev.slice(0, 12)}…</Empty>;
  const hunks = shown.reduce((n, f) => n + f.hunks.length, 0);
  const read = shown.reduce((n, f) => n + f.hunks.filter((h) => h.reviewed).length, 0);
  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <div className="flex h-6 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2 text-2xs">
        <Mono className="truncate text-[var(--text)]" title={ref.path}>
          {ref.path === "*" ? "all files" : ref.path}
        </Mono>
        <Dim className="shrink-0">{ref.rev.includes("..") ? `comparing ${ref.rev}` : ref.rev.slice(0, 12)}</Dim>
        <Dim className="ml-auto shrink-0" title="hunks read, shared with every view that shows them (R-D17)">
          {hunks === 0 ? "no hunks" : `${read}/${hunks} read`}
        </Dim>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {shown.length === 0 ? (
          <Empty hint="a binary file, a change git shows no text for, or a path this revision does not touch">nothing to show</Empty>
        ) : (
          <DiffList files={shown} sessionId={ref.session} />
        )}
      </div>
    </div>
  );
}
