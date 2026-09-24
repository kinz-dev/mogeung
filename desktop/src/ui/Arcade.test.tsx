/**
 * The way in, and the room. Each of these is a way the easter egg could be
 * wrong without anyone noticing, because nobody is told it is there:
 *
 * - **It is found by π and only by π**, and never out of digits typed into a
 *   text box — a terminal command with 314159 in it must stay a command.
 * - **It is in no list.** The shortcuts window, the palette and the rebind
 *   table are all built from `ACTIONS`, so staying out of it is staying
 *   undocumented.
 * - **π opens a picker, not a game**, since the Trading Game joined it
 *   (2026-09-24) — and Escape walks back out: a game's first page to the
 *   picker, the picker to closed.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Arcade } from "@/ui/Arcade";
import { ACTIONS } from "@/lib/keymap";
import { useStore } from "@/store";

function press(keys: string, on: Element = document.body) {
  for (const key of keys) fireEvent.keyDown(on, { key });
}

const room = () => screen.queryByRole("dialog");

beforeEach(() => {
  localStorage.clear();
  useStore.setState({ queue: [], sessions: {} } as never);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the way in", () => {
  it("opens the picker on the digits of π, typed where the window has the keyboard", () => {
    render(<Arcade />);
    press("31415");
    expect(room()).toBeNull();
    press("9");
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Mental maths/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Trading Game/ })).toBeInTheDocument();
    expect(screen.getByText("you found it")).toBeInTheDocument();
  });

  it("does not open from digits typed into a text box", () => {
    render(
      <>
        <input aria-label="filter" />
        <Arcade />
      </>,
    );
    const box = screen.getByLabelText("filter");
    box.focus();
    press("314159", box);
    expect(room()).toBeNull();
  });

  it("starts over on another key, or on a pause between digits", () => {
    render(<Arcade />);
    press("314");
    press("a");
    press("159");
    expect(room()).toBeNull();

    let now = 1_000_000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    press("314");
    now += 2000;
    press("159");
    expect(room()).toBeNull();
  });

  it("is in no list the window documents its keys from", () => {
    const told = ACTIONS.filter((a) => /math|game|arcade|trading|secret|\bpi\b|π/i.test(`${a.id} ${a.label}`));
    expect(told).toEqual([]);
  });

  it("is only a little shy the second time", () => {
    const first = render(<Arcade />);
    press("314159");
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(room()).toBeNull();
    first.unmount();

    render(<Arcade />);
    press("314159");
    expect(room()).toBeInTheDocument();
    expect(screen.queryByText("you found it")).toBeNull();
  });
});

describe("the picker", () => {
  it("1 opens mental maths and 2 the Trading Game; Escape comes back here, then closes", () => {
    render(<Arcade />);
    press("314159");
    press("1");
    expect(screen.getByRole("dialog", { name: "mental math" })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();

    press("2");
    expect(screen.getByRole("dialog", { name: "trading game" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /start game/ })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(room()).toBeNull();
  });

  it("a click on a card opens it, and the header's back link returns", () => {
    render(<Arcade />);
    press("314159");
    fireEvent.click(screen.getByRole("button", { name: /Trading Game/ }));
    expect(screen.getByRole("dialog", { name: "trading game" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /games/ }));
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();
  });

  it("says on each card what has been played there", async () => {
    localStorage.setItem(
      "mogeung.arcade.trading-game",
      `${JSON.stringify({
        v: 1, at: "2026-09-24T10:00:00Z", settings: {}, ending: "complete", balance: 612, pnl: 112, place: 1,
        reports: 2, exact: 2, finalReported: null, rounds: [],
      })}\n`,
    );
    render(<Arcade />);
    press("314159");
    await waitFor(() => expect(screen.getByText(/1 session played · best \+\$112/)).toBeInTheDocument());
    expect(screen.getByText("not played yet")).toBeInTheDocument();
  });

  it("still says when a session needs you, and a click goes to it", () => {
    const select = vi.fn();
    useStore.setState({
      queue: [{ session_id: "s1", reason: "awaiting_input", score: 1, detail: "" }],
      sessions: { s1: { id: "s1", title: "fix the parser" } },
      select,
    } as never);
    render(<Arcade />);
    press("314159");
    fireEvent.click(screen.getByRole("button", { name: /a session needs you — fix the parser/ }));
    expect(select).toHaveBeenCalledWith("s1");
    expect(room()).toBeNull();
  });
});
