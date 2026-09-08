/**
 * The daemon list is hand-editable, which means it is also hand-corruptible.
 * Losing every bookmark because one row is malformed is the failure the prefs
 * loader already refuses, and this refuses it the same way.
 *
 * Since `R-I16` there are two stores rather than one — a `0600` file the shell
 * owns, and `localStorage` when there is no shell (ADR-0036) — so most of what
 * is worth testing here is the seam between them, and the one-way migration
 * across it.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

const tauri = { on: false };
vi.mock("@/lib/tauri", () => ({ isTauri: () => tauri.on }));

/** The shell, standing in for `~/.mogeung/connections.json`. */
const file: { list: unknown[]; saves: number } = { list: [], saves: 0 };
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string, args?: { list?: unknown[] }) => {
    if (cmd === "connections_load") return file.list;
    if (cmd === "connections_save") {
      file.list = args?.list ?? [];
      file.saves += 1;
      return undefined;
    }
    throw new Error(`unexpected command ${cmd}`);
  }),
}));

import {
  defaultName,
  dialledUrl,
  loadConnections,
  reorder,
  saveConnections,
  storageKind,
  withCurrent,
  type Connection,
} from "@/lib/connections";

const KEY = "mogeung.connections";
const box = (over: Partial<Connection> = {}): Connection => ({
  id: "c1",
  name: "dev box",
  url: "ws://devbox:7717/ws",
  ...over,
});

beforeEach(() => {
  localStorage.clear();
  tauri.on = false;
  file.list = [];
  file.saves = 0;
});

describe("the stored connections, in a browser tab", () => {
  it("round-trips", async () => {
    await saveConnections([box()]);
    expect(await loadConnections()).toEqual([box()]);
  });

  it("keeps the good rows when one is malformed", async () => {
    localStorage.setItem(
      KEY,
      JSON.stringify([{ id: "a", url: "ws://a/ws", name: "a" }, { nope: 1 }, null, "string"]),
    );
    expect(await loadConnections()).toEqual([{ id: "a", name: "a", url: "ws://a/ws" }]);
  });

  it("survives a store that is not JSON at all", async () => {
    localStorage.setItem(KEY, "{{{");
    expect(await loadConnections()).toEqual([]);
  });

  it("names a row that has no name after its address", async () => {
    localStorage.setItem(KEY, JSON.stringify([{ id: "a", url: "ws://a/ws" }]));
    expect((await loadConnections())[0].name).toBe("ws://a/ws");
    expect(defaultName("ws://devbox:7717/ws")).toBe("devbox:7717");
  });

  /** A row written before `R-I16` has no id, and must not be dropped for it. */
  it("gives an entry that predates ids an id of its own", async () => {
    localStorage.setItem(KEY, JSON.stringify([{ name: "old", url: "ws://a/ws" }]));
    const list = await loadConnections();
    expect(list).toHaveLength(1);
    expect(list[0].id).toBeTruthy();
  });

  it("says where the list is being kept", () => {
    expect(storageKind()).toBe("browser");
    tauri.on = true;
    expect(storageKind()).toBe("file");
  });
});

describe("moving the list into the shell's file", () => {
  /**
   * The whole point of ADR-0036: a list that predates the file has to arrive in
   * it, exactly once, and the old key has to go — a token left behind in
   * `localStorage` is the exposure this row exists to end.
   */
  it("migrates a localStorage list into the file and clears the key", async () => {
    localStorage.setItem(KEY, JSON.stringify([{ id: "a", name: "a", url: "ws://a/ws" }]));
    tauri.on = true;

    const list = await loadConnections();

    expect(list).toEqual([{ id: "a", name: "a", url: "ws://a/ws" }]);
    expect(file.list).toEqual([{ id: "a", name: "a", url: "ws://a/ws" }]);
    expect(localStorage.getItem(KEY)).toBeNull();
  });

  it("migrates once and not again", async () => {
    localStorage.setItem(KEY, JSON.stringify([{ id: "a", name: "a", url: "ws://a/ws" }]));
    tauri.on = true;

    await loadConnections();
    expect(file.saves).toBe(1);

    await loadConnections();
    expect(file.saves).toBe(1);
  });

  /**
   * The dangerous direction: a file that already has entries must win, or a
   * stale `localStorage` list overwrites edits made since the move.
   */
  it("never overwrites a file that already has entries", async () => {
    file.list = [{ id: "new", name: "current", url: "ws://current/ws" }];
    localStorage.setItem(KEY, JSON.stringify([{ id: "old", name: "stale", url: "ws://old/ws" }]));
    tauri.on = true;

    expect(await loadConnections()).toEqual([{ id: "new", name: "current", url: "ws://current/ws" }]);
    expect(file.saves).toBe(0);
  });

  it("writes to the file rather than to localStorage when the shell is there", async () => {
    tauri.on = true;
    await saveConnections([box({ token: "6f1c" })]);
    expect(file.list).toEqual([box({ token: "6f1c" })]);
    expect(localStorage.getItem(KEY)).toBeNull();
  });
});

