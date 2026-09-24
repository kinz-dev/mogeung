/**
 * The Event Contracts game through its own screen, clock faked and events
 * seeded. A player who does the update from the brief and quotes around it is
 * told the quote was well centred; S stands aside; a quote too wide is refused
 * in words; the clock you set runs out; and the session is kept.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { posterior, type Direction } from "@/lib/eventContracts";
import { EventGame } from "@/ui/EventGame";

const tick = (ms: number) => act(() => vi.advanceTimersByTime(ms));

/** The update, read off the brief as a player would do it. */
function update(): number {
  const text = screen.getByLabelText("the brief").textContent ?? "";
  const prior = Number(/Base rate (\d+)%/.exec(text)![1]);
  const lr = /([\d.]+)× as likely if it (happens|does not happen)/.exec(text);
  const dir: Direction = !lr ? "none" : lr[2] === "happens" ? "for" : "against";
  return posterior(prior, lr ? Number(lr[1]) : null, dir);
}

function quoteAround(p: number, width: number) {
  const bid = Math.max(0, Math.round(p - width / 2));
  fireEvent.change(screen.getByLabelText("bid"), { target: { value: String(bid) } });
  fireEvent.change(screen.getByLabelText("ask"), { target: { value: String(Math.min(100, bid + width)) } });
  fireEvent.keyDown(screen.getByLabelText("ask"), { key: "Enter" });
}

beforeEach(() => {
  localStorage.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("an Event Contracts game, page by page", () => {
  it("the update from the brief, quoted around, is a centred quote every round — and the session is kept", () => {
    render(<EventGame onExit={() => {}} onClose={() => {}} seed={42} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    for (let i = 0; i < 6; i++) {
      quoteAround(update(), 7);
      const review = screen.getByLabelText("the review");
      expect(within(review).getByText("true probability")).toBeInTheDocument();
      expect(within(review).queryByText(/Mis-centred/)).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: /^(next round|results)/ }));
    }
    expect(screen.getByText("Markets resolved")).toBeInTheDocument();
    expect(screen.getByText("6/6")).toBeInTheDocument();
    const kept = JSON.parse((localStorage.getItem("mogeung.arcade.event-contracts") ?? "").trim());
    expect(kept).toMatchObject({ ending: "complete", quoted: 6, centred: 6 });
    expect(kept.rounds).toHaveLength(6);
  });

  it("a quote wider than the round allows is refused in words; S stands aside at no cost", () => {
    render(<EventGame onExit={() => {}} onClose={() => {}} seed={3} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    fireEvent.change(screen.getByLabelText("bid"), { target: { value: "10" } });
    fireEvent.change(screen.getByLabelText("ask"), { target: { value: "90" } });
    fireEvent.keyDown(screen.getByLabelText("ask"), { key: "Enter" });
    expect(screen.getByText(/widest quote is 14¢/)).toBeInTheDocument();
    fireEvent.keyDown(screen.getByLabelText("ask"), { key: "s" });
    expect(screen.getByText("stood aside")).toBeInTheDocument();
    expect(screen.getByLabelText("score so far")).toHaveTextContent("0");
  });

  it("the clock you set runs out, and the round closes as a skip would", () => {
    render(<EventGame onExit={() => {}} onClose={() => {}} seed={5} />);
    fireEvent.change(screen.getByLabelText("Time per round, seconds"), { target: { value: "12" } });
    fireEvent.blur(screen.getByLabelText("Time per round, seconds"));
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.getByLabelText("seconds left")).toHaveTextContent("12s");
    tick(12_200);
    expect(screen.getByText("timed out")).toBeInTheDocument();
  });

  it("a difficulty sets rounds and clock; changing them keeps it", () => {
    render(<EventGame onExit={() => {}} onClose={() => {}} seed={6} />);
    fireEvent.click(screen.getByRole("button", { name: /^Hard/ }));
    expect(screen.getByLabelText("Rounds")).toHaveTextContent("10");
    expect(screen.getByLabelText("Time per round, seconds")).toHaveValue("30");
    fireEvent.click(screen.getAllByRole("button", { name: /^no limit/ })[0]);
    expect(screen.getByRole("button", { name: /^Hard/ })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.getByText(/no clock/)).toBeInTheDocument();
    expect(screen.getByText(/±6 points/)).toBeInTheDocument();
  });
});
