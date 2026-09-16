/**
 * Prose wraps; code does not.
 *
 * Reported 2026-09-16: *"there is a problem when viewing a md file in a
 * markdown model, if the line is too long it doesn't automatically wrap"*. It
 * did not — `wordWrap` was `off` for every file in the pane, and the header's
 * toggle was the only way to turn it on, per file, for ever.
 *
 * These assert the option Monaco is actually handed, which is the thing that
 * was wrong: the mock below exists to put `wordWrap` where a test can read it.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { wrapsByDefault } from "@/lib/explorer";

vi.mock("@monaco-editor/react", () => {
  const Editor = ({ value, options }: { value: string; options: { wordWrap?: string } }) => (
    <div data-testid="source" data-wrap={options?.wordWrap}>
      {value}
    </div>
  );
  return { __esModule: true, default: Editor, Editor };
});

const LONG = `# a heading\n\n${"a very long paragraph ".repeat(40)}`;

function show(path: string) {
  cleanup();
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    explorer: {
      s1: {
        ...emptyExplorer(),
        open: [{ path, rev: null, pinned: true, content: LONG, truncated: false, gotoLine: null }] as never,
      },
    },
  });
  render(
    <PaneScope id={filePaneId("s1", path, null)}>
      <FilePane />
    </PaneScope>,
  );
}

const wrapNow = () => screen.getByTestId("source").getAttribute("data-wrap");
const toggle = () => fireEvent.click(screen.getByRole("button", { name: /wrap/i }));

beforeEach(() => cleanup());

describe("which files wrap on their own", () => {
  /** The reported bug, as one assertion. */
  it("wraps a markdown file without being asked", () => {
    show("docs/notes.md");
    expect(wrapNow()).toBe("on");
  });

  it("wraps the other prose extensions too", () => {
    for (const path of ["README.markdown", "notes.txt", "a/b/thing.TXT"]) {
      show(path);
      expect(wrapNow(), path).toBe("on");
    }
  });

  /**
   * The other half, and the reason this is not `wordWrap: "on"` everywhere:
   * indentation is structure, and a wrapped line of Rust puts its
   * continuation where a nested block would be.
   */
  it("leaves code running off the edge, as it always did", () => {
    for (const path of ["src/main.rs", "a.ts", "data.json", "rows.csv", "server.log"]) {
      show(path);
      expect(wrapNow(), path).toBe("off");
    }
  });

  it("answers the same question outside a component", () => {
    expect(wrapsByDefault("a.md")).toBe(true);
    expect(wrapsByDefault("a.rs")).toBe(false);
    // No extension at all is not prose: a `Makefile` is indentation-sensitive
    // in the strongest sense there is.
    expect(wrapsByDefault("Makefile")).toBe(false);
  });
});

describe("the button, which overrides in both directions", () => {
  it("turns wrap off for a markdown file, and remembers", () => {
    show("notes.md");
    expect(wrapNow()).toBe("on");
    toggle();
    expect(wrapNow()).toBe("off");
    expect(useStore.getState().scoped().editorNoWrap).toEqual(["notes.md"]);
    expect(useStore.getState().scoped().editorWrap).toEqual([]);
  });

  it("turns wrap on for a code file, and remembers", () => {
    show("main.rs");
    expect(wrapNow()).toBe("off");
    toggle();
    expect(wrapNow()).toBe("on");
    expect(useStore.getState().scoped().editorWrap).toEqual(["main.rs"]);
    expect(useStore.getState().scoped().editorNoWrap).toEqual([]);
  });

  /** A path may never sit in both lists, whichever way it was toggled. */
  it("keeps a path out of the list it does not belong in", () => {
    show("notes.md");
    toggle();
    toggle();
    const scoped = useStore.getState().scoped();
    expect(scoped.editorWrap).toEqual(["notes.md"]);
    expect(scoped.editorNoWrap).toEqual([]);
    expect(wrapNow()).toBe("on");
  });

  /** A preferences file written before the default still means what it said. */
  it("honours a wrap you turned on before prose wrapped on its own", () => {
    show("main.rs");
    act(() => useStore.getState().setScoped({ editorWrap: ["main.rs"] }));
    expect(wrapNow()).toBe("on");
  });
});
