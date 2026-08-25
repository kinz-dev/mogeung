/**
 * Finding in the rendering, and getting back to the source. `R-B38`.
 *
 * The preview shipped with `R-B29` and `Ctrl+F` went to Monaco's find, on the
 * buffer behind it — so the words actually on the screen were the one thing
 * that could not be searched. These mount the real pane, because the part that
 * was missing is the wiring rather than the matching, and `markdown.test.ts`
 * already owns the matching.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";

vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  // Monaco needs a real canvas and measures in device pixels; jsdom has
  // neither. The source view is a textarea's worth of stand-in here — what is
  // under test is which view is showing and what it was told to reveal.
  default: ({ value }: { value: string }) => <div data-testid="source">{value}</div>,
  Editor: ({ value }: { value: string }) => <div data-testid="source">{value}</div>,
}));

const DOC = [
  "# Release notes", // 1
  "", // 2
  "the **egress** path changed", // 3
  "", // 4
  "| Env | Shape |", // 5
  "|---|---|", // 6
  "| prod | wide |", // 7
].join("\n");

function openMarkdown(body = DOC) {
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    explorer: {
      s1: {
        ...emptyExplorer(),
        open: [{ path: "notes.md", rev: null, pinned: true, content: body, truncated: false, gotoLine: null }] as never,
      },
    },
  });
}

/** One file, in its own pane — `R-B53`'s shape. */
function renderPane() {
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  return render(
    <PaneScope id={filePaneId("s1", "notes.md", null)}>
      <FilePane />
    </PaneScope>,
  );
}

const openFind = () => {
  // The preview holds focus so the chord can be scoped to it.
  const preview = screen.getByText("Release notes").closest("[tabindex]") as HTMLElement;
  preview.focus();
  fireEvent.keyDown(window, { key: "f", ctrlKey: true });
};

beforeEach(() => {
  cleanup();
  openMarkdown();
});

describe("finding in the rendered preview", () => {
  it("opens on Ctrl+F from inside the preview", async () => {
    renderPane();
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    await act(async () => openFind());
    expect(screen.getByLabelText("find in the preview")).toBeInTheDocument();
  });

  it("matches words the source spells with syntax around them", async () => {
    renderPane();
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    await act(async () => openFind());
    fireEvent.change(screen.getByLabelText("find in the preview"), { target: { value: "egress" } });

    expect(screen.getByText("1/1")).toBeInTheDocument();
    // ...and offers the source line, which is what a hit is for.
    expect(screen.getByText("line 3")).toBeInTheDocument();
  });

  /** A phrase only exists once the cell walls are gone. */
  it("matches across a table row the source splits with pipes", async () => {
    renderPane();
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    await act(async () => openFind());
    fireEvent.change(screen.getByLabelText("find in the preview"), { target: { value: "prod wide" } });
    expect(screen.getByText("line 7")).toBeInTheDocument();
  });

  it("says nothing matched rather than pointing at a line", async () => {
    renderPane();
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    await act(async () => openFind());
    fireEvent.change(screen.getByLabelText("find in the preview"), { target: { value: "zzz" } });
    expect(screen.getByText("0/0")).toBeInTheDocument();
    expect(screen.queryByText(/^line /)).toBeNull();
  });

  /**
   * The whole payoff. Clicking the line leaves the rendering and asks the
   * editor to reveal it — through `gotoLine`, the channel a search hit and a
   * diff row already use.
   */
  it("leaves the preview and points the editor at the line", async () => {
    renderPane();
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    await act(async () => openFind());
    fireEvent.change(screen.getByLabelText("find in the preview"), { target: { value: "egress" } });

    await act(async () => {
      fireEvent.click(screen.getByText("line 3"));
    });

    expect(screen.getByTestId("source")).toBeInTheDocument();
    expect(useStore.getState().explorer.s1.open[0].gotoLine).toBe(3);
  });

  /**
   * In source mode the chord belongs to Monaco's own find widget. A window-wide
   * binding would have had to take it back from the editor to give it to a
   * preview that is usually off.
   */
  it("does not open the preview find while the source is showing", async () => {
    renderPane();
    fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    expect(screen.queryByLabelText("find in the preview")).toBeNull();
  });
});

/**
 * The wrap button, in the preview. `R-J52`.
 *
 * Reported 2026-08-25: pressing it while reading a `.md` did nothing. It fed
 * `wordWrap` to Monaco, which is not what is on screen in preview mode — and
 * prose wraps anyway, so the only thing left visibly refusing to wrap was a
 * fenced block. These pin the join rather than the CSS: that the same per-file
 * state reaches both renderings, and that it survives the round trip to
 * source and back.
 */
describe("wrapping long lines in the preview", () => {
  const body = ["# Doc", "", "```", "x".repeat(400), "```"].join("\n");
  const prose = () => document.querySelector(".prose-mogeung") as HTMLElement;

  const openPreview = () => {
    openMarkdown(body);
    renderPane();
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
  };

  it("does not wrap until asked — code scrolls sideways by default", () => {
    openPreview();
    expect(prose().className).not.toContain("wrap-code");
  });

  it("wraps the rendering when the header's wrap is on", () => {
    openPreview();
    fireEvent.click(screen.getByTitle(/^wrap long lines/));
    expect(prose().className).toContain("wrap-code");
  });

  /** One control, two renderings: the state is the editor's own, per file. */
  it("is the same setting the source view uses", () => {
    openPreview();
    fireEvent.click(screen.getByTitle(/^wrap long lines/));
    expect(useStore.getState().scoped().editorWrap).toEqual(["notes.md"]);

    // Back to source and into the preview again — still wrapped.
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    fireEvent.click(screen.getByTitle(/read it as markdown/i));
    expect(prose().className).toContain("wrap-code");
  });
});
