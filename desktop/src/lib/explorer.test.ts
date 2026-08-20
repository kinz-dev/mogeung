import { beforeEach, describe, expect, it, vi } from "vitest";
import { useStore } from "@/store";
import { setDock } from "@/lib/panes";
import {
  closeFile,
  explorerFetch,
  fileFilter,
  forgetEveryFileBody,
  forgetFileBodies,
  openFile,
} from "@/lib/explorer";

/**
 * The Files filter. Written against the two things that made the old one
 * useless — it only saw the rows that happened to be expanded, and it could
 * only match a literal — so every case here would have failed before.
 */
describe("the files filter", () => {
  const tree = [
    "src/ui/tools/FilesTool.tsx",
    "src/ui/tools/SearchTool.tsx",
    "src/lib/explorer.ts",
    "src/store/index.ts",
    "crates/mogeungd/src/state.rs",
    "crates/mogeung-core/src/session.rs",
    "README.md",
  ];
  const matching = (q: string) => tree.filter((p) => fileFilter(q).test(p));

  it("anchors against the name, not only the path", () => {
    // `^state` cannot match the path — it starts with `crates/` — so a filter
    // that tested the path alone would answer nothing here.
    expect(matching("^state")).toEqual(["crates/mogeungd/src/state.rs"]);
  });

  it("anchors against the path too, so a subtree can be asked for", () => {
    expect(matching("^src/lib")).toEqual(["src/lib/explorer.ts"]);
  });

  it("takes an extension as the regex it looks like", () => {
    expect(matching("\\.rs$")).toEqual([
      "crates/mogeungd/src/state.rs",
      "crates/mogeung-core/src/session.rs",
    ]);
  });

  it("is smart-cased: lowercase ignores case, an uppercase letter does not", () => {
    expect(matching("filestool")).toEqual(["src/ui/tools/FilesTool.tsx"]);
    expect(matching("Filestool")).toEqual([]);
  });

  it("reads an unfinished pattern literally rather than answering nothing", () => {
    // Mid-keystroke on the way to `explorer(x)`. A thrown SyntaxError, or an
    // empty result, would both read as "no such file".
    const f = fileFilter("explorer(");
    expect(f.regex).toBe(false);
    expect(f.test("src/lib/explorer(1).ts")).toBe(true);
    expect(f.test("src/lib/explorer.ts")).toBe(false);
  });

  it("is empty when nothing is typed, and matches everything", () => {
    const f = fileFilter("   ");
    expect(f.empty).toBe(true);
    expect(matching("   ")).toEqual(tree);
  });

  it("matches every file with a bare dot-star, which is the cap's job to survive", () => {
    expect(matching(".*")).toEqual(tree);
  });
});

/**
 * One open file, one pane. `R-B53`.
 *
 * The window ran two tab systems until 2026-08-07 — dockview's, and the Code
 * pane's own strip with its own two-way split. These pin the state half of
 * collapsing them: what `open` means now that it no longer carries a side or
 * an active index.
 */
describe("opening files, one pane each", () => {
  beforeEach(() => {
    useStore.setState({ explorer: {} });
    setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  });

  it("keeps one entry per (path, revision)", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s1", "a.rs", { pin: true });
    expect(useStore.getState().explorer.s1.open).toHaveLength(1);

    // The worktree twin of a path and a revision of it are different files.
    openFile("s1", "a.rs", { pin: true, rev: "abc1234" });
    expect(useStore.getState().explorer.s1.open).toHaveLength(2);
  });

  /** The IntelliJ rule, now one preview per session rather than per side. */
  it("reuses the unpinned preview and keeps the pinned ones", () => {
    openFile("s1", "pinned.rs", { pin: true });
    openFile("s1", "browsed-a.rs");
    openFile("s1", "browsed-b.rs");

    const paths = useStore.getState().explorer.s1.open.map((t) => t.path);
    expect(paths).toEqual(["pinned.rs", "browsed-b.rs"]);
  });

  it("promotes a preview to pinned rather than opening it twice", () => {
    openFile("s1", "a.rs");
    openFile("s1", "a.rs", { pin: true });
    const open = useStore.getState().explorer.s1.open;
    expect(open).toHaveLength(1);
    expect(open[0].pinned).toBe(true);
  });

  it("carries a pending gotoLine onto a file already open", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s1", "a.rs", { line: 42 });
    expect(useStore.getState().explorer.s1.open[0].gotoLine).toBe(42);
  });

  it("closes by identity, not by index", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s1", "b.rs", { pin: true });
    closeFile("s1", "a.rs", null);
    expect(useStore.getState().explorer.s1.open.map((t) => t.path)).toEqual(["b.rs"]);
  });

  /** Each session keeps its own files — that is what "bound to its session" is. */
  it("keeps one session's files out of another's", () => {
    openFile("s1", "a.rs", { pin: true });
    openFile("s2", "b.rs", { pin: true });
    expect(useStore.getState().explorer.s1.open.map((t) => t.path)).toEqual(["a.rs"]);
    expect(useStore.getState().explorer.s2.open.map((t) => t.path)).toEqual(["b.rs"]);
  });
});

