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

import type { SessionId, SessionSource } from "@/wire/types";
import type { Mark } from "@/lib/marks";

export type Scope = "needs_you" | "live" | "all";
export type ThemeMode = "dark" | "light" | "system";
/**
 * The rail's tools, in the order the strip draws them — which is also the
 * order they stack in when more than one is open (`R-J33`). A set, written as
 * a list: the stack does not reorder itself around the order you opened
 * things in, because a panel that moves when you add a second one is a panel
 * you have to re-find.
 */
export const RAIL_TOOLS = ["files", "search", "notes", "bookmarks"] as const;

export type RailTool = (typeof RAIL_TOOLS)[number];

/**
 * What the bottom dock can show.
 *
 * It began as *reference* — Git, Insight and Debt, things you consult and go
 * back from — against a centre that held what you were doing. **Changes and
 * Transcript joined them on 2026-08-06**, at an explicit ask, which redraws
 * that line: the centre is now the file and the agent, and the dock holds
 * everything you read *about* a session.
 *
 * The cost is stated where it will be felt: the dock shows **one tool at a
 * time**, so the diff and the conversation can no longer be split side by side
 * the way the tile tree allowed. That was possible before and is not now.
 */
export type DockTool = "insight" | "git" | "debt" | "changes" | "transcript" | "run";

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
  /**
   * Marked lines in files: `[session, path, line]`, plus a digit since
   * `R-J37`. See [`lib/marks.ts`](../lib/marks.ts) — the fourth element is
   * absent on every mark made by clicking the margin, so an older preferences
   * file loads unchanged.
   */
  bookmarks: Mark[];
  /**
   * A colour you put on a session yourself, by id — see `lib/tags.ts`.
   * Machine-scoped like the labels, because a session id from the dev box means
   * nothing on the laptop.
   */
  tags: Record<SessionId, string>;
  /** The panel's shells: `[tmux session name, worktree root]`. `R-B33`. */
  shells: [string, string][];
  /**
   * Folders you keep in the New session window, in the order you added them.
   * `R-J45`. See [`lib/favourites.ts`](../lib/favourites.ts), which carries the
   * argument for why a path belongs on this side of the split rather than
   * beside the view preferences.
   */
  favouriteDirs: string[];
  /**
   * Panes held on a session rather than following the selection, by pane id.
   * `R-B49`.
   *
   * **Machine-scoped, and it belongs here for the reason above**: the value is
   * a session id, which means nothing on another machine. A hold carried to the
   * laptop would name a session that daemon has never heard of, and the pane
   * would sit on the ended state for ever with no way to read why.
   *
   * Called `paneHold` and not `panePinned` on purpose. `pinned` above is the
   * queue's pin — *keep this session at the top of the list* — and two
   * different pins in one window is a worse cost than an unfamiliar verb.
   */
  paneHold: Record<string, SessionId>;
}

export interface Prefs {
  /**
   * Which defaults this file was written against. See [`PREFS_VERSION`].
   *
   * Absent in every file written before 2026-08-17, which is exactly what makes
   * it usable as a mark.
   */
  version: number;

  scope: Scope;
  theme: ThemeMode;

