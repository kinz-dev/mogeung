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
 * **Nesting and grouping are the document's, not the panel's** (`R-L10`). A
 * task indented under another is drawn indented; a task under a markdown
 * heading is drawn under that heading. Neither needed new syntax, because
 * markdown already has both — which is the same reason a task is a checkbox
 * rather than a record.
 *
 * **Open first, then what you closed today.** The done half is collapsed into a
 * count rather than a list, because a checklist that keeps its corpses at eye
 * level stops being a list of what to do. The count is the one thing the
 * markdown cannot answer — a checkbox has no memory of *when* — which is why
 * the derived table exists at all, and it is therefore the thing worth putting
 * on screen rather than a second column of ticks.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import type { Task } from "@/wire/types";
import { CheckSquare, Square } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Empty, Input, Row, SectionLabel } from "@/ui/primitives";
import { TASKS_DOC, addTask, noteTitle, openNote } from "@/lib/notes";

export function TasksTool() {
  const tasks = useStore((s) => s.tasks);
  const closedToday = useStore((s) => s.closedToday);
  const notes = useStore((s) => s.notes);
  const send = useStore((s) => s.send);
  const [showDone, setShowDone] = useState(false);
  const [draft, setDraft] = useState("");
  const box = useRef<HTMLInputElement>(null);

  // Asked for on mount because a document may have been edited by another
  // window — or the daemon restarted and rebuilt the table — while this panel
  // was shut.
  useEffect(() => {
    send({ cmd: "task_list" });
  }, [send]);

  const open = useMemo(() => tasks.filter((t) => !t.done), [tasks]);
  const done = useMemo(() => tasks.filter((t) => t.done), [tasks]);

  /**
   * Split a list into its headings, keeping document order. `R-L10`.
   *
   * Ordered by first appearance rather than alphabetically, and ungrouped
   * tasks stay where they are rather than being swept into an *(other)* bucket
   * — a heading is a thing you wrote, and a task you did not file under one is
   * not filed under "nothing", it is simply above the first heading.
   */
  const inGroups = (list: Task[]): [string | null, Task[]][] => {
    const out: [string | null, Task[]][] = [];
    for (const t of list) {
      const key = t.group ?? null;
      const last = out[out.length - 1];
      if (last && last[0] === key) last[1].push(t);
      else out.push([key, [t]]);
    }
    return out;
  };

  /** The document a task lives in, for the line under it. */
  const noteName = (id: string) => {
    const note = notes.find((n) => n.id === id);
    return note ? noteTitle(note.body) || "untitled" : "a document that has gone";
  };

  const toggle = (note_id: string, ord: number, done: boolean) =>
    send({ cmd: "task_set", note_id, ord, done });

  /**
   * The one thing this panel could not do until 2026-09-10, and the reason it
   * read as broken: a list of checkboxes with no way to write one.
   *
   * It appends to a document rather than inventing a task, because ADR-0015
   * says there is no task outside a `- [ ]` line — so *adding* one is writing a
   * line, and the derived table follows as it does for any other edit.
   */
  const add = () => {
    addTask(draft);
    setDraft("");
    box.current?.focus();
  };

  const adder = (
    <div className="shrink-0 border-b border-[var(--border)] px-2 py-1">
      <Input
        inputRef={box}
        value={draft}
        placeholder={`add a task to ${TASKS_DOC}…`}
        ariaLabel="add a task"
        onChange={setDraft}
        onKeyDown={(e) => e.key === "Enter" && add()}
      />
    </div>
  );

  const row = (t: Task) => (
    <Row
      key={`${t.note_id}:${t.ord}`}
      className="flex items-start gap-2 py-1 pr-2"
      // Indented by the document's own nesting. `R-L10`.
      style={{ paddingLeft: `${(t.depth ?? 0) * 14 + 8}px` }}
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
      <div className="flex min-h-0 flex-1 flex-col">
        {adder}
        <Empty hint={`type above, or write \`- [ ] something\` in any note — a task is a checkbox in a document and nothing else, so anything you add lands in a document called ${TASKS_DOC}`}>
          no tasks yet
        </Empty>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {adder}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {open.length === 0 ? (
          <Empty hint="everything with a box is ticked">nothing open</Empty>
        ) : (
          inGroups(open).map(([group, rows], i) => (
            <div key={`${group ?? ""}:${i}`}>
              {group !== null && (
                <div className="px-2 pt-2 pb-0.5">
                  <SectionLabel>{group}</SectionLabel>
                </div>
              )}
              {rows.map(row)}
            </div>
          ))
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
