/**
 * The Event Contracts Game — the arcade's seventh, after tradermath.org's. The
 * events, the flow and the scoring live in
 * [`lib/eventContracts.ts`](../lib/eventContracts.ts), a game in progress in
 * [`lib/eventRun.ts`](../lib/eventRun.ts); this file draws it and keeps time.
 *
 * Three pages: a **lobby**, the **rounds** — the brief and your quote, then
 * the review — and the **results**. Escape walks back: from a round it stops
 * the game, from the results to the lobby, from the lobby to the arcade.
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { CircleCheck, CircleX, Scale3d, Users } from "lucide-react";
import { cn } from "@/lib/cn";
import { historyPlace } from "@/lib/arcade";
import { clockFrom, clockText } from "@/lib/arcadeClock";
import {
  CALIBRATION_SCALE,
  DEFAULT_EVENTS,
  LEVELS,
  ROUNDS,
  fillValue,
  mid,
  oddsText,
  type EventSettings,
  type Level,
} from "@/lib/eventContracts";
import { contract, eventOutcome, nextRound, quote, skip, startRun, stopRun, timedOut, type EventRound, type EventRun } from "@/lib/eventRun";
import { appendEventSession, eventStanding, loadEventSessions, perRound, toEventSession, type EventSessionRecord } from "@/lib/eventStats";
import { seeded, type Rand } from "@/lib/tradingGame";
import { ArcadeHeader, AutoFocus, BigButton, Choice, ClockSetting, Countdown, Key, Label, NumberBox, Stepper, Tile, fs } from "@/ui/arcadeKit";
import { TrendChart } from "@/ui/MentalMathHistory";

type Phase = "lobby" | "play" | "results";

const SETTINGS_KEY = "mogeung.events.settings";
const LEVEL_NAMES: Level[] = ["easy", "medium", "hard"];
const title = (s: string) => s[0].toUpperCase() + s.slice(1);
/** `+3.4`, `−1`, `0`. */
const pts = (n: number) => (n > 0 ? `+${n}` : n < 0 ? `−${Math.abs(n)}` : "0");
const tone = (n: number) => (n > 0 ? "text-[var(--green)]" : n < 0 ? "text-[var(--red)]" : "text-[var(--text-strong)]");

function loadSettings(): EventSettings {
  try {
    const raw = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "null") as Partial<EventSettings> | null;
    if (!raw || typeof raw !== "object") return DEFAULT_EVENTS;
    const n = raw.rounds;
    return {
      level: LEVEL_NAMES.includes(raw.level as Level) ? (raw.level as Level) : "easy",
      rounds: typeof n === "number" && Number.isInteger(n) && n >= ROUNDS[0] && n <= ROUNDS[1] ? n : DEFAULT_EVENTS.rounds,
      seconds: clockFrom(raw.seconds, DEFAULT_EVENTS.seconds),
    };
  } catch {
    return DEFAULT_EVENTS;
  }
}

