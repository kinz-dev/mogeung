import { describe, expect, it } from "vitest";
import { ACTIONS, bindingsFor, chordFromEvent, rebind } from "./keymap";

function press(init: Partial<KeyboardEventInit> & { key: string }): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("chordFromEvent", () => {
  /** Holding Control is not a binding. Recording one the instant a modifier
   * goes down would make every chord impossible to enter. */
  it("waits through a bare modifier", () => {
    expect(chordFromEvent(press({ key: "Control", ctrlKey: true }))).toBeNull();
    expect(chordFromEvent(press({ key: "Alt", altKey: true }))).toBeNull();
    expect(chordFromEvent(press({ key: "Shift", shiftKey: true }))).toBeNull();
  });

  it("spells a chord the way tinykeys reads one", () => {
    expect(chordFromEvent(press({ key: "k", ctrlKey: true }))).toBe("Control+k");
    expect(chordFromEvent(press({ key: "4", altKey: true }))).toBe("Alt+4");
    expect(chordFromEvent(press({ key: "F12", altKey: true }))).toBe("Alt+F12");
    expect(chordFromEvent(press({ key: "ArrowDown" }))).toBe("ArrowDown");
  });

  /**
   * `Shift+$` is noise — the `$` *is* the shift, and tinykeys would then
   * require both. `Shift+Tab` is a real chord because `Tab` carries no shift of
   * its own.
   */
  it("only names Shift when the character does not already carry it", () => {
    expect(chordFromEvent(press({ key: "$", shiftKey: true }))).toBe("$");
    expect(chordFromEvent(press({ key: "Tab", shiftKey: true }))).toBe("Shift+Tab");
  });

  it("lower-cases a letter, so one chord has one spelling", () => {
    expect(chordFromEvent(press({ key: "C" }))).toBe("c");
  });
});

describe("rebind", () => {
  const idOf = (label: string) => ACTIONS.find((a) => a.label.includes(label))!.id;

  it("records a chord against an action", () => {
    const { next } = rebind({}, "palette", "Control+Shift+p");
    expect(next.palette).toEqual(["Control+Shift+p"]);
  });

  /**
   * A conflict steals rather than refuses, because a dialog saying "that key is
   * taken" sends you off to find the other binding when the thing you wanted
   * was this key on this action. What loses it is left **unbound** — an action
   * with no binding is visible in this window, whereas one silently moved to
   * some free key is a booby trap.
   */
  it("takes a chord from whoever held it, and says who", () => {
    const palette = idOf("Command palette");
    const { next, stoleFrom } = rebind({}, "rescan", "$mod+k");
    expect(next.rescan).toEqual(["$mod+k"]);
    expect(next[palette]).toEqual([]);
    expect(stoleFrom).toMatch(/palette/i);
  });

  it("reports nothing when the chord was free", () => {
    expect(rebind({}, "rescan", "Control+Alt+Shift+F9").stoleFrom).toBeNull();
  });

  /** An action absent from the overrides uses the shipped binding, so a default
   * that improves later still reaches someone who never touched it. */
  it("falls through to the default until you change it", () => {
    const palette = ACTIONS.find((a) => a.id === "palette")!;
    expect(bindingsFor(palette, {})).toEqual(palette.keys);
    expect(bindingsFor(palette, { palette: ["Control+j"] })).toEqual(["Control+j"]);
    // Explicitly emptied is *unbound*, not "use the default" — otherwise there
    // would be no way to turn a binding off.
    expect(bindingsFor(palette, { palette: [] })).toEqual([]);
  });
});

describe("the shipped defaults", () => {
  /**
   * The egui client has this test and it is worth having twice: a conflict in
   * the defaults means one of the pair silently never fires, and which one
   * depends on map iteration order. That is a bug you cannot reproduce.
   */
  it("bind no chord to two actions", () => {
    const seen = new Map<string, string>();
    const clashes: string[] = [];
    for (const a of ACTIONS) {
      for (const key of a.keys) {
        const prev = seen.get(key);
        if (prev) clashes.push(`${key} is bound to both "${prev}" and "${a.label}"`);
        else seen.set(key, a.label);
      }
    }
    expect(clashes).toEqual([]);
  });

  it("give every action a binding, or it can never be discovered", () => {
    expect(ACTIONS.filter((a) => a.keys.length === 0).map((a) => a.label)).toEqual([]);
  });
});
