/**
 * The list of scratch files, and the one thing it must not become. `R-L6`.
 *
 * Asked 2026-09-08: *"I think we need to panel to show the scratch files. The
 * current files panel didn't show it."* Files is right not to — it browses the
 * session's worktree, and a scratch file is in nobody's worktree.
 *
 * **These pinned absences until 2026-09-09**, when the next ask reversed the
 * line: *"enhance the scratch path panel with right-click menu to support all
 * file related operations."* So `offers no delete and no rename` is gone rather
 * than amended — a test asserting the opposite of the current requirement is
 * not a regression test, it is a stale one, and leaving it skipped would have
 * left the argument looking live.
 *
 * What replaces it is the fence that *did* survive: the daemon still checks
 * every name, delete still asks first, and rename is the one verb where the
 * window proposes a name. See `R-L7` and ADR-0035's 2026-09-09 amendment.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useStore } from "@/store";
import { ScratchTool } from "@/ui/tools/ScratchTool";

const sent: unknown[] = [];
const opened: string[] = [];

vi.mock("@/lib/scratch", async (orig) => ({
  ...(await orig<typeof import("@/lib/scratch")>()),
  openScratch: (name: string) => opened.push(name),
}));

beforeEach(() => {
  cleanup();
  sent.length = 0;
  opened.length = 0;
  useStore.setState({
    scratch: { names: [], folders: [], open: {} } as never,
    activePane: null,
    paletteOpen: false,
    send: ((m: unknown) => sent.push(m)) as never,
  });
});

describe("the scratch files panel", () => {
  /**
   * A file can be made — or `rm`'d — while the rail is shut, and a stale list
   * offers you a file that is not there.
   */
  it("asks for the list when it opens", () => {
    render(<ScratchTool />);
    expect(sent).toContainEqual({ cmd: "scratch_list" });
  });

  it("says there are none, and how to make one", () => {
    render(<ScratchTool />);
    expect(screen.getByText(/no scratch files/i)).toBeInTheDocument();
    expect(screen.getByText(/Ctrl\+Alt\+Shift\+Insert/)).toBeInTheDocument();
  });

  it("lists what there is, in the order the daemon gave", () => {
    useStore.setState({ scratch: { names: ["scratch-2.sql", "scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    const rows = screen.getAllByText(/^scratch-\d+\./).map((el) => el.textContent);
    expect(rows).toEqual(["scratch-2.sql", "scratch-1.java"]);
  });

  it("opens one when it is clicked", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.click(screen.getByText("scratch-1.java"));

    expect(opened).toEqual(["scratch-1.java"]);
  });

  it("names the language beside the file", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);
    expect(screen.getByText("java")).toBeInTheDocument();
  });

  /**
   * It asks the picker rather than creating anything: the daemon mints every
   * name (ADR-0035), and a second way to ask would be a second place to get
   * that wrong.
   */
  it("hands a new file to the picker instead of minting a name", () => {
    render(<ScratchTool />);

    fireEvent.click(screen.getByText(/new scratch file/i));

    expect(useStore.getState().paletteOpen).toBe(true);
    expect(useStore.getState().paletteMode).toBe("scratch");
    expect(sent).not.toContainEqual(expect.objectContaining({ cmd: "scratch_create" }));
  });

  // -- The right-click menu. `R-L7`.

  it("offers the file operations on a right-click", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));

    for (const label of [/^Open$/, /^Rename…$/, /^Duplicate$/, /Copy full path/, /Copy file name/, /^Delete…$/]) {
      expect(await screen.findByText(label)).toBeInTheDocument();
    }
  });

  it("duplicates through the daemon rather than minting a name", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Duplicate"));

    expect(sent).toContainEqual({ cmd: "scratch_duplicate", name: "scratch-1.java" });
  });

  /**
   * The one irreversible thing in this panel. Before `R-L7` the delete was
   * `rm`, which at least makes you type the name.
   */
  it("asks before deleting, and does not send until you say so", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Delete…"));

    expect(sent).not.toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });

    fireEvent.click(screen.getByText("delete"));
    expect(sent).toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });
  });

  it("keeps the file when the confirmation is declined", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Delete…"));
    fireEvent.click(screen.getByText("keep"));

    expect(sent).not.toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });
    expect(screen.getByText("scratch-1.java")).toBeInTheDocument();
  });

  it("renames in place, on Enter", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Rename…"));

    const box = screen.getByLabelText("new name");
    fireEvent.change(box, { target: { value: "Gateway.java" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(sent).toContainEqual({
      cmd: "scratch_rename",
      name: "scratch-1.java",
      to: "Gateway.java",
    });
  });

  it("sends nothing when the name is unchanged", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Rename…"));
    fireEvent.keyDown(screen.getByLabelText("new name"), { key: "Enter" });

    expect(sent.some((m) => (m as { cmd?: string }).cmd === "scratch_rename")).toBe(false);
  });

  it("abandons a rename on Escape", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Rename…"));

    const box = screen.getByLabelText("new name");
    fireEvent.change(box, { target: { value: "other.java" } });
    fireEvent.keyDown(box, { key: "Escape" });

    expect(sent.some((m) => (m as { cmd?: string }).cmd === "scratch_rename")).toBe(false);
    expect(screen.getByText("scratch-1.java")).toBeInTheDocument();
  });

  /**
   * Another window deletes the file — or `rm` does — while a rename box is
   * open over it. An editor for a file that is not there is worse than a list
   * that changed under you.
   */
  it("closes the rename box when the file goes away", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    const { rerender } = render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Rename…"));
    expect(screen.getByLabelText("new name")).toBeInTheDocument();

    act(() => {
      useStore.setState({ scratch: { names: [], folders: [], open: {} } as never });
    });
    rerender(<ScratchTool />);

    expect(screen.queryByLabelText("new name")).not.toBeInTheDocument();
  });
});

