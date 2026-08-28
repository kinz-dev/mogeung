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
import { currentPlatform, type Platform } from "@/lib/platform";
import { togglePaneHold, useStore } from "@/store";
import { exportFilename, exportPayload, type DockTool, type RailTool } from "@/store/prefs";
import { toggleRail } from "@/lib/rail";
import { cursorIn, markFor, setMnemonic, MNEMONICS } from "@/lib/marks";
import { openFile } from "@/lib/explorer";
import { exportText } from "@/lib/tauri";
import { addAgentPane, focusPane, movePane, parseFilePaneId, resetLayout, showFilePane } from "@/lib/panes";
import { closeFile } from "@/lib/explorer";
import { fullPath } from "@/ui/PaneChrome";
import { writeClipboard } from "@/lib/clipboard";
import { nudgeAppZoom } from "@/lib/zoom";
import { visibleQueue } from "@/lib/queue";
import { PANE_MOVE_CHORDS, RELEASE_CHORD, keyboardIsHeld, releaseKeyboard } from "@/lib/focus";
import type { Direction } from "@/lib/spatial";

/** The id the Attention list carries, so `Alt+1` has something to focus. */
export const QUEUE_LIST_ID = "queue-list";

export interface Action {
  id: string;
  label: string;
  group: string;
  /**
   * tinykeys syntax, and **the binding everywhere that is not a Mac**. A Mac
   * takes `MAC_KEYS[id]` when there is one; `defaultKeys` is the only thing
   * that should read this field directly.
   */
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
    // chords to learn. Since `R-J33` the opening half **adds**: the chord no
    // longer takes away the tool you were already reading.
    setPrefs({ rail: toggleRail(prefs.rail, tool) });
  },
});

/**
 * The nine numbered bookmarks. `R-J37`.
 *
 * Two actions per digit, and they are deliberately not symmetrical about where
 * they can be used. **Setting** needs a file and a line, so it refuses unless
 * the pane the keyboard is in *is* a file pane — a chord fires wherever focus
 * is, including over a terminal, and `R-B47` is the standing reminder that a
 * chord this window claims is one it takes from whatever is underneath.
 * **Jumping** works from anywhere, which is the entire point of a mnemonic:
 * you press it because you are somewhere else.
 *
 * Both say something when they cannot act. A silent no-op on a chord is
 * indistinguishable from a binding that is broken — `R-J5`'s argument about
 * empty states, applied to a keystroke.
 */
const setMark = (d: string): Action => ({
  id: `mark.set.${d}`,
  label: `Set bookmark ${d} on this line`,
  group: "Bookmarks",
  keys: [`$mod+Shift+Digit${d}`],
  run: () => {
    const { activePane, scoped, setScoped, pushNotice } = useStore.getState();
    const at = cursorIn(activePane);
    if (!at) {
      pushNotice(`bookmark ${d} needs a line — put the cursor in a file first`);
      return;
    }
    const next = setMnemonic(scoped().bookmarks, at.session, at.path, at.line, d);
    setScoped({ bookmarks: next });
    pushNotice(
      markFor(next, d)
        ? `bookmark ${d} — ${at.path.slice(at.path.lastIndexOf("/") + 1)}:${at.line}`
        : `bookmark ${d} cleared`,
    );
  },
});

const goMark = (d: string): Action => ({
  id: `mark.go.${d}`,
  label: `Go to bookmark ${d}`,
  group: "Bookmarks",
  keys: [`$mod+Digit${d}`],
  run: () => {
    const { scoped, pushNotice } = useStore.getState();
    const mark = markFor(scoped().bookmarks, d);
    if (!mark) {
      pushNotice(`no bookmark ${d} yet — set one with the same chord and Shift`);
      return;
    }
    const [session, path, line] = mark;
    // The same door a search hit and a diff row already use: it opens the
    // file, pins it, raises the pane and asks for the line.
    openFile(session, path, { pin: true, line });
  },
});

