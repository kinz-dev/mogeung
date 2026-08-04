/**
 * What the selected search result looks like, at more than one line of it.
 *
 * Three kinds of result, three fidelities, and the difference is visible on
 * purpose: a file and a turn can be shown in full because we can fetch them; a
 * cross-session hit cannot, because the ~200-character clip is the whole of
 * what the daemon returns for one. Closing that gap needs a wire message that
 * does not exist yet, and the pane says so rather than looking truncated.
 */

import { useEffect, useRef } from "react";
import { useStore } from "@/store";
import { Dim, Mono } from "@/ui/primitives";
import { stamp } from "@/lib/format";
import { eventText } from "@/wire/types";
import type { SearchSel } from "@/store";

export function Preview({ onOpen }: { onOpen: (sel: SearchSel) => void }) {
  const sel = useStore((s) => s.search.sel);
  const preview = useStore((s) => s.search.preview);
  const previewAsked = useStore((s) => s.search.previewAsked);
  const patch = useStore((s) => s.patchSearch);
  const send = useStore((s) => s.send);
  const id = useStore((s) => s.selected);
  const events = useStore((s) => (s.selected ? s.events[s.selected] : undefined));
  const sessions = useStore((s) => s.sessions);
  const hitRef = useRef<HTMLDivElement>(null);

  // One request per path, not one per frame. The answer lands through the same
  // `file_content` the Code pane uses; the store hands it to whichever of the
  // two asked for it.
  useEffect(() => {
    if (sel?.kind !== "file" || !id) return;
    if (previewAsked === sel.path) return;
    patch({ previewAsked: sel.path, preview: null });
    send({ cmd: "fetch_file", session_id: id, path: sel.path });
  }, [sel, id, previewAsked, patch, send]);

  // Scroll to the matched line when the selection moves — once. Doing it every
  // render pins the preview and puts every other line out of the mouse's reach.
  useEffect(() => {
    hitRef.current?.scrollIntoView({ block: "center" });
  }, [sel, preview]);

  if (!sel) {
    return (
      <div className="h-52 shrink-0 border-t border-[var(--border)]">
        <div className="flex h-full items-center justify-center">
          <Dim className="text-xs">pick a result</Dim>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-52 shrink-0 flex-col border-t border-[var(--border)]">
      <div className="flex shrink-0 items-center gap-2 px-2 py-1">
        <Dim className="truncate text-2xs">
          {sel.kind === "file" && `${sel.path}:${sel.line}`}
          {sel.kind === "turn" && `turn ${sel.seq}`}
          {sel.kind === "hit" &&
            `${sel.session.slice(0, 8)} L${sel.line}${sel.ts ? ` ${stamp(sel.ts)}` : ""}`}
        </Dim>
        <button
          type="button"
          onClick={() => onOpen(sel)}
          disabled={sel.kind === "hit" && !sessions[sel.session]}
          title={
            sel.kind === "hit" && !sessions[sel.session]
              ? "this session is not in the queue — the search found it on disk, but nothing here is watching it"
              : "open it where it lives"
          }
          className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] ml-auto rounded-sm border border-[var(--border)] px-1.5 py-px text-2xs hover:border-[var(--border-hover)] disabled:opacity-40"
        >
          open
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-2 pb-2">
        {sel.kind === "file" &&
          (preview && preview[0] === sel.path ? (
            <div className="min-w-max">
              {preview[1].split("\n").map((line, i) => {
                const n = i + 1;
                const hit = n === sel.line;
                return (
                  <div key={n} ref={hit ? hitRef : undefined} className="flex whitespace-pre">
                    <Mono className="w-12 shrink-0 text-right text-2xs text-[var(--dim)]">{n}</Mono>
                    <Mono className={hit ? "pl-2 text-xs text-[var(--amber)]" : "pl-2 text-xs"}>
                      {line || " "}
                    </Mono>
                  </div>
                );
              })}
            </div>
          ) : (
            <Dim className="text-xs">reading…</Dim>
          ))}

        {sel.kind === "turn" &&
          (() => {
            const ev = events?.find((e) => e.seq === sel.seq);
            if (!ev) return <Dim className="text-xs">that turn is no longer in this transcript</Dim>;
            return <Mono className="text-xs whitespace-pre-wrap">{eventText(ev.kind)}</Mono>;
          })()}

        {sel.kind === "hit" && (
          <>
            <Mono className="text-xs whitespace-pre-wrap">{sel.preview}</Mono>
            <div className="mt-2">
              <Dim className="text-2xs">
                this is the whole clip the search returns — open it to read the conversation around it
              </Dim>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
