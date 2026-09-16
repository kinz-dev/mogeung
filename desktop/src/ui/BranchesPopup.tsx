/**
 * The Branches popup. `R-D32`, on `7` from the operations popup and on
 * `Ctrl+Shift+` ` directly.
 *
 * IntelliJ's Git Branches popup: one search box over **branches and actions
 * together**, the repository and the branch it is on, the branches you were
 * last looking at, then Local, Remote and Tags grouped on `/`.
 *
 * Almost none of this is new data. `refTree` has built that tree since `R-D26`
 * — with the query filtering and the favourites already in it — `gitRecents`
 * has remembered the recent refs, and `switchTo`, `compareWith` and
 * `toggleFavourite` are the same verbs the branch pane calls. What is new is
 * the **shape**: a tree in a pane you open, versus a list that arrives on the
 * key you were already pressing.
 *
 * Two rows are not routing, and both are stated where they are drawn:
 *
 *  - *Update Project* is `Ctrl+T`, which mogeung already binds to `git fetch`.
 *    It reads the remote and merges nothing
 *    ([ADR-0014](../../../docs/decisions/0014-fetch-is-not-publishing.md)),
 *    which is the honest half of IntelliJ's command and the only half this
 *    product will ever have.
 *  - *Checkout Tag or Revision* is why `git_switch` grew `detach`: `git switch`
 *    refuses anything that is not a branch.
 *
 * Checking out goes behind `R-D21`'s warning — named live sessions in this
 * worktree, and proceeding only on a confirm — because the popup must not be a
 * way around a guard the pane has.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { GitBranch, Search, Star, Tag } from "lucide-react";
import { useStore } from "@/store";
import { Button, Checkbox, Dim, Input, Kbd, Mono } from "@/ui/primitives";
import { Dialog } from "@/ui/Dialog";
import { ContextMenu, MenuItem, MenuSeparator } from "@/ui/Menu";
import { refTree, type RefNode } from "@/lib/gitTree";
import {
  branchCreate,
  compareWith,
  copyText,
  liveSessionsIn,
  recentsOf,
  scopeTo,
  switchTo,
  toggleFavourite,
} from "@/lib/gitActions";
import { GIT_OPS, opsContext, openGit, type GitOp, type OpsContext } from "@/lib/gitOps";
import { ACTIONS, bindingsFor, formatChord } from "@/lib/keymap";
import { cn } from "@/lib/cn";
import { interactive, row as rowCls, rowSelected } from "@/ui/styles";

const NONE: string[] = [];

/** The rows above the refs: three shared entries, and the two dialogs. */
type ActionRow = {
  id: string;
  label: string;
  note?: string;
  /** A keymap action id whose live binding is drawn beside the label. */
  action?: string;
  unavailable(ctx: OpsContext): string | null;
};

const shared = (id: string): ActionRow => {
  const op = GIT_OPS.find((o) => o.id === id) as GitOp;
  return { id: op.id, label: op.label, note: op.note, action: op.action, unavailable: op.unavailable };
};

type Row =
  | { kind: "action"; row: ActionRow }
  | { kind: "ref"; node: RefNode; recent?: boolean }
  | { kind: "header"; label: string };

