/**
 * Turning a conversation into something you keep. `R-L2`, `R-L3`.
 *
 * A transcript belongs to a session and dies with it — forget the session and
 * the reading is gone. A note is the opposite: the `notes` table carries
 * `session_id` and `seq` as **tags rather than parents** (no foreign key, no
 * cascade, stated in `store.rs`), so a note survives the thing it was written
 * against. That asymmetry is the whole feature. Copying a turn into a note is
 * how you move something across that line, deliberately, at the moment you
 * realise it matters — which is exactly when a bookmark is not enough.
 *
 * Everything here is a pure string function. The window sends the result as an
 * ordinary `note_save`; the daemon has no idea a conversation was involved,
 * and nothing new was added to the wire for this.
 */

import type { Note, Session, TranscriptEvent } from "@/wire/types";
import { repoName } from "@/lib/gitSync";
import { sessionLabel } from "@/wire/types";
import { useStore } from "@/store";

/** How a turn is labelled in a note. Short, because it prefixes every block. */
function speaker(ev: TranscriptEvent): string {
  switch (ev.kind.t) {
    case "user_prompt":
      return "You";
    case "assistant_text":
      return "Agent";
    case "thinking":
      return "Agent (thinking)";
    case "tool_use":
      return `Tool · ${ev.kind.name}`;
    case "tool_result":
      return ev.kind.is_error ? "Tool result (error)" : "Tool result";
    case "result":
      return "Result";
    case "notice":
      return "Notice";
    case "init":
      return "Session start";
  }
}

/** The text a turn actually carries, or `null` for the ones that carry none. */
export function textOf(ev: TranscriptEvent): string | null {
  switch (ev.kind.t) {
    case "user_prompt":
    case "assistant_text":
    case "thinking":
      return ev.kind.text;
    case "tool_use":
      return ev.kind.summary;
    case "tool_result":
      return ev.kind.preview;
    case "result":
      return ev.kind.text;
    case "notice":
      return ev.kind.message;
    case "init":
      // Deliberately nothing: `init` is metadata, and a note that opens with
      // "model: …, tool_count: 42" buries the reason you copied anything.
      return null;
  }
}

/**
 * An ISO-ish stamp that sorts and reads. Local, because a note is personal.
 *
 * Takes both spellings the window has: a transcript event's `ts` is an ISO
 * string off the wire, and `Date.now()` is epoch milliseconds.
 */
