/**
 * The ETF Arbitrage Game — the arcade's fourth, after tradermath.org's. The
 * rules live in [`lib/etfGame.ts`](../lib/etfGame.ts), the round-by-round race
 * in [`lib/etfTable.ts`](../lib/etfTable.ts); this file draws it and keeps time.
 *
 * Three pages, like the other market games: a **lobby**, the **table** — the
 * basket, the ETF's quote, your order and the standings — and the **results**.
 * Escape walks back: from the table it stops the game, from the results to the
 * lobby, from the lobby to the arcade.
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Crown, Scale, TrendingDown, TrendingUp, Zap } from "lucide-react";
import { cn } from "@/lib/cn";
import { historyPlace } from "@/lib/arcade";
import { clockFrom, clockText } from "@/lib/arcadeClock";
import {
  COST,
  DEFAULT_ETF,
  FIRST_BONUS,
  LEVELS,
  MAX_UNITS,
  PERSONALITIES,
  ROUNDS,
  rightSide,
  seeded,
  signedUsd,
  usd,
  type EtfSettings,
  type Level,
  type Market,
  type Rand,
} from "@/lib/etfGame";
import {
  YOU,
  market,
  nextRound,
  openRound,
  outcome,
  roundWinner,
  startGame,
  stopGame,
  tick,
  yourTrade,
  type Act,
  type EtfTable,
} from "@/lib/etfTable";
import { appendEtfSession, etfStanding, loadEtfSessions, toEtfSession, type EtfSessionRecord } from "@/lib/etfStats";
import { ordinal } from "@/lib/tradingGame";
import { ArcadeHeader, AutoFocus, BigButton, Choice, ClockSetting, Countdown, Key, Label, NumberBox, Stepper, Tile, Toggle, fs } from "@/ui/arcadeKit";
import { TrendChart } from "@/ui/MentalMathHistory";

type Phase = "lobby" | "table" | "results";

const SETTINGS_KEY = "mogeung.etf.settings";
const LEVEL_NAMES: Level[] = ["easy", "medium", "hard"];
const title = (s: string) => s[0].toUpperCase() + s.slice(1);

/** The basket fills in a row at a time, then the clock starts — the reference's pace. */
const ROW_MS = 150;
const INTRO_LEAD_MS = 300;
export const introMs = (rows: number) => INTRO_LEAD_MS + rows * ROW_MS + 200;

function loadSettings(): EtfSettings {
  try {
    const raw = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "null") as Partial<EtfSettings> | null;
    if (!raw || typeof raw !== "object") return DEFAULT_ETF;
    const rounds = typeof raw.rounds === "number" && Number.isInteger(raw.rounds) && raw.rounds >= ROUNDS[0] && raw.rounds <= ROUNDS[1] ? raw.rounds : DEFAULT_ETF.rounds;
    return {
      level: LEVEL_NAMES.includes(raw.level as Level) ? (raw.level as Level) : "easy",
      rounds,
      seconds: clockFrom(raw.seconds, DEFAULT_ETF.seconds),
      events: raw.events === true,
      showWxP: raw.showWxP !== false,
    };
  } catch {
    return DEFAULT_ETF;
  }
}

