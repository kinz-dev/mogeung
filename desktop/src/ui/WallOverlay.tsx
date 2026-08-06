/**
 * Every session at once, as a floorplan. `R-B50`.
 *
 * The Attention queue answers *which one needs me* as a **ranked list**, and
 * ranking is its whole value — but a ranked list reorders, so the row for a
 * given session is somewhere new every time you look and spatial memory never
 * forms. A wall does not move. `dotfiles` is where it was, and a change in the
 * bottom-left corner registers before you have read a word.
 *
 * That is a genuinely different mechanism from ranking, and it is also
 * [A1](../../../docs/product/assumptions.md) being quietly re-litigated: the
 * product's claim is that it **tells** you who needs you, and someone scanning
 * six tiles has gone back to looking. Which is why this is a **chord**, held
 * open only while you want it, rather than a view you can leave up. If it turns
 * out to be where you live, that is a finding about the queue, not a feature
 * request for a bigger wall.
 *
 * **Nothing here is fetched.** Every tile is built from what the snapshot
 * already streams — `last_activity`, `recent_tools`, `live_status`, the
 * attention reason. The version of this that opened twelve `tmux attach`es was
 * designed and rejected in the same conversation: an 80-column TUI in a 260px
 * tile is illegible mush, and noticing needs three lines rather than eighty
 * columns.
 */

import { useEffect, useMemo, useRef } from "react";
import { useStore } from "@/store";
import { Chip, Dim, Empty } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { needsHuman, reasonLabel, sessionLabel, type AttentionItem, type Session } from "@/wire/types";
import { fmtDur, secsSince } from "@/lib/format";
import { tagColor } from "@/lib/tags";

/** The ring, and it is the only colour on a quiet tile. */
function ringFor(reason: AttentionItem["reason"]): string {
  if (reason === "failed" || reason === "rate_limited") return "var(--red)";
  if (needsHuman(reason)) return "var(--amber)";
  return "transparent";
}

/**
 * The three lines a tile shows under its name.
 *
 * `last_activity` is what the session is *doing* and is the line worth having;
 * the tools are what it has been reaching for, which is how a loop looks from
 * across the room. Neither is a transcript — this is a contact sheet, and a
 * tile that tried to be a terminal would be neither.
 */
function tileLines(s: Session): string[] {
  const out: string[] = [];
  if (s.last_activity) out.push(s.last_activity);
  if (s.recent_tools.length > 0) out.push(s.recent_tools.slice(0, 4).join(" · "));
  if (s.error) out.push(s.error);
  if (out.length === 0 && s.last_prompt) out.push(s.last_prompt);
  return out.slice(0, 3);
}

export function WallOverlay() {
  const open = useStore((s) => s.showWall);
  // Set directly, the way every other overlay in this window is — see
  // `HealthWindow`. A store action for one boolean is ceremony.
  const setOpen = (v: boolean) => useStore.setState({ showWall: v });
  const queue = useStore((s) => s.queue);
  const sessions = useStore((s) => s.sessions);
  const scoped = useStore((s) => s.scoped());
  const select = useStore((s) => s.select);
  const rootRef = useRef<HTMLDivElement>(null);

  /**
   * **Sorted by session id, not by score.** This is the whole point and the
   * easiest thing to get wrong: ordering the wall by the queue's ranking would
   * rebuild the grid every time a session changed state, which is precisely the
   * moving-target problem the wall exists to escape. A stable key means a tile
   * stays where you learnt it.
   */
  const tiles = useMemo(() => {
    return queue
      .filter((q) => sessions[q.session_id])
      .filter((q) => !scoped.hidden.includes(q.session_id))
      .map((q) => ({ q, s: sessions[q.session_id] }))
      .sort((a, b) => a.s.id.localeCompare(b.s.id));
  }, [queue, sessions, scoped.hidden]);

  useEffect(() => {
    if (!open) return;
    rootRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  if (!open) return null;

  const waiting = tiles.filter((t) => needsHuman(t.q.reason)).length;

  return (
    <div
      ref={rootRef}
      tabIndex={-1}
      role="dialog"
      aria-label="the wall"
      className="absolute inset-0 z-40 flex flex-col bg-[var(--bg)]/95 outline-none backdrop-blur-sm"
      // Clicking the backdrop leaves, the way every other overlay here does.
      onClick={(e) => {
        if (e.target === e.currentTarget) setOpen(false);
      }}
    >
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-3 py-1.5">
        <span className="text-xs font-semibold tracking-wider text-[var(--text-strong)] uppercase">Wall</span>
        <Dim className="text-2xs">
          {tiles.length} session{tiles.length === 1 ? "" : "s"}
          {waiting > 0 ? ` · ${waiting} waiting` : ""}
        </Dim>
        <Dim className="ml-auto text-2xs">
          click a tile to go to it · Esc to leave · positions never move
        </Dim>
      </div>

      {tiles.length === 0 ? (
        <Empty hint="the wall shows what the queue holds — nothing is hidden from one and shown on the other">
          nothing to show
        </Empty>
      ) : (
        <div className="grid min-h-0 flex-1 auto-rows-min gap-2 overflow-y-auto p-3 sm:grid-cols-2 lg:grid-cols-3">
          {tiles.map(({ q, s }) => {
            const wants = needsHuman(q.reason);
            const tag = scoped.tags[s.id];
            return (
              <button
                key={s.id}
                type="button"
                onClick={() => {
                  select(s.id);
                  setOpen(false);
                }}
                title={`${sessionLabel(s)} — ${q.detail || reasonLabel(q.reason)}`}
                className={cn(
                  "flex min-w-0 flex-col gap-1 rounded-sm border p-2 text-left outline-none",
                  "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
                  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--ring)]",
                  wants
                    ? "border-transparent bg-[var(--bg-raised)]"
                    : "border-[var(--border)] bg-[var(--bg-panel)] hover:border-[var(--border-hover)]",
                )}
                style={wants ? { boxShadow: `inset 0 0 0 1px ${ringFor(q.reason)}` } : undefined}
              >
                <div className="flex min-w-0 items-center gap-1.5">
                  {tag && (
                    <span
                      aria-hidden
                      className="h-2 w-2 shrink-0 rounded-full"
                      style={{ background: tagColor(tag) ?? "var(--dim)" }}
                    />
                  )}
                  <span className="truncate text-sm text-[var(--text-strong)]">
                    {scoped.labels[s.id] ?? sessionLabel(s)}
                  </span>
                  <Dim className="ml-auto shrink-0 text-2xs">{fmtDur(secsSince(s.last_event_at))}</Dim>
                </div>

                <div className="flex min-w-0 items-center gap-1.5">
                  <Chip color={wants ? ringFor(q.reason) : "var(--dim)"}>{reasonLabel(q.reason)}</Chip>
                  {s.git_branch && <Dim className="truncate text-2xs">{s.git_branch}</Dim>}
                </div>

                {/* The tail. Fixed height so a quiet tile and a busy one are the
                    same size — a grid whose cells resize with their content is
                    a grid that moves, which is the one thing this must not do. */}
                <div className="h-12 min-w-0 overflow-hidden font-mono text-2xs leading-4 text-[var(--dim)]">
                  {tileLines(s).map((line, i) => (
                    <div key={i} className="truncate">
                      {line}
                    </div>
                  ))}
                </div>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
