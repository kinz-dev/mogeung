/**
 * The bottom rule: what is selected, and the metadata that used to sit under
 * the pane header — three rows of reference material between you and the diff.
 */

import { Folder } from "lucide-react";
import { useStore, useSelectedSession } from "@/store";
import { Dim } from "@/ui/primitives";
import { NoticeButton } from "@/ui/Notices";
import { compact, dirTail, fmtDur, secsSince } from "@/lib/format";
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
          <Dim title="tokens in / out. Tokens here, never dollars — cost lives on the Analytics view (ADR-0024).">
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
          {/*
            **The folder, which used to sit beside the tabs.** `R-J48`, asked
            2026-08-25. It arrived there in `R-J24`'s week as a left header
            action, drawn straight after the tab so it read as part of it — and
            that is exactly what went wrong as the centre filled up: a path is
            the widest thing in the window and it was taking its width from the
            tabs, which are the part you navigate by. Down here it competes with
            nothing.

            **Framed rather than dimmed**, and the frame is the ask: everything
            else on this row is grey running text, so a second grey phrase
            beside *"4m since last event"* would read as more of the same
            sentence. A mono path in a bordered chip is a different *kind* of
            thing at a glance — a place, not a measurement — before you have
            read either.

            `cwd`, not `repo_root`, unchanged from the header: the question is
            where you ran the CLI, and the two differ exactly when a session was
            started in a subdirectory. The whole path is on hover, and the
            visible part is shortened from the front because the tail of a path
            is the half that identifies it.

            **64 characters, asked for 2026-08-25**, against `dirTail`'s default
            of 34 — which was chosen for a pane header competing with the tabs,
            and this row has the width to spare. The CSS ceiling is stated in
            `ch` rather than `rem` so the two agree by construction: at 10px
            mono a character is not 6px on every machine, and a `max-w` guessed
            in `rem` either clips a path `dirTail` thought would fit or leaves
            a gap it shortened for nothing.

            **The selected session's**, where the header showed the *active
            pane's*. In practice these agree — activating a held pane selects
            its session (see `App`) — and where they could differ, this row has
            already answered `sessionLabel`, `repoName` and the branch for the
            selection, so a folder from somewhere else would be the one lie on
            it.
          */}
          {s.cwd && (
            <span
              className="flex min-w-0 shrink-0 items-center gap-1 rounded-sm border border-[var(--border)] bg-[var(--bg-faint)] px-1.5 py-px font-mono text-2xs text-[var(--text)]"
              title={`started in ${s.cwd}`}
            >
              <Folder className="h-3 w-3 shrink-0 text-[var(--dim)]" />
              <span className="max-w-[64ch] truncate">{dirTail(s.cwd, 64)}</span>
            </span>
          )}
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
