/**
 * A folder you kept has to still be there tomorrow, spelled the way you left
 * it. `R-J45`.
 *
 * These aim at the two ways a hand-made list goes wrong: the same folder in it
 * twice under two spellings, and a `~` that means one home here and another
 * one at the far end of the socket.
 */

import { describe, expect, it } from "vitest";
import { addFavourite, isFavourite, normaliseDir, removeFavourite } from "@/lib/favourites";

describe("keeping a folder", () => {
  it("holds one entry per folder however you spelled it", () => {
    let list = addFavourite([], "~/projects/mogeung");
    list = addFavourite(list, "~/projects/mogeung/");
    list = addFavourite(list, "  ~/projects/mogeung  ");

    expect(list).toEqual(["~/projects/mogeung"]);
  });

  it("keeps the order you built, newest last", () => {
    const list = addFavourite(addFavourite(["~/a"], "~/b"), "~/c");
    expect(list).toEqual(["~/a", "~/b", "~/c"]);
  });

  it("refuses blank, so an empty field cannot make an entry you can see but not read", () => {
    expect(addFavourite([], "   ")).toEqual([]);
  });

  /**
   * The one the client must not be clever about: `shellexpand` runs in the
   * **daemon**, so a `~` written here has to survive to the far end untouched.
   * Expanding it against this machine's home is how a favourite opens the
   * wrong folder — or nothing — the first time the window is pointed at a
   * remote daemon.
   */
  it("leaves ~ alone, because the daemon is what expands it", () => {
    expect(normaliseDir("~/projects/foo")).toBe("~/projects/foo");
    expect(addFavourite([], "~/projects/foo")).toEqual(["~/projects/foo"]);
  });

  it("never strips the root down to nothing", () => {
    expect(normaliseDir("/")).toBe("/");
  });

  it("removes by the spelling it stored, not by the one you typed", () => {
    const list = addFavourite([], "~/projects/foo");
    expect(removeFavourite(list, "~/projects/foo/")).toEqual([]);
  });

  it("reads a folder back as kept whichever way it is asked", () => {
    const list = addFavourite([], "~/projects/foo");
    expect(isFavourite(list, "~/projects/foo/")).toBe(true);
    expect(isFavourite(list, "~/projects/bar")).toBe(false);
    expect(isFavourite(list, "")).toBe(false);
  });
});