export function EventGame({ onExit, onClose, seed }: { onExit: () => void; onClose: () => void; seed?: number }) {
  const [phase, setPhase] = useState<Phase>("lobby");
  const [settings, setSettings] = useState<EventSettings>(loadSettings);
  const [run, setRun] = useState<EventRun | null>(null);
  const [sessions, setSessions] = useState<EventSessionRecord[]>([]);
  const [unsaved, setUnsaved] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const rand = useRef<Rand>(seeded(seed ?? (Date.now() ^ 0xec)));

  useEffect(() => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Forgetting a setting is not worth an error.
    }
  }, [settings]);

  useEffect(() => {
    let live = true;
    void loadEventSessions().then((ss) => live && setSessions((mine) => [...ss, ...mine]));
    return () => {
      live = false;
    };
  }, []);

  const start = () => {
    const at = Date.now();
    setUnsaved(false);
    setNow(at);
    setRun(startRun(settings, rand.current, at));
    setPhase("play");
  };

  useEffect(() => {
    if (phase !== "play") return;
    const id = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(id);
  }, [phase]);

  useEffect(() => {
    if (!run || run.deadline === null || now < run.deadline) return;
    setRun((r) => (r ? timedOut(r) : r));
  }, [now, run]);

  useEffect(() => {
    if (phase !== "play" || !run || run.stage !== "over") return;
    const record = toEventSession(run);
    if (record) {
      setSessions((ss) => [...ss, record]);
      void appendEventSession(record).then((ok) => setUnsaved(!ok));
    }
    setPhase("results");
  }, [phase, run]);

  const escape = useRef(() => {});
  escape.current = () => {
    if (phase === "play") setRun((r) => (r ? stopRun(r) : r));
    else if (phase === "results") setPhase("lobby");
    else onExit();
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      e.preventDefault();
      escape.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const playing = phase === "play";
  return (
    <>
      <ArcadeHeader
        icon={<Scale3d size={20} className="text-[var(--green)]" />}
        title="event contracts"
        detail={playing && run ? `round ${run.i + 1} of ${run.contracts.length}` : null}
        onBack={playing ? null : onExit}
        onClose={playing ? () => escape.current() : onClose}
        closeTitle={playing ? "stop the game  (Esc)" : "close  (Esc)"}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1100px] flex-col px-[5vw] py-[3vh]">
          {phase === "lobby" && <Lobby settings={settings} onChange={setSettings} sessions={sessions} onStart={start} />}
          {phase === "play" && run && <Round run={run} now={now} rand={rand.current} onRun={setRun} />}
          {phase === "results" && run && (
            <Results run={run} sessions={sessions} unsaved={unsaved} onAgain={start} onLobby={() => setPhase("lobby")} />
          )}
        </div>
      </div>
    </>
  );
}

// --- lobby ----------------------------------------------------------------------

function Lobby({
  settings,
  onChange,
  sessions,
  onStart,
}: {
  settings: EventSettings;
  onChange: (s: EventSettings) => void;
  sessions: EventSessionRecord[];
  onStart: () => void;
}) {
  const rules = LEVELS[settings.level];
  const pick = (l: Level) => onChange({ ...settings, level: l, rounds: LEVELS[l].rounds, seconds: LEVELS[l].seconds });
  const st = useMemo(() => eventStanding(sessions), [sessions]);
  const small = fs(12, 1.7, 15);
  const startRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const t = setTimeout(() => startRef.current?.querySelector("button")?.focus(), 0);
    return () => clearTimeout(t);
  }, []);

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="text-[var(--dim)]">
        Read an event brief, work out a fair probability, and quote a two-sided market in cents. Noise traders pay you the
        spread; informed traders punish a wrong centre.
      </div>
      <div className="grid gap-[4vh] lg:grid-cols-2">
        <div className="flex flex-col gap-[2vh]">
          <div>
            <Label>difficulty</Label>
            <div className="flex flex-wrap gap-2">
              {LEVEL_NAMES.map((l) => (
                <Choice key={l} on={settings.level === l} onClick={() => pick(l)}>
                  {title(l)}
                </Choice>
              ))}
            </div>
            <div className="mt-2 text-[var(--dim)]" style={small}>
              The truth within ±{rules.band} points of your update · quotes up to {rules.maxWidth}¢ wide · about {rules.traders} traders
              a round, {Math.round(rules.informed * 100)}% of them informed.
            </div>
          </div>
          <div>
            <Label>rounds and clock — yours at any difficulty</Label>
            <Stepper label="Rounds" value={settings.rounds} range={ROUNDS} onChange={(v) => onChange({ ...settings, rounds: v })} />
            <ClockSetting
              label="Time per round"
              value={settings.seconds}
              fallback={rules.seconds}
              hint={settings.seconds !== rules.seconds ? `${title(settings.level)} is ${rules.seconds}s as it comes` : "running out closes the round as a skip does"}
              onChange={(v) => onChange({ ...settings, seconds: v })}
            />
          </div>
          <div ref={startRef} className="flex items-center gap-3">
            <BigButton solid onClick={onStart}>
              start game <Key>↵</Key>
            </BigButton>
            <span className="text-[var(--dim)]" style={small}>
              {settings.rounds} rounds · {clockText(settings.seconds)} each
            </span>
          </div>
        </div>
        <div className="flex flex-col gap-[2vh]">
          <div>
            <Label>your statistics</Label>
            {st.sessions === 0 ? (
              <div className="text-[var(--dim)]">No sessions yet — every one you finish is kept and charted here.</div>
            ) : (
              <>
                <div className="grid grid-cols-3 gap-2">
                  <Tile n={st.sessions} label={st.sessions === 1 ? "session" : "sessions"} />
                  <Tile n={st.best === null ? "—" : pts(st.best)} label="best, a round" />
                  <Tile n={st.latest === null ? "—" : pts(st.latest)} label="latest, a round" />
                  <Tile n={st.average === null ? "—" : pts(st.average)} label="average, a round" />
                  <Tile n={st.centred === null ? "—" : `${st.centred}%`} label="quotes well centred" />
                  <Tile n={st.widthLean === null ? "—" : `${pts(st.widthLean)}¢`} label="width against the balance" />
                </div>
                {st.scored.length > 1 && (
                  <div className="mt-[2vh]">
                    <TrendChart
                      values={st.scored.map(perRound)}
                      labels={st.scored.map((s) => `${new Date(s.at).toLocaleString()} · ${s.settings.level} · ${s.score} over ${s.rounds.length}`)}
                      color="var(--green)"
                      format={(n) => pts(Math.round(n * 10) / 10)}
                      label="score a round, session by session"
                    />
                  </div>
                )}
              </>
            )}
          </div>
          <details className="rounded-lg border border-[var(--border)] bg-[var(--bg-panel)] px-[1em] py-[0.6em]">
            <summary className="cursor-pointer font-semibold text-[var(--text-strong)]">How to play</summary>
            <HowToPlay />
          </details>
        </div>
      </div>
    </div>
  );
}

