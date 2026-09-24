/**
 * The Market Making Dice Game — the arcade's third game, after tradermath.org's.
 * The dice live in [`lib/diceGame.ts`](../lib/diceGame.ts); the rounds, the
 * pages and the clocks are the market-taking game's, shared with the card
 * game ([`lib/takerTable.ts`](../lib/takerTable.ts), [`TakerGame`](TakerGame.tsx)).
 * What is here is what makes it dice: the faces, and the words.
 */

import { Dices } from "lucide-react";
import { cn } from "@/lib/cn";
import { DEFAULT_DICE, LIMITS, PRESETS, STARTING_BALANCE, dealDice, describeDice, eventText, type DiceEvent, type DiceSettings, type Die } from "@/lib/diceGame";
import { fs } from "@/ui/arcadeKit";
import { Para, SharedRules, TakerGame, type TakerVariant } from "@/ui/TakerGame";

/** How long the dice tumble before the quote goes up. */
export const ROLL_MS = 1200;

/** Where a face's pips sit on a 3 × 3 grid. */
const PIPS: Record<number, number[]> = {
  1: [4],
  2: [0, 8],
  3: [0, 4, 8],
  4: [0, 2, 6, 8],
  5: [0, 2, 4, 6, 8],
  6: [0, 2, 3, 5, 6, 8],
};

const SIZE = { width: "clamp(44px, 9vmin, 96px)", height: "clamp(44px, 9vmin, 96px)" };
const PAD = "p-[clamp(5px,1.1vmin,12px)]";

function Pip({ on }: { on: boolean }) {
  return <span className={cn("h-[70%] w-[70%] rounded-full", on ? "bg-[var(--text-strong)]" : "bg-transparent")} />;
}

function DieFace({ die, faceUp, tumble }: { die: Die; faceUp: boolean; tumble: number | null }) {
  if (tumble !== null) {
    return (
      <div
        style={{ ...SIZE, transform: `rotate(${(tumble * 37) % 360}deg)` }}
        aria-label="a rolling die"
        className={cn("grid grid-cols-3 grid-rows-3 place-items-center rounded-xl border-2 border-[var(--border)] bg-[var(--bg-raised)]", PAD)}
      >
        {Array.from({ length: 9 }, (_, i) => (
          <Pip key={i} on={PIPS[(tumble % 6) + 1].includes(i)} />
        ))}
      </div>
    );
  }
  if (!faceUp) {
    return (
      <div
        style={SIZE}
        aria-label="a face-down die"
        className="flex items-center justify-center rounded-xl border-2 border-[var(--border)] bg-[repeating-linear-gradient(45deg,var(--bg-panel),var(--bg-panel)_6px,var(--bg-faint)_6px,var(--bg-faint)_12px)] font-semibold text-[var(--dim)]"
      >
        <span style={fs(18, 3.6, 38)}>?</span>
      </div>
    );
  }
  // An event can make a face worth something else; the die says so.
  const revalued = die.worth !== die.show;
  return (
    <div
      style={SIZE}
      aria-label={`a die showing ${die.show}${revalued ? `, worth ${die.worth}` : ""}`}
      className={cn(
        "relative grid grid-cols-3 grid-rows-3 place-items-center rounded-xl border-2 bg-[var(--bg-raised)]",
        PAD,
        die.open ? "border-[var(--border)]" : "border-[var(--amber)]",
      )}
    >
      {Array.from({ length: 9 }, (_, i) => (
        <Pip key={i} on={PIPS[die.show].includes(i)} />
      ))}
      {revalued && (
        <span className="absolute -bottom-2 rounded bg-[var(--amber)] px-1 font-mono text-[var(--on-accent-dark)]" style={fs(10, 1.6, 14)}>
          = {die.worth}
        </span>
      )}
    </div>
  );
}

export const DICE: TakerVariant<Die, DiceEvent, DiceSettings> = {
  game: "dice-game",
  settingsKey: "mogeung.dice.settings",
  title: "market making dice",
  icon: (size, muted) => <Dices className={muted ? "text-[var(--dim)]" : "text-[var(--amber)]"} style={{ width: size, height: size }} />,
  color: "var(--amber)",
  blurb:
    "Trade against a market maker quoting on a roll of dice, some face down, and keep count of your own profit and loss as you go. The maker guesses the hidden dice; you know what they are worth on average.",
  howToPlay: (
    <>
      <Para head="Aim:">end with more than you started — {STARTING_BALANCE}.</Para>
      <Para head="The roll.">
        Each round rolls a set of dice. At least one lands face down; the rest you can see. The round settles at the
        total of all of them.
      </Para>
      <Para head="The quote.">
        The market maker quotes <i>bid at ask</i>: you may buy at the ask, sell at the bid, or skip. It counts the dice
        you can see and <i>guesses</i> each hidden one at random — so its quote is often off. A hidden die is worth 3.5
        on average; when the quote is off, trade.
      </Para>
      <SharedRules what="dice" />
      <Para head="Events.">
        When on, a round may limit what a die can show — only 5 or 6, only even… — or make a 1 worth 7. They move both
        the fair value and what a sale can lose.
      </Para>
    </>
  ),
  countLabel: "Dice",
  countText: (n) => `${n} dice`,
  limits: LIMITS,
  presets: PRESETS,
  defaults: DEFAULT_DICE,
  deal: dealDice,
  eventText,
  landMs: ROLL_MS,
  landingText: "Rolling — waiting for the market maker's quote…",
  dealVerb: "roll",
  handLabel: "the dice",
  cameTo: "The dice came to",
  renderItem: (die, { faceUp, landing, now, index }) => (
    <DieFace die={die} faceUp={faceUp} tumble={landing ? Math.floor(now / 90) + index * 5 : null} />
  ),
  describe: describeDice,
};

export function DiceGame(props: { onExit: () => void; onClose: () => void; seed?: number }) {
  return <TakerGame variant={DICE} {...props} />;
}
