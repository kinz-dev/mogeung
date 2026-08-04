/**
 * The scratchpad. `R-B35`, pillar L.
 *
 * The only thing the daemon stores that it could not recompute —
 * [ADR-0015](../../../docs/decisions/0015-markdown-is-the-truth.md), which is
 * also why every note is mirrored to `~/.mogeung/notes/*.md` one way.
 *
 * **This is the one place in the window that writes**, and the distinction is
 * load-bearing: a document under `~/.mogeung` is the user's own writing, where
 * a worktree file belongs to the repository and pillar K forbids editing it.
 * If that line is not held, it erodes a feature at a time.
 *
 * Notes are daemon-owned, so this is a cache of theirs: `notes` is replaced
 * wholesale on every `notes` message and never edited in place. That is what
 * makes two windows on one daemon agree.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Empty, IconButton, Input, Row } from "@/ui/primitives";
import { stamp } from "@/lib/format";
import { sessionLabel } from "@/wire/types";

/**
 * The line worth showing for a note: the one that matched, else the first.
 *
 * The filter searches the **whole body**, so a note can match on its fourth
 * line while its first says something else entirely — and a result list that
 * shows only first lines then looks like it matched at random. Showing the
 * matching line is what makes the answer legible.
 */
export function previewOf(body: string, query: string): string {
  const lines = body.split("\n");
  const q = query.trim().toLowerCase();
  if (!q) return lines[0] ?? "";
  return lines.find((l) => l.toLowerCase().includes(q)) ?? lines[0] ?? "";
}

/**
 * The matching run of `text`, marked.
 *
 * A filtered list tells you *which* notes matched; it does not tell you where,
 * and with a one-line preview per note that is most of what you wanted to know.
 */
function Marked({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (!q) return <>{text}</>;
  const at = text.toLowerCase().indexOf(q.toLowerCase());
  if (at < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, at)}
      <span className="rounded-sm bg-[var(--amber)] px-px text-[var(--on-accent-dark)]">
        {text.slice(at, at + q.length)}
      </span>
      {text.slice(at + q.length)}
    </>
  );
}

