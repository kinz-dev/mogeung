/**
 * The screen of a market-taking game — the dice game's and the card game's,
 * which are the same game with a different hand (see `lib/takerTable.ts`).
 * Each game passes a `TakerVariant`: what its hand looks like, what it is
 * called, how it is dealt; this file draws the rest and keeps time for it.
 *
 * Three pages: a **lobby** (settings, how to play, your statistics), the
 * **table**, and the **results**. Escape walks back: from the table it stops
 * the game and shows how it went, from the results to the lobby, from the
 * lobby to the arcade.
 *
 * The clocks are the dice game's ask — *"I want a more flexible setting on the
 * time limit"* — kept for both: any second from 1 to 600, or none, at any
 * difficulty, without the game turning "custom".
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Eye, EyeOff, Trophy, Zap } from "lucide-react";
import { cn } from "@/lib/cn";
import { historyPlace, type ArcadeGame } from "@/lib/arcade";
import { clockFrom, clockText } from "@/lib/arcadeClock";
import {
  PENALTY,
  STARTING_BALANCE,
  ackEvent,
  landed,
  moveLine,
  nextRound,
  outcome,
  reportTimedOut,
  rightCall,
  roundPnl,
  startGame,
  stopGame,
  submitFinal,
  submitReport,
  tradingTimedOut,
  yourOrder,
  type Dealer,
  type TakerSettings,
  type TakerTable,
} from "@/lib/takerTable";
import {
  appendTakerSession,
  loadTakerSessions,
  takerStanding,
  toTakerSession,
  type Describe,
  type TakerSessionRecord,
} from "@/lib/takerStats";
import { seeded, type Rand } from "@/lib/tradingGame";
import { money } from "@/lib/tradingStats";
import { ArcadeHeader, AutoFocus, BigButton, Choice, ClockSetting, Countdown, Key, Label, NumberBox, Stepper, Tile, Toggle, fs, secs } from "@/ui/arcadeKit";
import { TrendChart } from "@/ui/MentalMathHistory";

export type Difficulty = "easy" | "medium" | "hard" | "custom";
type Preset = Pick<TakerSettings, "count" | "rounds" | "tradeSeconds" | "showBalance" | "events">;

/** Everything that makes one market-taking game differ from another. */
export interface TakerVariant<I, E, S extends TakerSettings & { difficulty: Difficulty }> {
  game: ArcadeGame;
  settingsKey: string;
  title: string;
  /** Drawn at the size it is given, in the game's colour — or muted, for a result that is not a win. */
  icon: (size: number | string, muted?: boolean) => ReactNode;
  /** The game's colour, as a CSS value, for its trend line. */
  color: string;
  blurb: ReactNode;
  howToPlay: ReactNode;
  /** The stepper's name for the hand's size, and the hand's size said in words. */
  countLabel: string;
  countText: (n: number) => string;
  limits: { count: readonly [number, number]; rounds: readonly [number, number] };
  presets: Record<Exclude<Difficulty, "custom">, Preset>;
  defaults: S;
  deal: Dealer<I, E, S>;
  eventText: (e: E) => string;
  /** How long the hand takes to land, what is said meanwhile, and the button after an event. */
  landMs: number;
  landingText: string;
  dealVerb: string;
  /** "the dice", "the hand" — the hand's accessible name — and "The dice came to". */
  handLabel: string;
  cameTo: string;
  renderItem: (item: I, o: { faceUp: boolean; landing: boolean; now: number; index: number }) => ReactNode;
  describe: Describe<I, E>;
}

type Phase = "lobby" | "table" | "results";

/** How long a report shows the hand before turning the hidden ones back. */
const PEEK_MS = 5000;
const DIFFICULTIES = ["easy", "medium", "hard", "custom"] as const;
const title = (d: string) => d[0].toUpperCase() + d.slice(1);

function loadSettings<S extends TakerSettings & { difficulty: Difficulty }>(v: TakerVariant<unknown, unknown, S>): S {
  const d = v.defaults;
  try {
    const raw = JSON.parse(localStorage.getItem(v.settingsKey) ?? "null") as Partial<S> | null;
    if (!raw || typeof raw !== "object") return d;
    const n = (k: "count" | "rounds", x: unknown) =>
      typeof x === "number" && Number.isInteger(x) && x >= v.limits[k][0] && x <= v.limits[k][1] ? x : d[k];
    return {
      ...d,
      difficulty: DIFFICULTIES.includes(raw.difficulty as Difficulty) ? (raw.difficulty as Difficulty) : "easy",
      count: n("count", raw.count),
      rounds: n("rounds", raw.rounds),
      tradeSeconds: clockFrom(raw.tradeSeconds, d.tradeSeconds),
      reportSeconds: clockFrom(raw.reportSeconds, d.reportSeconds),
      showBalance: raw.showBalance !== false,
      events: raw.events === true,
    };
  } catch {
    return d;
  }
}

