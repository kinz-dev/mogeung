/**
 * Everything about the selected session that is not its diff or its words.
 */

import { useStore, useSelectedSession } from "@/store";
import { Chip, Dim, Empty, Mono, PaneHeader } from "@/ui/primitives";
import { compact, fmtDur, num, secsSince, stamp } from "@/lib/format";
import { repoName, sessionLabel, sourceColor, sourceLabel, unverified } from "@/wire/types";
import { SignalRunner } from "@/ui/SignalRunner";

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3 px-3 py-0.5 text-sm">
      <Dim className="w-32 shrink-0">{label}</Dim>
      <div className="min-w-0 flex-1 break-words">{children}</div>
    </div>
  );
}

export function InfoPane() {
  const s = useSelectedSession();
  const subagents = useStore((st) => (st.selected ? st.subagents[st.selected] : undefined));
  const send = useStore((st) => st.send);

  if (!s) return <Empty>select a session</Empty>;

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <PaneHeader title="Info" />
      <div className="min-h-0 flex-1 overflow-y-auto py-1">
        <Field label="label">{sessionLabel(s)}</Field>
        <Field label="id"><Mono className="text-xs">{s.id}</Mono></Field>
        <Field label="source">
          <Chip color={sourceColor(s.source)}>{sourceLabel(s.source)}</Chip>
        </Field>
        <Field label="state">
          {s.alive ? (s.live_status ?? "live") : "ended"}
          {s.pid !== null && <Dim> · pid {s.pid}</Dim>}
        </Field>
        <Field label="repo">{repoName(s)}</Field>
        <Field label="root"><Mono className="text-xs">{s.repo_root ?? s.cwd}</Mono></Field>
        {s.git_branch && <Field label="branch">{s.git_branch}</Field>}
        {s.base_sha && <Field label="diff base"><Mono className="text-xs">{s.base_sha.slice(0, 12)}</Mono></Field>}
        <Field label="started">{stamp(s.started_at)} <Dim>({fmtDur(secsSince(s.started_at))} ago)</Dim></Field>
        <Field label="last event">{stamp(s.last_event_at)} <Dim>({fmtDur(secsSince(s.last_event_at))} ago)</Dim></Field>
        <Field label="turns">{num(s.turns)} · {num(s.tool_calls)} tool calls</Field>
        <Field label="tokens">
          {compact(s.tokens_in)} in · {compact(s.tokens_out)} out
          <Dim> — tokens; cost is on Analytics (ADR-0024)</Dim>
        </Field>
        <Field label="changed">
          <span className="text-[var(--add-fg)]">+{s.insertions}</span>{" "}
          <span className="text-[var(--del-fg)]">−{s.deletions}</span> in {s.files_changed} file(s)
        </Field>
        {s.version && <Field label="cli version">{s.version}</Field>}
        {s.tmux_target && <Field label="tmux"><Mono className="text-xs">{s.tmux_target}</Mono></Field>}
        {s.error && <Field label="error"><span className="text-[var(--red)]">{s.error}</span></Field>}
        {s.limit_hit_at && (
          <Field label="rate limit">
            <span className="text-[var(--amber)]">
              hit {stamp(s.limit_hit_at)}
              {s.limit_resets ? ` — resets ${s.limit_resets}` : ""}
            </span>
          </Field>
        )}

        {/* Pillar E's two halves, adjacent on purpose: what the agent
            happened to run, and what you can ask for yourself. */}
        {unverified(s) && (
          <Field label="verification">
            <span className="text-[var(--amber)]">
              no build, test or typecheck completed in this session
            </span>
          </Field>
        )}

        {s.verify_runs.length > 0 && (
          <>
            <div className="mt-2 px-3 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
              Verification
            </div>
            {s.verify_runs.map((r, i) => (
              <Field key={i} label={r.kind}>
                <Mono className="text-xs">{r.command}</Mono>{" "}
                {r.ok === true && <span className="text-[var(--green)]">passed</span>}
                {r.ok === false && <span className="text-[var(--red)]">failed</span>}
                {r.ok === null && <Dim>no result on record</Dim>}
              </Field>
            ))}
          </>
        )}

        {s.claims.length > 0 && (
          <>
            <div className="mt-2 px-3 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
              Claims the agent made
            </div>
            {s.claims.map((c, i) => (
              <Field key={i} label={c.contradicted ? "contradicted" : c.kind}>
                <span className={c.contradicted ? "text-[var(--red)]" : undefined}>{c.text}</span>
                <Dim className="block text-xs">{c.evidence}</Dim>
              </Field>
            ))}
          </>
        )}

        <SignalRunner sessionId={s.id} repo={s.repo_root ?? s.cwd} />

        {s.touched_files.length > 0 && (
          <>
            <div className="mt-2 px-3 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
              Files touched ({s.touched_files.length})
            </div>
            {s.touched_files.slice(0, 200).map((f) => (
              <div key={f} className="px-3 py-px">
                <Mono className="text-xs text-[var(--blue)]">{f}</Mono>
              </div>
            ))}
          </>
        )}

        <div className="mt-2 px-3">
          <button
            type="button"
            onClick={() => send({ cmd: "fetch_subagents", session_id: s.id })}
            className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] rounded-sm border border-[var(--border)] px-1.5 py-0.5 text-2xs hover:border-[var(--border-hover)]"
          >
            load subagents
          </button>
        </div>
        {subagents?.map((n) => (
          <Field key={n.agent_id} label={n.agent_id.slice(0, 8)}>
            {n.lines} lines · {compact(n.tokens_out)} out
            {n.last_activity && <Dim> · {n.last_activity}</Dim>}
          </Field>
        ))}
      </div>
    </div>
  );
}
