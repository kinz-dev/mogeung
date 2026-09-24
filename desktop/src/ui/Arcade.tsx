/**
 * The arcade: the one thing in this window nobody is told about.
 *
 * It opens when you type the first six digits of π — `314159` — with nothing
 * that takes text focused, and it is in neither the shortcuts window nor the
 * palette (see [`lib/mentalMath.ts`](../lib/mentalMath.ts) for why it is not
 * an `Action`). Since 2026-09-24 the digits open a **picker** rather than a
 * game, because there are more than one: the mental-math drill it started as,
 * and — the same day — all six of tradermath.org's market games: the Trading
 * Game, the Market Making Dice Game, the ETF Arbitrage Game, the Card Game,
 * the Fermi Estimation Game and the Event Contracts Game. Asked for in those
 * words: *"the code 314159 will now popup a game screen, where user can select
 * Mental maths or Trading Game to start and follow the pages"* — and then
 * *"please port all of the 3 remaining game over"*.
 *
 * The arcade owns the room — the full-window surface, focus in and back out,
 * and the one line that keeps an eye on your sessions. Each game owns its own
 * pages inside it, and Escape walks back through them: a game's first page
 * returns here, and Escape here closes.
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { CandlestickChart, Dices, Flag, Gamepad2, Ruler, Scale, Scale3d, Sigma, Spade, Sparkles } from "lucide-react";
import { cn } from "@/lib/cn";
import { signedUsd } from "@/lib/etfGame";
import { etfStanding, loadEtfSessions } from "@/lib/etfStats";
import { eventStanding, loadEventSessions } from "@/lib/eventStats";
import { fermiStanding, loadFermiSessions } from "@/lib/fermiStats";
import { focusOwns } from "@/lib/keymap";
import { SECRET, SECRET_GAP_MS, secretStep } from "@/lib/mentalMath";
import { loadHistory, metrics } from "@/lib/mentalMathStats";
import { loadTakerSessions, takerStanding } from "@/lib/takerStats";
import { loadSessions, money, standing } from "@/lib/tradingStats";
import { useStore } from "@/store";
import { needsHuman, sessionLabel, type SessionId } from "@/wire/types";
import { ArcadeHeader, Key, fs } from "@/ui/arcadeKit";
import { CardGame } from "@/ui/CardGame";
import { DiceGame } from "@/ui/DiceGame";
import { EtfGame } from "@/ui/EtfGame";
import { EventGame } from "@/ui/EventGame";
import { FermiGame } from "@/ui/FermiGame";
import { MentalMathGame } from "@/ui/MentalMath";
import { TradingGame } from "@/ui/TradingGame";
import { interactive } from "@/ui/styles";

export function Arcade({ seed }: { seed?: number } = {}) {
  const [open, setOpen] = useState(false);
  useSecret(!open, () => setOpen(true));
  return open ? <Room onClose={() => setOpen(false)} seed={seed} /> : null;
}

/**
 * Listens for π. Only where a bare key belongs to the window — the same rule
 * the window's own bare bindings obey — so the digits typed into a terminal,
 * a filter box or a scratch file are only ever digits.
 */
function useSecret(armed: boolean, onFound: () => void) {
  const found = useRef(onFound);
  found.current = onFound;
  useEffect(() => {
    if (!armed) return;
    let progress = "";
    let at = 0;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat) return;
      const digit = /^\d$/.test(e.key) && !e.ctrlKey && !e.altKey && !e.metaKey;
      if (!digit || focusOwns(e.key)) {
        progress = "";
        return;
      }
      const now = Date.now();
      progress = secretStep(now - at > SECRET_GAP_MS ? "" : progress, e.key);
      at = now;
      if (progress === SECRET) {
        progress = "";
        e.preventDefault();
        found.current();
      }
    };
    // Capture, like the keymap: a pane that stops propagation must not be
    // able to hide the sequence from this.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [armed]);
}

type Game = "math" | "trading" | "dice" | "etf" | "cards" | "fermi" | "events";
type Screen = "picker" | Game;

const icon = (el: typeof Sigma, cls: string) => {
  const Icon = el;
  return <Icon className={cls} style={{ width: "1.8em", height: "1.8em" }} />;
};

const played = (n: number, noun = "session") => `${n} ${noun}${n === 1 ? "" : "s"} played`;

type GameProps = { onExit: () => void; onClose: () => void; seed?: number };

/**
 * The games, in the order the picker lays them out — its keys are their
 * places, 1 to 7. Each says what it is and how to open it, and reads its own
 * history for the line on its card: a record you can see is a reason to come
 * back to one game rather than another.
 */
