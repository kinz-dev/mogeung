/**
 * Run it yourself, and watch. `R-N5`.
 *
 * The step the verification loop was missing. `R-E1` records the `cargo test`
 * the agent ran, `R-E3` binds *"tests pass"* to that evidence or visibly to
 * none — and until now the only way to actually check was to leave. This pane
 * is the other half.
 *
 * **Three rules from the ADRs are visible in this file**, and each of them is
 * the kind of thing a later tidy-up would quietly undo:
 *
 * 1. Nothing here sends a command. A run request names a configuration **by
 *    id** ([ADR-0025](../../../docs/decisions/0025-run-a-process-you-named-never-an-agent.md)
 *    clause 1), which is why the port is not a shell.
 * 2. An entry mogeung cannot run is **still listed**, with the reason
 *    ([ADR-0026](../../../docs/decisions/0026-other-peoples-run-configurations.md)) —
 *    a configuration that silently vanishes reads as *"mogeung did not find
 *    it"*.
 * 3. Environment **values are never rendered** until asked for by name, one at
 *    a time. `R-N6`, and the sweep found a real API key in a real repository.
 */

import { useEffect, useMemo, useState } from "react";
import { Play, Square, Eye, AlertTriangle } from "lucide-react";
import { useStore, usePaneBinding } from "@/store";
import { Empty, Dim, Badge } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { interactive, disabled as disabledStyle } from "@/ui/styles";
import { runPassed, type Run, type RunConfig } from "@/wire/types";

