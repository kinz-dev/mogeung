/**
 * One query over three corpora. `R-F13`.
 *
 * Not one search — three, with three different costs, kept apart so the
 * cheapest is never held up by the dearest:
 *
 * - **this conversation** matches events already in memory, and runs on every
 *   keystroke because it is free;
 * - **this session's files** and **every session** are daemon round-trips and
 *   wait for Enter, which is the rule `R-F1` shipped with — the corpus is tens
 *   of MB and a per-keystroke scan would punish typing.
 *
 * Both daemon answers echo their query back, so each group drops a reply to a
 * question that has since been retyped without knowing anything about the
 * other. That is why there is no shared results slot for two writers to fight
 * over.
 */

import { useMemo, useRef, useState } from "react";
import { useStore } from "@/store";
import { Checkbox, Dim, Empty, Input, Mono } from "@/ui/primitives";
import { best, searchMove, type Engine } from "@/lib/search";
import { openFile } from "@/lib/explorer";
import { cn } from "@/lib/cn";
import { oneLine, shortDir, base, stamp } from "@/lib/format";
import { eventText, type SearchHit } from "@/wire/types";
import type { SearchSel } from "@/store";
import { Preview } from "@/ui/tools/SearchPreview";

/** A group's header: what it searched, and what state its answer is in. */
function groupHead(name: string, g: { running: boolean; hits: unknown[] | null }, off: string | null): string {
  if (off) return `${name} · ${off}`;
  if (g.running) return `${name} · searching…`;
  // Three states kept apart on purpose (`R-J5`): still running, answered with
  // nothing, and never asked are different things, and flattening them is how
  // a search comes to look broken while working perfectly.
  return g.hits === null ? `${name} · press Enter` : `${name} · ${g.hits.length} hit(s)`;
}

function Group({
  head,
  children,
  salt,
}: {
  head: string;
  children: React.ReactNode;
  salt: string;
}) {
  const [open, setOpen] = useState(true);
  return (
    <div className="border-b border-[var(--border)]">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] flex w-full items-center gap-1 px-2 py-1 text-left text-xs font-semibold text-[var(--text-strong)] hover:bg-[var(--bg-faint)]"
      >
        <span className="text-[var(--dim)]">{open ? "▾" : "▸"}</span>
        {head}
      </button>
      {open && <div data-group={salt}>{children}</div>}
    </div>
  );
}

