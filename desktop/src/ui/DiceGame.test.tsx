/**
 * The dice game, played through its own screen with the clock faked and the
 * roll seeded. What these watch is what a player would: the pages follow one
 * another, the dice and the quote are there to read, a report worked out from
 * the dice is taken as exact, and the session is kept — and, since it was the
 * one thing asked for, that the time limit you set in the lobby is the one the
 * table runs, including none at all.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { DiceGame, ROLL_MS } from "@/ui/DiceGame";

const tick = (ms: number) => act(() => vi.advanceTimersByTime(ms));

/** The dice as a player reads them: each face-up die's worth, and how many are face down. */
function readDice(): { open: number; hidden: number } {
  const dice = screen.getByLabelText("the dice");
  const open = [...dice.querySelectorAll("[aria-label^='a die showing']")].map((el) => {
    const label = el.getAttribute("aria-label") ?? "";
    return Number(/worth (\d+)/.exec(label)?.[1] ?? /showing (\d+)/.exec(label)?.[1]);
  });
  return { open: open.reduce((a, b) => a + b, 0), hidden: dice.querySelectorAll("[aria-label='a face-down die']").length };
}

function readQuote(): { bid: number; ask: number } {
  const [, bid, ask] = /(\d+) at (\d+)/.exec(screen.getByLabelText("the quote").textContent ?? "") ?? [];
  return { bid: Number(bid), ask: Number(ask) };
}

/** Land the dice and wait for the quote. */
function toQuote() {
  tick(ROLL_MS + 200);
}

/** Play every round: buy one when the fair value is over the ask, else sell one; report it exactly. */
function playThrough(): number {
  let rounds = 0;
  for (let guard = 0; guard < 40; guard++) {
    if (screen.queryByLabelText("profit and loss") || screen.queryByLabelText("your final balance")) return rounds;
    const roll = screen.queryByRole("button", { name: /^roll/ });
    if (roll) {
      fireEvent.click(roll);
      continue;
    }
    if (screen.queryByLabelText("units")) {
      const { open, hidden } = readDice();
      const buy = open + 3.5 * hidden > readQuote().ask;
      fireEvent.click(screen.getByRole("button", { name: buy ? /^buy at/ : /^sell at/ }));
      continue;
    }
    if (screen.queryByLabelText("your profit or loss")) {
      // Within the five seconds, every die is face up.
      const came = readDice().open;
      const { bid, ask } = readQuote();
      const bought = /You bought/.test(document.body.textContent ?? "");
      const pnl = bought ? came - ask : bid - came;
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
    toQuote();
  }
  throw new Error("the game did not end");
}

const start = () => fireEvent.click(screen.getByRole("button", { name: /^start game/ }));

beforeEach(() => {
  localStorage.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("a dice game, page by page", () => {
  it("lobby → every round → results, with exact reports, and the session is kept", async () => {
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    expect(screen.getByText(/No sessions yet/)).toBeInTheDocument();
    start();
    // The dice tumble before there is a quote to trade on.
    expect(screen.getAllByLabelText("a rolling die")).toHaveLength(3);
    expect(screen.getByText("no quote yet")).toBeInTheDocument();

    expect(playThrough()).toBe(3);
    expect(screen.getByText("Market closed")).toBeInTheDocument();
    const table = screen.getByRole("table", { name: "round by round" });
    expect(within(table).getAllByRole("row")).toHaveLength(1 + 3);
    // Every report was worked out from the dice, so every one was exact.
    const [, exact, of] = /(\d+)\/(\d+)/.exec(screen.getByText("reports exact").previousElementSibling?.textContent ?? "") ?? [];
    expect(exact).toBe(of);

    const kept = (localStorage.getItem("mogeung.arcade.dice-game") ?? "").trim().split("\n");
    expect(kept).toHaveLength(1);
    expect(JSON.parse(kept[0])).toMatchObject({ ending: "complete", settings: { tradeSeconds: 60 }, rounds: expect.any(Array) });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /^lobby/ }));
    });
    expect(screen.getByText("session")).toBeInTheDocument();
  });

  it("with the balance hidden, the game ends by asking for it, and a wrong answer costs 50", () => {
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={7} />);
    fireEvent.click(screen.getByRole("button", { name: /^Hard/ }));
    // Hard's own clock is five seconds; the test plays at its own pace.
    fireEvent.click(screen.getAllByRole("button", { name: /^no limit/ })[0]);
    start();
    expect(screen.getByLabelText("your balance")).toHaveTextContent("hidden");
    playThrough();
    expect(screen.getByText(/What is your final balance/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("your final balance"), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: /^answer/ }));
    expect(screen.getByText(/You said 1; it was -?\d+ — −50/)).toBeInTheDocument();
  });

  it("Escape at the table stops the game and shows it as played, not a result", () => {
    const onExit = vi.fn();
    render(<DiceGame onExit={onExit} onClose={() => {}} seed={3} />);
    start();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByText(/Stopped early/)).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("button", { name: /^start game/ })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onExit).toHaveBeenCalled();
  });

  it("an order over your balance is forfeited in words, and B, S and K trade from the keyboard", () => {
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    start();
    toQuote();
    fireEvent.change(screen.getByLabelText("units"), { target: { value: "999" } });
    fireEvent.keyDown(screen.getByLabelText("units"), { key: "b" });
    expect(screen.getByText(/Order forfeited — buy 999 did not fit your balance/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /^next round/ }));
    toQuote();
    fireEvent.keyDown(screen.getByLabelText("units"), { key: "k" });
    expect(screen.getByText("You skipped.")).toBeInTheDocument();
  });
});

