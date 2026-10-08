/**
 * The price table in Health, and its alerts. `R-J124`.
 *
 * On 2026-10-08 five models in use had no price and the only sign was a line
 * under the Analytics summary. These pin that the gap reaches the window that
 * exists to say what mogeung cannot see, and that switching the weekly check
 * on is one click to a config draft you still have to save.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { HealthWindow, alertMessage, isUrgent } from "@/ui/HealthWindow";
import { withLine } from "@/ui/ConfigWindow";
import { useStore } from "@/store";
import type { Alert, Health, PricingHealth } from "@/wire/types";

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

const pricing = (p: Partial<PricingHealth> = {}): PricingHealth => ({
  built_in_as_of: "2026-10-08",
  host: null,
  refusal: null,
  last_attempt: null,
  last_ok: null,
  last_error: null,
  models_read: 0,
  differences: [],
  unmapped: [],
  ...p,
});

function show(alerts: Alert[], p: PricingHealth | null = pricing()) {
  const health = {
    last_scan: null,
    scans: 1,
    sessions_known: 1,
    sessions_live: 0,
    transcripts_found: 1,
    lines_seen: 10,
    lines_parsed: 10,
    lines_ignored: 0,
    lines_barren: 0,
    lines_unknown: 0,
    lines_malformed: 0,
    unknown_types: [],
    versions_seen: [],
    current_version: null,
    history_skipped_bytes: 0,
    max_transcript_bytes: 0,
    alerts,
    pricing: p,
  } as Health;
  useStore.setState({ showHealth: true, showConfig: false, configAppend: null, health, send: (() => {}) as never });
  render(<HealthWindow />);
}

beforeEach(() => cleanup());

describe("the price table's alerts", () => {
  it("names an unpriced model and counts it as something mogeung cannot see", () => {
    const a: Alert = { kind: "unpriced_model", model: "claude-opus-9", messages: 12 };
    expect(isUrgent(a)).toBe(true);
    show([a]);
    expect(screen.getByText(/1 thing\(s\) mogeung cannot see/)).toBeTruthy();
    expect(screen.getByText(/"claude-opus-9" has no price in this build — its 12 message/)).toBeTruthy();
  });

  /** Sent by the daemon since `R-N2` and drawn as nothing until now. */
  it("describes a run configuration it cannot run, without calling it urgent", () => {
    const a: Alert = { kind: "unknown_run_config_type", ty: "cppvsdbg", count: 2 };
    expect(isUrgent(a)).toBe(false);
    expect(alertMessage(a)).toContain("cppvsdbg");
  });

  /** A kind from a newer daemon is said, not rendered as an empty box. */
  it("says something for a kind it has never heard of", () => {
    expect(alertMessage({ kind: "warp_drive" } as unknown as Alert)).toContain("warp_drive");
  });
});

describe("the Prices section", () => {
  it("says nothing checks the table, and offers to — as an unsaved config line", () => {
    show([]);
    expect(screen.getByText(/this build's price table, read 2026-10-08/)).toBeTruthy();
    fireEvent.click(screen.getByText("check it weekly…"));
    const s = useStore.getState();
    expect(s.showConfig).toBe(true);
    expect(s.showHealth).toBe(false);
    expect(s.configAppend).toBe('refresh_pricing = "platform.claude.com"');
  });

  it("shows the last reading and every difference it found", () => {
    show(
      [{ kind: "prices_differ", count: 1 }],
      pricing({
        host: "platform.claude.com",
        last_ok: "2026-10-15T12:00:00Z",
        models_read: 21,
        differences: ["claude-opus-6 is on the pricing page at $5/$25 and has no rate in this build"],
        unmapped: ["Claude Haiku 3.5"],
      }),
    );
    expect(screen.getByText(/21 models/)).toBeTruthy();
    expect(screen.getByText("1 difference")).toBeTruthy();
    expect(screen.getByText(/claude-opus-6 is on the pricing page/)).toBeTruthy();
    expect(screen.getByText(/Claude Haiku 3.5/)).toBeTruthy();
    // Compare-only, and it says so.
    expect(screen.getByText(/changes no price yet/)).toBeTruthy();
  });

  it("says why the last attempt failed", () => {
    show([], pricing({ host: "platform.claude.com", last_error: "curl: (6) Could not resolve host" }));
    expect(screen.getByText(/failed: curl: \(6\) Could not resolve host/)).toBeTruthy();
    expect(screen.getByText("Not read yet.")).toBeTruthy();
  });

  it("is absent for a daemon that predates it", () => {
    show([], null);
    expect(screen.queryByText("Prices")).toBeNull();
  });
});

describe("the config line", () => {
  it("is added at the end, on a line of its own", () => {
    expect(withLine("", 'refresh_pricing = "platform.claude.com"')).toBe('refresh_pricing = "platform.claude.com"\n');
    expect(withLine("beacon = false", 'refresh_pricing = "x"')).toBe('beacon = false\nrefresh_pricing = "x"\n');
  });

  /** TOML refuses a key given twice, and a wrong host is what gets corrected. */
  it("replaces the key's line rather than adding a second", () => {
    const text = '# mine\nrefresh_pricing = "evil.example"\nbeacon = false\n';
    expect(withLine(text, 'refresh_pricing = "platform.claude.com"')).toBe(
      '# mine\nrefresh_pricing = "platform.claude.com"\nbeacon = false\n',
    );
  });
});
