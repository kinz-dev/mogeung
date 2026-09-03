/**
 * Scratch files, the window's half. `R-L5`.
 *
 * The property that matters is in the second block: **only the answer to
 * this window's own create opens a pane.** A read comes back on the same
 * event, and a pane restored from the layout reads on mount — if that opened
 * a pane too, every restart would open every scratch file twice.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const showScratchPane = vi.fn();
vi.mock("@/lib/scratch", async (importOriginal) => {
  const orig = await importOriginal<typeof import("@/lib/scratch")>();
  return { ...orig, showScratchPane: (name: string) => showScratchPane(name) };
});

import { useStore } from "@/store";
import {
  SCRATCH_LANGUAGES,
  createScratch,
  fetchScratch,
  forgetScratch,
  parseScratchPaneId,
  scratchPaneId,
  scratchPath,
} from "@/lib/scratch";

const send = vi.fn();

beforeEach(() => {
  send.mockReset();
  showScratchPane.mockReset();
  useStore.setState({ send, scratch: { names: [], files: {} } });
});
afterEach(() => vi.restoreAllMocks());

describe("naming", () => {
  it("round-trips a pane id, and refuses one with no name", () => {
    expect(parseScratchPaneId(scratchPaneId("scratch-1.java"))).toBe("scratch-1.java");
    expect(parseScratchPaneId("scratch:")).toBeNull();
    expect(parseScratchPaneId("file:s1::a.rs")).toBeNull();
  });

  it("offers Java first, every extension once, and plain text last", () => {
    expect(SCRATCH_LANGUAGES[0]).toEqual({ label: "Java", ext: "java" });
    expect(SCRATCH_LANGUAGES[SCRATCH_LANGUAGES.length - 1]?.ext).toBe("txt");
    const exts = SCRATCH_LANGUAGES.map((l) => l.ext);
    expect(new Set(exts).size).toBe(exts.length);
    // Every extension is one the daemon will accept: letters and digits only.
    for (const e of exts) expect(e).toMatch(/^[a-z0-9]{1,12}$/);
  });

  it("names the place on the daemon's machine", () => {
    expect(scratchPath("scratch-2.sql")).toBe("~/.mogeung/scratch/scratch-2.sql");
  });
});

describe("asking the daemon", () => {
  it("create sends the extension and opens nothing — the name is the daemon's", () => {
    createScratch("java");
    expect(send).toHaveBeenCalledWith({ cmd: "scratch_create", ext: "java" });
    expect(showScratchPane).not.toHaveBeenCalled();
  });

  it("fetch reads once, marks the body in flight, and forget lets it be re-read", () => {
    fetchScratch("scratch-1.py");
    fetchScratch("scratch-1.py");
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith({ cmd: "scratch_read", name: "scratch-1.py" });
    expect(useStore.getState().scratch.files["scratch-1.py"]).toEqual({ content: null, saved: 0 });
    forgetScratch("scratch-1.py");
    expect(useStore.getState().scratch.files["scratch-1.py"]).toBeUndefined();
    fetchScratch("scratch-1.py");
    expect(send).toHaveBeenCalledTimes(2);
  });
});

describe("what the daemon says", () => {
  const ingest = (msg: Parameters<ReturnType<typeof useStore.getState>["ingest"]>[0]) =>
    useStore.getState().ingest(msg);

  it("opens a pane on a fresh file, and only on a fresh one", () => {
    ingest({ ev: "scratch_content", name: "scratch-1.java", content: "", fresh: true });
    expect(showScratchPane).toHaveBeenCalledWith("scratch-1.java");
    expect(useStore.getState().scratch.files["scratch-1.java"]?.content).toBe("");

    ingest({ ev: "scratch_content", name: "scratch-2.java", content: "class B {}", fresh: false });
    expect(showScratchPane).toHaveBeenCalledTimes(1);
    expect(useStore.getState().scratch.files["scratch-2.java"]?.content).toBe("class B {}");
  });

  it("keeps the list, and ticks a save only for a file it holds", () => {
    ingest({ ev: "scratches", names: ["scratch-2.java", "scratch-1.java"] });
    expect(useStore.getState().scratch.names).toEqual(["scratch-2.java", "scratch-1.java"]);

    ingest({ ev: "scratch_saved", name: "scratch-9.md" });
    expect(useStore.getState().scratch.files["scratch-9.md"]).toBeUndefined();

    ingest({ ev: "scratch_content", name: "scratch-1.java", content: "", fresh: false });
    ingest({ ev: "scratch_saved", name: "scratch-1.java" });
    ingest({ ev: "scratch_saved", name: "scratch-1.java" });
    expect(useStore.getState().scratch.files["scratch-1.java"]?.saved).toBe(2);
  });
});