export function EtfGame({ onExit, onClose, seed }: { onExit: () => void; onClose: () => void; seed?: number }) {
  const [phase, setPhase] = useState<Phase>("lobby");
  const [settings, setSettings] = useState<EtfSettings>(loadSettings);
  const [table, setTable] = useState<EtfTable | null>(null);
  const [sessions, setSessions] = useState<EtfSessionRecord[]>([]);
  const [unsaved, setUnsaved] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [rows, setRows] = useState(0);
  const rand = useRef<Rand>(seeded(seed ?? (Date.now() ^ 0xe7f)));

  useEffect(() => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Forgetting a setting is not worth an error.
    }
  }, [settings]);

  useEffect(() => {
    let live = true;
    void loadEtfSessions().then((ss) => live && setSessions((mine) => [...ss, ...mine]));
    return () => {
      live = false;
    };
  }, []);

  const start = () => {
    setUnsaved(false);
    setNow(Date.now());
    setTable(startGame(settings, rand.current));
    setPhase("table");
  };

  // The clock, while a game is on — and with it, the bots' turns.
  useEffect(() => {
    if (phase !== "table") return;
    const id = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(id);
  }, [phase]);
  useEffect(() => {
    if (table?.stage === "active") setTable((t) => (t ? tick(t, now, rand.current) : t));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [now]);

  // A round's basket fills in, then it opens.
  const stageKey = table ? `${table.n}:${table.stage}` : "";
  useEffect(() => {
    if (!table || table.stage !== "intro") return;
    const count = market(table).stocks.length;
    setRows(0);
    const ids = Array.from({ length: count }, (_, i) => setTimeout(() => setRows(i + 1), INTRO_LEAD_MS + i * ROW_MS));
    ids.push(
      setTimeout(() => {
        // The clock is read once and shared, so the countdown starts on the
        // round's own second rather than up to a tick behind it.
        const at = Date.now();
        setNow(at);
        setTable((t) => (t ? openRound(t, at, rand.current) : t));
      }, introMs(count)),
    );
    return () => ids.forEach(clearTimeout);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stageKey]);

  // The game is over: record it and show how it went — once.
  useEffect(() => {
    if (phase !== "table" || !table || table.stage !== "over") return;
    const record = toEtfSession(table);
    if (record) {
      setSessions((ss) => [...ss, record]);
      void appendEtfSession(record).then((ok) => setUnsaved(!ok));
    }
    setPhase("results");
  }, [phase, table]);

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
        icon={<Scale size={20} className="text-[var(--blue)]" />}
        title="etf arbitrage"
        detail={playing && table ? `round ${table.n} of ${table.markets.length}` : null}
        onBack={playing ? null : onExit}
        onClose={playing ? () => escape.current() : onClose}
        closeTitle={playing ? "stop the game  (Esc)" : "close  (Esc)"}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1200px] flex-col px-[5vw] py-[3vh]">
          {phase === "lobby" && <Lobby settings={settings} onChange={setSettings} sessions={sessions} onStart={start} />}
          {phase === "table" && table && <TableView table={table} now={now} rows={rows} rand={rand.current} onTable={setTable} />}
          {phase === "results" && table && (
            <Results table={table} sessions={sessions} unsaved={unsaved} onAgain={start} onLobby={() => setPhase("lobby")} />
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
  settings: EtfSettings;
  onChange: (s: EtfSettings) => void;
  sessions: EtfSessionRecord[];
  onStart: () => void;
}) {
  const level = LEVELS[settings.level];
  const pick = (l: Level) => onChange({ ...settings, level: l, rounds: LEVELS[l].rounds, seconds: LEVELS[l].seconds });
  const st = useMemo(() => etfStanding(sessions), [sessions]);
  const small = fs(12, 1.7, 15);
  const startRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const t = setTimeout(() => startRef.current?.querySelector("button")?.focus(), 0);
    return () => clearTimeout(t);
  }, []);

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="text-[var(--dim)]">
        Work out an ETF's fair value from its basket of four stocks, spot when its quote strays from it, and race three
        bots to the trade.
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
              {level.wholePrices ? "Whole-dollar prices and weights in tenths, for easier sums. " : "Prices to the cent, weights to the hundredth. "}
              The quote strays {level.premium[0]}–{level.premium[1]}% from the NAV. Against{" "}
              {level.bots.map((b) => `a ${b}`).join(", ")}.
            </div>
          </div>
          <div>
            <Label>rounds and clock — yours at any difficulty</Label>
            <Stepper label="Rounds" value={settings.rounds} range={ROUNDS} onChange={(v) => onChange({ ...settings, rounds: v })} />
            <ClockSetting
              label="Time per round"
              value={settings.seconds}
              fallback={level.seconds}
              hint={settings.seconds !== level.seconds ? `${title(settings.level)} is ${level.seconds}s as it comes` : "from the basket showing to the round's end"}
              onChange={(v) => onChange({ ...settings, seconds: v })}
            />
            <Toggle label="Market events" on={settings.events} onChange={(v) => onChange({ ...settings, events: v })} />
            <Toggle label="Show W × P" on={settings.showWxP} onChange={(v) => onChange({ ...settings, showWxP: v })} />
          </div>
          <div ref={startRef} className="flex items-center gap-3">
            <BigButton solid onClick={onStart}>
              start game <Key>↵</Key>
            </BigButton>
            <span className="text-[var(--dim)]" style={small}>
              {settings.rounds} rounds · {clockText(settings.seconds)} a round · {settings.events ? "with news" : "no news"}
              {settings.showWxP ? "" : " · no W × P"}
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
                  <Tile n={st.best === null ? "—" : signedUsd(st.best)} label="best session" />
                  <Tile n={st.latest === null ? "—" : signedUsd(st.latest)} label="latest" />
                  <Tile n={st.average === null ? "—" : signedUsd(st.average)} label="average" />
                  <Tile n={st.wins} label="won outright" />
                  <Tile n={st.accuracy === null ? "—" : `${st.accuracy}%`} label="right side" />
                </div>
                {st.speed !== null && (
                  <div className="mt-1 text-[var(--dim)]" style={small}>
                    {st.speed.toFixed(1)}s to trade, on average
                  </div>
                )}
                {st.scored.length > 1 && (
                  <div className="mt-[2vh]">
                    <TrendChart
                      values={st.scored.map((s) => s.pnl / 100)}
                      labels={st.scored.map((s) => `${new Date(s.at).toLocaleString()} · ${s.settings.level} · ${ordinal(s.place)}`)}
                      color="var(--blue)"
                      format={(n) => signedUsd(Math.round(n * 100))}
                      label="profit and loss, session by session"
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
        <b>NAV.</b> An ETF holds a basket of four stocks in fixed weights. Its fair value — the net asset value — is the
        sum of weight × price. Weights 0.25 on stocks at $200, $400, $150 and $200 make a NAV of $237.50.
      </p>
      <p className={p}>
        <b>The trade.</b> The ETF is quoted <i>bid / ask</i> a little off its NAV. When the NAV is above the ask it is
        cheap: <b>buy</b>. When the NAV is below the bid it is dear: <b>sell</b>. Pick 1–{MAX_UNITS} units.
      </p>
      <p className={p}>
        <b>Paying for it.</b> Every trade costs ${COST / 100}. The first to trade in a round earns {FIRST_BONUS * 100}% on
        top of a profit — buying 10 at $233.40 on a NAV of $237.50 grosses $41.00, nets $31.00, and $37.20 if you were
        first. A thin edge may not cover the cost: skip it.
      </p>
      <p className={p}>
        <b>The race.</b> The basket stays the same all game; each round its prices move, and the Change column says how.
        Three bots trade too, each in its own way — fast and careless, slow and careful — and hurry once you have
        traded. The round ends when everyone has, or the clock runs out.
      </p>
      <p className={p}>
        <b>Make it harder.</b> Turn off W × P and the multiplication is yours. Turn on market events and headlines move
        the prices — a company's bad day, a sector's good one.
      </p>
    </div>
  );
}

