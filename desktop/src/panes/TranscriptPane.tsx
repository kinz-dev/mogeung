/**
 * The conversation, as evidence.
 *
 * Structured turns rather than a replayed terminal —
 * [ADR-0002](../../docs/decisions/0002-structured-transcript-not-a-terminal.md).
 * Every line was classified on the way in, which is what lets a turn be
 * searched, marked and linked to.
 *
 * Virtualised, which quietly deletes a wart: the egui client capped the drawn
 * events and grew a "show earlier" button, because markdown was parsed per
 * visible event per frame and an unbounded transcript tied the frame rate to
 * how long the session had been running. A windowed list has no such cap, so
 * the whole conversation is here.
 */

import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  Bookmark,
  ChevronDown,
  ChevronRight,
  Download,
  NotebookPen,
  Search as SearchIcon,
  X,
} from "lucide-react";
import { useStore } from "@/store";
import { Badge, Checkbox, Dim, Empty, IconButton, Input, Mono } from "@/ui/primitives";
import { best, type Engine } from "@/lib/search";
import { clock, oneLine } from "@/lib/format";
import { cn } from "@/lib/cn";
import { eventLabel, eventText, type TranscriptEvent } from "@/wire/types";
import { noteFromConversation, noteFromTurn, transcriptFilename, transcriptMarkdown } from "@/lib/notes";
import { chooseSavePath, exportText, isTauri } from "@/lib/tauri";

function kindColor(t: TranscriptEvent["kind"]["t"]): string {
  switch (t) {
    case "user_prompt":
      return "var(--blue)";
    case "assistant_text":
      return "var(--green)";
    case "thinking":
      return "var(--purple)";
    case "tool_use":
      return "var(--amber)";
    case "tool_result":
      return "var(--dim)";
    case "result":
      return "var(--text)";
    default:
      return "var(--dim)";
  }
}

/**
 * Memoized, with the mark/remark callbacks taking the turn's `seq` rather than
 * closing over it — fresh closures per render would defeat the memo, and the
 * memo is what keeps an events tick from re-parsing the markdown of every
 * visible turn.
 */