  queueCollapsed: boolean;
  /**
   * Which tools the right rail shows, stacked top to bottom, or empty for the
   * strip. `R-B40`, and a **list** since `R-J33` — it was one tool at a time
   * until 2026-08-19.
   *
   * Written by `railList` on load, so a preferences file saved as the old
   * `"files"` or `null` becomes `["files"]` or `[]` rather than putting a
   * string where the window now iterates.
   */
  rail: RailTool[];
  railWidth: number;
  /**
   * Each stacked tool's share of the rail's height, as a flex weight rather
   * than a pixel count. `R-J33`.
   *
   * Weights and not pixels because the rail is as tall as the window: a saved
   * `240px` means half the rail on a laptop and a third of it on the desktop,
   * and the stack would have to be re-dragged on every machine. A weight
   * survives the resize. Missing means 1, so a tool opened for the first time
   * takes an even share of what is already there.
   */
  railSizes: Partial<Record<RailTool, number>>;
  /** The bottom dock's open tool, or `null` for the strip. */
  dock: DockTool | null;
  dockHeight: number;
  /** Info sits under the queue, because it is about the row you just clicked. */
  infoOpen: boolean;
  /** The Files tree follows the file you are reading. `R-J25`. */
  filesFollow: boolean;
  /**
   * Suggested folders you have waved away, by repository root. `R-J39`.
   *
   * A preference and not a daemon fact: refusing a suggestion says nothing
   * about the session, only that you do not want to be asked again on this
   * machine. It is keyed by the same repository root the workspace itself is
   * keyed by, so dismissing outlives the session exactly as adding does.
   */
  dismissedDirs: Record<string, string[]>;
  infoHeight: number;
  /** The Notes editor's share of the rail. */
  notesEditorHeight: number;
  /**
   * Which CLI the New session window starts. `R-J51`.
   *
   * A preference rather than a per-machine one: it says which agent *you*
   * reach for, and that does not change when you walk to another desk. It is
   * remembered because this window is the front door since `R-J45` — picking
   * the same CLI every morning is the sort of click you stop noticing and
   * never stop paying.
   */
  launchSource: SessionSource;
  /**
   * Start new sessions headless — detached under tmux, no terminal window,
   * hosted in a mogeung pane. `R-J61`.
   *
   * Remembered for `launchSource`'s reason: it says how *you* run agents, and
   * re-ticking it every morning is the click you stop noticing and never stop
   * paying. Off by default — a terminal window is what this window always
   * did, and what a machine without tmux can still deliver.
   */
  launchHeadless: boolean;
  queueWidth: number;
  /**
   * The Git pane's list of commits and paths. Its own setting rather than a
   * shared one: what it holds is repository paths, which are longer than
   * anything else the window puts in a column.
   */
  gitSidebarWidth: number;

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
   * The Transcript reads newest-first. Off by default: a call and its result
   * swap places when it is on, which is a real cost and not everyone's trade.
   */
  newestFirst: boolean;
  /**
   * Render a note rather than showing its source. On by default: a note is
   * mostly *read*, and since 2026-08-06 most of what is in one was copied out
   * of a conversation as markdown rather than typed.
   */
  notesMarkdown: boolean;

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
   * **Not** `~/.mogeung/keymap.json`. That file is keyed by the retired egui
   * client's action names, and keeping the two apart is why retiring that
   * client ([ADR-0020](../../../docs/decisions/0020-the-egui-client-is-retired.md))
   * cost nobody their bindings. It is neither read nor deleted now.
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
  tags: {},
  shells: [],
  favouriteDirs: [],
  paneHold: {},
});

/**
 * Bumped only when a **default changes** and an existing file has to be told.
 *
 * The merge in [`loadPrefs`] fills in what a saved file lacks, which is right
 * for a new setting and useless for a changed one: `savePrefs` writes every
 * field, so a file written yesterday states the old default explicitly and
 * would keep it for ever. This is the one-time nudge for those files. It moves
 * a setting the reader may have chosen, so a bump needs to be worth that.
 *
 * - **1** — side-by-side became the default diff (2026-08-17).
 */
export const PREFS_VERSION = 1;

export const defaultPrefs = (): Prefs => ({
  version: PREFS_VERSION,
  scope: "needs_you",
  theme: "dark",
  queueCollapsed: false,
  rail: [],
  railWidth: 300,
  railSizes: {},
  dock: null,
  dockHeight: 280,
  infoOpen: false,
  filesFollow: false,
  dismissedDirs: {},
  infoHeight: 240,
  notesEditorHeight: 256,
  launchSource: "claude_code",
  launchHeadless: false,
  queueWidth: 380,
  gitSidebarWidth: 340,
  groupByRepo: false,
  autoSelect: false,
  previewOnSelect: true,
  hideReviewed: false,
  hideNoise: true,
  syntax: true,
  wordDiff: true,
  sideBySide: true,
  markdown: true,
  showThinking: true,
  newestFirst: false,
  notesMarkdown: true,
  notify: false,
  zoom: {},
  appZoom: 1,
  keymap: {},
  terminalFontPx: 13,
  scoped: {},
});

const KEY = "mogeung.prefs";

/**
 * The settings a file older than [`PREFS_VERSION`] is moved onto, once.
 *
 * Exported for its test, and kept as a pure function of the saved file so that
 * test can state the whole rule: an old file is nudged, and a file already at
 * this version is left exactly as its owner set it.
 */
export function migrateDefaults(saved: Partial<Prefs>): Partial<Prefs> {
  const from = saved.version ?? 0;
  const out: Partial<Prefs> = {};
  // 1 — side-by-side is the default diff now. Every file written before this
  // says `false`, whether that was a choice or the old default, and there is no
  // way to tell the two apart. Moving them all is the cost of changing a
  // default at all; the toggle in Changes and in Git is one click back.
  if (from < 1) out.sideBySide = true;
  return out;
}