const GAMES: {
  id: Game;
  label: string;
  title: string;
  icon: ReactNode;
  blurb: string;
  open: (p: GameProps) => ReactNode;
  record: () => Promise<string | null>;
}[] = [
  {
    id: "math",
    label: "mental math",
    title: "Mental maths",
    icon: icon(Sigma, "text-[var(--purple)]"),
    blurb: "Arithmetic against the clock — or endless, with no clock at all. Answers go in the moment they are right.",
    open: ({ onExit, onClose }) => <MentalMathGame onExit={onExit} onClose={onClose} />,
    record: async () => {
      const rs = await loadHistory();
      if (rs.length === 0) return null;
      const best = Math.max(...rs.filter((r) => !r.early).map((r) => metrics(r).perMinute), 0);
      return `${played(rs.length, "game")}${best ? ` · best ${best} a minute` : ""}`;
    },
  },
  {
    id: "trading",
    label: "trading game",
    title: "Trading Game",
    icon: icon(CandlestickChart, "text-[var(--green)]"),
    blurb: "Quote markets and trade a half-hidden hand of cards against three bots. Know the expected value, keep count of your money.",
    open: (p) => <TradingGame {...p} />,
    record: async () => {
      const ss = await loadSessions();
      const best = standing(ss).best;
      return ss.length ? `${played(ss.length)}${best !== null ? ` · best ${money(best)}` : ""}` : null;
    },
  },
  {
    id: "dice",
    label: "dice game",
    title: "Dice market",
    icon: icon(Dices, "text-[var(--amber)]"),
    blurb: "A market maker quotes on a roll of dice, some face down. Buy, sell or skip on a clock you choose — or none — and keep count of your P&L.",
    open: (p) => <DiceGame {...p} />,
    record: async () => {
      const ss = await loadTakerSessions("dice-game");
      const best = takerStanding(ss).best;
      return ss.length ? `${played(ss.length)}${best !== null ? ` · best ${money(best)}` : ""}` : null;
    },
  },
  {
    id: "etf",
    label: "etf arbitrage",
    title: "ETF arbitrage",
    icon: icon(Scale, "text-[var(--blue)]"),
    blurb: "Work out an ETF's fair value from its basket of stocks, spot when the quote strays, and race three bots to the trade.",
    open: (p) => <EtfGame {...p} />,
    record: async () => {
      const ss = await loadEtfSessions();
      const best = etfStanding(ss).best;
      return ss.length ? `${played(ss.length)}${best !== null ? ` · best ${signedUsd(best)}` : ""}` : null;
    },
  },
  {
    id: "cards",
    label: "card game",
    title: "Card game",
    icon: icon(Spade, "text-[var(--red)]"),
    blurb: "A market maker quotes on a hand of mostly hidden cards. Work out the expected value, then buy, sell or skip before the clock runs out.",
    open: (p) => <CardGame {...p} />,
    record: async () => {
      const ss = await loadTakerSessions("card-game");
      const best = takerStanding(ss).best;
      return ss.length ? `${played(ss.length)}${best !== null ? ` · best ${money(best)}` : ""}` : null;
    },
  },
  {
    id: "fermi",
    label: "fermi estimation",
    title: "Fermi estimation",
    icon: icon(Ruler, "text-[var(--purple)]"),
    blurb: "Estimate what nobody knows offhand with a range — tight when you know, wide when you don't — scored on a log scale.",
    open: (p) => <FermiGame {...p} />,
    record: async () => {
      const ss = await loadFermiSessions();
      const best = fermiStanding(ss).best;
      return ss.length ? `${played(ss.length)}${best !== null ? ` · best ${best}%` : ""}` : null;
    },
  },
  {
    id: "events",
    label: "event contracts",
    title: "Event contracts",
    icon: icon(Scale3d, "text-[var(--green)]"),
    blurb: "Read an event brief, update the odds, and quote a market in cents against noise traders and informed ones.",
    open: (p) => <EventGame {...p} />,
    record: async () => {
      const ss = await loadEventSessions();
      const best = eventStanding(ss).best;
      return ss.length ? `${played(ss.length)}${best !== null ? ` · best ${best > 0 ? "+" : ""}${best} a round` : ""}` : null;
    },
  },
];

const FOUND_KEY = "mogeung.arcade.found";

function everFound(): boolean {
  try {
    // The mental-math game kept its own flag before there was an arcade.
    return localStorage.getItem(FOUND_KEY) === "1" || JSON.parse(localStorage.getItem("mogeung.mentalMath") ?? "{}")?.found === true;
  } catch {
    return false;
  }
}

function Room({ onClose, seed }: { onClose: () => void; seed?: number }) {
  const [screen, setScreen] = useState<Screen>("picker");
  const [firstTime] = useState(() => !everFound());
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    try {
      localStorage.setItem(FOUND_KEY, "1");
    } catch {
      // Private window: it will say "you found it" again, which is harmless.
    }
  }, []);

  useEffect(() => {
    const returnTo = document.activeElement as HTMLElement | null;
    if (!panelRef.current?.contains(document.activeElement)) panelRef.current?.focus();
    return () => returnTo?.focus?.();
  }, []);

  const goTo = (id: SessionId) => {
    useStore.getState().select(id);
    onClose();
  };

  return (
    <div
      ref={panelRef}
      role="dialog"
      aria-modal="true"
      aria-label={screen === "picker" ? "games" : GAMES.find((g) => g.id === screen)!.label}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex flex-col bg-[var(--bg)] text-[var(--text)] outline-none"
    >
      {screen === "picker" ? (
        <Picker firstTime={firstTime} onPick={setScreen} onClose={onClose} />
      ) : (
        GAMES.find((g) => g.id === screen)!.open({ onExit: () => setScreen("picker"), onClose, seed })
      )}
      <Waiting onGo={goTo} />
    </div>
  );
}

