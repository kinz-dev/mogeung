/**
 * The list of scratch files, and the one thing it must not become. `R-L6`.
 *
 * Asked 2026-09-08: *"I think we need to panel to show the scratch files. The
 * current files panel didn't show it."* Files is right not to — it browses the
 * session's worktree, and a scratch file is in nobody's worktree.
 *
 * The interesting assertions here are the **absences**. Feature 0039 put a
 * list, search or delete out of scope on the argument that anything more turns
 * these into documents, and this row reopened only the first of those three.
 * A test that pins what is deliberately missing is worth more than one that
 * pins what is there, because the missing half is what a later change will
 * quietly add.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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

  /**
   * The line feature 0039 drew, and the one this row deliberately did not
   * cross. `rm` deletes a scratch file; a delete button here is the first step
   * to these being documents, which is what ADR-0035 is about.
   */
  it("offers no delete and no rename", () => {
    useStore.setState({ scratch: { names: ["scratch-1.java"], open: {} } as never });
    render(<ScratchTool />);

    expect(screen.queryByTitle(/delete|remove|forget/i)).not.toBeInTheDocument();
    expect(screen.queryByTitle(/rename/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });
});
