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

import { useEffect, useMemo, useRef, type CSSProperties } from "react";
import { useStore } from "@/store";
import { Chip, Dim, Empty } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { needsHuman, reasonLabel, sessionLabel, type AttentionItem, type Session } from "@/wire/types";
import { base, compact, fmtDur, num, secsSince, shortDir } from "@/lib/format";
import { tagBg } from "@/lib/tags";

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
      // **Live only**, asked for 2026-08-07 after the first look at it. The
      // queue's own scopes include dead sessions on purpose — an ended session
      // can still be `needs_review` and still wants you — but a *wall* of them
      // is the opposite of what this is for. A tile earns its square by being
      // something that might change while you are looking at it; a finished
      // session never will, and a grid where most squares are inert is a grid
      // you stop scanning. Reading the queue's `live` scope instead was the
      // alternative and lost: the wall would then mean different things
      // depending on a filter set somewhere else, which is exactly the kind of
      // "why is this empty" that a glanceable surface must not have.
      .filter((q) => sessions[q.session_id].alive)
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
        <span className="text-sm font-semibold tracking-wider text-[var(--text-strong)] uppercase">Wall</span>
        <Dim className="text-xs">
          {tiles.length} session{tiles.length === 1 ? "" : "s"}
          {waiting > 0 ? ` · ${waiting} waiting` : ""}
        </Dim>
        <Dim className="ml-auto text-xs">
          live sessions only · click a tile to go to it · Esc to leave · positions never move
        </Dim>
      </div>

      {tiles.length === 0 ? (
        <Empty hint="the wall shows sessions that are still running — a finished one is in the queue, where it can be read rather than watched">
          nothing running
        </Empty>
      ) : (
        <div className="grid min-h-0 flex-1 auto-rows-min gap-2 overflow-y-auto p-3 sm:grid-cols-2 2xl:grid-cols-3">
          {tiles.map(({ q, s }) => {
            const wants = needsHuman(q.reason);
            const tint = tagBg(scoped.tags[s.id]);
            /**
             * How long it has been *in this state*, which is the number the
             * wall exists to surface. `last_event_at` answers "when did
             * anything happen", and a session that has been sitting on a
             * permission prompt for four minutes is still emitting nothing —
             * so the two agree, and only one of them says *waiting*.
             */
            const stuckFor = s.status_since ? fmtDur(secsSince(s.status_since)) : null;
            const diff = s.files_changed > 0;
            return (
              <button
                key={s.id}
                type="button"
                onClick={() => {
                  select(s.id);
                  setOpen(false);
                }}
                title={`${sessionLabel(s)} — ${q.detail || reasonLabel(q.reason)}`}
                /**
                 * The tint is the **whole card**, asked for 2026-08-07 — and it
                 * arrives as `--tag-bg` through the same `.mogeung-tag-row`
                 * rule the queue uses, so the palette stays written down in one
                 * place and a tinted tile answers the pointer the same way a
                 * tinted row does. A 8px dot was what shipped first and it lost
                 * for the reason `R-B42` already recorded about the queue's
                 * 4px bar: the point of a colour is knowing which session this
                 * is *without reading*, and a dot has to be looked for.
                 */
                style={{
                  ...(tint ? ({ "--tag-bg": tint } as CSSProperties) : {}),
                  ...(wants ? { boxShadow: `inset 0 0 0 1px ${ringFor(q.reason)}` } : {}),
                }}
                className={cn(
                  "flex min-w-0 flex-col gap-1 rounded-sm border p-2 text-left outline-none",
                  "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
                  "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--ring)]",
                  wants ? "border-transparent" : "border-[var(--border)] hover:border-[var(--border-hover)]",
                  // Only when nothing else owns the surface — the same ordering
                  // trap `QueuePanel` documents: `cn` is `twMerge`, so a
                  // Tailwind `bg-*` here would win over the tint class, which
                  // twMerge cannot see.
                  tint ? "mogeung-tag-row" : wants ? "bg-[var(--bg-raised)]" : "bg-[var(--bg-panel)]",
                )}
              >
                <div className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate text-base text-[var(--text-strong)]">
                    {scoped.labels[s.id] ?? sessionLabel(s)}
                  </span>
                  <Dim className="ml-auto shrink-0 text-xs" title="since anything last happened">
                    {fmtDur(secsSince(s.last_event_at))}
                  </Dim>
                </div>

                <div className="flex min-w-0 items-center gap-1.5">
                  {/* Overriding `Chip`'s shared `text-2xs`, and only here. The
                      reason is the one thing on this surface you read from
                      across the room — a wall whose verdict is set in the same
                      type as its metadata is a wall you have to lean into,
                      which is the opposite of what it is for. */}
                  <Chip color={wants ? ringFor(q.reason) : "var(--dim)"} className="px-1.5 text-xs">
                    {reasonLabel(q.reason)}
                  </Chip>
                  {/* The one number that changes what you do next: a prompt
                      waited on for four minutes is a different situation from
                      the same prompt four seconds old, and nothing else on the
                      card distinguishes them. */}
                  {wants && stuckFor && (
                    <Dim className="shrink-0 text-xs" title="how long it has been in this state">
                      for {stuckFor}
                    </Dim>
                  )}
                  {s.git_branch && (
                    <Dim className="ml-auto truncate text-xs" title="branch">
                      {s.git_branch}
                    </Dim>
                  )}
                </div>

                {/* What it has actually done — the half a queue row has no room
                    for. Only what is non-zero: a card of `0`s reads as broken
                    rather than as quiet. */}
                <div className="flex min-w-0 flex-wrap items-center gap-x-2 text-xs text-[var(--dim)]">
                  <span title="the repository this session is working in" className="truncate">
                    {s.repo_root ? base(s.repo_root) : shortDir(s.cwd)}
                  </span>
                  {s.turns > 0 && <span title="turns in the conversation">{s.turns} turns</span>}
                  {diff && (
                    <span title="uncommitted work, against the base this session started from">
                      {s.files_changed}f{" "}
                      <span className="text-[var(--add-fg)]">+{num(s.insertions)}</span>{" "}
                      <span className="text-[var(--del-fg)]">−{num(s.deletions)}</span>
                    </span>
                  )}
                  {s.tokens_out > 0 && (
                    <span title="tokens out — never money, ADR-0005">{compact(s.tokens_out)} out</span>
                  )}
                  {s.collisions.length > 0 && (
                    <span className="text-[var(--amber)]" title="another live session is editing the same file">
                      {s.collisions.length} collision{s.collisions.length === 1 ? "" : "s"}
                    </span>
                  )}
                  {s.loop_signal && (
                    <span className="text-[var(--amber)]" title={s.loop_signal}>
                      thrashing
                    </span>
                  )}
                  {s.limit_hit_at && (
                    <span className="text-[var(--red)]" title="this session hit a rate limit">
                      limit
                    </span>
                  )}
                </div>

                {/* The tail. Fixed height so a quiet tile and a busy one are the
                    same size — a grid whose cells resize with their content is
                    a grid that moves, which is the one thing this must not do. */}
                <div className="h-[3.75rem] min-w-0 overflow-hidden font-mono text-xs leading-5 text-[var(--dim)]">
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
