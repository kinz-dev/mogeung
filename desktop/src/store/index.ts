/**
 * The whole client state, and the only place a `ServerMsg` is handled.
 *
 * The daemon is the product and this is a projection of it — nothing here has
 * local authority. That is why `ingest` mostly *replaces* rather than merges:
 * a snapshot is the truth, and reconciling would be inventing a second copy of
 * state the daemon already holds.
 *
 * What genuinely belongs to the client is view state — selection, prefs, which
 * directories are expanded, which files are open. Those live here too, and are
 * marked as such.
 */

import { create } from "zustand";
import { summarize } from "@/store/changes";
import { usePaneId } from "@/lib/paneScope";
import { DaemonClient, defaultUrl, type ConnState } from "@/wire/client";
import type {
  Analytics,
  Run,
  RunConfig,
  RunLine,
  AttentionItem,
  BlameLine,
  BlastRadius,
  Change,
  ChangeSummary,
  CommitDetail,
  CommitInfo,
  ContentMatch,
  DaemonIdentity,
  DayDigest,
  DecisionCandidate,
  DirEntry,
  DocInventory,
  FileChange,
  FileSession,
  Health,
  Note,
  PromptCluster,
  RecurringFailure,
  RefsInfo,
  ReflogEntry,
  ReviewDebt,
  SearchResults,
  ServerMsg,
  Session,
  SessionId,
  SignalRun,
  StashInfo,
  StatusEntry,
  SubagentNode,
  SubmoduleInfo,
  TranscriptEvent,
  UsageReport,
  WorktreeInfo,
  WorkspaceHint,
  ClientMsg,
} from "@/wire/types";
import {
  daemonAcquire,
  isTauri,
  localAddrOf,
  machineId as machineIdFromShell,
  type DaemonStatus,
} from "@/lib/tauri";
import { applyAppZoom } from "@/lib/zoom";
import type { FlaggedHunk } from "@/lib/prompt";
import {
  defaultPrefs,
  emptyScoped,
  loadPrefs,
  migrateSuccession,
  savePrefs,
  setZoom,
  successions,
  type Prefs,
  type ScopedPrefs,
} from "./prefs";

/** Which detail pane is forward. Serialised into the dockview layout. */
export type PaneId =
  | "queue"
  | "changes"
  | "transcript"
  | "info"
  | "debt"
  | "agent"
  | "code"
  | "git"
  | "insight";

/** One open file in the Code pane. Client-side view state. */
export interface FileTab {
  path: string;
  /** `null` is the worktree; a sha opens that revision, read-only twice over. */
  rev: string | null;
  pinned: boolean;
  content: string | null;
  truncated: boolean;
  gotoLine: number | null;
  /**
   * The body we are holding is known to be out of date. `R-J38`.
   *
   * A flag rather than `content = null`, and the difference is the whole
   * behaviour: nulling it unmounts the editor and shows *loading…*, which
   * would flash the pane — and lose the viewport — every time an agent writes
   * the file you are reading, which is precisely when this fires. The old body
   * stays on screen until the new one lands.
   */
  reload: boolean;
}

export interface ExplorerState {
  dirs: Record<string, DirEntry[]>;
  expanded: string[];
  pending: string[];
  /**
   * The files this session has open. `R-B53`.
   *
   * Which one is *forward*, and how they are arranged, is dockview's business
   * now: every open file is a pane of its own. This list carries only what the
   * tree cannot re-derive — the bodies, the pins, a pending `gotoLine`.
   */
  open: FileTab[];
  reveal: string | null;
  treePaths: string[] | null;
  treeTruncated: boolean;
  treePending: boolean;
  /** Bodies already asked for, so a render is not a request. */
  pendingFiles: string[];
  /**
   * Folders added to this session's workspace by hand, and any that have gone
   * away since. `R-J40`.
   *
   * Absolute, and answered by the daemon rather than remembered here: the
   * store is `~/.mogeung/workspaces.json`, keyed by the session's
   * **repository**, so it outlives the session and every client sees the same
   * one. `null` means nobody has asked yet — which is a different thing from
   * *asked, and there are none*, and the tree draws them differently.
   */
  workspace: { dirs: string[]; missing: string[]; hints: WorkspaceHint[] } | null;
}

export const emptyExplorer = (): ExplorerState => ({
  workspace: null,
  dirs: {},
  expanded: [],
  pending: [],
  open: [],
  reveal: null,
  treePaths: null,
  treeTruncated: false,
  treePending: false,
  pendingFiles: [],
});

export interface GitState {
  commits: CommitInfo[];
  done: boolean;
  /**
   * A log has been asked for. `R-D12`.
   *
   * The pane used to read an **empty list** as "nobody has asked yet", which
   * held right up until a filter could empty it on purpose: every filtered
   * query fired a second, unfiltered one behind it, and the whole log came
   * back. Emptiness is a result; this is the question.
   */
  logAsked: boolean;
  rev: string | null;
  grep: string;
  author: string;
  path: string;
  pickaxe: string;
  selected: string | null;
  diff: FileChange[] | null;
  detail: CommitDetail | null;
  status: StatusEntry[] | null;
  refs: RefsInfo | null;
  stashes: StashInfo[] | null;
  submodules: SubmoduleInfo[] | null;
  reflog: ReflogEntry[] | null;
  worktrees: WorktreeInfo[] | null;
  /**
   * The working-tree file whose diff the pane asked for, which is to the
   * `local` list what `selected` is to the log: it highlights the row, and it
   * is what a late answer is checked against before it is shown.
   */
  selectedPath: string | null;
  fetching: boolean;
  fetched: string[] | null;
  /** The three stages of a conflicted file, read-only. `R-D16`. */
  conflict: { path: string; base: string; ours: string; theirs: string; truncated: boolean } | null;
  /** What the diff pane is currently showing, so its header can say. */
  diffLabel: string | null;
}

export const emptyGit = (): GitState => ({
  commits: [],
  done: false,
  logAsked: false,
  rev: null,
  grep: "",
  author: "",
  path: "",
  pickaxe: "",
  selected: null,
  diff: null,
  detail: null,
  status: null,
  refs: null,
  stashes: null,
  submodules: null,
  reflog: null,
  worktrees: null,
  selectedPath: null,
  fetching: false,
  fetched: null,
  conflict: null,
  diffLabel: null,
});