export function TakerGame<I, E, S extends TakerSettings & { difficulty: Difficulty }>({
  variant: v,
  onExit,
  onClose,
  seed,
}: {
  variant: TakerVariant<I, E, S>;
  onExit: () => void;
  onClose: () => void;
  seed?: number;
}) {
  const [phase, setPhase] = useState<Phase>("lobby");
  const [settings, setSettings] = useState<S>(() => loadSettings(v as TakerVariant<unknown, unknown, S>));
  const [table, setTable] = useState<TakerTable<I, E, S> | null>(null);
  const [sessions, setSessions] = useState<TakerSessionRecord<S>[]>([]);
  const [unsaved, setUnsaved] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const rand = useRef<Rand>(seeded(seed ?? (Date.now() ^ 0xd1ce)));

  useEffect(() => {
    try {
      localStorage.setItem(v.settingsKey, JSON.stringify(settings));
    } catch {
      // Forgetting a setting is not worth an error.
    }
  }, [settings, v.settingsKey]);

  useEffect(() => {
    let live = true;
    void loadTakerSessions<S>(v.game).then((ss) => live && setSessions((mine) => [...ss, ...mine]));
    return () => {
      live = false;
    };
  }, [v.game]);

  const start = () => {
    setUnsaved(false);
    setNow(Date.now());
    setTable(startGame(settings, v.deal, rand.current));
    setPhase("table");
  };

  // The clock, while a game is on.
  useEffect(() => {
    if (phase !== "table") return;
    const id = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(id);
  }, [phase]);

  // A stage whose clock has run out.
  useEffect(() => {
    if (!table || table.deadline === null || now < table.deadline) return;
    setTable((t) => (t ? (t.stage === "trading" ? tradingTimedOut(t) : t.stage === "report" ? reportTimedOut(t) : t) : t));
  }, [now, table]);

  // The hand lands after a moment, and the quote goes up.
  const stageKey = table ? `${table.round.n}:${table.stage}` : "";
  useEffect(() => {
    if (!table || table.stage !== "rolling") return;
    const id = setTimeout(() => {
      // One reading of the clock for both, so the countdown starts on its own
      // second rather than up to a tick behind it.
      const at = Date.now();
      setNow(at);
      setTable((t) => (t ? landed(t, at) : t));
    }, v.landMs);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stageKey]);

  // The game is over: record it and show how it went — once.
  useEffect(() => {
    if (phase !== "table" || !table || table.stage !== "over") return;
    const record = toTakerSession(table, v.describe);
    if (record) {
      setSessions((ss) => [...ss, record]);
      void appendTakerSession(v.game, record).then((ok) => setUnsaved(!ok));
    }
    setPhase("results");
  }, [phase, table, v]);

  const escape = useRef(() => {});
  escape.current = () => {
    if (phase === "table") setTable((t) => (t ? stopGame(t) : t));
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

  const playing = phase === "table";
  return (
    <>
      <ArcadeHeader
        icon={v.icon(20)}
        title={v.title}
        detail={playing && table ? `round ${table.round.n} of ${table.settings.rounds}` : null}
        onBack={playing ? null : onExit}
        onClose={playing ? () => escape.current() : onClose}
        closeTitle={playing ? "stop the game  (Esc)" : "close  (Esc)"}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1200px] flex-col px-[5vw] py-[3vh]">
          {phase === "lobby" && <Lobby v={v} settings={settings} onChange={setSettings} sessions={sessions} onStart={start} />}
          {phase === "table" && table && <TableView v={v} table={table} now={now} rand={rand.current} onTable={setTable} />}
          {phase === "results" && table && (
            <Results v={v} table={table} sessions={sessions} unsaved={unsaved} onAgain={start} onLobby={() => setPhase("lobby")} />
          )}
        </div>
      </div>
    </>
  );
}

// --- lobby ----------------------------------------------------------------------