function ResultRow({
  selected,
  scroll,
  tag,
  text,
  onClick,
  onDoubleClick,
}: {
  selected: boolean;
  scroll: boolean;
  tag: string;
  text: string;
  onClick: () => void;
  onDoubleClick: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  if (scroll && selected) ref.current?.scrollIntoView({ block: "center" });
  return (
    <div
      ref={ref}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      className={cn(
        "cursor-default overflow-hidden px-2 py-px text-xs whitespace-nowrap hover:bg-[var(--bg-faint)]",
        selected && "bg-[var(--selection-bg)] text-[var(--text-strong)]",
      )}
    >
      <Mono className="text-2xs text-[var(--dim)]">{tag}</Mono>{" "}
      <Mono>{oneLine(text, 240)}</Mono>
    </div>
  );
}

/**
 * `onOpened` fires when a result is acted on.
 *
 * The rail stays put — you are keeping results beside the thing you are
 * reading. The overlay closes, because you asked it a question, it answered,
 * and you have gone where it sent you. Same component, different job, and the
 * caller is the one that knows which.
 */
export function SearchTool({ onOpened }: { onOpened?: () => void } = {}) {
  const search = useStore((s) => s.search);
  const patch = useStore((s) => s.patchSearch);
  const send = useStore((s) => s.send);
  const selectedId = useStore((s) => s.selected);
  const events = useStore((s) => (s.selected ? s.events[s.selected] : undefined));
  const sessions = useStore((s) => s.sessions);
  const select = useStore((s) => s.select);
  const [inList, setInList] = useState(false);
  const [scrollRow, setScrollRow] = useState(false);
  const boxRef = useRef<HTMLInputElement>(null);

  const q = search.query.trim();

  /** The free group. Recomputed every render from events already in memory. */
  const turns = useMemo(() => {
    const rows: { seq: number; engine: Engine; text: string; score: number }[] = [];
    if (!q || !events) return rows;
    for (const ev of events) {
      const text = eventText(ev.kind);
      const hit = best(q, text);
      if (hit) rows.push({ seq: ev.seq, engine: hit.engine, text, score: hit.score });
    }
    rows.sort((a, b) => b.score - a.score);
    return rows.slice(0, 200);
  }, [q, events]);

  /** Every visible row, in draw order, so the keyboard has a list to walk. */
  const rows = useMemo<SearchSel[]>(() => {
    const out: SearchSel[] = [];
    if (!q) return out;
    for (const t of turns) out.push({ kind: "turn", seq: t.seq });
    for (const m of search.files.hits ?? []) out.push({ kind: "file", path: m.path, line: m.line });
    if (search.everywhere)
      for (const h of search.insight.hits ?? [])
        out.push({ kind: "hit", session: h.session_id, line: h.line, ts: h.timestamp, preview: h.preview });
    return out;
  }, [q, turns, search.files.hits, search.insight.hits, search.everywhere]);

  const same = (a: SearchSel | null, b: SearchSel) => a !== null && JSON.stringify(a) === JSON.stringify(b);

  const run = () => {
    const query = search.query.trim();
    if (!query) {
      patch({ issued: "", files: { running: false, hits: null, truncated: false }, insight: { running: false, hits: null, truncated: false } });
      return;
    }
    patch({
      issued: query,
      files: selectedId
        ? { running: true, hits: null, truncated: false }
        : { running: false, hits: null, truncated: false },
      insight: search.everywhere
        ? { running: true, hits: null, truncated: false }
        : { running: false, hits: null, truncated: false },
    });
    if (selectedId) send({ cmd: "search_content", session_id: selectedId, query });
    if (search.everywhere) send({ cmd: "insight_search", query });
    setInList(false);
    // Refocus: the box keeps the caret so the next keystroke — the Down that
    // walks into the results you just asked for — does not go to the queue.
    requestAnimationFrame(() => boxRef.current?.focus());
  };

  const open = (sel: SearchSel) => {
    onOpened?.();
    if (sel.kind === "file") {
      if (selectedId) openFile(selectedId, sel.path, { pin: true, line: sel.line });
    } else if (sel.kind === "turn") {
      const ev = events?.find((e) => e.seq === sel.seq);
      useStore.setState({ focusEventTs: ev?.ts ?? null, highlightSeq: sel.seq });
    } else if (sessions[sel.session]) {
      // `select`, not a bare assignment: a session reached from the
      // cross-session corpus is usually one this window has never hydrated,
      // and landing on an empty Transcript is the failure this jump avoids.
      select(sel.session);
      useStore.setState({ focusEventTs: sel.ts });
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const at = rows.findIndex((r) => same(search.sel, r));
      const [next, to] = searchMove(inList, at < 0 ? null : at, rows.length, e.key === "ArrowDown");
      setInList(next);
      if (to !== null && to !== at && rows[to]) {
        patch({ sel: rows[to] });
        setScrollRow(true);
      }
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      // Enter means two things, safely, because `inList` says which:
      // run the search while typing, open the row once you have arrowed in.
      if (inList && search.sel) open(search.sel);
      else run();
    }
  };

  const pick = (sel: SearchSel) => {
    patch({ sel });
    setScrollRow(false);
  };

  return (
    <>
      <div className="shrink-0 space-y-1 border-b border-[var(--border)] px-2 py-1.5">
        <Input
          inputRef={boxRef}
          value={search.query}
          onChange={(v) => {
            setInList(false);
            const stale = search.issued !== v.trim();
            patch({
              query: v,
              ...(stale
                ? {
                    files: { running: false, hits: null, truncated: false },
                    insight: { running: false, hits: null, truncated: false },
                  }
                : {}),
            });
          }}
          onKeyDown={onKeyDown}
          placeholder="find…"
          autoFocus
        />
        <div className="flex items-center gap-2">
          <Checkbox
            checked={search.everywhere}
            onChange={(v) => patch({ everywhere: v })}
            label="every session"
            title="Search the transcripts and prompt history of every session ever run (R-F1), not only this one's files and conversation. It is the slow half of this box."
          />
          <button
            type="button"
            onClick={run}
            className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] ml-auto rounded-sm border border-[var(--border)] px-1.5 py-0.5 text-2xs hover:border-[var(--border-hover)]"
            title="or press Enter in the box"
          >
            search
          </button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {!q ? (
          <Empty hint="Enter also searches files and every session">type to search this conversation</Empty>
        ) : (
          <>
            <Group
              salt="turns"
              head={
                selectedId
                  ? `this conversation · ${turns.length} turn(s)`
                  : "this conversation · no session selected"
              }
            >
              {selectedId && turns.length === 0 && <Dim className="block px-2 py-1 text-xs">no match</Dim>}
              {turns.map((t) => {
                const sel: SearchSel = { kind: "turn", seq: t.seq };
                return (
                  <ResultRow
                    key={`t${t.seq}`}
                    selected={same(search.sel, sel)}
                    scroll={scrollRow}
                    tag={t.engine}
                    text={t.text}
                    onClick={() => pick(sel)}
                    onDoubleClick={() => open(sel)}
                  />
                );
              })}
            </Group>

            <Group
              salt="files"
              head={groupHead("this session's files", search.files, selectedId ? null : "no session")}
            >
              {search.files.hits?.length === 0 && <Dim className="block px-2 py-1 text-xs">no match</Dim>}
              {(search.files.hits ?? []).map((m, i) => {
                const sel: SearchSel = { kind: "file", path: m.path, line: m.line };
                return (
                  <ResultRow
                    key={`f${i}-${m.path}-${m.line}`}
                    selected={same(search.sel, sel)}
                    scroll={scrollRow}
                    tag={`${shortDir(m.path)}${base(m.path)}:${m.line}`}
                    text={m.text.trim()}
                    onClick={() => pick(sel)}
                    onDoubleClick={() => open(sel)}
                  />
                );
              })}
              {search.files.truncated && (
                <Dim className="block px-2 py-1 text-2xs">… capped — narrow the query</Dim>
              )}
            </Group>

            <Group
              salt="insight"
              head={groupHead("every session", search.insight, search.everywhere ? null : "off")}
            >
              {!search.everywhere && (
                <Dim className="block px-2 py-1 text-xs">off — tick “every session” above</Dim>
              )}
              {search.everywhere && search.insight.hits?.length === 0 && (
                <Dim className="block px-2 py-1 text-xs">no match</Dim>
              )}
              {search.everywhere &&
                (search.insight.hits ?? []).map((h: SearchHit, i: number) => {
                  const sel: SearchSel = {
                    kind: "hit",
                    session: h.session_id,
                    line: h.line,
                    ts: h.timestamp,
                    preview: h.preview,
                  };
                  const src = h.source === "history" ? "hist" : h.source === "prompt" ? "you" : "sess";
                  return (
                    <ResultRow
                      key={`i${i}-${h.session_id}-${h.line}`}
                      selected={same(search.sel, sel)}
                      scroll={scrollRow}
                      tag={`${src} ${h.session_id.slice(0, 8)}${h.timestamp ? ` ${stamp(h.timestamp)}` : ""}`}
                      text={h.preview.trim()}
                      onClick={() => pick(sel)}
                      onDoubleClick={() => open(sel)}
                    />
                  );
                })}
              {search.insight.truncated && (
                <Dim className="block px-2 py-1 text-2xs">… capped — narrow the query</Dim>
              )}
            </Group>
          </>
        )}
      </div>

      <Preview onOpen={open} />
    </>
  );
}