/**
 * The rail's open tools, out of whatever a saved file happens to hold. `R-J33`.
 *
 * Three shapes reach this, and only one of them is current:
 *
 * - `["files", "notes"]` — today's, filtered to the tools this build has;
 * - `"files"` — every file written before 2026-08-19, when the rail showed one
 *   tool at a time;
 * - `null` or absent — the rail was collapsed, or the file predates it.
 *
 * Forgiving on purpose, and the reason is older than this row: `Prefs` is one
 * `JSON.parse`, so a tool name from a newer build must cost that name and not
 * every setting in the file. The result is also put back in strip order, so
 * the stack reads the same way whichever order the tools were opened in.
 */
export function railList(value: unknown): RailTool[] {
  const raw = typeof value === "string" ? [value] : Array.isArray(value) ? value : [];
  return RAIL_TOOLS.filter((t) => raw.includes(t));
}

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
    // Every **scoped** entry is completed against the current shape, and this
    // is load-bearing rather than tidy. `scoped()` returns the stored object
    // itself — it has to, because a selector that mints a fresh object every
    // call re-renders for ever — so a field added after a file was written is
    // `undefined` at every read site, and the first `scoped.tags[id]` throws.
    // React then unwinds the whole tree and the window is blank, which is
    // exactly what adding `tags` did to a preferences file written yesterday.
    //
    // This is the TypeScript of `#[serde(default)]`, and it belongs here for
    // the same reason the Rust puts it on the struct: at the boundary, once,
    // rather than as a `?.` at every reader.
    const scoped: Record<string, ScopedPrefs> = {};
    for (const [key, value] of Object.entries(saved.scoped ?? {})) {
      scoped[key] = { ...emptyScoped(), ...(value ?? {}) };
    }
    return {
      ...base,
      ...saved,
      ...migrateDefaults(saved),
      // After the spread, never before: `...saved` would otherwise put the old
      // single-tool string straight into a field the window now iterates, and
      // `"files".map` is a blank window rather than a lost setting.
      rail: railList(saved.rail),
      version: PREFS_VERSION,
      zoom: { ...base.zoom, ...saved.zoom },
      scoped,
    };
  } catch {
    return base;
  }
}

/**
 * Everything you have told this window, as one file you can keep.
 * [ADR-0023](../../../docs/decisions/0023-judgements-stay-in-the-client.md).
 *
 * That ADR settled *where* your judgements live — here, in the client — and
 * named the exposure it accepts in the same breath: a label is text you wrote,
 * living in a webview's `localStorage`, with no export and no backup. Clearing
 * that storage loses every label, tag and pin on the machine, silently and
 * with nothing to restore from.
 *
 * This is the cheap half of the answer. It does not move ownership and does not
 * need a new capability — the shell's save path already exists for `R-B43`'s
 * transcript export — and it turns a total silent loss into a file you chose
 * where to put.
 *
 * **The whole preferences object, not just the judgements.** Picking out
 * labels, tags and pins would produce a backup that restores some of your
 * settings and quietly not others, which is a worse promise than either whole
 * answer. `kind` and `version` are here so an importer — which does not exist
 * yet, and is the other half — can refuse a file that is not this.
 */
export interface PrefsExport {
  kind: "mogeung.prefs";
  version: 1;
  exported: string;
  prefs: Prefs;
}

export function exportPayload(prefs: Prefs, now: Date): PrefsExport {
  return { kind: "mogeung.prefs", version: 1, exported: now.toISOString(), prefs };
}

