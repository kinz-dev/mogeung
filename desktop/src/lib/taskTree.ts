/**
 * The task list as a tree of groups and sub-tasks. `R-L11`.
 *
 * The daemon sends a flat list carrying `group` and `depth` — the two things
 * markdown already said — and this is the only place they become a shape. Kept
 * out of the component because the interesting decisions are all about **what
 * stays visible**, and a test should be able to state them without rendering.
 *
 * # Three rules, and the third is the one that would go wrong
 *
 * 1. **A group is a folder.** Groups keep document order, and a task above the
 *    first heading has no group rather than a group called nothing.
 * 2. **Depth makes the sub-tasks.** A task's parent is the nearest task above
 *    it, in the same group, with a smaller depth. That is what markdown means
 *    by indenting one under another.
 * 3. **A done task stays visible while an open child needs it.** Hiding the
 *    ticked ones is what makes a checklist a list of what to do — but hiding a
 *    ticked *parent* would orphan its children, which either drops real work
 *    off the screen or floats it at the wrong level. So a task survives the
 *    filter if anything under it survived.
 *
 * Ticking a parent deliberately does **not** tick its children. Each line is
 * its own checkbox in the document, and a cascade would write lines the user
 * did not tick — ADR-0015 says the document is the truth, and inventing edits
 * to it is the one thing this panel must not do.
 */

import type { Note, Task } from "@/wire/types";
import { noteTitle } from "@/lib/notes";

export interface TaskNode {
  task: Task;
  /** Its own nesting, as drawn — not the document's raw depth. See `treeOf`. */
  depth: number;
  children: TaskNode[];
}

export interface TaskGroup {
  /** The heading, or `null` for the tasks above the first one. */
  name: string | null;
  nodes: TaskNode[];
  /** How many tasks in this group are still open, at any depth. */
  open: number;
  /** How many there are in total, at any depth. */
  total: number;
}

/** A stable key for a task, which has no id of its own by design. */
export function keyOf(t: Task): string {
  return `${t.note_id}:${t.ord}`;
}

/**
 * Nest a flat, document-ordered list by `depth`.
 *
 * **Redepthed as it nests.** A document may jump from depth 0 to depth 2 —
 * markdown lets you over-indent — and drawing that as two levels of blank
 * indentation is a gap nobody wrote. The tree keeps the *relationship* and
 * renumbers the levels, so a child is always exactly one deeper than the
 * parent it is under.
 */
function treeOf(tasks: Task[]): TaskNode[] {
  const roots: TaskNode[] = [];
  // Each entry is an ancestor still open for children, with its raw depth.
  const stack: { raw: number; node: TaskNode }[] = [];

  for (const task of tasks) {
    const raw = task.depth ?? 0;
    while (stack.length > 0 && raw <= stack[stack.length - 1].raw) stack.pop();
    const parent = stack[stack.length - 1];
    const node: TaskNode = { task, depth: parent ? parent.node.depth + 1 : 0, children: [] };
    if (parent) parent.node.children.push(node);
    else roots.push(node);
    stack.push({ raw, node });
  }
  return roots;
}

/** Split a document-ordered list into its headings, keeping that order. */
export function groupsOf(tasks: readonly Task[]): TaskGroup[] {
  const out: TaskGroup[] = [];
  for (const t of tasks) {
    const name = t.group ?? null;
    const last = out[out.length - 1];
    // Contiguous runs, not a map: a heading used twice in one document is two
    // sections, and merging them would move tasks up the page.
    if (last && last.name === name) last.total += 1;
    else out.push({ name, nodes: [], open: 0, total: 1 });
  }

  let at = 0;
  for (const g of out) {
    const slice = tasks.slice(at, at + g.total);
    at += g.total;
    g.nodes = treeOf(slice as Task[]);
    g.open = slice.filter((t) => !t.done).length;
  }
  return out;
}

