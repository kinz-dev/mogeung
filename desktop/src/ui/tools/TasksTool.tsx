/**
 * Every checkbox in every document, in one list. `R-L3`, ADR-0015.
 *
 * **This is a view, and the document is the thing.** A task is a `- [ ]` line
 * and nothing else — no id, no due date, no assignee, because none of those can
 * be written on the line. Ticking a box here sends `task_set`, which rewrites
 * the markdown and lets the daemon re-derive; there is no path that marks a row
 * and leaves the document alone, and that absence is the whole of ADR-0015's
 * defence against two sources of truth.
 *
 * **Open first, then what you closed today.** The done half is collapsed into a
 * count rather than a list, because a checklist that keeps its corpses at eye
 * level stops being a list of what to do. The count is the one thing the
 * markdown cannot answer — a checkbox has no memory of *when* — which is why
 * the derived table exists at all, and it is therefore the thing worth putting
 * on screen rather than a second column of ticks.
 */

import { useEffect, useMemo, useState } from "react";
import { CheckSquare, Square } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Empty, Row, SectionLabel } from "@/ui/primitives";
import { noteTitle, openNote } from "@/lib/notes";

export function TasksTool() {
  const tasks = useStore((s) => s.tasks);
  const closedToday = useStore((s) => s.closedToday);
  const notes = useStore((s) => s.notes);
  const send = useStore((s) => s.send);
  const [showDone, setShowDone] = useState(false);

  // Asked for on mount because a document may have been edited by another
  // window — or the daemon restarted and rebuilt the table — while this panel
  // was shut.
  useEffect(() => {
    send({ cmd: "task_list" });
  }, [send]);

  const open = useMemo(() => tasks.filter((t) => !t.done), [tasks]);
  const done = useMemo(() => tasks.filter((t) => t.done), [tasks]);

  /** The document a task lives in, for the line under it. */
  const noteName = (id: string) => {
    const note = notes.find((n) => n.id === id);
    return note ? noteTitle(note.body) || "untitled" : "a document that has gone";
  };

  const toggle = (note_id: string, ord: number, done: boolean) =>
    send({ cmd: "task_set", note_id, ord, done });

  const row = (t: { note_id: string; ord: number; text: string; done: boolean }) => (
    <Row
      key={`${t.note_id}:${t.ord}`}
      className="flex items-start gap-2 px-2 py-1"
      // The row opens the document; the box ticks. Two targets, because
      // "where did I write this" and "I have done it" are different questions
      // and one of them must not be reachable only by the other.
      onClick={() => openNote(t.note_id)}
    >
      <button
        type="button"
        aria-label={t.done ? `reopen ${t.text}` : `close ${t.text}`}
        className="mt-0.5 shrink-0 rounded-sm text-[var(--dim)] outline-none hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-[var(--ring)]"
        onClick={(e) => {
          e.stopPropagation();
          toggle(t.note_id, t.ord, !t.done);
        }}
      >
        {t.done ? <CheckSquare size={12} /> : <Square size={12} />}
      </button>
      <div className="min-w-0 flex-1">
        <div className={`text-xs ${t.done ? "text-[var(--dim)] line-through" : ""}`}>
          {t.text || <Dim className="text-2xs">an empty checkbox</Dim>}
        </div>
        <Dim className="block truncate text-2xs">{noteName(t.note_id)}</Dim>
      </div>
    </Row>
  );

  if (tasks.length === 0) {
    return (
      <Empty hint="write `- [ ] something` in a note and it appears here — a task is a checkbox in a document and nothing else">
        no tasks
      </Empty>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto">
        {open.length === 0 ? (
          <Empty hint="everything with a box is ticked">nothing open</Empty>
        ) : (
          open.map(row)
        )}

        {done.length > 0 && (
          <>
            <button
              type="button"
              className="flex w-full items-center gap-1 px-2 py-1 text-left outline-none hover:bg-[var(--bg-hover)] focus-visible:outline-2 focus-visible:outline-[var(--ring)]"
              onClick={() => setShowDone((v) => !v)}
            >
              <SectionLabel>
                {showDone ? "hide" : "show"} {done.length} done
              </SectionLabel>
            </button>
            {showDone && done.map(row)}
          </>
        )}
      </div>

      {/*
        The count, not a list. This is the only thing here the markdown cannot
        say — a ticked box does not remember *when* — so it is the one number
        worth the strip it costs. It counts **closures** rather than closed
        tasks: unticking something does not undo having done it.
      */}
      <div className="shrink-0 border-t border-[var(--border)] px-2 py-1">
        <Dim className="text-2xs">
          {closedToday === 0
            ? "nothing closed today"
            : `${closedToday} closed today`}
        </Dim>
      </div>
    </div>
  );
}