export interface InsightState {
  query: string;
  results: [string, SearchResults] | null;
  searchPending: boolean;
  day: string;
  digest: DayDigest | null;
  digestPending: boolean;
  analytics: Analytics | null;
  prompts: PromptCluster[] | null;
  failures: RecurringFailure[] | null;
  decisions: Record<SessionId, DecisionCandidate[]>;
  fileQuery: string;
  fileSessions: [string, FileSession[]] | null;
  docRepo: string;
  docs: [string, DocInventory] | null;
}

const emptyInsight = (): InsightState => ({
  query: "",
  results: null,
  searchPending: false,
  day: new Date().toISOString().slice(0, 10),
  digest: null,
  digestPending: false,
  analytics: null,
  prompts: null,
  failures: null,
  decisions: {},
  fileQuery: "",
  fileSessions: null,
  docRepo: "",
  docs: null,
});

/** One group of the global search panel. `R-F13`. */
export interface SearchGroup<T> {
  running: boolean;
  /**
   * `null` is *not yet answered*; `[]` is *answered, and there is nothing*.
   * Flattening the two is how a search comes to look broken while working —
   * `R-J5`, in a panel with three of them side by side.
   */
  hits: T[] | null;
  truncated: boolean;
}

const idleGroup = <T,>(): SearchGroup<T> => ({ running: false, hits: null, truncated: false });

export type SearchSel =
  | { kind: "turn"; seq: number }
  | { kind: "file"; path: string; line: number }
  | { kind: "hit"; session: string; line: number; ts: string | null; preview: string };

export interface SearchPanelState {
  query: string;
  /** What the daemon groups were actually asked, so a stale answer is drop-able. */
  issued: string;
  files: SearchGroup<ContentMatch>;
  insight: SearchGroup<import("@/wire/types").SearchHit>;
  everywhere: boolean;
  sel: SearchSel | null;
  preview: [string, string] | null;
  previewAsked: string | null;
}

const EMPTY_SCOPED: ScopedPrefs = emptyScoped();
const EMPTY_EXPLORER: ExplorerState = emptyExplorer();
const EMPTY_GIT: GitState = emptyGit();

const emptySearch = (): SearchPanelState => ({
  query: "",
  issued: "",
  files: idleGroup(),
  insight: idleGroup(),
  everywhere: true,
  sel: null,
  preview: null,
  previewAsked: null,
});

/** One thing that went wrong, and whether you have looked at it. */
export interface Notice {
  id: number;
  text: string;
  /**
   * Whether this is something that went **wrong**.
   *
   * There was one channel until 2026-08-07 and it was styled as an alert, so
   * the only way to tell you a thing had *worked* was to say it in red. Both
   * places that wanted to — the export saying where it wrote, the bell
   * explaining which process announces — went without rather than lie about
   * severity, which is how a window ends up with no way to answer "did that
   * do anything?".
   */
  kind: "error" | "info";
  /** Epoch ms — a toast shows while it is young, the log keeps it after. */
  at: number;
  seen: boolean;
}

let noticeSeq = 0;

/**
 * Most transcript events one session keeps in this window.
 *
 * The store held 52k events / 22 MB of JSON on the machine this was tuned
 * on, all of it live for the life of the window. Five thousand covers all
 * but the outlier sessions whole; past it the oldest turns drop off the
 * top, which is the same deal `runOutput` has always offered.
 */
const EVENTS_CAP = 5000;

export interface AppState {
  // -- connection ---------------------------------------------------------
  conn: ConnState;
  connDetail: string | null;
  daemon: DaemonIdentity | null;
  /** Whether a snapshot has ever arrived — not the same as being connected. `R-J7`. */
  firstSnapshot: boolean;

  // -- daemon-owned data --------------------------------------------------
  sessions: Record<SessionId, Session>;
  queue: AttentionItem[];
  events: Record<SessionId, TranscriptEvent[]>;
  /** Full diffs, hunks included — held only for sessions someone asked about
   *  (`refresh_change`); the scan loop stopped broadcasting these. */
  changes: Record<SessionId, Change>;
  /** Every session's diff as counts and paths, kept current by broadcast.
   *  What the status bar and staleness checks read; never hunk bodies. */
  changeSummaries: Record<SessionId, ChangeSummary>;
  health: Health | null;
  debt: ReviewDebt | null;
  radius: BlastRadius | null;
  usage: UsageReport | null;
  notes: Note[];
  /** Memory and skills under `~/.claude`, and whichever one is open. `R-F14`. */
  kit: import("@/wire/types").KitEntry[];
  kitDoc: import("@/wire/types").KitDoc | null;
  /**
   * Whether a `kit` answer has ever arrived.
   *
   * Not derivable from `kit.length`: an empty list is a legitimate answer —
   * this machine may have no skills — and an answer that never came is a
   * different thing that must not read as one. See `KitView`'s empty state,
   * and the case that produced it: a window talking to a daemon older than
   * itself, which ADR-0009 makes an ordinary thing to be sitting in front of.
   */
  kitLoaded: boolean;
  signals: Record<string, { command: string | null; running: boolean; last: SignalRun | null }>;
  blame: Record<string, { lines: BlameLine[]; truncated: boolean }>;
  subagents: Record<SessionId, SubagentNode[]>;

