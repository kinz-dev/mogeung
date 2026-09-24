/**
 * The mental-math game. [`lib/mentalMath.ts`](../lib/mentalMath.ts) says whose
 * rules it plays by; [`Arcade`](Arcade.tsx) is the way in and the room it plays
 * in, since the Trading Game joined it on 2026-09-24.
 *
 * **It takes the whole window**, and its type is sized from the window rather
 * than from the app's compact scale. It started as a 580px dialog in the
 * app's 13px type, which read as a form rather than a game and did not grow
 * with a big display — asked on 2026-09-23 to be "much bigger … a full
 * application screen".
 *
 * Its own surface rather than `Dialog`, for one reason: **Escape**. In a
 * dialog it closes. Here, mid-run, it ends the run and shows how it went —
 * throwing away two minutes of work on the key you press to stop would be the
 * cruellest thing a game could do.
 */

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { BookOpen, Dumbbell, Flag, Infinity as Endless, LineChart, Sigma, Sparkles, ThumbsUp, Trophy } from "lucide-react";
import { cn } from "@/lib/cn";
import {
  DIFFICULTIES,
  DURATIONS,
  LEVELS,
  OPS,
  QUESTION_CLOCKS,
  STANDARD,
  SYMBOL,
  answer,
  bestKey,
  cleanAnswer,
  endRun,
  giveUp,
  loadSaved,
  mixedHint,
  opHint,
  questionLeft,
  questionSeconds,
  recordBest,
  save,
  startRun,
  summarize,
  tick,
  verdict,
  type QuestionClock,
  type Run,
  type Settings,
} from "@/lib/mentalMath";
import { ArcadeHeader, BigButton, COLOR, Choice, Key, Label, OP_NAME, clock, fs, secs } from "@/ui/arcadeKit";
import { History } from "@/ui/MentalMathHistory";
import { againstRecent, appendHistory, loadHistory, toRecord, type GameRecord } from "@/lib/mentalMathStats";

type Phase = "setup" | "playing" | "results" | "history";

function lengthLabel(s: Settings): string {
  return s.duration === null ? "endless" : `${s.duration / 60} min`;
}

/** The settings a run was played at, as its header and results name them. */
function runLabel(s: Settings): string {
  const clock = s.question === "auto" ? "" : s.question === "off" ? " · no question timer" : ` · ${s.question}s a question`;
  return `${LEVELS[s.difficulty].label} · ${lengthLabel(s)}${clock}`;
}

function clockChoice(q: QuestionClock, s: Settings): string {
  if (q === "auto") return `auto · ${s.duration === null ? "none" : `${LEVELS[s.difficulty].seconds}s`}`;
  return q === "off" ? "off" : `${q}s`;
}

/** What the two clocks will do, in one sentence. */
function clockLine(s: Settings): string {
  const q = questionSeconds(s);
  if (s.duration === null) {
    return q === null
      ? "no clock on the run or on a question — train until you stop, then Esc for how it went"
      : `no clock on the run, but a question left ${q}s skips itself — train until you stop, then Esc for how it went`;
  }
  return q === null
    ? "solve as many as you can; a question waits as long as you do"
    : `solve as many as you can; a question left ${q}s skips itself, no penalty`;
}

/**
 * One game of mental maths, drawn inside the arcade's full-window shell: its
 * own header and its own pages. Escape steps back a page — from a run to its
 * results, from its first page to the arcade's picker.
 */
