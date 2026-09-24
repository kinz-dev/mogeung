/**
 * The ETF game, played through its own screen with the clock faked and the
 * markets seeded. What these watch is what a player would: the basket fills in
 * before the clock starts, the NAV worked out from the table is the one
 * revealed, trading the side it favours pays, the bots race and the round ends
 * without you if the clock runs out — and the session is kept.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { EtfGame, introMs } from "@/ui/EtfGame";

const tick = (ms: number) => act(() => vi.advanceTimersByTime(ms));
const money = (s: string | null | undefined) => Number((s ?? "").replace(/[^\d.]/g, ""));

/** The NAV as a player works it out: Σ weight × price, off the table. */
function workOutNav(): number {
  const basket = screen.getByRole("table", { name: "the basket" });
  const rows = within(basket).getAllByLabelText(/ row$/);
  return rows.reduce((t, row) => {
    const ticker = row.getAttribute("aria-label")!.replace(/ row$/, "");
    const w = Number(within(row).getByLabelText(`${ticker} weight`).textContent);
    const p = money(within(row).getByLabelText(`${ticker} price`).textContent);
    return t + w * p;
  }, 0);
}

const quote = () => ({ bid: money(screen.getByLabelText("bid").textContent), ask: money(screen.getByLabelText("ask").textContent) });

/** Open the round: the basket fills in and the clock starts. */
const open = () => tick(introMs(4) + 50);

/** Play every round on the side the worked-out NAV favours, `units` at a time. */
function playThrough(units = "10"): number {
  let rounds = 0;
  for (let guard = 0; guard < 80; guard++) {
    if (screen.queryByLabelText("profit and loss")) return rounds;
    if (screen.queryByLabelText("units")) {
      const nav = workOutNav();
      const q = quote();
      fireEvent.change(screen.getByLabelText("units"), { target: { value: units } });
      fireEvent.click(screen.getByRole("button", { name: nav > q.ask ? /^buy at/ : /^sell at/ }));
      continue;
    }
    const next = screen.queryByRole("button", { name: /^(next round|results)/ });
    if (next) {
      rounds++;
      fireEvent.click(next);
      continue;
    }
    tick(500);
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

describe("an ETF game, page by page", () => {
  it("lobby → every round → results, on the right side every time, and the session is kept", async () => {
    render(<EtfGame onExit={() => {}} onClose={() => {}} seed={42} />);
    expect(screen.getByText(/No sessions yet/)).toBeInTheDocument();
    start();
    // The basket fills in before there is anything to trade.
    expect(screen.queryByLabelText("units")).toBeNull();
    expect(screen.getByText(/market is opening/)).toBeInTheDocument();
    open();
    expect(screen.getByLabelText("the NAV")).toHaveTextContent("?");

    expect(playThrough()).toBe(6);
    expect(screen.getByText("Market closed")).toBeInTheDocument();
    const [, right, trades] = /(\d+)\/(\d+)/.exec(screen.getByText("trades on the right side").previousElementSibling?.textContent ?? "") ?? [];
    expect(right).toBe(trades);
    expect(within(screen.getByRole("table", { name: "round by round" })).getAllByRole("row")).toHaveLength(1 + 6);

    const kept = (localStorage.getItem("mogeung.arcade.etf-arbitrage") ?? "").trim().split("\n");
    expect(kept).toHaveLength(1);
    expect(JSON.parse(kept[0])).toMatchObject({ ending: "complete", settings: { level: "easy", seconds: 20 } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /^lobby/ }));
    });
    expect(screen.getByText("session")).toBeInTheDocument();
  });

  it("the NAV worked out from the table is the NAV revealed, and the right side pays", () => {
    render(<EtfGame onExit={() => {}} onClose={() => {}} seed={7} />);
    start();
    open();
    const nav = workOutNav();
    const q = quote();
    fireEvent.change(screen.getByLabelText("units"), { target: { value: "20" } });
    fireEvent.click(screen.getByRole("button", { name: nav > q.ask ? /^buy at/ : /^sell at/ }));
    tick(2_500);
    expect(money(screen.getByLabelText("the NAV").textContent)).toBeCloseTo(nav, 2);
    expect(screen.getByText(/gross − \$10\.00/)).toBeInTheDocument();
    expect(screen.queryByText(/the wrong side/)).toBeNull();
  });

  it("the clock runs out without you: the round ends, and you passed", () => {
    render(<EtfGame onExit={() => {}} onClose={() => {}} seed={3} />);
    start();
    open();
    expect(screen.getByLabelText("seconds left")).toHaveTextContent("20s");
    tick(20_200);
    expect(screen.getByText("Time ran out — no trade.")).toBeInTheDocument();
    expect(screen.getByLabelText("the NAV")).not.toHaveTextContent("?");
  });

  it("an order of 0 or 21 is refused in words; B, S and K trade from the keyboard", () => {
    render(<EtfGame onExit={() => {}} onClose={() => {}} seed={5} />);
    start();
    open();
    fireEvent.change(screen.getByLabelText("units"), { target: { value: "21" } });
    fireEvent.keyDown(screen.getByLabelText("units"), { key: "b" });
    expect(screen.getByText("Trade 1 to 20 whole units.")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("units"), { target: { value: "3" } });
    fireEvent.keyDown(screen.getByLabelText("units"), { key: "k" });
    expect(screen.getByText(/You passed — waiting for the others/)).toBeInTheDocument();
    tick(2_500);
    expect(screen.getByText("You skipped.")).toBeInTheDocument();
  });

  it("Escape at the table stops the game and shows it as played, not a result", () => {
    const onExit = vi.fn();
    render(<EtfGame onExit={onExit} onClose={() => {}} seed={3} />);
    start();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByText(/Stopped early/)).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onExit).toHaveBeenCalled();
  });
});