/** `mogeung-preferences-2026-08-07.json` — sorts by date in a downloads folder. */
export function exportFilename(now: Date): string {
  return `mogeung-preferences-${now.toISOString().slice(0, 10)}.json`;
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

/** What succession needs from a session — a subset of `Session`. */
export interface SuccessionFact {
  id: SessionId;
  alive: boolean;
  pid: number | null;
  cwd: string;
  last_event_at: string;
}

const epoch = (ts: string): number => {
  const t = Date.parse(ts);
  return Number.isFinite(t) ? t : 0;
};

/**
 * The dead sessions this live one carries on from, most recently active first.
 *
 * Every `/clear` in a process's life adds one, so a terminal left open for two
 * days has a *line* behind it and not a single predecessor. They are ordered by
 * `last_event_at` and **not** by `started_at`, which is the flaw that let this
 * bug back in: the daemon fills `started_at` from the live registry's
 * `startedAt`, which is when the *process* started, not when the conversation
 * did. Every session on one pid therefore reports the same `started_at` once it
 * has been alive, the comparison that was meant to find the newest predecessor
 * always tied, and the tie kept whichever the session map happened to hold
 * first — the oldest. `last_event_at` is the last line that session actually
 * wrote, which is the one thing that does order a chain.
 */
function lineage(heir: SuccessionFact, sessions: SuccessionFact[]): SuccessionFact[] {
  return sessions
    .filter((s) => !s.alive && s.id !== heir.id && s.pid === heir.pid && s.cwd === heir.cwd)
    .sort((a, b) => epoch(b.last_event_at) - epoch(a.last_event_at));
}

/**
 * Every dead session `/clear` replaced, mapped to the live one that carries on.
 *
 * The whole line maps to the same heir, not just the session immediately before
 * it: a window that was closed — or a daemon that was restarted — across a
 * `/clear` never made that hop, and the state it was holding is stranded two
 * ids back. See [`migrateSuccession`] for what counts as a succession.
 */
export function successions(sessions: SuccessionFact[]): Map<SessionId, SessionId> {
  const heirs = new Map<SessionId, SessionId>();
  for (const heir of sessions) {
    if (!heir.alive || heir.pid == null) continue;
    for (const pred of lineage(heir, sessions)) heirs.set(pred.id, heir.id);
  }
  return heirs;
}

/**
 * `/clear` in Claude Code keeps the process but mints a fresh session id, so a
 * hand-applied label lands on a dead id while the same work carries on under a
 * new one. The live registry is per-*pid*, which makes succession a fact rather
 * than a guess: a dead session and a live one sharing a pid are the same
 * conversation. Labels, tags and pins follow it.
 *
 * A port of `Prefs::migrate_succession`, which the egui client carried and this
 * one shipped without — the reason a label kept disappearing on `/clear` here
 * while that window kept it. Same rules, deliberately:
 *
 * - The cwd must match as well as the pid. Pids are reused by the OS
 *   eventually, and a label jumping onto an unrelated session that happened to
 *   inherit a number would be worse than the bug this fixes.
 * - Nothing hand-applied is ever overwritten. A successor you already named
 *   keeps its name, and the predecessor keeps its own rather than losing it to
 *   a move that went nowhere.
 * - Two live sessions never trade state. Succession requires a death.
 *
 * Each thing moves from the most recent session in the line that still has one,
 * rather than from the immediate predecessor alone. The immediate predecessor
 * is usually that session — but when a hop was missed, insisting on it means
 * asking an id that never held the label whether it holds the label, and the
 * answer strands it for good.
 *
 * Nothing is invented — only moved. Returns the patch for `setScoped`, or
 * `null` when nothing moved, so the caller writes to disk only on a change.
 */
export function migrateSuccession(
  scoped: ScopedPrefs,
  sessions: SuccessionFact[],
): Partial<ScopedPrefs> | null {
  const labels = { ...scoped.labels };
  const tags = { ...scoped.tags };
  let pinned = [...scoped.pinned];
  let changed = false;

  for (const heir of sessions) {
    if (!heir.alive || heir.pid == null) continue;
    const line = lineage(heir, sessions);
    if (line.length === 0) continue;

    const donor = (held: Record<SessionId, string>): SessionId | null =>
      line.find((s) => held[s.id])?.id ?? null;

    const namer = donor(labels);
    if (namer && !labels[heir.id]) {
      labels[heir.id] = labels[namer];
      delete labels[namer];
      changed = true;
    }
    const tagger = donor(tags);
    if (tagger && !tags[heir.id]) {
      tags[heir.id] = tags[tagger];
      delete tags[tagger];
      changed = true;
    }
    // One pin for the line, on its live head: a chain of dead ids each pinned
    // separately would be one queue row per `/clear`, all but one of them a
    // conversation that ended.
    const pins = new Set(line.filter((s) => pinned.includes(s.id)).map((s) => s.id));
    if (pins.size > 0 && !pinned.includes(heir.id)) {
      pinned = [...pinned.filter((id) => !pins.has(id)), heir.id];
      changed = true;
    }
  }

  return changed ? { labels, tags, pinned } : null;
}
