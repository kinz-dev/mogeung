/**
 * The Attention queue — the reason this app exists.
 *
 * Collapsed it is a strip, never nothing. A panel you can lose entirely is one
 * you have to rediscover, and a count you can still see is what makes taking
 * the room back a decision rather than a search.
 */

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ChevronDown, ChevronLeft, ChevronRight, Clock, EyeOff, FolderTree, Monitor, Network, Pin, Tag, TerminalSquare } from "lucide-react";
import { useStore } from "@/store";
import { Badge, Chip, Dim, Empty, IconButton, Input, Row, Segmented, Tooltip } from "@/ui/primitives";
import { ContextMenu, MenuItem, MenuLabel, MenuSeparator } from "@/ui/Menu";
import { cn } from "@/lib/cn";
import { ZoomPane } from "@/ui/ZoomPane";
import { TAGS, tagBg, tagColor, tagLabel } from "@/lib/tags";
import { groupQueue, hiddenByTmux, machineName, matchesFilter, queueDetail, visibleQueue, type QueueLine } from "@/lib/queue";
// Clicking a queue row means *put that session on screen*, which is not the
// same as `select` once a pane can be held — see `revealSession`. `R-J31`.
import { revealSession } from "@/lib/panes";

// Re-exported so `QueueFilter.test.ts` and anything else that learnt this name
// here keeps working; it lives in `lib/queue.ts` now, where the keymap can
// reach it without importing a component module.
export { matchesFilter };
import { InfoDock } from "@/ui/InfoDock";
import { SourceMark } from "@/ui/SourceMark";
import { QUEUE_LIST_ID } from "@/lib/keymap";
import { fmtDur, secsSince } from "@/lib/format";
import {
  awaitingPermission,
  needsHuman,
  reasonLabel,
  repoName,
  sessionLabel,
  type AttentionItem,
  type AttentionReason,
  type Session,
} from "@/wire/types";

/** The colour a reason wears. Matches the egui client's `reason_color`. */
export function reasonColor(reason: AttentionReason): string {
  switch (reason) {
    case "awaiting_permission":
      return "var(--urgent)";
    case "awaiting_input":
      return "var(--red)";
    case "failed":
      return "var(--red)";
    case "rate_limited":
      return "var(--amber)";
    case "needs_review":
      return "var(--blue)";
    case "stalled":
      return "var(--amber)";
    case "running":
      return "var(--green)";
    default:
      return "var(--dim)";
  }
}

/** Amber and green fills want dark lettering — white on amber is 2.36:1. */
function darkInk(reason: AttentionReason): boolean {
  return reason === "rate_limited" || reason === "stalled" || reason === "running";
}

/**
 * What the CLI itself says the session is doing, right now.
 *
 * Deliberately separate from the queue's own verdict beside it. The badge is
 * mogeung's *ranking* — why this row is where it is — and this is the registry's
 * raw answer. They can legitimately disagree: a session can be `live·idle` and
 * still be ranked `APPROVE` because an unanswered tool call is what actually
 * needs you (`R-B4`). Showing both is what makes that legible instead of
 * looking like a contradiction.
 *
 * `null` when the session has ended: "not live" is already said by the absence
 * of everything else on the row.
 */
function liveText(session: Session): { text: string; color: string } | null {
  if (!session.alive) return null;
  switch (session.live_status) {
    case "busy":
      return { text: "live·busy", color: "var(--blue)" };
    case "idle":
      return { text: "live·idle", color: "var(--amber)" };
    default:
      // Alive, but the registry reported a state we do not model. Saying
      // "live" is honest; guessing which kind would not be.
      return { text: "live", color: "var(--dim)" };
  }
}

/** A stable letter and colour per name, so a chip means the same tomorrow. */
export function chipChar(name: string): string {
  const c = name.trim()[0];
  return (c ?? "?").toUpperCase();
}

export function labelColor(name: string): string {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  const palette = [
    "var(--graph-0)",
    "var(--graph-1)",
    "var(--graph-2)",
    "var(--graph-3)",
    "var(--graph-4)",
    "var(--graph-5)",
  ];
  return palette[h % palette.length];
}


