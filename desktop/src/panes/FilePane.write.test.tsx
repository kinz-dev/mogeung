/**
 * The one editable file pane. `R-P9`, ADR-0019's 2026-10-09 amendment.
 *
 * Pinned: a file stays a viewer for every session you started by hand, for
 * a seat in the team's own checkout, and for a revision; it is editable for
 * a seat whose launch record says its directory is a worktree the daemon
 * made, and the header says so. A save carries the hash the draft **began**
 * from — not whatever the store holds now — so an agent's write between
 * your first keystroke and your save is refused by the daemon rather than
 * overwritten. A refusal for a moved file offers to reload theirs and says
 * how far apart the two are; one for a busy seat offers *save anyway*,
 * which is the one override fence 3 admits.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { FilePane } from "@/panes/FilePane";
import { PaneScope } from "@/lib/paneScope";
import { filePaneId, setDock } from "@/lib/panes";
import { useStore, emptyExplorer, type FileTab } from "@/store";
import { defaultPrefs } from "@/store/prefs";
import type { ClientMsg, SeatLaunch, Session } from "@/wire/types";

vi.mock("@monaco-editor/react", () => ({
  __esModule: true,
  default: ({ value, onChange, options }: { value?: string; onChange?: (v: string) => void; options?: { readOnly?: boolean } }) => (
    <textarea data-testid="source" readOnly={options?.readOnly} value={value ?? ""} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));

const sent: ClientMsg[] = [];

const launch = (seat: string, worktree: boolean): SeatLaunch => ({
  team_id: "t1",
  team: "port",
  seat,
  cli: "claude_code",
  pane_id: "%1",
  tmux_target: "x:0.0",
  tmux_session: "x",
  team_session: "geung-port",
  dir: worktree ? "/w/builder" : "/repo",
  worktree,
  files_dir: "/f",
  started: 1,
  signals: { kind: "hooked", events: 0, unread: 0 },
});

const session = (id: string, seat: string | null): Session =>
  ({ id, alive: true, live_status: "idle", seat: seat ? { team_id: "t1", team: "port", seat, signals: "hooked" } : undefined }) as unknown as Session;

function tab(over: Partial<FileTab> = {}): FileTab {
  return {
    path: "src/lib.rs",
    rev: null,
    pinned: true,
    content: "fn a() {}\n",
    truncated: false,
    hash: "h1",
    saved: 0,
    refusal: null,
    gotoLine: null,
    reload: false,
    ...over,
  };
}

function open(sessionId: string, seat: string | null, worktree: boolean, t: FileTab = tab()) {
  useStore.setState({
    prefs: defaultPrefs(),
    selected: sessionId,
    sessions: { [sessionId]: session(sessionId, seat) },
    seatLaunches: seat ? [launch(seat, worktree)] : [],
    explorer: { [sessionId]: { ...emptyExplorer(), open: [t] } },
    send: (m) => void sent.push(m),
  });
  setDock({ getPanel: () => undefined, panels: [], addPanel: () => {} } as never);
  return render(
    <PaneScope id={filePaneId(sessionId, t.path, t.rev)}>
      <FilePane />
    </PaneScope>,
  );
}

const source = () => screen.getByTestId("source") as HTMLTextAreaElement;

beforeEach(() => {
  cleanup();
  sent.length = 0;
});

describe("the file pane's one write", () => {
  it("stays a viewer for a hand-started session, a checkout seat and a revision", () => {
    open("hand", null, false);
    expect(source().readOnly).toBe(true);
    expect(screen.queryByTestId("editable")).toBeNull();
    cleanup();
    open("k", "keeper", false);
    expect(source().readOnly).toBe(true);
    cleanup();
    open("b", "builder", true, tab({ rev: "abc123" }));
    expect(source().readOnly).toBe(true);
    cleanup();
    open("b", "builder", true, tab({ truncated: true }));
    expect(source().readOnly).toBe(true);
  });

  it("is editable for a seat's worktree, says so, and saves with the hash the draft began from", () => {
    open("b", "builder", true);
    expect(source().readOnly).toBe(false);
    expect(screen.getByTestId("editable").textContent).toContain("editable · seat builder's worktree");
    expect(screen.getByTestId("save-status").textContent).toBe("saved");
    fireEvent.change(source(), { target: { value: "fn a() {}\nfn b() {}\n" } });
    expect(screen.getByTestId("save-status").textContent).toBe("unsaved");
    // An agent writes and the store reloads under you: the draft stays on
    // screen, and the hash the save carries is still the one you began from.
    act(() =>
      useStore.setState((s) => ({
        explorer: { b: { ...s.explorer.b, open: [tab({ content: "fn a() {}\n// theirs\n", hash: "h2" })] } },
      })),
    );
    expect(source().value).toBe("fn a() {}\nfn b() {}\n");
    fireEvent.click(screen.getByText("save"));
    expect(sent).toContainEqual({
      cmd: "file_write",
      session_id: "b",
      path: "src/lib.rs",
      content: "fn a() {}\nfn b() {}\n",
      base_hash: "h1",
      force: false,
    });
    // It lands: the draft ends, and the body follows the store again.
    act(() =>
      useStore.setState((s) => ({
        explorer: { b: { ...s.explorer.b, open: [tab({ content: "fn a() {}\nfn b() {}\n", hash: "h3", saved: 1 })] } },
      })),
    );
    expect(screen.getByTestId("save-status").textContent).toBe("saved");
    act(() =>
      useStore.setState((s) => ({
        explorer: { b: { ...s.explorer.b, open: [tab({ content: "fn a() {}\nfn b() {}\n// later\n", hash: "h4", saved: 1 })] } },
      })),
    );
    expect(source().value).toBe("fn a() {}\nfn b() {}\n// later\n");
  });

  it("shows a moved file's refusal with the distance and a reload, and a busy seat's with save anyway", () => {
    open("b", "builder", true);
    fireEvent.change(source(), { target: { value: "fn a() {}\nmine\n" } });
    act(() =>
      useStore.setState((s) => ({
        explorer: {
          b: {
            ...s.explorer.b,
            open: [
              tab({
                refusal: { why: "the file changed on disk since you opened it — reload it", current: "fn a() {}\ntheirs\n", hash: "h9", busy: null },
              }),
            ],
          },
        },
      })),
    );
    const banner = screen.getByTestId("refusal");
    expect(banner.textContent).toContain("not saved — the file changed on disk");
    expect(banner.textContent).toContain("theirs differs from yours on 2 lines");
    expect(screen.queryByText("save anyway")).toBeNull();
    fireEvent.click(screen.getByText("reload theirs"));
    expect(sent.filter((m) => m.cmd === "fetch_file")).toHaveLength(1);
    expect(useStore.getState().explorer.b.open[0].refusal).toBeNull();

    cleanup();
    sent.length = 0;
    open("b", "builder", true);
    fireEvent.change(source(), { target: { value: "fn a() {}\nmine\n" } });
    act(() =>
      useStore.setState((s) => ({
        explorer: {
          b: { ...s.explorer.b, open: [tab({ refusal: { why: "seat `builder` is mid-turn", current: null, hash: "h1", busy: "builder" } })] },
        },
      })),
    );
    expect(screen.getByTestId("refusal").textContent).toContain("mid-turn");
    expect(screen.queryByText("reload theirs")).toBeNull();
    fireEvent.click(screen.getByText("save anyway"));
    const forced = sent.find((m) => m.cmd === "file_write");
    expect(forced).toMatchObject({ cmd: "file_write", force: true, base_hash: "h1", content: "fn a() {}\nmine\n" });
  });
});
