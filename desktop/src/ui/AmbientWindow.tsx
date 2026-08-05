/**
 * A big, glanceable board for a second monitor. `R-C5`.
 *
 * Readable across a room, which decides everything about it: only the sessions
 * that need you, only the reason and the label, nothing to click, and — when
 * there is nothing — a green *All clear* rather than an empty list. An ambient
 * display that shows an empty list is one you have to walk over and inspect,
 * which is the opposite of the point.
 */

import { useStore } from "@/store";
import { needsHuman, sessionLabel } from "@/wire/types";
import { reasonColor, useVisibleQueue } from "@/ui/QueuePanel";
import { reasonLabel } from "@/wire/types";
import { fmtDur, secsSince } from "@/lib/format";
import { X } from "lucide-react";
import { IconButton } from "@/ui/primitives";

export function AmbientWindow() {
  // The board's data subscriptions live in `Board`, mounted only while open —
  // otherwise a closed board still recomputes the visible queue on every
  // session update, forever.
  const open = useStore((s) => s.ambient);
  if (!open) return null;
  return <Board />;
}

function Board() {
  const sessions = useStore((s) => s.sessions);
  const rows = useVisibleQueue();

  const needing = rows.filter((r) => needsHuman(r.item.reason));
  const live = Object.values(sessions).filter((s) => s.alive).length;
  const now = Date.now();

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-[var(--bg)]">
      <div className="flex shrink-0 items-center justify-end p-2">
        <IconButton title="close the ambient board" onClick={() => useStore.setState({ ambient: false })}>
          <X size={16} />
        </IconButton>
      </div>
      {needing.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-2">
          <div className="text-6xl font-semibold text-[var(--green)]">All clear</div>
          <div className="text-xl text-[var(--dim)]">{live} live session(s) working</div>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">
          {needing.map(({ item, session }) => (
            <div
              key={session.id}
              className="mb-3 flex items-center gap-4 rounded-md border border-[var(--border)] px-4 py-3"
            >
              <span
                className="shrink-0 text-3xl font-semibold"
                style={{ color: reasonColor(item.reason) }}
              >
                {reasonLabel(item.reason)}
              </span>
              <span className="min-w-0 flex-1 truncate text-3xl text-[var(--text-strong)]">
                {sessionLabel(session)}
              </span>
              <span className="shrink-0 text-2xl text-[var(--dim)]">
                {fmtDur(secsSince(session.last_event_at, now))}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