export function BranchesPopup() {
  const open = useStore((s) => s.gitPopup === "branches");
  const id = useStore((s) => s.selected);
  const session = useStore((s) => (s.selected ? s.sessions[s.selected] : undefined));
  const refs = useStore((s) => (id ? (s.git[id]?.refs ?? null) : null));
  const overrides = useStore((s) => s.prefs.keymap);
  const repoRoot = session?.repo_root ?? null;
  const favs = useStore((s) => (repoRoot ? (s.prefs.gitFavourites[repoRoot] ?? NONE) : NONE));
  const send = useStore((s) => s.send);

  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const [dialog, setDialog] = useState<null | "branch" | "revision">(null);
  const [newName, setNewName] = useState("");
  const [newSwitch, setNewSwitch] = useState(true);
  const [revision, setRevision] = useState("");
  /** A checkout waiting on `R-D21`'s warning: the ref, and who is live. */
  const [checkout, setCheckout] = useState<{ ref: string; detach: boolean; live: { id: string; title: string }[] } | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const ctx = useMemo(() => (open ? opsContext() : null), [open]);

  // The refs are asked for here as well as by the pane: this popup is reachable
  // with the Git tool window never once opened, and an empty branch list would
  // read as a repository with no branches.
  useEffect(() => {
    if (open && id && repoRoot && !refs) send({ cmd: "git_refs", session_id: id });
  }, [open, id, repoRoot, refs, send]);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setDialog(null);
      setCheckout(null);
      return;
    }
    setCursor(0);
    inputRef.current?.focus();
  }, [open]);

  const rows = useMemo<Row[]>(() => {
    if (!ctx) return [];
    const q = query.trim().toLowerCase();
    const out: Row[] = [];

    const actions: ActionRow[] = [
      shared("fetch"),
      shared("commit"),
      shared("push"),
      {
        id: "new_branch",
        label: "New Branch…",
        unavailable: (c) => (c.repoRoot ? null : "that session is not in a git repository"),
      },
      {
        id: "checkout_revision",
        label: "Checkout Tag or Revision…",
        note: "detaches HEAD",
        unavailable: (c) => (c.repoRoot ? null : "that session is not in a git repository"),
      },
    ];
    for (const row of actions) {
      if (!q || row.label.toLowerCase().includes(q)) out.push({ kind: "action", row });
    }

    if (!refs) return out;

    // Recent branches, and only unfiltered: with a query the tree below is
    // already the answer, and the same branch twice in one list is noise.
    if (!q && repoRoot) {
      const recents = recentsOf(repoRoot)
        .map((ref) => refs.branches.find((b) => b.name === ref) ?? refs.remote_branches.find((b) => b.name === ref))
        .filter((b): b is NonNullable<typeof b> => !!b);
      if (recents.length > 0) {
        out.push({ kind: "header", label: "Recent branches" });
        for (const b of recents) {
          out.push({
            kind: "ref",
            recent: true,
            node: { kind: "branch", key: `recent:${b.name}`, label: b.name, depth: 1, ref: b.name, count: 1, branch: b },
          });
        }
      }
    }

    for (const node of refTree(refs, query, favs)) {
      // The tree's own HEAD row is drawn as the repository line above, so it
      // would be the same fact twice.
      if (node.kind === "head") continue;
      out.push({ kind: "ref", node });
    }
    return out;
  }, [ctx, query, refs, favs, repoRoot]);

  const selectable = (r: Row) => r.kind === "action" || (r.kind === "ref" && !!r.node.ref);

  useEffect(() => {
    // Typing changes the list under the cursor; the first thing you can act on
    // is the only sane place for it to land.
    setCursor(rows.findIndex(selectable));
  }, [rows]);

  if (!open || !ctx) return null;

  const close = () => useStore.setState({ gitPopup: null });

  /** Check out: straight away when nothing is running here, behind the
   *  warning when something is. `R-D21`, the branch pane's rule. */
  const checkOut = (ref: string, detach: boolean) => {
    if (!id) return;
    const live = repoRoot ? liveSessionsIn(repoRoot) : [];
    if (live.length === 0) {
      switchTo(id, ref, detach);
      close();
      return;
    }
    setCheckout({ ref, detach, live });
  };

  const runAction = (row: ActionRow) => {
    const why = row.unavailable(ctx);
    if (why) {
      useStore.getState().pushError(`${row.label} — ${why}`);
      return;
    }
    if (row.id === "new_branch") {
      setNewName("");
      setDialog("branch");
      return;
    }
    if (row.id === "checkout_revision") {
      setRevision("");
      setDialog("revision");
      return;
    }
    const op = GIT_OPS.find((o) => o.id === row.id);
    op?.run(ctx);
    close();
  };

  const activate = (r: Row) => {
    if (r.kind === "action") return runAction(r.row);
    if (r.kind === "ref" && r.node.ref) return checkOut(r.node.ref, false);
  };

  const move = (delta: number) => {
    setCursor((c) => {
      let n = c;
      for (let i = 0; i < rows.length; i++) {
        n = (n + delta + rows.length) % rows.length;
        if (selectable(rows[n])) return n;
      }
      return c;
    });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const r = rows[cursor];
      if (r) activate(r);
    }
  };

  const head = refs?.head ?? (refs ? `detached at ${refs.head_sha.slice(0, 8)}` : null);
  const current = refs?.branches.find((b) => b.current);

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[10vh]" onClick={close}>
      <div
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        role="dialog"
        aria-label="Branches"
        className="flex max-h-[70vh] w-[520px] max-w-[92vw] flex-col overflow-hidden rounded-md border border-[var(--window-stroke)] bg-[var(--bg-raised)] shadow-[var(--elev-3)]"
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-3 py-1.5">
          <Search size={13} className="shrink-0 text-[var(--dim)]" />
          <Input
            inputRef={inputRef}
            value={query}
            onChange={setQuery}
            placeholder="Search for branches and actions…"
            className="flex-1"
          />
        </div>

        {!ctx.repoRoot ? (
          <div className="px-3 py-6 text-center text-sm text-[var(--dim)]">
            {ctx.session ? "this session is not in a git repository" : "no session is selected"}
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto py-1" role="listbox" aria-label="branches and actions">
            {/* The repository line: IntelliJ lists every repository it has open;
                a mogeung session is exactly one worktree, so this is the row
                that would be a list, and it says which one you are acting on. */}
            <div className="flex items-center gap-2 px-3 py-1">
              <span className="h-2.5 w-2.5 shrink-0 rounded-sm bg-[var(--graph-2)]" />
              <Mono className="truncate text-xs text-[var(--text-strong)]">
                {repoRoot?.split("/").filter(Boolean).pop()}
              </Mono>
              <Dim className="ml-auto truncate text-2xs">{head}</Dim>
              {current && (current.ahead > 0 || current.behind > 0) && (
                <Dim className="shrink-0 text-2xs tabular-nums">
                  ↑{current.ahead} ↓{current.behind}
                </Dim>
              )}
            </div>
            <div className="my-1 border-t border-[var(--border)]" />

            {rows.map((r, i) => {
              if (r.kind === "header") {
                return (
                  <Dim key={`h:${r.label}`} className="block px-3 pt-2 pb-0.5 text-2xs tracking-wider uppercase">
                    {r.label}
                  </Dim>
                );
              }
              if (r.kind === "action") {
                const why = r.row.unavailable(ctx);
                const action = ACTIONS.find((a) => a.id === r.row.action);
                const chord = action ? bindingsFor(action, overrides)[0] : null;
                return (
                  <button
                    key={`a:${r.row.id}`}
                    type="button"
                    role="option"
                    aria-selected={i === cursor}
                    aria-disabled={!!why}
                    title={why ?? undefined}
                    onMouseEnter={() => setCursor(i)}
                    onClick={() => runAction(r.row)}
                    className={cn(
                      interactive,
                      "flex w-full items-center gap-2 px-3 py-[3px] text-left text-sm",
                      i === cursor && rowSelected,
                      why && "text-[var(--dim)]",
                    )}
                  >
                    <span className="flex-1 truncate">{r.row.label}</span>
                    {r.row.note && <Dim className="truncate text-2xs">{r.row.note}</Dim>}
                    {chord && <Kbd>{formatChord(chord)}</Kbd>}
                  </button>
                );
              }

              const n = r.node;
              const isGroup = !n.ref;
              const row = (
                <div
                  role="option"
                  aria-selected={i === cursor}
                  onMouseEnter={() => !isGroup && setCursor(i)}
                  onClick={() => activate(r)}
                  style={{ paddingLeft: 12 + n.depth * 12 }}
                  className={cn(
                    isGroup ? "cursor-default" : rowCls,
                    "flex items-center gap-1.5 py-[3px] pr-3 text-sm",
                    isGroup && "text-2xs tracking-wider text-[var(--dim)] uppercase",
                    i === cursor && !isGroup && rowSelected,
                  )}
                >
                  {!isGroup &&
                    (n.kind === "tag" ? (
                      <Tag size={11} className="shrink-0 text-[var(--dim)]" />
                    ) : (
                      <GitBranch size={11} className="shrink-0 text-[var(--dim)]" />
                    ))}
                  <span className={cn("truncate", n.branch?.current && "font-semibold text-[var(--text-strong)]")}>
                    {n.label}
                  </span>
                  {n.favourite && <Star size={10} className="shrink-0 fill-current text-[var(--amber)]" />}
                  {n.branch?.upstream && (
                    <Dim className="ml-auto truncate text-2xs">{n.branch.upstream}</Dim>
                  )}
                  {isGroup && <Dim className="ml-auto text-2xs tabular-nums">{n.count}</Dim>}
                </div>
              );

              if (isGroup) return <div key={n.key}>{row}</div>;

              // The per-branch actions IntelliJ puts behind `→`. Merge, rebase
              // and pull-into are absent by ADR-0014, not by omission.
              return (
                <ContextMenu key={n.key} trigger={row}>
                  <>
                      <MenuItem onSelect={() => checkOut(n.ref!, false)}>Checkout</MenuItem>
                      <MenuItem
                        onSelect={() => {
                          if (id) compareWith(id, n.ref!);
                          openGit("log");
                          close();
                        }}
                      >
                        Compare with current
                      </MenuItem>
                      <MenuItem
                        onSelect={() => {
                          if (id) scopeTo(id, n.ref!, repoRoot);
                          openGit("log");
                          close();
                        }}
                      >
                        Show its log
                      </MenuItem>
                      <MenuSeparator />
                      <MenuItem onSelect={() => copyText(n.ref!)}>Copy branch name</MenuItem>
                    <MenuItem onSelect={() => repoRoot && toggleFavourite(repoRoot, n.ref!)}>
                      {n.favourite ? "Remove from favourites" : "Mark as favourite"}
                    </MenuItem>
                  </>
                </ContextMenu>
              );
            })}
          </div>
        )}
      </div>

      {dialog === "branch" && (
        <Dialog title="New branch" subtitle="from HEAD, which is what git branch does" onClose={() => setDialog(null)}>
          <div className="flex flex-col gap-3 px-3 py-3" onClick={(e) => e.stopPropagation()}>
            <Input value={newName} onChange={setNewName} placeholder="feature/thing" autoFocus />
            <Checkbox checked={newSwitch} onChange={setNewSwitch} label="switch to it" />
            <div className="flex items-center gap-2">
              <Button
                variant="solid"
                disabled={!newName.trim()}
                onClick={() => {
                  if (id && newName.trim()) branchCreate(id, newName.trim(), newSwitch);
                  setDialog(null);
                  close();
                }}
              >
                Create
              </Button>
              <Button variant="outline" onClick={() => setDialog(null)}>
                Cancel
              </Button>
            </div>
          </div>
        </Dialog>
      )}

      {dialog === "revision" && (
        <Dialog
          title="Checkout tag or revision"
          subtitle="git switch --detach — HEAD stops being on a branch"
          onClose={() => setDialog(null)}
        >
          <div className="flex flex-col gap-3 px-3 py-3" onClick={(e) => e.stopPropagation()}>
            <Input value={revision} onChange={setRevision} placeholder="v1.4.0, a sha, origin/main…" autoFocus />
            <Dim className="text-xs">
              Commit from a detached HEAD and the commit belongs to no branch — git says so on arrival, and the
              reflog is how it is found again.
            </Dim>
            <div className="flex items-center gap-2">
              <Button
                variant="solid"
                disabled={!revision.trim()}
                onClick={() => {
                  const ref = revision.trim();
                  setDialog(null);
                  if (ref) checkOut(ref, true);
                }}
              >
                Checkout
              </Button>
              <Button variant="outline" onClick={() => setDialog(null)}>
                Cancel
              </Button>
            </div>
          </div>
        </Dialog>
      )}

      {checkout && (
        <Dialog
          title="Check out while sessions are running?"
          subtitle="the worktree is shared — their files move underneath them"
          onClose={() => setCheckout(null)}
        >
          <div className="flex flex-col gap-3 px-3 py-3" onClick={(e) => e.stopPropagation()}>
            <Dim className="text-xs">
              {checkout.live.length} session(s) are live in this worktree:
            </Dim>
            <ul className="m-0 max-h-40 list-none overflow-y-auto p-0">
              {checkout.live.map((l) => (
                <li key={l.id} className="truncate py-0.5 text-xs text-[var(--text)]">
                  {l.title}
                </li>
              ))}
            </ul>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                className="border-[var(--amber)] text-[var(--amber)]"
                onClick={() => {
                  if (id) switchTo(id, checkout.ref, checkout.detach);
                  setCheckout(null);
                  close();
                }}
              >
                Check out {checkout.ref}
              </Button>
              <Button variant="outline" onClick={() => setCheckout(null)}>
                Cancel
              </Button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
}
