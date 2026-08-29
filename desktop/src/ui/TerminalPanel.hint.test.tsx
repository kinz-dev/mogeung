/**
 * The command box's chord, named where the shells are. `R-O12`.
 *
 * Asked for on 2026-08-29: *"can we show this shortcut key in the tmux bar for
 * terminal so that it will display a hints"*. It reads the **binding** rather
 * than a string, which is the whole test: this window lets any action be
 * rebound, and a hint that names a chord somebody has moved is worse than no
 * hint at all. Four tooltips were found doing exactly that before `useChord`
 * existed.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { TerminalPanel } from "@/ui/TerminalPanel";

beforeEach(() => {
  cleanup();
  useStore.setState({
    showTerminal: true,
    prefs: { ...defaultPrefs(), keymap: {} },
    send: () => {},
  } as never);
});

describe("the terminal panel's hint", () => {
  it("names the chord that opens the command box", () => {
    render(<TerminalPanel />);
    expect(screen.getByText(/ask for a command/)).toBeInTheDocument();
    expect(screen.getByText(/Alt\+Shift\+M/i)).toBeInTheDocument();
  });

  /** The point of reading the binding: a rebind must move the hint with it. */
  it("follows a rebind rather than naming the default", () => {
    useStore.setState({
      prefs: { ...defaultPrefs(), keymap: { "terminal.command_box": ["Alt+Shift+j"] } },
    } as never);
    render(<TerminalPanel />);
    expect(screen.getByText(/Alt\+Shift\+J/i)).toBeInTheDocument();
    expect(screen.queryByText(/Alt\+Shift\+M/i)).not.toBeInTheDocument();
  });
});
