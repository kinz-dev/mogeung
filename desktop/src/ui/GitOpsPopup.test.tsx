/**
 * The Git Operations popup, from the keyboard. `R-D31`.
 *
 * The list's rules are `lib/gitOps.test.ts`; this is the half that can only be
 * tested by pressing keys — that the chord opens it, that a digit runs the
 * entry it is drawn beside, that `7` hands over to the branches popup, and
 * that a refusal is *said* rather than swallowed.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { GitOpsPopup } from "@/ui/GitOpsPopup";
import { useStore, emptyGit } from "@/store";
import type { ClientMsg, Session } from "@/wire/types";

class FakeSocket {
  static OPEN = 1;
  readyState = 0;
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: ((e: CloseEvent) => void) | null = null;
  constructor(public url: string) {}
  send() {}
  close() {}
}
vi.stubGlobal("WebSocket", FakeSocket);

const sent: ClientMsg[] = [];

const session = (over: Partial<Session> = {}): Session =>
  ({ id: "s1", cwd: "/repo", repo_root: "/repo", title: "the session", alive: true, ...over }) as unknown as Session;

function open(over: { repo?: boolean; selectedPath?: string | null } = {}) {
  sent.length = 0;
  useStore.setState({
    selected: "s1",
    sessions: { s1: session(over.repo === false ? { repo_root: null, cwd: "/tmp" } : {}) },
    git: { s1: { ...emptyGit(), selectedPath: over.selectedPath ?? null } },
    gitPopup: "ops",
    gitTab: "log",
    commitFocus: 0,
    notices: [],
    activePane: null,
    send: ((m: ClientMsg) => sent.push(m)) as never,
  } as never);
  return render(<GitOpsPopup />);
}

const menu = () => screen.getByRole("menu", { name: "Git operations" });
const press = (key: string) => fireEvent.keyDown(menu(), { key });

beforeEach(() => cleanup());

describe("the popup", () => {
  it("draws IntelliJ's list, with the numbers it is driven by", () => {
    open();
    expect(screen.getByText("Commit…")).toBeInTheDocument();
    expect(screen.getByText("Branches…")).toBeInTheDocument();
    expect(screen.getByText("Worktrees…")).toBeInTheDocument();
    // The digit column, which is the whole reason this is not the palette.
    for (const d of ["1", "7", "8", "0"]) expect(screen.getByText(d)).toBeInTheDocument();
  });

  it("runs the entry a digit is drawn beside, and closes", () => {
    open();
    press("1");
    expect(useStore.getState().gitTab).toBe("local");
    expect(useStore.getState().prefs.dock).toBe("git");
    expect(useStore.getState().commitFocus).toBe(1);
    expect(useStore.getState().gitPopup).toBeNull();
  });

  /** The ask, in one line: *press 7 and go to the next popup*. */
  it("hands over to the branches popup on 7", () => {
    open();
    press("7");
    expect(useStore.getState().gitPopup).toBe("branches");
  });

  it("walks with the arrows and runs with Enter", () => {
    open();
    press("ArrowDown");
    press("ArrowDown");
    press("ArrowDown");
    // 1 → 2 → 3 → 4, and the fourth is Show History.
    press("Enter");
    expect(sent.at(-1)).toMatchObject({ cmd: "git_log", session_id: "s1" });
  });

  /**
   * A popup that ignores a key you pressed is indistinguishable from one that
   * has crashed — so a refusal is a sentence, and the list stays up because
   * the answer is usually the next entry down.
   */
  it("says why an entry cannot run, and stays open", () => {
    open();
    press("3");
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/Rollback… — no file is selected/);
    expect(useStore.getState().gitPopup).toBe("ops");
    expect(sent).toEqual([]);
  });

  it("refuses push with the decision behind it, not with a shrug", () => {
    open({ selectedPath: "a.rs" });
    press("8");
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/never publishes/);
    expect(sent).toEqual([]);
  });

  /** Rollback is the one verb with no undo; the popup asks first, by name. */
  it("names the file before it rolls anything back", () => {
    open({ selectedPath: "a.rs" });
    press("3");
    expect(screen.getByRole("dialog", { name: /Roll back/ })).toBeInTheDocument();
    expect(screen.getByText(/a\.rs/)).toBeInTheDocument();
    expect(sent).toEqual([]);
    act(() => {
      fireEvent.click(screen.getByText("Roll it back"));
    });
    expect(sent).toEqual([{ cmd: "git_discard", session_id: "s1", paths: ["a.rs"] }]);
    expect(useStore.getState().gitPopup).toBeNull();
  });

  it("closes on Escape without doing anything", () => {
    open();
    press("Escape");
    expect(useStore.getState().gitPopup).toBeNull();
    expect(sent).toEqual([]);
  });

  it("refuses everything but push for a session outside a repository", () => {
    open({ repo: false });
    press("1");
    expect(useStore.getState().notices.at(-1)?.text).toMatch(/not in a git repository/);
    expect(useStore.getState().gitPopup).toBe("ops");
  });
});