function Lobby<I, E, S extends TakerSettings & { difficulty: Difficulty }>({
  v,
  settings,
  onChange,
  sessions,
  onStart,
}: {
  v: TakerVariant<I, E, S>;
  settings: S;
  onChange: (s: S) => void;
  sessions: TakerSessionRecord<S>[];
  onStart: () => void;
}) {
  const d = settings.difficulty;
  const custom = d === "custom";
  const pickPreset = (next: Difficulty) =>
    onChange(next === "custom" ? { ...settings, difficulty: next } : { ...settings, difficulty: next, ...v.presets[next] });
  // The hand, the rounds and what you are shown make the difficulty; the
  // clocks do not — they are yours at any difficulty.
  const edit = (patch: Partial<S>) => onChange({ ...settings, ...patch, difficulty: "custom" });
  const clocks = (patch: Partial<S>) => onChange({ ...settings, ...patch });
  const own = custom ? null : v.presets[d].tradeSeconds;
  const st = useMemo(() => takerStanding(sessions), [sessions]);
  const small = fs(12, 1.7, 15);
  const startRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const t = setTimeout(() => startRef.current?.querySelector("button")?.focus(), 0);
    return () => clearTimeout(t);
  }, []);

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="text-[var(--dim)]">{v.blurb}</div>

      <div className="grid gap-[4vh] lg:grid-cols-2">
        <div className="flex flex-col gap-[2vh]">
          <div>
            <Label>difficulty</Label>
            <div className="flex flex-wrap gap-2">
              {DIFFICULTIES.map((x) => (
                <Choice key={x} on={d === x} onClick={() => pickPreset(x)}>
                  {title(x)}
                </Choice>
              ))}
            </div>
          </div>
          <div>
            <Label>clocks — yours at any difficulty</Label>
            <ClockSetting
              label="Time to trade"
              value={settings.tradeSeconds}
              fallback={own ?? 20}
              hint={
                own !== null && settings.tradeSeconds !== own
                  ? `${title(d)} is ${clockText(own)} as it comes`
                  : "from the quote going up to your order"
              }
              onChange={(x) => clocks({ tradeSeconds: x } as Partial<S>)}
            />
            <ClockSetting
              label="Time to report"
              value={settings.reportSeconds}
              fallback={20}
              hint={settings.reportSeconds === null ? "no clock in the original" : "a late report counts as wrong"}
              onChange={(x) => clocks({ reportSeconds: x } as Partial<S>)}
            />
          </div>
          <div>
            <Label>{custom ? "settings" : "settings — change any to go custom"}</Label>
            <Stepper label={v.countLabel} value={settings.count} range={v.limits.count} onChange={(x) => edit({ count: x } as Partial<S>)} />
            <Stepper label="Rounds" value={settings.rounds} range={v.limits.rounds} onChange={(x) => edit({ rounds: x } as Partial<S>)} />
            <Toggle label="Show balance" on={settings.showBalance} onChange={(x) => edit({ showBalance: x } as Partial<S>)} />
            <Toggle label="Market events" on={settings.events} onChange={(x) => edit({ events: x } as Partial<S>)} />
          </div>
          <div ref={startRef} className="flex items-center gap-3">
            <BigButton solid onClick={onStart}>
              start game <Key>↵</Key>
            </BigButton>
            <span className="text-[var(--dim)]" style={small}>
              {settings.rounds} rounds · {v.countText(settings.count)} · {clockText(settings.tradeSeconds)} to trade · you start with{" "}
              {STARTING_BALANCE}
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
                  <Tile n={st.best === null ? "—" : money(st.best)} label="best session" />
                  <Tile n={st.latest === null ? "—" : money(st.latest)} label="latest" />
                  <Tile n={st.average === null ? "—" : money(st.average)} label="average" />
                  <Tile n={st.exactness === null ? "—" : `${st.exactness}%`} label="reports exact" />
                  <Tile n={st.rightSide === null ? "—" : `${st.rightSide}%`} label="edges taken" />
                </div>
                {st.decisionMs !== null && (
                  <div className="mt-1 text-[var(--dim)]" style={small}>
                    {secs(st.decisionMs)} from quote to order, on average
                  </div>
                )}
                {st.scored.length > 1 && (
                  <div className="mt-[2vh]">
                    <TrendChart
                      values={st.scored.map((s) => s.pnl)}
                      labels={st.scored.map(
                        (s) => `${new Date(s.at).toLocaleString()} · ${s.settings.difficulty} · ${clockText(s.settings.tradeSeconds)}`,
                      )}
                      color={v.color}
                      format={(n) => money(n)}
                      label="profit and loss, session by session"
                    />
                  </div>
                )}
              </>
            )}
          </div>
          <details className="rounded-lg border border-[var(--border)] bg-[var(--bg-panel)] px-[1em] py-[0.6em]">
            <summary className="cursor-pointer font-semibold text-[var(--text-strong)]">How to play</summary>
            <div style={fs(12, 1.8, 16)}>{v.howToPlay}</div>
          </details>
        </div>
      </div>
    </div>
  );
}

