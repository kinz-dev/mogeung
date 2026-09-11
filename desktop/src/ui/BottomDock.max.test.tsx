/**
 * The dock can fill the window. `R-D29`.
 *
 * Three panes at 280 px is a cramped IntelliJ, and IntelliJ answers that with
 * a maximise. Here it is a preference, so it survives switching tools; the
 * gesture is the button, the chord, and IntelliJ's own — a double-click on
 * the open tool's tab.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { BottomDock } from "@/ui/BottomDock";
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

function show(dock: "git" | null, dockMax = false) {
  useStore.setState({
    selected: null,
    sessions: {},
    git: {},
    prefs: { ...defaultPrefs(), dock, dockMax },
  } as never);
  return render(<BottomDock />);
}

beforeEach(() => cleanup());

describe("maximising the dock", () => {
  it("asks for the whole column when maximised, and a height otherwise", () => {
    const { container, rerender } = show("git");
    const dockEl = () => container.querySelector<HTMLElement>("[data-max]") ?? container.querySelector<HTMLElement>("div[style*='height']");
    expect(dockEl()?.style.height).toBe("280px");
    fireEvent.click(screen.getByRole("button", { name: /maximise the dock/ }));
    expect(useStore.getState().prefs.dockMax).toBe(true);
    rerender(<BottomDock />);
    const maxed = container.querySelector<HTMLElement>("[data-max]");
    expect(maxed?.style.flex).toBe("1 1 100%");
    expect(maxed?.style.height).toBe("");
    expect(screen.getByRole("button", { name: /restore the dock/ })).toBeInTheDocument();
  });

  it("toggles on a double-click of the open tool's tab, and only that tab", () => {
    show("git");
    const tabs = screen.getAllByRole("button", { pressed: false });
    const changes = tabs.find((b) => b.textContent === "Changes")!;
    fireEvent.doubleClick(changes);
    expect(useStore.getState().prefs.dockMax).toBe(false);
    fireEvent.doubleClick(screen.getByRole("button", { pressed: true, name: /Git/ }));
    expect(useStore.getState().prefs.dockMax).toBe(true);
  });

  it("leaves the collapsed strip alone", () => {
    show(null, true);
    expect(screen.queryByRole("button", { name: /restore the dock/ })).toBeNull();
  });
});
