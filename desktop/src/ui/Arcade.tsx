/**
 * The arcade: the one thing in this window nobody is told about.
 *
 * It opens when you type the first six digits of π — `314159` — with nothing
 * that takes text focused, and it is in neither the shortcuts window nor the
 * palette (see [`lib/mentalMath.ts`](../lib/mentalMath.ts) for why it is not
 * an `Action`). Since 2026-09-24 the digits open a **picker** rather than a
 * game, because there are two: the mental-math drill it started as, and a
 * Trading Game modelled on tradermath.org's. Asked for in those words: *"the
 * code 314159 will now popup a game screen, where user can select Mental maths
 * or Trading Game to start and follow the pages"*.
 *
 * The arcade owns the room — the full-window surface, focus in and back out,
 * and the one line that keeps an eye on your sessions. Each game owns its own
 * pages inside it, and Escape walks back through them: a game's first page
 * returns here, and Escape here closes.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { CandlestickChart, Flag, Gamepad2, Sigma, Sparkles } from "lucide-react";
import { cn } from "@/lib/cn";
import { focusOwns } from "@/lib/keymap";
import { SECRET, SECRET_GAP_MS, secretStep } from "@/lib/mentalMath";
import { loadHistory, metrics } from "@/lib/mentalMathStats";
import { loadSessions, money, standing } from "@/lib/tradingStats";
import { useStore } from "@/store";
import { needsHuman, sessionLabel, type SessionId } from "@/wire/types";
import { ArcadeHeader, Key, fs } from "@/ui/arcadeKit";
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

type Screen = "picker" | "math" | "trading";

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

  const label = screen === "picker" ? "games" : screen === "math" ? "mental math" : "trading game";
  return (
    <div
      ref={panelRef}
      role="dialog"
      aria-modal="true"
      aria-label={label}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex flex-col bg-[var(--bg)] text-[var(--text)] outline-none"
    >
      {screen === "picker" && <Picker firstTime={firstTime} onPick={setScreen} onClose={onClose} />}
      {screen === "math" && <MentalMathGame onExit={() => setScreen("picker")} onClose={onClose} />}
      {screen === "trading" && <TradingGame onExit={() => setScreen("picker")} onClose={onClose} seed={seed} />}
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
        <span className="font-semibold text-[var(--text-strong)]" style={fs(22, 4.5, 44)}>
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
  const [mathLine, setMathLine] = useState<string | null>(null);
  const [tradingLine, setTradingLine] = useState<string | null>(null);

  // What you have played, said on each card: a record you can see is a reason
  // to come back to one game rather than the other.
  useEffect(() => {
    let live = true;
    void loadHistory().then((rs) => {
      if (!live || rs.length === 0) return;
      const best = Math.max(...rs.filter((r) => !r.early).map((r) => metrics(r).perMinute), 0);
      setMathLine(`${rs.length} game${rs.length === 1 ? "" : "s"} played${best ? ` · best ${best} a minute` : ""}`);
    });
    void loadSessions().then((ss) => {
      if (!live || ss.length === 0) return;
      const st = standing(ss);
      setTradingLine(`${ss.length} session${ss.length === 1 ? "" : "s"} played${st.best !== null ? ` · best ${money(st.best)}` : ""}`);
    });
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
      } else if (e.key === "1" || e.key === "2") {
        e.preventDefault();
        pick.current(e.key === "1" ? "math" : "trading");
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const cards = useMemo(
    () => [
      {
        screen: "math" as const,
        icon: <Sigma className="text-[var(--purple)]" style={{ width: "1.8em", height: "1.8em" }} />,
        title: "Mental maths",
        blurb: "Arithmetic against the clock — or endless, with no clock at all. Answers go in the moment they are right.",
        record: mathLine,
      },
      {
        screen: "trading" as const,
        icon: <CandlestickChart className="text-[var(--green)]" style={{ width: "1.8em", height: "1.8em" }} />,
        title: "Trading Game",
        blurb: "Quote markets and trade a half-hidden hand of cards against three bots. Know the expected value, keep count of your money.",
        record: tradingLine,
      },
    ],
    [mathLine, tradingLine],
  );

  return (
    <>
      <ArcadeHeader icon={<Gamepad2 size={20} className="text-[var(--purple)]" />} title="games" onBack={null} onClose={onClose} />
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-[1200px] flex-col justify-center gap-[4vh] px-[5vw] py-[4vh]">
          {firstTime && (
            <div className="flex items-center gap-2 text-[var(--purple)]" style={fs(14, 2.4, 22)}>
              <Sparkles style={{ width: "1em", height: "1em" }} /> you found it
            </div>
          )}
          <div className="flex flex-col gap-[3vmin] md:flex-row">
            {cards.map((c, i) => (
              <GameCard key={c.screen} n={i + 1} {...c} onClick={() => onPick(c.screen)} autoFocus={i === 0} />
            ))}
          </div>
          <div className="text-[var(--dim)]" style={fs(12, 1.7, 15)}>
            <Key>1</Key> or <Key>2</Key> to pick · <Key>Esc</Key> to close
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