/**
 * A file re-read after it changed underneath the pane. `R-J38`.
 *
 * Reported 2026-08-20: *"if I edit an opened file somewhere else, on the
 * editor, I can still see the old content until I have to re-open it."* The
 * body was fetched once and cached in the tab, and nothing ever put that cache
 * down — so the pane showed the version it read when the file was opened, with
 * nothing to say it was stale. Every test here fails against that.
 */
describe("re-reading a file that changed underneath", () => {
  const sent: unknown[] = [];

  beforeEach(() => {
    sent.length = 0;
    useStore.setState({
      selected: "s1",
      explorer: {},
      send: ((m: unknown) => sent.push(m)) as never,
    });
  });

  const openWithBody = (path: string, body: string, rev: string | null = null) => {
    openFile("s1", path, { pin: true, rev: rev ?? undefined });
    const st = useStore.getState().explorer.s1;
    useStore.getState().patchExplorer("s1", {
      open: st.open.map((t) => (t.path === path ? { ...t, rev, content: body } : t)),
      pendingFiles: [],
    });
  };
  const tab = (path: string) => useStore.getState().explorer.s1.open.find((t) => t.path === path)!;
  const changed = (...paths: string[]) =>
    useStore.getState().ingest({
      ev: "change_updated",
      session_id: "s1",
      change: { files: paths.map((path) => ({ path })) },
    } as never);

  it("marks a changed file for re-reading without taking its body away", () => {
    openWithBody("src/a.ts", "old");

    changed("src/a.ts");

    expect(tab("src/a.ts").reload).toBe(true);
    // The body stays on screen: nulling it unmounts the editor and shows
    // *loading…*, which would flash the pane every time an agent writes the
    // file you are reading — which is when this fires.
    expect(tab("src/a.ts").content).toBe("old");
  });

  it("leaves a file the change does not name alone", () => {
    openWithBody("src/a.ts", "old");
    openWithBody("src/b.ts", "also old");

    changed("src/a.ts");

    expect(tab("src/b.ts").reload).toBe(false);
  });

  /** A sha's content cannot change, so asking again could only ever return
   *  what we already have. */
  it("never re-reads a revision tab", () => {
    openWithBody("src/a.ts", "at that commit", "abc1234");

    changed("src/a.ts");
    forgetEveryFileBody();

    expect(tab("src/a.ts").reload).toBe(false);
  });

  it("asks the daemon again for a stale body, and not for one it still trusts", () => {
    openWithBody("src/a.ts", "old");
    explorerFetch("s1");
    expect(sent.filter((m) => (m as { cmd: string }).cmd === "fetch_file")).toHaveLength(0);

    forgetFileBodies("s1", ["src/a.ts"]);
    explorerFetch("s1");

    expect(sent).toContainEqual({ cmd: "fetch_file", session_id: "s1", path: "src/a.ts" });
  });

  /** The window coming forward is what catches an edit git cannot see — a
   *  gitignored file, or one already reverted. */
  it("forgets every open worktree body at once", () => {
    openWithBody("src/a.ts", "old");
    openWithBody("src/b.ts", "also old");

    forgetEveryFileBody();

    expect(tab("src/a.ts").reload).toBe(true);
    expect(tab("src/b.ts").reload).toBe(true);
  });
});

/**
 * The reload has to fire in the **app**, not only in a browser tab. `R-J38`.
 *
 * The first cut listened for the DOM's `window.focus` and nothing else, which
 * works in `npm run dev` and does nothing at all in the shipped window: a Tauri
 * webview never loses document focus when its OS window goes behind another,
 * so the event simply does not arrive. Reported the same day, in those words —
 * *"it may be working in the web but not in the tauri desktop"*.
 *
 * What is pinned here is that the hook subscribes to the **shell's** signal as
 * well, because that is the difference between the two, and a test that only
 * dispatched a DOM event would have passed against the broken version.
 */
describe("re-reading when the window comes forward", () => {
  it("subscribes to the shell's focus event, not only the DOM's", async () => {
    const listeners: ((e: { payload: boolean }) => void)[] = [];
    const unlisten = vi.fn();
    vi.stubGlobal("__TAURI_INTERNALS__", {});
    vi.doMock("@tauri-apps/api/window", () => ({
      getCurrentWindow: () => ({
        onFocusChanged: (cb: (e: { payload: boolean }) => void) => {
          listeners.push(cb);
          return Promise.resolve(unlisten);
        },
      }),
    }));

    const { onWindowFocus } = await import("@/lib/tauri");
    const fired: number[] = [];
    const stop = await onWindowFocus(() => fired.push(1));

    expect(listeners).toHaveLength(1);
    // Losing focus is not the moment to re-read; coming back is.
    listeners[0]({ payload: false });
    expect(fired).toHaveLength(0);
    listeners[0]({ payload: true });
    expect(fired).toHaveLength(1);

    stop();
    expect(unlisten).toHaveBeenCalledOnce();
    vi.unstubAllGlobals();
    vi.doUnmock("@tauri-apps/api/window");
  });

  /** In a browser there is no shell to ask, and saying so must not throw. */
  it("is a no-op outside the desktop shell", async () => {
    vi.unstubAllGlobals();
    const { onWindowFocus } = await import("@/lib/tauri");
    const stop = await onWindowFocus(() => {});
    expect(() => stop()).not.toThrow();
  });
});
