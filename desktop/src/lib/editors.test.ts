/**
 * The editor a fold chord acts on. `R-J99`.
 *
 * Two ways this goes wrong, both silent: the chord folds a file in a pane the
 * keyboard is not in, or it is handed an editor its pane has since disposed.
 */

import { describe, expect, it } from "vitest";
import { editorIn, forgetEditor, noteEditor } from "./editors";

const fake = () => ({ getAction: () => null });

describe("the editor behind a pane", () => {
  it("answers for the active pane only", () => {
    const a = fake();
    const b = fake();
    noteEditor("file:s1::a.json", a);
    noteEditor("file:s1::b.json", b);
    expect(editorIn("file:s1::a.json")).toBe(a);
    expect(editorIn("file:s1::b.json")).toBe(b);
    // The keyboard is in the Agent pane: no editor, rather than the last one.
    expect(editorIn("agent")).toBeNull();
    expect(editorIn(null)).toBeNull();
  });

  it("is forgotten with its pane", () => {
    noteEditor("file:s1::c.json", fake());
    forgetEditor("file:s1::c.json");
    expect(editorIn("file:s1::c.json")).toBeNull();
  });
});
