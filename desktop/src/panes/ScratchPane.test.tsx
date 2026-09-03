/**
 * The pane that saves as you type. `R-L5`.
 *
 * What is pinned: one write per pause rather than per keystroke; a close
 * flushes what is pending; an acknowledged save says *saved* — and only when
 * nothing has been typed since. Monaco is replaced by a textarea, because
 * jsdom has no layout for it and the pane's behaviour is entirely in what it
 * sends and when.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useEffect } from "react";

let value = "";
const addCommand = vi.fn();
vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  default: ({
    defaultValue,
    onChange,
    onMount,
  }: {
    defaultValue: string;
    onChange: (v: string) => void;
    onMount: (ed: unknown, monaco: unknown) => void;
  }) => {
    // Once, like the real editor — a mount per render would reset what the
    // pane believes it last sent.
    useEffect(() => {
      value = defaultValue;
      onMount(
        { getValue: () => value, addCommand, focus: () => {} },
        { editor: { setTheme: () => {}, defineTheme: () => {} }, KeyMod: { CtrlCmd: 2048 }, KeyCode: { KeyS: 49 } },
      );
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);
    return (
      <textarea
        data-testid="editor"
        defaultValue={defaultValue}
        onChange={(e) => {
          value = e.target.value;
          onChange(value);
        }}
      />
    );
  },
}));

import { ScratchPane, SAVE_DELAY_MS } from "@/panes/ScratchPane";
import { PaneScope } from "@/lib/paneScope";
import { scratchPaneId } from "@/lib/scratch";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

const NAME = "scratch-1.java";
const send = vi.fn();

function open(content: string | null = "") {
  useStore.setState({
    send,
    prefs: defaultPrefs(),
    scratch: { names: [NAME], files: content === null ? {} : { [NAME]: { content, saved: 0 } } },
  });
  return render(
    <PaneScope id={scratchPaneId(NAME)}>
      <ScratchPane />
    </PaneScope>,
  );
}

const writes = () => send.mock.calls.filter(([m]) => m.cmd === "scratch_write");
const status = () => screen.getByTestId("scratch-status").textContent;

beforeEach(() => {
  vi.useFakeTimers();
  send.mockReset();
  addCommand.mockReset();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("a scratch file, saved as you type", () => {
  it("asks for the body on mount when it has none — a pane restored from the layout", () => {
    open(null);
    expect(send).toHaveBeenCalledWith({ cmd: "scratch_read", name: NAME });
    expect(screen.getByText(/loading scratch-1\.java/)).toBeTruthy();
  });

  it("sends one write per pause, not one per keystroke", () => {
    open("");
    const box = screen.getByTestId("editor");
    fireEvent.change(box, { target: { value: "c" } });
    fireEvent.change(box, { target: { value: "cl" } });
    fireEvent.change(box, { target: { value: "class A {}" } });
    expect(writes()).toHaveLength(0);
    expect(status()).toMatch(/unsaved/);

    act(() => void vi.advanceTimersByTime(SAVE_DELAY_MS));
    expect(writes()).toEqual([[{ cmd: "scratch_write", name: NAME, content: "class A {}" }]]);
  });

  it("says saved once the daemon has the body it was sent, and not before", () => {
    open("");
    const box = screen.getByTestId("editor");
    fireEvent.change(box, { target: { value: "x" } });
    act(() => void vi.advanceTimersByTime(SAVE_DELAY_MS));
    expect(status()).toMatch(/unsaved/);

    // Typed again before the acknowledgement arrives: the ack is for the old
    // body, so the pane is still unsaved.
    fireEvent.change(box, { target: { value: "xy" } });
    act(() => useStore.getState().ingest({ ev: "scratch_saved", name: NAME }));
    expect(status()).toMatch(/unsaved/);

    act(() => void vi.advanceTimersByTime(SAVE_DELAY_MS));
    act(() => useStore.getState().ingest({ ev: "scratch_saved", name: NAME }));
    expect(status()).toBe("saved");
    expect(writes()).toHaveLength(2);
  });

  it("flushes what is pending when the pane closes, and nothing when nothing changed", () => {
    const { unmount } = open("");
    fireEvent.change(screen.getByTestId("editor"), { target: { value: "pending" } });
    unmount();
    expect(writes()).toEqual([[{ cmd: "scratch_write", name: NAME, content: "pending" }]]);

    send.mockReset();
    const second = open("as it was");
    second.unmount();
    expect(writes()).toHaveLength(0);
  });

  it("binds Ctrl+S to save now, inside the editor rather than the keymap", () => {
    open("");
    expect(addCommand).toHaveBeenCalledWith(2048 | 49, expect.any(Function));
    fireEvent.change(screen.getByTestId("editor"), { target: { value: "now" } });
    const saveNow = addCommand.mock.calls[0]?.[1] as () => void;
    act(() => saveNow());
    expect(writes()).toEqual([[{ cmd: "scratch_write", name: NAME, content: "now" }]]);
  });
});