/** How-to-play paragraphs, styled alike for every variant. */
export function Para({ head, children }: { head: string; children: ReactNode }) {
  return (
    <p className="mt-2 text-[var(--dim)]">
      <b>{head}</b> {children}
    </p>
  );
}

/** The how-to-play paragraphs every market-taking game shares. */
export function SharedRules({ what }: { what: string }) {
  return (
    <>
      <Para head="Orders must fit.">
        A purchase must be affordable (units × ask ≤ balance). A sale must survive the worst the hidden {what} could come
        to. An order that does not fit is forfeited and costs {PENALTY}.
      </Para>
      <Para head="Report.">
        After a trade, say your profit or loss — exactly, with a minus for a loss. The {what} show for five seconds, then
        the hidden ones turn back. A wrong answer forfeits the round's profit and costs {PENALTY}. With the balance
        hidden, you are asked for it at the end too.
      </Para>
      <Para head="Clocks.">
        Time to trade is any number of seconds, or none; running out skips the round at no cost. Time to report is off
        unless you set it, and a late report counts as wrong.
      </Para>
    </>
  );
}

// --- the table ----------------------------------------------------------------------

const UNIT_CHOICES = ["1", "5", "10"];

function TableView<I, E, S extends TakerSettings & { difficulty: Difficulty }>({
  v,
  table,
  now,
  rand,
  onTable,
}: {
  v: TakerVariant<I, E, S>;
  table: TakerTable<I, E, S>;
  now: number;
  rand: Rand;
  onTable: (t: TakerTable<I, E, S>) => void;
}) {
  const r = table.round;
  const s = table.settings;
  const [units, setUnits] = useState("1");
  const [report, setReport] = useState("");
  // During a report the hand shows for five seconds, then the hidden ones turn
  // back — the reference's rule: the total is yours to hold, not to read off.
  const [peek, setPeek] = useState(true);
  useEffect(() => {
    setUnits("1");
    setReport("");
    setPeek(true);
    if (table.stage !== "report") return;
    const id = setTimeout(() => setPeek(false), PEEK_MS);
    return () => clearTimeout(id);
  }, [table.stage, r.n]);

  const shown = (i: I) =>
    table.stage === "settled" ||
    table.stage === "final" ||
    (table.stage === "report" ? peek || v.describe.open(i) : table.stage !== "rolling" && v.describe.open(i));
  const small = fs(12, 1.7, 15);
  const quoted = table.stage !== "event" && table.stage !== "rolling";
  const num = (x: string) => (x === "" || x === "-" ? NaN : Number(x));
  const order = (side: "buy" | "sell") => onTable(yourOrder(table, { side, units: num(units) }, Date.now()));
  const skip = () => onTable(yourOrder(table, "skip", Date.now()));
  const sendReport = () => report !== "" && report !== "-" && onTable(submitReport(table, Number(report)));
  const sendFinal = () => report !== "" && report !== "-" && onTable(submitFinal(table, Number(report)));

  let panel: ReactNode = null;
  switch (table.stage) {
    case "event":
      panel = (
        <div className="flex flex-col items-start gap-3">
          <div className="flex items-center gap-2 text-[var(--amber)]" style={fs(18, 3.4, 32)}>
            <Zap style={{ width: "1em", height: "1em" }} /> Market event
          </div>
          <div className="text-[var(--text-strong)]" style={fs(16, 3, 28)}>
            {r.event && v.eventText(r.event)}
          </div>
          <BigButton solid onClick={() => onTable(ackEvent(table))}>
            <AutoFocus />
            {v.dealVerb} <Key>↵</Key>
          </BigButton>
        </div>
      );
      break;
    case "rolling":
      panel = <div className="text-[var(--dim)]">{v.landingText}</div>;
      break;
    case "trading":
      panel = (
        <div className="flex flex-col gap-3">
          <div>
            How many units?{" "}
            <span className="text-[var(--dim)]" style={small}>
              <Key>B</Key> buy · <Key>S</Key> sell · <Key>K</Key> skip
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            {UNIT_CHOICES.map((u) => (
              <Choice key={u} on={units === u} onClick={() => setUnits(u)} className="font-mono">
                {u}
              </Choice>
            ))}
            <NumberBox
              label="units"
              value={units}
              onChange={(x) => setUnits(x.replace(/-/g, ""))}
              onEnter={() => undefined}
              onKey={(k) => {
                if (k === "b") order("buy");
                else if (k === "s") order("sell");
                else if (k === "k") skip();
                else return false;
                return true;
              }}
              autoFocus
              width="4.5em"
            />
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <BigButton solid onClick={() => order("buy")}>
              buy at {r.quote.ask}
            </BigButton>
            <BigButton solid onClick={() => order("sell")}>
              sell at {r.quote.bid}
            </BigButton>
            <BigButton onClick={skip}>skip</BigButton>
          </div>
          {table.problem && <div className="text-[var(--red)]">{table.problem}</div>}
          {s.tradeSeconds === null ? (
            <div className="text-[var(--dim)]" style={small}>
              no clock — take your time
            </div>
          ) : (
            <Countdown deadline={table.deadline} total={s.tradeSeconds} now={now} />
          )}
        </div>
      );
      break;
    case "report": {
      const m = r.move?.kind === "trade" ? r.move : null;
      panel = (
        <div className="flex flex-col gap-3">
          <div>
            You {m?.side === "buy" ? "bought" : "sold"} {m?.units} unit{m?.units === 1 ? "" : "s"} at{" "}
            {m?.side === "buy" ? r.quote.ask : r.quote.bid}. <b>What was your profit or loss this round?</b>
          </div>
          <div className="text-[var(--dim)]" style={small}>
            Exact answers only: a wrong one forfeits this round's profit and costs {PENALTY}. A minus sign for a loss.
          </div>
          <div className="flex items-center gap-3">
            <NumberBox label="your profit or loss" value={report} onChange={setReport} onEnter={sendReport} autoFocus />
            <BigButton solid onClick={sendReport}>
              report <Key>↵</Key>
            </BigButton>
          </div>
          {s.reportSeconds !== null && <Countdown deadline={table.deadline} total={s.reportSeconds} now={now} />}
        </div>
      );
      break;
    }
    case "settled":
      panel = <Settled v={v} table={table} rand={rand} onTable={onTable} />;
      break;
    case "final":
      panel = (
        <div className="flex flex-col gap-3">
          <div>
            <b>The market is closed. What is your final balance?</b>{" "}
            <span className="text-[var(--dim)]" style={small}>
              exact answers only — a wrong one costs {PENALTY}
            </span>
          </div>
          <div className="flex items-center gap-3">
            <NumberBox label="your final balance" value={report} onChange={setReport} onEnter={sendFinal} autoFocus />
            <BigButton solid onClick={sendFinal}>
              answer <Key>↵</Key>
            </BigButton>
          </div>
        </div>
      );
      break;
  }

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-center gap-[3vw]">
        <div aria-label="your balance">
          <div className="text-[var(--dim)]" style={small}>
            balance
          </div>
          <div className="font-mono font-semibold text-[var(--text-strong)] tabular-nums" style={fs(20, 4, 40)}>
            {s.showBalance ? table.balance : <span className="text-[var(--dim)]">hidden</span>}
          </div>
        </div>
        {r.event && table.stage !== "event" && (
          <div className="flex items-center gap-2 text-[var(--amber)]" style={small}>
            <Zap style={{ width: "1em", height: "1em" }} /> {v.eventText(r.event)}
          </div>
        )}
      </div>
      <div className="flex flex-wrap items-center justify-center gap-[2vmin] py-[1vh]" aria-label={v.handLabel}>
        {table.stage !== "event" &&
          r.items.map((item, i) => (
            <span key={`${r.n}-${i}`} className="contents">
              {v.renderItem(item, { faceUp: shown(item), landing: table.stage === "rolling", now, index: i })}
            </span>
          ))}
      </div>
      <div className="flex items-baseline justify-center gap-3" aria-label="the quote">
        {quoted ? (
          <>
            <span className="text-[var(--dim)]">The market maker quotes</span>
            <span className="font-mono font-semibold text-[var(--text-strong)] tabular-nums" style={fs(26, 6, 64)}>
              {r.quote.bid} <span className="text-[var(--dim)]">at</span> {r.quote.ask}
            </span>
          </>
        ) : (
          <span className="text-[var(--dim)]">no quote yet</span>
        )}
      </div>
      <div className="rounded-xl border border-[var(--border)] bg-[var(--bg-panel)] p-[2.5vmin]">{panel}</div>
      <div className="flex items-center gap-2 text-[var(--dim)]" style={small}>
        {s.showBalance ? <Eye style={{ width: "1em", height: "1em" }} /> : <EyeOff style={{ width: "1em", height: "1em" }} />}
        {s.showBalance ? "your balance is shown" : "your balance is hidden — keep count"} · <Key>Esc</Key> stops the game
      </div>
    </div>
  );
}

