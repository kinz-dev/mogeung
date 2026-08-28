/**
 * The keymap on a Mac. `R-J19`.
 *
 * Reported 2026-08-08: the defaults work on Ubuntu and the `Alt+*` half of
 * them does nothing on macOS. The failure has no symptom — no error, no
 * warning, just a key that does not do the thing — so the guards here are the
 * only place it can be seen without a Mac in front of you.
 *
 * Two kinds of test, and the second kind is the one that would have caught the
 * bug: the table checks pin *decisions* (which chord, and that none collides),
 * while `presses` mounts the real window and dispatches the events a Mac
 * actually sends — `key: "å"`, `code: "KeyA"` — because a table asserting
 * `Alt+KeyA` would pass just as happily if tinykeys never matched it.
 */

import { beforeAll, afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { ACTIONS, MAC_KEYS, bindingsFor, chordFromEvent, defaultKeys, formatChord } from "@/lib/keymap";
import { currentPlatform } from "@/lib/platform";
import { useStore } from "@/store";
import { defaultPrefs } from "@/store/prefs";

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

beforeAll(() => {
  vi.stubGlobal("WebSocket", FakeSocket);
});

/** Pretend to be the platform, the way the whole keymap asks: `navigator`. */
function asPlatform(platform: string): void {
  Object.defineProperty(window.navigator, "platform", { value: platform, configurable: true });
}

describe("the platform", () => {
  afterEach(() => asPlatform(""));

  it("reads a Mac the way tinykeys does, so `$mod` cannot disagree", () => {
    asPlatform("MacIntel");
    expect(currentPlatform()).toBe("mac");
    asPlatform("Linux x86_64");
    expect(currentPlatform()).toBe("other");
  });

  /** `navigator.platform` is deprecated and may one day be empty. */
  it("falls back to the user agent only when the property is missing", () => {
    asPlatform("");
    expect(currentPlatform()).toBe("other"); // jsdom's UA says nothing about Macs
  });
});

describe("the macOS defaults", () => {
  const byId = (id: string) => ACTIONS.find((a) => a.id === id)!;
  const macKeys = (id: string) => defaultKeys(byId(id), "mac");

  /**
   * The bug itself, stated as an invariant.
   *
   * macOS composes a character under Option, so `Alt+a` is matched against
   * `"å"` and never fires. Any binding that reaches a Mac still spelled
   * `Alt+<character>` is dead on arrival — this is what fails if a new
   * `Alt+…` action is added without a row in `MAC_KEYS`.
   */
  it("leave no Option chord spelled by character", () => {
    const dead = ACTIONS.flatMap((a) =>
      defaultKeys(a, "mac")
        .filter((k) => /(^|\+)Alt\+/.test(k) && /\+[a-z0-9]$/i.test(k))
        .map((k) => `${a.label} — ${k}`),
    );
    expect(dead).toEqual([]);
  });

  /** A conflict means one of the pair silently never fires, and which one
   * depends on map iteration order — the same test the shipped defaults have,
   * with `$mod` resolved the way tinykeys resolves it on a Mac. */
  it("bind no chord to two actions", () => {
    const seen = new Map<string, string>();
    const clashes: string[] = [];
    for (const a of ACTIONS) {
      for (const key of defaultKeys(a, "mac")) {
        // tinykeys turns `$mod` into Meta here, so `$mod+k` and `Meta+k` are
        // one chord and a table that used both would collide invisibly.
        const chord = key.replace("$mod", "Meta");
        const prev = seen.get(chord);
        if (prev) clashes.push(`${chord} is bound to both "${prev}" and "${a.label}"`);
        else seen.set(chord, a.label);
      }
    }
    expect(clashes).toEqual([]);
  });

  /**
   * macOS owns these before the webview is asked, and we `preventDefault`
   * everything we handle — so taking one would not read as a clash. It would
   * read as copy, or select-all, quietly ceasing to work in every text box.
   */
  it("leave the system's own chords alone", () => {
    const reserved = /^Meta\+[acvxzwhqm]$/;
    const taken = ACTIONS.flatMap((a) =>
      defaultKeys(a, "mac")
        .filter((k) => reserved.test(k))
        .map((k) => `${a.label} — ${k}`),
    );
    expect(taken).toEqual([]);
  });

  it("name only actions that exist", () => {
    const ids = new Set(ACTIONS.map((a) => a.id));
    expect(Object.keys(MAC_KEYS).filter((id) => !ids.has(id))).toEqual([]);
  });

  /** The decisions themselves, so a change to one is a change on purpose. */
  it("put the numbered strip on ⌘ and the rest where ⌘ is free", () => {
    expect(macKeys("dock.changes")).toEqual(["Meta+2"]);
    expect(macKeys("queue.focus")).toEqual(["Meta+1"]);
    expect(macKeys("rail.files")).toEqual(["Meta+f"]);
    expect(macKeys("rescan")).toEqual(["Meta+r"]);
  });

  it("keep ⌥ where ⌘ is spoken for, and say which", () => {
    // `⌘A` selects all, `⌘C` copies, `⌘W` closes the window, `⌘H` hides the
    // app, `⌘K` is our palette and `⌘0` is our reset-zoom.
    //
    // `rail.chat` is the one that holds `A` since 2026-08-28 — it inherited
    // the chord from `pane.agent` and, with it, the reason the chord cannot be
    // `⌘A`. The Agent pane moved to a letter where `⌘` is free.
    expect(macKeys("rail.chat")).toEqual(["Alt+KeyA"]);
    expect(macKeys("pane.agent")).toEqual(["Meta+g"]);
    expect(macKeys("pane.code")).toEqual(["Alt+KeyC"]);
    expect(macKeys("wall")).toEqual(["Alt+KeyW"]);
    expect(macKeys("health")).toEqual(["Alt+KeyH"]);
    expect(macKeys("keymap")).toEqual(["Alt+KeyK"]);
    expect(macKeys("layout.reset")).toEqual(["Alt+Digit0"]);
  });

  /** Arrows, `Escape`, `[`/`]` and the `$mod` family carry the same key on
   * both platforms — a second spelling would be two literals to drift. */
  it("leave alone what already worked", () => {
    expect(macKeys("pane.move.left")).toEqual(["Alt+Shift+ArrowLeft"]);
    expect(macKeys("focus.release")).toEqual(["Shift+Escape"]);
    expect(macKeys("palette")).toEqual(["$mod+k"]);
    expect(macKeys("queue.toggle")).toEqual(["["]);
  });

  /** Nothing about a Linux desktop changes. */
  it("change nothing off a Mac", () => {
    for (const a of ACTIONS) expect(defaultKeys(a, "other")).toEqual(a.keys);
  });
});

describe("recording a chord on a Mac", () => {
  /**
   * `⌥A` arrives as `"å"`. Recording that would produce a binding that works
   * until the keyboard layout changes and reads as line noise in the shortcuts
   * window; the physical key is recorded instead.
   */
  it("names the physical key under Option", () => {
    const opt = new KeyboardEvent("keydown", { key: "å", code: "KeyA", altKey: true });
    expect(chordFromEvent(opt, "mac")).toBe("Alt+KeyA");
    const shifted = new KeyboardEvent("keydown", { key: "Å", code: "KeyA", altKey: true, shiftKey: true });
    expect(chordFromEvent(shifted, "mac")).toBe("Alt+Shift+KeyA");
  });

  /** Only under Option — `⌘K` composes nothing, so it stays readable. */
  it("leaves every other chord as it was", () => {
    const cmd = new KeyboardEvent("keydown", { key: "k", code: "KeyK", metaKey: true });
    expect(chordFromEvent(cmd, "mac")).toBe("Meta+k");
    const linux = new KeyboardEvent("keydown", { key: "a", code: "KeyA", altKey: true });
    expect(chordFromEvent(linux, "other")).toBe("Alt+a");
  });
});

describe("showing a chord", () => {
  it("says what the keycaps say on a Mac", () => {
    expect(formatChord("Alt+KeyA", "mac")).toBe("⌥A");
    expect(formatChord("Meta+Shift+e", "mac")).toBe("⌘⇧E");
    expect(formatChord("$mod+k", "mac")).toBe("⌘K");
    expect(formatChord("Alt+Digit0", "mac")).toBe("⌥0");
  });

  /** `$mod` and `Equal` are spellings for tinykeys, not for a person. */
  it("resolves the spellings that are not keys anywhere", () => {
    expect(formatChord("$mod+k", "other")).toBe("Ctrl+K");
    expect(formatChord("$mod+Equal", "other")).toBe("Ctrl+=");
    expect(formatChord("Alt+Shift+ArrowLeft", "other")).toBe("Alt+Shift+ArrowLeft");
  });
});

/**
 * The half a table cannot check: that these chords reach an action through
 * tinykeys, from the events macOS actually sends.
 */
describe("a Mac pressing them", () => {
  beforeEach(() => {
    asPlatform("MacIntel");
    useStore.setState({
      prefs: defaultPrefs(),
      paletteOpen: false,
      showKeymap: false,
      showWall: false,
    });
    useStore.getState().setPrefs({ dock: null, infoOpen: false });
  });
  afterEach(() => asPlatform(""));

  function press(init: KeyboardEventInit) {
    window.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
  }

  it("opens a dock tool on ⌘2", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press({ key: "2", code: "Digit2", metaKey: true });
    expect(useStore.getState().prefs.dock).toBe("changes");
  });

  /** The composed character is the whole bug: this is `⌥W` as a Mac sends it. */
  it("opens the wall on ⌥W, composed character and all", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press({ key: "∑", code: "KeyW", altKey: true });
    expect(useStore.getState().showWall).toBe(true);
  });

  /** And the chord it did **not** take: `⌘W` still belongs to the window. */
  it("does not answer ⌘W", async () => {
    const { default: App } = await import("@/App");
    render(<App />);
    press({ key: "w", code: "KeyW", metaKey: true });
    expect(useStore.getState().showWall).toBe(false);
  });

  it("shows the chord in force, not the one that shipped", () => {
    const wall = ACTIONS.find((a) => a.id === "wall")!;
    expect(bindingsFor(wall, {}, "mac")).toEqual(["Alt+KeyW"]);
    expect(bindingsFor(wall, { wall: ["Meta+Shift+w"] }, "mac")).toEqual(["Meta+Shift+w"]);
  });
});
