/**
 * Folders added to a session's workspace, in the tree. `R-J40`.
 *
 * Asked for 2026-08-21: a session started in one folder that also works in a
 * sibling repository, with no way to reach the sibling's files. The daemon
 * serves one root and refuses everything outside it — deliberately — so the
 * answer is a list of folders **you** authorised, kept per repository in
 * `~/.mogeung/workspaces.json`.
 *
 * Every test here fails against the tree as it stood: it had one root and no
 * way to be told about another.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilesTool } from "@/ui/tools/FilesTool";
import { useStore, emptyExplorer } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import { ancestors, explorerFetch, openFile } from "@/lib/explorer";
import { base } from "@/lib/format";

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

const session = (id: string, cwd: string) =>
  ({ id, cwd, repo_root: cwd, title: id, alive: true, machine_id: "m" }) as never;

const sent: unknown[] = [];

function setup(opts: { dirs?: string[]; missing?: string[]; expanded?: string[]; listings?: Record<string, unknown[]> } = {}) {
  sent.length = 0;
  useStore.setState({
    prefs: defaultPrefs(),
    selected: "s1",
    sessions: { s1: session("s1", "/repo/own") },
    send: ((m: unknown) => sent.push(m)) as never,
    explorer: {
      s1: {
        ...emptyExplorer(),
        workspace: opts.dirs || opts.missing ? { dirs: opts.dirs ?? [], missing: opts.missing ?? [] } : null,
        expanded: opts.expanded ?? [],
        dirs: { "": [], ...(opts.listings ?? {}) } as never,
      },
    },
  } as never);
}

beforeEach(() => cleanup());

describe("a workspace's added folders", () => {
  it("asks the daemon for the workspace, once", () => {
    setup();
    render(<FilesTool />);
    expect(sent.filter((m) => (m as { cmd: string }).cmd === "fetch_workspace")).toEqual([
      { cmd: "fetch_workspace", session_id: "s1" },
    ]);
  });

  it("draws an added folder as a root of its own, with the path it points at", () => {
    setup({ dirs: ["/repo/sibling"] });
    render(<FilesTool />);

    expect(screen.getByText("sibling")).toBeInTheDocument();
    expect(screen.getByText("/repo/sibling")).toBeInTheDocument();
  });

  /** Its children arrive under absolute paths, which is what lets one string
   *  keep naming one file whichever root it came from. */
  it("shows what is inside one, once it is expanded", () => {
    setup({
      dirs: ["/repo/sibling"],
      expanded: ["/repo/sibling"],
      listings: { "/repo/sibling": [{ name: "Main.java", is_dir: false }] },
    });
    render(<FilesTool />);

    expect(screen.getByText("Main.java")).toBeInTheDocument();
  });

  it("lets go of one, and says which", () => {
    setup({ dirs: ["/repo/sibling"] });
    render(<FilesTool />);

    fireEvent.click(screen.getByLabelText("remove sibling from this workspace"));

    expect(sent).toContainEqual({
      cmd: "remove_workspace_dir",
      session_id: "s1",
      path: "/repo/sibling",
    });
  });

  /**
   * A folder you added and then moved is struck through rather than dropped.
   * You asked for it deliberately, so it vanishing on its own would be the
   * tree lying about what you asked for — `R-J5`'s rule about empty states,
   * applied to a root.
   */
  it("keeps a folder that has gone away on screen, marked missing", () => {
    setup({ dirs: [], missing: ["/repo/moved-away"] });
    render(<FilesTool />);

    expect(screen.getByText("moved-away")).toBeInTheDocument();
    expect(screen.getByText(/missing/)).toBeInTheDocument();
  });
});

/**
 * Go-to-file and search reach the added folders. `R-J40`, second slice.
 *
 * The daemon walks every root on one budget and spells a file from an added
 * folder **absolutely**, which is the same spelling its resolver reads back.
 * The client needed nothing for that — which is the point of having picked one
 * path grammar — so what is pinned here is that opening such a path really
 * does ask for it unchanged, rather than joining a root onto it somewhere.
 */
describe("opening a file that lives in an added folder", () => {
  it("asks the daemon for exactly the path the list gave it", () => {
    setup({ dirs: ["/repo/sibling"] });
    openFile("s1", "/repo/sibling/src/Main.java", { pin: true });
    // The fetch is a render's job, the way the tool does it — one door.
    explorerFetch("s1");

    expect(sent).toContainEqual({
      cmd: "fetch_file",
      session_id: "s1",
      path: "/repo/sibling/src/Main.java",
    });
  });

  /** The tab is named by the file, not by the length of its path. */
  it("names the pane after the file", () => {
    setup({ dirs: ["/repo/sibling"] });
    openFile("s1", "/repo/sibling/src/Main.java", { pin: true });
    const tab = useStore.getState().explorer.s1.open.at(-1);
    expect(tab?.path).toBe("/repo/sibling/src/Main.java");
    expect(base(tab!.path)).toBe("Main.java");
  });
});

/**
 * The tree expands a file's ancestors to reveal it, and an absolute path's
 * ancestors are absolute. Without this, revealing `/repo/sibling/src/Main.java`
 * would ask for `repo` and `repo/sibling` — two directories that exist nowhere,
 * left in `expanded` for ever.
 */
describe("the ancestors of a path in an added folder", () => {
  it("keeps the leading slash", () => {
    expect(ancestors("/repo/sibling/src/Main.java")).toEqual([
      "/repo",
      "/repo/sibling",
      "/repo/sibling/src",
    ]);
  });

  it("leaves a session-relative path exactly as it was", () => {
    expect(ancestors("src/ui/Thing.tsx")).toEqual(["src", "src/ui"]);
  });
});
