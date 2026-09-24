/**
 * The way in, and the room. Each of these is a way the easter egg could be
 * wrong without anyone noticing, because nobody is told it is there:
 *
 * - **It is found by π and only by π**, and never out of digits typed into a
 *   text box — a terminal command with 314159 in it must stay a command.
 * - **It is in no list.** The shortcuts window, the palette and the rebind
 *   table are all built from `ACTIONS`, so staying out of it is staying
 *   undocumented.
 * - **π opens a window of its own** (2026-09-24), and a second π brings that
 *   one forward rather than opening another. Where no window can be opened
 *   the room opens over the main window instead, so π never does nothing.
 * - **π opens a picker, not a game**, since the Trading Game joined it
 *   (2026-09-24; the other five the same day) — and Escape walks
 *   back out: a game's first page to the picker, the picker to closed.
 * - **The arcade's window is not a client**, and still says when a session
 *   needs you: the main window tells it, and a click there selects the
 *   session over there.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Arcade } from "@/ui/Arcade";
import { ArcadeWindow } from "@/ui/ArcadeRoom";
import { ACTIONS } from "@/lib/keymap";
import { useStore } from "@/store";

function press(keys: string, on: Element = document.body) {
  for (const key of keys) fireEvent.keyDown(on, { key });
}

const room = () => screen.queryByRole("dialog");

/** The windows `window.open` has handed out, so each test starts with none open. */
let popups: { closed: boolean; focus: () => void }[] = [];

function opens(result: "window" | "nothing" = "window") {
  return vi.spyOn(window, "open").mockImplementation(() => {
    if (result === "nothing") return null;
    const w = { closed: false, focus: vi.fn() };
    popups.push(w);
    return w as unknown as Window;
  });
}

beforeEach(() => {
  localStorage.clear();
  useStore.setState({ queue: [], sessions: {} } as never);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  for (const w of popups) w.closed = true;
  popups = [];
});

describe("the way in", () => {
  it("opens the arcade's window on the digits of π, typed where the window has the keyboard", () => {
    const open = opens();
    render(<Arcade />);
    press("31415");
    expect(open).not.toHaveBeenCalled();
    press("9");
    expect(open).toHaveBeenCalledTimes(1);
    expect(open.mock.calls[0][0]).toBe("/arcade.html");
    // Not over the main window: the room is in the other one.
    expect(room()).toBeNull();
  });

  it("carries the theme across, so the new window does not flash the wrong one", () => {
    const open = opens();
    document.documentElement.setAttribute("data-theme", "light");
    try {
      render(<Arcade />);
      press("314159");
      expect(open.mock.calls[0][0]).toBe("/arcade.html?theme=light");
    } finally {
      document.documentElement.removeAttribute("data-theme");
    }
  });

  it("brings the open window forward rather than opening a second", () => {
    const open = opens();
    render(<Arcade />);
    press("314159");
    press("314159");
    expect(open).toHaveBeenCalledTimes(1);
    expect(popups[0].focus).toHaveBeenCalled();

    popups[0].closed = true;
    press("314159");
    expect(open).toHaveBeenCalledTimes(2);
  });

  it("opens the room over the main window when no window can be opened", () => {
    opens("nothing");
    render(<Arcade />);
    press("314159");
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(room()).toBeNull();
  });

  it("does not open from digits typed into a text box", () => {
    const open = opens();
    render(
      <>
        <input aria-label="filter" />
        <Arcade />
      </>,
    );
    const box = screen.getByLabelText("filter");
    box.focus();
    press("314159", box);
    expect(open).not.toHaveBeenCalled();
  });

  it("starts over on another key, or on a pause between digits", () => {
    const open = opens();
    render(<Arcade />);
    press("314");
    press("a");
    press("159");
    expect(open).not.toHaveBeenCalled();

    let now = 1_000_000;
    vi.spyOn(Date, "now").mockImplementation(() => now);
    press("314");
    now += 2000;
    press("159");
    expect(open).not.toHaveBeenCalled();
  });

  it("is in no list the window documents its keys from", () => {
    const told = ACTIONS.filter((a) => /math|game|arcade|trading|dice|etf|arbitrage|card|fermi|estimat|contract|secret|\bpi\b|π/i.test(`${a.id} ${a.label}`));
    expect(told).toEqual([]);
  });
});

describe("the arcade's window", () => {
  it("opens on the picker, and says so the first time", () => {
    render(<ArcadeWindow />);
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Mental maths/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Trading Game/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Dice market/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ETF arbitrage/ })).toBeInTheDocument();
    for (const name of [/Card game/, /Fermi estimation/, /Event contracts/]) expect(screen.getByRole("button", { name })).toBeInTheDocument();
    expect(screen.getByText("you found it")).toBeInTheDocument();
  });

  it("is only a little shy the second time", () => {
    render(<ArcadeWindow />).unmount();
    render(<ArcadeWindow />);
    expect(screen.queryByText("you found it")).toBeNull();
  });

  it("closes itself on Escape at the picker", () => {
    const close = vi.spyOn(window, "close").mockImplementation(() => undefined);
    render(<ArcadeWindow />);
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(close).toHaveBeenCalled();
  });

  it("hears from the main window when a session needs you, and a click selects it there", async () => {
    const raise = vi.spyOn(window, "focus").mockImplementation(() => undefined);
    const select = vi.fn();
    useStore.setState({
      queue: [{ session_id: "s1", reason: "awaiting_input", score: 1, detail: "" }],
      sessions: { s1: { id: "s1", title: "fix the parser" } },
      select,
    } as never);
    render(
      <>
        <Arcade />
        <ArcadeWindow />
      </>,
    );
    const line = await screen.findByRole("button", { name: /a session needs you — fix the parser/ });
    fireEvent.click(line);
    await waitFor(() => expect(select).toHaveBeenCalledWith("s1"));
    expect(raise).toHaveBeenCalled();
    // The game stays: it is the main window that comes forward.
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();

    useStore.setState({ queue: [] } as never);
    await waitFor(() => expect(screen.queryByRole("button", { name: /needs you/ })).toBeNull());
  });
});

describe("the picker", () => {
  it("1–7 open the seven games; Escape comes back here, then closes", () => {
    render(<ArcadeWindow />);
    press("1");
    expect(screen.getByRole("dialog", { name: "mental math" })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();

    press("2");
    expect(screen.getByRole("dialog", { name: "trading game" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /start game/ })).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();

    press("3");
    expect(screen.getByRole("dialog", { name: "dice game" })).toBeInTheDocument();
    expect(screen.getByLabelText("Time to trade, seconds")).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();

    press("4");
    expect(screen.getByRole("dialog", { name: "etf arbitrage" })).toBeInTheDocument();
    expect(screen.getByLabelText("Time per round, seconds")).toBeInTheDocument();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();

    for (const [key, name, lobby] of [
      ["5", "card game", "Cards"],
      ["6", "fermi estimation", "Questions"],
      ["7", "event contracts", "Rounds"],
    ]) {
      press(key);
      expect(screen.getByRole("dialog", { name })).toBeInTheDocument();
      expect(screen.getByLabelText(lobby)).toBeInTheDocument();
      fireEvent.keyDown(document.body, { key: "Escape" });
      expect(screen.getByRole("dialog", { name: "games" })).toBeInTheDocument();
    }
  });

  it("a click on a card opens it, and the header's back link returns", () => {
    render(<ArcadeWindow />);
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
    render(<ArcadeWindow />);
    await waitFor(() => expect(screen.getByText(/1 session played · best \+\$112/)).toBeInTheDocument());
    expect(screen.getAllByText("not played yet")).toHaveLength(6);
  });

});
