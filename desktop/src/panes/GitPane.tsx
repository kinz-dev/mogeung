/**
 * The Git tool window. `R-D26`, the shape of
 * [feature 0042](../../../docs/features/0042-git-tool-window.md): Log, Local
 * changes, Stash and More as tabs, and the Log tab as three panes — the
 * branch tree, the graph log under its filter bar, and the commit inspector.
 *
 * **The write family is sent since `R-D28`** — stage, unstage, discard,
 * commit, branch, switch, stash, resolve — through `lib/gitActions.ts` and
 * nowhere else, guarded daemon-side by [ADR-0012](../../../docs/decisions/0012-write-locally-never-publish.md)'s
 * loopback-or-token rule and answered by a status re-broadcast, never by
 * anything this client models itself. It is
 * [A26](../../../docs/product/assumptions.md)'s test, run for the first time;
 * [feature 0025](../../../docs/features/0025-git-write-local.md)'s removal
 * condition stands.
 *
 * `git fetch` is the single outbound network call in the whole product, on an
 * explicit keystroke, admitted by
 * [ADR-0014](../../../docs/decisions/0014-fetch-is-not-publishing.md). It
 * never runs on a timer and it never pushes or merges.
 *
 * The tool lives in the bottom dock — chrome, one tool at a time, following
 * the selected session ([ADR-0017](../../../docs/decisions/0017-the-rail-is-chrome.md)).
 * Each region reads its own slice of the store and reaches the daemon
 * through `lib/gitActions.ts`, so this file is the arrangement and nothing
 * else.
 */

import { useEffect, useRef, useState } from "react";
import { CloudDownload, Columns2, PanelLeft, RefreshCw, X } from "lucide-react";
import { useStore, useSelectedSession, type GitTab } from "@/store";
import { Dim, Empty, IconButton, Segmented } from "@/ui/primitives";
import { sectionLabel } from "@/ui/styles";
import { repoName } from "@/wire/types";
import { askLog, pickRepo, repoOf, syncRepo } from "@/lib/gitActions";
import { stamp } from "@/lib/format";
import { BranchTree } from "./git/BranchTree";
import { LogToolbar, type Only } from "./git/LogToolbar";
import { LogTable } from "./git/LogTable";
import { CommitInspector } from "./git/CommitInspector";
import { LocalChanges } from "./git/LocalChanges";
import { StashTab } from "./git/StashTab";
import { MoreTab } from "./git/MoreTab";
import { ConsoleTab } from "./git/ConsoleTab";
import { Dropdown, DropItem, DropLabel, Splitter, useDragWidth, useWidth } from "./git/Dropdown";

// The tabs are `GitTab` in the store since `R-D31`: the operations popup
// routes to one — *stash changes* means the Stash tab — and a component
// holding that privately is a component nothing can send anybody to.
type Tab = GitTab;

/**
 * What the log keeps however wide the panes beside it are drawn: its
 * subject column at the floor, a graph and a date. The inspector's ceiling
 * is whatever this leaves — it used to be 42 % of the dock, which on a wide
 * monitor was the limit a hand met with room to spare (asked 2026-09-11:
 * *"is there a limit to the size, I can't resize to a bigger width?"*).
 */
const LOG_MIN = 320;
/** A stable empty list, so the selector does not re-render on every read. */
const NO_REPOS: string[] = [];

/** The last segment of a path — what a repository is called. */
function baseName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}
/** `Splitter` is `w-1`. */
const SPLITTER_PX = 4;

function Count({ n }: { n: number }) {
  return <span className="ml-1 rounded-sm bg-[var(--bg-faint)] px-1 text-2xs leading-3 tabular-nums text-[var(--dim)]">{n}</span>;
}