export function MentalMathGame({ onExit, onClose }: { onExit: () => void; onClose: () => void }) {
  const [saved] = useState(loadSaved);
  const [settings, setSettings] = useState(saved.settings);
  const [bests, setBests] = useState(saved.bests);
  const [phase, setPhase] = useState<Phase>("setup");
  const [run, setRun] = useState<Run | null>(null);
  const [record, setRecord] = useState<{ previous: number | null; isNew: boolean } | null>(null);
  const [now, setNow] = useState(() => Date.now());
  // Every game played, oldest first — read once on opening, then added to as
  // games end, so the history screen never waits on a file.
  const [records, setRecords] = useState<GameRecord[]>([]);
  const [against, setAgainst] = useState<ReturnType<typeof againstRecent>>(null);
  const [unsaved, setUnsaved] = useState(false);
  const [back, setBack] = useState<Phase>("setup");

  useEffect(() => {
    let live = true;
    void loadHistory().then((rs) => live && setRecords((mine) => [...rs, ...mine]));
    return () => {
      live = false;
    };
  }, []);

  const showHistory = () => {
    setBack(phase === "results" ? "results" : "setup");
    setPhase("history");
  };

  // Kept from the first frame, so even a game closed at once is remembered
  // as found.
  useEffect(() => save({ settings, bests, found: true }), [settings, bests]);

  const start = () => {
    setRecord(null);
    setNow(Date.now());
    setRun(startRun(settings, Date.now()));
    setPhase("playing");
  };

  const escape = useRef(() => {});
  escape.current = () => {
    if (phase === "playing") setRun((r) => r && endRun(r, Date.now()));
    else if (phase === "history") setPhase(back);
    // Pages walk back one at a time: results to setup, setup to the arcade.
    else if (phase === "results") setPhase("setup");
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

  useEffect(() => {
    if (phase !== "playing") return;
    const id = setInterval(() => {
      const t = Date.now();
      setNow(t);
      setRun((r) => r && tick(r, t));
    }, 100);
    return () => clearInterval(id);
  }, [phase]);

  // A run that has ended — on its clock or by hand — is scored exactly once.
  useEffect(() => {
    if (phase !== "playing" || !run || run.endedAt === null) return;
    const r = recordBest(bests, run);
    setBests(r.bests);
    setRecord({ previous: r.previous, isNew: r.isNew });
    // Kept: every game that asked anything, stopped early or not — a stopped
    // game is not a score, but its answers are still how fast you were.
    const game = toRecord(run);
    if (game) {
      setAgainst(againstRecent([...records, game], game));
      setRecords((rs) => [...rs, game]);
      void appendHistory(game).then((ok) => setUnsaved(!ok));
    } else {
      setAgainst(null);
    }
    setPhase("results");
  }, [phase, run, bests, records]);

  return (
    <>
      <ArcadeHeader
        icon={<Sigma size={20} className="text-[var(--purple)]" />}
        title="mental math"
        detail={phase === "playing" && run ? runLabel(run.settings) : null}
        onBack={phase === "playing" ? null : onExit}
        onClose={phase === "playing" ? () => escape.current() : onClose}
        closeTitle={phase === "playing" ? "end the run  (Esc)" : "close  (Esc)"}
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1200px] flex-col px-[5vw] py-[4vh]">
          {phase === "setup" && (
            <Setup
              settings={settings}
              onChange={setSettings}
              best={bests[bestKey(settings)] ?? null}
              onStart={start}
              games={records.length}
              onHistory={showHistory}
            />
          )}
          {phase === "playing" && run && <Play run={run} now={now} onRun={setRun} />}
          {phase === "results" && run && (
            <Results
              run={run}
              record={record}
              against={against}
              onAgain={start}
              onSettings={() => setPhase("setup")}
              onHistory={showHistory}
              onClose={onClose}
            />
          )}
          {phase === "history" && (
            <History
              records={records}
              onBack={() => setPhase(back)}
              initial={records.length ? records[records.length - 1].difficulty : settings.difficulty}
              failed={unsaved}
            />
          )}
        </div>
      </div>
    </>
  );
}

// --- setup ------------------------------------------------------------------

