/**
 * The hidden game, from the keyboard in. Each of these is a way it could be
 * wrong without anyone noticing, because nobody is told it is there:
 *
 * - **It is found by π and only by π**, and never out of digits typed into a
 *   text box — a terminal command with 314159 in it must stay a command.
 * - **It is in no list.** The shortcuts window, the palette and the rebind
 *   table are all built from `ACTIONS`, so staying out of it is staying
 *   undocumented.
 * - **Escape ends a run rather than throwing it away**, and a second Escape
 *   closes.
 * - **Endless has no clock**, and a timed run ends on its own.
 * - **It is the whole window**, since 2026-09-23 — a 580px box in the middle
 *   was too small to play on a big display.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MentalMath } from "@/ui/MentalMath";
import { ACTIONS } from "@/lib/keymap";
import { useStore } from "@/store";

function press(keys: string, on: Element = document.body) {
  for (const key of keys) fireEvent.keyDown(on, { key });
}

const dialog = () => screen.queryByRole("dialog", { name: "mental math" });

/** The question on screen, worked out the way a person would. */
function solve(): string {
  const text = screen.getByLabelText("question").textContent ?? "";
  const expr = text.replace(/,/g, "").replace(/×/g, "*").replace(/÷/g, "/").replace(/−/g, "-");
  return String(Function(`return (${expr})`)());
}

