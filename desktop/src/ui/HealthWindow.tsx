/**
 * What mogeung can and cannot see. `R-A4`.
 *
 * The whole of pillar A is the claim that this tool is trustworthy, and the
 * only honest way to make it is to publish what was **not** read. Everything
 * rests on two undocumented on-disk formats ([A4](../../docs/product/assumptions.md)),
 * so a line the parser did not recognise is a hole in the board — and a hole
 * nobody is told about is the failure the canary exists to prevent.
 */

import { useStore } from "@/store";
import { Button, Chip, Dim, Empty, Loading } from "@/ui/primitives";
import { Dialog } from "@/ui/Dialog";
import { humanBytes, type Alert, type PricingHealth } from "@/wire/types";
import { agentColor, agentSlots } from "@/lib/agentHealth";
import { num, stamp } from "@/lib/format";

/** Where the pricing page is, and the whole of the consent to read it. */
const PRICING_HOST = "platform.claude.com";

/**
 * Whether an alert means mogeung may be lying by omission — the daemon's
 * `Alert::is_urgent`. A skipped tail and an unrunnable run configuration are
 * stated limits, not holes.
 */
export function isUrgent(a: Alert): boolean {
  return a.kind !== "history_skipped" && a.kind !== "unknown_run_config_type";
}

export function alertMessage(a: Alert): string {
  switch (a.kind) {
    case "unknown_event_type":
      return `Unrecognised transcript event ${JSON.stringify(a.event_type)} seen ${a.count}× — mogeung is skipping it. The format may have changed.`;
    case "malformed_lines":
      return `${a.count} transcript line(s) could not be read as JSON. Anything they contained is missing from the board.`;
    case "version_changed":
      return `Claude Code is now ${a.to}, was ${a.from}. Formats move on upgrades — check the board still looks right.`;
    case "history_skipped":
      return `Session ${a.session_id.slice(0, 8)} was too large to read whole; ${humanBytes(a.skipped_bytes)} of earlier history was skipped and is not on the board.`;
    case "unknown_run_config_type":
      return `Run configuration type ${JSON.stringify(a.ty)} seen ${a.count}× — mogeung lists it but cannot run it.`;
    case "unpriced_model":
      return `Model ${JSON.stringify(a.model)} has no price in this build — its ${a.messages} message(s) are counted in tokens and missing from the equivalent cost.`;
    case "prices_differ":
      return `Anthropic's pricing page disagrees with this build's price table in ${a.count} place(s). The equivalent cost is using the build's table.`;
    default:
      // A kind from a daemon newer than this window. Said, not dropped: an
      // alert that renders as nothing is the failure this window exists for.
      return `An alert this window does not know how to describe: ${JSON.stringify(a)}`;
  }
}

/**
 * The price table, and whether anything checks it. `R-J124`.
 *
 * Off is the default and is said plainly, with the way to turn it on — which
 * opens the configuration with the line **added but unsaved**, so what would
 * be fetched, from where, is on screen before anything is.
 */