function HowToPlay() {
  const p = "mt-2 text-[var(--dim)]";
  return (
    <div style={fs(12, 1.8, 16)}>
      <p className={p}>
        <b>The contract.</b> Each round is one event. The contract pays 100¢ if it happens and 0¢ if it does not, so a
        fair price in cents is the probability in percent.
      </p>
      <p className={p}>
        <b>Update in odds.</b> The brief gives a base rate and, on most rounds, evidence with a likelihood ratio. Turn
        the base rate into odds, multiply by the ratio when the evidence points toward the event (divide when against),
        and turn it back: base 25% is 1:3; evidence 3× for makes 1:1 — fair value 50¢. The truth sits within the stated
        band either side.
      </p>
      <p className={p}>
        <b>Quote.</b> A bid and an ask in whole cents, no wider than the round allows. Your midpoint is your
        probability. <Key>↵</Key> sends, <Key>S</Key> stands aside — skipping costs nothing and scores nothing.
      </p>
      <p className={p}>
        <b>The flow.</b> Noise traders pay you the spread but walk away from a wide one. Informed traders know the truth
        and deal only when your quote leaves them an edge. Tight trades a lot and invites the informed; wide is safe
        and barely trades. The review says where the balance was.
      </p>
      <p className={p}>
        <b>Scoring.</b> P&amp;L points: your fills, with what you hold at the close marked at the true probability, a
        point for every 2¢. Calibration points: up to {CALIBRATION_SCALE} × how much closer your mid was than a coin
        flip's 50 — nothing at 50, a loss when you are further off than that.
      </p>
    </div>
  );
}

// --- a round ---------------------------------------------------------------------