export function NotesTool() {
  const notes = useStore((s) => s.notes);
  const send = useStore((s) => s.send);
  const sessions = useStore((s) => s.sessions);
  const selected = useStore((s) => s.selected);
  const [openId, setOpenId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [filter, setFilter] = useState("");
  const rootRef = useRef<HTMLDivElement>(null);
  const findRef = useRef<HTMLInputElement>(null);
  const editorHeight = useStore((s) => s.prefs.notesEditorHeight);
  const setPrefs = useStore((s) => s.setPrefs);
  const [height, setHeight] = useState(editorHeight);
  const heightRef = useRef(height);
  heightRef.current = height;

  useEffect(() => setHeight(editorHeight), [editorHeight]);

  /**
   * `Ctrl+F` finds **within Notes**, and only while Notes has the keyboard.
   *
   * Scoped by asking whether focus is inside this tool rather than by a global
   * binding: `Ctrl+F` belongs to whatever you are looking at, and Monaco wants
   * it for its own find the moment the Code pane has focus. A window-wide
   * binding would take it from everything to give it to one panel.
   */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.key === "f" && (e.ctrlKey || e.metaKey))) return;
      const root = rootRef.current;
      if (!root || !root.contains(document.activeElement)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      findRef.current?.focus();
      // Selected, not merely focused: `Ctrl+F` twice means "search for
      // something else", and making you clear the box first is a papercut.
      findRef.current?.select();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const onDrag = (e: React.MouseEvent) => {
    const startY = e.clientY;
    const startH = heightRef.current;
    const move = (ev: MouseEvent) =>
      // Bounded so the list above always keeps a few rows: an editor dragged to
      // full height hides the thing you pick notes from.
      setHeight(Math.min(window.innerHeight - 220, Math.max(90, startH - (ev.clientY - startY))));
    const up = () => {
      setPrefs({ notesEditorHeight: heightRef.current });
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const rows = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return notes
      .filter((n) => !q || n.body.toLowerCase().includes(q))
      .slice()
      .sort((a, b) => b.updated - a.updated);
  }, [notes, filter]);

  const open = openId ? notes.find((n) => n.id === openId) : null;

  const save = () => {
    if (!open) return;
    send({
      cmd: "note_save",
      id: open.id,
      body: draft,
      session_id: open.session_id ?? null,
      seq: open.seq ?? null,
      repo: open.repo ?? null,
    });
  };

  const q = filter.trim();

  return (
    <div ref={rootRef} tabIndex={-1} className="flex min-h-0 flex-1 flex-col outline-none">
      <div className="flex shrink-0 items-center gap-1 border-b border-[var(--border)] px-2 py-1">
        <Input
          inputRef={findRef}
          value={filter}
          onChange={setFilter}
          placeholder="find in notes  (Ctrl+F)"
          ariaLabel="find in notes"
          onKeyDown={(e) => {
            if (e.key === "Escape") setFilter("");
          }}
        />
        {q && (
          <Dim className="shrink-0 text-2xs whitespace-nowrap">
            {rows.length}/{notes.length}
          </Dim>
        )}
        <IconButton
          title="a new note, tagged with the selected session"
          onClick={() =>
            send({
              cmd: "note_save",
              id: "",
              body: "",
              session_id: selected,
              seq: null,
              repo: selected ? (sessions[selected]?.repo_root ?? null) : null,
            })
          }
        >
          <Plus size={13} />
        </IconButton>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {rows.length === 0 ? (
          <Empty hint="notes are yours — they outlive the session they are tagged with">
            {notes.length === 0 ? "nothing written yet" : "nothing matches"}
          </Empty>
        ) : (
          rows.map((n) => {
            const owner = n.session_id ? sessions[n.session_id] : null;
            return (
              <Row
                key={n.id}
                selected={openId === n.id}
                onClick={() => {
                  setOpenId(n.id);
                  setDraft(n.body);
                }}
                className="border-b border-[var(--border)] py-1"
              >
                <div className="truncate text-sm">
                  {previewOf(n.body, q) ? (
                    <Marked text={previewOf(n.body, q)} query={q} />
                  ) : (
                    <Dim>empty — a plain bookmark</Dim>
                  )}
                </div>
                <div className="flex items-center gap-2 text-2xs">
                  <Dim>{stamp(n.updated)}</Dim>
                  {owner && <Dim className="truncate">{sessionLabel(owner)}</Dim>}
                  {n.seq !== null && n.seq !== undefined && <Dim>turn {n.seq}</Dim>}
                </div>
              </Row>
            );
          })
        )}
      </div>

      {open && (
        <div className="flex shrink-0 flex-col border-t border-[var(--border)]" style={{ height }}>
          <div
            onMouseDown={onDrag}
            className="-mt-px h-1 shrink-0 cursor-row-resize hover:bg-[var(--blue)]"
            title="drag to resize"
          />
          <div className="flex shrink-0 items-center gap-2 px-2 py-1">
            <Dim className="text-2xs">markdown · mirrored to ~/.mogeung/notes</Dim>
            <div className="ml-auto flex items-center gap-1">
              <button
                type="button"
                onClick={save}
                disabled={draft === open.body}
                className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] rounded-sm border border-[var(--border)] px-1.5 py-px text-2xs hover:border-[var(--border-hover)] disabled:opacity-40"
              >
                save
              </button>
              <IconButton
                title="delete this note — the mirrored file stays"
                onClick={() => {
                  send({ cmd: "note_delete", id: open.id });
                  setOpenId(null);
                }}
              >
                <Trash2 size={12} />
              </IconButton>
            </div>
          </div>
          <textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            spellCheck={false}
            className="min-h-0 flex-1 resize-none bg-[var(--bg)] px-2 pb-2 font-mono text-sm outline-none"
            placeholder="write something"
          />
        </div>
      )}
    </div>
  );
}