  // -- client view state --------------------------------------------------
  prefs: Prefs;
  selected: SessionId | null;
  filter: string;
  explorer: Record<SessionId, ExplorerState>;
  git: Record<SessionId, GitState>;
  insight: InsightState;
  search: SearchPanelState;
  /** Scroll the Transcript to the first event at or after this moment. */
  focusEventTs: string | null;
  /**
   * Scroll the Transcript to this turn, by `seq`.
   *
   * A bookmark knows the turn it marks, and a timestamp only *approximates*
   * it — worse, a bookmark on a session this window has never opened has no
   * loaded events to take a timestamp from at all, so the jump silently did
   * nothing. This stays pending until the events arrive.
   */
  focusSeq: number | null;
  /**
   * Briefly ring the turn you were sent to.
   *
   * Scrolling something into view is not the same as pointing at it: the pane
   * lands mid-conversation and every turn looks like every other turn. The
   * highlight is what turns "somewhere near here" into "this one".
   */
  highlightSeq: number | null;
  /**
   * Things that went wrong, newest last.
   *
   * A *log*, not a modal. The old strip stayed until dismissed by hand, which
   * meant one refusal from the daemon sat across the window until you dealt
   * with it — and most of them are things you can do nothing about and only
   * need to know happened.
   */
  notices: Notice[];
  noticesOpen: boolean;
  paletteOpen: boolean;
  paletteMode: "actions" | "files";
  showHealth: boolean;
  /** The wall — every session as a tile, on a chord. `R-B50`. */
  showWall: boolean;
  /**
   * The dockview panel that is forward, by id. `R-J25`.
   *
   * The dock's own API knows this and the keymap asks it directly, but a tool
   * in the **rail** is outside dockview entirely — it has no handle to ask
   * with. Mirroring the id into the store is what lets the Files tree follow
   * the file you are reading without the rail reaching into the centre.
   */
  activePane: string | null;
  showKeymap: boolean;
  /** The daemon switcher. `R-I7`. */
  showConnections: boolean;
  /** The follow-up prompt builder, and what has been flagged for it. */
  showPrompt: boolean;
  flagged: FlaggedHunk[];
  /** The big board for a second monitor. `R-C5`. */
  ambient: boolean;
  /** Start a session in a terminal. `R-B2`. */
  showLaunch: boolean;
  showTerminal: boolean;
  /** The global-search overlay. `R-F13`, the ask-and-go half. */
  searchOpen: boolean;
  /** `~/.mogeung/machine-id`, or null in a browser. `R-I5`. */
  machineId: string | null;
  /** Whether this process is hosting the daemon or attached to one. ADR-0009. */
  daemonStatus: DaemonStatus | null;
  /** A rescan is in flight — a button that does nothing visible reads as broken. */
  rescanning: boolean;
  /** The session whose label is being edited. `R-B26`. */
  labelEditing: SessionId | null;

  // -- Run and debug. `R-N5`. ---------------------------------------------
  /** What the selected session's repository offers, keyed by session. */
  runConfigs: Record<SessionId, RunConfig[]>;
  /** ADR-0025 clause 4 as the client sees it: will this daemon start anything? */
  runsAllowed: boolean;
  /** Every run the daemon owns, newest first. */
  runs: Run[];
  /** Output per run, bounded the same way the daemon's ring is. */
  runOutput: Record<string, RunLine[]>;
  /** Values revealed **one at a time**, deliberately. `R-N6`. */
  revealedEnv: Record<string, string>;

  // -- actions ------------------------------------------------------------
  send: (msg: ClientMsg) => void;
  ingest: (msg: ServerMsg) => void;
  select: (id: SessionId) => void;
  setPrefs: (patch: Partial<Prefs>) => void;
  scoped: () => ScopedPrefs;
  setScoped: (patch: Partial<ScopedPrefs>) => void;
  zoomPane: (pane: string, factor: number) => void;
  pushError: (msg: string) => void;
  /** Something happened and it was fine. See `Notice.kind`. */
  pushNotice: (msg: string, kind?: Notice["kind"]) => void;
  markNoticesSeen: () => void;
  clearNotices: () => void;
  explorerOf: (id: SessionId) => ExplorerState;
  patchExplorer: (id: SessionId, patch: Partial<ExplorerState>) => void;
  gitOf: (id: SessionId) => GitState;
  patchGit: (id: SessionId, patch: Partial<GitState>) => void;
  patchInsight: (patch: Partial<InsightState>) => void;
  patchSearch: (patch: Partial<SearchPanelState>) => void;
  setUrl: (url: string) => void;
  url: string;
}

let client: DaemonClient | null = null;

/**
 * Predecessors whose one-time hop has already been made. See [`succeed`].
 *
 * Module state rather than store state on purpose: it is not a fact about the
 * sessions, it is a record of what this window has already done about them, and
 * nothing renders from it. Cleared when the window points at another daemon,
 * where these ids mean nothing.
 */
const hopped = new Set<SessionId>();

/**
 * Move hand-applied state — and where you are looking — onto the successor of a
 * session that `/clear` ended.
 *
 * Run wherever sessions arrive, because that is when the evidence — a dead
 * session and a live one sharing a pid — first exists. The migration is
 * idempotent and writes nothing when nothing moved, so calling it on every
 * `session_updated` costs a scan of the session map and no disk.
 *
 * **The selection and held panes move once, and only once.** The label landing
 * on the successor is no use on its own: the Agent pane below it is still
 * attached to an id whose pane died with the old conversation, which is what
 * "the terminal went blank and I had to click the session again" was. But a
 * dead session you click on *afterwards* to read is a place you asked to be,
 * and being thrown forwards out of it every time the daemon breathes would be a
 * worse bug than the one this fixes — so each predecessor donates its hop at
 * most once, and never again.
 */
function succeed(get: () => AppState): void {
  const st = get();
  const list = Object.values(st.sessions);
  const patch: Partial<ScopedPrefs> = migrateSuccession(st.scoped(), list) ?? {};

  const fresh = [...successions(list)].filter(([pred]) => !hopped.has(pred));
  let follow: SessionId | null = null;
  if (fresh.length > 0) {
    const holds = { ...st.scoped().paneHold };
    let moved = false;
    for (const [pred, heir] of fresh) {
      hopped.add(pred);
      if (st.selected === pred) follow = heir;
      for (const [pane, id] of Object.entries(holds)) {
        if (id !== pred) continue;
        holds[pane] = heir;
        moved = true;
      }
    }
    if (moved) patch.paneHold = holds;
  }

  if (Object.keys(patch).length > 0) st.setScoped(patch);
  // After the prefs, so the panes this wakes up read the migrated label rather
  // than the one that is about to move out from under them.
  if (follow) st.select(follow);
}