function Brief({ run }: { run: EventRun }) {
  const b = contract(run).brief;
  return (
    <section className="rounded-xl border border-[var(--border)] bg-[var(--bg-panel)] p-[2.5vmin]" aria-label="the brief">
      <Label>event brief</Label>
      <div className="font-semibold text-[var(--text-strong)]" style={fs(20, 4.2, 40)}>
        {b.title}
      </div>
      <div className="mt-2 flex flex-col gap-1">
        <div>
          Base rate <b className="font-mono">{b.prior}%</b> <span className="text-[var(--dim)]">(odds {oddsText(b.prior)})</span>.
        </div>
        <div>
          {b.evidence && b.ratio ? (
            <>
              {b.evidence}. You judge it <b className="font-mono">{b.ratio}×</b> as likely if it {b.direction === "for" ? "happens" : "does not happen"}.
            </>
          ) : (
            "No fresh evidence on this one."
          )}
        </div>
        <div className="text-[var(--dim)]">The truth may sit up to ±{b.band} points from your update.</div>
      </div>
      <div className="mt-3 flex flex-wrap gap-2 text-[var(--dim)]" style={fs(11, 1.6, 14)}>
        <Chip>max width {b.maxWidth}¢</Chip>
        <Chip>
          <Users className="inline" style={{ width: "1em", height: "1em" }} /> {b.traders} traders
        </Chip>
        <Chip>pays 100¢ if it happens</Chip>
      </div>
    </section>
  );
}

function Chip({ children }: { children: ReactNode }) {
  return <span className="rounded-full border border-[var(--border)] px-[0.7em] py-[0.15em] font-mono">{children}</span>;
}

function Round({ run, now, rand, onRun }: { run: EventRun; now: number; rand: Rand; onRun: (r: EventRun) => void }) {
  const [bid, setBid] = useState("");
  const [ask, setAsk] = useState("");
  useEffect(() => {
    setBid("");
    setAsk("");
  }, [run.i]);
  const small = fs(12, 1.7, 15);
  const num = (v: string) => (v === "" || v === "-" ? NaN : Number(v));
  const send = () => onRun(quote(run, { bid: num(bid), ask: num(ask) }, rand, Date.now()));
  const stand = () => onRun(skip(run, Date.now()));
  const b = num(bid);
  const a = num(ask);
  const reviewed = run.stage === "review" ? run.rounds[run.rounds.length - 1] : null;
  const onKey = (k: string) => {
    if (k !== "s") return false;
    stand();
    return true;
  };

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex items-baseline justify-between text-[var(--dim)]" style={small}>
        <span>
          {run.settings.level} · round {run.i + 1} of {run.contracts.length}
        </span>
        <span aria-label="score so far">score so far {pts(run.rounds.reduce((t, x) => Math.round((t + x.score) * 10) / 10, 0))}</span>
      </div>
      <Brief run={run} />
      {run.stage === "quoting" && (
        <div className="flex flex-col gap-3 rounded-xl border border-[var(--border)] bg-[var(--bg-panel)] p-[2.5vmin]">
          <div>
            Your market, in cents{" "}
            <span className="text-[var(--dim)]" style={small}>
              <Key>↵</Key> sends · <Key>S</Key> stands aside
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <NumberBox label="bid" value={bid} onChange={(v) => setBid(v.replace(/-/g, ""))} onEnter={send} onKey={onKey} autoFocus width="4.5em" />
            <span className="text-[var(--dim)]">/</span>
            <NumberBox label="ask" value={ask} onChange={(v) => setAsk(v.replace(/-/g, ""))} onEnter={send} onKey={onKey} width="4.5em" />
            <BigButton solid onClick={send}>
              quote <Key>↵</Key>
            </BigButton>
            <BigButton onClick={stand}>stand aside</BigButton>
          </div>
          <div className="text-[var(--dim)]" style={small} aria-label="your quote">
            {Number.isFinite(b) && Number.isFinite(a) && a > b ? `mid ${mid({ bid: b, ask: a })}¢ · ${a - b}¢ wide` : "a bid below an ask"}
          </div>
          {run.problem && <div className="text-[var(--red)]">{run.problem}</div>}
          {run.settings.seconds === null ? (
            <div className="text-[var(--dim)]" style={small}>
              no clock — take your time
            </div>
          ) : (
            <Countdown deadline={run.deadline} total={run.settings.seconds} now={now} />
          )}
        </div>
      )}
      {reviewed && <Review x={reviewed} last={run.i + 1 >= run.contracts.length} onNext={() => onRun(nextRound(run, Date.now()))} />}
    </div>
  );
}