function Setup({
  settings,
  onChange,
  best,
  onStart,
  games,
  onHistory,
}: {
  settings: Settings;
  onChange: (s: Settings) => void;
  best: number | null;
  onStart: () => void;
  games: number;
  onHistory: () => void;
}) {
  const anyOp = OPS.some((o) => settings.ops[o]);
  const special = mixedHint(settings.difficulty);
  const endless = settings.duration === null;
  const body = fs(14, 2.2, 20);
  const small = fs(12, 1.7, 15);
  return (
    <div className="my-auto flex flex-col gap-[3vh]" style={body}>
      <div>
        <Label>length</Label>
        <div className="flex flex-wrap gap-2">
          {DURATIONS.map((d) => (
            <Choice
              key={d}
              on={settings.duration === d}
              onClick={() => onChange({ ...settings, duration: d })}
              title={d === STANDARD ? "the standard run — the one a best means most at" : undefined}
            >
              {d / 60} min{d === STANDARD && <span className="text-[var(--dim)]"> · standard</span>}
            </Choice>
          ))}
          <Choice on={endless} onClick={() => onChange({ ...settings, duration: null })}>
            <span className="inline-flex items-center gap-1.5">
              <Endless size="1em" /> endless
            </span>
          </Choice>
        </div>
      </div>

      <div>
        <Label>question timer</Label>
        <div className="flex flex-wrap gap-2" role="group" aria-label="question timer">
          {QUESTION_CLOCKS.map((q) => (
            <Choice
              key={q}
              on={settings.question === q}
              onClick={() => onChange({ ...settings, question: q })}
              title={q === "auto" ? "the original's rule: the difficulty's seconds on a timed run, none when endless" : undefined}
            >
              {clockChoice(q, settings)}
            </Choice>
          ))}
        </div>
        <div className="mt-2 text-[var(--dim)]" style={small}>
          {clockLine(settings)}
        </div>
      </div>

      <div>
        <Label>difficulty</Label>
        <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
          {DIFFICULTIES.map((d) => (
            <Choice key={d} on={settings.difficulty === d} onClick={() => onChange({ ...settings, difficulty: d })}>
              <span className="flex items-center gap-2">
                <span className="h-[0.6em] w-[0.6em] shrink-0 rounded-full" style={{ background: COLOR[d] }} />
                <span className="font-semibold text-[var(--text)]">{LEVELS[d].label}</span>
              </span>
              <span className="mt-0.5 block text-[var(--dim)]" style={small}>
                {LEVELS[d].tagline}
              </span>
            </Choice>
          ))}
        </div>
      </div>

      <div>
        <Label>operations</Label>
        <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
          {OPS.map((op) => (
            <Choice
              key={op}
              on={settings.ops[op]}
              onClick={() => onChange({ ...settings, ops: { ...settings.ops, [op]: !settings.ops[op] } })}
            >
              <span className="flex items-baseline gap-2">
                <span className="font-mono text-[1.5em] leading-none text-[var(--text-strong)]">{SYMBOL[op]}</span>
                <span className="text-[var(--text)]">{OP_NAME[op]}</span>
              </span>
              <span className="mt-1 block text-[var(--dim)]" style={small}>
                {opHint(settings.difficulty, op)}
              </span>
            </Choice>
          ))}
        </div>
        {special && (
          <div className="mt-2 text-[var(--dim)]" style={small}>
            and with two or more on, {special}
          </div>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-x-6 gap-y-3 border-t border-[var(--border)] pt-[3vh]">
        <span className="text-[var(--dim)]">
          {best === null ? "no best here yet" : endless ? `longest streak here: ${best}` : `best here: ${best}`}
        </span>
        <span className="text-[var(--dim)]" style={small}>
          answers go in when right · <Key>Enter</Key> skips · <Key>Esc</Key> ends
        </span>
        <BigButton className="ml-auto" onClick={onHistory} title="every game you have played, and how you are trending">
          <LineChart size="1em" /> progress{games > 0 && <span className="text-[var(--dim)]"> · {games}</span>}
        </BigButton>
        <BigButton solid onClick={onStart} disabled={!anyOp} title={anyOp ? undefined : "turn at least one operation on"}>
          {/* Focused on open, so Enter starts. */}
          <AutoFocus />
          start <Key>↵</Key>
        </BigButton>
      </div>
    </div>
  );
}

/** Focuses the button it sits in, once — `autoFocus` belongs to inputs. */
function AutoFocus({ after = 0 }: { after?: number }) {
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    const t = setTimeout(() => ref.current?.closest("button")?.focus(), after);
    return () => clearTimeout(t);
  }, [after]);
  return <span ref={ref} hidden />;
}

