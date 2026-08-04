/**
 * How much of this repo's agent output nobody has read. `R-D8`.
 *
 * Across sessions, not within one — which is the number that actually decides
 * whether a repository is being reviewed or merely accumulating.
 */

import { useEffect } from "react";
import { useStore, useSelectedSession } from "@/store";
import { Dim, Empty, Mono, PaneHeader, Row } from "@/ui/primitives";
import { openFile } from "@/lib/explorer";

export function DebtPane() {
  const s = useSelectedSession();
  const debt = useStore((st) => st.debt);
  const send = useStore((st) => st.send);
  const repo = s?.repo_root ?? null;

  useEffect(() => {
    if (repo) send({ cmd: "fetch_review_debt", repo });
  }, [repo, send]);

  if (!s) return <Empty>select a session</Empty>;
  if (!repo) return <Empty hint="review debt is counted per repository">this session is not in a git repo</Empty>;
  if (!debt || debt.repo !== repo) return <Empty>counting…</Empty>;

  const pct = debt.hunks_total === 0 ? 100 : Math.round((debt.hunks_read / debt.hunks_total) * 100);

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <PaneHeader title="Debt" />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="px-3 py-2">
          <div className="text-base text-[var(--text-strong)]">
            {debt.hunks_total === 0
              ? "nothing outstanding"
              : `${debt.hunks_total - debt.hunks_read} of ${debt.hunks_total} hunks unread across ${debt.sessions} session(s)`}
          </div>
          <div className="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-[var(--bg-faint)]">
            <div className="h-full bg-[var(--blue)]" style={{ width: `${pct}%` }} />
          </div>
          <Dim className="mt-1 block text-xs">
            {debt.files_touched} file(s) touched · {debt.sessions_unread} session(s) with unread work ·{" "}
            {debt.unread_insertions} unread insertion(s)
          </Dim>
        </div>

        {debt.worst_files.length > 0 && (
          <>
            <div className="px-3 pt-2 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
              Worst first
            </div>
            {debt.worst_files.map((f) => (
              <Row
                key={`${f.session_id}:${f.path}`}
                onClick={() => openFile(s.id, f.path, { pin: true })}
                className="flex items-center gap-2 border-b border-[var(--border)] py-1"
                title="open in the Code pane"
              >
                <Mono className="flex-1 truncate text-sm text-[var(--blue)]">{f.path}</Mono>
                <Dim className="text-2xs">{f.unread_hunks} unread</Dim>
                <span className="text-2xs text-[var(--amber)]">{f.score}</span>
              </Row>
            ))}
          </>
        )}
      </div>
    </div>
  );
}
