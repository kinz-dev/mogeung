/**
 * The Trading Game, played through its own screen: lobby, every round of the
 * table, results — with the clock faked and the deal seeded, so the same
 * game is played every time. What these watch is what a player would: the
 * pages follow one another, the quote and the hand are there to read, a report
 * worked out from the cards is taken as exact, a hidden balance is asked for
 * at the end, and the session is kept.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { TradingGame } from "@/ui/TradingGame";

const tick = (ms: number) => act(() => vi.advanceTimersByTime(ms));

/** The face-up hand's sum, read off the cards as a player would. */
function readPrice(): number {
  const hand = screen.getByLabelText("the hand");
  return [...hand.querySelectorAll("[aria-label*='worth']")]
    .map((el) => Number(/worth (\d+)/.exec(el.getAttribute("aria-label") ?? "")?.[1] ?? NaN))
    .reduce((a, b) => a + b, 0);
}

/**
 * A sensible market: the face-up cards, plus about 8 for each face-down one,
 * one either side. The first draft quoted "20 at 22" on a five-card hand worth
 * 42, and the bots — rightly — bought everything and bust it in round one.
 */
function fairQuote(): { bid: string; ask: string } {
  const hand = screen.getByLabelText("the hand");
  const hidden = hand.querySelectorAll("[aria-label='a face-down card']").length;
  const fair = Math.round(readPrice() + 8 * hidden);
  return { bid: String(fair - 1), ask: String(fair + 1) };
}

function readQuote(): { bid: number; ask: number } {
  const [, bid, ask] = /(\d+) at (\d+)/.exec(screen.getByLabelText("the quote").textContent ?? "") ?? [];
  return { bid: Number(bid), ask: Number(ask) };
}

/** Play every round: quote on your turn, otherwise buy one and report it exactly. */
function playThrough(): number {
  let rounds = 0;
  for (let guard = 0; guard < 60; guard++) {
    if (screen.queryByLabelText("profit and loss") || screen.queryByLabelText("your final balance")) return rounds;
    const deal = screen.queryByRole("button", { name: /^deal/ });
    if (deal) {
      fireEvent.click(deal);
      continue;
    }
    if (screen.queryByLabelText("bid")) {
      const q = fairQuote();
      fireEvent.change(screen.getByLabelText("bid"), { target: { value: q.bid } });
      fireEvent.change(screen.getByLabelText("ask"), { target: { value: q.ask } });
      fireEvent.click(screen.getByRole("button", { name: /^quote/ }));
      tick(6_000);
      continue;
    }
    if (screen.queryByLabelText("units")) {
      fireEvent.change(screen.getByLabelText("units"), { target: { value: "1" } });
      fireEvent.click(screen.getByRole("button", { name: /^buy at/ }));
      continue;
    }
    if (screen.queryByLabelText("your profit or loss")) {
      const pnl = readPrice() - readQuote().ask;
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
    // A bot is quoting, or the bots are trading against yours.
    tick(2_000);
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

describe("a Trading Game, page by page", () => {
  it("lobby → every round → results, with exact reports, and the session is kept", async () => {
    render(<TradingGame onExit={() => {}} onClose={() => {}} seed={42} />);
    expect(screen.getByText(/No sessions yet/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.getByLabelText("seat You")).toBeInTheDocument();

    const rounds = playThrough();
    expect(rounds).toBe(4);

    expect(screen.getByText("Market closed")).toBeInTheDocument();
    const table = screen.getByRole("table", { name: "round by round" });
    expect(within(table).getAllByRole("row")).toHaveLength(1 + 4);
    // Every report was worked out from the cards, so every one was exact.
    const reported = screen.queryByText(/You reported \d+ of \d+ rounds? exactly/);
    if (reported) {
      const [, exact, of] = /reported (\d+) of (\d+)/.exec(reported.textContent ?? "")!;
      expect(exact).toBe(of);
    }

    const kept = (localStorage.getItem("mogeung.arcade.trading-game") ?? "").trim().split("\n");
    expect(kept).toHaveLength(1);
    expect(JSON.parse(kept[0])).toMatchObject({ ending: "complete", rounds: expect.any(Array) });

    // Back in the lobby, the statistics have the session.
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /^lobby/ }));
    });
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("session")).toBeInTheDocument();
  });

  it("with the balance hidden, the game ends by asking for it, and a wrong answer costs 50", () => {
    render(<TradingGame onExit={() => {}} onClose={() => {}} seed={7} />);
    fireEvent.click(screen.getByRole("button", { name: /^Hard/ }));
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.getByLabelText("You balance")).toHaveTextContent("hidden");

    playThrough();
    expect(screen.getByText(/What is your final balance/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("your final balance"), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: /^answer/ }));
    expect(screen.getByText(/You said 1; it was -?\d+ — −50/)).toBeInTheDocument();
  });

  it("Escape at the table stops the game and shows it as played, not a result", () => {
    const onExit = vi.fn();
    render(<TradingGame onExit={onExit} onClose={() => {}} seed={3} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByText(/Stopped early/)).toBeInTheDocument();
    // Results → lobby → out to the arcade.
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("button", { name: /^start game/ })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onExit).toHaveBeenCalled();
  });

  it("an order over your balance is forfeited in words, not silently", () => {
    render(<TradingGame onExit={() => {}} onClose={() => {}} seed={42} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    for (let guard = 0; guard < 20 && !screen.queryByLabelText("units"); guard++) {
      const next = screen.queryByRole("button", { name: /^(next round|finish)/ });
      if (screen.queryByLabelText("bid")) {
        const q = fairQuote();
        fireEvent.change(screen.getByLabelText("bid"), { target: { value: q.bid } });
        fireEvent.change(screen.getByLabelText("ask"), { target: { value: q.ask } });
        fireEvent.click(screen.getByRole("button", { name: /^quote/ }));
      } else if (next) fireEvent.click(next);
      tick(2_000);
    }
    fireEvent.change(screen.getByLabelText("units"), { target: { value: "999" } });
    fireEvent.click(screen.getByRole("button", { name: /^buy at/ }));
    expect(screen.getByText(/did not fit your balance — forfeited, −50/)).toBeInTheDocument();
  });
});