function Review({ x, last, onNext }: { x: EventRound; last: boolean; onNext: () => void }) {
  const small = fs(12, 1.7, 15);
  const c = x.contract;
  const truth = Math.round(c.truth * 10) / 10;
  return (
    <section className="flex flex-col gap-3 rounded-xl border border-[var(--border)] bg-[var(--bg-panel)] p-[2.5vmin]" aria-label="the review">
      <div className={cn("flex items-center gap-2 font-semibold", c.happened ? "text-[var(--green)]" : "text-[var(--red)]")}>
        {c.happened ? <CircleCheck style={{ width: "1.1em", height: "1.1em" }} /> : <CircleX style={{ width: "1.1em", height: "1.1em" }} />}
        {c.happened ? "It happened" : "It did not happen"}
        <span className="font-normal text-[var(--dim)]" style={small}>
          — you are marked on the true probability, not on this one outcome
        </span>
      </div>
      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <Tile n={`${truth}%`} label="true probability" />
        <Tile n={x.quote ? `${mid(x.quote)}¢` : x.outcome === "timeout" ? "timed out" : "stood aside"} label={`your mid · the brief implied ${Math.round(c.implied * 10) / 10}`} />
        <Tile n={pts(x.pnl)} label="P&L points" tone={x.pnl > 0 ? "var(--green)" : x.pnl < 0 ? "var(--red)" : undefined} />
        <Tile n={pts(x.calibration)} label="calibration" tone={x.calibration > 0 ? "var(--green)" : x.calibration < 0 ? "var(--red)" : undefined} />
      </div>
      {x.quote && (
        <>
          <div>
            <Label>flow against your {x.quote.bid} / {x.quote.ask}</Label>
            {x.fills.length ? (
              <ul className="flex flex-col gap-0.5 font-mono" style={small} aria-label="fills">
                {x.fills.map((f, i) => {
                  const v = Math.round(fillValue(f, c.truth) * 10) / 10;
                  return (
                    <li key={i} className="flex gap-2">
                      <span>
                        {f.side === "sold" ? "SOLD" : "BOUGHT"} {f.size} @ {f.price}¢
                      </span>
                      <span className={tone(v)}>{pts(v)}¢</span>
                      {f.informed && <span className="rounded bg-[var(--amber)] px-1 text-[var(--on-accent-dark)]">informed</span>}
                    </li>
                  );
                })}
              </ul>
            ) : (
              <div className="text-[var(--dim)]">No trader touched your market.</div>
            )}
            {x.fills.length > 0 && (
              <div className="mt-1 text-[var(--dim)]" style={small}>
                Closing position {x.held > 0 ? `+${x.held}` : x.held}, marked at {truth}¢.
              </div>
            )}
          </div>
          {x.verdict && (
            <div
              className={cn(
                x.verdict.tone === "good" && "text-[var(--green)]",
                x.verdict.tone === "warn" && "text-[var(--amber)]",
                x.verdict.tone === "bad" && "text-[var(--red)]",
              )}
              aria-label="verdict"
            >
              {x.verdict.text}{" "}
              <span className="text-[var(--dim)]">
                A balanced quote was {x.model.bid} / {x.model.ask}.
              </span>
            </div>
          )}
        </>
      )}
      <div>
        <BigButton solid onClick={onNext}>
          <AutoFocus />
          {last ? "results" : "next round"} <Key>↵</Key>
        </BigButton>
      </div>
    </section>
  );
}

// --- results ------------------------------------------------------------------------

