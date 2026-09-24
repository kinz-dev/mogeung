/**
 * The hidden game, from the keyboard in. Each of these is a way it could be
 * wrong without anyone noticing, because nobody is told it is there:
 *
 * - (How it is found — π, and only π — is `Arcade.test.tsx`'s, since the
 *   digits open the arcade's picker rather than this game.)
 * - **Escape ends a run rather than throwing it away**, and a second Escape
 *   closes.
 * - **Endless has no clock**, and a timed run ends on its own.
 * - **It is the whole window**, since 2026-09-23 — a 580px box in the middle
 *   was too small to play on a big display.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Arcade } from "@/ui/Arcade";
import { ArcadeWindow } from "@/ui/ArcadeRoom";
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
  // Escape at the picker closes the arcade's window, and jsdom's `close`
  // would take the test's document with it.
  vi.spyOn(window, "close").mockImplementation(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("playing", () => {
  /** Through the arcade's window, as it is really reached: its picker, then 1 for mental maths. */
  function open() {
    render(<ArcadeWindow />);
    press("1");
  }

  it("endless: answers go in as they are typed, Enter skips, Escape shows how it went, and walks back page by page", () => {
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

    // Results → setup → the arcade's picker → closed.
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("button", { name: /^start/ })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    // …and the picker closes the arcade's window.
    expect(window.close).toHaveBeenCalled();
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
    // The whole of its window, not a box in the middle of it.
    expect(dialog()).toHaveClass("h-screen");
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

  /**
   * The history, asked for 2026-09-23: every game is kept, the results say how
   * this one compares, and a progress screen draws the trend.
   */
  it("keeps every game, compares the next with it, and draws the progress", async () => {
    vi.useFakeTimers();
    open();
    const oneGame = (answers: number) => {
      fireEvent.click(screen.getByRole("button", { name: /^start/ }));
      for (let i = 0; i < answers; i++) fireEvent.change(screen.getByLabelText("answer"), { target: { value: solve() } });
      act(() => vi.advanceTimersByTime(61_000));
    };
    fireEvent.click(screen.getByRole("button", { name: "1 min" }));
    oneGame(2);
    const kept = (localStorage.getItem("mogeung.arcade.mental-math") ?? "").trim().split("\n");
    expect(kept).toHaveLength(1);
    expect(JSON.parse(kept[0]).log.filter((a: { how: string }) => a.how === "right")).toHaveLength(2);

    act(() => vi.advanceTimersByTime(600));
    fireEvent.click(screen.getByRole("button", { name: /again/ }));
    for (let i = 0; i < 3; i++) fireEvent.change(screen.getByLabelText("answer"), { target: { value: solve() } });
    act(() => vi.advanceTimersByTime(61_000));
    expect(screen.getByLabelText("against your recent games")).toHaveTextContent(/against your last game here/);

    fireEvent.click(screen.getByRole("button", { name: /progress/ }));
    expect(screen.getByText("time trained")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: /answers per minute/ })).toBeInTheDocument();
    expect(screen.getByRole("table", { name: "speed by operation" })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByLabelText("final score")).toBeInTheDocument();
  });

  it("still says when a session needs you, and a click goes to it without ending the game", async () => {
    vi.spyOn(window, "focus").mockImplementation(() => undefined);
    const select = vi.fn();
    useStore.setState({
      queue: [{ session_id: "s1", reason: "awaiting_input", score: 1, detail: "" }],
      sessions: { s1: { id: "s1", title: "fix the parser" } },
      select,
    } as never);
    // The main window's half, which is what tells the arcade's window.
    render(<Arcade />);
    open();
    fireEvent.click(await screen.findByRole("button", { name: /a session needs you — fix the parser/ }));
    await waitFor(() => expect(select).toHaveBeenCalledWith("s1"));
    expect(dialog()).toBeInTheDocument();
  });
});