function Prices({ p }: { p: PricingHealth }) {
  const turnOn = () =>
    useStore.setState({
      showHealth: false,
      showConfig: true,
      configAppend: `refresh_pricing = "${PRICING_HOST}"`,
    });
  return (
    <div>
      <div className="mb-1 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">Prices</div>
      <div className="space-y-1 text-xs">
        <div>
          The equivalent cost uses this build's price table, read {p.built_in_as_of}.
        </div>
        {p.refusal && <div className="text-[var(--amber)]">{p.refusal}</div>}
        {!p.host ? (
          <div className="flex items-center gap-2">
            <Dim>Nothing checks it against Anthropic's pricing page.</Dim>
            <Button
              onClick={turnOn}
              title={`opens the configuration with refresh_pricing = "${PRICING_HOST}" added — unsaved, so you see it before anything is read`}
            >
              {p.refusal ? "fix the host…" : "check it weekly…"}
            </Button>
          </div>
        ) : (
          <>
            <Dim className="block">
              Read from {p.host} once a week and compared. A reading changes no price yet: that waits
              until four readings have shown the page can be trusted (feature 0050).
            </Dim>
            <div>
              {p.last_ok ? (
                <>
                  Last read {stamp(p.last_ok)} · {num(p.models_read)} models ·{" "}
                  {p.differences.length === 0 ? (
                    <span className="text-[var(--green)]">agrees with this build</span>
                  ) : (
                    <span className="text-[var(--amber)]">
                      {p.differences.length} difference{p.differences.length === 1 ? "" : "s"}
                    </span>
                  )}
                </>
              ) : (
                <Dim>Not read yet.</Dim>
              )}
            </div>
            {p.last_error && (
              <div className="text-[var(--red)]">
                The last attempt{p.last_attempt ? ` (${stamp(p.last_attempt)})` : ""} failed: {p.last_error}
              </div>
            )}
            {p.differences.map((d) => (
              <div key={d} className="rounded-sm border border-[var(--amber)] px-2 py-0.5">
                {d}
              </div>
            ))}
            {p.unmapped.length > 0 && (
              <Dim className="block text-2xs">
                On the page but matched to no published id, so not compared: {p.unmapped.join(", ")}
              </Dim>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <div className="rounded-sm border border-[var(--border)] bg-[var(--bg-raised)] px-2 py-1.5">
      <div className="text-base font-semibold" style={{ color: tone ?? "var(--text-strong)" }}>
        {value}
      </div>
      <Dim className="text-2xs">{label}</Dim>
    </div>
  );
}

export function HealthWindow() {
  const open = useStore((s) => s.showHealth);
  const health = useStore((s) => s.health);
  const send = useStore((s) => s.send);
  if (!open) return null;

  const close = () => useStore.setState({ showHealth: false });

  if (!health) {
    return (
      <Dialog title="Health" onClose={close}>
        <Loading what="asking the daemon" />
      </Dialog>
    );
  }

  const blind = health.lines_seen === 0 ? 0 : (health.lines_unknown + health.lines_malformed) / health.lines_seen;
  const urgent = health.alerts.filter(isUrgent).length;

  return (
    <Dialog
      title="Health"
      subtitle={health.last_scan ? `last scan ${stamp(health.last_scan)}` : "never scanned"}
      onClose={close}
      wide
    >
      <div className="space-y-3 p-3">
        <div
          className="rounded-sm px-2 py-1.5 text-sm"
          style={{
            background: urgent > 0 ? "var(--del-bg)" : "var(--add-bg)",
            color: urgent > 0 ? "var(--del-fg)" : "var(--add-fg)",
          }}
        >
          {health.lines_seen === 0
            ? "No transcript lines read yet."
            : urgent > 0
              ? `${urgent} thing(s) mogeung cannot see.`
              : "Reading everything it recognises."}
        </div>

        <div className="grid grid-cols-4 gap-2">
          <Stat label="sessions known" value={num(health.sessions_known)} />
          <Stat label="live now" value={num(health.sessions_live)} tone="var(--green)" />
          <Stat label="transcripts" value={num(health.transcripts_found)} />
          <Stat label="scans" value={num(health.scans)} />
        </div>

        <div>
          <div className="mb-1 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
            Every line accounted for
          </div>
          <div className="grid grid-cols-3 gap-2">
            <Stat label="parsed" value={num(health.lines_parsed)} tone="var(--green)" />
            <Stat label="ignored (known)" value={num(health.lines_ignored)} />
            <Stat label="barren" value={num(health.lines_barren)} />
            <Stat
              label="unknown type"
              value={num(health.lines_unknown)}
              tone={health.lines_unknown > 0 ? "var(--amber)" : undefined}
            />
            <Stat
              label="malformed"
              value={num(health.lines_malformed)}
              tone={health.lines_malformed > 0 ? "var(--red)" : undefined}
            />
            <Stat
              label="blind spot"
              value={`${(blind * 100).toFixed(2)}%`}
              tone={blind > 0 ? "var(--amber)" : undefined}
            />
          </div>
          <Dim className="mt-1 block text-2xs">
            Counted, not sampled — `R-A1` classifies every line so an unrecognised one is loud
            rather than silently dropped.
          </Dim>
        </div>

        {health.alerts.length > 0 && (
          <div>
            <div className="mb-1 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
              Alerts
            </div>
            <div className="space-y-1">
              {health.alerts.map((a, i) => (
                <div
                  key={i}
                  className="rounded-sm border border-[var(--border)] px-2 py-1 text-xs"
                  style={{ borderColor: isUrgent(a) ? "var(--amber)" : "var(--border)" }}
                >
                  {alertMessage(a)}
                </div>
              ))}
            </div>
          </div>
        )}

        {health.pricing && <Prices p={health.pricing} />}

        <div className="flex flex-wrap items-center gap-2">
          {health.current_version && <Chip color="var(--graph-0)">CLI {health.current_version}</Chip>}
          {agentSlots(health)
            .filter((a) => a.present)
            .map((a) => (
              <Chip key={a.source} color={agentColor(a.source)}>
                {a.source} · {num(a.threads)} session{a.threads === 1 ? "" : "s"}
                {a.error && " · unreadable"}
              </Chip>
            ))}
          {health.history_skipped_bytes > 0 && (
            <Chip color="var(--amber)">{humanBytes(health.history_skipped_bytes)} of history skipped</Chip>
          )}
          <button
            type="button"
            onClick={() => send({ cmd: "fetch_health" })}
            className="ml-auto rounded-sm border border-[var(--border)] px-2 py-0.5 text-2xs outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:border-[var(--border-hover)] focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
          >
            refresh
          </button>
        </div>

        {health.versions_seen.length > 1 && (
          <Dim className="block text-2xs">
            versions seen: {health.versions_seen.join(", ")} — formats move on upgrades
          </Dim>
        )}
        {health.lines_seen === 0 && <Empty>nothing has been read yet</Empty>}
      </div>
    </Dialog>
  );
}
