/**
 * The window's zoom, and the one thing it must stop doing.
 *
 * A CSS `zoom` on the document root scales layout but leaves mouse
 * coordinates and element rectangles in two different spaces. Anything that
 * maps a pointer to a cell then drifts, further the further you drag — which
 * is exactly how *"I can't select text in the Agent pane correctly"* was
 * reported, twice, after two fixes at layers that could not reach it.
 *
 * So the assertion worth having is a negative one: in the desktop shell, this
 * must ask the **webview** to zoom and leave the document alone.
 */

import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";

const setWebviewZoom = vi.fn(async (_factor: number) => {});
let tauri = false;

vi.mock("@/lib/tauri", () => ({
  isTauri: () => tauri,
  setWebviewZoom: (f: number) => setWebviewZoom(f),
}));

function cssZoom(): string {
  return (document.documentElement.style as unknown as { zoom: string }).zoom ?? "";
}

beforeEach(() => {
  setWebviewZoom.mockClear();
  setWebviewZoom.mockImplementation(async () => {});
  (document.documentElement.style as unknown as { zoom: string }).zoom = "";
});

afterEach(() => {
  tauri = false;
});

describe("the window's zoom", () => {
  it("asks the webview and never zooms the document", async () => {
    tauri = true;
    const { applyAppZoom } = await import("@/lib/zoom");
    applyAppZoom(1.4641);
    expect(setWebviewZoom).toHaveBeenCalledWith(1.4641);
    // The whole bug in one line.
    expect(cssZoom()).toBe("");
  });

  it("falls back to CSS in a browser tab, which has no webview to ask", async () => {
    tauri = false;
    const { applyAppZoom } = await import("@/lib/zoom");
    applyAppZoom(1.21);
    expect(setWebviewZoom).not.toHaveBeenCalled();
    expect(cssZoom()).toBe("1.21");
  });

  it("still zooms if the webview refuses — a window stuck at one size is worse", async () => {
    tauri = true;
    setWebviewZoom.mockImplementation(async () => {
      throw new Error("capability missing");
    });
    vi.spyOn(console, "error").mockImplementation(() => {});
    const { applyAppZoom } = await import("@/lib/zoom");
    applyAppZoom(1.1);
    // The rejection is handled a microtask later.
    await Promise.resolve();
    await Promise.resolve();
    expect(cssZoom()).toBe("1.1");
    expect(console.error).toHaveBeenCalled();
  });
});