/** One row to draw: a node, and whether its children are hidden. */
export interface TaskRow {
  node: TaskNode;
  collapsed: boolean;
}

/**
 * Flatten a group's tree into the rows on screen.
 *
 * Flat rather than nested because the panel is a list with indentation, and a
 * flat row list is what lets a keyboard walk it one visible row at a time.
 */
export function rowsOf(
  nodes: readonly TaskNode[],
  opts: {
    showDone: boolean;
    collapsed: ReadonlySet<string>;
    /** Keys that stay on screen done or not — a row just ticked, kept a
     *  moment so a slip can be unticked. `R-L13`. */
    keep?: ReadonlySet<string>;
  },
): TaskRow[] {
  const out: TaskRow[] = [];

  const visible = (n: TaskNode): boolean =>
    (!n.task.done || opts.showDone || opts.keep?.has(keyOf(n.task)) === true) || n.children.some(visible);

  const walk = (list: readonly TaskNode[]) => {
    for (const n of list) {
      if (!visible(n)) continue;
      const collapsed = opts.collapsed.has(keyOf(n.task));
      out.push({ node: n, collapsed });
      if (!collapsed) walk(n.children);
    }
  };
  walk(nodes);
  return out;
}

/** Whether anything under this node is still open — for the parent's count. */
export function openUnder(n: TaskNode): number {
  return (n.task.done ? 0 : 1) + n.children.reduce((sum, c) => sum + openUnder(c), 0);
}

/** How many tasks this node covers, itself included. */
export function totalUnder(n: TaskNode): number {
  return 1 + n.children.reduce((sum, c) => sum + totalUnder(c), 0);
}

/**
 * A document as a folder: its title, the note behind it, and its headings as
 * sections inside it. `R-L12`.
 *
 * Until 2026-10-21 the panel grouped by heading across every document at
 * once, in the daemon's `note_id` order — so a headingless note's tasks drew
 * flush under the previous note's last heading, two notes whose adjacent
 * sections shared a name merged, and the order was one nobody could predict.
 * With the owner's own two notes, *Open this panel with Alt+T* read as a
 * Mogeung task. The document is the unit now: two notes never merge, whatever
 * their headings say.
 */
export interface TaskFolder {
  noteId: string;
  /** `null` when the note has gone and only its history keeps the tasks. */
  note: Note | null;
  title: string;
  /** The headings inside the note, in document order; `name: null` first
   *  when tasks sit above the first heading. */
  sections: TaskGroup[];
  open: number;
  done: number;
  total: number;
}

/**
 * Fold a flat task list into folders, one per document, in the order the
 * notes were **created**, newest first.
 *
 * Creation rather than the Notes list's *updated* order, on purpose: a tick
 * rewrites the note and bumps `updated`, and a folder that jumped to the top
 * every time you ticked inside it would be the panel moving under your hand.
 * A note the list no longer holds sorts last, under a name that says so.
 */
export function foldersOf(tasks: readonly Task[], notes: readonly Note[]): TaskFolder[] {
  const byNote = new Map<string, Task[]>();
  for (const t of tasks) {
    const list = byNote.get(t.note_id);
    if (list) list.push(t);
    else byNote.set(t.note_id, [t]);
  }
  const created = new Map(notes.map((n) => [n.id, n]));
  const out: TaskFolder[] = [];
  for (const [noteId, list] of byNote) {
    const note = created.get(noteId) ?? null;
    const done = list.filter((t) => t.done).length;
    out.push({
      noteId,
      note,
      title: note ? noteTitle(note.body) || "untitled" : "a note that was deleted",
      sections: groupsOf(list),
      open: list.length - done,
      done,
      total: list.length,
    });
  }
  out.sort((a, b) => {
    if (!a.note || !b.note) return a.note ? -1 : b.note ? 1 : a.noteId.localeCompare(b.noteId);
    return b.note.created - a.note.created || a.noteId.localeCompare(b.noteId);
  });
  return out;
}

