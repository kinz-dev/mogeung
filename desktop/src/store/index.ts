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
import { DaemonClient, defaultUrl, type ConnState } from "@/wire/client";
import type {
  Analytics,
  AttentionItem,
  BlameLine,
  BlastRadius,
  Change,
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
import { defaultPrefs, emptyScoped, loadPrefs, savePrefs, setZoom, type Prefs, type ScopedPrefs } from "./prefs";

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
  group: 0 | 1;
  content: string | null;
  truncated: boolean;
  gotoLine: number | null;
}

export interface ExplorerState {
  dirs: Record<string, DirEntry[]>;
  expanded: string[];
  pending: string[];
  open: FileTab[];
  active: [number | null, number | null];
  focus: 0 | 1;
  reveal: string | null;
  treePaths: string[] | null;
  treeTruncated: boolean;
  treePending: boolean;
  /** Bodies already asked for, so a render is not a request. */
  pendingFiles: string[];
}

export const emptyExplorer = (): ExplorerState => ({
  dirs: {},
  expanded: [],
  pending: [],
  open: [],
  active: [null, null],
  focus: 0,
  reveal: null,
  treePaths: null,
  treeTruncated: false,
  treePending: false,
  pendingFiles: [],
});

export interface GitState {
  commits: CommitInfo[];
  done: boolean;
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
  fileDiff: Record<string, FileChange[]>;
  fetching: boolean;
  fetched: string[] | null;
}

export const emptyGit = (): GitState => ({
  commits: [],
  done: false,
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
  fileDiff: {},
  fetching: false,
  fetched: null,
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
  /** Epoch ms — a toast shows while it is young, the log keeps it after. */
  at: number;
  seen: boolean;
}

let noticeSeq = 0;

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
  changes: Record<SessionId, Change>;
  health: Health | null;
  debt: ReviewDebt | null;
  radius: BlastRadius | null;
  usage: UsageReport | null;
  notes: Note[];
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
  showKeymap: boolean;
  /** The daemon switcher. `R-I7`. */
  showConnections: boolean;
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

  // -- actions ------------------------------------------------------------
  send: (msg: ClientMsg) => void;
  ingest: (msg: ServerMsg) => void;
  select: (id: SessionId) => void;
  setPrefs: (patch: Partial<Prefs>) => void;
  scoped: () => ScopedPrefs;
  setScoped: (patch: Partial<ScopedPrefs>) => void;
  zoomPane: (pane: string, factor: number) => void;
  pushError: (msg: string) => void;
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

export const useStore = create<AppState>((set, get) => ({
  conn: "connecting",
  connDetail: null,
  daemon: null,
  firstSnapshot: false,

  sessions: {},
  queue: [],
  events: {},
  changes: {},
  health: null,
  debt: null,
  radius: null,
  usage: null,
  notes: [],
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
  showKeymap: false,
  showConnections: false,
  showTerminal: false,
  searchOpen: false,
  machineId: null,
  daemonStatus: null,
  rescanning: false,
  labelEditing: null,
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

  pushError: (text) =>
    set((s) => ({
      // Capped, because a daemon that fails in a loop must not grow the log
      // without bound — and the fiftieth copy of one message tells you nothing
      // the first did not.
      notices: [...s.notices, { id: ++noticeSeq, text, at: Date.now(), seen: false }].slice(-50),
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
        set({
          sessions,
          queue: msg.queue,
          daemon: msg.daemon ?? null,
          firstSnapshot: true,
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
        break;
      }
      case "session_updated":
        set((s) => ({ sessions: { ...s.sessions, [msg.session.id]: msg.session } }));
        break;
      case "session_removed":
        set((s) => {
          const sessions = { ...s.sessions };
          delete sessions[msg.session_id];
          return {
            sessions,
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
          for (const ev of msg.events) {
            const list = next[ev.session_id] ?? [];
            // Replayed history and live tail both land here; `seq` is the
            // identity, so a re-send is an update rather than a duplicate.
            const at = list.findIndex((e) => e.seq === ev.seq);
            if (at >= 0) list[at] = ev;
            else list.push(ev);
            next[ev.session_id] = list;
          }
          for (const id of Object.keys(next)) next[id] = [...next[id]].sort((a, b) => a.seq - b.seq);
          return { events: next };
        });
        break;
      }
      case "change_updated":
        set((s) => ({ changes: { ...s.changes, [msg.session_id]: msg.change } }));
        break;
      case "health":
        // The daemon sends this after every scan, which makes it the honest
        // signal that a rescan finished.
        set({ health: msg.health, rescanning: false });
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
      case "signal_status":
        set((s) => ({
          signals: {
            ...s.signals,
            [msg.repo]: { command: msg.command, running: msg.running, last: msg.last },
          },
        }));
        break;

      // -- explorer -------------------------------------------------------
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
              ? { ...t, content: msg.content, truncated: msg.truncated }
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
              ? { ...t, content: msg.content, truncated: msg.truncated }
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
          // echoes the scope back precisely so it can be dropped.
          if ((st.rev ?? null) !== (msg.rev ?? null)) return {};
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
          return {
            git: {
              ...s.git,
              [msg.session_id]: { ...st, fileDiff: { ...st.fileDiff, [msg.path]: msg.files } },
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
      case "git_stash_diff":
        get().patchGit(msg.session_id, { diff: msg.files });
        break;
      case "git_conflict_stages":
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

/** The selected session, or `null`. The single definition of "current". */
export function useSelectedSession(): Session | null {
  return useStore((s) => (s.selected ? (s.sessions[s.selected] ?? null) : null));
}
