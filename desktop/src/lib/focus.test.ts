/**
 * Getting the keyboard back. `R-B51`.
 *
 * The window's rule is that a **chord** always fires and a **bare** key belongs
 * to whatever has focus — which is why an agent can receive `j`, and why once
 * you click into a terminal there was no keyboard way out. Reported 2026-08-07,
 * with the observation that the capture itself is correct and what was missing
 * is a release.
 *
 * The subtle half is that this must be a *release* rather than a blur: pressing
 * it while the queue has focus should do nothing at all.
 */

import { beforeEach, describe, expect, it } from "vitest";
import {
  PANE_MOVE_CHORDS,
  RELEASE_CHORD,
  isReleaseChord,
  keyboardIsHeld,
  paneMoveDirection,
  releaseKeyboard,
} from "@/lib/focus";

function build(html: string): void {
  document.body.innerHTML = html;
}

const held = (id: string) => {
  document.getElementById(id)?.focus();
  return keyboardIsHeld();
};

beforeEach(() => {
  document.body.innerHTML = "";
});

/**
 * The chord took three attempts, and both earlier ones failed the same way:
 * claimed by the desktop, so the keystroke never arrived and nothing happened
 * — the hardest failure to diagnose from inside the app, because nothing is
 * wrong except that nothing happens.
 *
 * `Alt+Escape` is GNOME's *switch windows directly*; `Alt+Shift+Escape` is also
 * claimed on Ubuntu, and three keys is a bad ask mid-flow anyway. These pin the
 * two properties that matter now: the terminal and the keymap agree on one
 * definition, and **plain `Escape` still reaches the agent**.
 */
describe("the release chord", () => {
  const ev = (init: Partial<KeyboardEvent> & { key: string }) =>
    ({ type: "keydown", altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...init }) as KeyboardEvent;

  it("is one definition, shared by the keymap and the terminal", () => {
    expect(RELEASE_CHORD).toBe("Shift+Escape");
  });

  it("matches Shift+Escape", () => {
    expect(isReleaseChord(ev({ key: "Escape", shiftKey: true }))).toBe(true);
  });

  /**
   * The one that had to keep working. `Escape` is how you cancel a turn, and a
   * release that swallowed it would break the agent to fix the window.
   */
  it("leaves plain Escape alone", () => {
    expect(isReleaseChord(ev({ key: "Escape" }))).toBe(false);
  });

  /** The two bindings the desktop took. Neither may quietly still work. */
  it("is not the chords Ubuntu claimed", () => {
    expect(isReleaseChord(ev({ key: "Escape", altKey: true }))).toBe(false);
    expect(isReleaseChord(ev({ key: "Escape", altKey: true, shiftKey: true }))).toBe(false);
  });

  it("ignores keyup, so a release fires once", () => {
    expect(isReleaseChord(ev({ key: "Escape", shiftKey: true, type: "keyup" } as never))).toBe(false);
  });
});

/**
 * The move chords, and the reason the terminal has to swallow them too.
 *
 * An arrow reaches a TUI as a real escape sequence, so a move that also
 * forwarded it would walk the agent's menu on the way out — you would arrive at
 * the next pane having quietly typed into the one you left. Different mechanism
 * from the release chord, same lesson.
 */
describe("the pane-move chords", () => {
  const ev = (init: Partial<KeyboardEvent> & { key: string }) =>
    ({ type: "keydown", altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, ...init }) as KeyboardEvent;
  const move = (key: string, mods: Partial<KeyboardEvent> = {}) =>
    paneMoveDirection(ev({ key, altKey: true, shiftKey: true, ...mods }));

  it("reads all four arrows", () => {
    expect(move("ArrowLeft")).toBe("left");
    expect(move("ArrowRight")).toBe("right");
    expect(move("ArrowUp")).toBe("up");
    expect(move("ArrowDown")).toBe("down");
  });

  /** A bare arrow is the agent's — scrolling its history, walking its menu. */
  it("leaves a bare arrow to the agent", () => {
    expect(paneMoveDirection(ev({ key: "ArrowLeft" }))).toBeNull();
    expect(paneMoveDirection(ev({ key: "ArrowLeft", altKey: true }))).toBeNull();
    expect(paneMoveDirection(ev({ key: "ArrowLeft", shiftKey: true }))).toBeNull();
  });

  /** GNOME's `Ctrl+Alt+arrow` switches workspaces; it is not ours to take. */
  it("is not the workspace chord", () => {
    expect(move("ArrowLeft", { ctrlKey: true })).toBeNull();
    expect(move("ArrowLeft", { metaKey: true })).toBeNull();
  });

  it("keeps its binding strings where the keymap reads them", () => {
    expect(PANE_MOVE_CHORDS.left).toBe("Alt+Shift+ArrowLeft");
    expect(PANE_MOVE_CHORDS.down).toBe("Alt+Shift+ArrowDown");
  });

  it("ignores keyup, so one press is one move", () => {
    expect(paneMoveDirection(ev({ key: "ArrowLeft", altKey: true, shiftKey: true, type: "keyup" } as never))).toBeNull();
  });
});

describe("who is holding the keyboard", () => {
  it("says a terminal is", () => {
    build(`<div class="xterm"><textarea id="t"></textarea></div>`);
    expect(held("t")).toBe(true);
  });

  it("says an editor is", () => {
    build(`<div class="monaco-editor"><textarea id="m"></textarea></div>`);
    expect(held("m")).toBe(true);
  });

  it("says a text box is", () => {
    build(`<input id="i" />`);
    expect(held("i")).toBe(true);
  });

  /**
   * The queue list is a focusable `div`, and it must **not** count — bare keys
   * are how it is driven, and they already work there.
   */
  it("says an ordinary focusable element is not", () => {
    build(`<div id="q" tabindex="-1"></div>`);
    expect(held("q")).toBe(false);
  });

  it("says nothing is, when nothing has focus", () => {
    build(`<div></div>`);
    expect(keyboardIsHeld()).toBe(false);
  });
});

describe("releasing it", () => {
  /**
   * The host is an **ancestor** of the terminal, which is the whole trick:
   * `focusOwns` asks `closest(".xterm")`, and focus parked on an ancestor is
   * outside the terminal by that test — so every bare binding starts working
   * again without the window having to special-case anything.
   */
  it("parks focus on the pane, outside the terminal", () => {
    build(`
      <div id="host" tabindex="-1" data-focus-host>
        <div class="xterm"><textarea id="t"></textarea></div>
      </div>
    `);
    document.getElementById("t")?.focus();
    expect(keyboardIsHeld()).toBe(true);

    releaseKeyboard();

    expect(document.activeElement?.id).toBe("host");
    expect(keyboardIsHeld()).toBe(false);
  });

  /** No host to land on is still a release — a blur alone fixes bare keys. */
  it("falls back to letting go entirely", () => {
    build(`<div class="xterm"><textarea id="t"></textarea></div>`);
    document.getElementById("t")?.focus();

    releaseKeyboard();

    expect(keyboardIsHeld()).toBe(false);
  });

  it("does nothing when nothing has focus", () => {
    build(`<div></div>`);
    expect(() => releaseKeyboard()).not.toThrow();
  });
});
