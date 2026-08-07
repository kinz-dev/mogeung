/**
 * The keyboard.
 *
 * `A13` — the user drives by keyboard and reaches for a palette before a menu —
 * is `SUPPORTED`, and it is why every action has a name and a binding rather
 * than only a button.
 *
 * The bindings are the ones the egui client trained, kept deliberately: a
 * binding that moves under trained hands costs more than a mnemonic that lags a
 * rename, and the two clients will run side by side until this one reaches
 * parity.
 */

import { useEffect, type RefObject } from "react";
import { tinykeys } from "tinykeys";
import type { DockviewApi } from "dockview";
import { togglePaneHold, useStore } from "@/store";
import { exportFilename, exportPayload, type DockTool, type RailTool } from "@/store/prefs";
import { exportText } from "@/lib/tauri";
import { focusPane, movePane, resetLayout, splitAgent } from "@/lib/panes";
import { nudgeAppZoom } from "@/lib/zoom";
import { PANE_MOVE_CHORDS, RELEASE_CHORD, keyboardIsHeld, releaseKeyboard } from "@/lib/focus";
import type { Direction } from "@/lib/spatial";

/** The id the Attention list carries, so `Alt+1` has something to focus. */
export const QUEUE_LIST_ID = "queue-list";

export interface Action {
  id: string;
  label: string;
  group: string;
  /** tinykeys syntax. The keymap window shows this verbatim. */
  keys: string[];
  run: (dock: DockviewApi | null) => void;
}

const pane = (id: string, title: string, keys: string[]): Action => ({
  id: `pane.${id}`,
  label: `Show the ${title} pane`,
  group: "Panes",
  keys,
  run: (dock) => focusPane(dock, id, title),
});

/**
 * A bottom-dock tool. Same one-key-both-ways rule as the rail: the chord that
 * opens it closes it, so reaching a thing and leaving it are one gesture.
 */
const dockTool = (tool: DockTool, label: string, keys: string[]): Action => ({
  id: `dock.${tool}`,
  label: `Show ${label}`,
  group: "Dock",
  keys,
  run: () => {
    const { prefs, setPrefs } = useStore.getState();
    setPrefs({ dock: prefs.dock === tool ? null : tool });
  },
});

const rail = (tool: RailTool, label: string, keys: string[]): Action => ({
  id: `rail.${tool}`,
  label: `Show ${label} in the rail`,
  group: "Rail",
  keys,
  run: () => {
    const { prefs, setPrefs } = useStore.getState();
    // One key both ways — reaching a thing and leaving it should not be two
    // chords to learn.
    setPrefs({ rail: prefs.rail === tool ? null : tool });
  },
});