// --- play -------------------------------------------------------------------

function Bar({ value, color, thin }: { value: number; color: string; thin?: boolean }) {
  return (
    <div className={cn("w-full overflow-hidden rounded-full bg-[var(--bg-faint)]", thin ? "h-1.5" : "h-3")}>
      <div
        className="h-full rounded-full"
        style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%`, background: color }}
      />
    </div>
  );
}

/**
 * As big as the window allows **for this question**. A mono character is about
 * 0.6em wide, so `7 + 8` can take the full height-scaled size while an
 * Expert `(20 + 19) × (48 − 17)` — four times the characters — has to shrink
 * by its length or run off the side of the screen.
 */
function questionSize(text: string): CSSProperties {
  const n = Math.max(text.length, 5);
  return { fontSize: `clamp(24px, min(13vmin, ${(125 / n).toFixed(2)}vw, ${Math.round(1600 / n)}px), 190px)` };
}

function Play({ run, now, onRun }: { run: Run; now: number; onRun: (r: Run) => void }) {
  const [typed, setTyped] = useState("");
  const [right, setRight] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const endless = run.endsAt === null;
  const seconds = questionSeconds(run.settings) ?? 1;
  const left = questionLeft(run, now);

  // A question that skipped itself takes what was half-typed with it.
  useEffect(() => setTyped(""), [run.seq]);
  useEffect(() => {
    if (!right) return;
    const t = setTimeout(() => setRight(false), 160);
    return () => clearTimeout(t);
  }, [right]);

  const miss = run.missed;
  return (
    // Anywhere you click, the keyboard stays in the answer box.
    <div className="flex flex-1 flex-col gap-[2vh]" onMouseUp={() => input.current?.focus()}>
      <div className="flex items-center gap-[3vw]">
        <span className="font-mono font-semibold text-[var(--text-strong)] tabular-nums" style={fs(22, 5, 52)} aria-label="clock">
          {endless ? clock(now - run.startedAt) : clock(Math.max(0, (run.endsAt ?? now) - now) + 999)}
        </span>
        {endless ? (
          <span className="flex flex-1 items-center gap-2 text-[var(--dim)]" style={fs(13, 2, 18)}>
            <Endless size="1.1em" /> endless — this one {secs(now - run.shownAt)}
          </span>
        ) : (
          <div className="flex-1">
            <Bar value={(now - run.startedAt) / ((run.endsAt ?? now) - run.startedAt)} color={COLOR[run.settings.difficulty]} />
          </div>
        )}
        <span className="text-right">
          <span className="block font-mono leading-none font-semibold text-[var(--text-strong)] tabular-nums" style={fs(32, 8, 88)} aria-label="score">
            {run.correct}
          </span>
          <span className="text-[var(--dim)]" style={fs(11, 1.6, 15)}>
            score
          </span>
        </span>
      </div>

      {left !== null && (
        <div title="this question skips itself when the bar runs out">
          <Bar
            value={left / (seconds * 1000)}
            color={left < Math.min(2500, seconds * 350) ? "var(--red)" : "var(--dim)"}
            thin
          />
        </div>
      )}

      <div className="flex flex-1 flex-col items-center justify-center gap-[2.5vh] py-[2vh]">
        <div
          className="font-mono leading-none font-semibold whitespace-nowrap text-[var(--text-strong)] tabular-nums"
          style={questionSize(run.problem.text)}
          aria-label="question"
        >
          {run.problem.text}
        </div>
        <div className="text-[var(--dim)]" style={fs(16, 3, 30)}>
          = ?
        </div>
        <input
          ref={input}
          autoFocus
          aria-label="answer"
          inputMode="numeric"
          autoComplete="off"
          spellCheck={false}
          value={typed}
          onChange={(e) => {
            const v = cleanAnswer(e.target.value);
            const next = answer(run, v, Date.now());
            if (next) {
              setTyped("");
              setRight(true);
              onRun(next);
            } else setTyped(v);
          }}
          onKeyDown={(e) => {
            if (e.key !== "Enter") return;
            e.preventDefault();
            setTyped("");
            onRun(giveUp(run, typed, Date.now()));
          }}
          style={{ ...fs(28, 7.5, 84), width: "clamp(220px, 44vmin, 620px)" }}
          className={cn(
            "rounded-xl border-[3px] bg-[var(--bg-panel)] px-4 py-[0.15em] text-center font-mono text-[var(--text-strong)] tabular-nums outline-none",
            "transition-colors duration-[var(--dur-fast)]",
            right ? "border-[var(--green)]" : "border-[var(--border)] focus:border-[var(--selection-stroke)]",
          )}
        />
        <div className="min-h-[1.5em]" style={fs(14, 2.6, 26)} aria-live="polite">
          {miss && (
            <span className="text-[var(--amber)]">
              {miss.why === "wrong" ? `not ${miss.typed} — ` : miss.why === "timeout" ? "time's up — " : "skipped — "}
              <span className="font-mono">
                {miss.text} = {miss.answer.toLocaleString("en-US")}
              </span>
            </span>
          )}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-[3vw] gap-y-2 text-[var(--dim)]" style={fs(13, 2.2, 20)}>
        <Stat n={run.correct} label="correct" color="var(--green)" />
        <Stat n={run.wrong} label="wrong" color="var(--red)" />
        <Stat n={run.skipped} label="skipped" color="var(--amber)" />
        <Stat n={run.streak} label="streak" />
        <span className="ml-auto" style={fs(12, 1.7, 15)}>
          <Key>Enter</Key> skip · <Key>Esc</Key> {endless ? "finish" : "end"}
        </span>
      </div>
    </div>
  );
}

function Stat({ n, label, color }: { n: number | string; label: string; color?: string }) {
  return (
    <span>
      <span className="font-mono text-[1.3em] font-semibold tabular-nums" style={{ color: color ?? "var(--text)" }}>
        {n}
      </span>{" "}
      {label}
    </span>
  );
}

// --- results ----------------------------------------------------------------

const TIER_ICON = [BookOpen, Dumbbell, ThumbsUp, Sparkles, Trophy] as const;

function Results({
  run,
  record,
  against,
  onAgain,
  onSettings,
  onHistory,
  onClose,
}: {
  run: Run;
  record: { previous: number | null; isNew: boolean } | null;
  /** This game against your recent ones at the same setting, if any. */
  against: ReturnType<typeof againstRecent>;
  onAgain: () => void;
  onSettings: () => void;
  onHistory: () => void;
  onClose: () => void;
}) {
  const sum = useMemo(() => summarize(run), [run]);
  const endless = run.settings.duration === null;
  const judged = run.settings.duration !== null && !run.early ? verdict(sum.score, run.settings.duration) : null;
  const Icon = judged ? TIER_ICON[judged.tier] : endless ? Endless : Flag;
  // Not at once: an Enter meant as a last skip must not start the next run.
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setReady(true), 500);
    return () => clearTimeout(t);
  }, []);

  const headline = judged
    ? judged.text
    : endless
      ? `${clock(sum.ms)} of training`
      : `stopped at ${clock(sum.ms)} — played, not a score`;
  const bestLine = (() => {
    if (!record || run.early) return null;
    const what = endless ? "longest streak" : "best";
    if (record.isNew) return record.previous === null ? `first ${what} at this setting` : `new ${what} — was ${record.previous}`;
    return record.previous === null ? null : `${what} here: ${record.previous}`;
  })();
  const small = fs(12, 1.7, 15);

  return (
    <div className="my-auto flex flex-col gap-[3vh]" style={fs(14, 2.2, 20)}>
      <div className="flex items-center gap-[3vw]">
        <Icon
          style={{ width: "clamp(40px, 9vmin, 96px)", height: "clamp(40px, 9vmin, 96px)" }}
          className={judged && judged.tier >= 3 ? "text-[var(--amber)]" : "text-[var(--dim)]"}
        />
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-[var(--text-strong)]" style={fs(22, 5, 52)}>
            {headline}
          </div>
          <div className="mt-1 text-[var(--dim)]">
            {runLabel(run.settings)} ·{" "}
            <span className="font-mono">{OPS.filter((o) => run.settings.ops[o]).map((o) => SYMBOL[o]).join(" ")}</span>
          </div>
          {bestLine && (
            <div className={cn("mt-2 flex items-center gap-1.5", record?.isNew ? "text-[var(--green)]" : "text-[var(--dim)]")}>
              {record?.isNew && <Trophy size="1em" />}
              {bestLine}
            </div>
          )}
          {against && !run.early && (
            <div className="mt-1 text-[var(--dim)]" aria-label="against your recent games">
              {sum.perMinute} per minute{" "}
              <span className={sum.perMinute >= against.avgPerMinute ? "text-[var(--green)]" : "text-[var(--amber)]"}>
                {sum.perMinute >= against.avgPerMinute ? "▲" : "▼"}{" "}
                {Math.abs(Math.round((sum.perMinute - against.avgPerMinute) * 10) / 10)}
              </span>{" "}
              against your last {against.games === 1 ? "game" : `${against.games} games`} here ({against.avgPerMinute}) · best{" "}
              {against.bestPerMinute}
            </div>
          )}
        </div>
        <div className="text-right">
          <div className="font-mono leading-none font-semibold text-[var(--text-strong)] tabular-nums" style={fs(56, 16, 180)} aria-label="final score">
            {sum.score}
          </div>
          <div className="text-[var(--dim)]" style={small}>
            correct
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Tile n={sum.accuracy + "%"} label="accuracy" />
        <Tile n={sum.perMinute} label="per minute" />
        <Tile n={secs(sum.avgMs)} label="average" />
        <Tile n={run.bestStreak} label="longest streak" />
        <Tile n={run.correct} label="correct" color="var(--green)" />
        <Tile n={run.wrong} label="wrong" color="var(--red)" />
        <Tile n={run.skipped} label="skipped" color="var(--amber)" />
        <Tile n={sum.total} label="asked" />
      </div>

      {sum.kinds.length > 0 && (
        <table className="w-full" aria-label="by operation">
          <thead>
            <tr className="text-[var(--dim)]" style={small}>
              <th className="pb-1 text-left font-normal">by operation</th>
              <th className="pb-1 text-right font-normal">solved</th>
              <th className="pb-1 text-right font-normal">missed</th>
              <th className="pb-1 text-right font-normal">average</th>
            </tr>
          </thead>
          <tbody>
            {sum.kinds.map((k) => (
              <tr key={k.kind} className="border-t border-[var(--border)]">
                <td className="py-[0.4em]">
                  <span className="inline-block w-[2em] font-mono text-[var(--text-strong)]">{SYMBOL[k.kind]}</span>
                  {OP_NAME[k.kind]}
                </td>
                <td className="text-right font-mono tabular-nums">{k.solved}</td>
                <td className="text-right font-mono tabular-nums">{k.missed}</td>
                <td className="text-right font-mono tabular-nums">{secs(k.avgMs)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div className="flex flex-wrap items-center gap-3 border-t border-[var(--border)] pt-[3vh]">
        <BigButton onClick={onSettings}>settings</BigButton>
        <BigButton onClick={onHistory}>
          <LineChart size="1em" /> progress
        </BigButton>
        <BigButton onClick={onClose}>close</BigButton>
        <BigButton solid className="ml-auto" onClick={onAgain} disabled={!ready}>
          {ready && <AutoFocus />}
          again <Key>↵</Key>
        </BigButton>
      </div>
    </div>
  );
}

function Tile({ n, label, color }: { n: number | string; label: string; color?: string }) {
  return (
    <div className="rounded-lg border border-[var(--border)] bg-[var(--bg-panel)] px-[1em] py-[0.6em]">
      <div className="font-mono font-semibold tabular-nums" style={{ ...fs(20, 4.5, 44), color: color ?? "var(--text-strong)" }}>
        {n}
      </div>
      <div className="text-[var(--dim)]" style={fs(12, 1.7, 15)}>
        {label}
      </div>
    </div>
  );
}
