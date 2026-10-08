/**
 * The outline goes where you click. `R-J125`.
 *
 * Asked 2026-10-08: *"The file editor 'symbol outline' is good, but I can't
 * actually click on the outline and navigate to it directly from the UI"*.
 *
 * Measured in a real browser before this was written: in the **source** view a
 * click jumped as it should; in the **rendered markdown** view it did nothing,
 * because every click went to the editor that view had unmounted. A repository
 * of docs is mostly read rendered, so that was most of the clicks.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { blockAt, symbolAt } from "@/lib/symbols";

/** What the pane asked of the editor, in order. */
const calls: string[] = [];
let moveCursor: ((line: number) => void) | null = null;

vi.mock("@monaco-editor/react", () => {
  const Fake = ({ onMount }: { onMount?: (ed: unknown, monaco: unknown) => void }) => {
    // Mounted once per editor, as the real component does.
    const ref = (el: HTMLDivElement | null) => {
      if (!el || el.dataset.mounted) return;
      el.dataset.mounted = "1";
      const listeners: ((e: { position: { lineNumber: number } }) => void)[] = [];
      moveCursor = (line) => listeners.forEach((l) => l({ position: { lineNumber: line } }));
      const ed = {
        revealLineInCenter: (l: number) => calls.push(`reveal ${l}`),
        setPosition: (p: { lineNumber: number }) => calls.push(`position ${p.lineNumber}`),
        focus: () => calls.push("focus"),
        getPosition: () => ({ lineNumber: 1 }),
        onDidChangeCursorPosition: (l: (e: { position: { lineNumber: number } }) => void) => listeners.push(l),
        onMouseDown: () => {},
        createDecorationsCollection: () => ({ set: () => {} }),
        getScrollTop: () => 0,
        setScrollTop: () => {},
      };
      const monaco = { editor: { setTheme: () => {}, defineTheme: () => {}, MouseTargetType: {} }, Range: class {} };
      onMount?.(ed, monaco);
    };
    return <div data-testid="source" ref={ref} />;
  };
  return { __esModule: true, default: Fake, Editor: Fake };
});

const DOC = ["# Intro", "", "words", "", "## Setup", "", "more", "", "## Usage", "", "end"].join("\n");

function open(path: string, content: string) {
  calls.length = 0;
  useStore.setState({
    prefs: { ...defaultPrefs() },
    selected: "s1",
    explorer: {
      s1: {
        ...emptyExplorer(),
        open: [{ path, rev: null, pinned: true, content, truncated: false, gotoLine: null }] as never,
      },
    },
  });
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  return render(
    <PaneScope id={filePaneId("s1", path, null)}>
      <FilePane />
    </PaneScope>,
  );
}

const openOutline = () => fireEvent.click(screen.getByTitle(/symbol outline/));
const row = (line: number) => screen.getByTitle(`line ${line}`);

beforeEach(() => {
  cleanup();
  calls.length = 0;
});

describe("clicking the outline", () => {
  it("jumps the editor to the symbol, cursor and all", () => {
    open("notes.md", DOC);
    openOutline();
    fireEvent.click(row(9));
    expect(calls).toEqual(["reveal 9", "position 9", "focus"]);
    expect(row(9).getAttribute("aria-current")).toBe("location");
  });

  /** **The bug.** Rendered, the click went to an editor that was gone. */
  it("scrolls the rendered markdown to the heading", () => {
    const scrolled: string[] = [];
    const spy = vi
      .spyOn(Element.prototype, "scrollIntoView")
      .mockImplementation(function (this: Element) {
        if (this.hasAttribute("data-line")) scrolled.push(`${this.tagName} ${this.textContent}`);
      });
    open("notes.md", DOC);
    fireEvent.click(screen.getByTitle(/read it as markdown/));
    openOutline();
    calls.length = 0;

    fireEvent.click(row(5));
    expect(scrolled).toEqual(["H2 Setup"]);
    // Not a word to the unmounted editor.
    expect(calls).toEqual([]);

    // The same heading again, after scrolling away, scrolls back.
    fireEvent.click(row(5));
    expect(scrolled).toEqual(["H2 Setup", "H2 Setup"]);
    spy.mockRestore();
  });

  it("marks the symbol the cursor is in", () => {
    open("notes.md", DOC);
    openOutline();
    act(() => moveCursor?.(7));
    expect(row(5).getAttribute("aria-current")).toBe("location");
    act(() => moveCursor?.(11));
    expect(row(9).getAttribute("aria-current")).toBe("location");
    expect(row(5).getAttribute("aria-current")).toBeNull();
  });
});

describe("go to symbol…", () => {
  /** The box was always called *go to*, and Enter in it did nothing. */
  it("goes to the first match on Enter", () => {
    open("notes.md", DOC);
    openOutline();
    const box = screen.getByLabelText("go to symbol");
    fireEvent.change(box, { target: { value: "us" } });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(calls).toEqual(["reveal 9", "position 9", "focus"]);
  });

  it("walks the matches with the arrows", () => {
    open("notes.md", DOC);
    openOutline();
    const box = screen.getByLabelText("go to symbol");
    // `Setup` and `Usage` both have an `s`; `Intro` does not.
    fireEvent.change(box, { target: { value: "s" } });
    fireEvent.keyDown(box, { key: "ArrowDown" });
    fireEvent.keyDown(box, { key: "Enter" });
    expect(calls).toEqual(["reveal 9", "position 9", "focus"]);
    fireEvent.keyDown(box, { key: "Escape" });
    expect((box as HTMLInputElement).value).toBe("");
  });
});

describe("the helpers", () => {
  it("puts a line in the symbol that starts at or above it", () => {
    const syms = [{ line: 1 }, { line: 5 }, { line: 9 }];
    expect(symbolAt(syms, 4)).toBe(0);
    expect(symbolAt(syms, 5)).toBe(1);
    expect(symbolAt(syms, 100)).toBe(2);
    expect(symbolAt([{ line: 3 }], 1)).toBe(-1);
  });

  it("finds the block a line belongs to, not the next one", () => {
    const blocks = [{ line: 1 }, { line: 6 }, { line: 9 }];
    expect(blockAt(blocks, 5)?.line).toBe(1);
    expect(blockAt(blocks, 9)?.line).toBe(9);
    expect(blockAt([{ line: 4 }], 2)).toBeNull();
  });
});