export const ACTIONS: Action[] = [
  // **Numbered, left to right along the dock strip**, since 2026-08-06.
  //
  // The mnemonics went because one of them was actively harmful: `Alt+T` is
  // Claude Code's own *toggle thinking*, and a chord always fires in the window
  // — `focusOwns` only defers bare keys to a focused terminal — so pressing it
  // over the Agent pane opened the Transcript instead of reaching the agent.
  // A binding that steals a key from the program the pane exists to show is a
  // binding in the wrong place, whatever it spells.
  //
  // Numbers cannot collide that way and they carry an order the letters never
  // did: the digit is the tool's position in the strip. Git keeps `Alt+9` and
  // sits at the far right — a rightmost thing wearing the highest digit.
  dockTool("changes", "Changes", ["Alt+2"]),
  dockTool("transcript", "Transcript", ["Alt+3"]),
  pane("agent", "Agent", ["Alt+a"]),
  pane("code", "Code", ["Alt+c"]),

  // Two agents at once. `R-B49`.
  //
  // `Alt+Shift+…` on both, beside `Alt+A`, because these are things you do
  // *to* the Agent pane rather than places you go. The hold has no bare-letter
  // binding on purpose: it is the one action here you can leave switched on and
  // forget, and a key that easy to hit by accident would produce exactly the
  // failure this feature has to avoid — a pane that has quietly stopped
  // following the queue.
  {
    id: "pane.agent.split",
    label: "Another Agent pane, beside this one",
    group: "Panes",
    keys: ["Alt+Shift+a"],
    run: () => splitAgent(),
  },
  {
    id: "pane.agent.hold",
    label: "Hold this Agent pane on its session",
    group: "Panes",
    keys: ["Alt+Shift+h"],
    run: (dock) => {
      // The pane the keyboard is aimed at, which dockview already tracks as
      // the active panel — the same thing the focus ring draws.
      const id = dock?.activePanel?.id;
      if (id) togglePaneHold(id);
    },
  },
  // Moving between panes, spatially. `R-B52`.
  //
  // Asked for 2026-08-07 with two agents up: *"move the focus to the left
  // claude session on screen"*. The arrows are the obvious spelling and the
  // `Alt+Shift+…` family is where the pane actions already live. GNOME takes
  // `Super+arrow` for tiling and `Ctrl+Alt+arrow` for workspaces; neither of
  // those is this.
  //
  // **The terminal swallows these**, and for a reason the release chord did not
  // have: an arrow reaches a TUI as a real escape sequence, so a move that also
  // forwarded it would walk the agent's menu on the way out — you would arrive
  // at the next pane having typed into the one you left.
  ...(["left", "right", "up", "down"] as Direction[]).map((dir) => ({
    id: `pane.move.${dir}`,
    label: `Focus the pane to the ${dir}`,
    group: "Panes",
    keys: [PANE_MOVE_CHORDS[dir]],
    run: () => movePane(dir),
  })),
  {
    id: "focus.release",
    label: "Give the keyboard back to the window",
    group: "Navigation",
    /**
     * `Alt+Escape`, and it is the only binding here that exists because of the
     * rule rather than in spite of it. A chord already fires from a focused
     * pane; a **bare** key does not, correctly — `focusOwns` gives those to
     * whatever has focus, which is what lets an agent receive `j`. This is the
     * way back out, so that rule can stay as strict as it should be.
     *
     * The terminal catches this chord itself, before xterm forwards anything,
     * because an Escape-bearing chord reaches a TUI as `ESC ESC` — Claude Code
     * reads that as a cancel, and a release that also interrupted the agent
     * would be a worse bug than the one it fixes. This entry is what makes it
     * discoverable in the palette and the keymap window, and what makes it work
     * from Monaco and from a filter box too.
     *
     * The chord took three attempts and both earlier ones failed the same way
     * — claimed by the desktop, so the keystroke never arrived and nothing
     * happened. `RELEASE_CHORD` in `focus.ts` records which and why.
     */
    keys: [RELEASE_CHORD],
    // Guarded, so this is a release rather than a blur. Without the test it
    // would take focus off the queue list or a filter box you were happily
    // using and hand it to nothing, which is a way of making the window feel
    // haunted by a key you pressed for an unrelated reason.
    run: () => {
      if (keyboardIsHeld()) releaseKeyboard();
    },
  },
  {
    id: "prefs.export",
    label: "Export your labels, tags and pins to a file",
    group: "Windows",
    /**
     * The other half of [ADR-0023](../../../docs/decisions/0023-judgements-stay-in-the-client.md).
     *
     * That ADR kept your judgements in the client and named what it was
     * accepting: they live in a webview's `localStorage`, so clearing it loses
     * every label, tag and pin silently, with nothing to restore from.
     *
     * `Alt+Shift+E` rather than a prime chord, because this is run twice a year
     * — it has a binding at all only because every action here must
     * (`rebind.test.ts` asserts it), and the palette is how it will actually be
     * found.
     */
    keys: ["Alt+Shift+e"],
    run: () => {
      const { prefs, pushError } = useStore.getState();
      const now = new Date();
      void exportText(exportFilename(now), JSON.stringify(exportPayload(prefs, now), null, 2))
        .then((path) => useStore.getState().pushNotice(`preferences written to ${path}`))
        .catch((e) => pushError(`could not export preferences: ${String(e)}`));
    },
  },
  {
    id: "wall",
    label: "The wall — every session at once",
    group: "Navigation",
    // `Alt+W` for wall, and a **toggle** rather than hold-to-peek. The hold
    // gesture was the nicer design and lost to a real constraint: the centre is
    // usually an xterm, which handles keys aggressively, and a peek whose keyup
    // never arrives is a wall stuck open. One key both ways, Esc as well.
    keys: ["Alt+w"],
    run: () => {
      const { showWall } = useStore.getState();
      useStore.setState({ showWall: !showWall });
    },
  },
  {
    id: "layout.reset",
    label: "Reset the pane layout",
    group: "Panes",
    // Deliberately not adjacent to anything pressed often: this discards an
    // arrangement you may have spent a while on. Same binding the egui client
    // gave it, for hands that learnt it there.
    keys: ["Alt+0"],
    run: () => resetLayout(),
  },

  // The rest of the strip, in the order it is drawn.
  dockTool("insight", "Insight", ["Alt+4"]),
  dockTool("debt", "Debt", ["Alt+5"]),
  dockTool("git", "Git", ["Alt+9"]),

  {
    id: "info.toggle",
    label: "Show Info — about the selected session",
    group: "Dock",
    // Same chord it had in the bottom dock. It moved under the queue because
    // it is about the row you just clicked, not because it changed job.
    keys: ["Alt+o"],
    run: () => {
      const { prefs, setPrefs } = useStore.getState();
      setPrefs({ infoOpen: !prefs.infoOpen });
    },
  },

  rail("files", "the worktree", ["Alt+f"]),
  rail("search", "global search", ["Alt+s"]),
  rail("notes", "notes", ["Alt+n"]),
  rail("bookmarks", "bookmarks", ["Alt+b"]),

  {
    id: "queue.focus",
    label: "Focus the Attention list",
    group: "Navigation",
    keys: ["Alt+1"],
    run: () => {
      const { prefs, setPrefs, queue, sessions, selected, select } = useStore.getState();
      // Focusing something that is collapsed to a strip is not focusing it.
      if (prefs.queueCollapsed) setPrefs({ queueCollapsed: false });
      // Nothing selected yet? Take the top of the queue, so the first arrow
      // press moves *from* somewhere rather than jumping to the top.
      if (!selected) {
        const first = queue.find((q) => sessions[q.session_id]);
        if (first) select(first.session_id);
      }
      // After the render that expanding may have caused.
      requestAnimationFrame(() => document.getElementById(QUEUE_LIST_ID)?.focus());
    },
  },
  {
    id: "queue.toggle",
    label: "Collapse or expand the queue",
    group: "Panes",
    keys: ["["],
    run: () => {
      const { prefs, setPrefs } = useStore.getState();
      setPrefs({ queueCollapsed: !prefs.queueCollapsed });
    },
  },
  {
    id: "rail.toggle",
    label: "Collapse or expand the right rail",
    group: "Panes",
    // `]` for the right edge, `[` for the left. The brackets point the way the
    // panels move, which is the whole reason to spend two of them.
    keys: ["]"],
    run: () => {
      const { prefs, setPrefs } = useStore.getState();
      setPrefs({ rail: prefs.rail ? null : "files" });
    },
  },
  {
    id: "palette",
    label: "Command palette — every action by name",
    group: "Windows",
    keys: ["$mod+k"],
    run: () => useStore.setState({ paletteOpen: true }),
  },
  {
    id: "goto.file",
    label: "Go to file by name",
    group: "Editor",
    keys: ["$mod+p"],
    run: () => useStore.setState({ paletteOpen: true, paletteMode: "files" }),
  },
  {
    id: "search.overlay",
    label: "Search everything — this conversation, this session's files, every session",
    group: "Editor",
    // The chord every editor taught for "find in files". It opens the overlay
    // rather than the rail: this is the ask-and-go one, and `Alt+S` is the
    // keep-it-open one.
    keys: ["$mod+Shift+f"],
    run: () => useStore.setState({ searchOpen: true }),
  },
  {
    id: "terminal.toggle",
    label: "Show or hide the terminal panel — your own shells",
    group: "Panes",
    // What the editors people arrive here from already bound it to: Alt+F12 is
    // IntelliJ's, Ctrl+` is VS Code's. A shell is the one pane that arrives
    // with an existing reflex attached, so it does not get a bare letter.
    keys: ["Control+`", "Alt+F12"],
    run: () => useStore.setState({ showTerminal: !useStore.getState().showTerminal }),
  },
  {
    id: "terminal.focus_app",
    label: "Raise the terminal application this session runs in",
    group: "Navigation",
    keys: ["Alt+Shift+o"],
    run: () => {
      const { selected, send } = useStore.getState();
      if (selected) send({ cmd: "focus_terminal", session_id: selected });
    },
  },
  {
    id: "zoom.in",
    label: "Zoom in",
    group: "View",
    // `Equal`/`Minus` are key *codes*, so they match whether or not Shift is
    // held — `Ctrl++` and `Ctrl+=` are the same gesture to everyone but the
    // keyboard.
    keys: ["$mod+Equal", "$mod+Shift+Equal"],
    run: () => nudgeAppZoom(1),
  },
  {
    id: "zoom.out",
    label: "Zoom out",
    group: "View",
    keys: ["$mod+Minus"],
    run: () => nudgeAppZoom(-1),
  },
  {
    id: "zoom.reset",
    label: "Reset zoom",
    group: "View",
    keys: ["$mod+Digit0"],
    run: () => nudgeAppZoom(0),
  },
  {
    id: "zoom.reset.panes",
    label: "Reset every pane's zoom",
    group: "View",
    keys: ["$mod+Shift+Digit0"],
    run: () => {
      // Per-pane factors multiply with the window's, so "everything looks
      // wrong" needs one key that clears the lot rather than a hunt.
      const { setPrefs } = useStore.getState();
      setPrefs({ zoom: {} });
    },
  },
  {
    id: "rescan",
    label: "Rescan for sessions now",
    group: "Windows",
    keys: ["Alt+r"],
    run: () => useStore.getState().send({ cmd: "rescan" }),
  },
  {
    id: "health",
    label: "What mogeung can and cannot see",
    group: "Windows",
    keys: ["Alt+h"],
    run: () => {
      useStore.getState().send({ cmd: "fetch_health" });
      useStore.setState({ showHealth: true });
    },
  },
  {
    id: "keymap",
    label: "Keyboard shortcuts",
    group: "Windows",
    keys: ["Alt+k"],
    run: () => useStore.setState({ showKeymap: true }),
  },
  {
    id: "theme",
    label: "Cycle the theme",
    group: "Windows",
    // Displaced by "show the Transcript". The mnemonic is worth keeping, and a
    // theme is a once-in-a-while act where a pane is a constant one — so the
    // constant gesture gets the cheaper chord.
    keys: ["Alt+Shift+t"],
    run: () => {
      const order = ["dark", "light", "system"] as const;
      const { prefs, setPrefs } = useStore.getState();
      setPrefs({ theme: order[(order.indexOf(prefs.theme) + 1) % order.length] });
    },
  },
  {
    id: "sync",
    label: "Update remote-tracking refs (git fetch)",
    group: "Git",
    keys: ["$mod+t"],
    run: () => {
      const { selected, send, pushError } = useStore.getState();
      if (!selected) {
        pushError("pick a session first — a fetch needs to know which repo");
        return;
      }
      send({ cmd: "git_fetch", session_id: selected });
    },
  },
  {
    id: "queue.next",
    label: "Next session in the queue",
    group: "Navigation",
    keys: ["j", "ArrowDown"],
    run: () => moveSelection(1),
  },
  {
    id: "queue.prev",
    label: "Previous session in the queue",
    group: "Navigation",
    keys: ["k", "ArrowUp"],
    run: () => moveSelection(-1),
  },
];

