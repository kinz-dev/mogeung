/**
 * The Trading Game — the arcade's second game, after tradermath.org's. The
 * rules and the bots live in [`lib/tradingGame.ts`](../lib/tradingGame.ts),
 * the round-by-round moves in [`lib/tradingTable.ts`](../lib/tradingTable.ts);
 * this file draws the table and keeps time for it.
 *
 * Three pages, as the reference has them: a **lobby** (settings, how to play,
 * your statistics), the **table**, and the **results**. Escape walks back:
 * from the table it stops the game and shows how it went, from the results to
 * the lobby, from the lobby to the arcade.
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { CandlestickChart, Crown, Eye, EyeOff, Zap } from "lucide-react";
import { cn } from "@/lib/cn";
import { historyPlace } from "@/lib/arcade";
import {
  BASE_WORTH,
  DEFAULT_TG,
  LIMITS,
  PENALTY,
  PRESETS,
  STARTING_BALANCE,
  eventText,
  isRed,
  ordinal,
  seeded,
  settlePrice,
  type Card,
  type Difficulty,
  type Rand,
  type TgSettings,
} from "@/lib/tradingGame";
import {
  ackEvent,
  botQuotes,
  makerDone,
  makingTimedOut,
  nextRound,
  outcome,
  roundLine,
  startGame,
  stopGame,
  submitFinal,
  submitReport,
  takingTimedOut,
  yourPnl,
  yourQuote,
  yourTrade,
  type Table,
} from "@/lib/tradingTable";
import { appendSession, loadSessions, money, standing, toSession, type SessionRecord } from "@/lib/tradingStats";
import { ArcadeHeader, AutoFocus, BigButton, Choice, Countdown, Key, Label, NumberBox, Stepper, Tile, Toggle, fs } from "@/ui/arcadeKit";
import { TrendChart } from "@/ui/MentalMathHistory";

type Phase = "lobby" | "table" | "results";

const SETTINGS_KEY = "mogeung.trading.settings";

function loadSettings(): TgSettings {
  try {
    const raw = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "null") as Partial<TgSettings> | null;
    if (!raw || typeof raw !== "object") return DEFAULT_TG;
    const n = (k: keyof typeof LIMITS, v: unknown, d: number) =>
      typeof v === "number" && v >= LIMITS[k][0] && v <= LIMITS[k][1] ? v : d;
    return {
      difficulty: ["easy", "medium", "hard", "custom"].includes(raw.difficulty as string) ? (raw.difficulty as Difficulty) : "easy",
      mode: raw.mode === "openOutcry" ? "openOutcry" : "rotational",
      cards: n("cards", raw.cards, DEFAULT_TG.cards),
      rounds: n("rounds", raw.rounds, DEFAULT_TG.rounds),
      timeToAnswer: n("timeToAnswer", raw.timeToAnswer, DEFAULT_TG.timeToAnswer),
      minSpread: n("minSpread", raw.minSpread, DEFAULT_TG.minSpread),
      maxSpread: n("maxSpread", raw.maxSpread, DEFAULT_TG.maxSpread),
      showBalance: raw.showBalance !== false,
      events: raw.events === true,
    };
  } catch {
    return DEFAULT_TG;
  }
}

export function TradingGame({ onExit, onClose, seed }: { onExit: () => void; onClose: () => void; seed?: number }) {
  const [phase, setPhase] = useState<Phase>("lobby");
  const [settings, setSettings] = useState<TgSettings>(loadSettings);
  const [table, setTable] = useState<Table | null>(null);
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [unsaved, setUnsaved] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const rand = useRef<Rand>(seeded(seed ?? (Date.now() ^ 0x5eed)));

  useEffect(() => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Forgetting a setting is not worth an error.
    }
  }, [settings]);

  useEffect(() => {
    let live = true;
    void loadSessions().then((ss) => live && setSessions((mine) => [...ss, ...mine]));
    return () => {
      live = false;
    };
  }, []);

  const start = () => {
    setUnsaved(false);
    setNow(Date.now());
    setTable(startGame(settings, rand.current, Date.now()));
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
    setTable((t) => (t ? (t.stage === "making" ? makingTimedOut(t) : t.stage === "taking" ? takingTimedOut(t) : t) : t));
  }, [now, table]);

  // The bots' turns: a rotational maker quotes after a moment; in open outcry
  // the first bot shouts somewhere between five and eight seconds in.
  const stageKey = table ? `${table.round.n}:${table.stage}:${table.round.quote ? "q" : "-"}` : "";
  useEffect(() => {
    if (!table || table.stage !== "making" || table.round.quote) return;
    const outcry = table.settings.mode === "openOutcry";
    if (!outcry && table.round.maker === table.you) return;
    const delay = outcry ? 5000 + 3000 * rand.current() : 1500;
    const id = setTimeout(() => setTable((t) => (t ? botQuotes(t, rand.current, Date.now()) : t)), delay);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stageKey]);

  // You made the market: the bots trade against it one by one, then it settles.
  const [shown, setShown] = useState(0);
  useEffect(() => {
    setShown(0);
    if (!table || table.stage !== "taking" || table.round.maker !== table.you) return;
    const bots = Object.keys(table.round.trades).length;
    const ids: ReturnType<typeof setTimeout>[] = [];
    let at = 0;
    for (let i = 1; i <= bots; i++) {
      at += 700 + 900 * rand.current();
      ids.push(setTimeout(() => setShown(i), at));
    }
    ids.push(setTimeout(() => setTable((t) => (t ? makerDone(t) : t)), at + 900));
    return () => ids.forEach(clearTimeout);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stageKey]);

  // The game is over: record it and show how it went — once.
  useEffect(() => {
    if (phase !== "table" || !table || table.stage !== "over") return;
    const record = toSession(table);
    if (record) {
      setSessions((ss) => [...ss, record]);
      void appendSession(record).then((ok) => setUnsaved(!ok));
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
        icon={<CandlestickChart size={20} className="text-[var(--green)]" />}
        title="trading game"
        detail={playing && table ? `round ${table.round.n} of ${table.settings.rounds}` : null}
        onBack={playing ? null : onExit}
        onClose={playing ? () => escape.current() : onClose}
        closeTitle={playing ? "stop the game  (Esc)" : "close  (Esc)"}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1200px] flex-col px-[5vw] py-[3vh]">
          {phase === "lobby" && <Lobby settings={settings} onChange={setSettings} sessions={sessions} onStart={start} />}
          {phase === "table" && table && (
            <TableView table={table} now={now} shown={shown} rand={rand.current} onTable={setTable} />
          )}
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
  settings: TgSettings;
  onChange: (s: TgSettings) => void;
  sessions: SessionRecord[];
  onStart: () => void;
}) {
  const custom = settings.difficulty === "custom";
  const pickPreset = (d: Difficulty) =>
    onChange(d === "custom" ? { ...settings, difficulty: d } : { ...settings, difficulty: d, ...PRESETS[d] });
  const edit = (patch: Partial<TgSettings>) => onChange({ ...settings, ...patch, difficulty: "custom" });
  const st = useMemo(() => standing(sessions), [sessions]);
  const small = fs(12, 1.7, 15);
  const startRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const t = setTimeout(() => startRef.current?.querySelector("button")?.focus(), 0);
    return () => clearTimeout(t);
  }, []);

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="text-[var(--dim)]">
        Quote markets and trade a partly hidden hand of cards against three bots. Each round the cards settle at their
        sum; your edge is reading the face-up cards better than the quote does.
      </div>

      <div className="grid gap-[4vh] lg:grid-cols-2">
        <div className="flex flex-col gap-[2vh]">
          <div>
            <Label>difficulty</Label>
            <div className="flex flex-wrap gap-2">
              {(["easy", "medium", "hard", "custom"] as const).map((d) => (
                <Choice key={d} on={settings.difficulty === d} onClick={() => pickPreset(d)}>
                  {d[0].toUpperCase() + d.slice(1)}
                </Choice>
              ))}
            </div>
          </div>
          <div>
            <Label>market making</Label>
            <div className="flex flex-wrap gap-2">
              <Choice on={settings.mode === "rotational"} onClick={() => onChange({ ...settings, mode: "rotational" })} title="each round another player makes the market — you included">
                Rotational
              </Choice>
              <Choice on={settings.mode === "openOutcry"} onClick={() => onChange({ ...settings, mode: "openOutcry" })} title="everyone shouts a quote at once; the first valid one makes the market">
                Open outcry
              </Choice>
            </div>
          </div>
          <div>
            <Label>{custom ? "settings" : "settings — change any to go custom"}</Label>
            <Stepper label="Cards" value={settings.cards} range={LIMITS.cards} onChange={(v) => edit({ cards: v })} />
            <Stepper label="Rounds" value={settings.rounds} range={LIMITS.rounds} onChange={(v) => edit({ rounds: v })} />
            <Stepper label="Time to answer" value={settings.timeToAnswer} range={LIMITS.timeToAnswer} step={2} unit="s" onChange={(v) => edit({ timeToAnswer: v })} />
            <Stepper label="Min spread" value={settings.minSpread} range={LIMITS.minSpread} unit=" pts" onChange={(v) => edit({ minSpread: Math.min(v, settings.maxSpread) })} />
            <Stepper label="Max spread" value={settings.maxSpread} range={LIMITS.maxSpread} unit=" pts" onChange={(v) => edit({ maxSpread: Math.max(v, settings.minSpread) })} />
            <Toggle label="Show balance" on={settings.showBalance} onChange={(v) => edit({ showBalance: v })} />
            <Toggle label="Market events" on={settings.events} onChange={(v) => edit({ events: v })} />
          </div>
          <div ref={startRef} className="flex items-center gap-3">
            <BigButton solid onClick={onStart}>
              start game <Key>↵</Key>
            </BigButton>
            <span className="text-[var(--dim)]" style={small}>
              {settings.rounds} rounds · 4 players · everyone starts with {STARTING_BALANCE}
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
                  <Tile n={st.wins} label="won outright" />
                  <Tile n={st.exactness === null ? "—" : `${st.exactness}%`} label="reports exact" />
                </div>
                {st.scored.length > 1 && (
                  <div className="mt-[2vh]">
                    <TrendChart
                      values={st.scored.map((s) => s.pnl)}
                      labels={st.scored.map((s) => `${new Date(s.at).toLocaleString()} · ${s.settings.difficulty} · ${ordinal(s.place)}`)}
                      color="var(--green)"
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
        <b>Aim:</b> end with more than you started — {STARTING_BALANCE}.
      </p>
      <p className={p}>
        <b>The hand.</b> Each round deals a few cards, worth 2 to 14 (J 11, Q 12, K 13, A 14). Some are face up. The
        round settles at the sum of all of them.
      </p>
      <p className={p}>
        <b>The quote.</b> One player makes the market: a <i>bid</i> they will buy at and an <i>ask</i> they will sell at,
        written <i>bid at ask</i>. The others may buy at the ask, sell at the bid, or skip. Rotational: the market maker
        rotates, and on your turn you quote — within the spread limits. Open outcry: everyone shouts, and the first
        valid quote makes the market.
      </p>
      <p className={p}>
        <b>Orders must fit.</b> A purchase must be affordable (units × ask ≤ balance). A sale must survive the worst the
        hidden cards could be. An order that does not fit is forfeited and costs {PENALTY}.
      </p>
      <p className={p}>
        <b>Report.</b> After a round you traded in, say your profit or loss — exactly, with a minus for a loss. The
        cards show for five seconds, then turn back. A wrong answer forfeits the round's profit and costs {PENALTY}.
        With the balance hidden, you are asked for it at the end too.
      </p>
      <p className={p}>
        <b>Events.</b> When on, a round may change the deck: only red cards, only 10 and up, a card worth something else…
        They move both the fair value and what a short can lose.
      </p>
      <p className={p}>
        <b>The edge.</b> The bots quote about 8 a card and do not look at the face-up cards. You can.
      </p>
    </div>
  );
}

// --- the table ----------------------------------------------------------------------

export function PlayingCard({ card, faceUp }: { card: Card; faceUp: boolean }) {
  const red = isRed(card.suit);
  const style = { width: "clamp(56px, 11vmin, 120px)", height: "clamp(80px, 15.5vmin, 168px)" };
  if (!faceUp) {
    return (
      <div
        style={style}
        aria-label="a face-down card"
        className="rounded-lg border-2 border-[var(--border)] bg-[repeating-linear-gradient(45deg,var(--bg-panel),var(--bg-panel)_6px,var(--bg-faint)_6px,var(--bg-faint)_12px)]"
      />
    );
  }
  // A market event can re-value one rank for the round; the card says so.
  const revalued = card.worth !== BASE_WORTH[card.rank];
  return (
    <div
      style={style}
      aria-label={`${card.rank}${card.suit}, worth ${card.worth}`}
      className="relative flex flex-col items-center justify-center rounded-lg border-2 border-[var(--border)] bg-[var(--bg-raised)] font-semibold"
    >
      <span style={{ ...fs(20, 4.4, 46), color: red ? "var(--red)" : "var(--text-strong)" }}>{card.rank}</span>
      <span style={{ ...fs(16, 3.4, 36), color: red ? "var(--red)" : "var(--text-strong)" }}>{card.suit}</span>
      {revalued && (
        <span className="absolute bottom-1 rounded bg-[var(--amber)] px-1 font-mono text-[var(--on-accent-dark)]" style={fs(10, 1.6, 14)}>
          = {card.worth}
        </span>
      )}
    </div>
  );
}

function Seats({ table, shown }: { table: Table; shown: number }) {
  const r = table.round;
  const settled = table.stage === "settled";
  const botOrder = Object.keys(r.trades).map(Number).filter((id) => id !== table.you);
  return (
    <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
      {table.players.map((p) => {
        const maker = r.maker === p.id;
        const hidden = p.human && !table.settings.showBalance;
        const a = r.trades[p.id];
        const acted = !p.human && !maker && a && (table.round.maker !== table.you || botOrder.indexOf(p.id) < shown);
        const delta = settled ? (r.pnl[p.id] ?? 0) + (p.human ? r.adjust : 0) : null;
        return (
          <div
            key={p.id}
            className={cn(
              "rounded-lg border-2 px-[0.9em] py-[0.5em]",
              p.human ? "border-[var(--selection-stroke)]" : "border-[var(--border)]",
              "bg-[var(--bg-panel)]",
            )}
            aria-label={`seat ${p.name}`}
          >
            <div className="flex items-center gap-2">
              <span className="font-semibold text-[var(--text-strong)]">{p.name}</span>
              {maker && (
                <span className="rounded bg-[var(--amber)] px-1.5 text-[var(--on-accent-dark)]" style={fs(10, 1.5, 13)}>
                  MAKER
                </span>
              )}
            </div>
            <div className="font-mono tabular-nums" style={fs(16, 3, 28)} aria-label={`${p.name} balance`}>
              {hidden ? <span className="text-[var(--dim)]">hidden</span> : p.balance}
            </div>
            <div className="min-h-[1.4em] text-[var(--dim)]" style={fs(11, 1.6, 14)}>
              {delta !== null && delta !== 0 && !hidden && (
                <span className={delta > 0 ? "text-[var(--green)]" : "text-[var(--red)]"}>{money(delta)} </span>
              )}
              {acted && (settled ? (a.action === "skip" ? "skipped" : `${a.action === "buy" ? "bought" : "sold"} ${a.volume}`) : "✓ decided")}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function TableView({
  table,
  now,
  shown,
  rand,
  onTable,
}: {
  table: Table;
  now: number;
  shown: number;
  rand: Rand;
  onTable: (t: Table) => void;
}) {
  const r = table.round;
  const s = table.settings;
  const [bid, setBid] = useState("");
  const [ask, setAsk] = useState("");
  const [units, setUnits] = useState("");
  const [report, setReport] = useState("");
  // During a report the cards show for five seconds, then turn back — the
  // reference's rule: the sum is yours to hold, not to read off the table.
  const [peek, setPeek] = useState(true);
  useEffect(() => {
    setBid("");
    setAsk("");
    setUnits("");
    setReport("");
    setPeek(true);
    if (table.stage !== "report") return;
    const id = setTimeout(() => setPeek(false), 5000);
    return () => clearTimeout(id);
  }, [table.stage, r.n]);

  const faceUp = (c: Card) =>
    table.stage === "settled" || table.stage === "final" || (table.stage === "report" ? peek : c.open);
  const you = table.players.find((p) => p.human)!;
  const makerName = table.players.find((p) => p.id === r.maker)?.name;
  const small = fs(12, 1.7, 15);

  const num = (v: string) => (v === "" || v === "-" ? NaN : Number(v));
  const quote = () => onTable(yourQuote(table, { bid: num(bid), ask: num(ask) }, rand, Date.now()));
  const trade = (side: "buy" | "sell") => onTable(yourTrade(table, { action: side, volume: num(units) }));
  const skip = () => onTable(yourTrade(table, { action: "skip" }));

  let panel: ReactNode = null;
  switch (table.stage) {
    case "event":
      panel = (
        <div className="flex flex-col items-start gap-3">
          <div className="flex items-center gap-2 text-[var(--amber)]" style={fs(18, 3.4, 32)}>
            <Zap style={{ width: "1em", height: "1em" }} /> Market event
          </div>
          <div className="text-[var(--text-strong)]" style={fs(16, 3, 28)}>
            {r.event && eventText(r.event)}
          </div>
          <BigButton solid onClick={() => onTable(ackEvent(table, Date.now()))}>
            <AutoFocus />
            deal <Key>↵</Key>
          </BigButton>
        </div>
      );
      break;
    case "making": {
      const yourTurn = s.mode === "openOutcry" || r.maker === table.you;
      panel = yourTurn ? (
        <div className="flex flex-col gap-3">
          <div>
            {s.mode === "openOutcry"
              ? "Open outcry — shout your quote first to make the market."
              : "You are the market maker. Quote a bid and an ask."}{" "}
            <span className="text-[var(--dim)]" style={small}>
              spread {s.minSpread}–{s.maxSpread} points
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-3">
            <NumberBox label="bid" value={bid} onChange={setBid} onEnter={quote} autoFocus />
            <span className="text-[var(--dim)]">at</span>
            <NumberBox label="ask" value={ask} onChange={setAsk} onEnter={quote} />
            <BigButton solid onClick={quote}>
              quote <Key>↵</Key>
            </BigButton>
          </div>
          {table.problem && <div className="text-[var(--red)]">{table.problem}</div>}
          <Countdown deadline={table.deadline} total={s.mode === "openOutcry" ? 15 : s.timeToAnswer} now={now} />
        </div>
      ) : (
        <div className="text-[var(--dim)]">{makerName} is making the market…</div>
      );
      break;
    }
    case "taking":
      panel =
        r.maker === table.you ? (
          <div className="text-[var(--dim)]">The others are trading against your quote…</div>
        ) : (
          <div className="flex flex-col gap-3">
            <div>
              How many units? <span className="text-[var(--dim)]" style={small}>
                buy at {r.quote?.ask}, sell at {r.quote?.bid} — <Key>B</Key> buy · <Key>S</Key> sell · <Key>K</Key> skip
              </span>
            </div>
            <div className="flex flex-wrap items-center gap-3">
              <NumberBox
                label="units"
                value={units}
                onChange={setUnits}
                onEnter={() => undefined}
                onKey={(k) => {
                  if (k === "b") trade("buy");
                  else if (k === "s") trade("sell");
                  else if (k === "k") skip();
                  else return false;
                  return true;
                }}
                autoFocus
                width="5em"
              />
              <BigButton solid onClick={() => trade("buy")}>
                buy at {r.quote?.ask}
              </BigButton>
              <BigButton solid onClick={() => trade("sell")}>
                sell at {r.quote?.bid}
              </BigButton>
              <BigButton onClick={skip}>skip</BigButton>
            </div>
            {table.problem && <div className="text-[var(--red)]">{table.problem}</div>}
            <Countdown deadline={table.deadline} total={s.timeToAnswer} now={now} />
          </div>
        );
      break;
    case "report": {
      const mine = r.you.kind === "trade" ? r.you : null;
      panel = (
        <div className="flex flex-col gap-3">
          <div>
            You {mine?.side === "buy" ? "bought" : "sold"} {mine?.volume} at {mine?.side === "buy" ? r.quote?.ask : r.quote?.bid}.{" "}
            <b>What was your profit or loss this round?</b>
          </div>
          <div className="text-[var(--dim)]" style={small}>
            Exact answers only: a wrong one forfeits this round's profit and costs {PENALTY}. A minus sign for a loss.
          </div>
          <div className="flex items-center gap-3">
            <NumberBox label="your profit or loss" value={report} onChange={setReport} onEnter={() => report !== "" && report !== "-" && onTable(submitReport(table, Number(report)))} autoFocus />
            <BigButton solid onClick={() => report !== "" && report !== "-" && onTable(submitReport(table, Number(report)))}>
              report <Key>↵</Key>
            </BigButton>
          </div>
        </div>
      );
      break;
    }
    case "settled": {
      const price = r.price ?? settlePrice(r.cards);
      const mine = yourPnl(r, table.you);
      const last = r.n >= s.rounds || you.balance < 0;
      panel = (
        <div className="flex flex-col gap-2">
          <div style={fs(16, 3, 28)}>
            Settled at <b className="font-mono">{price}</b>
            {r.quote && (
              <span className="text-[var(--dim)]">
                {" "}
                · quote {r.quote.bid} at {r.quote.ask}
              </span>
            )}
          </div>
          {r.skipped && <div className="text-[var(--dim)]">{r.skipped}</div>}
          {r.you.kind === "trade" && (
            <div>
              You {r.you.side === "buy" ? "bought" : "sold"} {r.you.volume}:{" "}
              <b className={mine >= 0 ? "text-[var(--green)]" : "text-[var(--red)]"}>{money(mine)}</b>
              {r.reported !== null &&
                (r.reported === mine ? (
                  <span className="text-[var(--green)]"> — you reported it exactly</span>
                ) : (
                  <span className="text-[var(--red)]">
                    {" "}
                    — you reported {r.reported}; {mine >= 0 ? "profit forfeited and " : ""}−{PENALTY}
                  </span>
                ))}
            </div>
          )}
          {r.you.kind === "forfeit" && (
            <div className="text-[var(--red)]">
              Your order to {r.you.side} {r.you.volume} did not fit your balance — forfeited, −{PENALTY}.
            </div>
          )}
          {r.you.kind === "skip" && <div className="text-[var(--dim)]">You skipped.</div>}
          {r.you.kind === "timeout" && <div className="text-[var(--dim)]">Time ran out — no trade.</div>}
          {r.feedback && (
            <div>
              As market maker: <b className={mine >= 0 ? "text-[var(--green)]" : "text-[var(--red)]"}>{money(mine)}</b> ·{" "}
              <b>{r.feedback.title}.</b> <span className="text-[var(--dim)]">{r.feedback.body}</span>
            </div>
          )}
          {you.balance < 0 && <div className="text-[var(--red)]">Your balance has gone negative — the game ends here.</div>}
          <div>
            <BigButton solid onClick={() => onTable(nextRound(table, rand, Date.now()))}>
              <AutoFocus />
              {last ? "finish" : "next round"} <Key>↵</Key>
            </BigButton>
          </div>
        </div>
      );
      break;
    }
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
            <NumberBox label="your final balance" value={report} onChange={setReport} onEnter={() => report !== "" && report !== "-" && onTable(submitFinal(table, Number(report)))} autoFocus />
            <BigButton solid onClick={() => report !== "" && report !== "-" && onTable(submitFinal(table, Number(report)))}>
              answer <Key>↵</Key>
            </BigButton>
          </div>
        </div>
      );
      break;
  }

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <Seats table={table} shown={shown} />
      {r.event && table.stage !== "event" && (
        <div className="flex items-center gap-2 text-[var(--amber)]" style={small}>
          <Zap style={{ width: "1em", height: "1em" }} /> {eventText(r.event)}
        </div>
      )}
      <div className="flex flex-wrap items-end justify-center gap-[2vmin]" aria-label="the hand">
        {r.cards.map((c, i) => (
          <PlayingCard key={`${r.n}-${i}`} card={c} faceUp={table.stage !== "event" && faceUp(c)} />
        ))}
      </div>
      <div className="flex items-baseline justify-center gap-3" aria-label="the quote">
        {r.quote ? (
          <>
            <span className="text-[var(--dim)]">{r.maker === table.you ? "You quote" : `${makerName} quotes`}</span>
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

// --- results ------------------------------------------------------------------------

function Results({
  table,
  sessions,
  unsaved,
  onAgain,
  onLobby,
}: {
  table: Table;
  sessions: SessionRecord[];
  unsaved: boolean;
  onAgain: () => void;
  onLobby: () => void;
}) {
  const o = outcome(table);
  const st = standing(sessions);
  const small = fs(12, 1.7, 15);
  const headline =
    table.ending === "bust" ? "Bust — your balance went negative" : table.ending === "stopped" ? "Stopped early — played, not a result" : "Market closed";
  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-center gap-[3vw]">
        {o.place === 1 && table.ending === "complete" ? (
          <Crown className="text-[var(--amber)]" style={{ width: "clamp(40px, 9vmin, 96px)", height: "clamp(40px, 9vmin, 96px)" }} />
        ) : (
          <CandlestickChart className="text-[var(--dim)]" style={{ width: "clamp(40px, 9vmin, 96px)", height: "clamp(40px, 9vmin, 96px)" }} />
        )}
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-[var(--text-strong)]" style={fs(22, 5, 52)}>
            {headline}
          </div>
          <div className="text-[var(--dim)]">
            finished {ordinal(o.place)} of 4 · {table.history.length} round{table.history.length === 1 ? "" : "s"} ·{" "}
            {table.settings.difficulty} · {table.settings.mode === "rotational" ? "rotational" : "open outcry"}
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
        {o.standings.map((p, i) => (
          <Tile
            key={p.id}
            n={p.balance}
            label={`${ordinal(i + 1)} · ${p.name}`}
            tone={p.human ? "var(--blue)" : undefined}
          />
        ))}
      </div>

      <div>
        <Label>round by round</Label>
        <table className="w-full" aria-label="round by round">
          <thead>
            <tr className="text-[var(--dim)]" style={small}>
              <th className="pb-1 text-left font-normal">round</th>
              <th className="pb-1 text-left font-normal">hand</th>
              <th className="pb-1 text-right font-normal">quote</th>
              <th className="pb-1 text-right font-normal">settled</th>
              <th className="pb-1 text-left font-normal">&nbsp;&nbsp;you</th>
              <th className="pb-1 text-right font-normal">P&amp;L</th>
            </tr>
          </thead>
          <tbody>
            {table.history.map((r) => {
              const mine = yourPnl(r, table.you) + r.adjust;
              return (
                <tr key={r.n} className="border-t border-[var(--border)]">
                  <td className="py-[0.35em] font-mono">{r.n}</td>
                  <td className="font-mono" style={small}>
                    {r.cards.map((c) => `${c.rank}${c.suit}`).join(" ")}
                  </td>
                  <td className="text-right font-mono tabular-nums">{r.quote ? `${r.quote.bid}–${r.quote.ask}` : "—"}</td>
                  <td className="text-right font-mono tabular-nums">{r.price ?? settlePrice(r.cards)}</td>
                  <td style={small}>&nbsp;&nbsp;{roundLine(r, table)}</td>
                  <td className={cn("text-right font-mono tabular-nums", mine > 0 && "text-[var(--green)]", mine < 0 && "text-[var(--red)]")}>
                    {money(mine)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {o.reports > 0 && (
          <div className="mt-1 text-[var(--dim)]" style={small}>
            You reported {o.exact} of {o.reports} round{o.reports === 1 ? "" : "s"} exactly.
          </div>
        )}
      </div>

      {unsaved && (
        <div className="text-[var(--amber)]" style={small}>
          This session could not be written to {historyPlace("trading-game")}.
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
