/**
 * The file pane fills its pane. Reported 2026-08-07: *"the editor now opens but
 * the content is not fully expanded to the size of the pane, I can barely see
 * the content"*.
 *
 * The cause was a root asking for `flex-1` inside `ZoomPane`, which is a
 * **block** — so the height was never distributed, the root sized to its
 * content, and the editor under it collapsed to a sliver beneath its own
 * toolbar. It worked before `R-B53` because this markup was a child of the Code
 * pane's flex column; lifting it to be the pane root took the flex context away.
 *
 * **What this test can and cannot do.** jsdom performs no layout and no
 * Tailwind, so nothing here can measure a height — asserting `clientHeight`
 * would pass against the bug. What it pins instead is the contract the bug
 * broke: mounted in the real wrapper, the pane's root must size itself to that
 * wrapper rather than to its content. That is a weaker instrument than a
 * screenshot, and it is the one that fails today and would have failed then.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { ZoomPane } from "@/ui/ZoomPane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";

vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  default: ({ value }: { value: string }) => <div data-testid="source">{value}</div>,
  Editor: ({ value }: { value: string }) => <div data-testid="source">{value}</div>,
}));

beforeEach(() => {
  cleanup();
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    explorer: {
      s1: {
        ...emptyExplorer(),
        open: [
          { path: "main.rs", rev: null, pinned: true, content: "fn main() {}", truncated: false, gotoLine: null },
        ] as never,
      },
    },
  });
});

/** Exactly how `App` mounts it: `PaneScope` outside, `ZoomPane` between. */
function mounted(): HTMLElement {
  render(
    <PaneScope id={filePaneId("s1", "main.rs", null)}>
      <ZoomPane name="file" scale={false}>
        <FilePane />
      </ZoomPane>
    </PaneScope>,
  );
  return screen.getByTestId("source").closest("div.flex.flex-col") as HTMLElement;
}

describe("the file pane's height", () => {
  it("takes it from the wrapper rather than from its own content", () => {
    expect(mounted().className).toContain("h-full");
  });

  /**
   * The specific mistake, named. `ZoomPane` is a block, so `flex-1` here is not
   * merely redundant — it is inert, and it is what made the editor a sliver.
   */
  it("does not ask a non-flex wrapper for a share of the height", () => {
    const root = mounted();
    expect(root.parentElement?.className).not.toContain("flex-col");
    expect(root.className.split(/\s+/)).not.toContain("flex-1");
  });
});