// --- the table ----------------------------------------------------------------------

const price = (m: Market, p: number) => (Number.isInteger(p) && m.stocks.every((s) => Number.isInteger(s.price)) ? `$${p}` : usd(Math.round(p * 100)));

function MarketTable({ m, rows, reveal, showWxP }: { m: Market; rows: number; reveal: boolean; showWxP: boolean }) {
  const small = fs(12, 1.7, 15);
  const num = "text-right font-mono tabular-nums";
  return (
    <div className="rounded-xl border border-[var(--border)] bg-[var(--bg-panel)] p-[2vmin]">
      <table className="w-full" aria-label="the basket">
        <thead>
          <tr className="text-[var(--dim)]" style={small}>
            <th className="pb-1 text-left font-normal">{m.basket}</th>
            <th className="pb-1 text-right font-normal">price</th>
            <th className="pb-1 text-right font-normal">change</th>
            <th className="pb-1 text-right font-normal">%</th>
            <th className="pb-1 text-right font-normal">weight</th>
            {showWxP && <th className="pb-1 text-right font-normal">W × P</th>}
          </tr>
        </thead>
        <tbody>
          {m.stocks.map((s, i) => {
            const change = s.prev === null ? null : s.price - s.prev;
            const tone = change === null || Math.abs(change) < 0.005 ? "" : change > 0 ? "text-[var(--green)]" : "text-[var(--red)]";
            return (
              <tr key={s.ticker} className={cn("border-t border-[var(--border)]", i >= rows && "invisible")} aria-label={`${s.ticker} row`}>
                <td className="py-[0.4em] font-mono font-semibold text-[var(--text-strong)]">{s.ticker}</td>
                <td className={num} aria-label={`${s.ticker} price`}>
                  {price(m, s.price)}
                </td>
                <td className={cn(num, tone)}>{change === null ? "—" : `${change >= 0 ? "+" : "−"}${Math.abs(change).toFixed(2)}`}</td>
                <td className={cn(num, tone)} style={small}>
                  {change === null || s.prev === null ? "" : `${change >= 0 ? "+" : "−"}${Math.abs((change / s.prev) * 100).toFixed(1)}%`}
                </td>
                <td className={num} aria-label={`${s.ticker} weight`}>
                  {s.weight.toFixed(2)}
                </td>
                {showWxP && (
                  <td className={num} aria-label={`${s.ticker} W × P`}>
                    {(s.weight * s.price).toFixed(2)}
                  </td>
                )}
              </tr>
            );
          })}
          <tr className="border-t-2 border-[var(--border)]">
            <td className="py-[0.4em] font-semibold" colSpan={showWxP ? 5 : 4}>
              NAV
            </td>
            <td className={cn(num, "font-semibold")} aria-label="the NAV">
              {reveal ? usd(Math.round(m.nav * 100)) : "?"}
            </td>
          </tr>
        </tbody>
      </table>
      <div className="mt-[2vh] grid grid-cols-2 gap-2" aria-label="the ETF quote">
        <div className="rounded-lg border border-[var(--border)] px-[0.9em] py-[0.4em]">
          <div className="text-[var(--dim)]" style={small}>
            ETF bid — you sell here
          </div>
          <div className="font-mono font-semibold text-[var(--text-strong)] tabular-nums" style={fs(20, 4.2, 40)} aria-label="bid">
            {rows >= m.stocks.length ? usd(Math.round(m.bid * 100)) : "…"}
          </div>
        </div>
        <div className="rounded-lg border border-[var(--border)] px-[0.9em] py-[0.4em]">
          <div className="text-[var(--dim)]" style={small}>
            ETF ask — you buy here
          </div>
          <div className="font-mono font-semibold text-[var(--text-strong)] tabular-nums" style={fs(20, 4.2, 40)} aria-label="ask">
            {rows >= m.stocks.length ? usd(Math.round(m.ask * 100)) : "…"}
          </div>
        </div>
      </div>
    </div>
  );
}

