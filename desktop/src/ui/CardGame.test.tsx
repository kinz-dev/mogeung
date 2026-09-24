/**
 * The card game through its own screen — the market-taking screen with cards
 * in it — with the clock faked and the deal seeded. The shared pages are
 * pinned by the dice game's tests; these watch what the cards change: the hand
 * reads as cards, a report worked out from them is exact, and the session is
 * kept in the card game's own file.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { CardGame, DEAL_MS } from "@/ui/CardGame";

const tick = (ms: number) => act(() => vi.advanceTimersByTime(ms));

/** The face-up cards' worth, and how many are face down, read off the hand. */
function readHand(): { open: number; hidden: number } {
  const hand = screen.getByLabelText("the hand");
  const open = [...hand.querySelectorAll("[aria-label*='worth']")]
    .map((el) => Number(/worth (\d+)/.exec(el.getAttribute("aria-label") ?? "")?.[1] ?? NaN))
    .reduce((a, b) => a + b, 0);
  return { open, hidden: hand.querySelectorAll("[aria-label='a face-down card']").length };
}

function readQuote(): { bid: number; ask: number } {
  const [, bid, ask] = /(\d+) at (\d+)/.exec(screen.getByLabelText("the quote").textContent ?? "") ?? [];
  return { bid: Number(bid), ask: Number(ask) };
}

/** Buy one when the hand looks worth more than the ask (8 a hidden card), else sell one; report exactly. */
function playThrough(): number {
  let rounds = 0;
  for (let guard = 0; guard < 60; guard++) {
    if (screen.queryByLabelText("profit and loss") || screen.queryByLabelText("your final balance")) return rounds;
    const deal = screen.queryByRole("button", { name: /^deal/ });
    if (deal) {
      fireEvent.click(deal);
      continue;
    }
    if (screen.queryByLabelText("units")) {
      const { open, hidden } = readHand();
      fireEvent.click(screen.getByRole("button", { name: open + 8 * hidden > readQuote().ask ? /^buy at/ : /^sell at/ }));
      continue;
    }
    if (screen.queryByLabelText("your profit or loss")) {
      const came = readHand().open;
      const { bid, ask } = readQuote();
      const pnl = /You bought/.test(document.body.textContent ?? "") ? came - ask : bid - came;
      fireEvent.change(screen.getByLabelText("your profit or loss"), { target: { value: String(pnl) } });
      fireEvent.click(screen.getByRole("button", { name: /^report/ }));
      continue;
    }
    const next = screen.queryByRole("button", { name: /^(next round|finish)/ });
    if (next) {
      rounds++;
      fireEvent.click(next);
      continue;
    }
    tick(DEAL_MS + 100);
  }
  throw new Error("the game did not end");
}

beforeEach(() => {
  localStorage.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("a card game, page by page", () => {
  it("lobby → every round → results, with exact reports, kept in the card game's own file", () => {
    render(<CardGame onExit={() => {}} onClose={() => {}} seed={42} />);
    expect(screen.getByLabelText("Cards")).toHaveTextContent("3");
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.getByText("Shuffling the deck…")).toBeInTheDocument();
    tick(DEAL_MS + 100);
    // Three cards, at most one of them face up.
    expect(readHand().hidden).toBeGreaterThanOrEqual(2);

    expect(playThrough()).toBe(4);
    expect(screen.getByText("Market closed")).toBeInTheDocument();
    const [, exact, of] = /(\d+)\/(\d+)/.exec(screen.getByText("reports exact").previousElementSibling?.textContent ?? "") ?? [];
    expect(exact).toBe(of);
    const kept = (localStorage.getItem("mogeung.arcade.card-game") ?? "").trim().split("\n");
    expect(kept).toHaveLength(1);
    expect(JSON.parse(kept[0]).rounds[0].hand).toMatch(/[♠♥♦♣]/);
    expect(localStorage.getItem("mogeung.arcade.dice-game")).toBeNull();
  });

  it("on Hard at your own pace, the game ends by asking for the hidden balance", () => {
    render(<CardGame onExit={() => {}} onClose={() => {}} seed={7} />);
    fireEvent.click(screen.getByRole("button", { name: /^Hard/ }));
    expect(screen.getByLabelText("Cards")).toHaveTextContent("5");
    fireEvent.click(screen.getAllByRole("button", { name: /^no limit/ })[0]);
    expect(screen.getByRole("button", { name: /^Hard/ })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.getByLabelText("your balance")).toHaveTextContent("hidden");
    playThrough();
    expect(screen.getByText(/What is your final balance/)).toBeInTheDocument();
  });
});
