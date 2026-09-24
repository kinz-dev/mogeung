/**
 * The arcade's room — the picker and the seven games — and the page that
 * holds it in a window of its own. The way in, and the main window's end of
 * the bridge, are in [`Arcade.tsx`](./Arcade.tsx).
 *
 * **Nothing here reads the store.** This is the whole of what the arcade's
 * window loads, and the store would bring the daemon client and every pane
 * with it; what the room needs to know about sessions arrives as a prop.
 */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { CandlestickChart, Dices, Flag, Gamepad2, Ruler, Scale, Scale3d, Sigma, Spade, Sparkles } from "lucide-react";
import { channel, closeThisWindow, raiseThisWindow, type ArcadeMsg, type Waiting } from "@/lib/arcadeWindow";
import { cn } from "@/lib/cn";
import { signedUsd } from "@/lib/etfGame";
import { etfStanding, loadEtfSessions } from "@/lib/etfStats";
import { eventStanding, loadEventSessions } from "@/lib/eventStats";
import { fermiStanding, loadFermiSessions } from "@/lib/fermiStats";
import { loadHistory, metrics } from "@/lib/mentalMathStats";
import { loadTakerSessions, takerStanding } from "@/lib/takerStats";
import { loadSessions, money, standing } from "@/lib/tradingStats";
import type { SessionId } from "@/wire/types";
import { ArcadeHeader, Key, fs } from "@/ui/arcadeKit";
import { CardGame } from "@/ui/CardGame";
import { DiceGame } from "@/ui/DiceGame";
import { EtfGame } from "@/ui/EtfGame";
import { EventGame } from "@/ui/EventGame";
import { FermiGame } from "@/ui/FermiGame";
import { MentalMathGame } from "@/ui/MentalMath";
import { TradingGame } from "@/ui/TradingGame";
import { interactive } from "@/ui/styles";

/**
 * The arcade's own page, in its own window. It hears about sessions from the
 * main window rather than from the daemon, and a click on the waiting line
 * brings the main window forward on that session — the game stays where it
 * is, in the window behind.
 */
export function ArcadeWindow({ seed }: { seed?: number } = {}) {
  const [waiting, setWaiting] = useState<Waiting>(NONE);
  const ch = useRef<BroadcastChannel | null>(null);

  useEffect(() => {
    const c = channel();
    ch.current = c;
    if (!c) return;
    c.onmessage = (e: MessageEvent<ArcadeMsg>) => {
      const m = e.data;
      if (m.t === "state") {
        setWaiting(m.waiting);
        if (m.theme) document.documentElement.setAttribute("data-theme", m.theme);
      } else if (m.t === "raise") {
        void raiseThisWindow();
      }
    };
    c.postMessage({ t: "hello" } satisfies ArcadeMsg);
    return () => {
      c.close();
      ch.current = null;
    };
  }, []);

  useEffect(() => {
    document.title = "mogeung · games";
  }, []);

  const goTo = (id: SessionId) => ch.current?.postMessage({ t: "go", id } satisfies ArcadeMsg);
  return <Room onClose={() => void closeThisWindow()} seed={seed} waiting={waiting} onGo={goTo} />;
}

const NONE: Waiting = { count: 0, first: null };

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

export function Room({
  onClose,
  seed,
  waiting,
  onGo,
  overlay = false,
}: {
  onClose: () => void;
  seed?: number;
  waiting: Waiting;
  onGo: (id: SessionId) => void;
  /** Over the main window, as a fallback — rather than a window of its own. */
  overlay?: boolean;
}) {
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

  return (
    <div
      ref={panelRef}
      role="dialog"
      aria-modal={overlay || undefined}
      aria-label={screen === "picker" ? "games" : GAMES.find((g) => g.id === screen)!.label}
      tabIndex={-1}
      className={cn(
        "flex flex-col bg-[var(--bg)] text-[var(--text)] outline-none",
        overlay ? "fixed inset-0 z-50" : "h-screen",
      )}
    >
      {screen === "picker" ? (
        <Picker firstTime={firstTime} onPick={setScreen} onClose={onClose} />
      ) : (
        GAMES.find((g) => g.id === screen)!.open({ onExit: () => setScreen("picker"), onClose, seed })
      )}
      <WaitingLine waiting={waiting} onGo={onGo} />
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
 * A line, not an interruption: a click takes you to the session — the main
 * window comes forward on it, and the game stays in its own.
 */
function WaitingLine({ waiting, onGo }: { waiting: Waiting; onGo: (id: SessionId) => void }) {
  const { count, first } = waiting;
  if (count === 0 || !first) return null;
  return (
    <button
      type="button"
      onClick={() => onGo(first.id)}
      className={cn(
        "flex shrink-0 items-center gap-2 border-t border-[var(--border)] bg-[var(--bg-panel)] px-5 py-2.5 text-left text-base text-[var(--amber)]",
        interactive,
      )}
      title="go to it"
    >
      <Flag size={16} />
      {count === 1 ? "a session needs you" : `${count} sessions need you`}
      {first.label && <span className="truncate text-[var(--dim)]">— {first.label}</span>}
    </button>
  );
}
