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
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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

/**
 * Where the keyboard goes after a command lands, and what `Alt+Enter` sends.
 * `R-O12`, reported 2026-08-29: *"the keyboard focus back to the attention
 * list"* — so the command was in the shell and the cursor was not.
 *
 * The panel is rendered without a Tauri pty here, so what is pinned is the
 * store signal and the bytes, which are the two things a test can hold. The
 * write itself is the same `ptyWrite` every keystroke already takes.
 */
describe("accepting a command", () => {
  it("asks the terminal for the keyboard, by pane id", async () => {
    const { CommandBox } = await import("@/ui/CommandBox");
    const seen: string[] = [];
    useStore.setState({
      showCommandBox: true,
      focusTerminal: null,
      commandDraft: {
        id: "c1",
        question: "list the files",
        command: "ls -la",
        pending: false,
        started: 0,
        error: null,
        model: "qwen",
        elapsed_ms: 10,
      },
    } as never);
    render(
      <CommandBox
        repo="/w"
        onAccept={(c) => {
          seen.push(c);
          useStore.setState({ focusTerminal: { id: "shell:one", nonce: 1 } });
        }}
      />,
    );
    fireEvent.click(screen.getByText(/put it in the line/));
    expect(seen).toEqual(["ls -la"]);
    // The signal names the pane: two shells open, and only the one the command
    // went into should take the keyboard.
    expect(useStore.getState().focusTerminal).toEqual({ id: "shell:one", nonce: 1 });
  });
});
