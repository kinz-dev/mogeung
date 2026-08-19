/**
 * Two tools in the rail at once. `R-J33`.
 *
 * The rail showed one at a time from `R-B40` until 2026-08-19 —
 * [ADR-0027](../../docs/decisions/0027-the-rail-stacks.md) — so both of these
 * fail against the code as it stood when this was asked for.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Rail } from "@/ui/Rail";
import { TooltipProvider } from "@/ui/primitives";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

class FakeSocket {
  static OPEN = 1;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  constructor(public url: string) {}
  send() {}
  close() {}
}
vi.stubGlobal("WebSocket", FakeSocket);

beforeEach(() => cleanup());

const open = (...rail: ("files" | "search" | "notes" | "bookmarks")[]) =>
  useStore.setState({ prefs: { ...defaultPrefs(), rail } });

describe("the rail's stack", () => {
  it("draws every open tool, each under its own heading", () => {
    open("files", "bookmarks");
    render(
      <TooltipProvider>
        <Rail />
      </TooltipProvider>,
    );

    expect(screen.getByRole("heading", { name: "Files" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Bookmarks" })).toBeTruthy();
  });

  /** The strip adds rather than swaps: the tool you were reading stays. */
  it("keeps what is open when the strip's next icon is clicked", () => {
    open("files");
    render(
      <TooltipProvider>
        <Rail />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByTitle(/^Bookmarks/));

    expect(useStore.getState().prefs.rail).toEqual(["files", "bookmarks"]);
  });

  /**
   * The divider between two tools, and the half that was wrong when this was
   * first written: the weights were read back off a ref at pointer-up, which
   * is only as fresh as the last render — so a drag that began and ended
   * inside one frame saved the sizes it started with.
   *
   * jsdom lays nothing out, so both sections are given a height; what is
   * asserted is the arithmetic, not the rendering. 400px each, dragged 100px
   * down, is a 500/300 split of the same two weights.
   */
  it("saves the split when a drag ends in the frame it started in", () => {
    const rect = vi
      .spyOn(HTMLDivElement.prototype, "getBoundingClientRect")
      .mockReturnValue({ height: 400, top: 0, left: 0, right: 0, bottom: 400, width: 300, x: 0, y: 0, toJSON: () => ({}) });
    open("files", "notes");
    render(
      <TooltipProvider>
        <Rail />
      </TooltipProvider>,
    );

    const split = screen.getByTestId("rail-split-notes");
    fireEvent.mouseDown(split, { clientY: 400 });
    // Both in one `act`, deliberately: a `fireEvent` each would let React
    // render in between, which is exactly the render the broken version
    // depended on and a quick drag does not get.
    act(() => {
      window.dispatchEvent(new MouseEvent("mousemove", { clientY: 500 }));
      window.dispatchEvent(new MouseEvent("mouseup", { clientY: 500 }));
    });

    expect(useStore.getState().prefs.railSizes).toEqual({ files: 1.25, notes: 0.75 });
    rect.mockRestore();
  });

  /** A section's ✕ closes that one only — `]` is what puts the rail away. */
  it("closes one tool and leaves the rest of the stack", () => {
    open("files", "notes", "bookmarks");
    render(
      <TooltipProvider>
        <Rail />
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByTitle(/^close Notes/));

    expect(useStore.getState().prefs.rail).toEqual(["files", "bookmarks"]);
  });
});
