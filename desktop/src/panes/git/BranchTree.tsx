/**
 * The branch pane: HEAD, Local, one node per remote, Tags — grouped on `/`,
 * searchable, starred. `R-D26`.
 *
 * A click **scopes the log** and checks nothing out; checking out is a menu
 * item and a write, and arrives with `R-D28`. The current branch is bold and
 * carries ↑↓ against its upstream, and the hover names the fetch those
 * numbers are as of — `R-D23`'s rule, kept: a number that can lie carries
 * its age.
 */

import { useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight, GitBranch, Star, Tag } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Empty, Input } from "@/ui/primitives";
import { ContextMenu, MenuItem, MenuSeparator } from "@/ui/Menu";
import { refTree, visible, type RefNode } from "@/lib/gitTree";
import { compareWith, copyText, scopeTo, selectCommit, toggleFavourite } from "@/lib/gitActions";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import { interactive, row as rowCls, rowSelected } from "@/ui/styles";

const NONE: string[] = [];

export function BranchTree({ id, repoRoot }: { id: string; repoRoot: string }) {
  const refs = useStore((s) => s.git[id]?.refs ?? null);
  const rev = useStore((s) => s.git[id]?.rev ?? null);
  const favs = useStore((s) => s.prefs.gitFavourites[repoRoot] ?? NONE);
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const [cursor, setCursor] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const rows = useMemo(
    () => (refs ? visible(refTree(refs, query, favs), collapsed, (r) => r.key) : []),
    [refs, query, favs, collapsed],
  );

  const toggle = (key: string) =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });

  const activate = (r: RefNode) => {
    if (r.kind === "group" || r.kind === "remote" || r.kind === "dir") {
      toggle(r.key);
      return;
    }
    if (!r.ref) return;
    scopeTo(id, r.ref, repoRoot);
    // A tag names a commit; scoping to it *and* selecting that commit is
    // what "show me v0.11.0" means.
    if (r.tag) selectCommit(id, r.tag.sha);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (rows.length === 0) return;
    const r = rows[cursor];
    if (e.key === "ArrowDown" || e.key === "j") setCursor(Math.min(rows.length - 1, cursor + 1));
    else if (e.key === "ArrowUp" || e.key === "k") setCursor(Math.max(0, cursor - 1));
    else if (e.key === "Enter") activate(r);
    else if (e.key === "ArrowLeft" && r && !collapsed.has(r.key) && r.kind !== "branch" && r.kind !== "tag" && r.kind !== "head") toggle(r.key);
    else if (e.key === "ArrowRight" && r && collapsed.has(r.key)) toggle(r.key);
    else return;
    e.preventDefault();
    const el = listRef.current?.children[e.key === "ArrowDown" || e.key === "j" ? Math.min(rows.length - 1, cursor + 1) : Math.max(0, cursor - 1)];
    (el as HTMLElement | undefined)?.scrollIntoView?.({ block: "nearest" });
  };

  const fetchAge = refs?.fetch_epoch ? `as of the fetch at ${stamp(refs.fetch_epoch)}` : "no fetch has run — these numbers may be stale";

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-7 shrink-0 items-center gap-1 border-b border-[var(--border)] px-1.5">
        <Input value={query} onChange={setQuery} placeholder="branch or tag" ariaLabel="branch or tag" className="h-5 text-2xs" />
      </div>
      {!refs ? (
        <Empty>reading refs…</Empty>
      ) : rows.length === 0 ? (
        <Empty hint="no branch or tag matches">nothing here</Empty>
      ) : (
        <div
          ref={listRef}
          role="tree"
          aria-label="branches and tags"
          tabIndex={0}
          onKeyDown={onKeyDown}
          className="min-h-0 flex-1 overflow-y-auto py-0.5 outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
        >
          {rows.map((r, i) => {
            const scoped = !!r.ref && r.ref === rev;
            const folder = r.kind === "group" || r.kind === "remote" || r.kind === "dir";
            const inner = (
              <div
                role="treeitem"
                aria-expanded={folder ? !collapsed.has(r.key) : undefined}
                aria-selected={scoped}
                data-cursor={i === cursor || undefined}
                onClick={() => {
                  setCursor(i);
                  activate(r);
                }}
                title={
                  r.branch
                    ? `${r.ref}${r.branch.upstream ? ` → ${r.branch.upstream}` : ""} · ${fetchAge}`
                    : r.tag
                      ? `${r.ref} → ${r.tag.sha} · ${stamp(r.tag.epoch)}`
                      : r.kind === "head"
                        ? "where the working tree is — click scopes the log to it"
                        : undefined
                }
                style={{ paddingLeft: 4 + r.depth * 12 }}
                className={cn(
                  rowCls,
                  "flex h-5 items-center gap-1 pr-1.5 text-xs whitespace-nowrap",
                  scoped && rowSelected,
                  i === cursor && "outline-1 -outline-offset-1 outline-[var(--border-hover)]",
                )}
              >
                {folder ? (
                  collapsed.has(r.key) ? (
                    <ChevronRight size={11} className="shrink-0 text-[var(--dim)]" />
                  ) : (
                    <ChevronDown size={11} className="shrink-0 text-[var(--dim)]" />
                  )
                ) : r.kind === "branch" ? (
                  <button
                    type="button"
                    tabIndex={-1}
                    aria-label={r.favourite ? `unfavourite ${r.ref}` : `favourite ${r.ref}`}
                    aria-pressed={r.favourite}
                    title={r.favourite ? "favourite — click to remove" : "favourite this branch"}
                    onClick={(e) => {
                      e.stopPropagation();
                      if (r.ref) toggleFavourite(repoRoot, r.ref);
                    }}
                    className={cn("grid h-4 w-3 shrink-0 place-items-center rounded-sm", interactive, r.favourite ? "text-[var(--amber)]" : "text-transparent hover:text-[var(--dim)]")}
                  >
                    <Star size={10} fill={r.favourite ? "currentColor" : "none"} />
                  </button>
                ) : (
                  <span className="w-3 shrink-0" />
                )}
                {r.kind === "branch" && <GitBranch size={11} className="shrink-0 text-[var(--dim)]" />}
                {r.kind === "tag" && <Tag size={11} className="shrink-0 text-[var(--amber)]" />}
                {r.kind === "head" && <Dim className="text-2xs font-semibold tracking-wider uppercase">HEAD →</Dim>}
                <span
                  className={cn(
                    "truncate",
                    r.kind === "group" && "text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase",
                    r.branch?.current && "font-semibold text-[var(--text-strong)]",
                    r.kind === "head" && "font-semibold text-[var(--text-strong)]",
                  )}
                >
                  {r.label}
                </span>
                {r.branch && (r.branch.ahead > 0 || r.branch.behind > 0) && (
                  <Dim className="ml-auto shrink-0 text-2xs tabular-nums">
                    {r.branch.ahead > 0 && `↑${r.branch.ahead}`}
                    {r.branch.behind > 0 && ` ↓${r.branch.behind}`}
                  </Dim>
                )}
                {r.branch && r.branch.ahead === 0 && r.branch.behind === 0 && r.branch.upstream && r.branch.current && (
                  <Dim className="ml-auto shrink-0 text-2xs">in sync</Dim>
                )}
                {folder && r.count > 0 && <Dim className="ml-auto shrink-0 text-2xs tabular-nums">{r.count}</Dim>}
                {r.kind === "group" && r.count === 0 && <Dim className="ml-auto shrink-0 text-2xs">none</Dim>}
              </div>
            );
            if (r.kind === "branch" || r.kind === "tag") {
              const isCurrent = !!r.branch?.current;
              return (
                <ContextMenu key={r.key} trigger={inner}>
                  <MenuItem onSelect={() => activate(r)}>Scope the log to {r.ref}</MenuItem>
                  {r.tag && <MenuItem onSelect={() => selectCommit(id, r.tag!.sha)}>Show the tagged commit</MenuItem>}
                  {r.branch && (
                    <MenuItem disabled={isCurrent} onSelect={() => compareWith(id, r.ref!)}>
                      Compare with the current branch — from the merge base
                    </MenuItem>
                  )}
                  <MenuSeparator />
                  <MenuItem onSelect={() => copyText(r.ref!)}>Copy name</MenuItem>
                  {r.branch && (
                    <MenuItem onSelect={() => toggleFavourite(repoRoot, r.ref!)}>
                      {r.favourite ? "Remove from favourites" : "Add to favourites"}
                    </MenuItem>
                  )}
                </ContextMenu>
              );
            }
            return <div key={r.key}>{inner}</div>;
          })}
        </div>
      )}
    </div>
  );
}