const Turn = memo(function Turn({
  ev,
  markdown,
  noteBody,
  onMark,
  onRemark,
  onCopyToNote,
  highlighted,
}: {
  ev: TranscriptEvent;
  markdown: boolean;
  noteBody: string | null;
  onMark: (seq: number) => void;
  onRemark: (seq: number, text: string) => void;
  onCopyToNote: (seq: number) => void;
  highlighted: boolean;
}) {
  const [open, setOpen] = useState(true);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const k = ev.kind;
  const text = eventText(k);
  const isError = (k.t === "tool_result" && k.is_error) || (k.t === "result" && k.is_error);
  const long = text.length > 600;

  return (
    <div
      className={cn(
        "group border-b border-[var(--border)] px-3 py-1.5",
        // Not a permanent state: it fades, because a highlight that stays
        // becomes part of the furniture and stops meaning "here".
        "transition-colors duration-[var(--dur-base)] ease-[var(--ease-standard)]",
        highlighted && "bg-[var(--selection-bg)]/40 ring-1 ring-inset ring-[var(--blue)]",
      )}
    >
      <div className="flex items-center gap-2">
        <Badge color={kindColor(k.t)} dark={k.t === "tool_use"}>
          {eventLabel(k)}
        </Badge>
        {k.t === "tool_use" && <Mono className="text-xs text-[var(--text-strong)]">{k.name}</Mono>}
        {isError && <span className="text-2xs text-[var(--red)]">error</span>}
        <Dim className="ml-auto shrink-0 text-2xs">{clock(ev.ts)}</Dim>
        <IconButton
          title={noteBody === null ? "mark this turn" : "unmark — the remark goes with it"}
          active={noteBody !== null}
          onClick={() => onMark(ev.seq)}
          className={noteBody === null ? "opacity-0 group-hover:opacity-100" : undefined}
        >
          <Bookmark size={11} />
        </IconButton>
        {/*
          Copy, not mark — and the difference is the point. A mark *points at*
          this turn and dies with the transcript; a note **takes the words with
          it** and outlives the session, because `session_id` on a note is a tag
          with no foreign key behind it. This is the button for "I want this
          after the session is gone".
        */}
        <IconButton
          title="copy this turn into a note — the note keeps the words, and outlives the session"
          onClick={() => onCopyToNote(ev.seq)}
          className="opacity-0 group-hover:opacity-100"
        >
          <NotebookPen size={11} />
        </IconButton>
        {long && (
          <IconButton title={open ? "collapse" : "expand"} onClick={() => setOpen(!open)}>
            {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </IconButton>
        )}
      </div>

      {/*
        The remark. A bookmark with no words still says *where*, but one with a
        line of your own says *why* — and "why did I mark this" is the question
        you are actually asking a week later. Written here rather than in a
        separate pane because the moment you want to write it is the moment you
        are reading the turn.
      */}
      {noteBody !== null &&
        (editing ? (
          <input
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={() => {
              onRemark(ev.seq, draft);
              setEditing(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                onRemark(ev.seq, draft);
                setEditing(false);
              }
              if (e.key === "Escape") setEditing(false);
            }}
            placeholder="a short remark — why this turn matters"
            className="mt-1 w-full rounded-sm border border-[var(--purple)] bg-[var(--bg)] px-2 py-1 text-xs outline-none"
          />
        ) : (
          <button
            type="button"
            onClick={() => {
              setDraft(noteBody);
              setEditing(true);
            }}
            title="click to write, or change, the remark"
            className={cn(
              "mt-1 block w-full rounded-sm border-l-2 border-[var(--purple)] bg-[var(--bg-raised)] px-2 py-1 text-left text-xs",
              "outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
              "hover:bg-[var(--state-hover)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2",
            )}
          >
            {noteBody.trim() || <Dim className="italic">add a remark…</Dim>}
          </button>
        ))}

      <div className="mt-1 text-sm leading-relaxed">
        {k.t === "tool_use" ? (
          <Mono className="text-xs text-[var(--dim)]">{k.summary}</Mono>
        ) : markdown && (k.t === "assistant_text" || k.t === "user_prompt") ? (
          <div className="prose-mogeung">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {open || !long ? text : `${text.slice(0, 600)}…`}
            </ReactMarkdown>
          </div>
        ) : (
          <Mono
            className={cn(
              "text-xs whitespace-pre-wrap",
              k.t === "thinking" && "text-[var(--purple)] opacity-90",
              k.t === "tool_result" && "text-[var(--dim)]",
              isError && "text-[var(--del-fg)]",
            )}
          >
            {open || !long ? text : `${text.slice(0, 600)}…`}
          </Mono>
        )}
      </div>
    </div>
  );
});

export function TranscriptPane() {
  const id = useStore((s) => s.selected);
  const events = useStore((s) => (s.selected ? s.events[s.selected] : undefined));
  const markdown = useStore((s) => s.prefs.markdown);
  const showThinking = useStore((s) => s.prefs.showThinking);
  const setPrefs = useStore((s) => s.setPrefs);
  const notes = useStore((s) => s.notes);
  const sessions = useStore((s) => s.sessions);
  const send = useStore((s) => s.send);
  const pushError = useStore((s) => s.pushError);
  const focusTs = useStore((s) => s.focusEventTs);
  const highlightSeq = useStore((s) => s.highlightSeq);
  const focusSeq = useStore((s) => s.focusSeq);
  const [find, setFind] = useState("");
  const [showFind, setShowFind] = useState(false);
  /** Where the last export landed. Shown, not flashed — see `onExport`. */
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const shown = useMemo(() => {
    if (!events) return [];
    return showThinking ? events : events.filter((e) => e.kind.t !== "thinking");
  }, [events, showThinking]);

  const noteFor = useMemo(() => {
    const map = new Map<number, string>();
    for (const n of notes) {
      if (n.session_id === id && n.seq !== null && n.seq !== undefined) map.set(n.seq, n.body);
    }
    return map;
  }, [notes, id]);

  const onRemark = useCallback(
    (seq: number, text: string) => {
      if (!id) return;
      const existing = notes.find((n) => n.session_id === id && n.seq === seq);
      send({
        cmd: "note_save",
        // An id keeps it the same note; empty would mint a second one against
        // the same turn.
        id: existing?.id ?? "",
        body: text,
        session_id: id,
        seq,
        repo: null,
      });
    },
    [id, notes, send],
  );

  const onMark = useCallback(
    (seq: number) => {
      if (!id) return;
      // An empty body is a plain bookmark: marking a turn and writing about it
      // are one gesture with two depths.
      const existing = notes.find((n) => n.session_id === id && n.seq === seq);
      if (existing) send({ cmd: "note_delete", id: existing.id });
      else send({ cmd: "note_save", id: "", body: "", session_id: id, seq, repo: null });
    },
    [id, notes, send],
  );

  /**
   * Copy a turn — or the whole conversation — into a note of its own.
   *
   * A **new** note every time, never an edit of an existing one: this is a
   * scratchpad gesture, and silently folding a second copy into the note you
   * made an hour ago would lose whichever one you meant to keep. `seq` is left
   * null on purpose, so these do not collide with `onMark`'s one-note-per-turn
   * bookmarks — a copied turn is a document, not a mark on a transcript.
   */
  const copyToNote = useCallback(
    (body: string) => {
      if (!body.trim()) return;
      send({
        cmd: "note_save",
        id: "",
        body,
        session_id: id,
        seq: null,
        repo: id ? (sessions[id]?.repo_root ?? null) : null,
      });
      // Open the rail on Notes: a copy that lands somewhere you cannot see
      // looks like a button that did nothing, and the next thing you want is
      // to write the sentence explaining why you kept it.
      setPrefs({ rail: "notes" });
    },
    [id, sessions, send, setPrefs],
  );

  const onCopyToNote = useCallback(
    (seq: number) => {
      const ev = shown.find((e) => e.seq === seq);
      if (ev) copyToNote(noteFromTurn(id ? (sessions[id] ?? null) : null, ev));
    },
    [shown, id, sessions, copyToNote],
  );

  /**
   * The whole transcript, written to a file. `R-B43`.
   *
   * **Every turn, `showThinking` notwithstanding.** The checkbox above is a
   * reading preference and this is an export: a file that quietly omitted the
   * agent's reasoning because a box happened to be unticked would be wrong in
   * the one direction an archive must not be, and you would find out much
   * later.
   *
   * The picker is the OS's, and only the picker: `plugin-dialog` returns a
   * path and this window's own `export_text` does the writing. Adding the fs
   * plugin instead would have handed the webview a general write verb; this way
   * the only file it can write is one you named in a native dialog.
   *
   * The path is shown afterwards rather than a "saved" flash, because a save
   * you cannot find is a save that did not happen — and on the fallback route
   * the destination was ours to choose, so it is ours to tell you.
   */
  const onExport = useCallback(async () => {
    if (!events) return;
    const session = id ? (sessions[id] ?? null) : null;
    const body = transcriptMarkdown(session, events);
    const name = transcriptFilename(session);
    if (isTauri()) {
      // Three answers, and only the middle one is nothing happening: a path
      // means save there, `null` means you cancelled and nothing should be
      // written, `undefined` means there was no picker to ask and the shell
      // should choose rather than let the file evaporate.
      const picked = await chooseSavePath(name);
      if (picked === null) return;
      try {
        setSavedAt(await exportText(name, body, picked));
      } catch (e) {
        pushError(`could not save the transcript: ${String(e)}`);
      }
      return;
    }
    {
      // A browser tab has no shell to write through, and the anchor trick is
      // the only route it has. Not the shipped path — the desktop build is —
      // but leaving `npm run dev` unable to test the button is its own cost.
      const url = URL.createObjectURL(new Blob([body], { type: "text/markdown" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = name;
      a.click();
      URL.revokeObjectURL(url);
      setSavedAt(name);
    }
  }, [events, id, sessions, pushError]);

  const virt = useVirtualizer({
    count: shown.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 90,
    overscan: 6,
  });

  // `R-B36`: find within the conversation you are reading, which `R-F1` cannot
  // do — that one searches every session and answers with sessions.
  const hits = useMemo(() => {
    const q = find.trim();
    if (!q) return [];
    const out: { i: number; score: number; engine: Engine }[] = [];
    shown.forEach((ev, i) => {
      const h = best(q, eventText(ev.kind));
      if (h) out.push({ i, score: h.score, engine: h.engine });
    });
    out.sort((a, b) => b.score - a.score);
    return out;
  }, [find, shown]);

  // Land on the first turn at or after a moment a search hit or a commit asked
  // for. Consumed once, so it does not drag the scroll back every render.
  useEffect(() => {
    if (!focusTs) return;
    const at = shown.findIndex((e) => e.ts >= focusTs);
    if (at >= 0) virt.scrollToIndex(at, { align: "center" });
    useStore.setState({ focusEventTs: null });
  }, [focusTs, shown, virt]);

  // A jump by seq waits for its turn to exist. Bookmarks use this rather than a
  // timestamp because the session may not have been loaded when you clicked.
  useEffect(() => {
    if (focusSeq === null) return;
    const at = shown.findIndex((e) => e.seq === focusSeq);
    if (at < 0) return; // events still on their way — stay pending
    virt.scrollToIndex(at, { align: "center" });
    useStore.setState({ focusSeq: null });
  }, [focusSeq, shown, virt]);

  // The path fades: it is a receipt, not a status, and one that stays becomes
  // furniture — the same reasoning as the turn highlight below.
  useEffect(() => {
    if (savedAt === null) return;
    const t = window.setTimeout(() => setSavedAt(null), 12_000);
    return () => window.clearTimeout(t);
  }, [savedAt]);

  useEffect(() => {
    if (highlightSeq === null) return;
    const t = window.setTimeout(() => useStore.setState({ highlightSeq: null }), 2600);
    return () => window.clearTimeout(t);
  }, [highlightSeq]);

  if (!id) return <Empty>select a session</Empty>;
  if (!events) return <Empty>reading the transcript…</Empty>;

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <div className="flex h-7 shrink-0 items-center gap-3 border-b border-[var(--border)] px-2">
        <Checkbox
          checked={markdown}
          onChange={(v) => setPrefs({ markdown: v })}
          label="markdown"
          title="render agent replies as Markdown rather than raw text"
        />
        <Checkbox
          checked={showThinking}
          onChange={(v) => setPrefs({ showThinking: v })}
          label="thinking"
          title="show the agent's reasoning blocks"
        />
        <Dim className="text-2xs">{shown.length} turns</Dim>
        {savedAt && (
          // The path, not a checkmark. The window chose the directory, so the
          // only useful thing it can tell you is where to look.
          <Dim className="min-w-0 truncate text-2xs text-[var(--green)]" title={savedAt}>
            saved · {savedAt}
          </Dim>
        )}
        <div className="ml-auto flex items-center gap-1">
          <IconButton
            title="save the whole transcript as a Markdown file in your downloads"
            onClick={() => void onExport()}
          >
            <Download size={12} />
          </IconButton>
          <IconButton
            title="copy this conversation into a note — long ones keep the tail, and the note says so"
            onClick={() => copyToNote(noteFromConversation(id ? (sessions[id] ?? null) : null, shown))}
          >
            <NotebookPen size={12} />
          </IconButton>
          <IconButton title="find in this transcript  (Ctrl+F)" active={showFind} onClick={() => setShowFind(!showFind)}>
            <SearchIcon size={12} />
          </IconButton>
        </div>
      </div>

      {showFind && (
        <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-2 py-1">
          <div className="flex-1">
            <Input
              value={find}
              onChange={setFind}
              autoFocus
              placeholder="in this transcript"
              onKeyDown={(e) => {
                if (e.key === "Enter" && hits.length > 0) virt.scrollToIndex(hits[0].i, { align: "center" });
                if (e.key === "Escape") setShowFind(false);
              }}
            />
          </div>
          <Dim className="shrink-0 text-2xs">
            {find.trim() ? (hits.length ? `${hits.length} turn(s) · best is ${hits[0].engine}` : "no match") : ""}
          </Dim>
          <IconButton title="close" onClick={() => setShowFind(false)}>
            <X size={12} />
          </IconButton>
        </div>
      )}

      <div ref={parentRef} className="min-h-0 flex-1 overflow-y-auto">
        {shown.length === 0 ? (
          <Empty hint="the session has not spoken yet">no events in this transcript</Empty>
        ) : (
          <div style={{ height: virt.getTotalSize(), position: "relative" }}>
            {virt.getVirtualItems().map((v) => {
              const ev = shown[v.index];
              return (
                <div
                  key={ev.seq}
                  ref={virt.measureElement}
                  data-index={v.index}
                  style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${v.start}px)` }}
                >
                  <Turn
                    ev={ev}
                    markdown={markdown}
                    highlighted={highlightSeq === ev.seq}
                    noteBody={noteFor.get(ev.seq) ?? null}
                    onRemark={onRemark}
                    onMark={onMark}
                    onCopyToNote={onCopyToNote}
                  />
                </div>
              );
            })}
          </div>
        )}
      </div>

      {find.trim() && hits.length > 0 && (
        <div className="max-h-32 shrink-0 overflow-y-auto border-t border-[var(--border)]">
          {hits.slice(0, 30).map((h) => (
            <div
              key={h.i}
              onClick={() => virt.scrollToIndex(h.i, { align: "center" })}
              className="cursor-default truncate px-2 py-px text-xs hover:bg-[var(--bg-faint)]"
            >
              <Dim className="text-2xs">{clock(shown[h.i].ts)}</Dim>{" "}
              <Mono>{oneLine(eventText(shown[h.i].kind), 160)}</Mono>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