export function useVisibleQueue(): { item: AttentionItem; session: Session }[] {
  const queue = useStore((s) => s.queue);
  const sessions = useStore((s) => s.sessions);
  // `scope` alone, not the whole prefs object — this hook is mounted by the
  // queue, the strip and the ambient board at once, and a whole-prefs
  // subscription meant every zoom or font tweak recomputed all three.
  const scope = useStore((s) => s.prefs.scope);
  const tmuxOnly = useStore((s) => s.prefs.tmuxOnly);
  const filter = useStore((s) => s.filter);
  const scoped = useStore((s) => s.scoped());

  return useMemo(
    () => visibleQueue({ queue, sessions, scope, filter, scoped, tmuxOnly }),
    [queue, sessions, scope, filter, scoped, tmuxOnly],
  );
}

/**
 * How many rows the tmux rule is holding back. `R-J93`.
 *
 * A second subscription rather than a field on `useVisibleQueue`, because the
 * strip and the ambient board want the rows and have nowhere to put a number
 * — and this one is only ever read by the panel that can turn the rule off.
 */
function useHiddenByTmux(): number {
  const queue = useStore((s) => s.queue);
  const sessions = useStore((s) => s.sessions);
  const scope = useStore((s) => s.prefs.scope);
  const tmuxOnly = useStore((s) => s.prefs.tmuxOnly);
  const filter = useStore((s) => s.filter);
  const scoped = useStore((s) => s.scoped());

  return useMemo(
    () => hiddenByTmux({ queue, sessions, scope, filter, scoped, tmuxOnly }),
    [queue, sessions, scope, filter, scoped, tmuxOnly],
  );
}

function Strip() {
  const setPrefs = useStore((s) => s.setPrefs);
  const selected = useStore((s) => s.selected);
  const scoped = useStore((s) => s.scoped());
  const rows = useVisibleQueue();
  const needing = rows.filter((r) => needsHuman(r.item.reason)).length;

  return (
    // The same surface collapsed as expanded: the strip *is* the queue, and a
    // panel that changes colour when you collapse it looks like a different
    // thing rather than the same thing narrower.
    <div className="flex w-[30px] shrink-0 flex-col items-center gap-1.5 border-r border-[var(--border)] bg-[var(--queue-bg)] py-2">
      <IconButton title="show the queue  ([)" onClick={() => setPrefs({ queueCollapsed: false })}>
        <ChevronRight size={14} />
      </IconButton>
      {needing > 0 && (
        <span
          title={`${needing} session(s) need you`}
          className="text-base font-semibold text-[var(--amber)]"
        >
          {needing}
        </span>
      )}
      {/* The full width, so a chip's status dot, which sits over its corner,
          is inside the scrolling box rather than clipped at its edge. */}
      <div className="flex min-h-0 w-full flex-1 flex-col items-center gap-1 overflow-y-auto pt-1">
        {rows.map(({ item, session }) => {
          const name = scoped.labels[session.id] ?? repoName(session);
          // Your colour wins over the hashed one here. The hash exists so a
          // chip means *something* before you have decided anything; once you
          // have, that decision is the better signal — and this strip is the
          // view with no text at all, so it is where a colour earns most.
          const tag = tagColor(scoped.tags[session.id]);
          return (
            <Tooltip
              key={session.id}
              content={`${scoped.labels[session.id] ?? sessionLabel(session)}\n${repoName(session)} · ${reasonLabel(item.reason)}${tag ? ` · tagged ${tagLabel(scoped.tags[session.id])}` : ""}`}
            >
              <button
                type="button"
                aria-label={`${scoped.labels[session.id] ?? sessionLabel(session)} — ${reasonLabel(item.reason)}`}
                onClick={() => revealSession(session.id)}
                className={cn(
                "outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
                  "relative grid h-5 w-5 shrink-0 place-items-center rounded-sm text-2xs font-bold",
                  selected === session.id && "ring-1 ring-[var(--selection-stroke)]",
                )}
                style={{
                  background: tag ?? labelColor(name),
                  color: "var(--on-accent)",
                  outline: needsHuman(item.reason) ? "1px solid var(--urgent)" : undefined,
                }}
              >
                {chipChar(name)}
                {/*
                  **The status, as a dot** (`R-J113`), asked 2026-09-25: *"when
                  I collapse it … there is no way for me to tell the status
                  (like WAITING, RUNNING) … may be with little RED circle or
                  little GREEN/AMBER circle"*. The chip's own colour is the
                  session — your tag, or its name's hash — so the status needs
                  a mark of its own, and it wears the expanded row's badge
                  colour (`reasonColor`) so the two views mean one thing: red
                  waiting or failed, orange approve, amber stalled or limited,
                  blue review, green running, grey idle. Ringed in the strip's
                  background so it reads apart from a chip of the same colour.
                */}
                <span
                  aria-hidden
                  data-status={item.reason}
                  className="absolute -right-0.5 -bottom-0.5 h-2 w-2 rounded-full"
                  style={{ background: reasonColor(item.reason), boxShadow: "0 0 0 1.5px var(--queue-bg)" }}
                />
              </button>
            </Tooltip>
          );
        })}
      </div>
    </div>
  );
}

