/**
 * View preferences that survive a restart.
 *
 * A port of `crates/mogeung-ui/src/prefs.rs`, minus its two-file split — see
 * the note on `scoped` below.
 *
 * Client-side, like the keymap, for the reason ADR-0001 gives: none of this is
 * daemon state. Which sessions *you* have hidden in *this* window says nothing
 * about the sessions themselves.
 */

import type { SessionId } from "@/wire/types";

export type Scope = "needs_you" | "live" | "all";
export type ThemeMode = "dark" | "light" | "system";
export type RailTool = "files" | "search" | "notes" | "bookmarks";

/**
 * What the bottom dock can show.
 *
 * These four are *reference*: you consult one and go back. The centre is what
 * you are doing — the diff, the conversation, the file, the agent.
 */
export type DockTool = "insight" | "git" | "debt";

/**
 * The half of the preferences that is about a *machine* rather than about this
 * window — pins, labels, hidden sessions, shells.
 *
 * `R-I11` split this out because a session id from the dev box means nothing on
 * the laptop, and `~/projects/mogeung` means different files on each. Keyed by
 * the daemon's `machine_id`, never by its URL: an `ssh -L` tunnel makes a
 * remote daemon answer on `127.0.0.1`, and keying on the address would file the
 * dev box's pins under the laptop.
 *
 * `R-I12` records the argument that this belongs to the daemon instead. It is
 * still here, and still client-side — moving it is a decision, not a port.
 */
export interface ScopedPrefs {
  hidden: SessionId[];
  pinned: SessionId[];
  labels: Record<SessionId, string>;
  editorWrap: string[];
  bookmarks: [SessionId, string, number][];
  /** The panel's shells: `[tmux session name, worktree root]`. `R-B33`. */
  shells: [string, string][];
}

export interface Prefs {
  scope: Scope;
  theme: ThemeMode;

  queueCollapsed: boolean;
  /** Which tool the right rail shows, or `null` for the strip. `R-B40`. */
  rail: RailTool | null;
  railWidth: number;
  /** The bottom dock's open tool, or `null` for the strip. */
  dock: DockTool | null;
  dockHeight: number;
  /** Info sits under the queue, because it is about the row you just clicked. */
  infoOpen: boolean;
  infoHeight: number;
  /** The Notes editor's share of the rail. */
  notesEditorHeight: number;
  queueWidth: number;

  groupByRepo: boolean;
  autoSelect: boolean;
  previewOnSelect: boolean;

  hideReviewed: boolean;
  hideNoise: boolean;
  syntax: boolean;
  wordDiff: boolean;
  sideBySide: boolean;

  markdown: boolean;
  showThinking: boolean;

  /**
   * Post a desktop banner when a session starts needing you. `R-C1`.
   *
   * Off until asked for, which is the rule `notify.rs` states and the reason
   * `mogeungd` hides delivery behind `--notify`: a tool that starts posting
   * banners the first time you run it has overstepped. One click in the top
   * bar, and the OS permission is asked for then rather than at startup.
   */
  notify: boolean;

  /** Per-pane content zoom. Only levels that are not 1.0 are stored. */
  zoom: Record<string, number>;
  /** The whole window's scale. A Tauri webview has no browser zoom of its own. */
  appZoom: number;

  /**
   * Rebound keys, by action id. Only what you changed — an action absent here
   * uses the shipped binding, so a default that improves later reaches you.
   *
   * **Not** `~/.mogeung/keymap.json`, deliberately. That file is keyed by the
   * egui client's own action names, and its loader fails the whole file on a
   * key it does not recognise — writing this client's ids into it would
   * silently reset your Rust keymap to defaults. Two clients, two files, until
   * there is one client.
   */
  keymap: Record<string, string[]>;

  terminalFontPx: number;

  /** Keyed by `machine_id`. */
  scoped: Record<string, ScopedPrefs>;
}

export const emptyScoped = (): ScopedPrefs => ({
  hidden: [],
  pinned: [],
  labels: {},
  editorWrap: [],
  bookmarks: [],
  shells: [],
});

export const defaultPrefs = (): Prefs => ({
  scope: "needs_you",
  theme: "dark",
  queueCollapsed: false,
  rail: null,
  railWidth: 300,
  dock: null,
  dockHeight: 280,
  infoOpen: false,
  infoHeight: 240,
  notesEditorHeight: 256,
  queueWidth: 380,
  groupByRepo: false,
  autoSelect: false,
  previewOnSelect: true,
  hideReviewed: false,
  hideNoise: true,
  syntax: true,
  wordDiff: true,
  sideBySide: false,
  markdown: true,
  showThinking: true,
  notify: false,
  zoom: {},
  appZoom: 1,
  keymap: {},
  terminalFontPx: 13,
  scoped: {},
});

const KEY = "mogeung.prefs";

/**
 * Load, filling anything missing from the defaults.
 *
 * Merging rather than replacing matters across versions: preferences saved
 * today must not leave a field added next month permanently unset. A corrupt
 * file degrades to defaults rather than refusing to start — losing a setting
 * you can redo beats a window that will not open.
 */
export function loadPrefs(): Prefs {
  const base = defaultPrefs();
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return base;
    const saved = JSON.parse(raw) as Partial<Prefs>;
    return { ...base, ...saved, zoom: { ...base.zoom, ...saved.zoom }, scoped: { ...saved.scoped } };
  } catch {
    return base;
  }
}

export function savePrefs(p: Prefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(p));
  } catch {
    // A full quota is not worth an error strip. The setting is lost, the
    // window keeps working.
  }
}

/** Per-pane zoom, clamped. Near-1.0 erases the entry so the file stays quiet. */
export function setZoom(zoom: Record<string, number>, pane: string, level: number): Record<string, number> {
  const next = { ...zoom };
  const z = Math.min(2.5, Math.max(0.5, level));
  if (Math.abs(z - 1) < 0.05) delete next[pane];
  else next[pane] = z;
  return next;
}

export function zoomOf(zoom: Record<string, number>, pane: string): number {
  return zoom[pane] ?? 1;
}
