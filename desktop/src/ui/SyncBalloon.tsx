/**
 * The balloon that says how a sync went. `R-J109`, `lib/gitSync.ts`.
 *
 * Asked 2026-09-25: *"a little popup balloon at the bottom corner to show the
 * status of the git command … once git sync complete, popup a little nice info
 * box saying sync complete (and if there is any warn/error msg)"*.
 *
 * It stands in the toasts' corner, in their stack, so the two can never cover
 * each other — and unlike a toast it is there from the press: *syncing* while
 * git runs, then *complete*, *complete, with warnings* or *failed*. Severity is
 * the glyph and the border, the toasts' rule; the words stay plain.
 */

import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, Loader2, X, XCircle } from "lucide-react";
import { cn } from "@/lib/cn";
import { openGit } from "@/lib/gitOps";
import { expiresAt, headline, moved, standing, type SyncRun } from "@/lib/gitSync";
import { useStore } from "@/store";
import { interactive } from "@/ui/styles";

/** How many of git's lines a balloon shows before pointing at the Console. */
const LINES = 4;

/** Seconds, to one place under ten: `0.8s`, `4.2s`, `12s`. */
function took(ms: number): string {
  const s = ms / 1000;
  return s < 10 ? `${s.toFixed(1)}s` : `${Math.round(s)}s`;
}

export function SyncBalloons() {
  const runs = useStore((s) => s.syncs);
  const [tick, setTick] = useState(0);
  const now = Date.now();
  const live = runs.filter((r) => !r.dismissed && expiresAt(r) > now);
  const running = live.some((r) => r.phase === "running");

  // Wake for the next balloon to go — and, while one runs, once a second for
  // its clock. Rescheduled on every wake, so the second of two balloons goes
  // on time too.
  useEffect(() => {
    const at = Date.now();
    const next = Math.min(...runs.filter((r) => !r.dismissed).map(expiresAt).filter((t) => t > at));
    const wait = Math.min(running ? 1000 : Infinity, Number.isFinite(next) ? next - at : Infinity);
    if (!Number.isFinite(wait)) return;
    const t = window.setTimeout(() => setTick((x) => x + 1), Math.max(50, wait));
    return () => window.clearTimeout(t);
  }, [runs, running, tick]);

  return (
    <>
      {live.map((r) => (
        <Balloon key={r.id} run={r} now={now} />
      ))}
    </>
  );
}

function Balloon({ run, now }: { run: SyncRun; now: number }) {
  const dismiss = () =>
    useStore.setState((s) => ({ syncs: s.syncs.map((r) => (r.id === run.id ? { ...r, dismissed: true } : r)) }));
  const trouble = run.phase === "failed" ? "failed" : run.phase === "done" && run.warnings.length > 0 ? "warned" : null;
  const elapsed = took((run.endedAt ?? now) - run.startedAt);
  const shownUpdates = run.updates.slice(0, Math.max(0, LINES - run.warnings.length));
  const extra = run.updates.length + run.warnings.length > LINES || run.phase === "failed";

  return (
    <div
      role="status"
      aria-label="git sync"
      className={cn(
        "pointer-events-auto flex items-start gap-2 rounded-md border bg-[var(--bg-raised)] px-2 py-1.5 shadow-[var(--elev-2)]",
        trouble === "failed" ? "border-[var(--red)]" : trouble === "warned" ? "border-[var(--amber)]" : "border-[var(--border)]",
      )}
    >
      {run.phase === "running" ? (
        <Loader2 size={12} className="mt-0.5 shrink-0 animate-spin text-[var(--blue)]" />
      ) : trouble === "failed" ? (
        <XCircle size={12} className="mt-0.5 shrink-0 text-[var(--red)]" />
      ) : trouble === "warned" ? (
        <AlertTriangle size={12} className="mt-0.5 shrink-0 text-[var(--amber)]" />
      ) : (
        <CheckCircle2 size={12} className="mt-0.5 shrink-0 text-[var(--green)]" />
      )}
      <div className="min-w-0 flex-1 text-xs text-[var(--text)]">
        <div className="truncate font-medium">{headline(run)}</div>
        <div className="text-2xs text-[var(--dim)]">
          {run.phase === "running"
            ? `git fetch --all --prune · ${elapsed}`
            : run.phase === "done"
              ? `${standing(run)} · ${moved(run)} · ${elapsed}`
              : elapsed}
        </div>
        {run.phase === "failed" && run.error && (
          <pre className="mt-1 max-h-24 overflow-auto font-mono text-2xs whitespace-pre-wrap text-[var(--red)]">
            {run.error}
          </pre>
        )}
        {run.warnings.length > 0 && (
          <ul aria-label="warnings" className="mt-1 font-mono text-2xs text-[var(--amber)]">
            {run.warnings.slice(0, LINES).map((w, i) => (
              <li key={i} className="truncate" title={w}>
                {w}
              </li>
            ))}
          </ul>
        )}
        {shownUpdates.length > 0 && (
          <ul aria-label="ref updates" className="mt-1 font-mono text-2xs text-[var(--dim)]">
            {shownUpdates.map((u, i) => (
              <li key={i} className="truncate" title={u}>
                {u}
              </li>
            ))}
          </ul>
        )}
        {extra && (
          <button
            type="button"
            onClick={() => openGit("console")}
            className={cn("mt-1 rounded-sm text-2xs text-[var(--blue)] hover:underline", interactive)}
          >
            the whole of it, in the Git console
          </button>
        )}
      </div>
      <button
        type="button"
        title={run.phase === "running" ? "hide — the sync carries on" : "dismiss"}
        onClick={dismiss}
        className={cn("shrink-0 rounded-sm text-[var(--dim)] hover:text-[var(--text)]", interactive)}
      >
        <X size={11} />
      </button>
    </div>
  );
}
