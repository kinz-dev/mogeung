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
    scratch: { names: [], open: {} } as never,
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
    useStore.setState({ scratch: { names: ["scratch-2.sql", "scratch-1.java"], open: {} } as never });
    render(<ScratchTool />);

    const rows = screen.getAllByText(/^scratch-\d+\./).map((el) => el.textContent);
    expect(rows).toEqual(["scratch-2.sql", "scratch-1.java"]);
  });

  it("opens one when it is clicked", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.click(screen.getByText("scratch-1.java"));

    expect(opened).toEqual(["scratch-1.java"]);
  });

  it("names the language beside the file", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
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
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));

    for (const label of [/^Open$/, /^Rename…$/, /^Duplicate$/, /Copy full path/, /Copy file name/, /^Delete…$/]) {
      expect(await screen.findByText(label)).toBeInTheDocument();
    }
  });

  it("duplicates through the daemon rather than minting a name", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
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
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Delete…"));

    expect(sent).not.toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });

    fireEvent.click(screen.getByText("delete"));
    expect(sent).toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });
  });

  it("keeps the file when the confirmation is declined", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Delete…"));
    fireEvent.click(screen.getByText("keep"));

    expect(sent).not.toContainEqual({ cmd: "scratch_delete", name: "scratch-1.java" });
    expect(screen.getByText("scratch-1.java")).toBeInTheDocument();
  });

  it("renames in place, on Enter", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
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
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
    render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Rename…"));
    fireEvent.keyDown(screen.getByLabelText("new name"), { key: "Enter" });

    expect(sent.some((m) => (m as { cmd?: string }).cmd === "scratch_rename")).toBe(false);
  });

  it("abandons a rename on Escape", async () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
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
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
    const { rerender } = render(<ScratchTool />);

    fireEvent.contextMenu(screen.getByText("scratch-1.java"));
    fireEvent.click(await screen.findByText("Rename…"));
    expect(screen.getByLabelText("new name")).toBeInTheDocument();

    act(() => {
      useStore.setState({ scratch: { names: [], open: {} } as never });
    });
    rerender(<ScratchTool />);

    expect(screen.queryByLabelText("new name")).not.toBeInTheDocument();
  });
});