function QueueRow({ item, session }: { item: AttentionItem; session: Session }) {
  const selected = useStore((s) => s.selected);
  const setScoped = useStore((s) => s.setScoped);
  const scoped = useStore((s) => s.scoped());
  const send = useStore((s) => s.send);
  const label = scoped.labels[session.id];
  const tag = tagColor(scoped.tags[session.id]);
  const tint = tagBg(scoped.tags[session.id]);
  const isSelected = selected === session.id;
  const pinned = scoped.pinned.includes(session.id);
  const hidden = scoped.hidden.includes(session.id);
  const perm = awaitingPermission(session);
  const snoozed = !!session.snoozed_until && Date.parse(session.snoozed_until) > Date.now();
  const live = liveText(session);
  const now = Date.now();
  // Another machine's, mirrored from a daemon this one follows. `R-I21`.
  const remote = machineName(session);
  const stale = !!session.origin?.stale;

  // A live session cannot be hidden: it is still doing things, and hiding one
  // means you stop being told about it. Withheld by being **absent** rather
  // than greyed out — an item you can see but not press invites the question
  // "why not", every time.
  const hideable = !session.alive || hidden;

  return (
    <ContextMenu
      trigger={
        <Row
          // **Both signals, always, and they are answering different
          // questions.** The first attempt withheld the tint while a row was
          // selected, on the theory that one surface can only say one thing;
          // the report back was that a tagged row then stops looking tagged
          // the moment you click it, which is when you are most likely to be
          // checking you are on the right one. So the colour stays and
          // selection is said *differently* — a ring, not a surface — rather
          // than by taking the colour away.
          //
          // `selected` is still passed: an untagged row keeps the plain
          // selection surface, which is the case where a surface is free.
          selected={isSelected && !tint}
          aria-selected={isSelected}
          onClick={() => revealSession(session.id)}
          style={tint ? ({ "--tag-bg": tint } as CSSProperties) : undefined}
          className={cn(
            "relative border-b border-[var(--border)] py-1.5 pr-2 pl-2",
            // The card surface, and only when nothing else owns the row's
            // background. Three things want this one property — the card, a
            // tag tint, the plain selection — and they are not settled the
            // same way: `cn` is `twMerge`, so the *last* Tailwind `bg-*` wins
            // and this one, arriving through `className`, would silently drop
            // `Row`'s selection surface; `.mogeung-tag-row` is not a Tailwind
            // class at all, so twMerge cannot see it and stylesheet order
            // would decide. Stating the condition is what makes it readable
            // rather than emergent — and it has a test, because removing this
            // guard breaks selection somewhere else entirely.
            !tint && !isSelected && "bg-[var(--row-bg)]",
            tint && "mogeung-tag-row",
            // Inset, so it draws *inside* the row rather than between rows —
            // an outset ring on a bordered list item is clipped by its
            // neighbour and reads as a thick line on one edge only.
            isSelected && "outline-2 -outline-offset-2 outline-[var(--selection-stroke)]",
            isSelected && "font-medium text-[var(--text-strong)]",
            // Last known, not current: its daemon has gone quiet, and the
            // row is kept so the session is not simply gone. `R-I21`.
            stale && "opacity-55",
          )}
          data-stale={stale || undefined}
        >
          {/* The leading edge is the one strip of a dense row that carries no
              other signal, which is what keeps a tag from arguing with the
              badge beside it. Absolute so it cannot push the text along.

              Wider while selected: the ring runs the whole way round, and the
              bar is what tells you *which* row inside it is yours when the
              list is scrolled to a group of them. */}
          {tag && (
            <span
              aria-hidden
              title={`tagged ${tagLabel(scoped.tags[session.id])}`}
              className={cn("absolute top-0 bottom-0 left-0", isSelected ? "w-1.5" : "w-1")}
              style={{ background: tag }}
            />
          )}
          <div className="flex min-w-0 items-center gap-1.5">
            <Badge color={reasonColor(item.reason)} dark={darkInk(item.reason)} className="shrink-0">
              {reasonLabel(item.reason)}
            </Badge>
            {live && (
              <span
                className="shrink-0 text-2xs"
                style={{ color: live.color }}
                title="what the CLI's own registry says this session is doing — the badge beside it is mogeung's ranking, and the two can differ"
              >
                {live.text}
              </span>
            )}
            {label && (
              <Chip color={labelColor(label)} title="your label for this session" className="shrink-0">
                {label}
              </Chip>
            )}
            {/*
              A pane running an agent that has written nothing. `R-J75`,
              ADR-0038. Said out loud rather than left to the zeroes below,
              because a row whose counts are all zero looks like a session that
              did nothing — and this is one that has not been *able* to. The
              terminal is the point: open it and type.
            */}
            {session?.provisional && (
              <Chip
                color="var(--amber)"
                title="a tmux pane running an agent that has not written anything yet — open its terminal to see what it is waiting for"
                className="shrink-0"
              >
                not started writing
              </Chip>
            )}
            {/*
              Where it runs, when that is not here. `R-I21`, feature 0047.
              Always said on the row, not only by the heading: grouping is a
              toggle, and a remote row must never pass for a local one.
            */}
            {remote && (
              <Chip
                color={stale ? "var(--dim)" : "var(--blue)"}
                title={
                  stale
                    ? `on ${remote} — its daemon has not been heard from; last known, not current`
                    : `on ${remote} — mirrored from a daemon this one follows`
                }
                className="shrink-0"
              >
                {`on ${remote}`}
              </Chip>
            )}
            {/* `min-w-0 flex-1` is what makes `truncate` work at all: without
                it the span is content-sized and the ellipsis never engages. */}
            <span className="min-w-0 flex-1 truncate text-sm text-[var(--text-strong)]">
              {sessionLabel(session)}
            </span>
            {pinned && <Pin size={9} className="shrink-0 text-[var(--blue)]" />}
            {snoozed && <Clock size={9} className="shrink-0 text-[var(--dim)]" />}
            {hidden && <EyeOff size={9} className="shrink-0 text-[var(--dim)]" />}
          </div>
          <div className="mt-0.5 flex items-center gap-2 text-xs">
            <Dim className="shrink-0">{repoName(session)}</Dim>
            {/* Blue, not dim: repo and branch ran together as one grey phrase,
                and which half was which took a moment every time. Blue is what
                git identity already wears in the status bar. */}
            {session.git_branch && (
              <span className="truncate text-[var(--blue)]" title="branch">
                {session.git_branch}
              </span>
            )}
            <Dim className="ml-auto shrink-0">{fmtDur(secsSince(session.last_event_at, now))} ago</Dim>
          </div>
          {/*
            The detail, and — bottom right, under the elapsed time — **which
            CLI this is**. `R-J49`, asked 2026-08-25.

            Placed on this line rather than up beside the badge on purpose: the
            first line is where the row says what needs *you*, and a permanent
            fact about the session competing there would push the title, which
            is the thing you actually read, further into its ellipsis. Down here
            it sits in the corner the eye reaches last, in the column the "4m
            ago" already owns.

            `min-w-0` on the detail is what keeps the mark from being pushed off
            the row by a long one — without it the truncation never engages and
            the flex line simply overflows.
          */}
          <div className="mt-0.5 flex items-center gap-2 text-xs">
            <span className="min-w-0 flex-1 truncate text-[var(--dim)]">{queueDetail(item, session, now)}</span>
            <SourceMark source={session.source} />
          </div>
          {perm && (
            <div className="mt-1 truncate rounded-sm border border-[var(--urgent)] px-1 py-px text-2xs text-[var(--urgent)]">
              {perm.name}
              {perm.summary ? `: ${perm.summary}` : ""}
            </div>
          )}
          {session.collisions.length > 0 && (
            <div className="mt-1 text-2xs text-[var(--amber)]">
              also being edited by {session.collisions[0].other_label} — {session.collisions[0].path}
            </div>
          )}
        </Row>
      }
    >
      <MenuLabel>{label ?? repoName(session)}</MenuLabel>
      {/* A row of swatches rather than seven menu items, because the whole
          point of a colour is that you recognise it faster than a word. */}
      <div className="flex items-center gap-1 px-2 py-1">
        {TAGS.map((t) => (
          <button
            key={t.id}
            type="button"
            title={t.label}
            aria-label={t.label}
            onClick={() =>
              setScoped({
                tags: { ...scoped.tags, [session.id]: t.id },
              })
            }
            className={cn(
              "h-4 w-4 shrink-0 rounded-full outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:outline-offset-1",
              scoped.tags[session.id] === t.id && "ring-2 ring-[var(--selection-stroke)]",
            )}
            style={{ background: t.color }}
          />
        ))}
        <button
          type="button"
          title="no colour"
          aria-label="no colour"
          onClick={() => {
            const next = { ...scoped.tags };
            delete next[session.id];
            setScoped({ tags: next });
          }}
          className="grid h-4 w-4 shrink-0 place-items-center rounded-full border border-[var(--border-hover)] text-2xs text-[var(--dim)] outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:outline-offset-1"
        >
          ×
        </button>
      </div>
      <MenuItem onSelect={() => useStore.setState({ labelEditing: session.id })}>
        {label ? "Edit label…" : "Label…"}
      </MenuItem>
      <MenuItem
        onSelect={() =>
          setScoped({
            pinned: pinned ? scoped.pinned.filter((x) => x !== session.id) : [...scoped.pinned, session.id],
          })
        }
      >
        {pinned ? "Unpin" : "Pin to the top"}
      </MenuItem>
      <MenuItem
        onSelect={() =>
          send({ cmd: "snooze", session_id: session.id, minutes: snoozed ? 0 : 30 })
        }
      >
        {snoozed ? "Wake" : "Snooze 30m"}
      </MenuItem>
      {hideable && (
        <MenuItem
          onSelect={() =>
            setScoped({
              hidden: hidden ? scoped.hidden.filter((x) => x !== session.id) : [...scoped.hidden, session.id],
            })
          }
        >
          {hidden ? "Unhide" : "Hide from the queue"}
        </MenuItem>
      )}
      <MenuSeparator />
      {/* Its window is on another machine's screen when it is mirrored. `R-I22`. */}
      <MenuItem
        disabled={!!session.origin}
        onSelect={() => send({ cmd: "focus_terminal", session_id: session.id })}
      >
        {session.origin ? `Its terminal window is on ${machineName(session)}` : "Raise its terminal window"}
      </MenuItem>
      <MenuItem onSelect={() => void navigator.clipboard?.writeText(session.id)}>
        Copy session id
      </MenuItem>
      <MenuSeparator />
      {/* Last, behind a separator, and unconfirmed — the same shape the egui
          client gives it. It drops the session and its review marks from the
          daemon's record; it does not touch `~/.claude`, so a session whose
          transcript is still there comes back on the next scan as one nobody
          has read yet. */}
      <MenuItem onSelect={() => send({ cmd: "forget_session", session_id: session.id })}>
        Forget this session
      </MenuItem>
    </ContextMenu>
  );
}

