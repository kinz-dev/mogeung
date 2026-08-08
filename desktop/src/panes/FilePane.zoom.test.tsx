/**
 * Ctrl+wheel reaches the file pane — in both of its views. `R-J26`.
 *
 * Asked 2026-08-08: *"if I click and enabled the eye icon, after that it will
 * display the content as MD file, how can I zoom in/out with that?"* — and the
 * answer was that you could not, in **either** view.
 *
 * Two faults behind one symptom, and the second was hiding the first. The
 * wrapper writes `zoom["file"]` and this pane read `zoom["code"]`, a key
 * `R-B53` left behind when it dissolved the Code pane — so the factor was
 * written, stored, clamped, and read by nothing. And the pane is registered
 * `scale={false}` so that Monaco can take the factor as a font size, which
 * leaves the *rendered* view with nothing applying it at all.
 *
 * These fail against the code as it stood when the question was asked, which
 * is the point: a test that documents working behaviour would have said
 * nothing here.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";

vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  // The one thing under test on the source side is the font size it is handed.
  default: ({ options }: { options?: { fontSize?: number } }) => (
    <div data-testid="source" data-font={options?.fontSize} />
  ),
  Editor: ({ options }: { options?: { fontSize?: number } }) => (
    <div data-testid="source" data-font={options?.fontSize} />
  ),
}));

const DOC = "# Title\n\nsome prose";

function open(zoom: Record<string, number>) {
  useStore.setState({
    prefs: { ...defaultPrefs(), zoom },
    selected: "s1",
    explorer: {
      s1: {
        ...emptyExplorer(),
        open: [
          { path: "notes.md", rev: null, pinned: true, content: DOC, truncated: false, gotoLine: null },
        ] as never,
      },
    },
  });
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  return render(
    <PaneScope id={filePaneId("s1", "notes.md", null)}>
      <FilePane />
    </PaneScope>,
  );
}

/** The rendered document's own scroller, which is what carries the factor. */
const previewBox = () => screen.getByTestId("preview-body");

beforeEach(() => cleanup());

describe("zooming a file pane", () => {
  /**
   * The key the wheel writes is the key the pane must read. It wrote `file`
   * and this read `code`, so every Ctrl+wheel in a file pane went nowhere.
   */
  it("reads the factor the wheel actually writes", () => {
    open({ file: 1.5 });
    expect(screen.getByTestId("source").getAttribute("data-font")).toBe(`${12 * 1.5}`);
  });

  /** A factor set before `R-B53`'s rename is inherited rather than reset. */
  it("inherits a zoom stored under the old Code-pane key", () => {
    open({ code: 2 });
    expect(screen.getByTestId("source").getAttribute("data-font")).toBe("24");
  });

  /** …but the current key wins where both exist. */
  it("prefers the current key over the legacy one", () => {
    open({ file: 1.5, code: 2 });
    expect(screen.getByTestId("source").getAttribute("data-font")).toBe("18");
  });

  /**
   * The ask itself: the **rendered** view scales too. `ZoomPane` keeps its
   * hands off this pane so Monaco can have the factor as a font size, which
   * leaves the preview with nothing applying it unless the pane does.
   */
  it("scales the rendered markdown, not only the source", () => {
    open({ file: 1.5 });
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    expect(previewBox().style.zoom).toBe("1.5");
  });

  /**
   * **The chrome does not scale.** The find bar is a control, and a filter box
   * that grows with the prose is one you then have to hunt for.
   */
  it("leaves the pane's controls at their own size", () => {
    open({ file: 2 });
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    const eye = screen.getByTitle(/read it as markdown/i);
    expect(eye.closest("[style*='zoom']")).toBeNull();
  });

  /**
   * No factor, no style attribute at all — `zoom: 1` is not quite a no-op in a
   * webview, which is the same reason `ZoomPane` omits it rather than writing
   * the identity. (Asserted on the attribute: jsdom does not implement the
   * non-standard `zoom` property, so an unset one reads `undefined` rather
   * than the empty string a standard property would give.)
   */
  it("sets nothing at all at the default factor", () => {
    open({});
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    expect(previewBox().getAttribute("style")).toBeNull();
  });
});