function moveSelection(delta: number): void {
  const { queue, sessions, selected, select } = useStore.getState();
  const ids = queue.map((q) => q.session_id).filter((id) => sessions[id]);
  if (ids.length === 0) return;
  const at = selected ? ids.indexOf(selected) : -1;
  const next = at < 0 ? 0 : Math.min(ids.length - 1, Math.max(0, at + delta));
  select(ids[next]);
}

/**
 * The chord a key press should be recorded as, in tinykeys spelling.
 *
 * `null` for a bare modifier — holding Control is not a binding, and treating
 * one as a chord means the recorder fires the instant you reach for a key.
 *
 * The key half prefers `e.key`, because that is what the shipped bindings are
 * written in and what a person recognises: `4`, `ArrowDown`, `F12`. tinykeys
 * matches `e.key` first, so a recorded chord and a hand-written one behave
 * identically.
 */
export function chordFromEvent(e: KeyboardEvent): string | null {
  if (["Control", "Alt", "Shift", "Meta", "OS"].includes(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Meta");
  // Shift only when the character does not already carry it. `Shift+$` is
  // noise — the `$` *is* the shift. `Shift+Tab` is not.
  if (e.shiftKey && e.key.length > 1) parts.push("Shift");
  parts.push(e.key.length === 1 ? e.key.toLowerCase() : e.key);
  return parts.join("+");
}

/** The bindings in force for an action: the user's, if they set any, else ours. */
export function bindingsFor(action: Action, overrides: Record<string, string[]>): string[] {
  const own = overrides[action.id];
  return own ? own : action.keys;
}

/**
 * Record a chord against an action, taking it from whoever holds it.
 *
 * Stealing rather than refusing, and **saying so** — the egui client does the
 * same. A conflict dialog makes you go and find the other binding first, when
 * the thing you wanted was this key on this action. What loses the chord is
 * left **unbound** rather than shuffled onto some free key nobody chose: an
 * action with no binding is discoverable in this window, whereas one silently
 * moved to `Alt+J` is a booby trap.
 *
 * Returns whatever lost it, for the sentence the window shows.
 */
export function rebind(
  overrides: Record<string, string[]>,
  actionId: string,
  chord: string,
): { next: Record<string, string[]>; stoleFrom: string | null } {
  const next = { ...overrides };
  let stoleFrom: string | null = null;
  for (const other of ACTIONS) {
    if (other.id === actionId) continue;
    const held = bindingsFor(other, next);
    if (!held.includes(chord)) continue;
    stoleFrom = other.label;
    next[other.id] = held.filter((k) => k !== chord);
  }
  next[actionId] = [chord];
  return { next, stoleFrom };
}

/**
 * Keys a focused code viewer legitimately owns.
 *
 * Moving and finding. Everything else — the bare letters that switch panes —
 * belongs to the window, because a **read-only** editor has no use for them.
 */
const VIEWER_KEYS = new Set([
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Enter",
  "Escape",
  "Tab",
]);

/**
 * Does whatever has focus own this key, or does the window?
 *
 * Three surfaces, three answers, and the middle one is the bug this replaced:
 *
 * - **A terminal owns everything.** Every byte belongs to the pty; a terminal
 *   that swallows shortcuts is one you cannot run a terminal program in. The
 *   chords we deliberately steal are chords, so they never reach here.
 * - **A code viewer owns only navigation.** Monaco keeps a hidden `<textarea>`
 *   for selection and clipboard, so the old "is the active element a textarea"
 *   test read the Code pane as *typing* and killed every bare-letter shortcut
 *   in the window for as long as it had focus. It is read-only (ADR-0019):
 *   pressing `c` there cannot mean text, so it means Changes. **If a Monaco
 *   here ever becomes editable, this has to become a per-editor check.**
 * - **A real text box owns everything bare.** Typing `c` into the queue filter
 *   must not throw you into another pane.
 */
function focusOwns(key: string): boolean {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return false;
  if (el.closest(".xterm")) return true;
  if (el.closest(".monaco-editor")) return VIEWER_KEYS.has(key);
  const tag = el.tagName.toLowerCase();
  return tag === "input" || tag === "textarea" || el.isContentEditable;
}

export function useKeymap(dock: RefObject<DockviewApi | null>): void {
  // Subscribed, so a rebind takes effect the moment it is recorded rather than
  // at the next reload — the window you rebind in is the window you test it in.
  const overrides = useStore((s) => s.prefs.keymap);
  useEffect(() => {
    const map: Record<string, (e: KeyboardEvent) => void> = {};
    for (const action of ACTIONS) {
      for (const key of bindingsFor(action, overrides)) {
        map[key] = (e: KeyboardEvent) => {
          // A bare key may belong to whatever has focus; for a bare binding the
          // binding string *is* the key name, so it can be asked directly.
          // Chords always fire — Ctrl+K has to open the palette from inside a
          // filter box, or the palette is unreachable exactly when you want it.
          const bare = !/\+/.test(key);
          if (bare && focusOwns(key)) return;
          e.preventDefault();
          action.run(dock.current);
        };
      }
    }
    // **Capture**, not bubble. Monaco and xterm both stop propagation on keys
    // they handle, so a listener on the way *up* never sees them — which is a
    // window whose shortcuts die the moment you click into a pane. Listening on
    // the way down means `focusOwns` above is the only thing deciding who gets
    // a key, rather than whichever component called `stopPropagation` first.
    return tinykeys(window, map, { capture: true });
  }, [dock, overrides]);
}
