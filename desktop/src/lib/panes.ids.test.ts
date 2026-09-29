/**
 * A file or diff pane's id carries its session, and a followed session's id
 * has a colon in it. `R-I26`.
 *
 * Reported 2026-09-29: *"when I open a remote session that I follow … when I
 * press <ctl>+p to search for the file to open, I can't actually open the
 * files"*. The daemon was not the fault — `list_tree` and `fetch_file` both
 * cross the relay and come back under the mirrored id, checked against the
 * running daemon. The pane was: its id is `file:<session>:<rev>:<path>`, read
 * back by splitting at the first two colons on the grounds that *a session id
 * cannot contain one* — true until following (`R-I21`) named a remote session
 * `@<machine>:<id>`. The pane then read session `@<machine>`, revision `<id>`,
 * and found nothing to show.
 */

import { describe, expect, it } from "vitest";
import { diffPaneId, filePaneId, parseDiffPaneId, parseFilePaneId } from "@/lib/panes";

const FOLLOWED = "@de5059de3a37d86bf5689e0227db3e1a:27196eb5-e9ed-414d-8d36-90c1fac97849";

describe("a pane id with a followed session in it", () => {
  it("gives a file pane its whole session back", () => {
    expect(parseFilePaneId(filePaneId(FOLLOWED, ".cargo/config.toml", null))).toEqual({
      session: FOLLOWED,
      rev: null,
      path: ".cargo/config.toml",
    });
  });

  it("and a revision, and a path with colons of its own", () => {
    expect(parseFilePaneId(filePaneId(FOLLOWED, "a/b:c.rs", "fa64b61"))).toEqual({
      session: FOLLOWED,
      rev: "fa64b61",
      path: "a/b:c.rs",
    });
  });

  it("gives a diff pane its whole session back", () => {
    expect(parseDiffPaneId(diffPaneId(FOLLOWED, "fa64b61", "src/lib.rs"))).toEqual({
      session: FOLLOWED,
      rev: "fa64b61",
      path: "src/lib.rs",
    });
  });

  /** This machine's own sessions, unchanged. */
  it("leaves a session of this machine's as it was", () => {
    expect(parseFilePaneId(filePaneId("s1", "x:y", null))).toEqual({ session: "s1", rev: null, path: "x:y" });
    expect(parseDiffPaneId(diffPaneId("s1", "abc", "p"))).toEqual({ session: "s1", rev: "abc", path: "p" });
  });

  it("refuses an id cut short", () => {
    expect(parseFilePaneId(`file:${FOLLOWED}`)).toBeNull();
    expect(parseFilePaneId("file:@m")).toBeNull();
    expect(parseDiffPaneId(`diff:${FOLLOWED}:abc`)).toBeNull();
  });
});
