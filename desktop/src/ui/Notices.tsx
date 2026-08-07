/**
 * Things that went wrong: a toast that leaves, and a log that does not.
 *
 * The old strip stayed until dismissed by hand. That is right for something you
 * must act on and wrong for almost everything the daemon refuses — selecting a
 * session outside a repository is not an *error you fix*, it is a fact, and a
 * banner you have to click through to keep working is the window nagging you
 * about your own choice.
 *
 * So: it announces itself, it goes away on its own, and the count in the status
 * bar is there if you want to look. Nothing waits for you.
 */

import { useEffect, useState } from "react";
import { AlertTriangle, Info, Trash2, X } from "lucide-react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Empty } from "@/ui/primitives";
import { stamp } from "@/lib/format";

/** How long a toast stays. Long enough to read a sentence twice. */
const TOAST_MS = 15_000;

export function Toasts() {
  const notices = useStore((s) => s.notices);
  const [, tick] = useState(0);

  // One timer, for the next expiry — not one per toast, and not a poll. When
  // nothing is live this schedules nothing at all.
  useEffect(() => {
    const now = Date.now();
    const live = notices.filter((n) => now - n.at < TOAST_MS);
    if (live.length === 0) return;
    const soonest = Math.min(...live.map((n) => n.at + TOAST_MS - now));
    const t = window.setTimeout(() => tick((x) => x + 1), Math.max(50, soonest));
    return () => window.clearTimeout(t);
  }, [notices]);

  const now = Date.now();
  // Newest at the bottom, nearest the status bar it came from, and capped so a
  // burst cannot cover the window it is reporting about.
  const live = notices.filter((n) => now - n.at < TOAST_MS).slice(-3);
  if (live.length === 0) return null;

  return (
    <div className="pointer-events-none absolute right-3 bottom-9 z-40 flex w-80 flex-col gap-1">
      {live.map((n) => (
        <div
          key={n.id}
          // Severity is the border and the glyph, not the words. A success
          // reported in red teaches you to read red as "look at this" rather
          // than as "something is wrong", and then a real failure has nothing
          // left to say with.
          className={cn(
            "pointer-events-auto flex items-start gap-2 rounded-md border bg-[var(--bg-raised)] px-2 py-1.5 shadow-[var(--elev-2)]",
            n.kind === "error" ? "border-[var(--red)]" : "border-[var(--border)]",
          )}
        >
          {n.kind === "error" ? (
            <AlertTriangle size={12} className="mt-0.5 shrink-0 text-[var(--red)]" />
          ) : (
            <Info size={12} className="mt-0.5 shrink-0 text-[var(--dim)]" />
          )}
          <span className="min-w-0 flex-1 text-xs text-[var(--text)]">{n.text}</span>
          <button
            type="button"
            title="dismiss — it stays in the log"
            onClick={() => useStore.setState({ notices: notices.filter((x) => x.id !== n.id) })}
            className="shrink-0 text-[var(--dim)] outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
          >
            <X size={11} />
          </button>
        </div>
      ))}
    </div>
  );
}

/** The count in the status bar. Absent when there is nothing to report. */
export function NoticeButton() {
  const notices = useStore((s) => s.notices);
  const unseen = notices.filter((n) => !n.seen).length;
  if (notices.length === 0) return null;

  return (
    <button
      type="button"
      title={unseen > 0 ? `${unseen} unread of ${notices.length}` : `${notices.length} in the log`}
      onClick={() => {
        useStore.setState({ noticesOpen: true });
        useStore.getState().markNoticesSeen();
      }}
      className="flex shrink-0 items-center gap-1 rounded-sm px-1 outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:bg-[var(--state-hover)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
      style={{ color: unseen > 0 ? "var(--red)" : "var(--dim)" }}
    >
      <AlertTriangle size={11} />
      <span className="text-2xs">{unseen > 0 ? unseen : notices.length}</span>
    </button>
  );
}

export function NoticesWindow() {
  const open = useStore((s) => s.noticesOpen);
  const notices = useStore((s) => s.notices);
  const clear = useStore((s) => s.clearNotices);
  if (!open) return null;

  return (
    <Dialog
      title="Warnings"
      subtitle={`${notices.length} recorded`}
      onClose={() => useStore.setState({ noticesOpen: false })}
    >
      <div className="p-2">
        {notices.length === 0 ? (
          <Empty>nothing has gone wrong</Empty>
        ) : (
          <>
            <div className="mb-1 flex justify-end">
              <Button variant="outline" size="sm" onClick={clear}>
                <Trash2 size={11} /> clear
              </Button>
            </div>
            {[...notices].reverse().map((n) => (
              <div key={n.id} className="flex gap-2 border-b border-[var(--border)] px-1 py-1 last:border-0">
                <Dim className="w-20 shrink-0 text-2xs">{stamp(n.at)}</Dim>
                <span className="min-w-0 flex-1 text-xs">{n.text}</span>
              </div>
            ))}
          </>
        )}
      </div>
    </Dialog>
  );
}
