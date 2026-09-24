/**
 * The Fermi game through its own screen, clock faked and questions seeded.
 * A player who knows every answer scores the most there is; a range typed as
 * people type ("8.1bn") reads; the clock you set is the one that runs out;
 * and the session is kept.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { BANK } from "@/lib/fermi";
import { FermiGame } from "@/ui/FermiGame";

const tick = (ms: number) => act(() => vi.advanceTimersByTime(ms));
const answerTo = () => BANK.find((q) => q.text === screen.getByLabelText("the question").textContent)!.answer;

function answer(low: string, high: string) {
  fireEvent.change(screen.getByLabelText("low"), { target: { value: low } });
  fireEvent.change(screen.getByLabelText("high"), { target: { value: high } });
  fireEvent.keyDown(screen.getByLabelText("high"), { key: "Enter" });
}

beforeEach(() => {
  localStorage.clear();
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("a Fermi game, page by page", () => {
  it("knowing every answer scores the most there is, and the session is kept", () => {
    render(<FermiGame onExit={() => {}} onClose={() => {}} seed={42} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    for (let i = 0; i < 8; i++) {
      const a = answerTo();
      answer(String(a), String(a));
      expect(screen.getByText(/Inside your range — near perfect/)).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: /^(next question|results)/ }));
    }
    expect(screen.getByLabelText("score")).toHaveTextContent("1600");
    expect(screen.getByText(/of 1600 · 100%/)).toBeInTheDocument();
    const kept = JSON.parse((localStorage.getItem("mogeung.arcade.fermi-estimation") ?? "").trim());
    expect(kept).toMatchObject({ points: 1600, max: 1600, hits: 8, ending: "complete" });
  });

  it("reads a range typed the way people type, shows what it read, and says how it scored", () => {
    render(<FermiGame onExit={() => {}} onClose={() => {}} seed={1} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    const a = answerTo();
    fireEvent.change(screen.getByLabelText("low"), { target: { value: String(a / 2) } });
    fireEvent.change(screen.getByLabelText("high"), { target: { value: `${(a * 2) / 1000}k` } });
    expect(screen.getByLabelText("your range")).toHaveTextContent(/a range ×4\.0 wide/);
    fireEvent.click(screen.getByRole("button", { name: /^answer/ }));
    // Midpoint on the answer, but ×4 wide — past the ×3.2 band, so the bonus is ×1.
    expect(document.body.textContent).toMatch(/near perfect: 50 \+ 100 × 1 = 150/);
    expect(screen.getByLabelText("points so far")).toHaveTextContent("150");
  });

  it("the clock you set runs out, and the question scores nothing", () => {
    render(<FermiGame onExit={() => {}} onClose={() => {}} seed={2} />);
    fireEvent.change(screen.getByLabelText("Time per question, seconds"), { target: { value: "9" } });
    fireEvent.blur(screen.getByLabelText("Time per question, seconds"));
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.getByLabelText("seconds left")).toHaveTextContent("9s");
    tick(9_200);
    expect(screen.getByText("Time ran out — no points.")).toBeInTheDocument();
  });

  it("hints show when asked for; without them, there is no hint to ask for", () => {
    render(<FermiGame onExit={() => {}} onClose={() => {}} seed={3} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    fireEvent.click(screen.getByRole("button", { name: /^show a hint/ }));
    const q = BANK.find((x) => x.text === screen.getByLabelText("the question").textContent)!;
    expect(screen.getByText(q.hints[0])).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: /^Medium/ }));
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    expect(screen.queryByRole("button", { name: /^show a hint/ })).toBeNull();
  });

  it("Escape stops the game and shows it as played; then back to the lobby and out", () => {
    const onExit = vi.fn();
    render(<FermiGame onExit={onExit} onClose={() => {}} seed={4} />);
    fireEvent.click(screen.getByRole("button", { name: /^start game/ }));
    fireEvent.click(screen.getByRole("button", { name: /^skip/ }));
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByText(/Stopped early/)).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(onExit).toHaveBeenCalled();
  });
});