beforeEach(() => {
  localStorage.clear();
  useStore.setState({ queue: [], sessions: {} } as never);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("the way in", () => {
  it("opens on the digits of π, typed where the window has the keyboard", () => {
    render(<MentalMath />);
    press("31415");
    expect(dialog()).toBeNull();
    press("9");
    expect(dialog()).toBeInTheDocument();
    expect(screen.getByText("you found it")).toBeInTheDocument();
  });

  it("does not open from digits typed into a text box", () => {
    render(
      <>
        <input aria-label="filter" />
        <MentalMath />
      </>,
    );
    const box = screen.getByLabelText("filter");
    box.focus();
    press("314159", box);
    expect(dialog()).toBeNull();
  });

  it("starts over on another key, or on a pause between digits", () => {
    render(<MentalMath />);
    press("314");
    press("a");
    press("159");
    expect(dialog()).toBeNull();

    let now = 1_000_000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    press("314");
    now += 2000;
    press("159");
    expect(dialog()).toBeNull();
  });

  it("is in no list the window documents its keys from", () => {
    const told = ACTIONS.filter((a) => /math|game|secret|\bpi\b|π/i.test(`${a.id} ${a.label}`));
    expect(told).toEqual([]);
  });

  it("is only a little shy the second time", () => {
    const first = render(<MentalMath />);
    press("314159");
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(dialog()).toBeNull();
    first.unmount();

    render(<MentalMath />);
    press("314159");
    expect(dialog()).toBeInTheDocument();
    expect(screen.queryByText("you found it")).toBeNull();
  });
});

describe("playing", () => {
  function open() {
    render(<MentalMath />);
    press("314159");
  }

  it("endless: answers go in as they are typed, Enter skips, Escape shows how it went, Escape again closes", () => {
    open();
    fireEvent.click(screen.getByRole("button", { name: /endless/ }));
    fireEvent.click(screen.getByRole("button", { name: /^start/ }));

    // No run clock, no question clock.
    expect(screen.queryByTitle(/skips itself/)).toBeNull();
    expect(screen.getByText(/endless — this one/)).toBeInTheDocument();

    const box = screen.getByLabelText("answer");
    for (let i = 1; i <= 3; i++) {
      fireEvent.change(box, { target: { value: solve() } });
      expect(screen.getByLabelText("score")).toHaveTextContent(String(i));
      expect(box).toHaveValue("");
    }

    const asked = screen.getByLabelText("question").textContent ?? "";
    fireEvent.keyDown(box, { key: "Enter" });
    expect(screen.getByText(/skipped —/)).toHaveTextContent(asked);

    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(dialog()).toBeInTheDocument();
    expect(screen.getByText(/of training/)).toBeInTheDocument();
    expect(screen.getByLabelText("final score")).toHaveTextContent("3");
    expect(screen.getByText("first longest streak at this setting")).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "by operation" })).toBeInTheDocument();

    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(dialog()).toBeNull();
  });

  it("a timed run ends on its own clock and is judged", () => {
    vi.useFakeTimers();
    open();
    fireEvent.click(screen.getByRole("button", { name: "1 min" }));
    fireEvent.click(screen.getByRole("button", { name: /^start/ }));
    fireEvent.change(screen.getByLabelText("answer"), { target: { value: solve() } });

    act(() => vi.advanceTimersByTime(59_000));
    expect(screen.getByLabelText("answer")).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(1_500));

    expect(screen.queryByLabelText("answer")).toBeNull();
    expect(screen.getByText("Keep practising")).toBeInTheDocument();
    expect(screen.getByLabelText("final score")).toHaveTextContent("1");
    // Enter meant as a last skip must not start the next run.
    expect(screen.getByRole("button", { name: /again/ })).toBeDisabled();
    act(() => vi.advanceTimersByTime(600));
    expect(screen.getByRole("button", { name: /again/ })).toBeEnabled();
  });

  it("a timed run stopped by hand is played, not a score", () => {
    open();
    fireEvent.click(screen.getByRole("button", { name: /^start/ }));
    fireEvent.change(screen.getByLabelText("answer"), { target: { value: solve() } });
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByText(/played, not a score/)).toBeInTheDocument();
    expect(screen.queryByText(/best/)).toBeNull();
  });

  it("takes the whole window, and a click anywhere keeps the keyboard in the answer box", () => {
    open();
    // The whole application screen, not a box in the middle of it.
    expect(dialog()).toHaveClass("fixed", "inset-0");
    fireEvent.click(screen.getByRole("button", { name: /^start/ }));
    const box = screen.getByLabelText("answer");
    box.blur();
    fireEvent.mouseUp(screen.getByLabelText("question"));
    expect(box).toHaveFocus();
  });

  it("the question timer is a setting: auto is the original's, a number applies even when endless, off has no bar", () => {
    vi.useFakeTimers();
    open();
    const timer = within(screen.getByRole("group", { name: "question timer" }));
    expect(timer.getByRole("button", { name: "auto · 8s" })).toHaveAttribute("aria-pressed", "true");

    fireEvent.click(screen.getByRole("button", { name: /endless/ }));
    expect(timer.getByRole("button", { name: "auto · none" })).toBeInTheDocument();
    fireEvent.click(timer.getByRole("button", { name: "5s" }));
    expect(screen.getByText(/a question left 5s skips itself/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /^start/ }));

    expect(screen.getByText("Easy · endless · 5s a question")).toBeInTheDocument();
    expect(screen.getByTitle(/skips itself/)).toBeInTheDocument();
    const asked = screen.getByLabelText("question").textContent ?? "";
    act(() => vi.advanceTimersByTime(5_100));
    expect(screen.getByText(/time's up —/)).toHaveTextContent(asked);

    // Kept: the next game opens where this one was left.
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "settings" }));
    fireEvent.click(within(screen.getByRole("group", { name: "question timer" })).getByRole("button", { name: "off" }));
    fireEvent.click(screen.getByRole("button", { name: /^2 min/ }));
    fireEvent.click(screen.getByRole("button", { name: /^start/ }));
    expect(screen.queryByTitle(/skips itself/)).toBeNull();
    const waiting = screen.getByLabelText("question").textContent;
    act(() => vi.advanceTimersByTime(60_000));
    expect(screen.getByLabelText("question")).toHaveTextContent(waiting ?? "");
    expect(screen.queryByText(/time's up/)).toBeNull();
  });

  it("still says when a session needs you, and a click goes to it", () => {
    const select = vi.fn();
    useStore.setState({
      queue: [{ session_id: "s1", reason: "awaiting_input", score: 1, detail: "" }],
      sessions: { s1: { id: "s1", title: "fix the parser" } },
      select,
    } as never);
    open();
    fireEvent.click(screen.getByRole("button", { name: /a session needs you — fix the parser/ }));
    expect(select).toHaveBeenCalledWith("s1");
    expect(dialog()).toBeNull();
  });
});