function Results({
  run,
  sessions,
  unsaved,
  onAgain,
  onLobby,
}: {
  run: EventRun;
  sessions: EventSessionRecord[];
  unsaved: boolean;
  onAgain: () => void;
  onLobby: () => void;
}) {
  const o = eventOutcome(run);
  const st = eventStanding(sessions);
  const small = fs(12, 1.7, 15);
  const per = run.rounds.length ? Math.round((o.score / run.rounds.length) * 10) / 10 : 0;
  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-center gap-[3vw]">
        <Scale3d className="text-[var(--green)]" style={{ width: "clamp(40px, 9vmin, 96px)", height: "clamp(40px, 9vmin, 96px)" }} />
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-[var(--text-strong)]" style={fs(22, 5, 52)}>
            {run.ending === "stopped" ? "Stopped early — played, not a result" : "Markets resolved"}
          </div>
          <div className="text-[var(--dim)]">
            {run.rounds.length} round{run.rounds.length === 1 ? "" : "s"} · {run.settings.level} · {clockText(run.settings.seconds)} each
          </div>
          {st.best !== null && run.ending === "complete" && (
            <div className="text-[var(--dim)]">
              {st.scored.length === 1
                ? "your first session — the next one is compared with it"
                : `${per >= st.best ? "a new best" : `best ${pts(st.best)} a round`} · average ${pts(st.average ?? 0)} over ${st.scored.length}`}
            </div>
          )}
        </div>
        <div className="text-right">
          <div className={cn("font-mono leading-none font-semibold tabular-nums", tone(o.score))} style={fs(40, 10, 110)} aria-label="score">
            {pts(o.score)}
          </div>
          <div className="text-[var(--dim)]" style={small}>
            {pts(per)} a round
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <Tile n={pts(o.pnl)} label="P&L points" />
        <Tile n={pts(o.calibration)} label="calibration points" />
        <Tile n={o.quoted ? `${o.centred}/${o.quoted}` : "—"} label="quotes centred on the update" />
        <Tile n={o.avgMiss === null ? "—" : `${o.avgMiss} pts`} label="mid from the truth, on average" />
      </div>

      <div>
        <Label>round by round</Label>
        <table className="w-full" aria-label="round by round">
          <thead>
            <tr className="text-[var(--dim)]" style={small}>
              <th className="pb-1 text-left font-normal">event</th>
              <th className="pb-1 text-right font-normal">implied</th>
              <th className="pb-1 text-right font-normal">truth</th>
              <th className="pb-1 text-right font-normal">your quote</th>
              <th className="pb-1 text-right font-normal">P&amp;L</th>
              <th className="pb-1 text-right font-normal">calib.</th>
            </tr>
          </thead>
          <tbody>
            {run.rounds.map((x, i) => (
              <tr key={i} className="border-t border-[var(--border)]">
                <td className="py-[0.35em] pr-3" style={small}>
                  {x.contract.brief.title}
                </td>
                <td className="text-right font-mono tabular-nums">{Math.round(x.contract.implied)}</td>
                <td className="text-right font-mono tabular-nums">{Math.round(x.contract.truth)}</td>
                <td className="text-right font-mono whitespace-nowrap tabular-nums">
                  {x.quote ? `${x.quote.bid} / ${x.quote.ask}` : x.outcome === "timeout" ? "timed out" : "aside"}
                </td>
                <td className={cn("text-right font-mono tabular-nums", tone(x.pnl))}>{pts(x.pnl)}</td>
                <td className={cn("text-right font-mono tabular-nums", tone(x.calibration))}>{pts(x.calibration)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {unsaved && (
        <div className="text-[var(--amber)]" style={small}>
          This session could not be written to {historyPlace("event-contracts")}.
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3 border-t border-[var(--border)] pt-[3vh]">
        <BigButton onClick={onLobby}>lobby</BigButton>
        <BigButton solid className="ml-auto" onClick={onAgain}>
          <AutoFocus />
          play again <Key>↵</Key>
        </BigButton>
      </div>
    </div>
  );
}
