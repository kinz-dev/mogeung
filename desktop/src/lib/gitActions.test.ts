/**
 * `R-D21`'s warning names the agents on the machine the switch acts on.
 * `R-I23`, feature 0047.
 *
 * One repository path, two machines: the laptop's checkout of `/w/app` and
 * the Mac's checkout of `/w/app` are different worktrees that happen to be
 * spelt alike — the ordinary case for one person's machines. Matched by path
 * alone, a checkout on the Mac warned about the laptop's agent and said
 * nothing about the Mac's own.
 */

import { afterEach, describe, expect, it } from "vitest";
import { useStore } from "@/store";
import { liveSessionsIn } from "@/lib/gitActions";
import type { Session } from "@/wire/types";

const s = (id: string, origin?: Session["origin"]) =>
  ({ id, title: id, cwd: "/w/app", repo_root: "/w/app", alive: true, origin }) as unknown as Session;

afterEach(() => useStore.setState({ sessions: {} }));

describe("who is live in a worktree", () => {
  it("names only the agents on the switched session's own machine", () => {
    const mac = { machine_id: "mac-id", host: "mac" };
    useStore.setState({
      sessions: { here: s("here"), "@mac-id:there": s("@mac-id:there", mac), "@mac-id:asker": s("@mac-id:asker", mac) },
    });
    expect(liveSessionsIn("/w/app", "@mac-id:asker").map((x) => x.id).sort()).toEqual([
      "@mac-id:asker",
      "@mac-id:there",
    ]);
    expect(liveSessionsIn("/w/app", "here").map((x) => x.id)).toEqual(["here"]);
  });

  it("is this machine's when no session is named", () => {
    useStore.setState({ sessions: { here: s("here"), "@m:x": s("@m:x", { machine_id: "m" }) } });
    expect(liveSessionsIn("/w/app").map((x) => x.id)).toEqual(["here"]);
  });
});