export function GitPane() {
  const s = useSelectedSession();
  const id = s?.id ?? null;
  /**
   * The repository this window is showing, and the ones it could. `R-D34`.
   *
   * `git[id].repo` is the pick and `null` means the session's own — so a
   * session that has never picked anything behaves exactly as it always did.
   * The list comes from the workspace answer, because only the daemon can say
   * which of a session's roots hold a `.git`.
   */
  const picked = useStore((st) => (id ? (st.git[id]?.repo ?? null) : null));
  const repoRoot = picked ?? s?.repo_root ?? null;
  const repos = useStore((st) => (id ? (st.explorer[id]?.workspace?.repos ?? NO_REPOS) : NO_REPOS));
  const knowsWorkspace = useStore((st) => (id ? !!st.explorer[id]?.workspace : false));
  // Field by field, never the whole slice: this header sits above every row
  // of the log and every line of the diff.
  const logAsked = useStore((st) => (id ? (st.git[id]?.logAsked ?? false) : false));
  const hasStatus = useStore((st) => (id ? !!st.git[id]?.status : false));
  const hasRefs = useStore((st) => (id ? !!st.git[id]?.refs : false));
  const changed = useStore((st) => (id ? (st.git[id]?.status?.filter((e) => e.state !== "!!").length ?? null) : null));
  const stashCount = useStore((st) => (id ? (st.git[id]?.stashes?.length ?? null) : null));
  const fetching = useStore((st) => (id ? (st.git[id]?.fetching ?? false) : false));
  const fetched = useStore((st) => (id ? (st.git[id]?.fetched ?? null) : null));
  const fetchEpoch = useStore((st) => (id ? (st.git[id]?.refs?.fetch_epoch ?? null) : null));
  const patchGit = useStore((st) => st.patchGit);
  const send = useStore((st) => st.send);
  const sideBySide = useStore((st) => st.prefs.sideBySide);
  const branchesOpen = useStore((st) => st.prefs.gitBranchesOpen);
  const savedBranches = useStore((st) => st.prefs.gitBranchesWidth);
  const savedInspector = useStore((st) => st.prefs.gitInspectorWidth);
  const setPrefs = useStore((st) => st.setPrefs);
  const tab = useStore((st) => st.gitTab);
  const setTab = (next: Tab) => useStore.setState({ gitTab: next });
  const [only, setOnly] = useState<Only>({ session: false, read: false });
  const rootRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  // The outer panes are remembered in pixels and drawn no wider than the log
  // can afford: a dock 620 px wide with a 210 px branch pane and a 360 px
  // inspector left the log 50 px, which is what the first browser look found.
  // The branch pane is at most a share; the inspector may take everything
  // the log's floor leaves, since a wide dock is exactly where a wide diff
  // wants to be. The saved widths are untouched; a wider window gets them
  // back — and the drag is clamped to the same ceiling, so the edge stops
  // where the pane does rather than saving a width it cannot draw.
  const total = useWidth(bodyRef);
  const [branchesWidth, dragBranches] = useDragWidth(savedBranches, 160, 480, (px) => setPrefs({ gitBranchesWidth: px }));
  const bw = total > 0 ? Math.min(branchesWidth, Math.floor(total * 0.28)) : branchesWidth;
  const inspectorMax = total > 0 ? Math.max(260, total - (branchesOpen ? bw + SPLITTER_PX : 0) - SPLITTER_PX - LOG_MIN) : Infinity;
  const [inspectorWidth, dragInspector] = useDragWidth(savedInspector, 260, inspectorMax, (px) => setPrefs({ gitInspectorWidth: px }), -1);
  const iw = Math.min(inspectorWidth, inspectorMax);

  // One door for the first questions, in the render, so a docked pane works
  // unswitched — and **only for a repository**: a session outside one used to
  // fire three questions the daemon could only answer with an error.
  // The picker's list, asked for once. Cheap — the Files tool asks for the
  // same answer — and it is the only way to learn which roots are checkouts.
  useEffect(() => {
    if (id && !knowsWorkspace) send({ cmd: "fetch_workspace", session_id: id });
  }, [id, knowsWorkspace, send]);

  /**
   * A session with no repository of its own adopts the first it may show.
   * `R-D34`.
   *
   * This is the layout the feature was asked from: `claude` started in the
   * folder *holding* four checkouts is in none of them, so there is nothing to
   * fall back to and the window would say "not in a git repo" beside a picker
   * full of repositories. Picking one is a real pick — `repoOf` has to answer
   * it, or every request would resolve to the session's own root and fail.
   */
  useEffect(() => {
    if (id && !picked && !s?.repo_root && repos.length > 0) pickRepo(id, repos[0]);
  }, [id, picked, s?.repo_root, repos]);

  useEffect(() => {
    if (!id || !repoRoot) return;
    // Because nobody has asked, not because the list is empty: a filtered
    // query empties the list on purpose. Reload clears the flag.
    if (!logAsked) askLog(id, 0);
    if (!hasStatus) send({ cmd: "git_status", session_id: id, repo: repoOf(id) });
    if (!hasRefs) send({ cmd: "git_refs", session_id: id, repo: repoOf(id) });
  }, [id, repoRoot, logAsked, hasStatus, hasRefs, send]);

  if (!s) return <Empty>select a session</Empty>;
  // A session **outside** a repository is no longer the end of it: the whole
  // point of `R-D34` is the workspace folder holding four checkouts, where the
  // session's own root is not one of them.
  if (!repoRoot) {
    return (
      <Empty hint={repos.length > 0 ? "pick one from the header" : "git needs a repository — add one in Files"}>
        this session is not in a git repo
      </Empty>
    );
  }
  const sid = s.id;
  const root = repoRoot;

  const focusLog = () => rootRef.current?.querySelector<HTMLElement>('[role="listbox"]')?.focus();
  const focusInspector = () => rootRef.current?.querySelector<HTMLElement>('[aria-label="the selected commit"]')?.focus();

  return (
    <div ref={rootRef} className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
        {repos.length > 1 ? (
          // More than one to choose from, so the name becomes the control.
          // `R-D34`; below two it is a label, because a picker with one entry
          // is a button that does nothing.
          <Dropdown label="Git" value={baseName(root)} title={root} active={!!picked}>
            <DropLabel>Repository</DropLabel>
            {repos.map((r) => (
              <DropItem
                key={r}
                mono
                active={r === root}
                onSelect={() => pickRepo(sid, r === s.repo_root ? null : r)}
              >
                {baseName(r)}
                {r === s.repo_root && <Dim className="ml-auto text-2xs">this session's</Dim>}
              </DropItem>
            ))}
          </Dropdown>
        ) : (
          <span className={sectionLabel} title={root}>
            Git · <span className="font-mono tracking-normal text-[var(--text)] normal-case">{repoName(s)}</span>
          </span>
        )}
        <Segmented
          value={tab}
          onChange={setTab}
          options={[
            { value: "log", label: "Log", title: "commits — branches, the graph, and what each one changed" },
            {
              value: "local",
              label: (
                <>
                  Local changes
                  {changed !== null && changed > 0 && <Count n={changed} />}
                </>
              ),
              title: "the working tree: staged, unstaged, untracked, conflicted",
            },
            {
              value: "stash",
              label: (
                <>
                  Stash
                  {stashCount !== null && stashCount > 0 && <Count n={stashCount} />}
                </>
              ),
              title: "shelved work",
            },
            { value: "console", label: "Console", title: "what this window asked git, and what git said" },
            { value: "more", label: "More", title: "reflog, worktrees, submodules" },
          ]}
        />
        <div className="ml-auto flex items-center gap-0.5">
          {fetchEpoch !== null ? (
            <Dim className="mr-1 text-2xs" title="ahead/behind counts are as of this fetch (R-D23)">
              fetched {stamp(fetchEpoch)}
            </Dim>
          ) : (
            hasRefs && (
              <Dim className="mr-1 text-2xs" title="no fetch has ever run here — ahead/behind may be stale (R-D23)">
                never fetched
              </Dim>
            )
          )}
          {tab === "log" && (
            <IconButton
              title="show or hide the branch pane"
              active={branchesOpen}
              onClick={() => setPrefs({ gitBranchesOpen: !branchesOpen })}
            >
              <PanelLeft size={13} />
            </IconButton>
          )}
          <IconButton
            title="side by side — the file as it was left, as it is right  (R-D6)"
            active={sideBySide}
            onClick={() => setPrefs({ sideBySide: !sideBySide })}
          >
            <Columns2 size={13} />
          </IconButton>
          <IconButton
            title={fetching ? "fetching…" : "update remote-tracking refs — the only outbound call there is (Ctrl+T)"}
            disabled={fetching}
            onClick={() => void syncRepo(sid)}
          >
            <CloudDownload size={13} />
          </IconButton>
          <IconButton
            title="reload — ask git again for everything this window shows"
            onClick={() =>
              patchGit(sid, {
                commits: [],
                status: null,
                refs: null,
                stashes: null,
                reflog: null,
                worktrees: null,
                submodules: null,
                logAsked: false,
              })
            }
          >
            <RefreshCw size={13} />
          </IconButton>
        </div>
      </div>

      {fetched && (
        <div className="flex max-h-24 shrink-0 items-start gap-2 overflow-y-auto border-b border-[var(--border)] px-2 py-1">
          <Dim className="min-w-0 flex-1 text-2xs whitespace-pre-wrap">
            {fetched.length === 0 ? "fetch: the remotes had nothing new" : fetched.join("\n")}
          </Dim>
          <IconButton title="dismiss" onClick={() => patchGit(sid, { fetched: null })}>
            <X size={11} />
          </IconButton>
        </div>
      )}

      <div ref={bodyRef} className="flex min-h-0 flex-1">
        {tab === "log" && (
          <>
            {branchesOpen && (
              <>
                <div className="flex shrink-0 flex-col" style={{ width: bw }}>
                  <BranchTree id={sid} repoRoot={root} />
                </div>
                <Splitter onMouseDown={dragBranches} />
              </>
            )}
            <div className="flex min-w-0 flex-1 flex-col">
              <LogToolbar id={sid} repoRoot={root} only={only} setOnly={setOnly} />
              <LogTable id={sid} only={only} onEnter={focusInspector} />
            </div>
            <Splitter onMouseDown={dragInspector} />
            <div className="flex shrink-0 flex-col" style={{ width: iw }}>
              <CommitInspector id={sid} onBack={focusLog} />
            </div>
          </>
        )}
        {tab === "local" && <LocalChanges id={sid} repoRoot={root} />}
        {tab === "stash" && <StashTab id={sid} />}
        {tab === "console" && <ConsoleTab id={sid} />}
        {tab === "more" && <MoreTab id={sid} />}
      </div>
    </div>
  );
}
