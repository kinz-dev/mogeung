/**
 * The markdown preview fits its pane. `R-J96`.
 *
 * Reported 2026-09-16, twice: *"the line wrapper still doesn't work if I view
 * a md file in markdown mode"*. The wrap **option** was right by then — the
 * editor had `wordWrap: "on"` and `isViewportWrapping: true` for a `.md`
 * (`R-J95`) — and the preview still ran off the right edge, because nothing in
 * it was ever asked to wrap against the pane.
 *
 * The cause is one CSS default. The preview is a flex **item** in a row, and a
 * flex item's `min-width` is `auto`: it will not shrink below the intrinsic
 * width of its content. One wide table, or one long line, and the preview lays
 * itself out at the width of its widest child — **measured in the running
 * window at 10 154 px inside an 821 px pane**, with the dockview group
 * clipping the overflow. So the text had nothing to wrap against and no way to
 * scroll to it.
 *
 * **What this test can and cannot do.** jsdom performs no layout and no
 * Tailwind, so nothing here can measure a width — asserting `clientWidth`
 * would pass against the bug. What it pins is the contract the bug broke, the
 * same weaker instrument [`FilePane.layout.test.tsx`](FilePane.layout.test.tsx)
 * settled for when this happened to the *height*: the boxes that have to be
 * allowed to shrink say so, in the markup, where the next person editing these
 * class strings will see it.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";

vi.mock("@monaco-editor/react", () => {
  const Editor = ({ value }: { value: string }) => <div data-testid="source">{value}</div>;
  return { __esModule: true, default: Editor, Editor };
});

/** A table with rows far wider than any pane — the shape that blew it out. */
const WIDE = [
  "# Roadmap",
  "",
  "| # | Item |",
  "|---|---|",
  `| R-1 | ${"a very long cell that goes on and on ".repeat(30)} |`,
  "",
  `${"an ordinary paragraph that is also very long ".repeat(40)}`,
].join("\n");

function show() {
  cleanup();
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    explorer: {
      s1: {
        ...emptyExplorer(),
        open: [{ path: "docs/roadmap.md", rev: null, pinned: true, content: WIDE, truncated: false, gotoLine: null }] as never,
      },
    },
  });
  render(
    <PaneScope id={filePaneId("s1", "docs/roadmap.md", null)}>
      <FilePane />
    </PaneScope>,
  );
  fireEvent.click(screen.getByTitle(/read it as markdown/i));
}

beforeEach(() => cleanup());

describe("the markdown preview's width", () => {
  /**
   * The two boxes between the pane and the prose. Both are flex items; both
   * have to be able to shrink, or the wider of their children decides how wide
   * the pane's contents are.
   */
  it("lets the preview shrink to its pane rather than to its widest child", () => {
    show();
    const body = screen.getByTestId("preview-body");
    expect(body.className, "the scrolling body").toContain("min-w-0");

    const root = body.parentElement!;
    expect(root.className, "the preview root").toContain("min-w-0");
  });

  /** A wide table still scrolls — inside the pane, which is the whole point. */
  it("keeps the overflow on the box that scrolls", () => {
    show();
    const body = screen.getByTestId("preview-body");
    expect(body.className).toContain("overflow-auto");
  });

  /** And the prose is there to wrap: the preview renders, table and all. */
  it("renders the document it was blowing out", () => {
    show();
    expect(screen.getByText("Roadmap")).toBeInTheDocument();
    expect(document.querySelector(".prose-mogeung table")).not.toBeNull();
  });
});
