/**
 * The Run panel, and the three rules it must not quietly lose. `R-N5`.
 *
 * Every assertion here is a decision from an ADR rather than a preference, and
 * each is the kind of thing a later tidy-up would undo without noticing:
 *
 * - a run request names an **id**, never a command (ADR-0025 clause 1);
 * - an entry mogeung cannot run is **listed with its reason** (ADR-0026);
 * - an environment **value never reaches the screen** unasked (`R-N6`).
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { RunPane } from "@/panes/RunPane";
import { useStore } from "@/store";
import { PaneScope } from "@/lib/paneScope";
import type { ClientMsg, Run, RunConfig } from "@/wire/types";

const sent: ClientMsg[] = [];

const config = (extra: Partial<RunConfig> = {}): RunConfig => ({
  id: "detected:cargo-test",
  name: "cargo test",
  program: "cargo",
  args: ["test"],
  dir: "",
  origin: "detected",
  env_keys: [],
  unrunnable: null,
  ...extra,
});

const run = (extra: Partial<Run> = {}): Run => ({
  id: "run-1",
  config_id: "detected:cargo-test",
  name: "cargo test",
  command: "cargo test",
  session_id: "s1",
  state: "exited",
  exit_code: 0,
  started_at: "2026-08-11T10:00:00Z",
  ended_at: "2026-08-11T10:01:00Z",
  dropped: 0,
  ...extra,
});

function show(configs: RunConfig[], runs: Run[] = [], allowed = true) {
  useStore.setState({
    selected: "s1",
    runConfigs: { s1: configs },
    runs,
    runsAllowed: allowed,
    revealedEnv: {},
    send: ((m: ClientMsg) => sent.push(m)) as never,
  });
  return render(
    <PaneScope id="run">
      <RunPane />
    </PaneScope>,
  );
}

beforeEach(() => {
  cleanup();
  sent.length = 0;
  vi.clearAllMocks();
});

describe("starting a run", () => {
  /**
   * **ADR-0025 clause 1, and it is the whole security argument.** The wire
   * carries "run config #3", never a command string — that is the difference
   * between a port that runs this repository's test suite and a port that is a
   * shell. A `command` field appearing on this message would undo it silently.
   */
  it("names a configuration by id and never sends a command", () => {
    show([config()]);
    fireEvent.click(screen.getByTitle("run it"));

    const start = sent.find((m) => m.cmd === "run_start");
    expect(start).toEqual({ cmd: "run_start", session_id: "s1", config_id: "detected:cargo-test" });
    // Belt and braces: no message this pane can send may carry a command line.
    for (const m of sent) {
      expect(JSON.stringify(m)).not.toContain("cargo test");
    }
  });

  /** ADR-0025 clause 4 reaching the surface, so the panel explains rather than
   * drawing buttons that will be refused. */
  it("says why it cannot run anything when the daemon forbids it", () => {
    show([config()], [], false);

    expect(screen.getByText(/--allow-run/)).toBeTruthy();
    expect(screen.getByText(/clause 4/)).toBeTruthy();
    // …and the button is refused rather than merely explained away.
    expect((screen.getByTitle("run it") as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("hiding nothing", () => {
  /** ADR-0026: a configuration that silently vanishes reads as "mogeung did
   * not find it". So it is listed, named, and carries its reason. */
  it("lists an unrunnable entry with the reason and no run button", () => {
    show([
      config({
        id: "vscode:lldb",
        name: "Cargo launch",
        origin: "vs_code",
        unrunnable: "this configuration names no program — it needs the debugger R-N9 adds",
      }),
    ]);

    expect(screen.getByText("Cargo launch")).toBeTruthy();
    expect(screen.getByText(/needs the debugger/)).toBeTruthy();
    // Present but refused, rather than absent.
    const button = screen.getByTitle(/names no program/);
    expect((button as HTMLButtonElement).disabled).toBe(true);
  });

  /** ADR-0026 named the cost of promoting detection: a wrong inference is a
   * first impression. A guess has to say it is one. */
  it("says which entries were inferred", () => {
    show([config(), config({ id: "vscode:x", name: "theirs", origin: "vs_code" })]);
    expect(screen.getByText("inferred")).toBeTruthy();
    expect(screen.getByText("launch.json")).toBeTruthy();
  });
});

describe("environment values", () => {
  /**
   * **`R-N6`, and the sweep found a real API key in this exact position.** The
   * name is shown because "this run sets API_KEY" is worth knowing; the value
   * is not on screen and was never sent.
   */
  it("shows the name and never the value until it is asked for", () => {
    show([config({ env_keys: ["API_KEY"] })]);

    expect(screen.getByText(/API_KEY/)).toBeTruthy();
    expect(document.body.textContent).toContain("••••••");
    expect(document.body.textContent).not.toContain("sk-live");
    // Nothing was requested merely by rendering.
    expect(sent.some((m) => m.cmd === "reveal_run_env")).toBe(false);
  });

  /** Revealing is deliberate, per value, and by name. */
  it("asks for one value by name when you click it", () => {
    show([config({ env_keys: ["API_KEY", "PORT"] })]);
    fireEvent.click(screen.getByTitle("reveal API_KEY"));

    expect(sent).toContainEqual({
      cmd: "reveal_run_env",
      session_id: "s1",
      config_id: "detected:cargo-test",
      key: "API_KEY",
    });
    // One at a time: clicking one must not fetch its neighbour.
    expect(sent.filter((m) => m.cmd === "reveal_run_env")).toHaveLength(1);
  });
});

describe("what a run says about a claim", () => {
  /**
   * **`R-N7`'s third answer.** A stopped run is not a pass and not a failure —
   * nobody let it finish. Colouring one as a failure would be the window
   * asserting something it does not know, which is exactly the laundering the
   * row forbids in the other direction.
   */
  it("reports a stopped run as stopped rather than failed", () => {
    show([config()], [run({ state: "stopped", exit_code: null })]);
    expect(screen.getByText("stopped")).toBeTruthy();
    expect(screen.queryByText(/exit/)).toBeNull();
  });

  it("reports a passing run and a failing one as themselves", () => {
    show([config()], [run({ state: "exited", exit_code: 0 })]);
    expect(screen.getByText("passed")).toBeTruthy();

    cleanup();
    show([config()], [run({ state: "exited", exit_code: 3 })]);
    expect(screen.getByText("exit 3")).toBeTruthy();
  });
});

describe("an empty repository", () => {
  /** Most repositories carry no configuration file — 35 of 58 here — so this
   * is the ordinary case and must read as a fact rather than a fault. */
  it("explains where entries come from rather than looking broken", () => {
    show([]);
    expect(screen.getByText("nothing to run here")).toBeTruthy();
    expect(screen.getByText(/Cargo.toml/)).toBeTruthy();
  });
});