/** A round turned over: what it came to, what you made, and what the fair value said. */
function Settled<I, E, S extends TakerSettings & { difficulty: Difficulty }>({
  v,
  table,
  rand,
  onTable,
}: {
  v: TakerVariant<I, E, S>;
  table: TakerTable<I, E, S>;
  rand: Rand;
  onTable: (t: TakerTable<I, E, S>) => void;
}) {
  const r = table.round;
  const m = r.move;
  const pnl = roundPnl(r);
  const call = rightCall(r.quote, r.fair);
  const last = r.n >= table.settings.rounds || table.balance < 0;
  return (
    <div className="flex flex-col gap-2">
      <div style={fs(16, 3, 28)}>
        {v.cameTo} <b className="font-mono">{r.settle}</b>
        <span className="text-[var(--dim)]">
          {" "}
          · quote {r.quote.bid} at {r.quote.ask}
        </span>
      </div>
      {m?.kind === "trade" && (
        <div>
          You {m.side === "buy" ? "bought" : "sold"} {m.units}:{" "}
          <b className={pnl >= 0 ? "text-[var(--green)]" : "text-[var(--red)]"}>{money(pnl)}</b>
          {r.reportLate ? (
            <span className="text-[var(--red)]">
              {" "}
              — no report in time; {pnl >= 0 ? "profit forfeited and " : ""}−{PENALTY}
            </span>
          ) : r.reported === pnl ? (
            <span className="text-[var(--green)]"> — you reported it exactly</span>
          ) : (
            <span className="text-[var(--red)]">
              {" "}
              — you reported {r.reported}; {pnl >= 0 ? "profit forfeited and " : ""}−{PENALTY}
            </span>
          )}
        </div>
      )}
      {m?.kind === "forfeit" && (
        <div className="text-[var(--red)]">
          Order forfeited — {m.side} {m.units} did not fit your balance. −{PENALTY}.
        </div>
      )}
      {m?.kind === "skip" && <div className="text-[var(--dim)]">You skipped.</div>}
      {m?.kind === "timeout" && <div className="text-[var(--dim)]">Time ran out — no trade, no penalty.</div>}
      <div className="text-[var(--dim)]">
        Fair value {r.fair.toFixed(1)} —{" "}
        {call === "skip"
          ? "inside the quote: no edge either way."
          : `${call === "buy" ? "above the ask: the edge was to buy" : "below the bid: the edge was to sell"}${
              m?.kind === "trade" ? (m.side === call ? ", and you did." : ", and you went the other way.") : "."
            }`}
      </div>
      {table.balance < 0 && <div className="text-[var(--red)]">Your balance has gone negative — the game ends here.</div>}
      <div>
        <BigButton solid onClick={() => onTable(nextRound(table, v.deal, rand))}>
          <AutoFocus />
          {last ? "finish" : "next round"} <Key>↵</Key>
        </BigButton>
      </div>
    </div>
  );
}