/**
 * Sessions this window has watched being alive, so that an ending is a
 * **transition** rather than a state.
 *
 * The distinction is the whole safety of [`reap`]. On `alive: false` alone, a
 * pane anchored to a session you deliberately opened to *read* — a finished one,
 * which is a place you asked to be — would close under you the moment the
 * daemon breathed. Worse, sessions load from the store with `alive: false` and
 * have their liveness re-derived on the first scan, so a window restored beside
 * a running agent would reap the pane a second before being told it was live.
 *
 * Module state for the same reason as [`hopped`]: it is not a fact about the
 * sessions, it is a record of what this window has already noticed about them,
 * and nothing renders from it.
 */
const seenAlive = new Set<SessionId>();

/** What to do about sessions that have just ended. See [`onSessionsEnded`]. */
type EndedHandler = (ids: SessionId[]) => void;
let endedHandler: EndedHandler | null = null;

/**
 * Register the reaction to a session ending. `App` wires this to the panes.
 *
 * A registration rather than an import, because the answer lives in
 * `lib/panes.ts` and that module already imports this one. Calling it directly
 * from here would close the loop, and the comment at the top of `panes.ts` is
 * about exactly that: the store must not reach back into the things built on
 * top of it.
 */
export function onSessionsEnded(fn: EndedHandler | null): void {
  endedHandler = fn;
}

/**
 * Notice the sessions that have just stopped running.
 *
 * **Runs after [`succeed`], and the order is load-bearing.** `/clear` ends a
 * session and starts its successor on the same pid, and succession's whole job
 * is to move the held panes forward onto the heir. Reaping first would close
 * the very panes it is about to re-aim — which is `R-J15` again, the "terminal
 * went blank after /clear" bug, with the pane gone instead of blank. Running
 * second, the hold already names the heir and there is nothing here to do.
 */
function reap(get: () => AppState): void {
  const ended: SessionId[] = [];
  for (const s of Object.values(get().sessions)) {
    if (s.alive) seenAlive.add(s.id);
    else if (seenAlive.delete(s.id)) ended.push(s.id);
  }
  if (ended.length > 0) endedHandler?.(ended);
}