export function QueuePanel() {
  const prefs = useStore((s) => s.prefs);
  const setPrefs = useStore((s) => s.setPrefs);
  const filter = useStore((s) => s.filter);
  const firstSnapshot = useStore((s) => s.firstSnapshot);
  const rows = useVisibleQueue();
  const hidden = useHiddenByTmux();
  const selected = useStore((s) => s.selected);
  const parentRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(prefs.queueWidth);
  // The width as of *now*, for the pointer-up handler to read. See `onDrag`.
  const latest = useRef(width);
  const folded = useStore((s) => s.foldedGroups);
  const here = useStore((s) => s.daemon?.host ?? null);
  // Whether any row is another machine's — the machine toggle is offered only
  // then, since with none it would do nothing.
  const anyRemote = useStore((s) => Object.values(s.sessions).some((x) => x.origin));

  // `R-B6`, `R-I21`. One function, which the keyboard asks too — see
  // `groupQueue` for the order it keeps and the one exception it makes.
  const display = useMemo<QueueLine[]>(
    () =>
      groupQueue(rows, {
        byRepo: prefs.groupByRepo,
        byMachine: prefs.groupByMachine,
        collapsed: folded,
        here,
      }),
    [rows, prefs.groupByRepo, prefs.groupByMachine, folded, here],
  );

  // Follow the selection with the viewport. Arrowing down a long queue that
  // does not scroll walks the highlight straight off the bottom, and the list
  // then looks like it stopped responding.
  useEffect(() => {
    if (!selected) return;
    const at = display.findIndex((r) => r.kind === "row" && r.session.id === selected);
    if (at >= 0) rowVirtualizer.scrollToIndex(at, { align: "auto" });
    // `display` deliberately absent: this follows the *selection*, not every
    // re-rank the daemon sends, or a queue that reorders under you would yank
    // the viewport while you are reading.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected]);

  const rowVirtualizer = useVirtualizer({
    count: display.length,
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => (display[i]?.kind === "header" ? 24 : 74),
    overscan: 8,
  });

  if (prefs.queueCollapsed) return <Strip />;

  const onDrag = (e: React.MouseEvent) => {
    const startX = e.clientX;
    const startW = latest.current;
    const move = (ev: MouseEvent) => {
      // Through the ref as well as through state, because `up` below is
      // registered once and closes over the render it was made in. Reading
      // `width` there saved the width the drag *started* at, so the file
      // trailed a drag behind and a reload gave you back the size before the
      // one you had just chosen.
      latest.current = Math.min(620, Math.max(280, startW + ev.clientX - startX));
      setWidth(latest.current);
    };
    const up = () => {
      // Written on pointer-up only — a drag would otherwise save on every
      // frame of the drag, which is the rule the layout already follows.
      setPrefs({ queueWidth: latest.current });
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  return (
    // `--queue-bg` on the outermost element, so the header, the rows and the
    // gap under a short list are one surface. Anything less leaves a seam
    // where the list ends, which is the sort of thing that reads as a
    // rendering fault rather than as a boundary.
    <div className="flex shrink-0 border-r border-[var(--border)] bg-[var(--queue-bg)]" style={{ width }}>
      <div className="flex min-w-0 flex-1 flex-col">
      <ZoomPane name="queue">
      <div className="flex h-full min-w-0 flex-1 flex-col">
        <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
          <span className="text-2xs font-semibold tracking-wider text-[var(--blue)] uppercase">
            Attention
          </span>
          <span className="text-2xs text-[var(--dim)]">{rows.length}</span>
          <div className="ml-auto flex items-center gap-1">
            <IconButton
              title={
                prefs.tmuxOnly
                  ? "under tmux only, in “needs you” and “live”  (R-J93)"
                  : "showing sessions with no tmux pane too  (R-J93)"
              }
              active={prefs.tmuxOnly}
              onClick={() => setPrefs({ tmuxOnly: !prefs.tmuxOnly })}
            >
              <TerminalSquare size={13} />
            </IconButton>
            {anyRemote && (
              <IconButton
                title="group by machine, this one first  (R-I21)"
                active={prefs.groupByMachine}
                onClick={() => setPrefs({ groupByMachine: !prefs.groupByMachine })}
              >
                <Network size={13} />
              </IconButton>
            )}
            <IconButton
              title="group by repository  (R-B6)"
              active={prefs.groupByRepo}
              onClick={() => setPrefs({ groupByRepo: !prefs.groupByRepo })}
            >
              <FolderTree size={13} />
            </IconButton>
            <IconButton title="collapse the queue  ([)" onClick={() => setPrefs({ queueCollapsed: true })}>
              <ChevronLeft size={14} />
            </IconButton>
          </div>
        </div>

        <div className="flex shrink-0 items-center border-b border-[var(--border)] px-2 py-1">
          <Segmented
            value={prefs.scope}
            onChange={(scope) => setPrefs({ scope })}
            options={[
              { value: "needs_you", label: "needs you", title: "only sessions that want something from you" },
              { value: "live", label: "live", title: "every running session" },
              { value: "all", label: "all", title: "everything mogeung has seen" },
            ]}
          />
        </div>

        <div className="shrink-0 px-2 py-1">
          <Input
            value={filter}
            onChange={(v) => useStore.setState({ filter: v })}
            placeholder="filter — repo: branch: file: label:"
          />
        </div>

        {/*
          What the rule took, said out loud. `R-J93`.

          A hidden row and a row that was never reported look identical from
          here, and this panel is the product's one claim — so the number that
          is missing is worth a line of its own, and the line is the way back:
          pressing it shows them without leaving the scope you are working in.
        */}
        {hidden > 0 && (
          <button
            type="button"
            onClick={() => setPrefs({ tmuxOnly: false })}
            title="these sessions have no tmux pane, so there is no terminal to attach to"
            className="shrink-0 px-2 pb-1 text-left text-2xs text-[var(--dim)] underline-offset-2 outline-none hover:underline focus-visible:outline-2 focus-visible:outline-[var(--ring)]"
          >
            {hidden} hidden — not under tmux
          </button>
        )}

        <div
          ref={parentRef}
          id={QUEUE_LIST_ID}
          role="listbox"
          aria-label="Attention"
          tabIndex={-1}
          // Focusable so `Alt+1` has a target, and it *shows* the focus —
          // "which list are my arrows driving" has to be answerable by looking,
          // or a focus model is just a hidden mode.
          className="min-h-0 flex-1 overflow-y-auto outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
        >
          {rows.length === 0 ? (
            <Empty
              hint={
                firstSnapshot
                  ? hidden > 0
                    ? `${hidden} session(s) hidden — none of them is under tmux`
                    : prefs.scope === "needs_you"
                      ? "nothing needs you — try the “all” scope"
                      : "no sessions match this filter"
                  : undefined
              }
            >
              {firstSnapshot ? "nothing here" : "reading the first scan…"}
            </Empty>
          ) : (
            <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
              {rowVirtualizer.getVirtualItems().map((v) => {
                const r = display[v.index];
                return (
                  <div
                    key={r.kind === "header" ? r.key : r.session.id}
                    ref={rowVirtualizer.measureElement}
                    data-index={v.index}
                    style={{ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${v.start}px)` }}
                  >
                    {r.kind === "header" ? (
                      <button
                        type="button"
                        onClick={() =>
                          useStore.setState((st) => ({
                            foldedGroups: st.foldedGroups.includes(r.key)
                              ? st.foldedGroups.filter((x) => x !== r.key)
                              : [...st.foldedGroups, r.key],
                          }))
                        }
                        title={
                          r.stale
                            ? "not heard from — these rows are last known, not current, and rank below everything live"
                            : undefined
                        }
                        data-stale={r.stale || undefined}
                        className={cn(
                          "flex w-full items-center gap-1 bg-[var(--bg-faint)] py-0.5 pr-2 text-left outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2",
                          r.depth === 1 ? "pl-4" : "pl-2",
                          r.stale && "opacity-60",
                        )}
                      >
                        {r.collapsed ? (
                          <ChevronRight size={11} className="shrink-0 text-[var(--dim)]" />
                        ) : (
                          <ChevronDown size={11} className="shrink-0 text-[var(--dim)]" />
                        )}
                        {r.key.startsWith("machine:") && (
                          <Monitor size={11} className="shrink-0 text-[var(--dim)]" />
                        )}
                        <span className="truncate text-xs font-semibold text-[var(--text-strong)]">
                          {r.label}
                        </span>
                        <Dim className="shrink-0 text-2xs">
                          {r.count} session{r.count === 1 ? "" : "s"}
                          {/* Only when there are any: a repo that needs nothing
                              should read as quiet, not as a zero to check. */}
                          {r.needing > 0 && `, ${r.needing} need you`}
                          {r.stale && " — not heard from"}
                        </Dim>
                      </button>
                    ) : (
                      <QueueRow item={r.item} session={r.session} />
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
      </ZoomPane>
      <InfoDock />
      </div>
      <div
        onMouseDown={onDrag}
        className="w-1 cursor-col-resize hover:bg-[var(--blue)]"
        title="drag to resize"
      />
    </div>
  );
}

export { Tag };
