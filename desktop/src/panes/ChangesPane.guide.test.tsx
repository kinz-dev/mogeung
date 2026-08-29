/**
 * The reading guide, in the pane. `R-O3`.
 *
 * Two properties carry this feature and both come from `--bin judge`'s corpus
 * run rather than from taste:
 *
 * - **Nothing is hidden.** `claude-opus-5` ranked sixteen files of sixty and
 *   said nothing about the other 44. A pane that rendered only the model's
 *   list would hide them.
 * - **It is a second ordering, never a blend.** With the guide off, or with no
 *   model, this pane is exactly what it was — which is [pillar K]'s rule about
 *   risk scoring applied one surface over.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { Change, ClientMsg, FileChange } from "@/wire/types";
import { ChangesPane } from "@/panes/ChangesPane";

// jsdom has no layout, and the diff rows do not need one for this.
vi.mock("@/ui/Mermaid", () => ({ Mermaid: () => null }));

const file = (path: string, score: number): FileChange =>
  ({
    path,
    old_path: null,
    status: "modified",
    insertions: 1,
    deletions: 0,
    hunks: [],
    flags: [],
    score,
    truncated: false,
  }) as unknown as FileChange;

/** Risk order, as the daemon sends it: highest score first. */
const change: Change = {
  insertions: 3,
  deletions: 0,
  files: [file("docs/notes.md", 90), file("src/core.rs", 50), file("src/tail.rs", 10)],
  error: null,
} as unknown as Change;

const sent: ClientMsg[] = [];

const paths = () =>
  screen.getAllByText(/^(docs|src)\//).map((n) => n.textContent);

beforeEach(() => {
  sent.length = 0;
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    changes: { s1: change },
    guides: {},
    send: (m: ClientMsg) => void sent.push(m),
  } as never);
});

describe("the reading guide in the Changes pane", () => {
  it("shows risk order until it is asked for", () => {
    render(<ChangesPane />);
    expect(paths()).toEqual(["docs/notes.md", "src/core.rs", "src/tail.rs"]);
    expect(sent).toEqual([]);
  });

  /**
   * Asked for, never automatic. It spends a model call of up to a minute, so a
   * pane that ordered on selection would quietly spend somebody's plan every
   * time they clicked a session.
   */
  it("asks only when the button is pressed", () => {
    render(<ChangesPane />);
    fireEvent.click(screen.getByTitle(/which file to read first/));
    expect(sent).toContainEqual({ cmd: "reading_guide", session_id: "s1" });
  });

  /** The property the corpus bought: unranked files stay on the screen. */
  it("puts the model's order first and keeps everything else", () => {
    useStore.setState({
      guides: {
        s1: {
          // The model named two of three, and not the two risk ranked highest.
          files: [
            { path: "src/core.rs", reason: "carries the change", ranked: true },
            { path: "src/tail.rs", reason: "follows from it", ranked: true },
            { path: "docs/notes.md", reason: "", ranked: false },
          ],
          summary: "the core moved; the doc follows.",
          model: "qwen",
          elapsed_ms: 2000,
          error: null,
          pending: false,
        },
      },
    } as never);
    render(<ChangesPane />);
    fireEvent.click(screen.getByTitle(/which file to read first/));

    expect(paths()).toEqual(["src/core.rs", "src/tail.rs", "docs/notes.md"]);
    expect(screen.getByText(/carries the change/)).toBeInTheDocument();
    expect(screen.getByText("unranked")).toBeInTheDocument();
    expect(screen.getByText(/the core moved/)).toBeInTheDocument();
    // The counts are stated, so a reader can tell how much the model looked at.
    expect(screen.getByText(/2 of 3 file\(s\) ordered by qwen/)).toBeInTheDocument();
  });

  /** Switching back is the keyword order, untouched. Never a blend. */
  it("returns to risk order when switched off", () => {
    useStore.setState({
      guides: {
        s1: {
          files: [{ path: "src/tail.rs", reason: "first", ranked: true }],
          summary: "",
          model: "m",
          elapsed_ms: 1,
          error: null,
          pending: false,
        },
      },
    } as never);
    render(<ChangesPane />);
    const button = screen.getByTitle(/which file to read first/);
    fireEvent.click(button);
    expect(paths()[0]).toBe("src/tail.rs");

    fireEvent.click(screen.getByTitle(/back to risk order/));
    expect(paths()).toEqual(["docs/notes.md", "src/core.rs", "src/tail.rs"]);
    expect(screen.queryByText("unranked")).not.toBeInTheDocument();
  });

  /**
   * A failure says so, rather than leaving an empty pane or a switch that did
   * nothing. The sequence is the real one: pressing the button asks, the ask
   * is pending, and the daemon's refusal arrives afterwards.
   */
  it("shows why it could not order the diff, and keeps the files", () => {
    render(<ChangesPane />);
    fireEvent.click(screen.getByTitle(/which file to read first/));
    expect(screen.getByText(/reading the diff/)).toBeInTheDocument();

    act(() =>
      useStore.getState().ingest({
        ev: "reading_guide_ready",
        session_id: "s1",
        files: [],
        summary: "",
        model: "",
        elapsed_ms: 0,
        error: "no model configured",
      }),
    );

    expect(screen.getByText(/no model configured/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /try again/ })).toBeInTheDocument();
    // The diff is untouched underneath: a guide that failed must not cost you
    // the files you already had.
    expect(paths()).toEqual(["docs/notes.md", "src/core.rs", "src/tail.rs"]);
  });
});

/**
 * The crash this pane shipped with, on 2026-08-29. `R-O3`.
 *
 * The guide's two `useMemo`s were written **below** the pane's early returns,
 * so with no session selected it called three hooks and with one selected it
 * called five. React's rule is that every hook runs on every path; break it
 * and it throws *rendered more hooks than during the previous render*, unmounts
 * the tree, and the pane is blank — permanently, and across restarts, because
 * it is a code bug rather than state.
 *
 * Reported as *"I clicked on one of the sessions and now every time I click on
 * it, it just gives me a blank screen"*, which is this sequence exactly.
 */
describe("selecting a session after none", () => {
  it("does not change how many hooks the pane runs", () => {
    const err = vi.spyOn(console, "error").mockImplementation(() => {});

    // Nothing selected: the pane takes its earliest return.
    useStore.setState({ selected: null, changes: {} } as never);
    const { rerender } = render(<ChangesPane />);
    expect(screen.getByText(/select a session/)).toBeInTheDocument();

    // Now select one. Before the fix this threw and left nothing rendered.
    useStore.setState({ selected: "s1", changes: { s1: change } } as never);
    rerender(<ChangesPane />);

    expect(paths()).toEqual(["docs/notes.md", "src/core.rs", "src/tail.rs"]);
    expect(
      err.mock.calls.flat().join(" "),
    ).not.toMatch(/more hooks|Rendered fewer hooks/);
    err.mockRestore();
  });

  /** And the other order: a session, then none, then one again. */
  it("survives losing the selection and getting it back", () => {
    useStore.setState({ selected: "s1", changes: { s1: change } } as never);
    const { rerender } = render(<ChangesPane />);
    expect(paths()).toHaveLength(3);

    useStore.setState({ selected: null } as never);
    rerender(<ChangesPane />);
    expect(screen.getByText(/select a session/)).toBeInTheDocument();

    useStore.setState({ selected: "s1" } as never);
    rerender(<ChangesPane />);
    expect(paths()).toHaveLength(3);
  });
});