describe("the time limit — set in the lobby, kept at the table", () => {
  const clockBox = () => screen.getByLabelText("Time to trade, seconds");

  it("any number of seconds can be typed, and the table runs out on that second", () => {
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    fireEvent.change(clockBox(), { target: { value: "7" } });
    fireEvent.blur(clockBox());
    start();
    toQuote();
    expect(screen.getByLabelText("seconds left")).toHaveTextContent("7s");
    tick(6_500);
    expect(screen.getByLabelText("units")).toBeInTheDocument();
    tick(700);
    expect(screen.getByText(/Time ran out — no trade, no penalty/)).toBeInTheDocument();
  });

  it("no limit means no clock: ten minutes on, the quote is still yours to take", () => {
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    fireEvent.click(screen.getAllByRole("button", { name: /^no limit/ })[0]);
    start();
    toQuote();
    expect(screen.getByText(/no clock — take your time/)).toBeInTheDocument();
    tick(10 * 60_000);
    expect(screen.getByLabelText("units")).toBeInTheDocument();
    // And a second press puts back the clock it had.
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.click(screen.getAllByRole("button", { name: /^no limit/ })[0]);
    expect(clockBox()).toHaveValue("60");
  });

  it("changing the clock keeps the difficulty: Hard at your own pace is still Hard", () => {
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    fireEvent.click(screen.getByRole("button", { name: /^Hard/ }));
    expect(clockBox()).toHaveValue("5");
    fireEvent.click(screen.getByTitle("more time — Time to trade"));
    fireEvent.click(screen.getByTitle("more time — Time to trade"));
    expect(clockBox()).toHaveValue("7");
    expect(screen.getByRole("button", { name: /^Hard/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("Hard is 5s as it comes")).toBeInTheDocument();
    // The dice, unlike the clock, are the difficulty's: change them and it is custom.
    fireEvent.click(screen.getByTitle("more — Dice"));
    expect(screen.getByRole("button", { name: /^Custom/ })).toHaveAttribute("aria-pressed", "true");
    expect(clockBox()).toHaveValue("7");
  });

  it("a report clock, when set, turns a late report into a wrong one", () => {
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    const report = screen.getByLabelText("Time to report, seconds");
    fireEvent.change(report, { target: { value: "10" } });
    fireEvent.keyDown(report, { key: "Enter" });
    start();
    toQuote();
    fireEvent.click(screen.getByRole("button", { name: /^buy at/ }));
    expect(screen.getByLabelText("your profit or loss")).toBeInTheDocument();
    tick(10_200);
    expect(screen.getByText(/no report in time; (profit forfeited and )?−50/)).toBeInTheDocument();
  });

  it("the clock you chose is remembered next time", () => {
    const first = render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    fireEvent.change(clockBox(), { target: { value: "45" } });
    fireEvent.blur(clockBox());
    first.unmount();
    render(<DiceGame onExit={() => {}} onClose={() => {}} seed={42} />);
    expect(clockBox()).toHaveValue("45");
  });
});