const actText = (a: Act) =>
  a.action === "pass" ? (a.timedOut ? "out of time" : "passed") : `${a.action === "buy" ? "bought" : "sold"} ${a.units}`;

function Standings({ table }: { table: EtfTable }) {
  const summary = table.stage === "summary";
  const order = [...table.players].sort((a, b) => b.pnl - a.pnl || a.id - b.id);
  const winner = summary ? roundWinner({ acts: table.acts }) : null;
  const small = fs(11, 1.6, 14);
  return (
    <div className="flex flex-col gap-2" aria-label="standings">
      {order.map((p, i) => {
        const a = table.acts[p.id];
        return (
          <div
            key={p.id}
            aria-label={`seat ${p.name}`}
            className={cn(
              "rounded-lg border-2 bg-[var(--bg-panel)] px-[0.9em] py-[0.45em]",
              p.human ? "border-[var(--selection-stroke)]" : "border-[var(--border)]",
            )}
          >
            <div className="flex items-baseline gap-2">
              <span className="font-mono text-[var(--dim)]" style={small}>
                {ordinal(i + 1)}
              </span>
              <span className="font-semibold text-[var(--text-strong)]">{p.name}</span>
              {p.style && (
                <span className="text-[var(--dim)]" style={small} title={PERSONALITIES[p.style].blurb}>
                  {p.style}
                </span>
              )}
              {winner === p.id && <Crown className="text-[var(--amber)]" style={{ width: "1em", height: "1em" }} aria-label="won the round" />}
              <span
                className={cn("ml-auto font-mono tabular-nums", p.pnl > 0 ? "text-[var(--green)]" : p.pnl < 0 ? "text-[var(--red)]" : "")}
                aria-label={`${p.name} P&L`}
              >
                {signedUsd(p.pnl)}
              </span>
            </div>
            <div className="min-h-[1.3em] text-[var(--dim)]" style={small}>
              {table.stage === "active" && (a ? (p.human ? actText(a) : a.action === "pass" ? "passed" : "✓ traded") : p.human ? "" : "thinking…")}
              {summary && a && (
                <>
                  {actText(a)}
                  {a.action !== "pass" && (
                    <span className={a.score.net >= 0 ? "text-[var(--green)]" : "text-[var(--red)]"}> {signedUsd(a.score.net)}</span>
                  )}
                  {a.score.bonus > 0 && (
                    <span className="text-[var(--amber)]">
                      {" "}
                      <Zap className="inline" style={{ width: "0.9em", height: "0.9em" }} /> first
                    </span>
                  )}
                </>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

const UNIT_CHOICES = [1, 5, 10, 20];

function TableView({ table, now, rows, rand, onTable }: { table: EtfTable; now: number; rows: number; rand: Rand; onTable: (t: EtfTable) => void }) {
  const m = market(table);
  const s = table.settings;
  const [units, setUnits] = useState("10");
  const small = fs(12, 1.7, 15);
  const you = table.players[YOU];
  const mine = table.acts[YOU];
  const summary = table.stage === "summary";
  const trade = (action: "buy" | "sell" | "pass") =>
    onTable(yourTrade(table, action, action === "pass" ? 0 : units === "" ? NaN : Number(units), Date.now(), rand));

  let panel: ReactNode = null;
  if (table.stage === "intro") {
    panel = <div className="text-[var(--dim)]">The market is opening…</div>;
  } else if (table.stage === "active" && !mine) {
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
            <Choice key={u} on={units === String(u)} onClick={() => setUnits(String(u))} className="font-mono">
              {u}
            </Choice>
          ))}
          <NumberBox
            label="units"
            value={units}
            onChange={(v) => setUnits(v.replace(/-/g, ""))}
            onEnter={() => undefined}
            onKey={(k) => {
              if (k === "b") trade("buy");
              else if (k === "s") trade("sell");
              else if (k === "k") trade("pass");
              else return false;
              return true;
            }}
            autoFocus
            width="4.5em"
          />
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <BigButton solid onClick={() => trade("buy")}>
            buy at {usd(Math.round(m.ask * 100))}
          </BigButton>
          <BigButton solid onClick={() => trade("sell")}>
            sell at {usd(Math.round(m.bid * 100))}
          </BigButton>
          <BigButton onClick={() => trade("pass")}>skip</BigButton>
        </div>
        {table.problem && <div className="text-[var(--red)]">{table.problem}</div>}
      </div>
    );
  } else if (table.stage === "active") {
    panel = (
      <div className="text-[var(--dim)]">
        You {actText(mine)} — waiting for the others…
      </div>
    );
  } else if (summary) {
    const right = rightSide(m);
    const last = table.n >= table.markets.length;
    panel = (
      <div className="flex flex-col gap-2">
        <div style={fs(16, 3, 28)}>
          NAV <b className="font-mono">{usd(Math.round(m.nav * 100))}</b>
          <span className="text-[var(--dim)]">
            {" "}
            — the ETF was {m.premiumPct > 0 ? `${m.premiumPct.toFixed(2)}% dear: sell` : `${Math.abs(m.premiumPct).toFixed(2)}% cheap: buy`}
          </span>
        </div>
        {mine && mine.action !== "pass" ? (
          <div>
            You {actText(mine)} in {mine.secs?.toFixed(1)}s: {signedUsd(mine.score.gross)} gross − {usd(mine.score.cost)}
            {mine.score.bonus > 0 && ` + ${usd(mine.score.bonus)} for being first`} ={" "}
            <b className={mine.score.net >= 0 ? "text-[var(--green)]" : "text-[var(--red)]"}>{signedUsd(mine.score.net)}</b>
            {mine.action !== right && <span className="text-[var(--red)]"> — the wrong side</span>}
          </div>
        ) : (
          <div className="text-[var(--dim)]">{mine?.timedOut ? "Time ran out — no trade." : "You skipped."}</div>
        )}
        <div>
          <BigButton solid onClick={() => onTable(nextRound(table))}>
            <AutoFocus />
            {last ? "results" : "next round"} <Key>↵</Key>
          </BigButton>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-[2.5vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-center gap-[3vw]">
        <div className="min-w-[12em] flex-1">
          {table.stage === "active" && s.seconds !== null ? (
            <Countdown deadline={table.deadline} total={s.seconds} now={now} />
          ) : (
            table.stage === "active" && (
              <span className="text-[var(--dim)]" style={small}>
                no clock — the round ends when everyone has traded
              </span>
            )
          )}
        </div>
        <div aria-label="your P&L" className="text-right">
          <div className="text-[var(--dim)]" style={small}>
            your P&amp;L
          </div>
          <div className={cn("font-mono font-semibold tabular-nums", you.pnl > 0 ? "text-[var(--green)]" : you.pnl < 0 ? "text-[var(--red)]" : "text-[var(--text-strong)]")} style={fs(18, 3.6, 34)}>
            {signedUsd(you.pnl)}
          </div>
        </div>
      </div>
      {m.headline && (
        <div
          className={cn("flex items-center gap-2 rounded-lg border px-[0.9em] py-[0.5em]", m.sentiment?.startsWith("+") ? "border-[var(--green)] text-[var(--green)]" : "border-[var(--red)] text-[var(--red)]")}
          aria-label="headline"
        >
          {m.sentiment?.startsWith("+") ? <TrendingUp style={{ width: "1.1em", height: "1.1em" }} /> : <TrendingDown style={{ width: "1.1em", height: "1.1em" }} />}
          {m.headline}
        </div>
      )}
      <div className="grid gap-[2vmin] md:grid-cols-[7fr_5fr]">
        <MarketTable m={m} rows={table.stage === "intro" ? rows : m.stocks.length} reveal={summary} showWxP={s.showWxP} />
        <Standings table={table} />
      </div>
      <div className="rounded-xl border border-[var(--border)] bg-[var(--bg-panel)] p-[2.5vmin]">{panel}</div>
      <div className="text-[var(--dim)]" style={small}>
        ${COST / 100} a trade · first to trade earns {FIRST_BONUS * 100}% on a profit · <Key>Esc</Key> stops the game
      </div>
    </div>
  );
}

// --- results ------------------------------------------------------------------------

function Results({
  table,
  sessions,
  unsaved,
  onAgain,
  onLobby,
}: {
  table: EtfTable;
  sessions: EtfSessionRecord[];
  unsaved: boolean;
  onAgain: () => void;
  onLobby: () => void;
}) {
  const o = outcome(table);
  const st = etfStanding(sessions);
  const small = fs(12, 1.7, 15);
  const s = table.settings;
  const big = { width: "clamp(40px, 9vmin, 96px)", height: "clamp(40px, 9vmin, 96px)" };
  const name = (id: number | null) => (id === null ? "—" : (table.players.find((p) => p.id === id)?.name ?? "—"));
  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-center gap-[3vw]">
        {o.place === 1 && table.ending === "complete" ? <Crown className="text-[var(--amber)]" style={big} /> : <Scale className="text-[var(--dim)]" style={big} />}
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-[var(--text-strong)]" style={fs(22, 5, 52)}>
            {table.ending === "stopped" ? "Stopped early — played, not a result" : "Market closed"}
          </div>
          <div className="text-[var(--dim)]">
            finished {ordinal(o.place)} of 4 · {table.history.length} round{table.history.length === 1 ? "" : "s"} · {s.level} ·{" "}
            {clockText(s.seconds)} a round · {table.markets[0].basket} basket
          </div>
          {st.best !== null && table.ending === "complete" && (
            <div className="text-[var(--dim)]">
              {st.scored.length === 1
                ? "your first session — the next one is compared with it"
                : `${o.pnl >= st.best ? "a new best session" : `best session ${signedUsd(st.best)}`} · average ${signedUsd(st.average ?? 0)} over ${st.scored.length}`}
            </div>
          )}
        </div>
        <div
          className={cn("font-mono leading-none font-semibold tabular-nums", o.pnl > 0 ? "text-[var(--green)]" : o.pnl < 0 ? "text-[var(--red)]" : "text-[var(--text-strong)]")}
          style={fs(40, 10, 110)}
          aria-label="profit and loss"
        >
          {signedUsd(o.pnl)}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <Tile n={o.roundsWon} label="rounds won" />
        <Tile n={o.trades ? `${o.right}/${o.trades}` : "—"} label="trades on the right side" />
        <Tile n={o.firsts} label="first-trader bonuses" />
        <Tile n={o.avgSecs === null ? "—" : `${o.avgSecs.toFixed(1)}s`} label="to trade, on average" />
      </div>

      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        {o.standings.map((p, i) => (
          <Tile key={p.id} n={signedUsd(p.pnl)} label={`${ordinal(i + 1)} · ${p.name}${p.style ? ` (${p.style})` : ""}`} tone={p.human ? "var(--blue)" : undefined} />
        ))}
      </div>

      <div>
        <Label>round by round</Label>
        <table className="w-full" aria-label="round by round">
          <thead>
            <tr className="text-[var(--dim)]" style={small}>
              <th className="pb-1 text-left font-normal">round</th>
              <th className="pb-1 text-right font-normal">NAV</th>
              <th className="pb-1 text-right font-normal">bid / ask</th>
              <th className="pb-1 text-left font-normal">&nbsp;&nbsp;you</th>
              <th className="pb-1 text-right font-normal">P&amp;L</th>
              <th className="pb-1 text-left font-normal">&nbsp;&nbsp;best</th>
            </tr>
          </thead>
          <tbody>
            {table.history.map((r) => {
              const a = r.acts[YOU];
              return (
                <tr key={r.n} className="border-t border-[var(--border)]" title={r.market.headline ?? undefined}>
                  <td className="py-[0.35em] font-mono">{r.n}</td>
                  <td className="text-right font-mono tabular-nums">{usd(Math.round(r.market.nav * 100))}</td>
                  <td className="text-right font-mono tabular-nums" style={small}>
                    {usd(Math.round(r.market.bid * 100))} / {usd(Math.round(r.market.ask * 100))}
                  </td>
                  <td style={small}>
                    &nbsp;&nbsp;{actText(a)}
                    {a.secs !== null && a.action !== "pass" && ` in ${a.secs.toFixed(1)}s`}
                    {a.score.bonus > 0 && " · first"}
                  </td>
                  <td className={cn("text-right font-mono tabular-nums", a.score.net > 0 && "text-[var(--green)]", a.score.net < 0 && "text-[var(--red)]")}>
                    {signedUsd(a.score.net)}
                  </td>
                  <td style={small}>&nbsp;&nbsp;{name(roundWinner(r))}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      {unsaved && (
        <div className="text-[var(--amber)]" style={small}>
          This session could not be written to {historyPlace("etf-arbitrage")}.
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