describe("driving the panel from the keyboard", () => {
  /**
   * `R-L8`, asked 2026-09-09. The claim in `data-owns-keys` is the load-bearing
   * part: window-wide `F2` is *Label the selected session*, and `focusOwns`
   * hands bare keys to whatever has focus — a `div` is not a text box, so
   * without the claim this panel's `F2` would open the session label dialog.
   * The keymap listens in **capture**, so the panel cannot win by stopping
   * propagation; it has to be granted the key.
   */
  it("claims F2 and Delete from the window", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    const { container } = render(<ScratchTool />);

    const claim = container.querySelector("[data-owns-keys]");
    expect(claim?.getAttribute("data-owns-keys")).toBe("F2 Delete");
  });

  it("renames the selected file on F2", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java", "b.sql"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    const row = screen.getByText("scratch-1.java");
    fireEvent.focus(row.closest("[role=option]")!);
    fireEvent.keyDown(row.closest("[role=option]")!, { key: "F2" });

    expect(screen.getByLabelText("new name")).toHaveValue("scratch-1.java");
  });

  it("asks before deleting on Delete, and sends only on confirm", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    const row = screen.getByText("scratch-1.java").closest("[role=option]")!;
    fireEvent.focus(row);
    fireEvent.keyDown(row, { key: "Delete" });

    expect(sent).not.toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });

    fireEvent.click(screen.getByText("delete"));
    expect(sent).toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });
  });

  /**
   * The `j` lesson, one key over: the rename box lives inside the container
   * that handles these keys, so a `Delete` pressed while editing text would
   * otherwise delete the file you are renaming.
   */
  it("does not treat Delete inside the rename box as a delete", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    const row = screen.getByText("scratch-1.java").closest("[role=option]")!;
    fireEvent.focus(row);
    fireEvent.keyDown(row, { key: "F2" });

    const box = screen.getByLabelText("new name");
    fireEvent.keyDown(box, { key: "Delete" });

    expect(screen.queryByText("delete")).not.toBeInTheDocument();
    expect(sent.some((m) => (m as { cmd?: string }).cmd === "scratch_delete")).toBe(false);
  });

  /** Nor F2, which would stack a second rename on the one being typed. */
  it("does not restart a rename from inside the rename box", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    const row = screen.getByText("scratch-1.java").closest("[role=option]")!;
    fireEvent.focus(row);
    fireEvent.keyDown(row, { key: "F2" });

    const box = screen.getByLabelText("new name");
    fireEvent.change(box, { target: { value: "half-typed" } });
    fireEvent.keyDown(box, { key: "F2" });

    expect(screen.getByLabelText("new name")).toHaveValue("half-typed");
  });

  it("moves the selection with the arrows", () => {
    useStore.setState({ scratch: { names: ["a.java", "b.sql"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    const first = screen.getByText("a.java").closest("[role=option]")!;
    fireEvent.focus(first);
    fireEvent.keyDown(first, { key: "ArrowDown" });
    fireEvent.keyDown(screen.getByText("b.sql").closest("[role=option]")!, { key: "F2" });

    expect(screen.getByLabelText("new name")).toHaveValue("b.sql");
  });

  /** Arrowing past a file must not open it — that would fill the dock. */
  it("does not open a file the selection merely passes over", () => {
    useStore.setState({ scratch: { names: ["a.java", "b.sql"], folders: [], open: {} } as never });
    render(<ScratchTool />);

    const first = screen.getByText("a.java").closest("[role=option]")!;
    fireEvent.focus(first);
    fireEvent.keyDown(first, { key: "ArrowDown" });

    expect(opened).toEqual([]);
  });
});

describe("folders", () => {
  /** `R-L9`, asked 2026-09-09: add a folder, remove one, move files between. */
  const withTree = () =>
    useStore.setState({
      scratch: { names: ["root.txt", "sql/query.sql"], folders: ["sql"], open: {} } as never,
    });

  it("draws the tree with folders above files", () => {
    withTree();
    render(<ScratchTool />);

    // The row shows the leaf, not the whole path — the folder above it says
    // where it is.
    expect(screen.getByLabelText("folder sql")).toBeInTheDocument();
    expect(screen.getByText("query.sql")).toBeInTheDocument();
    expect(screen.getByText("root.txt")).toBeInTheDocument();
  });

  it("makes a folder at the top level", () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.click(screen.getByText(/new folder/i));
    const box = screen.getByLabelText("new name");
    fireEvent.change(box, { target: { value: "notes" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(sent).toContainEqual({ cmd: "scratch_mkdir", path: "notes" });
  });

  it("makes a folder inside another", async () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByLabelText("folder sql"));
    fireEvent.click(await screen.findByText("New folder here…"));
    const box = screen.getByLabelText("new name");
    fireEvent.change(box, { target: { value: "reports" } });
    fireEvent.keyDown(box, { key: "Enter" });

    expect(sent).toContainEqual({ cmd: "scratch_mkdir", path: "sql/reports" });
  });

  /** The most destructive verb in the window, so it says what goes with it. */
  it("asks before removing a folder, and names how many files go too", async () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByLabelText("folder sql"));
    fireEvent.click(await screen.findByText("Delete folder…"));

    expect(screen.getByText(/and 1 file/)).toBeInTheDocument();
    expect(sent).not.toContainEqual({ cmd: "scratch_rmdir", path: "sql" });

    fireEvent.click(screen.getByText("delete"));
    expect(sent).toContainEqual({ cmd: "scratch_rmdir", path: "sql" });
  });

  /**
   * A move **is** a rename, which is why there is no move verb: the daemon's
   * rename takes a path, so another folder is another path.
   */
  it("moves a file into a folder by renaming it", async () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("root.txt"));
    fireEvent.click(await screen.findByText("Move to sql"));

    expect(sent).toContainEqual({
      cmd: "scratch_rename",
      name: "root.txt",
      to: "sql/root.txt",
    });
  });

  it("moves a file back to the top level", async () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("query.sql"));
    fireEvent.click(await screen.findByText("Move to the top level"));

    expect(sent).toContainEqual({
      cmd: "scratch_rename",
      name: "sql/query.sql",
      to: "query.sql",
    });
  });

  /** Offering to move a file where it already is would be a no-op menu item. */
  it("does not offer the folder the file is already in", async () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("query.sql"));
    await screen.findByText("Open");

    expect(screen.queryByText("Move to sql")).not.toBeInTheDocument();
  });

  it("collapses a folder, hiding what is in it", () => {
    withTree();
    render(<ScratchTool />);
    expect(screen.getByText("query.sql")).toBeInTheDocument();

    fireEvent.click(screen.getByLabelText("folder sql"));

    expect(screen.queryByText("query.sql")).not.toBeInTheDocument();
    expect(screen.getByLabelText("folder sql")).toBeInTheDocument();
  });

  /**
   * A folder rename would be a move of everything under it, and the daemon has
   * no single verb for that — so the panel does not offer a gesture it cannot
   * honour.
   */
  it("does not offer to rename a folder", async () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByLabelText("folder sql"));
    await screen.findByText("Delete folder…");

    expect(screen.queryByText("Rename…")).not.toBeInTheDocument();
  });

  it("makes a new file in the folder you asked from", async () => {
    withTree();
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByLabelText("folder sql"));
    fireEvent.click(await screen.findByText("New scratch file here…"));

    expect(useStore.getState().paletteMode).toBe("scratch");
    expect(useStore.getState().scratchFolder).toBe("sql");
  });
});