describe("the settings", () => {
  it("a difficulty sets its rounds and clock; changing either keeps the difficulty", () => {
    render(<EtfGame onExit={() => {}} onClose={() => {}} seed={1} />);
    fireEvent.click(screen.getByRole("button", { name: /^Hard/ }));
    expect(screen.getByLabelText("Rounds")).toHaveTextContent("10");
    expect(screen.getByLabelText("Time per round, seconds")).toHaveValue("12");
    fireEvent.change(screen.getByLabelText("Time per round, seconds"), { target: { value: "45" } });
    fireEvent.blur(screen.getByLabelText("Time per round, seconds"));
    fireEvent.click(screen.getByTitle("more — Rounds"));
    expect(screen.getByRole("button", { name: /^Hard/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("Hard is 12s as it comes")).toBeInTheDocument();
    start();
    expect(screen.getByText("round 1 of 11")).toBeInTheDocument();
    open();
    expect(screen.getByLabelText("seconds left")).toHaveTextContent("45s");
  });

  it("with no clock the round waits for you; without W × P, the column is gone", () => {
    render(<EtfGame onExit={() => {}} onClose={() => {}} seed={2} />);
    fireEvent.click(screen.getAllByRole("button", { name: /^no limit/ })[0]);
    fireEvent.click(within(screen.getByText("Show W × P").parentElement!).getByRole("button"));
    start();
    open();
    expect(screen.queryByText("W × P")).toBeNull();
    tick(10 * 60_000);
    expect(screen.getByLabelText("units")).toBeInTheDocument();
    expect(screen.getByText(/no clock/)).toBeInTheDocument();
  });

  it("with market events on, a headline comes from the second round", () => {
    render(<EtfGame onExit={() => {}} onClose={() => {}} seed={4} />);
    fireEvent.click(within(screen.getByText("Market events").parentElement!).getByRole("button"));
    start();
    open();
    expect(screen.queryByLabelText("headline")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /^skip/ }));
    tick(2_500);
    fireEvent.click(screen.getByRole("button", { name: /^next round/ }));
    expect(screen.getByLabelText("headline")).toBeInTheDocument();
  });
});