/** What `]` put away, so pressing it again brings the same stack back. */
let collapsed: RailTool[] = ["files"];

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
  {
    id: "pane.code",
    label: "Focus the newest open file",
    group: "Panes",
    /**
     * `Alt+C` outlived the Code pane. `R-B53`.
     *
     * There is no Code pane to show any more — a file is a pane of its own —
     * but the binding is trained, and a key that silently stops working is
     * worse than one that has been repointed. It now raises the most recently
     * opened file, which is what "show me the code" meant in practice.
     */
    keys: ["Alt+c"],
    run: () => {
      const { selected, explorer } = useStore.getState();
      const open = selected ? explorer[selected]?.open : undefined;
      const last = open?.[open.length - 1];
      if (selected && last) showFilePane(selected, last.path, last.rev);
    },
  },

  {
    id: "file.copy_path",
    label: "Copy the full path of the file you are reading",
    group: "Panes",
    /**
     * `Ctrl+Alt+Shift+C`, and `⌘⌥C` on a Mac. `R-J24`.
     *
     * **Not `Ctrl+Shift+C`**, which is the obvious spelling and is already
     * spoken for: it is the terminal's copy-the-selection chord
     * (`clipboard.ts`), and a chord this window claims is one it
     * `preventDefault`s — so binding it here would take copy away from every
     * embedded terminal. That is `R-B47`'s lesson (`Alt+T` was Claude Code's
     * own *toggle thinking*) applied before it could be re-learnt.
     *
     * The Mac chord is not a translation of the Linux one but the platform's
     * own: `⌘⌥C` is what Finder binds to *copy the path of the selection*, so
     * the hand that knows it already knows this.
     */
    keys: ["Control+Alt+Shift+c"],
    run: (dock) => {
      const id = dock?.activePanel?.id;
      const ref = id ? parseFilePaneId(id) : null;
      if (!ref) return;
      const { sessions, pushNotice, pushError } = useStore.getState();
      const owner = sessions[ref.session];
      const path = fullPath(owner ? owner.repo_root || owner.cwd : null, ref.path);
      void writeClipboard(path)
        .then(() => pushNotice(`full path copied — ${path}`))
        .catch((e) => pushError(`could not copy: ${String(e)}`));
    },
  },

  {
    id: "file.close",
    label: "Close the file you are reading",
    group: "Panes",
    /**
     * `Ctrl+F4`, asked for 2026-08-07 — the chord that closes a document in
     * every Windows application and in most Linux desktops. Spelled
     * `Control+F4` rather than `$mod+F4` on purpose: `$mod` is Meta on macOS,
     * and `Cmd+F4` is not that platform's close-a-document key either. Someone
     * on a Mac is better served rebinding this to `Cmd+w` than being given a
     * chord neither platform uses.
     *
     * **Only a file pane**, which is why this reads the active panel rather
     * than closing whatever is forward. `PaneTab` has the same rule and the
     * same reason: every other pane in the centre is permanent, because a pane
     * you can lose is a pane you have to rediscover — and an Agent pane's close
     * is a *detach*, which is not what anyone presses a close-the-document
     * chord for. With an agent forward this does nothing at all.
     *
     * It closes the *file*, not just the pane: `closeFile` drops the tab from
     * the session's explorer state as well, so `Alt+C` does not then raise a
     * file you have just closed.
     */
    keys: ["Control+F4"],
    run: (dock) => {
      const id = dock?.activePanel?.id;
      const ref = id ? parseFilePaneId(id) : null;
      if (ref) closeFile(ref.session, ref.path, ref.rev);
    },
  },

  // Two agents at once. `R-B49`, arriving as a tab since `R-J44`.
  //
  // `Alt+Shift+…` on both, beside `Alt+A`, because these are things you do
  // *to* the Agent pane rather than places you go. The hold has no bare-letter
  // binding on purpose: it is the one action here you can leave switched on and
  // forget, and a key that easy to hit by accident would produce exactly the
  // failure this feature has to avoid — a pane that has quietly stopped
  // following the queue.
  {
    // **The id stays `…split` although the pane is a tab now** (`R-J44`).
    // `prefs.keymap` is keyed by action id, so renaming it here would silently
    // drop the binding of anyone who had rebound this one — a cost paid by the
    // few users who customised, to fix a word only the source ever shows.
    id: "pane.agent.split",
    label: "Another Agent pane, as a tab here",
    group: "Panes",
    keys: ["Alt+Shift+a"],
    run: () => addAgentPane(),
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
  dockTool("run", "Run", ["Alt+4"]),
  dockTool("debt", "Debt", ["Alt+5"]),
  dockTool("insight", "Insight", ["Alt+8"]),
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

  // `R-J37`. Nine of each, generated in one place so the palette lists them
  // in order rather than in whatever order eighteen literals were typed.
  ...MNEMONICS.flatMap((d) => [setMark(d), goMark(d)]),

  rail("files", "the worktree", ["Alt+f"]),
  rail("search", "global search", ["Alt+s"]),
  rail("notes", "notes", ["Alt+n"]),
  rail("bookmarks", "bookmarks", ["Alt+b"]),
  rail("chat", "the chat panel", ["Alt+j"]),

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
      // Collapsing remembers the stack, so `]` twice gets you back what you
      // had rather than Files alone. Deliberately **not** a preference: it is
      // about the gesture you just made, and a remembered stack restored after
      // a restart would reopen tools you had closed before quitting.
      if (prefs.rail.length > 0) {
        collapsed = prefs.rail;
        setPrefs({ rail: [] });
      } else {
        setPrefs({ rail: collapsed });
      }
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
    id: "session.label",
    // Named for the noun the dialog, the badge and the filter all use. The
    // palette searches `group + label`, so "rename" is the word it will not
    // find — accepted, because "label" is the one this window taught.
    label: "Label the selected session",
    group: "Session",
    /**
     * `F2`, asked for 2026-08-19 — the rename key every file manager and IDE
     * has trained, pointed at the one thing in this window you name yourself.
     *
     * The label dialog existed only on the queue row's context menu, so naming
     * a session meant right-click, read a menu, click. A session you triage by
     * is a session you name early and often, and a menu is the wrong cost for
     * that.
     *
     * **A bare key, so a focused terminal keeps it.** `focusOwns` hands bare
     * keys to whatever has focus, which is the rule that lets an agent receive
     * `j` — and `F2` is a key a TUI may well want. With the queue focused, or
     * the window generally, it fires; clicked into an Agent pane it reaches the
     * agent, and `Shift+Escape` is the way back out. Same key on a Mac, so
     * there is no `MAC_KEYS` row: a function key composes no character.
     *
     * It names the **selected** session rather than the one a held pane is
     * showing. The dialog is the queue's, the selection is what the queue
     * highlights, and a rename that renamed something other than the row you
     * are looking at would be the worse surprise.
     */
    keys: ["F2"],
    run: () => {
      const { selected, sessions, pushError } = useStore.getState();
      if (!selected || !sessions[selected]) {
        pushError("pick a session first — a label needs something to name");
        return;
      }
      useStore.setState({ labelEditing: selected });
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

/**
 * What a Mac gets instead, by action id. `R-J19`.
 *
 * **The rule: `⌘` where `⌘` is free, `⌥` where it is not** — and where it is
 * not, the binding is respelled by *physical key* (`Alt+KeyA`, never `Alt+a`),
 * which is the only spelling of an Option chord that fires on macOS at all.
 * `platform.ts` explains why: Option composes a character rather than
 * modifying one, so `Alt+a` is matched against `"å"` and quietly never runs.
 * That is the bug this table exists for — every `Alt+<letter>` and
 * `Alt+<digit>` binding we ship is dead on a Mac today.
 *
 * **The exceptions are not taste.** `⌘` is already the modifier macOS and this
 * window's own `$mod` family speak for, so a blanket `Alt → ⌘` would take:
 *
 * | chord | already belongs to |
 * | ----- | ------------------ |
 * | `⌘A` `⌘C` | select all, copy — Tauri's default macOS menu owns these before the webview is asked |
 * | `⌘W` `⌘H` | close the window, hide the application |
 * | `⌘K` | our own command palette (`$mod+k`) |
 * | `⌘0` | our own reset zoom (`$mod+Digit0`) |
 *
 * A chord we handle is a chord we `preventDefault`, so taking any of those
 * would not be a clash you notice and route around — it would be copy, or
 * select-all, silently ceasing to work in every text box in the window.
 *
 * **What is absent from this table is deliberate too.** Arrows, `F12`,
 * `Escape`, `[`, `]`, `j`/`k` and the whole `$mod` family already carry the
 * same key on both platforms — `Alt+Shift+ArrowLeft` works on a Mac because an
 * arrow has no character to compose, and tinykeys turns `$mod` into `⌘` by
 * itself. Adding rows for those would be two spellings of one binding, which
 * is the drift `RELEASE_CHORD` exists to avoid.
 *
 * Every row is a *default*. A Mac that disagrees costs a rebind (`R-B12`),
 * not a patch.
 */
export const MAC_KEYS: Record<string, string[]> = {
  // Digits: free on macOS, and they still read as the strip they number.
  "queue.focus": ["Meta+1"],
  "dock.changes": ["Meta+2"],
  "dock.transcript": ["Meta+3"],
  "dock.run": ["Meta+4"],
  "dock.debt": ["Meta+5"],
  "dock.insight": ["Meta+8"],
  "dock.git": ["Meta+9"],
  // `⌘0` is reset-zoom, so the digit keeps its `⌥` — respelled to fire.
  "layout.reset": ["Alt+Digit0"],

  // Letters where `⌘` is free.
  "rail.files": ["Meta+f"],
  "rail.search": ["Meta+s"],
  "rail.notes": ["Meta+n"],
  "rail.bookmarks": ["Meta+b"],
  "rail.chat": ["Meta+j"],
  "info.toggle": ["Meta+o"],
  rescan: ["Meta+r"],
  "prefs.export": ["Meta+Shift+e"],
  "terminal.focus_app": ["Meta+Shift+o"],
  theme: ["Meta+Shift+t"],

  // Letters where `⌘` is spoken for. The mnemonic survives; only the modifier
  // and the spelling change.
  "pane.agent": ["Alt+KeyA"], // `⌘A` selects all
  "pane.code": ["Alt+KeyC"], // `⌘C` copies
  wall: ["Alt+KeyW"], // `⌘W` closes the window
  health: ["Alt+KeyH"], // `⌘H` hides the application
  keymap: ["Alt+KeyK"], // `⌘K` is the palette
  // Finder's own copy-the-path chord, rather than a translation of ours —
  // spelled by physical key because `Alt` is in it, and `leave no Option chord
  // spelled by character` caught the first attempt. Whether macOS composes a
  // character under ⌘⌥ is exactly the kind of thing worth not betting on.
  "file.copy_path": ["Meta+Alt+KeyC"],
  // These two stay beside the pane they act on rather than following the
  // free-`⌘` rule: splitting the Agent pane is `⌥⇧A` because reaching it is
  // `⌥A`, and a family split across two modifiers is a family you have to
  // memorise twice.
  "pane.agent.split": ["Alt+Shift+KeyA"],
  "pane.agent.hold": ["Alt+Shift+KeyH"],

  // `⌥F12` opens macOS' sound settings, and `⌃\`` is the chord a Mac already
  // knows from VS Code — so the second binding goes rather than being kept as
  // a chord that reaches System Settings instead of this window.
  "terminal.toggle": ["Control+`"],
  /**
   * `Ctrl+F4` is not reachable on a Mac laptop without `fn`, and `R-J18` said
   * as much when it chose the chord: *a Mac is better served rebinding this to
   * `Cmd+W`*. It cannot **be** `⌘W` — that is the window's own close — so it
   * is `⌘⇧W`, with `Ctrl+F4` kept alongside for an external keyboard with F
   * keys switched to standard.
   */
  "file.close": ["Meta+Shift+w", "Control+F4"],
};

/** The binding this action ships with **on this machine**. */
export function defaultKeys(action: Action, platform: Platform = currentPlatform()): string[] {
  return platform === "mac" ? (MAC_KEYS[action.id] ?? action.keys) : action.keys;
}

/**
 * Step to the next session **as the queue is showing it**. `R-J13`.
 *
 * This walked the raw `queue` — straight from the daemon, unfiltered and in
 * rank order — while the panel rendered a filtered, re-sorted list. So with a
 * scope on, `j`/`k` and the arrows stepped through sessions that were not on
 * screen, in an order that was not the visible one. Reported 2026-08-07 against
 * the `live` filter.
 *
 * `visibleQueue` is now the single answer to *what is on screen and in what
 * order*, and both the panel and this ask it. The bug was not the arithmetic;
 * it was that there were two lists.
 */
function moveSelection(delta: number): void {
  const { queue, sessions, selected, select, prefs, filter, scoped } = useStore.getState();
  const ids = visibleQueue({ queue, sessions, scope: prefs.scope, filter, scoped: scoped() }).map(
    (r) => r.session.id,
  );
  if (ids.length === 0) return;
  const at = selected ? ids.indexOf(selected) : -1;
  // A selection that is filtered *out* is not "before the first row" — landing
  // on the top is the only answer that does not depend on where it used to be.
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
 *
 * **Except under Option on a Mac**, where `e.key` is the composed character —
 * `⌥A` is `"å"`, `⌥2` is `"™"`. Recording that spelling would produce a
 * binding that *happens* to work (the same physical key composes the same
 * character every time) and reads as line noise in the shortcuts window, and
 * it would break the moment a keyboard layout changed under it. The physical
 * key is recorded instead: `Alt+KeyA`, which is what `MAC_KEYS` ships and what
 * tinykeys matches against `e.code`. `R-J19`.
 */
export function chordFromEvent(e: KeyboardEvent, platform: Platform = currentPlatform()): string | null {
  if (["Control", "Alt", "Shift", "Meta", "OS"].includes(e.key)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Meta");
  const composed = platform === "mac" && e.altKey && /^(Key[A-Z]|Digit\d)$/.test(e.code);
  // Shift only when the character does not already carry it. `Shift+$` is
  // noise — the `$` *is* the shift. `Shift+Tab` is not. A composed key is
  // named by its code, which carries no shift of its own, so `⌥⇧A` needs it.
  if (e.shiftKey && (composed || e.key.length > 1)) parts.push("Shift");
  parts.push(composed ? e.code : e.key.length === 1 ? e.key.toLowerCase() : e.key);
  return parts.join("+");
}

/**
 * The bindings in force for an action: the user's, if they set any, else the
 * ones this platform ships with.
 */
export function bindingsFor(
  action: Action,
  overrides: Record<string, string[]>,
  platform: Platform = currentPlatform(),
): string[] {
  const own = overrides[action.id];
  return own ? own : defaultKeys(action, platform);
}

const MAC_MODS: Record<string, string> = {
  $mod: "⌘",
  Meta: "⌘",
  Control: "⌃",
  Alt: "⌥",
  Shift: "⇧",
};

const MODS: Record<string, string> = { $mod: "Ctrl", Control: "Ctrl", Meta: "Meta" };

/** Physical-key spellings, back to the key you are looking at. */
const KEY_LABELS: Record<string, string> = { Equal: "=", Minus: "−", Backquote: "`" };

/**
 * A chord as it should be **read**, which is not always how it is matched.
 *
 * Three spellings in `ACTIONS` are for tinykeys rather than for a person:
 * `$mod` (Control here, `⌘` there), `KeyA`/`Digit0` (a physical key, so an
 * Option chord fires on macOS) and `Equal`/`Minus` (a *code*, so `Ctrl++` and
 * `Ctrl+=` are one gesture). All three used to reach the shortcuts window
 * verbatim, which made the one place that documents the keyboard the place
 * least able to say what to press. `R-J19`.
 *
 * Mac spelling is the platform's own — `⌘⇧E`, no separators — because that is
 * what every other Mac application shows and what the keycaps say.
 */
export function formatChord(chord: string, platform: Platform = currentPlatform()): string {
  const parts = chord.split("+");
  const key = parts.pop() ?? "";
  const mac = platform === "mac";
  const mods = parts.map((m) => (mac ? (MAC_MODS[m] ?? m) : (MODS[m] ?? m)));
  const named = KEY_LABELS[key] ?? key.replace(/^Key([A-Z])$/, "$1").replace(/^Digit(\d)$/, "$1");
  const label = named.length === 1 ? named.toUpperCase() : named;
  return mac ? [...mods, label].join("") : [...mods, label].join("+");
}

/**
 * The chord to *advertise* for an action, formatted — what a tooltip says.
 *
 * A hook rather than a constant because it reads the rebinds: the dock strip
 * used to name `Alt+2` in a `title` string beside a keymap that no longer said
 * so, and the rail's tooltips named four chords (`Alt+4`–`Alt+7`) that had not
 * been the rail's since `R-B47` moved the digits to the dock. A tooltip that
 * lies about a key is worse than one that says nothing, and on a Mac every one
 * of them would have lied.
 */
export function useChord(actionId: string): string {
  const overrides = useStore((s) => s.prefs.keymap);
  const action = ACTIONS.find((a) => a.id === actionId);
  if (!action) return "";
  const keys = bindingsFor(action, overrides);
  return keys.length > 0 ? formatChord(keys[0]) : "";
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
