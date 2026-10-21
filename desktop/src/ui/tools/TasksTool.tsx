/**
 * Every checkbox in every document, in one list. `R-L3`, ADR-0015 — redrawn
 * 2026-10-21 as feature 0052 (`R-L12`–`R-L17`) after a critique scored it
 * 13/40.
 *
 * **This is a view, and the document is the thing.** A task is a `- [ ]` line
 * and nothing else — no id, no due date, no assignee, because none of those can
 * be written on the line. Ticking a box here sends `task_set`, which rewrites
 * the markdown and lets the daemon re-derive; the box flips at once and goes
 * back if the daemon refuses, but there is no path that marks a row and leaves
 * the document alone, and that absence is the whole of ADR-0015's defence
 * against two sources of truth.
 *
 * **The document is the folder** (`R-L12`). Each note with tasks is a folder,
 * its headings are sections inside it, and two notes never merge. Until the
 * redesign the panel grouped by heading across every note at once, so a
 * headingless note's tasks drew under the previous note's last heading.
 *
 * **Three targets on a row, none of them the row** (`R-L13`). The tick is a
 * checkbox you can hit, the twisty folds, and *open in Notes* is its own
 * control on hover and focus. Clicking the words does nothing: a near miss on
 * the tick used to open the note, and the editor then showed a stale draft with
 * *save* lit.
 *
 * **Ticking a parent does not tick its children.** Each line is its own
 * checkbox in the document, and a cascade would write lines you never ticked.
 *
 * **The right-click menu** (`R-L18`, asked 2026-10-21: *"how can I delete a
 * task? how can I navigate to the source where I define the task"*) holds the
 * actions a row has: open in Notes, mark done or open, copy the words, and
 * delete — which turns the row into its own confirmation, the way the scratch
 * panel's delete does, and takes the nested sub-tasks with the line because a
 * nested checkbox left behind would attach to whatever is above it.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, NotebookPen, Plus } from "lucide-react";
import type { Task } from "@/wire/types";
import { sessionLabel } from "@/wire/types";
import { foldersOf, keyOf, openUnder, rowsOf, totalUnder, type TaskFolder, type TaskGroup, type TaskRow } from "@/lib/taskTree";
import { addTask, deleteTask, destinationFor, openNote, type Target } from "@/lib/notes";
import { repoName } from "@/lib/gitSync";
import { copyText } from "@/lib/gitActions";
import { useStore } from "@/store";
import { Button, Dim, IconButton, Segmented } from "@/ui/primitives";
import { ContextMenu, MenuItem, MenuSeparator } from "@/ui/Menu";
import { cn } from "@/lib/cn";
import { interactive, motion } from "@/ui/styles";

type Scope = "all" | "session";

/** `HH:MM`, local, for a closure today. */
function hhmm(secs: number): string {
  return new Date(secs * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

const twisty = "grid h-6 w-6 shrink-0 place-items-center rounded-sm text-[var(--dim)] hover:text-[var(--text)]";

/** How long a row you just ticked stays on screen before it folds away. */
const KEEP_MS = 3000;

export function TasksTool() {
  const tasksRaw = useStore((s) => s.tasks);
  const tasksLoaded = useStore((s) => s.tasksLoaded);
  const pendingTicks = useStore((s) => s.pendingTicks);
  const pendingAdd = useStore((s) => s.pendingAdd);
  const justAdded = useStore((s) => s.justAdded);
  const closedToday = useStore((s) => s.closedToday);
  const closed = useStore((s) => s.closed);
  const notes = useStore((s) => s.notes);
  const sessions = useStore((s) => s.sessions);
  const selected = useStore((s) => s.selected);
  const focusRail = useStore((s) => s.focusRail);
  const send = useStore((s) => s.send);
  const tickTask = useStore((s) => s.tickTask);

  const [scope, setScope] = useState<Scope>("all");
  const [showDone, setShowDone] = useState(false);
  const [showClosed, setShowClosed] = useState(false);
  const [draft, setDraft] = useState("");
  const [target, setTarget] = useState<Target | null>(null);
  // The task whose row is asking "delete?" — one at a time, in place.
  const [asking, setAsking] = useState<string | null>(null);
  const box = useRef<HTMLInputElement>(null);
  // One set for folders, sections and tasks, keyed differently. They never
  // collide and they behave identically, so two pieces of state would be two
  // things to keep in step for no gain.
  const [shut, setShut] = useState<ReadonlySet<string>>(() => new Set());
  const toggleShut = (key: string) =>
    setShut((prev) => {
      const next = new Set(prev);
      if (!next.delete(key)) next.add(key);
      return next;
    });

  // A row you just ticked stays on screen, checked and struck, for a moment:
  // long enough to see it land and to untick a slip. The old panel made the
  // row vanish on the round trip, which read as the panel eating your click.
  const [kept, setKept] = useState<ReadonlySet<string>>(() => new Set());
  const keep = (key: string) => {
    setKept((prev) => new Set(prev).add(key));
    setTimeout(
      () =>
        setKept((prev) => {
          const next = new Set(prev);
          next.delete(key);
          return next;
        }),
      KEEP_MS,
    );
  };

  // Asked for on mount because a document may have been edited by another
  // window — or the daemon restarted and rebuilt the table — while this panel
  // was shut.
  useEffect(() => {
    send({ cmd: "task_list" });
  }, [send]);

  /**
   * Opening the panel puts the cursor in the box. `R-J82`'s device, the way
   * `ChatTool` uses it: only when the toggle says *you just asked for this*,
   * never on a mount that merely restores a layout.
   */
  useEffect(() => {
    if (focusRail !== "tasks") return;
    box.current?.focus();
    useStore.setState({ focusRail: null });
  }, [focusRail]);

  // Optimistic ticks are drawn over the daemon's list, never written to it.
  const tasks = useMemo(
    () =>
      Object.keys(pendingTicks).length === 0
        ? tasksRaw
        : tasksRaw.map((t) => {
            const d = pendingTicks[keyOf(t)];
            return d === undefined ? t : { ...t, done: d };
          }),
    [tasksRaw, pendingTicks],
  );

  const session = selected ? sessions[selected] : undefined;
  const repo = session?.repo_root ?? null;
  const folders = useMemo(() => foldersOf(tasks, notes), [tasks, notes]);
  const inScope = useMemo(
    () =>
      scope === "session" && selected
        ? folders.filter((f) => f.note && ((repo && f.note.repo === repo) || f.note.session_id === selected))
        : folders,
    [folders, scope, selected, repo],
  );
  const hidden = folders.length - inScope.length;
  const open = inScope.reduce((n, f) => n + f.open, 0);
  const done = inScope.reduce((n, f) => n + f.done, 0);

  const closedAt = useMemo(() => {
    const m = new Map<string, number>();
    for (const c of closed) m.set(`${c.note_id}\u001f${c.text}`, c.at);
    return m;
  }, [closed]);
  const noteName = (id: string) => folders.find((f) => f.noteId === id)?.title ?? "a note that was deleted";

  const dest = useMemo(
    () => destinationFor({ notes, sessions, selected, target }),
    [notes, sessions, selected, target],
  );

  // The box empties when the broadcast carries the task, and keeps its text
  // when the add came back refused: the text is the one thing not to lose.
  const wasPending = useRef(false);
  useEffect(() => {
    if (wasPending.current && !pendingAdd) {
      if (justAdded) setDraft("");
      box.current?.focus();
    }
    wasPending.current = !!pendingAdd;
  }, [pendingAdd, justAdded]);

  // The new row, brought into view. `scrollIntoView` is optional: jsdom has none.
  useEffect(() => {
    if (!justAdded) return;
    const el = document.querySelector<HTMLElement>(`[data-task-key="${CSS.escape(justAdded)}"]`);
    el?.scrollIntoView?.({ block: "nearest" });
  }, [justAdded]);

  const add = () => {
    if (pendingAdd) return;
    addTask(draft, dest);
  };

  const folderKey = (f: TaskFolder) => `doc:${f.noteId}`;
  const sectionKey = (f: TaskFolder, i: number) => `sec:${f.noteId}:${i}`;

  /** The n-th section with these words inside this folder — the address an add needs. */
  const occurrenceOf = (f: TaskFolder, i: number) =>
    f.sections.slice(0, i).filter((s) => s.name === f.sections[i].name).length;

  const isTarget = (noteId: string, section: string | null, occurrence: number) =>
    target?.noteId === noteId &&
    (section === null ? target.section === null : target.section?.name === section && target.section.occurrence === occurrence);

  const pick = (f: TaskFolder, i: number | null) => {
    const section = i === null ? null : { name: f.sections[i].name as string, occurrence: occurrenceOf(f, i) };
    setTarget(isTarget(f.noteId, section?.name ?? null, section?.occurrence ?? 0) ? null : { noteId: f.noteId, title: f.title, section });
  };

  const taskRow = (row: TaskRow, base: number) => {
    const t: Task = row.node.task;
    const key = keyOf(t);
    const kids = row.node.children.length > 0;
    const under = totalUnder(row.node) - 1;
    const at = t.done ? closedAt.get(`${t.note_id}\u001f${t.text}`) : undefined;
    const tick = () => {
      keep(key);
      tickTask(t.note_id, t.ord, !t.done);
    };

    if (asking === key) {
      return (
        <div
          key={key}
          data-task-key={key}
          className="flex min-h-7 items-center gap-2 rounded-sm bg-[var(--state-focus)] py-0.5 pr-1"
          style={{ paddingLeft: `${base + row.node.depth * 16 + 8}px` }}
        >
          <span className="min-w-0 flex-1 truncate text-sm">
            delete “{t.text || "an empty checkbox"}”
            {under > 0 && (
              <Dim className="ml-1 text-xs">
                and {under} sub-task{under === 1 ? "" : "s"}
              </Dim>
            )}
            ?
          </span>
          <Button
            variant="outline"
            onClick={() => {
              deleteTask(t.note_id, t.ord);
              setAsking(null);
            }}
          >
            delete
          </Button>
          <Button variant="outline" onClick={() => setAsking(null)}>
            keep
          </Button>
        </div>
      );
    }

    const body = (
      <div
        data-task-key={key}
        className={cn(
          "group flex min-h-7 items-start gap-0.5 rounded-sm pr-1 hover:bg-[var(--state-hover)]",
          motion,
          justAdded === key && "bg-[var(--state-focus)]",
        )}
        style={{ paddingLeft: `${base + row.node.depth * 16}px` }}
      >
        {kids ? (
          <button
            type="button"
            aria-expanded={!row.collapsed}
            aria-label={`${row.collapsed ? "unfold" : "fold"} the sub-tasks of ${t.text}`}
            className={cn(twisty, interactive)}
            onClick={() => toggleShut(key)}
          >
            {row.collapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
          </button>
        ) : (
          // A leaf keeps the twisty's width, or every level of the tree is
          // ragged down its left edge.
          <span className="w-6 shrink-0" />
        )}
        <button
          type="button"
          role="checkbox"
          aria-checked={t.done}
          aria-label={`${t.done ? "mark open" : "mark done"}: ${t.text || "an empty checkbox"}`}
          className={cn("grid h-6 w-6 shrink-0 place-items-center rounded-sm", interactive)}
          onClick={tick}
        >
          <span
            className={cn(
              "grid h-3.5 w-3.5 place-items-center rounded-sm border-[1.5px]",
              motion,
              t.done ? "border-[var(--blue)] bg-[var(--blue)] text-[var(--on-accent)]" : "border-[var(--dim)]",
            )}
          >
            {t.done && <Check size={10} strokeWidth={3} />}
          </span>
        </button>
        <div className={cn("min-w-0 flex-1 py-1 text-sm", t.done && "text-[var(--dim)] line-through")}>
          {t.text || <Dim>an empty checkbox</Dim>}
          {/* What a folded parent is hiding, so shutting one never hides a
              count you needed. */}
          {kids && row.collapsed && (
            <Dim className="ml-1 text-xs no-underline">
              {" "}
              · {row.node.children.reduce((n, c) => n + openUnder(c), 0)} open under
            </Dim>
          )}
        </div>
        {at !== undefined && <Dim className="shrink-0 py-1 text-xs">{hhmm(at)}</Dim>}
        <IconButton
          title="open in Notes"
          onClick={() => openNote(t.note_id, t.ord)}
          className="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100"
        >
          <NotebookPen size={13} />
        </IconButton>
      </div>
    );

    return (
      <ContextMenu key={key} trigger={body}>
        <>
          <MenuItem onSelect={() => openNote(t.note_id, t.ord)}>Open in Notes</MenuItem>
          <MenuItem onSelect={tick}>{t.done ? "Mark open" : "Mark done"}</MenuItem>
          <MenuSeparator />
          <MenuItem onSelect={() => copyText(t.text)}>Copy task text</MenuItem>
          <MenuSeparator />
          <MenuItem danger onSelect={() => setAsking(key)}>
            {under > 0 ? `Delete with ${under} sub-task${under === 1 ? "" : "s"}…` : "Delete…"}
          </MenuItem>
        </>
      </ContextMenu>
    );
  };

  const sectionBlock = (f: TaskFolder, g: TaskGroup, i: number) => {
    const rows = rowsOf(g.nodes, { showDone, collapsed: shut, keep: kept });
    if (g.name === null) {
      // Tasks above the first heading sit directly under the document — a
      // heading is a thing you wrote, and *not filed* is not filed under nothing.
      return <div key={sectionKey(f, i)}>{rows.map((r) => taskRow(r, 20))}</div>;
    }
    const key = sectionKey(f, i);
    const folded = shut.has(key);
    const sectionDone = g.total - g.open;
    const chosen = isTarget(f.noteId, g.name, occurrenceOf(f, i));
    const header = (
        <div className="flex items-center gap-0.5 pr-2 pl-5">
          <button
            type="button"
            aria-expanded={!folded}
            aria-label={`${folded ? "unfold" : "fold"} ${g.name}`}
            className={cn(twisty, interactive)}
            onClick={() => toggleShut(key)}
          >
            {folded ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
          </button>
          <button
            type="button"
            aria-pressed={chosen}
            title={chosen ? "new tasks go here — click to go back to the default" : "click, and new tasks go into this section"}
            className={cn("min-w-0 truncate rounded-sm px-1 text-xs font-semibold", interactive, chosen && "bg-[var(--state-focus)] text-[var(--text-strong)]")}
            onClick={() => pick(f, i)}
          >
            {g.name}
          </button>
          <Dim className="ml-auto shrink-0 pl-1 text-xs whitespace-nowrap">
            {g.open === 0 ? `all ${g.total} done` : `${g.open} open${showDone && sectionDone > 0 ? ` · ${sectionDone} done` : ""}`}
          </Dim>
        </div>
    );
    const firstOrd = g.nodes[0]?.task.ord;
    return (
      <div key={key}>
        <ContextMenu trigger={header}>
          <>
            <MenuItem onSelect={() => openNote(f.noteId, firstOrd)}>Open in Notes</MenuItem>
            <MenuItem onSelect={() => pick(f, i)}>{chosen ? "Stop sending new tasks here" : "New tasks go here"}</MenuItem>
          </>
        </ContextMenu>
        {!folded && rows.map((r) => taskRow(r, 36))}
      </div>
    );
  };

  const folderBlock = (f: TaskFolder) => {
    const key = folderKey(f);
    const folded = shut.has(key);
    const chosen = isTarget(f.noteId, null, 0);
    const owner = f.note?.session_id ? sessions[f.note.session_id] : undefined;
    const tag = f.note?.repo ? repoName(f.note.repo) : null;
    const header = (
        <div className="flex items-center gap-1 pr-2 pl-1">
          <button
            type="button"
            aria-expanded={!folded}
            aria-label={`${folded ? "unfold" : "fold"} ${f.title}`}
            className={cn(twisty, interactive)}
            onClick={() => toggleShut(key)}
          >
            {folded ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
          </button>
          <button
            type="button"
            aria-pressed={chosen}
            title={chosen ? "new tasks go here — click to go back to the default" : "click, and new tasks go into this note"}
            className={cn(
              "min-w-0 truncate rounded-sm px-1 text-sm font-semibold text-[var(--text-strong)]",
              interactive,
              chosen && "bg-[var(--state-focus)]",
            )}
            onClick={() => pick(f, null)}
          >
            {f.title}
          </button>
          {tag && (
            <span
              className="shrink-0 rounded-sm bg-[var(--tag-blue-bg)] px-1 text-xs text-[var(--text-strong)]"
              title={owner ? `tagged with ${sessionLabel(owner)}` : `tagged with the repository ${f.note?.repo}`}
            >
              {tag}
            </span>
          )}
          <Dim className="ml-auto shrink-0 pl-1 text-xs whitespace-nowrap">
            {f.open === 0 ? `all ${f.total} done` : `${f.open} open${showDone && f.done > 0 ? ` · ${f.done} done` : ""}`}
          </Dim>
        </div>
    );
    return (
      <div key={key} className="pt-1.5">
        <ContextMenu trigger={header}>
          <>
            <MenuItem onSelect={() => openNote(f.noteId)}>Open note in Notes</MenuItem>
            <MenuItem onSelect={() => pick(f, null)}>{chosen ? "Stop sending new tasks here" : "New tasks go here"}</MenuItem>
            <MenuSeparator />
            <MenuItem onSelect={() => copyText(f.title)}>Copy note title</MenuItem>
          </>
        </ContextMenu>
        {!folded && f.sections.map((g, i) => sectionBlock(f, g, i))}
      </div>
    );
  };

  const adder = (
    <div className="shrink-0 px-2 pt-1.5">
      <div
        className={cn(
          "flex h-7 items-center gap-1.5 rounded-sm border border-[var(--border)] bg-[var(--bg)] pr-1 pl-1.5",
          motion,
          "hover:border-[var(--border-hover)] focus-within:border-[var(--ring)]",
        )}
      >
        <Plus size={14} className="shrink-0 text-[var(--dim)]" aria-hidden="true" />
        <input
          ref={box}
          type="text"
          value={draft}
          disabled={!!pendingAdd}
          placeholder="add a task…"
          aria-label="add a task"
          spellCheck={false}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") add();
            if (e.key === "Escape") setDraft("");
          }}
          className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-[var(--dim)] disabled:opacity-60"
        />
        <button
          type="button"
          aria-label={`where a new task goes: ${dest.label}`}
          title={target ? `new tasks go into ${dest.label} — click to go back to the default` : `new tasks go into ${dest.label}`}
          className={cn(
            "max-w-[46%] shrink-0 truncate rounded-sm border border-[var(--border)] px-1.5 text-xs leading-[18px] whitespace-nowrap",
            interactive,
            target ? "text-[var(--text-strong)]" : "text-[var(--dim)]",
          )}
          onClick={() => setTarget(null)}
        >
          → {dest.label}
        </button>
      </div>
    </div>
  );

  const filter = (
    <div className="flex shrink-0 items-center gap-2 px-2 py-1.5">
      <Segmented<Scope>
        size="md"
        value={scope === "session" && !selected ? "all" : scope}
        onChange={setScope}
        options={[
          { value: "all", label: "all" },
          {
            value: "session",
            label: "this session",
            disabled: !selected,
            title: selected
              ? `only the notes tagged with ${repo ? repoName(repo) : "this session"}`
              : "select a session first",
          },
        ]}
      />
      {scope === "session" && selected && repo && <Dim className="text-xs">repo {repoName(repo)}</Dim>}
    </div>
  );

  if (!tasksLoaded) {
    return <div className="flex min-h-0 flex-1 flex-col">{adder}</div>;
  }

  if (tasksRaw.length === 0 && !pendingAdd) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        {adder}
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
          <div className="text-base font-semibold text-[var(--text-strong)]">No tasks yet</div>
          <Dim className="max-w-[220px] text-sm">
            Type above and press Enter. It goes into a note called <span className="text-[var(--text)]">{dest.label}</span>.
          </Dim>
          <Dim className="max-w-[220px] text-sm">
            Or write <code className="rounded-sm bg-[var(--bg-panel)] px-1 font-mono text-xs text-[var(--text)]">- [ ]</code> on a
            line of any note.
          </Dim>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {adder}
      {filter}
      <div className="min-h-0 flex-1 overflow-y-auto pb-1">
        {inScope.map(folderBlock)}
        {hidden > 0 && (
          <Dim className="block px-3 pt-3 text-xs">
            {hidden} {hidden === 1 ? "note" : "notes"} without this session&apos;s tag {hidden === 1 ? "is" : "are"} hidden —{" "}
            <button type="button" className={cn("rounded-sm underline decoration-dotted underline-offset-2", interactive)} onClick={() => setScope("all")}>
              show all
            </button>
          </Dim>
        )}
      </div>

      {showClosed && (
        <div className="max-h-40 shrink-0 overflow-y-auto border-t border-[var(--border)] px-2 py-1" aria-label="closed today">
          {closed.length === 0 ? (
            <Dim className="text-xs">nothing closed today</Dim>
          ) : (
            closed.map((c) => (
              <div key={`${c.note_id}:${c.text}:${c.at}`} className="flex items-baseline gap-2 py-0.5">
                <span className="min-w-0 flex-1 truncate text-sm">{c.text}</span>
                <Dim className="shrink-0 text-xs">
                  {noteName(c.note_id)} · {hhmm(c.at)}
                </Dim>
              </div>
            ))
          )}
        </div>
      )}

      {/*
        One line of counts, and the done toggle lives in it: *N done* shows
        the ticked rows in place rather than in a list of their own. *Closed
        today* counts closures rather than closed tasks — unticking something
        does not undo having done it — and opens the list with the times.
      */}
      <div role="status" className="flex shrink-0 items-center gap-1.5 border-t border-[var(--border)] px-2 py-1 text-xs text-[var(--dim)]">
        <span>{open} open</span>
        <span aria-hidden="true">·</span>
        <button
          type="button"
          aria-pressed={showDone}
          title={showDone ? "hide the done tasks" : "show the done tasks in place"}
          className={cn(
            "rounded-sm border border-[var(--border)] px-1.5 leading-[18px]",
            interactive,
            showDone && "bg-[var(--state-focus)] text-[var(--text-strong)]",
          )}
          onClick={() => setShowDone((v) => !v)}
        >
          {done} done
        </button>
        <span aria-hidden="true">·</span>
        {closedToday === 0 ? (
          <span>nothing closed today</span>
        ) : (
          <button
            type="button"
            aria-expanded={showClosed}
            className={cn("rounded-sm px-0.5 underline decoration-dotted underline-offset-2", interactive)}
            onClick={() => setShowClosed((v) => !v)}
          >
            {closedToday} closed today
          </button>
        )}
      </div>
    </div>
  );
}
