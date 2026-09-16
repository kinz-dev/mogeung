/**
 * The listener half of `R-J94`: what the window does with a drag.
 *
 * Two facts are worth pinning, and only one of them is the feature. A file
 * drop has to open a pane — and a **dockview tab drag must pass straight
 * through**, because `R-J20` paid for those gestures with the shell's own
 * drop handler and a greedy listener here would take them back.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render } from "@testing-library/react";
import { useStore } from "@/store";
import { FileDrop } from "@/ui/FileDrop";
import type { Session } from "@/wire/types";

const openFile = vi.fn();
vi.mock("@/lib/explorer", () => ({ openFile: (...args: unknown[]) => openFile(...args) }));

const session = (id: string, root: string): Session =>
  ({ id, repo_root: root, cwd: root }) as unknown as Session;

/**
 * A drag event jsdom will dispatch. It has no `DragEvent` constructor, and
 * the handler reads three fields — so the event carries exactly those, which
 * also documents what the feature depends on.
 */
function drag(type: string, data: Record<string, string> | null): Event {
  const e = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(e, "dataTransfer", {
    value: data && {
      types: Object.keys(data),
      getData: (t: string) => data[t] ?? "",
      dropEffect: "none",
    },
  });
  return e;
}

beforeEach(() => {
  openFile.mockClear();
  useStore.setState({ notices: [], sessions: {}, selected: null, explorer: {} });
});

describe("a file dropped on the window", () => {
  it("opens it in the session whose root holds it", () => {
    act(() => {
      useStore.setState({ sessions: { a: session("a", "/repo") }, selected: "a" });
    });
    render(<FileDrop />);

    act(() => {
      window.dispatchEvent(drag("drop", { Files: "", "text/uri-list": "file:///repo/src/main.rs" }));
    });

    expect(openFile).toHaveBeenCalledWith("a", "src/main.rs", { pin: true });
  });

  /** `R-J40`'s door, opened by the drag itself — and said out loud, since it persists. */
  it("admits the folder first when the file is outside every session", () => {
    const sent: unknown[] = [];
    act(() => {
      useStore.setState({
        sessions: { a: session("a", "/repo") },
        selected: "a",
        send: (msg) => void sent.push(msg),
      });
    });
    render(<FileDrop />);

    act(() => {
      window.dispatchEvent(drag("drop", { Files: "", "text/uri-list": "file:///elsewhere/a.ts" }));
    });

    expect(sent).toEqual([{ cmd: "add_workspace_dir", session_id: "a", path: "/elsewhere" }]);
    expect(openFile).toHaveBeenCalledWith("a", "/elsewhere/a.ts", { pin: true });
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/joined this session's workspace/);
  });

  it("says so rather than opening an empty pane when there is no session", () => {
    render(<FileDrop />);
    act(() => {
      window.dispatchEvent(drag("drop", { Files: "", "text/uri-list": "file:///repo/a.ts" }));
    });
    expect(openFile).not.toHaveBeenCalled();
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/select a session first/);
  });

  it("lets a dockview tab drag past untouched", () => {
    act(() => {
      useStore.setState({ sessions: { a: session("a", "/repo") }, selected: "a" });
    });
    render(<FileDrop />);

    const over = drag("dragover", { "text/plain": "" });
    const dropped = drag("drop", { "text/plain": "" });
    act(() => {
      window.dispatchEvent(over);
      window.dispatchEvent(dropped);
    });

    // Not prevented is the whole assertion: dockview's own handler runs on an
    // event nobody has cancelled, and it never reaches this one otherwise.
    expect(over.defaultPrevented).toBe(false);
    expect(dropped.defaultPrevented).toBe(false);
    expect(openFile).not.toHaveBeenCalled();
  });
});