// --- results ------------------------------------------------------------------------

function Results<I, E, S extends TakerSettings & { difficulty: Difficulty }>({
  v,
  table,
  sessions,
  unsaved,
  onAgain,
  onLobby,
}: {
  v: TakerVariant<I, E, S>;
  table: TakerTable<I, E, S>;
  sessions: TakerSessionRecord<S>[];
  unsaved: boolean;
  onAgain: () => void;
  onLobby: () => void;
}) {
  const o = outcome(table);
  const st = takerStanding(sessions);
  const small = fs(12, 1.7, 15);
  const s = table.settings;
  const headline =
    table.ending === "bust" ? "Bust — your balance went negative" : table.ending === "stopped" ? "Stopped early — played, not a result" : "Market closed";
  const big = "clamp(40px, 9vmin, 96px)";
  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-center gap-[3vw]">
        {o.pnl > 0 && table.ending === "complete" ? (
          <Trophy className="text-[var(--amber)]" style={{ width: big, height: big }} />
        ) : (
          v.icon(big, true)
        )}
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-[var(--text-strong)]" style={fs(22, 5, 52)}>
            {headline}
          </div>
          <div className="text-[var(--dim)]">
            {table.history.length} round{table.history.length === 1 ? "" : "s"} · {v.countText(s.count)} · {s.difficulty} ·{" "}
            {clockText(s.tradeSeconds)} to trade
            {s.reportSeconds !== null && ` · ${clockText(s.reportSeconds)} to report`}
          </div>
          {table.finalReported !== null && (
            <div className={table.finalAdjust === 0 ? "text-[var(--green)]" : "text-[var(--red)]"}>
              You said {table.finalReported}; {table.finalAdjust === 0 ? "exactly right" : `it was ${o.balance - table.finalAdjust} — −${PENALTY}`}
            </div>
          )}
          {st.best !== null && table.ending === "complete" && (
            <div className="text-[var(--dim)]">
              {st.scored.length === 1
                ? "your first session — the next one is compared with it"
                : `${o.pnl >= st.best ? "a new best session" : `best session ${money(st.best)}`} · average ${money(st.average ?? 0)} over ${st.scored.length}`}
            </div>
          )}
        </div>
        <div className="text-right">
          <div
            className={cn("font-mono leading-none font-semibold tabular-nums", o.pnl > 0 ? "text-[var(--green)]" : o.pnl < 0 ? "text-[var(--red)]" : "text-[var(--text-strong)]")}
            style={fs(48, 12, 140)}
            aria-label="profit and loss"
          >
            {money(o.pnl)}
          </div>
          <div className="text-[var(--dim)]" style={small}>
            final balance {o.balance}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <Tile n={`${o.won}/${o.trades}`} label="trades that made money" />
        <Tile n={o.reports ? `${o.exact}/${o.reports}` : "—"} label="reports exact" />
        <Tile n={o.edges ? `${o.rightSide}/${o.edges}` : "—"} label="edges taken" />
        <Tile n={secs(o.avgDecisionMs)} label="quote to order" />
      </div>

      <div>
        <Label>round by round</Label>
        <table className="w-full" aria-label="round by round">
          <thead>
            <tr className="text-[var(--dim)]" style={small}>
              <th className="pb-1 text-left font-normal">round</th>
              <th className="pb-1 text-left font-normal">{v.countLabel.toLowerCase()}</th>
              <th className="pb-1 text-right font-normal">quote</th>
              <th className="pb-1 text-right font-normal">fair</th>
              <th className="pb-1 text-right font-normal">total</th>
              <th className="pb-1 text-left font-normal">&nbsp;&nbsp;you</th>
              <th className="pb-1 text-right font-normal">P&amp;L</th>
            </tr>
          </thead>
          <tbody>
            {table.history.map((r) => {
              const mine = roundPnl(r) + r.adjust;
              return (
                <tr key={r.n} className="border-t border-[var(--border)]">
                  <td className="py-[0.35em] font-mono">{r.n}</td>
                  <td className="font-mono" style={small} title="in brackets: face down when quoted">
                    {r.items.map((i) => (v.describe.open(i) ? v.describe.item(i) : `(${v.describe.item(i)})`)).join(" ")}
                  </td>
                  <td className="text-right font-mono tabular-nums">
                    {r.quote.bid}–{r.quote.ask}
                  </td>
                  <td className="text-right font-mono tabular-nums">{r.fair.toFixed(1)}</td>
                  <td className="text-right font-mono tabular-nums">{r.settle}</td>
                  <td style={small}>&nbsp;&nbsp;{moveLine(r)}</td>
                  <td className={cn("text-right font-mono tabular-nums", mine > 0 && "text-[var(--green)]", mine < 0 && "text-[var(--red)]")}>
                    {money(mine)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {unsaved && (
        <div className="text-[var(--amber)]" style={small}>
          This session could not be written to {historyPlace(v.game)}.
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