function when(ts: string | number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * The line between where a note's own words end and the copied ones begin.
 *
 * A rule rather than a wrapper, and that is the whole design here. The first
 * pass quoted every line with `> `, to keep the copied text from colliding with
 * the note's structure — and it bought that safety by destroying the thing the
 * copy was for. **A blockquote does not render as what was copied**: a table
 * becomes a table inside a quote, a fenced block becomes a quoted fence, and in
 * a plain editor every line simply carries a `>`. Reported 2026-08-06 as
 * *"that will not be able to render that as a markdown — I want to display that
 * as a markdown"*, which is correct and was correct from the start.
 */
const RULE = "---";

/**
 * One turn, as a note.
 *
 * Three lines of provenance, a rule, then the turn's own markdown untouched.
 * The provenance is what makes the note still mean something after the
 * transcript is gone — which is the case this whole feature exists for — and
 * the rule is what tells you where it stops.
 *
 * The cost of not wrapping is real and accepted: the copied text and the note
 * now share one structure, so a turn that opens with its own `#` heading will
 * read as a heading of the note. That is the price of it rendering at all, and
 * a note is yours to edit afterwards.
 */
export function noteFromTurn(session: Session | null, ev: TranscriptEvent): string {
  const label = session ? sessionLabel(session) : ev.session_id.slice(0, 8);
  return [
    `# ${label} · turn ${ev.seq}`,
    "",
    `**${speaker(ev)}** · ${when(ev.ts)}`,
    "",
    RULE,
    "",
    textOf(ev) ?? "",
    "",
  ].join("\n");
}

/**
 * The whole transcript, as a file. `R-B43`.
 *
 * The difference from `noteFromConversation` is the cap, and it is not an
 * oversight in either direction: a note is something you will read, so it keeps
 * the tail and says so, while an export is something you keep, and one that
 * silently dropped the beginning of the conversation would be worse than
 * useless — you would find out when you went looking for the part that is gone.
 *
 * The header states **what mogeung has read**, not what the session said. A
 * huge transcript is capped and tailed on the way in (`R-A5`), so the file can
 * honestly only claim the turns that reached us.
 */
export function transcriptMarkdown(
  session: Session | null,
  events: TranscriptEvent[],
  now: number = Date.now(),
): string {
  const label = session ? sessionLabel(session) : "transcript";
  const carried = events.filter((e) => textOf(e) !== null);

  const head = [`# ${label}`, ""];
  if (session) {
    const facts: string[] = [`- Session \`${session.id}\``];
    if (session.repo_root) facts.push(`- Repo \`${session.repo_root}\``);
    if (session.git_branch) facts.push(`- Branch \`${session.git_branch}\``);
    if (session.cwd) facts.push(`- Working directory \`${session.cwd}\``);
    head.push(...facts);
  }
  head.push(
    `- Exported ${when(now)} · ${carried.length} turn(s)`,
    "",
    "*These are the turns mogeung has read. A very large transcript is tailed on",
    "the way in, so this is the conversation as this window saw it.*",
    "",
    "---",
  );

  // Same rule as the note, and for the same reason: an exported transcript is
  // a document to be read in a markdown viewer, and quoting every line of it
  // renders the whole conversation as one long quotation of itself.
  const blocks = carried.map((ev) =>
    ["", RULE, "", `### ${speaker(ev)} · turn ${ev.seq} · ${when(ev.ts)}`, "", textOf(ev) ?? ""].join("\n"),
  );

  return [...head, ...blocks, ""].join("\n");
}

/**
 * What the exported file is called.
 *
 * Dated, because the second export of a session is a different document and
 * finding them in a directory listing means reading the name rather than the
 * mtime. The shell sanitises this again before it touches the filesystem — the
 * label is agent-written text, and this side only has to make it *readable*.
 */
export function transcriptFilename(session: Session | null, now: number = Date.now()): string {
  const label = session ? sessionLabel(session) : "transcript";
  const slug = label
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
  const d = new Date(now);
  const pad = (n: number) => String(n).padStart(2, "0");
  const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`;
  return `${slug || "transcript"}-${stamp}.md`;
}

/** How many turns a copied conversation carries before it is cut. */
export const CONVERSATION_LIMIT = 200;

/**
 * A whole conversation, as a note.
 *
 * **Capped, and it says so in the note.** A long session runs to thousands of
 * turns and hundreds of kilobytes; pasting that into a scratchpad produces
 * something nobody will ever read and a mirror file nobody will ever open. The
 * last `CONVERSATION_LIMIT` turns are kept — the end of a conversation is the
 * part you are copying it for — and the note states what was dropped rather
 * than quietly appearing complete. A note that lies about being the whole
 * conversation is worse than no note.
 */
export function noteFromConversation(session: Session | null, events: TranscriptEvent[]): string {
  const label = session ? sessionLabel(session) : "conversation";
  const carried = events.filter((e) => textOf(e) !== null);
  const dropped = Math.max(0, carried.length - CONVERSATION_LIMIT);
  const kept = carried.slice(-CONVERSATION_LIMIT);

  const head = [`# ${label}`, "", `Copied ${when(Date.now())} · ${kept.length} turn(s)`];
  if (dropped > 0) {
    head.push("", `*Earlier ${dropped} turn(s) not copied — this is the tail of the conversation.*`);
  }

  // A rule before each turn, so the boundaries survive whatever the turns
  // themselves contain — headings, tables and fenced blocks all arrive intact
  // now that nothing is quoted, and intact means they can run into each other.
  const blocks = kept.map((ev) =>
    [RULE, "", `### ${speaker(ev)} · turn ${ev.seq}`, "", textOf(ev) ?? "", ""].join("\n"),
  );

  return [...head, "", ...blocks].join("\n");
}

/**
 * Open a document from outside the Notes panel, and say which line.
 *
 * The Tasks panel lists checkboxes and the documents they live in are
 * elsewhere, so "where did I write this" has to be one click. Opens the Notes
 * tool if it is shut rather than assuming it — a click that silently sets state
 * behind a closed panel is the *"clicking the queue appears to do nothing"*
 * failure `R-J31` was about, one panel over. Since `R-L13` it also names the
 * task's `ord`, so the editor can scroll to the line rather than the top.
 */
export function openNote(id: string, ord?: number): void {
  const { prefs, setPrefs } = useStore.getState();
  useStore.setState({ noteOpenId: id, noteReveal: ord === undefined ? null : { id, ord } });
  if (!prefs.rail.includes("notes")) {
    setPrefs({ rail: [...prefs.rail, "notes"] });
  }
}

/**
 * A document's first line, as a label. `R-L3`.
 *
 * The same answer the daemon's `slug` reaches for and for the same reason: a
 * document has no title field — ADR-0015 says there is no field on a note that
 * is not written in the note — so the first thing you wrote is the closest
 * thing to a name it has. The leading `#` of a heading comes off, because a
 * label is not a rendering.
 */
export function noteTitle(body: string): string {
  const first = body.split("\n").find((l) => l.trim().length > 0) ?? "";
  return first.replace(/^#{1,6}\s*/, "").trim();
}

/** The document new tasks go into when nothing else says where. `R-L3`. */
export const TASKS_DOC = "Tasks";

/**
 * One line of a document, read the way the daemon's task parser reads it.
 *
 * `R-L3`'s rules: a fenced block and a block quotation hold no tasks, an
 * indented checkbox is nested when there is a list to nest it in and code when
 * there is not. The window needs the same reading for two things — where to
 * put a new line, and which line a task's `ord` names — and a reading that
 * disagreed with the daemon's would file a task under the wrong heading or
 * scroll the editor to the wrong line.
 */
export interface ScannedLine {
  kind: "heading" | "task" | "list" | "blank" | "text" | "quote" | "code" | "fence";
  indent: number;
  /** A heading's level, or a task's words. */
  level?: number;
  text?: string;
}

export function scanLines(body: string): ScannedLine[] {
  const out: ScannedLine[] = [];
  let inFence = false;
  let listOpen = false;
  for (const raw of body.split("\n")) {
    if (/^\s{0,3}(```|~~~)/.test(raw)) {
      inFence = !inFence;
      out.push({ kind: "fence", indent: 0 });
      continue;
    }
    if (inFence) {
      out.push({ kind: "fence", indent: 0 });
      continue;
    }
    const indent = (raw.match(/^ */) ?? [""])[0].length;
    const line = raw.slice(indent);
    if (line.trim() === "") {
      out.push({ kind: "blank", indent });
      continue;
    }
    if (line.startsWith(">")) {
      out.push({ kind: "quote", indent });
      listOpen = false;
      continue;
    }
    const h = indent < 4 ? line.match(/^(#{1,6})\s+(.*?)\s*#*\s*$/) : null;
    if (h) {
      out.push({ kind: "heading", indent, level: h[1].length, text: h[2] });
      listOpen = false;
      continue;
    }
    if (indent >= 4 && !listOpen) {
      out.push({ kind: "code", indent });
      continue;
    }
    const task = line.match(/^[-*+] \[( |x|X)\](?: (.*))?$/);
    if (task) {
      out.push({ kind: "task", indent, text: (task[2] ?? "").trim() });
      listOpen = true;
      continue;
    }
    if (/^([-*+]|\d+[.)]) /.test(line)) {
      out.push({ kind: "list", indent });
      listOpen = true;
      continue;
    }
    out.push({ kind: "text", indent });
    if (indent < 4) listOpen = false;
  }
  return out;
}

/** The line (from zero) a task's `ord` names, or `null` when the body has fewer. */
export function taskLine(body: string, ord: number): number | null {
  let seen = -1;
  const lines = scanLines(body);
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].kind !== "task") continue;
    seen += 1;
    if (seen === ord) return i;
  }
  return null;
}

/** Which heading a new task goes under: its words, and which run of them. */
export interface SectionRef {
  name: string;
  /** The n-th heading with these words, from zero — a heading used twice is
   *  two sections (`R-L11`), and the address has to say which. */
  occurrence: number;
}

/**
 * Put `- [ ] text` into a document where you can see it. `R-L14`.
 *
 * Decided 2026-10-21: with no section named, the line goes **right under the
 * document's title**, before its first heading — the top of the list, where
 * the eye is — not under whatever heading happened to be last, which is where
 * every add used to land and how things got mis-filed. With a section named,
 * it goes to the end of that section. Still one line in one document.
 */
export function insertTask(body: string, text: string, section: SectionRef | null): string {
  const lines = body.split("\n");
  const kinds = scanLines(body);
  const item = `- [ ] ${text}`;
  let at: number;

  if (section) {
    let seen = -1;
    let h = -1;
    for (let i = 0; i < kinds.length; i++) {
      if (kinds[i].kind === "heading" && kinds[i].text === section.name) {
        seen += 1;
        if (seen === section.occurrence) {
          h = i;
          break;
        }
      }
    }
    if (h < 0) return insertTask(body, text, null);
    let end = h + 1;
    while (end < kinds.length && kinds[end].kind !== "heading") end += 1;
    let last = end - 1;
    while (last > h && kinds[last].kind === "blank") last -= 1;
    at = last + 1;
  } else {
    let i = 0;
    while (i < kinds.length && kinds[i].kind === "blank") i += 1;
    if (kinds[i]?.kind === "heading" && kinds[i].level === 1) {
      at = i + 1;
      if (kinds[at]?.kind === "blank") at += 1;
    } else {
      at = 0;
    }
  }

  const out = [...lines];
  out.splice(at, 0, item);
  // A blank line between the new item and prose or a heading that follows,
  // so the list ends where it should and the heading stays a heading.
  const next = kinds[at];
  if (next && next.kind !== "blank" && next.kind !== "task" && next.kind !== "list") {
    out.splice(at + 1, 0, "");
  }
  return out.join("\n").replace(/\n*$/, "") + "\n";
}

/** A folder or a section the user picked as where new tasks go. `R-L14`. */
export interface Target {
  noteId: string;
  title: string;
  section: SectionRef | null;
}

/** Where the next task goes, and what to call it on the box. */
export interface Destination {
  label: string;
  /** An existing note, or `null` when one has to be created. */
  noteId: string | null;
  section: SectionRef | null;
  /** How to create the note when there is none: its first body, and the tags
   *  it is born with. */
  create: { body: (text: string) => string; session_id: string | null; repo: string | null } | null;
}

/**
 * Resolve where a typed task lands. `R-L14`, `R-L16`.
 *
 * In order: the folder or section the user picked; else, with a session
 * selected, the note tagged with that session's **repository** — created as
 * *Tasks — <repo>* on first use, tagged with the session and the repository,
 * so a task you add while looking at a session belongs to that session's work
 * (decided 2026-10-21: *tag on add, show it, filter by it*); else the plain
 * *Tasks* document, created on first use as before.
 *
 * The repository rather than the session id, because a session is one
 * conversation and a repository outlives it: tomorrow's session in the same
 * checkout should find today's tasks.
 */
export function destinationFor(args: {
  notes: readonly Note[];
  sessions: Record<string, Session>;
  selected: string | null;
  target: Target | null;
}): Destination {
  const { notes, sessions, selected, target } = args;
  if (target) {
    return {
      label: target.section ? `${target.title} › ${target.section.name}` : target.title,
      noteId: target.noteId,
      section: target.section,
      create: null,
    };
  }
  const session = selected ? sessions[selected] : undefined;
  const repo = session?.repo_root ?? null;
  if (repo) {
    const title = `${TASKS_DOC} — ${repoName(repo)}`;
    const existing = notes.find((n) => n.repo === repo && noteTitle(n.body) === title);
    return {
      label: title,
      noteId: existing?.id ?? null,
      section: null,
      create: existing
        ? null
        : { body: (text) => `# ${title}\n\n- [ ] ${text}\n`, session_id: selected, repo },
    };
  }
  const existing = notes.find((n) => noteTitle(n.body) === TASKS_DOC);
  return {
    label: TASKS_DOC,
    noteId: existing?.id ?? null,
    section: null,
    create: existing ? null : { body: (text) => `# ${TASKS_DOC}\n\n- [ ] ${text}\n`, session_id: null, repo: null },
  };
}

/** How long a sent task may stay *pending* before the box gives it back. */
export const ADD_TIMEOUT_MS = 4000;

/**
 * Add a task from the Tasks panel. `R-L3`, `R-L14`.
 *
 * **This writes a document, which is the only way a task can be made** —
 * ADR-0015 rule 2 says there is no task outside a `- [ ]` line, so an "add"
 * that did anything else would be inventing a second kind of task.
 *
 * The text stays in the box, pending, until a `tasks` broadcast carries the
 * new line (the store clears it and names the row to wash) or the daemon
 * refuses — or, failing either, until the timeout hands it back. A box that
 * emptied before anything had landed was how an add could vanish in silence.
 */
export function addTask(text: string, dest: Destination): boolean {
  const body = text.trim();
  if (!body) return false;
  const { notes, send } = useStore.getState();
  if (dest.noteId) {
    const existing = notes.find((n) => n.id === dest.noteId);
    if (!existing) return false;
    send({
      cmd: "note_save",
      id: existing.id,
      body: insertTask(existing.body, body, dest.section),
      session_id: existing.session_id ?? null,
      seq: existing.seq ?? null,
      repo: existing.repo ?? null,
    });
  } else if (dest.create) {
    send({
      cmd: "note_save",
      id: "",
      body: dest.create.body(body),
      session_id: dest.create.session_id,
      seq: null,
      repo: dest.create.repo,
    });
  } else {
    return false;
  }
  const pending = { text: body, noteId: dest.noteId };
  useStore.setState({ pendingAdd: pending });
  setTimeout(() => {
    if (useStore.getState().pendingAdd === pending) useStore.setState({ pendingAdd: null });
  }, ADD_TIMEOUT_MS);
  return true;
}

/**
 * Take the `ord`-th task out of a document, with the lines nested under it.
 * `R-L18`.
 *
 * The sub-tasks go with it on purpose: a nested checkbox left behind would
 * attach to whatever list item is now above it, or turn into an indented code
 * block, and either is a line the user did not write. The menu says how many
 * are going. `null` when the document has no such task any more.
 */
export function removeTask(body: string, ord: number): string | null {
  const lines = body.split("\n");
  const kinds = scanLines(body);
  const at = taskLine(body, ord);
  if (at === null) return null;
  const indent = kinds[at].indent;
  let end = at;
  for (let i = at + 1; i < kinds.length; i++) {
    const k = kinds[i];
    if (k.kind === "blank") continue;
    if (k.indent > indent && k.kind !== "heading" && k.kind !== "fence") {
      end = i;
      continue;
    }
    break;
  }
  const out = [...lines.slice(0, at), ...lines.slice(end + 1)];
  // The seam: two blank lines where the block was is one too many.
  if (at > 0 && at < out.length && out[at].trim() === "" && out[at - 1].trim() === "") out.splice(at, 1);
  const joined = out.join("\n").replace(/\n*$/, "");
  return joined ? `${joined}\n` : "";
}

/**
 * Delete a task from the Tasks panel: rewrite its document without the line.
 * `R-L18`. The same direction as a tick — the document changes and the daemon
 * re-derives — and the only one there is (ADR-0015). History keeps the words.
 */
export function deleteTask(noteId: string, ord: number): boolean {
  const { notes, send } = useStore.getState();
  const n = notes.find((x) => x.id === noteId);
  if (!n) return false;
  const body = removeTask(n.body, ord);
  if (body === null) return false;
  send({
    cmd: "note_save",
    id: n.id,
    body,
    session_id: n.session_id ?? null,
    seq: n.seq ?? null,
    repo: n.repo ?? null,
  });
  return true;
}

