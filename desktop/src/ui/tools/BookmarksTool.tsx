/**
 * Every marked turn, in one list, summarised. `R-B35`.
 *
 * A note with an empty body **is** a bookmark — marking a turn and writing
 * about it are one gesture with two depths rather than two features, which is
 * why this reads the same `notes` the scratchpad does rather than a second
 * store. What it adds is the view the transcript cannot give you: all the marks
 * across every session at once, newest first, each with the turn it points at.
 */

import { useMemo } from "react";
import { X } from "lucide-react";
import { useStore } from "@/store";
import { Chip, Dim, Empty, Row } from "@/ui/primitives";
import { oneLine, stamp } from "@/lib/format";
import { eventText, repoName, sessionLabel } from "@/wire/types";

export function BookmarksTool() {
  const notes = useStore((s) => s.notes);
  const sessions = useStore((s) => s.sessions);
  const events = useStore((s) => s.events);
  const select = useStore((s) => s.select);
  const send = useStore((s) => s.send);
  const pushError = useStore((s) => s.pushError);
  const selected = useStore((s) => s.selected);

  // Anchored notes only. A free-standing document belongs to the scratchpad;
  // this list is about places in conversations.
  const rows = useMemo(
    () =>
      notes
        .filter((n) => n.session_id && n.seq !== null && n.seq !== undefined)
        .slice()
        .sort((a, b) => b.updated - a.updated),
    [notes],
  );

  if (rows.length === 0) {
    return (
      <Empty hint="mark a turn in the Transcript and it appears here, with or without a note">
        no marked turns
      </Empty>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      {rows.map((n) => {
        const session = n.session_id ? sessions[n.session_id] : null;
        const ev = n.session_id ? events[n.session_id]?.find((e) => e.seq === n.seq) : undefined;
        const remark = n.body.trim();
        // A bookmark with no words still has to say what it points at, so the
        // turn's own text stands in until you write one.
        const turnText = ev ? eventText(ev.kind) : "";
        return (
          <Row
            key={n.id}
            selected={selected === n.session_id}
            onClick={() => {
              if (!n.session_id) return;
              if (!sessions[n.session_id]) {
                // Say so rather than swallowing the click. A bookmark outlives
                // the session it marks (ADR-0015) — that is the point of it —
                // so "nothing happened" is the wrong answer to a row that is
                // still on screen.
                pushError(
                  `that session is no longer being watched — the bookmark is kept, but there is no transcript to open`,
                );
                return;
              }
              select(n.session_id);
              // By **seq**, not timestamp. A session this window has never
              // opened has no loaded events to take a timestamp from, so the
              // old jump silently did nothing; a seq stays pending until the
              // events arrive and then lands exactly on the marked turn.
              useStore.setState({ focusSeq: n.seq ?? null, highlightSeq: n.seq ?? null });
            }}
            className="border-b border-[var(--border)] py-1"
            title={session ? sessionLabel(session) : "this session is no longer being watched"}
          >
            <div className="group flex items-center gap-1.5">
              {n.body.trim() ? (
                <Chip color="var(--purple)">note</Chip>
              ) : (
                <Chip color="var(--dim)">mark</Chip>
              )}
              <Dim className="truncate text-2xs">
                {session ? repoName(session) : (n.repo ?? "unknown repo")} · turn {n.seq}
              </Dim>
              <Dim className="ml-auto shrink-0 text-2xs">{stamp(n.updated)}</Dim>
              <button
                type="button"
                title="remove this bookmark"
                onClick={(e) => {
                  // The row is the trigger for jumping, so this must not also
                  // navigate on its way to deleting.
                  e.stopPropagation();
                  send({ cmd: "note_delete", id: n.id });
                }}
                className="shrink-0 text-[var(--dim)] opacity-0 outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] group-hover:opacity-100 hover:text-[var(--red)] focus-visible:opacity-100 focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
              >
                <X size={10} />
              </button>
            </div>
            {/*
              The remark leads, and the turn it marks follows underneath. That
              order is the whole point of a remark: "egress vs ingress" is what
              you are scanning for, and the turn's own words are how you confirm
              you found it.
            */}
            {remark ? (
              <>
                <div className="mt-0.5 truncate text-xs font-semibold text-[var(--text-strong)]">
                  {remark}
                </div>
                {turnText && <Dim className="mt-px block truncate text-2xs">{oneLine(turnText, 140)}</Dim>}
              </>
            ) : (
              <div className="mt-0.5 text-xs text-[var(--text)]">
                {turnText ? (
                  oneLine(turnText, 180)
                ) : (
                  <Dim>that turn is not loaded — open the session to read it</Dim>
                )}
              </div>
            )}
          </Row>
        );
      })}
    </div>
  );
}
