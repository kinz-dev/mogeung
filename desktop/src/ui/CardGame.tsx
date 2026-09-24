/**
 * The Card Game — tradermath.org's market-taking card game, the arcade's
 * fifth. The deal lives in [`lib/cardGame.ts`](../lib/cardGame.ts); the rounds,
 * the pages and the clocks are the market-taking game's, shared with the dice
 * game ([`TakerGame`](TakerGame.tsx)). What is here is what makes it cards: the
 * cards themselves — the Trading Game's — and the words.
 */

import { Spade } from "lucide-react";
import { DEFAULT_CARDS, LIMITS, PRESETS, dealCards, describeCards, eventText, type Card, type CardSettings, type MarketEvent } from "@/lib/cardGame";
import { STARTING_BALANCE } from "@/lib/takerTable";
import { Para, SharedRules, TakerGame, type TakerVariant } from "@/ui/TakerGame";
import { PlayingCard } from "@/ui/TradingGame";

/** How long the deal takes before the quote goes up — the reference's 700 ms. */
export const DEAL_MS = 700;

export const CARDS: TakerVariant<Card, MarketEvent, CardSettings> = {
  game: "card-game",
  settingsKey: "mogeung.cards.settings",
  title: "card game",
  icon: (size, muted) => <Spade className={muted ? "text-[var(--dim)]" : "text-[var(--red)]"} style={{ width: size, height: size }} />,
  color: "var(--red)",
  blurb:
    "Trade against a market maker quoting a hand of mostly hidden playing cards: work out the expected value, and buy, sell or skip before time runs out.",
  howToPlay: (
    <>
      <Para head="Aim:">end with more than you started — {STARTING_BALANCE}.</Para>
      <Para head="The hand.">
        Each round deals a few cards, worth 2 to 14 (J 11, Q 12, K 13, A 14). At most one is face up. The round settles
        at the sum of all of them.
      </Para>
      <Para head="The quote.">
        The market maker quotes <i>bid at ask</i>: you may buy at the ask, sell at the bid, or skip. Its quotes are
        random — it prices a hand of its own, not yours. A hidden card is worth 8 on average; the face-up card and that
        average are your edge.
      </Para>
      <SharedRules what="cards" />
      <Para head="Events.">
        When on, a round may change the deck: only red cards, only 10 and up, a card worth something else… They move
        both the fair value and what a sale can lose.
      </Para>
    </>
  ),
  countLabel: "Cards",
  countText: (n) => `${n} cards`,
  limits: LIMITS,
  presets: PRESETS,
  defaults: DEFAULT_CARDS,
  deal: dealCards,
  eventText,
  landMs: DEAL_MS,
  landingText: "Shuffling the deck…",
  dealVerb: "deal",
  handLabel: "the hand",
  cameTo: "The cards came to",
  renderItem: (card, { faceUp }) => <PlayingCard card={card} faceUp={faceUp} />,
  describe: describeCards,
};

export function CardGame(props: { onExit: () => void; onClose: () => void; seed?: number }) {
  return <TakerGame variant={CARDS} {...props} />;
}
