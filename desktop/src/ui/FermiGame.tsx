/**
 * The Fermi Estimation Game — the arcade's sixth, after tradermath.org's. The
 * questions and the scoring live in [`lib/fermi.ts`](../lib/fermi.ts), a game
 * in progress in [`lib/fermiRun.ts`](../lib/fermiRun.ts); this file draws it
 * and keeps time for it.
 *
 * Three pages: a **lobby**, the **questions** — each asked, then revealed —
 * and the **results**. Escape walks back: from a question it stops the game,
 * from the results to the lobby, from the lobby to the arcade.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { Lightbulb, Ruler, Target } from "lucide-react";
import { cn } from "@/lib/cn";
import { historyPlace } from "@/lib/arcade";
import { clockFrom, clockText } from "@/lib/arcadeClock";
import {
  BASE_BANDS,
  CREDIT,
  DEFAULT_FERMI,
  MAX_POINTS,
  PRESETS,
  QUESTIONS,
  SPREAD_BANDS,
  factorText,
  formatAmount,
  parseAmount,
  type FermiSettings,
  type Pool,
} from "@/lib/fermi";
import { current, fermiOutcome, nextQuestion, showHint, skip, startRun, stopRun, submit, timedOut, type FermiAnswer, type FermiRun } from "@/lib/fermiRun";
import { appendFermiSession, fermiStanding, loadFermiSessions, sessionShare, toFermiSession, type FermiSessionRecord } from "@/lib/fermiStats";
import { seeded, type Rand } from "@/lib/tradingGame";
import { ArcadeHeader, AutoFocus, BigButton, Choice, ClockSetting, Countdown, Key, Label, Stepper, Tile, Toggle, fs } from "@/ui/arcadeKit";
import { TrendChart } from "@/ui/MentalMathHistory";

type Phase = "lobby" | "play" | "results";

const SETTINGS_KEY = "mogeung.fermi.settings";
const POOLS: Pool[] = ["easy", "medium", "hard", "mixed"];
const title = (s: string) => s[0].toUpperCase() + s.slice(1);

function loadSettings(): FermiSettings {
  try {
    const raw = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "null") as Partial<FermiSettings> | null;
    if (!raw || typeof raw !== "object") return DEFAULT_FERMI;
    const n = raw.questions;
    return {
      pool: POOLS.includes(raw.pool as Pool) ? (raw.pool as Pool) : "easy",
      questions: typeof n === "number" && Number.isInteger(n) && n >= QUESTIONS[0] && n <= QUESTIONS[1] ? n : DEFAULT_FERMI.questions,
      seconds: clockFrom(raw.seconds, DEFAULT_FERMI.seconds),
      hints: raw.hints !== false,
    };
  } catch {
    return DEFAULT_FERMI;
  }
}

export function FermiGame({ onExit, onClose, seed }: { onExit: () => void; onClose: () => void; seed?: number }) {
  const [phase, setPhase] = useState<Phase>("lobby");
  const [settings, setSettings] = useState<FermiSettings>(loadSettings);
  const [run, setRun] = useState<FermiRun | null>(null);
  const [sessions, setSessions] = useState<FermiSessionRecord[]>([]);
  const [unsaved, setUnsaved] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const rand = useRef<Rand>(seeded(seed ?? (Date.now() ^ 0xfe41)));

  useEffect(() => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      // Forgetting a setting is not worth an error.
    }
  }, [settings]);

  useEffect(() => {
    let live = true;
    void loadFermiSessions().then((ss) => live && setSessions((mine) => [...ss, ...mine]));
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
    const record = toFermiSession(run);
    if (record) {
      setSessions((ss) => [...ss, record]);
      void appendFermiSession(record).then((ok) => setUnsaved(!ok));
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
        icon={<Ruler size={20} className="text-[var(--purple)]" />}
        title="fermi estimation"
        detail={playing && run ? `question ${run.i + 1} of ${run.questions.length}` : null}
        onBack={playing ? null : onExit}
        onClose={playing ? () => escape.current() : onClose}
        closeTitle={playing ? "stop the game  (Esc)" : "close  (Esc)"}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1100px] flex-col px-[5vw] py-[3vh]">
          {phase === "lobby" && <Lobby settings={settings} onChange={setSettings} sessions={sessions} onStart={start} />}
          {phase === "play" && run && <Question run={run} now={now} onRun={setRun} />}
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
  settings: FermiSettings;
  onChange: (s: FermiSettings) => void;
  sessions: FermiSessionRecord[];
  onStart: () => void;
}) {
  const pick = (p: Pool) => onChange(p === "mixed" ? { ...settings, pool: p } : { ...settings, pool: p, ...PRESETS[p] });
  const own = settings.pool === "mixed" ? null : PRESETS[settings.pool].seconds;
  const st = useMemo(() => fermiStanding(sessions), [sessions]);
  const small = fs(12, 1.7, 15);
  const startRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const t = setTimeout(() => startRef.current?.querySelector("button")?.focus(), 0);
    return () => clearTimeout(t);
  }, []);

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="text-[var(--dim)]">
        Answer questions nobody knows offhand with a range — tight when you know, wide when you don't — scored on a log
        scale, so being out by ×2 costs the same either way.
      </div>
      <div className="grid gap-[4vh] lg:grid-cols-2">
        <div className="flex flex-col gap-[2vh]">
          <div>
            <Label>questions</Label>
            <div className="flex flex-wrap gap-2">
              {POOLS.map((p) => (
                <Choice key={p} on={settings.pool === p} onClick={() => pick(p)} title={p === "mixed" ? "all three levels, shuffled" : undefined}>
                  {title(p)}
                </Choice>
              ))}
            </div>
          </div>
          <div>
            <Label>length, clock and hints — yours at any level</Label>
            <Stepper label="Questions" value={settings.questions} range={QUESTIONS} onChange={(v) => onChange({ ...settings, questions: v })} />
            <ClockSetting
              label="Time per question"
              value={settings.seconds}
              fallback={own ?? 30}
              hint={own !== null && settings.seconds !== own ? `${title(settings.pool)} is ${clockText(own)} as it comes` : "a question left unanswered scores nothing"}
              onChange={(v) => onChange({ ...settings, seconds: v })}
            />
            <Toggle label="Show hints" on={settings.hints} onChange={(v) => onChange({ ...settings, hints: v })} />
          </div>
          <div ref={startRef} className="flex items-center gap-3">
            <BigButton solid onClick={onStart}>
              start game <Key>↵</Key>
            </BigButton>
            <span className="text-[var(--dim)]" style={small}>
              {settings.questions} questions · {clockText(settings.seconds)} each · up to {MAX_POINTS} points a question
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
                  <Tile n={st.best === null ? "—" : `${st.best}%`} label="best score" />
                  <Tile n={st.latest === null ? "—" : `${st.latest}%`} label="latest" />
                  <Tile n={st.average === null ? "—" : `${st.average}%`} label="average" />
                  <Tile n={st.hitRate === null ? "—" : `${st.hitRate}%`} label="ranges that held" />
                  <Tile n={st.typicalFactor === null ? "—" : `×${st.typicalFactor.toFixed(1)}`} label="typical miss" />
                </div>
                {st.tooLow !== null && (
                  <div className="mt-1 text-[var(--dim)]" style={small}>
                    When you miss, the answer is above your range {st.tooLow}% of the time
                    {st.tooLow >= 65 ? " — you guess low" : st.tooLow <= 35 ? " — you guess high" : ""}.
                  </div>
                )}
                {st.scored.length > 1 && (
                  <div className="mt-[2vh]">
                    <TrendChart
                      values={st.scored.map(sessionShare)}
                      labels={st.scored.map((s) => `${new Date(s.at).toLocaleString()} · ${s.settings.pool} · ${s.points}/${s.max}`)}
                      color="var(--purple)"
                      format={(n) => `${Math.round(n)}%`}
                      label="score as a share of the most possible, session by session"
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
  const th = "pb-1 text-left font-normal";
  return (
    <div style={fs(12, 1.8, 16)}>
      <p className={p}>
        <b>Answer with a range.</b> A low and a high, in the unit shown. Type numbers however you like: 86400, 86,400,
        8.1e9, 8.1bn, 3.5k, 2×10^6.
      </p>
      <p className={p}>
        <b>Scoring.</b> Every answer starts with {CREDIT} points. If the answer is inside your range, add the points for how
        close your range's <i>geometric</i> midpoint came, times a bonus for how narrow the range was — up to {MAX_POINTS}.
        If it is outside, you keep some of the {CREDIT}: nothing for a tight miss, more for a wide one. A skip or a timeout
        scores 0.
      </p>
      <div className="mt-2 grid gap-4 text-[var(--dim)] sm:grid-cols-2">
        <table>
          <thead>
            <tr>
              <th className={th}>midpoint off by</th>
              <th className={th}>points</th>
            </tr>
          </thead>
          <tbody>
            {BASE_BANDS.map((b) => (
              <tr key={b.label}>
                <td>{b.upTo === Infinity ? "more than ×10" : `up to ${factorText(b.upTo)}`}</td>
                <td className="font-mono">+{b.points}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <table>
          <thead>
            <tr>
              <th className={th}>range wide</th>
              <th className={th}>hit ×</th>
              <th className={th}>miss keeps</th>
            </tr>
          </thead>
          <tbody>
            {SPREAD_BANDS.map((b) => (
              <tr key={b.upTo}>
                <td>{b.upTo === Infinity ? "over ×10" : `up to ${factorText(b.upTo)}`}</td>
                <td className="font-mono">×{b.mult}</td>
                <td className="font-mono">{b.missKeeps}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className={p}>
        <b>Why a log scale.</b> Guessing 50 for an answer of 100 is as wrong as guessing 200. Estimate the order of
        magnitude first, then narrow it.
      </p>
    </div>
  );
}

// --- a question ------------------------------------------------------------------

function AmountBox({ label, value, onChange, onEnter, autoFocus }: { label: string; value: string; onChange: (v: string) => void; onEnter: () => void; autoFocus?: boolean }) {
  const read = value.trim() ? parseAmount(value) : null;
  return (
    <label className="flex flex-col gap-1">
      <span className="text-[var(--dim)]" style={fs(11, 1.6, 14)}>
        {label}
      </span>
      <input
        aria-label={label}
        autoFocus={autoFocus}
        autoComplete="off"
        spellCheck={false}
        value={value}
        placeholder={label === "low" ? "e.g. 2e6" : "e.g. 8bn"}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onEnter();
          }
        }}
        style={{ ...fs(18, 3.6, 34), width: "9em" }}
        className="rounded-lg border-2 border-[var(--border)] bg-[var(--bg-panel)] px-3 py-[0.1em] text-center font-mono text-[var(--text-strong)] outline-none focus:border-[var(--selection-stroke)]"
      />
      <span className={cn("min-h-[1.3em]", read === null && value.trim() ? "text-[var(--red)]" : "text-[var(--dim)]")} style={fs(11, 1.6, 14)}>
        {value.trim() ? (read === null ? "not a number" : formatAmount(read)) : " "}
      </span>
    </label>
  );
}

function Question({ run, now, onRun }: { run: FermiRun; now: number; onRun: (r: FermiRun) => void }) {
  const q = current(run);
  const [low, setLow] = useState("");
  const [high, setHigh] = useState("");
  useEffect(() => {
    setLow("");
    setHigh("");
  }, [run.i]);
  const small = fs(12, 1.7, 15);
  const lo = low.trim() ? parseAmount(low) : null;
  const hi = high.trim() ? parseAmount(high) : null;
  const send = () => onRun(submit(run, low, high, Date.now()));
  const reveal = run.stage === "reveal" ? run.answers[run.answers.length - 1] : null;

  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex items-baseline justify-between gap-3 text-[var(--dim)]" style={small}>
        <span>
          {q.level} · question {run.i + 1} of {run.questions.length}
        </span>
        <span aria-label="points so far">{run.answers.reduce((t, a) => t + a.points, 0)} points so far</span>
      </div>
      <div className="font-semibold text-[var(--text-strong)]" style={fs(22, 4.6, 46)} aria-label="the question">
        {q.text}
      </div>
      <div className="text-[var(--dim)]">
        in <b className="text-[var(--text)]">{q.unit}</b>
      </div>

      {run.stage === "asking" && (
        <>
          <div className="flex flex-wrap items-start gap-4">
            <AmountBox label="low" value={low} onChange={setLow} onEnter={send} autoFocus />
            <span className="pt-[1.6em] text-[var(--dim)]">to</span>
            <AmountBox label="high" value={high} onChange={setHigh} onEnter={send} />
            <div className="flex flex-col gap-2 pt-[1.3em]">
              <BigButton solid onClick={send}>
                answer <Key>↵</Key>
              </BigButton>
            </div>
          </div>
          <div className="text-[var(--dim)]" style={small} aria-label="your range">
            {lo !== null && hi !== null && lo > 0 && hi >= lo
              ? `midpoint ${formatAmount(Math.sqrt(lo * hi))} · a range ${factorText(Math.log10(hi / lo))} wide`
              : "a low and a high — the answer should fall between them"}
          </div>
          {run.problem && <div className="text-[var(--red)]">{run.problem}</div>}
          {run.settings.hints && (
            <div className="flex flex-col gap-2">
              {q.hints.slice(0, run.hintsShown).map((h) => (
                <div key={h} className="flex items-center gap-2 text-[var(--amber)]">
                  <Lightbulb style={{ width: "1em", height: "1em" }} /> {h}
                </div>
              ))}
              {run.hintsShown < q.hints.length && (
                <div>
                  <BigButton onClick={() => onRun(showHint(run))}>show a hint</BigButton>
                </div>
              )}
            </div>
          )}
          <div className="flex flex-wrap items-center gap-3">
            {run.settings.seconds === null ? (
              <span className="text-[var(--dim)]" style={small}>
                no clock — take your time
              </span>
            ) : (
              <div className="min-w-[12em] flex-1">
                <Countdown deadline={run.deadline} total={run.settings.seconds} now={now} />
              </div>
            )}
            <BigButton onClick={() => onRun(skip(run, Date.now()))}>skip</BigButton>
          </div>
        </>
      )}

      {reveal && <Reveal a={reveal} last={run.i + 1 >= run.questions.length} onNext={() => onRun(nextQuestion(run, Date.now()))} />}
    </div>
  );
}

function Reveal({ a, last, onNext }: { a: FermiAnswer; last: boolean; onNext: () => void }) {
  const s = a.scored;
  const small = fs(12, 1.7, 15);
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-[var(--border)] bg-[var(--bg-panel)] p-[2.5vmin]">
      <div>
        <span className="text-[var(--dim)]">The answer: </span>
        <b className="font-mono text-[var(--text-strong)]" style={fs(20, 4, 38)} aria-label="the answer">
          {formatAmount(a.question.answer)}
        </b>{" "}
        <span className="text-[var(--dim)]">
          {a.question.unit}
          {a.question.note && ` · ${a.question.note}`}
        </span>
      </div>
      {s ? (
        <>
          <div>
            You said {formatAmount(a.low!)} to {formatAmount(a.high!)} — midpoint {formatAmount(s.mid)}, {factorText(s.distance)} off
            {s.distance < 0.005 ? " (spot on)" : a.question.answer > s.mid ? ", low" : ", high"}; a range {factorText(s.spread)} wide.
          </div>
          <div className={s.hit ? "text-[var(--green)]" : "text-[var(--red)]"}>
            {s.hit ? (
              <>
                <Target className="inline" style={{ width: "1em", height: "1em" }} /> Inside your range — {s.base.label}:{" "}
                {CREDIT} + {s.base.points} × {s.band.mult} = <b>{a.points}</b>
              </>
            ) : (
              <>
                Outside your range, {a.question.answer > a.high! ? "above" : "below"} it — a range that wide keeps{" "}
                <b>{a.points}</b> of the {CREDIT}.
              </>
            )}
          </div>
        </>
      ) : (
        <div className="text-[var(--dim)]">{a.outcome === "timeout" ? "Time ran out — no points." : "Skipped — no points."}</div>
      )}
      {a.hints > 0 && (
        <div className="text-[var(--dim)]" style={small}>
          with {a.hints} hint{a.hints === 1 ? "" : "s"}
        </div>
      )}
      <div>
        <BigButton solid onClick={onNext}>
          <AutoFocus />
          {last ? "results" : "next question"} <Key>↵</Key>
        </BigButton>
      </div>
    </div>
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
  run: FermiRun;
  sessions: FermiSessionRecord[];
  unsaved: boolean;
  onAgain: () => void;
  onLobby: () => void;
}) {
  const o = fermiOutcome(run);
  const st = fermiStanding(sessions);
  const share = o.max ? Math.round((o.points / o.max) * 100) : 0;
  const small = fs(12, 1.7, 15);
  return (
    <div className="flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex flex-wrap items-center gap-[3vw]">
        <Ruler className="text-[var(--purple)]" style={{ width: "clamp(40px, 9vmin, 96px)", height: "clamp(40px, 9vmin, 96px)" }} />
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-[var(--text-strong)]" style={fs(22, 5, 52)}>
            {run.ending === "stopped" ? "Stopped early — played, not a result" : "All answered"}
          </div>
          <div className="text-[var(--dim)]">
            {run.answers.length} question{run.answers.length === 1 ? "" : "s"} · {run.settings.pool} · {clockText(run.settings.seconds)} each
            {run.settings.hints ? " · hints on" : ""}
          </div>
          {st.best !== null && run.ending === "complete" && (
            <div className="text-[var(--dim)]">
              {st.scored.length === 1
                ? "your first session — the next one is compared with it"
                : `${share >= st.best ? "a new best" : `best ${st.best}%`} · average ${st.average}% over ${st.scored.length}`}
            </div>
          )}
        </div>
        <div className="text-right">
          <div className="font-mono leading-none font-semibold text-[var(--text-strong)] tabular-nums" style={fs(40, 10, 110)} aria-label="score">
            {o.points}
          </div>
          <div className="text-[var(--dim)]" style={small}>
            of {o.max} · {share}%
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 md:grid-cols-3">
        <Tile n={o.answered ? `${o.hits}/${o.answered}` : "—"} label="ranges that held the answer" />
        <Tile n={o.medianDistance === null ? "—" : factorText(o.medianDistance)} label="typical miss of your midpoint" />
        <Tile n={run.answers.filter((a) => a.outcome !== "answered").length} label="skipped or out of time" />
      </div>

      <div>
        <Label>question by question</Label>
        <table className="w-full" aria-label="question by question">
          <thead>
            <tr className="text-[var(--dim)]" style={small}>
              <th className="pb-1 text-left font-normal">question</th>
              <th className="pb-1 text-right font-normal">answer</th>
              <th className="pb-1 text-right font-normal">your range</th>
              <th className="pb-1 text-right font-normal">points</th>
            </tr>
          </thead>
          <tbody>
            {run.answers.map((a) => (
              <tr key={a.question.id} className="border-t border-[var(--border)]">
                <td className="py-[0.35em] pr-3" style={small}>
                  {a.question.text}
                </td>
                <td className="text-right font-mono whitespace-nowrap tabular-nums" style={small}>
                  {formatAmount(a.question.answer)}
                </td>
                <td className={cn("text-right font-mono whitespace-nowrap tabular-nums", a.scored?.hit ? "text-[var(--green)]" : "text-[var(--dim)]")} style={small}>
                  {a.scored ? `${formatAmount(a.low!)} – ${formatAmount(a.high!)}` : a.outcome === "timeout" ? "out of time" : "skipped"}
                </td>
                <td className="text-right font-mono tabular-nums">{a.points}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {unsaved && (
        <div className="text-[var(--amber)]" style={small}>
          This session could not be written to {historyPlace("fermi-estimation")}.
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