export function RunPane() {
  const { id: sessionId } = usePaneBinding();
  const send = useStore((s) => s.send);
  const configs = useStore((s) => (sessionId ? s.runConfigs[sessionId] : undefined));
  const allowed = useStore((s) => s.runsAllowed);
  const runs = useStore((s) => s.runs);
  const [openRun, setOpenRun] = useState<string | null>(null);

  // Re-read on every selection change rather than caching: a `launch.json` is
  // edited between one click and the next, and a stale list is the same
  // complaint in another costume.
  useEffect(() => {
    if (sessionId) send({ cmd: "fetch_run_configs", session_id: sessionId });
  }, [sessionId, send]);

  const mine = useMemo(
    () => runs.filter((r) => r.session_id === sessionId),
    [runs, sessionId],
  );

  if (!sessionId) return <Empty>select a session</Empty>;

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      {!allowed && (
        <div className="flex items-start gap-2 border-b border-[var(--border)] px-3 py-2">
          <AlertTriangle size={14} className="mt-0.5 shrink-0 text-[var(--amber)]" />
          <p className="text-xs text-[var(--dim)]">
            This daemon is bound past loopback and was started without{" "}
            <code className="font-mono">--allow-run</code>, so it will not start processes.
            That is ADR-0025 clause 4 — anyone who can reach the port could otherwise cause
            code to run on that machine.
          </p>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {configs === undefined ? (
          <Empty>reading this repository…</Empty>
        ) : configs.length === 0 ? (
          <Empty hint="mogeung infers runs from the manifests a project already has — a Cargo.toml, a package.json, a pyproject.toml. This directory has none it recognises.">
            nothing to run here
          </Empty>
        ) : (
          <ul>
            {configs.map((c) => (
              <ConfigRow
                key={c.id}
                config={c}
                sessionId={sessionId}
                allowed={allowed}
                runs={mine.filter((r) => r.config_id === c.id)}
                onOpen={setOpenRun}
              />
            ))}
          </ul>
        )}
      </div>

      {openRun && <Output runId={openRun} onClose={() => setOpenRun(null)} />}
    </div>
  );
}

function ConfigRow({
  config,
  sessionId,
  allowed,
  runs,
  onOpen,
}: {
  config: RunConfig;
  sessionId: string;
  allowed: boolean;
  runs: Run[];
  onOpen: (id: string) => void;
}) {
  const send = useStore((s) => s.send);
  const live = runs.find((r) => r.state === "running");
  const last = runs[0];

  return (
    <li className="border-b border-[var(--border)] px-3 py-2">
      <div className="flex items-center gap-2">
        {live ? (
          <button
            type="button"
            title="stop this run, and everything it started"
            className={cn(interactive, "shrink-0 rounded-sm p-1 text-[var(--red)]")}
            onClick={() => send({ cmd: "run_stop", run_id: live.id })}
          >
            <Square size={13} />
          </button>
        ) : (
          <button
            type="button"
            // **The request names an id.** ADR-0025 clause 1 — there is no
            // command on this line, and there must never be one.
            onClick={() => send({ cmd: "run_start", session_id: sessionId, config_id: config.id })}
            disabled={!!config.unrunnable || !allowed}
            title={config.unrunnable ?? "run it"}
            className={cn(
              interactive,
              disabledStyle,
              "shrink-0 rounded-sm p-1",
              config.unrunnable || !allowed ? "text-[var(--dim)]" : "text-[var(--green)]",
            )}
          >
            <Play size={13} />
          </button>
        )}

        <span className="min-w-0 flex-1 truncate text-sm text-[var(--text-strong)]">
          {config.name}
        </span>

        {/* ADR-0026 named the cost of promoting detection: a wrong inference is
            a first impression. So a guess says it is one. */}
        <Badge color={config.origin === "detected" ? "var(--dim)" : "var(--blue)"}>
          {config.origin === "detected" ? "inferred" : "launch.json"}
        </Badge>

        {last && <Verdict run={last} onOpen={onOpen} />}
      </div>

      <div className="mt-0.5 flex items-center gap-2 pl-7">
        <code className="truncate font-mono text-2xs text-[var(--dim)]">
          {config.program} {config.args.join(" ")}
          {config.dir && <span className="opacity-70"> ({config.dir})</span>}
        </code>
      </div>

      {config.env_keys.length > 0 && (
        <div className="mt-1 flex flex-wrap items-center gap-1 pl-7">
          {config.env_keys.map((k) => (
            <EnvChip key={k} sessionId={sessionId} configId={config.id} name={k} />
          ))}
        </div>
      )}

      {/* Hide nothing: listed, with the reason, rather than dropped. */}
      {config.unrunnable && (
        <p className="mt-1 pl-7 text-2xs text-[var(--dim)]">{config.unrunnable}</p>
      )}
    </li>
  );
}

/**
 * An environment variable, name shown and value not. `R-N6`.
 *
 * Revealing is per value and deliberate — a control that showed the whole block
 * would make *"show me this one"* indistinguishable from *"print every
 * secret"*, and the sweep behind ADR-0026 found a live API key in exactly this
 * position in a real repository.
 */
function EnvChip({
  sessionId,
  configId,
  name,
}: {
  sessionId: string;
  configId: string;
  name: string;
}) {
  const send = useStore((s) => s.send);
  const value = useStore((s) => s.revealedEnv[`${configId}${name}`]);
  return (
    <button
      type="button"
      title={value ? name : `reveal ${name}`}
      onClick={() =>
        !value && send({ cmd: "reveal_run_env", session_id: sessionId, config_id: configId, key: name })
      }
      className={cn(interactive, "flex items-center gap-1 rounded-sm border border-[var(--border)] px-1 py-0.5 font-mono text-2xs text-[var(--dim)]")}
    >
      {!value && <Eye size={9} />}
      {name}
      <span className="opacity-70">={value ?? "••••••"}</span>
    </button>
  );
}

/** What happened, in the one shape `R-N7` allows: a fact, not a verdict. */
function Verdict({ run, onOpen }: { run: Run; onOpen: (id: string) => void }) {
  const passed = runPassed(run);
  const label =
    run.state === "running"
      ? "running"
      : run.state === "stopped"
        ? "stopped"
        : passed
          ? "passed"
          : `exit ${run.exit_code ?? "?"}`;
  return (
    <button
      type="button"
      onClick={() => onOpen(run.id)}
      className={cn(
        interactive,
        "shrink-0 rounded-sm px-1.5 py-0.5 text-2xs",
        run.state === "running" && "text-[var(--blue)]",
        // **Stopped is neither.** Nobody let it finish, so it is not coloured
        // as a failure — that would be the window asserting something it does
        // not know.
        run.state === "stopped" && "text-[var(--dim)]",
        passed === true && "text-[var(--green)]",
        passed === false && "text-[var(--red)]",
      )}
    >
      {label}
    </button>
  );
}

function Output({ runId, onClose }: { runId: string; onClose: () => void }) {
  const send = useStore((s) => s.send);
  const lines = useStore((s) => s.runOutput[runId]);
  const run = useStore((s) => s.runs.find((r) => r.id === runId));

  useEffect(() => {
    send({ cmd: "fetch_run_output", run_id: runId });
  }, [runId, send]);

  return (
    <div className="flex h-1/2 min-h-0 flex-col border-t border-[var(--border)]">
      <div className="flex items-center gap-2 px-3 py-1">
        <span className="truncate font-mono text-2xs text-[var(--dim)]">{run?.command}</span>
        {/* Stated, never silent — the daemon's ring is bounded and says so. */}
        {run && run.dropped > 0 && (
          <Dim className="text-2xs">{run.dropped} earlier line(s) dropped</Dim>
        )}
        <button
          type="button"
          onClick={onClose}
          className={cn(interactive, "ml-auto rounded-sm px-1 text-2xs text-[var(--dim)] hover:text-[var(--text-strong)]")}
        >
          close
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-[var(--terminal-bg)] px-3 py-1">
        {(lines ?? []).map((l) => (
          <div
            key={l.seq}
            className={cn(
              "font-mono text-2xs whitespace-pre-wrap",
              l.stderr ? "text-[var(--red)]" : "text-[var(--text)]",
            )}
          >
            {l.text}
          </div>
        ))}
      </div>
    </div>
  );
}