/** First index in `list` (sorted by seq) whose seq is >= `seq`. */
function lowerBound(list: TranscriptEvent[], seq: number): number {
  let lo = 0;
  let hi = list.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (list[mid].seq < seq) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

export const useStore = create<AppState>((set, get) => ({
  conn: "connecting",
  connDetail: null,
  daemon: null,
  firstSnapshot: false,

  sessions: {},
  queue: [],
  events: {},
  changes: {},
  changeSummaries: {},
  health: null,
  debt: null,
  radius: null,
  usage: null,
  notes: [],
  kit: [],
  kitDoc: null,
  kitLoaded: false,
  signals: {},
  blame: {},
  subagents: {},

  prefs: defaultPrefs(),
  selected: null,
  filter: "",
  explorer: {},
  git: {},
  insight: emptyInsight(),
  search: emptySearch(),
  focusEventTs: null,
  focusSeq: null,
  highlightSeq: null,
  notices: [],
  noticesOpen: false,
  paletteOpen: false,
  paletteMode: "actions",
  showHealth: false,
  showWall: false,
  activePane: null,
  showKeymap: false,
  showConnections: false,
  showPrompt: false,
  flagged: [],
  ambient: false,
  showLaunch: false,
  showTerminal: false,
  searchOpen: false,
  machineId: null,
  daemonStatus: null,
  rescanning: false,
  labelEditing: null,
  runConfigs: {},
  runsAllowed: true,
  runs: [],
  runOutput: {},
  revealedEnv: {},
  url: defaultUrl(),

  send: (msg) => {
    client?.send(msg);
    // A note mutation is answered by a broadcast of the whole set — but if that
    // answer is ever missed, the row stays on screen and the action looks like
    // it failed. Asking again costs one small message and makes the list
    // self-correct without the client ever holding local authority over it,
    // which is the line ADR-0015 and the client contract both draw.
    if (msg.cmd === "note_delete" || msg.cmd === "note_save") {
      client?.send({ cmd: "note_list" });
    }
  },

  select: (id) => {
    const { selected, events, search } = get();
    if (selected === id) return;
    set({
      selected: id,
      focusEventTs: null,
  focusSeq: null,
  highlightSeq: null,
      // Two of the search panel's three groups are scoped to the selected
      // session, so they now describe somewhere else. Dropped rather than
      // re-run: a click through the queue is not a search, and re-issuing
      // would be a worktree scan per click.
      search: {
        ...search,
        files: idleGroup(),
        preview: null,
        previewAsked: null,
        sel: search.sel?.kind === "hit" ? search.sel : null,
      },
    });
    if (!events[id]) client?.send({ cmd: "fetch_events", session_id: id, since: 0 });
    client?.send({ cmd: "refresh_change", session_id: id });
  },

  setPrefs: (patch) => {
    const prefs = { ...get().prefs, ...patch };
    savePrefs(prefs);
    set({ prefs });
  },

  scoped: () => {
    const { prefs, daemon } = get();
    const key = daemon?.machine_id ?? "unknown";
    // `EMPTY_SCOPED`, not `emptyScoped()`. This is read through a selector, and
    // a selector that mints a fresh object every call is never equal to itself
    // — which zustand reads as "changed", re-renders, and calls again. That is
    // an infinite render loop and a blank window, caught by the smoke test
    // rather than by a user.
    return prefs.scoped[key] ?? EMPTY_SCOPED;
  },

  setScoped: (patch) => {
    const { prefs, daemon } = get();
    const key = daemon?.machine_id ?? "unknown";
    const current = prefs.scoped[key] ?? EMPTY_SCOPED;
    const next = { ...prefs, scoped: { ...prefs.scoped, [key]: { ...current, ...patch } } };
    savePrefs(next);
    set({ prefs: next });
  },

  zoomPane: (pane, factor) => {
    const { prefs } = get();
    const now = (prefs.zoom[pane] ?? 1) * factor;
    const next = { ...prefs, zoom: setZoom(prefs.zoom, pane, now) };
    savePrefs(next);
    set({ prefs: next });
  },

  pushError: (text) => get().pushNotice(text, "error"),
  pushNotice: (text, kind = "info") =>
    set((s) => ({
      // Capped, because a daemon that fails in a loop must not grow the log
      // without bound — and the fiftieth copy of one message tells you nothing
      // the first did not.
      notices: [...s.notices, { id: ++noticeSeq, text, kind, at: Date.now(), seen: false }].slice(-50),
    })),
  markNoticesSeen: () =>
    set((s) => ({ notices: s.notices.map((n) => (n.seen ? n : { ...n, seen: true })) })),
  clearNotices: () => set({ notices: [] }),

  explorerOf: (id) => get().explorer[id] ?? EMPTY_EXPLORER,
  patchExplorer: (id, patch) =>
    set((s) => ({
      explorer: { ...s.explorer, [id]: { ...(s.explorer[id] ?? emptyExplorer()), ...patch } },
    })),

  gitOf: (id) => get().git[id] ?? EMPTY_GIT,
  patchGit: (id, patch) =>
    set((s) => ({ git: { ...s.git, [id]: { ...(s.git[id] ?? emptyGit()), ...patch } } })),

  patchInsight: (patch) => set((s) => ({ insight: { ...s.insight, ...patch } })),
  patchSearch: (patch) => set((s) => ({ search: { ...s.search, ...patch } })),

  setUrl: (url) => {
    localStorage.setItem("mogeung.url", url);
    client?.setUrl(url);
    // Everything below is about the old machine. Session ids do not survive
    // the move, and a stale board is worse than an empty one.
    hopped.clear();
    seenAlive.clear();
    set({
      url,
      sessions: {},
      queue: [],
      events: {},
      changes: {},
      explorer: {},
      git: {},
      selected: null,
      firstSnapshot: false,
      daemon: null,
    });
  },

  ingest: (msg) => {
    switch (msg.ev) {
      case "snapshot": {
        const sessions: Record<SessionId, Session> = {};
        for (const s of msg.sessions) sessions[s.id] = s;
        const prev = get();
        // A reconnect replaces the board, and the per-session maps must not
        // outlive it: a session pruned while the laptop slept sent its
        // `session_removed` to nobody, and entries keyed by it would sit in
        // this heap for the life of the window.
        const keep = <T>(m: Record<SessionId, T>): Record<SessionId, T> => {
          const out: Record<SessionId, T> = {};
          for (const [id, v] of Object.entries(m)) if (sessions[id]) out[id] = v;
          return out;
        };
        set({
          sessions,
          queue: msg.queue,
          daemon: msg.daemon ?? null,
          firstSnapshot: true,
          events: keep(prev.events),
          changes: keep(prev.changes),
          changeSummaries: keep(prev.changeSummaries),
          explorer: keep(prev.explorer),
        });
        // Notes are not part of the snapshot — they have to be asked for. On
        // every snapshot rather than once, because a reconnect is a new
        // daemon's worth of state and the notes may be another machine's.
        client?.send({ cmd: "note_list" });
        // Keep the selection if it survived the reconnect; otherwise fall to
        // the top of the queue when auto-select is on.
        if (prev.selected && !sessions[prev.selected]) set({ selected: null });
        if (!get().selected && prev.prefs.autoSelect && msg.queue.length > 0) {
          get().select(msg.queue[0].session_id);
        }
        succeed(get);
        reap(get);
        break;
      }
      case "session_updated":
        set((s) => ({ sessions: { ...s.sessions, [msg.session.id]: msg.session } }));
        succeed(get);
        reap(get);
        break;
      case "session_removed":
        // Pruned outright. If this window ever saw it running, that is an
        // ending like any other — and unlike the `alive` transition there will
        // be no later message to notice it in, because the session is gone.
        if (seenAlive.delete(msg.session_id)) endedHandler?.([msg.session_id]);
        set((s) => {
          const sessions = { ...s.sessions };
          delete sessions[msg.session_id];
          // Its events and diffs go with it — a session the daemon pruned
          // must not keep paying rent in this window's heap either.
          const events = { ...s.events };
          delete events[msg.session_id];
          const changes = { ...s.changes };
          delete changes[msg.session_id];
          const changeSummaries = { ...s.changeSummaries };
          delete changeSummaries[msg.session_id];
          const explorer = { ...s.explorer };
          delete explorer[msg.session_id];
          return {
            sessions,
            events,
            changes,
            changeSummaries,
            explorer,
            selected: s.selected === msg.session_id ? null : s.selected,
          };
        });
        break;
      case "queue":
        set({ queue: msg.queue });
        break;
      case "events": {
        if (msg.events.length === 0) break;
        set((s) => {
          const next = { ...s.events };
          // Sessions untouched by this message keep their array identity, so
          // their subscribers see no change. Each touched session's list is
          // cloned once per message, not once per event.
          const touched = new Set<SessionId>();
          for (const ev of msg.events) {
            if (!touched.has(ev.session_id)) {
              next[ev.session_id] = [...(next[ev.session_id] ?? [])];
              touched.add(ev.session_id);
            }
            const list = next[ev.session_id];
            // Replayed history and live tail both land here; `seq` is the
            // identity, so a re-send is an update rather than a duplicate.
            // Live tail arrives in order, so the common case is a plain push;
            // out-of-order replay falls back to a binary-searched insert,
            // which keeps the list sorted without ever re-sorting it.
            if (list.length === 0 || ev.seq > list[list.length - 1].seq) {
              list.push(ev);
            } else {
              const at = lowerBound(list, ev.seq);
              if (at < list.length && list[at].seq === ev.seq) list[at] = ev;
              else list.splice(at, 0, ev);
            }
          }
          // Bounded, like `runOutput` below and for the same reason: the
          // live tail of every session arrives here whether or not this
          // window ever looks at it, and an uncapped array is a memory leak
          // with a scrollbar. The cap is generous — a session past it is the
          // outlier — and it is the *newest* events that are kept, which is
          // the end every pane reads from. `R-J54`.
          for (const sid of touched) {
            const list = next[sid];
            if (list.length > EVENTS_CAP) next[sid] = list.slice(-EVENTS_CAP);
          }
          return { events: next };
        });
        break;
      }
      case "change_updated":
        set((s) => {
          // **A changed file is a stale file.** `R-J38`. The daemon only
          // announces this when the diff actually moved (`unchanged` gates the
          // broadcast in `state.rs`), so an announcement naming a file we are
          // holding means what we are holding is out of date — the agent wrote
          // it while you were reading it, which is the whole scenario this
          // window exists for.
          //
          // Done here rather than through `explorer.ts` because the cache is
          // this store's, and reaching back the other way is the import cycle
          // `panes.ts` documents at its own top.
          const paths = new Set(msg.change.files.map((f) => f.path));
          const st = s.explorer[msg.session_id];
          const stale = (t: FileTab) => t.rev === null && t.content !== null && paths.has(t.path);
          const changes = { ...s.changes, [msg.session_id]: msg.change };
          // The summary map stays in step whichever message carried the news,
          // so its readers never have to know which arrived.
          const changeSummaries = {
            ...s.changeSummaries,
            [msg.session_id]: summarize(msg.change),
          };
          if (!st || !st.open.some(stale)) return { changes, changeSummaries };
          return {
            changes,
            changeSummaries,
            explorer: {
              ...s.explorer,
              [msg.session_id]: {
                ...st,
                open: st.open.map((t) => (stale(t) ? { ...t, reload: true } : t)),
                // A read already in flight would land after this and put the
                // old body back; dropping the key lets a fresh one go out.
                pendingFiles: st.pendingFiles.filter((k) => !paths.has(k)),
              },
            },
          };
        });
        break;
      case "change_summary":
        // The scan loop's word that a diff moved: counts and paths, no hunk
        // bodies. The full diff is fetched only by whoever is drawing it —
        // see `ChangesPane`, which compares this against what it holds.
        set((s) => {
          const changeSummaries = {
            ...s.changeSummaries,
            [msg.session_id]: msg.summary,
          };
          // A changed file is a stale file — `R-J38`'s rule, driven by the
          // summary's paths exactly as the full message drives it above.
          const paths = new Set(msg.summary.files.map((f) => f.path));
          const st = s.explorer[msg.session_id];
          const stale = (t: FileTab) => t.rev === null && t.content !== null && paths.has(t.path);
          if (!st || !st.open.some(stale)) return { changeSummaries };
          return {
            changeSummaries,
            explorer: {
              ...s.explorer,
              [msg.session_id]: {
                ...st,
                open: st.open.map((t) => (stale(t) ? { ...t, reload: true } : t)),
                pendingFiles: st.pendingFiles.filter((k) => !paths.has(k)),
              },
            },
          };
        });
        break;
      case "health":
        // The daemon sends this after every scan — though since `R-J55` only
        // when it changed, or on the slow heartbeat. The request paths
        // republish it unconditionally, which is what keeps this the honest
        // signal that an explicit rescan finished.
        set({ health: msg.health, rescanning: false });
        break;
      case "run_configs":
        set((st) => ({
          runConfigs: { ...st.runConfigs, [msg.session_id]: msg.configs },
          runsAllowed: msg.allowed,
        }));
        break;
      case "runs":
        set({ runs: msg.runs });
        break;
      case "run_started":
        set((st) => ({ runs: [msg.run, ...st.runs.filter((r) => r.id !== msg.run.id)] }));
        break;
      case "run_ended":
        set((st) => ({ runs: st.runs.map((r) => (r.id === msg.run.id ? msg.run : r)) }));
        break;
      case "run_output":
        set((st) => {
          // Bounded here too. The daemon's ring is the source of truth; a
          // client that grew without limit would be a memory leak with a
          // scrollbar.
          const prev = st.runOutput[msg.line.run_id] ?? [];
          const next = [...prev, msg.line];
          return {
            runOutput: {
              ...st.runOutput,
              [msg.line.run_id]: next.length > 2000 ? next.slice(-2000) : next,
            },
          };
        });
        break;
      case "run_output_history":
        set((st) => ({ runOutput: { ...st.runOutput, [msg.run_id]: msg.lines } }));
        break;
      case "run_env_value":
        // Keyed by config **and** name, so revealing one value never uncovers
        // its neighbour. `R-N6`.
        set((st) => ({
          revealedEnv: { ...st.revealedEnv, [`${msg.config_id}\u0001${msg.key}`]: msg.value },
        }));
        break;
      case "review_debt":
        set({ debt: msg.debt });
        break;
      case "blast_radius":
        set({ radius: msg.radius });
        break;
      case "usage_stats":
        set({ usage: msg.report });
        break;
      case "notes":
        set({ notes: msg.notes });
        break;
      case "kit":
        set({ kit: msg.entries, kitLoaded: true });
        break;
      case "kit_doc":
        // Replaced wholesale: one document is open at a time, and the answer
        // carries its own path so a slow read cannot land under a later click.
        set({ kitDoc: msg.doc });
        break;
      case "signal_status":
        set((s) => ({
          signals: {
            ...s.signals,
            [msg.repo]: { command: msg.command, running: msg.running, last: msg.last },
          },
        }));
        break;

      // -- explorer -------------------------------------------------------
      case "workspace":
        set((s) => {
          const st = s.explorer[msg.session_id] ?? emptyExplorer();
          return {
            explorer: {
              ...s.explorer,
              [msg.session_id]: {
                ...st,
                // `hints` defaults for a daemon older than `R-J39`: the field
                // is `#[serde(default)]` on the wire, and an undefined array
                // here would throw in the render rather than show one less row.
                workspace: { dirs: msg.dirs, missing: msg.missing, hints: msg.hints ?? [] },
              },
            },
          };
        });
        break;
      case "dir_listing":
        set((s) => {
          const st = s.explorer[msg.session_id] ?? emptyExplorer();
          return {
            explorer: {
              ...s.explorer,
              [msg.session_id]: {
                ...st,
                dirs: { ...st.dirs, [msg.path]: msg.entries },
                pending: st.pending.filter((p) => p !== msg.path),
              },
            },
          };
        });
        break;
      case "file_content":
        set((s) => {
          const st = s.explorer[msg.session_id] ?? emptyExplorer();
          const open = st.open.map((t) =>
            t.rev === null && t.path === msg.path
              ? { ...t, content: msg.content, truncated: msg.truncated, reload: false }
              : t,
          );
          // The search panel's preview is a second consumer of the same
          // message, taking it only if this is the file it asked for.
          // `R-F13` — a preview that opened a tab would be a selection
          // changing the thing it previews.
          const search =
            s.search.previewAsked === msg.path
              ? { ...s.search, preview: [msg.path, msg.content] as [string, string] }
              : s.search;
          // Cleared, or a file closed and reopened would never be asked for
          // again — the in-flight guard would still be holding its key.
          const pendingFiles = st.pendingFiles.filter((k) => k !== msg.path);
          return {
            explorer: { ...s.explorer, [msg.session_id]: { ...st, open, pendingFiles } },
            search,
          };
        });
        break;
      case "git_file_at_rev_content":
        set((s) => {
          const st = s.explorer[msg.session_id] ?? emptyExplorer();
          const open = st.open.map((t) =>
            t.rev === msg.sha && t.path === msg.path
              ? { ...t, content: msg.content, truncated: msg.truncated, reload: false }
              : t,
          );
          const pendingFiles = st.pendingFiles.filter((k) => k !== `${msg.sha}:${msg.path}`);
          return { explorer: { ...s.explorer, [msg.session_id]: { ...st, open, pendingFiles } } };
        });
        break;
      case "tree_listing":
        set((s) => {
          const st = s.explorer[msg.session_id] ?? emptyExplorer();
          return {
            explorer: {
              ...s.explorer,
              [msg.session_id]: {
                ...st,
                treePaths: msg.paths,
                treeTruncated: msg.truncated,
                treePending: false,
              },
            },
          };
        });
        break;
      case "content_matches": {
        const s = get();
        // Both the palette and the search panel ask this. Each drops what is
        // not theirs by the echoed query — no shared slot, so no two writers.
        if (s.search.issued === msg.query && s.selected === msg.session_id) {
          set({
            search: {
              ...s.search,
              files: { running: false, hits: msg.matches, truncated: msg.truncated },
            },
          });
        }
        break;
      }

      // -- git ------------------------------------------------------------
      case "git_commits":
        set((s) => {
          const st = s.git[msg.session_id] ?? emptyGit();
          // A page for filters we have since changed is a stray. The daemon
          // echoes the scope back precisely so it can be dropped — **all of
          // it**, and not only the branch scope: checking `rev` alone let an
          // unfiltered page overwrite the filtered one it raced, which is what
          // "the filter does nothing" looked like from the outside.
          if ((st.rev ?? null) !== (msg.rev ?? null)) return {};
          const echoed = (v: string | null | undefined, want: string) => (v ?? "") === want;
          if (
            !echoed(msg.grep, st.grep) ||
            !echoed(msg.author, st.author) ||
            !echoed(msg.path, st.path) ||
            !echoed(msg.pickaxe, st.pickaxe)
          ) {
            return {};
          }
          return {
            git: {
              ...s.git,
              [msg.session_id]: {
                ...st,
                commits: msg.skip === 0 ? msg.commits : [...st.commits, ...msg.commits],
                done: msg.done,
              },
            },
          };
        });
        break;
      case "git_commit_diff":
        set((s) => {
          const st = s.git[msg.session_id] ?? emptyGit();
          if (st.selected !== msg.sha) return {};
          return {
            git: {
              ...s.git,
              [msg.session_id]: { ...st, diff: msg.files, detail: msg.detail ?? null },
            },
          };
        });
        break;
      case "git_local_changes":
        get().patchGit(msg.session_id, { status: msg.entries });
        break;
      case "git_file_diff":
        set((s) => {
          const st = s.git[msg.session_id] ?? emptyGit();
          // The answer to a row that is no longer the selected one is a
          // stray — same rule the commit diff follows above. It used to land
          // in a `fileDiff` map nothing read, so clicking a file in `local`
          // fetched a diff and then showed the empty state.
          if (st.selectedPath !== msg.path) return {};
          return {
            git: {
              ...s.git,
              [msg.session_id]: {
                ...st,
                diff: msg.files,
                detail: null,
                conflict: null,
                diffLabel: `${msg.path} — the working tree against HEAD`,
              },
            },
          };
        });
        break;
      case "git_refs_info":
        get().patchGit(msg.session_id, { refs: msg.info });
        break;
      case "git_stash_list":
        get().patchGit(msg.session_id, { stashes: msg.stashes });
        break;
      case "git_submodule_list":
        get().patchGit(msg.session_id, { submodules: msg.submodules });
        break;
      case "git_reflog_list":
        get().patchGit(msg.session_id, { reflog: msg.entries });
        break;
      case "git_worktree_list":
        get().patchGit(msg.session_id, { worktrees: msg.worktrees });
        break;
      case "git_fetched":
        get().patchGit(msg.session_id, { fetching: false, fetched: msg.updates });
        break;
      case "git_annotation":
        set((s) => ({
          blame: {
            ...s.blame,
            [`${msg.session_id}:${msg.rev ?? ""}:${msg.path}`]: {
              lines: msg.lines,
              truncated: msg.truncated,
            },
          },
        }));
        break;
      case "git_range_diff":
        get().patchGit(msg.session_id, {
          diff: msg.files,
          detail: null,
          conflict: null,
          selectedPath: null,
          // `git_compare` answers with this event too, so the label says which
          // two ends were compared rather than leaving the pane unattributed.
          diffLabel: `${msg.from.slice(0, 8)}..${msg.to.slice(0, 8)}`,
        });
        break;
      case "git_stash_diff":
        get().patchGit(msg.session_id, {
          diff: msg.files,
          detail: null,
          conflict: null,
          selectedPath: null,
        });
        break;
      case "git_conflict_stages":
        get().patchGit(msg.session_id, {
          conflict: {
            path: msg.path,
            base: msg.base,
            ours: msg.ours,
            theirs: msg.theirs,
            truncated: msg.truncated,
          },
        });
        break;

      // -- insight --------------------------------------------------------
      case "insight_search_results": {
        const s = get();
        // Two askers, each dropping what is not theirs. Neither clobbers the
        // other, because the daemon echoes the query and both compare against
        // their own. `R-F13`.
        if (s.search.issued === msg.query && s.search.everywhere) {
          set({
            search: {
              ...s.search,
              insight: {
                running: false,
                hits: msg.results.hits,
                truncated: msg.results.truncated,
              },
            },
          });
        }
        set((st) => ({
          insight: {
            ...st.insight,
            searchPending: false,
            results: msg.query === st.insight.query.trim() ? [msg.query, msg.results] : st.insight.results,
          },
        }));
        break;
      }
      case "day_digest_report":
        set((s) => ({
          insight: {
            ...s.insight,
            digestPending: false,
            digest: msg.day === s.insight.day ? msg.digest : s.insight.digest,
          },
        }));
        break;
      case "recurring_failures":
        get().patchInsight({ failures: msg.failures });
        break;
      case "prompt_library":
        get().patchInsight({ prompts: msg.clusters });
        break;
      case "analytics_report":
        get().patchInsight({ analytics: msg.analytics });
        break;
      case "decision_report":
        set((s) => ({
          insight: {
            ...s.insight,
            decisions: { ...s.insight.decisions, [msg.session_id]: msg.candidates },
          },
        }));
        break;
      case "file_sessions":
        set((s) =>
          msg.path === s.insight.fileQuery.trim()
            ? { insight: { ...s.insight, fileSessions: [msg.path, msg.entries] } }
            : {},
        );
        break;
      case "doc_report":
        set((s) =>
          msg.repo === s.insight.docRepo.trim()
            ? { insight: { ...s.insight, docs: [msg.repo, msg.inventory] } }
            : {},
        );
        break;
      case "subagent_tree_report":
        set((s) => ({ subagents: { ...s.subagents, [msg.session_id]: msg.nodes } }));
        break;

      case "error":
        get().pushError(msg.message);
        break;

      default:
        // A message this build has never heard of. Ignored on purpose —
        // daemon and window upgrade separately, and a client that threw on an
        // unknown event would be a client that stops working when the daemon
        // gains a feature.
        break;
    }
  },
}));

/** Start the socket. Called once, from `main.tsx`. */
export function startClient(): DaemonClient {
  if (client) return client;
  const store = useStore.getState();
  client = new DaemonClient({
    url: store.url,
    onMessage: (msg) => useStore.getState().ingest(msg),
    onState: (conn, connDetail) => useStore.setState({ conn, connDetail: connDetail ?? null }),
  });
  const prefs = loadPrefs();
  useStore.setState({ prefs });
  applyAppZoom(prefs.appZoom);
  // Read once. It decides whether a terminal runs tmux here or over ssh, and
  // it must come from disk rather than be invented — see `lib/tmux.ts`.
  void machineIdFromShell().then((machineId) => useStore.setState({ machineId }));

  // Host a daemon if nothing is serving, **then** connect. ADR-0009: one
  // executable should be enough, so launching the app from a menu with nothing
  // else running has to work. Connecting first would race the bind.
  void ensureDaemon(store.url).finally(() => client?.connect());
  return client;
}

/**
 * Make sure something is serving before the socket dials.
 *
 * Three ways to do nothing, and each is deliberate:
 *
 * - **A browser has no daemon to host.** The dev server is a real client, but
 *   it cannot bind a port, and the window says so rather than hanging.
 * - **A non-loopback URL is a decision already made.** You told this window to
 *   watch another machine; starting a local daemon would answer a question
 *   nobody asked.
 * - **Somebody else is already serving.** The Rust side confirms it is actually
 *   mogeungd — an unrelated service on 7717 would otherwise leave the window on
 *   a socket that never connects, with no explanation.
 */
async function ensureDaemon(url: string): Promise<void> {
  if (!isTauri()) return;
  const addr = localAddrOf(url);
  if (!addr) return;
  try {
    const status = await daemonAcquire(addr);
    useStore.setState({ daemon: useStore.getState().daemon, daemonStatus: status });
    if (status.mode === "none") {
      useStore.getState().pushError(
        status.reason
          ? `nothing is serving ${addr}: ${status.reason}`
          : `nothing is serving ${addr} — the board will stay empty`,
      );
    }
  } catch (e) {
    useStore.getState().pushError(`could not start a daemon: ${String(e)}`);
  }
}

/**
 * Which session the surrounding pane is looking at, and whether it was told to.
 * `R-B49`.
 *
 * Outside a pane — the rail, the palette, a dialog — there is no binding and
 * this is the selection, which is what every caller got before panes could be
 * held at all.
 */
export interface PaneBinding {
  id: SessionId | null;
  /** Held on `id` rather than following the selection. */
  held: boolean;
}

export function usePaneBinding(): PaneBinding {
  const paneId = usePaneId();
  // Reading through `scoped()` rather than a stored object: the hold map is
  // machine-scoped, and this selects a **string** out of it, so the identity
  // trap `scoped()` documents cannot bite here.
  const held = useStore((s) => (paneId ? (s.scoped().paneHold[paneId] ?? null) : null));
  const selected = useStore((s) => s.selected);
  return held ? { id: held, held: true } : { id: selected, held: false };
}

/**
 * The session this pane is showing, or `null`. The single definition of
 * "current" — which since `R-B49` is a question with a per-pane answer.
 *
 * `null` from a **held** pane does not mean "nothing is selected", it means the
 * session that was held has gone. A caller that needs to tell them apart asks
 * `usePaneBinding` for the `held` flag; a caller that does not is right to
 * treat both as empty.
 */
export function useSelectedSession(): Session | null {
  const { id } = usePaneBinding();
  return useStore((s) => (id ? (s.sessions[id] ?? null) : null));
}

/**
 * Hold this pane on a session, or let it follow the selection again.
 *
 * Holding takes the session the pane is showing *now*, which is why this needs
 * no argument beyond the pane: "keep showing me this" is the whole gesture, and
 * asking which session to hold would be asking about the one already on screen.
 */
export function togglePaneHold(paneId: string): void {
  const { scoped, setScoped, selected } = useStore.getState();
  const hold = scoped().paneHold;
  const next = { ...hold };
  if (next[paneId]) delete next[paneId];
  else if (selected) next[paneId] = selected;
  else return; // Nothing on screen to hold onto.
  setScoped({ paneHold: next });
}