describe("the token, which is the reason any of this moved", () => {
  it("is composed into the dialled address and not stored in it", () => {
    const c = box({ token: "6f1c" });
    expect(c.url).not.toContain("6f1c");
    expect(dialledUrl(c)).toBe("ws://devbox:7717/ws?token=6f1c");
  });

  it("is left alone when the address already carries one", () => {
    const c = box({ url: "ws://devbox:7717/ws?token=typed", token: "stored" });
    expect(dialledUrl(c)).toBe("ws://devbox:7717/ws?token=typed");
  });

  it("changes nothing when there is no token", () => {
    expect(dialledUrl(box())).toBe("ws://devbox:7717/ws");
  });

  /** A malformed address must still be dialable — the daemon can refuse it. */
  it("hands back an unparseable address untouched", () => {
    expect(dialledUrl(box({ url: "not a url", token: "x" }))).toBe("not a url");
  });
});

describe("editing and ordering, which id made possible", () => {
  /**
   * The row keyed on `url` before `R-I16`, which is precisely why an address
   * could not be edited: two routes to one daemon collided.
   */
  it("edits one of two entries sharing a URL without disturbing the other", async () => {
    const list: Connection[] = [
      { id: "a", name: "tunnel", url: "ws://localhost:7717/ws" },
      { id: "b", name: "direct", url: "ws://localhost:7717/ws" },
    ];
    const edited = list.map((c) => (c.id === "b" ? { ...c, url: "ws://devbox:7717/ws" } : c));
    await saveConnections(edited);

    const back = await loadConnections();
    expect(back.find((c) => c.id === "a")!.url).toBe("ws://localhost:7717/ws");
    expect(back.find((c) => c.id === "b")!.url).toBe("ws://devbox:7717/ws");
  });

  it("moves an entry one place", () => {
    const list = [box({ id: "a" }), box({ id: "b" }), box({ id: "c" })];
    expect(reorder(list, 2, 1).map((c) => c.id)).toEqual(["a", "c", "b"]);
    expect(reorder(list, 0, 1).map((c) => c.id)).toEqual(["b", "a", "c"]);
  });

  it("does nothing at either end", () => {
    const list = [box({ id: "a" }), box({ id: "b" })];
    expect(reorder(list, 0, -1)).toBe(list);
    expect(reorder(list, 1, 2)).toBe(list);
    expect(reorder(list, 1, 1)).toBe(list);
  });
});

describe("the address in use", () => {
  /**
   * The window must never open on an empty list while plainly connected to
   * something — the entry you most want to keep is the one you never typed.
   */
  it("goes to the top when it is not already there", () => {
    const list = withCurrent([box({ id: "a", name: "a", url: "ws://a/ws" })], "ws://localhost:7717/ws");
    expect(list[0].name).toBe("localhost:7717");
    expect(list[0].url).toBe("ws://localhost:7717/ws");
    expect(list[0].id).toBeTruthy();
    expect(list).toHaveLength(2);
  });

  it("is not duplicated", () => {
    const list = withCurrent([box({ id: "a", name: "here", url: "ws://a/ws" })], "ws://a/ws");
    expect(list).toHaveLength(1);
  });
});
