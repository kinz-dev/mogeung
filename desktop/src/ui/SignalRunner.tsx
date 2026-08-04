/**
 * The signal runner. `R-E2`, and `R-E5`'s coverage.
 *
 * Pillar E's question is "did the thing the agent said actually hold up", and
 * the observed half — `verify_runs` and `claims`, scraped from the transcript —
 * only reports what the agent happened to run. This is the half where **you**
 * ask: one command per repository, stored by the daemon, run when you press the
 * button.
 *
 * **Never on its own.** Not on a scan, not when a session goes quiet, not on
 * open. mogeung observes; a tool that ran your test suite because it felt like
 * it would be spending your machine and your tokens on its own initiative, and
 * that is the line ADR-0003 draws. The button is the whole consent model, and
 * the label says so.
 */

import { useEffect, useState } from "react";
import { Play } from "lucide-react";
import { useStore } from "@/store";
import { Button, Dim, Input, Mono } from "@/ui/primitives";
import { stamp } from "@/lib/format";

export function SignalRunner({ sessionId, repo }: { sessionId: string; repo: string }) {
  const state = useStore((s) => s.signals[repo]);
  const send = useStore((s) => s.send);
  const [edit, setEdit] = useState("");
  const [touched, setTouched] = useState(false);
  const [tailOpen, setTailOpen] = useState(false);

  // One ask per repo, from the render — the same "one door" rule the explorer
  // follows. Guarded on the slot existing rather than on a local flag, so a
  // reconnect that clears the store re-asks.
  useEffect(() => {
    if (!state) send({ cmd: "fetch_signal", repo });
  }, [repo, state, send]);

  // The daemon's answer seeds the box, but only until you start typing: a
  // broadcast landing mid-edit must not overwrite what you are in the middle
  // of, which is the same rule the label editor follows.
  useEffect(() => {
    if (!touched) setEdit(state?.command ?? "");
  }, [state?.command, touched]);

  const last = state?.last ?? null;
  const running = state?.running ?? false;
  const canRun = !!state?.command && !running;

  return (
    <>
      <div className="mt-2 px-3 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
        Signal
      </div>
      <div className="px-3 py-1">
        <Dim className="mb-1 block text-2xs">
          runs only when you click, never on its own
        </Dim>
        <div className="flex items-center gap-1">
          <Input
            value={edit}
            mono
            onChange={(v) => {
              setTouched(true);
              setEdit(v);
            }}
            placeholder="e.g. cargo test --workspace"
            onKeyDown={(e) => {
              if (e.key !== "Enter") return;
              send({ cmd: "set_signal_command", repo, command: edit });
              setTouched(false);
            }}
          />
          <Button
            variant="outline"
            onClick={() => {
              send({ cmd: "set_signal_command", repo, command: edit });
              setTouched(false);
            }}
          >
            save
          </Button>
          <Button
            variant="outline"
            disabled={!canRun}
            title={
              running
                ? "still running"
                : state?.command
                  ? "run it now, in this session's repository"
                  : "save a command first"
            }
            onClick={() => send({ cmd: "run_signal", session_id: sessionId })}
          >
            <Play size={11} /> {running ? "running…" : "run"}
          </Button>
        </div>
      </div>

      {last && (
        <div className="px-3 pb-1">
          <div className="flex flex-wrap items-baseline gap-2">
            <span
              className={
                last.exit === 0 ? "text-xs text-[var(--green)]" : "text-xs text-[var(--red)]"
              }
            >
              {/* `null` is not a zero. A command that never ran and a command
                  that ran and passed are opposite answers. */}
              {last.exit === null ? "did not run" : last.exit === 0 ? "exited ok" : `exited ${last.exit}`}
            </span>
            <Dim className="text-2xs">
              <Mono>{last.command}</Mono> · {last.secs}s · {stamp(last.started_at)}
            </Dim>
          </div>

          {last.tail && (
            <>
              <button
                type="button"
                onClick={() => setTailOpen(!tailOpen)}
                className="mt-1 rounded-sm text-2xs text-[var(--dim)] outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
              >
                {tailOpen ? "▾" : "▸"} output tail
              </button>
              {tailOpen && (
                <pre className="mt-1 max-h-64 overflow-auto rounded-sm border border-[var(--border)] bg-[var(--bg)] p-1 font-mono text-2xs whitespace-pre-wrap">
                  {last.tail}
                </pre>
              )}
            </>
          )}

          {/* `R-E5`. Absent data says so: a made-up zero would read as "nothing
              is covered", which is a different and much louder claim. */}
          {!last.coverage || last.coverage.length === 0 ? (
            <Dim className="mt-1 block text-2xs">coverage on changed lines: no data</Dim>
          ) : (
            <>
              <Dim className="mt-1 block text-2xs">coverage on changed lines</Dim>
              {last.coverage.slice(0, 20).map((fc) => {
                const total = fc.changed_covered + fc.changed_missed;
                return (
                  <div key={fc.path} className="flex items-baseline gap-2">
                    <Mono className="truncate text-2xs">{fc.path}</Mono>
                    <span
                      className="shrink-0 text-2xs"
                      style={{ color: fc.changed_missed === 0 ? "var(--green)" : "var(--amber)" }}
                    >
                      {fc.changed_covered}/{total} covered
                    </span>
                  </div>
                );
              })}
            </>
          )}
        </div>
      )}
    </>
  );
}
