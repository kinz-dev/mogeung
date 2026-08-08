/**
 * A ```mermaid fence, drawn. `R-J22`.
 *
 * mermaid measures real text in a real layout engine, and jsdom has neither —
 * so the library is a stand-in here and what is under test is everything
 * around it: that a fence is routed to the renderer at all, that an ordinary
 * code block is left alone, that a diagram which does not parse gives back its
 * source instead of taking the pane down, and that the sanitiser is switched
 * on. The last one is the reason this file exists rather than being folded
 * into `FilePane.preview.test.tsx`: `securityLevel` is a security property of
 * agent-authored input and it should fail loudly if a future default moves it.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { __clearMermaidCache } from "@/ui/Mermaid";

const initialize = vi.fn();
const render_ = vi.fn(async (id: string, chart: string) => {
  if (chart.includes("!!broken")) throw new Error("Parse error on line 2:\nexpected a node");
  return { svg: `<svg data-id="${id}"><title>${chart.split("\n")[0]}</title></svg>` };
});

vi.mock("mermaid", () => ({ default: { initialize, render: render_ } }));

vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  default: ({ value }: { value: string }) => <div data-testid="source">{value}</div>,
  Editor: ({ value }: { value: string }) => <div data-testid="source">{value}</div>,
}));

const DIAGRAM = ["# Architecture", "", "```mermaid", "graph TD", "  A-->B", "```"].join("\n");

function open(body: string) {
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    explorer: {
      s1: {
        ...emptyExplorer(),
        open: [
          { path: "notes.md", rev: null, pinned: true, content: body, truncated: false, gotoLine: null },
        ] as never,
      },
    },
  });
}

async function showPreview() {
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  render(
    <PaneScope id={filePaneId("s1", "notes.md", null)}>
      <FilePane />
    </PaneScope>,
  );
  fireEvent.click(screen.getByTitle(/read it as markdown/i));
  // The import, the parse and the timer the parse sits behind (`R-J23`) —
  // so each pass has to yield a macrotask, not just flush microtasks.
  for (let i = 0; i < 3; i++) {
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
  }
}

beforeEach(() => {
  cleanup();
  // `R-J23` caches rendered SVG across mounts, which is module state — a case
  // that inherited it would pass without ever calling the renderer, which is a
  // test passing for the wrong reason.
  __clearMermaidCache();
  initialize.mockClear();
  render_.mockClear();
});

describe("mermaid in the file pane", () => {
  it("draws a mermaid fence instead of printing it", async () => {
    open(DIAGRAM);
    await showPreview();

    expect(screen.getByTestId("mermaid")).toBeInTheDocument();
    expect(render_).toHaveBeenCalledWith(expect.stringMatching(/^mermaid-\d+$/), "graph TD\n  A-->B");
  });

  /**
   * **The sanitiser is pinned, not defaulted.** These diagrams are agent
   * output read out of the user's repositories; `strict` is what refuses
   * inline HTML and click handlers in the SVG this pane then injects.
   */
  it("renders with the sanitiser on", async () => {
    open(DIAGRAM);
    await showPreview();
    expect(initialize).toHaveBeenCalledWith(expect.objectContaining({ securityLevel: "strict" }));
  });

  /** Dark unless the desktop says otherwise — the same rule Monaco follows. */
  it("follows the app's theme", async () => {
    open(DIAGRAM);
    useStore.getState().setPrefs({ theme: "light" });
    await showPreview();
    expect(initialize).toHaveBeenCalledWith(expect.objectContaining({ theme: "default" }));
  });

  /**
   * **Degrade, never panic.** A half-typed diagram in a file you are reading
   * must not cost you the pane — you get the error and the source, which is
   * also what you need in order to fix it.
   */
  it("gives back the source when a diagram does not parse", async () => {
    open(["```mermaid", "graph TD", "!!broken", "```"].join("\n"));
    await showPreview();

    expect(screen.queryByTestId("mermaid")).toBeNull();
    expect(screen.getByText(/did not parse/i)).toBeInTheDocument();
    // The source, verbatim, so the fix is in front of you.
    expect(screen.getByText(/!!broken/)).toBeInTheDocument();
  });

  /** Every other fence is the code block it always was. */
  it("leaves an ordinary code block alone", async () => {
    open(["```ts", "const a = 1;", "```"].join("\n"));
    await showPreview();

    expect(screen.queryByTestId("mermaid")).toBeNull();
    expect(render_).not.toHaveBeenCalled();
    expect(screen.getByText(/const a = 1;/)).toBeInTheDocument();
  });

  /** An inline `mermaid` is a word, not a diagram. */
  it("does not mistake an inline span for a fence", async () => {
    open("we use `mermaid` for diagrams");
    await showPreview();
    expect(screen.queryByTestId("mermaid")).toBeNull();
    expect(render_).not.toHaveBeenCalled();
  });
});