// --- the picker -------------------------------------------------------------------

function GameCard({
  n,
  icon,
  title,
  blurb,
  record,
  onClick,
  autoFocus,
}: {
  n: number;
  icon: React.ReactNode;
  title: string;
  blurb: string;
  record: string | null;
  onClick: () => void;
  autoFocus?: boolean;
}) {
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!autoFocus) return;
    const t = setTimeout(() => ref.current?.focus(), 0);
    return () => clearTimeout(t);
  }, [autoFocus]);
  return (
    <button
      ref={ref}
      type="button"
      onClick={onClick}
      className={cn(
        "flex flex-1 flex-col gap-[1.5vh] rounded-xl border-2 border-[var(--border)] bg-[var(--bg-panel)] p-[3vmin] text-left",
        "hover:border-[var(--selection-stroke)] focus-visible:border-[var(--selection-stroke)]",
        interactive,
      )}
    >
      <span className="flex items-center gap-3">
        {icon}
        <span className="font-semibold text-[var(--text-strong)]" style={fs(18, 3.2, 32)}>
          {title}
        </span>
        <span className="ml-auto" style={fs(12, 1.7, 15)}>
          <Key>{n}</Key>
        </span>
      </span>
      <span className="text-[var(--dim)]" style={fs(14, 2.2, 20)}>
        {blurb}
      </span>
      <span className="mt-auto text-[var(--dim)]" style={fs(12, 1.7, 15)}>
        {record ?? "not played yet"}
      </span>
    </button>
  );
}

function Picker({ firstTime, onPick, onClose }: { firstTime: boolean; onPick: (s: Screen) => void; onClose: () => void }) {
  const [lines, setLines] = useState<Partial<Record<Game, string>>>({});

  useEffect(() => {
    let live = true;
    for (const g of GAMES) {
      void g
        .record()
        .then((line) => live && line && setLines((ls) => ({ ...ls, [g.id]: line })))
        .catch(() => undefined);
    }
    return () => {
      live = false;
    };
  }, []);

  const pick = useRef(onPick);
  pick.current = onPick;
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        e.preventDefault();
        close.current();
        return;
      }
      const g = /^[1-9]$/.test(e.key) ? GAMES[Number(e.key) - 1] : undefined;
      if (g) {
        e.preventDefault();
        pick.current(g.id);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <>
      <ArcadeHeader icon={<Gamepad2 size={20} className="text-[var(--purple)]" />} title="games" onBack={null} onClose={onClose} />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1400px] flex-col justify-center gap-[4vh] px-[5vw] py-[4vh]">
          {firstTime && (
            <div className="flex items-center gap-2 text-[var(--purple)]" style={fs(14, 2.4, 22)}>
              <Sparkles style={{ width: "1em", height: "1em" }} /> you found it
            </div>
          )}
          <div className="grid gap-[3vmin] md:grid-cols-2 xl:grid-cols-3">
            {GAMES.map((g, i) => (
              <GameCard
                key={g.id}
                n={i + 1}
                icon={g.icon}
                title={g.title}
                blurb={g.blurb}
                record={lines[g.id] ?? null}
                onClick={() => onPick(g.id)}
                autoFocus={i === 0}
              />
            ))}
          </div>
          <div className="text-[var(--dim)]" style={fs(12, 1.7, 15)}>
            <Key>1</Key>–<Key>{GAMES.length}</Key> to pick · <Key>Esc</Key> to close
          </div>
        </div>
      </div>
    </>
  );
}

// --- the one thing it keeps an eye on -----------------------------------------

/**
 * The window is still for knowing which session needs you, game or no game.
 * A line, not an interruption: a click takes you to the session and puts the
 * arcade away.
 */
function Waiting({ onGo }: { onGo: (id: SessionId) => void }) {
  const queue = useStore((s) => s.queue);
  const sessions = useStore((s) => s.sessions);
  const waiting = useMemo(() => queue.filter((i) => needsHuman(i.reason)), [queue]);
  if (waiting.length === 0) return null;
  const first = waiting[0];
  const s = sessions[first.session_id];
  return (
    <button
      type="button"
      onClick={() => onGo(first.session_id)}
      className={cn(
        "flex shrink-0 items-center gap-2 border-t border-[var(--border)] bg-[var(--bg-panel)] px-5 py-2.5 text-left text-base text-[var(--amber)]",
        interactive,
      )}
      title="go to it — the game is put away"
    >
      <Flag size={16} />
      {waiting.length === 1 ? "a session needs you" : `${waiting.length} sessions need you`}
      {s && <span className="truncate text-[var(--dim)]">— {sessionLabel(s)}</span>}
    </button>
  );
}
