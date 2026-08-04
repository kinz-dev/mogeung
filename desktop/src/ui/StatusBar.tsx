/**
 * The bottom rule: what is selected, and the metadata that used to sit under
 * the pane header — three rows of reference material between you and the diff.
 */

import { useStore, useSelectedSession } from "@/store";
import { Dim } from "@/ui/primitives";
import { NoticeButton } from "@/ui/Notices";
import { compact, fmtDur, secsSince } from "@/lib/format";
import { repoName, sessionLabel } from "@/wire/types";

export function StatusBar() {
  const s = useSelectedSession();
  const health = useStore((st) => st.health);
  const change = useStore((st) => (st.selected ? st.changes[st.selected] : null));

  const unread =
    change && change.files.length > 0
      ? change.files.reduce((n, f) => n + f.hunks.filter((h) => !h.reviewed).length, 0)
      : 0;

  return (
    <div className="flex h-6 shrink-0 items-center gap-3 border-t border-[var(--border)] px-2.5 text-xs">
      {s ? (
        <>
          <span className="truncate text-[var(--text)]">{sessionLabel(s)}</span>
          <Dim>{repoName(s)}</Dim>
          {s.git_branch && <Dim>{s.git_branch}</Dim>}
          <Dim>
            {s.turns} turns · {s.tool_calls} tools
          </Dim>
          <Dim title="tokens in / out. Tokens, never dollars — ADR-0005.">
            {compact(s.tokens_in)} in · {compact(s.tokens_out)} out
          </Dim>
          {s.files_changed > 0 && (
            <span>
              <span className="text-[var(--add-fg)]">+{s.insertions}</span>{" "}
              <span className="text-[var(--del-fg)]">−{s.deletions}</span>{" "}
              <Dim>in {s.files_changed} file(s)</Dim>
            </span>
          )}
          {unread > 0 && <span className="text-[var(--blue)]">{unread} unread hunk(s)</span>}
          <Dim className="ml-auto">{fmtDur(secsSince(s.last_event_at))} since last event</Dim>
        </>
      ) : (
        <Dim>no session selected</Dim>
      )}
      <NoticeButton />
      {health && (
        <Dim className={s ? "" : "ml-auto"} title={health.alerts.map((a) => a.kind).join("\n")}>
          {health.alerts.length > 0 ? (
            <span className="text-[var(--amber)]">{health.alerts.length} thing(s) it cannot see</span>
          ) : (
            <>reading everything it recognises</>
          )}
        </Dim>
      )}
    </div>
  );
}
