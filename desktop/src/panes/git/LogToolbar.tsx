/**
 * The filter bar above the log: text or hash, then Branch, User, Date and
 * Paths as dropdowns that show what they hold, then the two toggles that are
 * mogeung's own. `R-D26`.
 *
 * Enter runs the query, which is cheap to forget — and a filter typed but
 * not run looks exactly like a filter ignored. So a box that differs from
 * the list says so, and a filter in force is named with the way out of it.
 * The search box speaks `R-D12`'s syntax — `author:`, `path:`, `find:` — so
 * every field the dropdowns reach by mouse is reachable from the keyboard.
 */

import { useMemo, useRef, useState } from "react";
import * as PopoverPrimitive from "@radix-ui/react-popover";
import { useStore } from "@/store";
import { Chip, Dim, Input } from "@/ui/primitives";
import { interactive, surface } from "@/ui/styles";
import { cn } from "@/lib/cn";
import {
  DATE_PRESETS,
  emptyQuery,
  inForce,
  looksLikeSha,
  parseSearch,
  presetOf,
  presetRange,
  searchText,
} from "@/lib/gitFilter";
import { askLog, queryOf, scopeTo, selectCommit } from "@/lib/gitActions";
import { Dropdown, DropItem, DropLabel, DropSeparator, DropSub } from "./Dropdown";

export interface Only {
  /** Only commits this session probably produced. `R-D13`. */
  session: boolean;
  /** Only commits every hunk of which has been read. `R-D17`. */
  read: boolean;
}

export function LogToolbar({
  id,
  repoRoot,
  only,
  setOnly,
}: {
  id: string;
  repoRoot: string;
  only: Only;
  setOnly: (o: Only) => void;
}) {
  const grep = useStore((s) => s.git[id]?.grep ?? "");
  const author = useStore((s) => s.git[id]?.author ?? "");
  const path = useStore((s) => s.git[id]?.path ?? "");
  const pickaxe = useStore((s) => s.git[id]?.pickaxe ?? "");
  const rev = useStore((s) => s.git[id]?.rev ?? null);
  const since = useStore((s) => s.git[id]?.since ?? null);
  const until = useStore((s) => s.git[id]?.until ?? null);
  const commits = useStore((s) => s.git[id]?.commits);
  const refs = useStore((s) => s.git[id]?.refs ?? null);
  const favs = useStore((s) => s.prefs.gitFavourites[repoRoot]);
  const recents = useStore((s) => s.prefs.gitRecents[repoRoot]);

  const applied = searchText({ grep, author, path, pickaxe });
  const [text, setText] = useState(applied);
  const [pathText, setPathText] = useState(path);
  // The box starts at what the list answers, per session — adjusted during
  // the render for the session that changed, React's own answer to state
  // derived from a prop.
  const syncedFor = useRef<string | null>(null);
  if (syncedFor.current !== id) {
    syncedFor.current = id;
    setText(applied);
    setPathText(path);
  }
  const unrun = text.trim() !== applied;

  const run = () => {
    const t = text.trim();
    if (looksLikeSha(t)) {
      selectCommit(id, t);
      return;
    }
    askLog(id, 0, parseSearch(t));
  };

  const authors = useMemo(() => {
    const n = new Map<string, number>();
    for (const c of commits ?? []) n.set(c.author, (n.get(c.author) ?? 0) + 1);
    return [...n.entries()].sort((a, b) => b[1] - a[1]).map(([a]) => a);
  }, [commits]);

  const q = queryOf(id);
  const force = inForce({ ...q, grep, author, path, pickaxe, rev, since, until });
  const preset = presetOf(since, until);
  const dateValue = preset ? DATE_PRESETS.find((d) => d.value === preset)!.label : "custom";

  return (
    <div className="shrink-0 border-b border-[var(--border)]">
      <div className="flex min-h-7 flex-wrap items-center gap-x-1 gap-y-0.5 px-1.5 py-0.5">
        <Input
          value={text}
          onChange={setText}
          placeholder="text or hash — Enter"
          ariaLabel="text or hash"
          className="h-5 max-w-64 text-2xs"
          onKeyDown={(e) => {
            if (e.key === "Enter") run();
            if (e.key === "Escape") setText(applied);
          }}
        />
        <Dropdown label="Branch" value={rev ?? "all"} active={!!rev} title="scope the log to one ref — nothing is checked out">
          <DropItem active={!rev} onSelect={() => scopeTo(id, null)}>
            all branches
          </DropItem>
          {refs?.head && (
            <DropItem active={rev === refs.head} onSelect={() => scopeTo(id, refs.head!, repoRoot)}>
              HEAD → {refs.head}
            </DropItem>
          )}
          {(favs?.length ?? 0) > 0 && (
            <>
              <DropLabel>favourites</DropLabel>
              {favs!.map((f) => (
                <DropItem key={f} mono active={rev === f} onSelect={() => scopeTo(id, f, repoRoot)}>
                  {f}
                </DropItem>
              ))}
            </>
          )}
          {(recents?.length ?? 0) > 0 && (
            <>
              <DropLabel>recent</DropLabel>
              {recents!.map((f) => (
                <DropItem key={f} mono active={rev === f} onSelect={() => scopeTo(id, f, repoRoot)}>
                  {f}
                </DropItem>
              ))}
            </>
          )}
          <DropSeparator />
          <DropSub label="Local" disabled={!refs || refs.branches.length === 0}>
            {refs?.branches.map((b) => (
              <DropItem key={b.name} mono active={rev === b.name} onSelect={() => scopeTo(id, b.name, repoRoot)}>
                {b.name}
              </DropItem>
            ))}
          </DropSub>
          {remoteNames(refs?.remote_branches.map((b) => b.name) ?? []).map((remote) => (
            <DropSub key={remote} label={remote}>
              {refs!.remote_branches
                .filter((b) => b.name.startsWith(`${remote}/`))
                .map((b) => (
                  <DropItem key={b.name} mono active={rev === b.name} onSelect={() => scopeTo(id, b.name, repoRoot)}>
                    {b.name.slice(remote.length + 1)}
                  </DropItem>
                ))}
            </DropSub>
          ))}
          <DropSub label="Tags" disabled={!refs || refs.tags.length === 0}>
            {refs?.tags.map((t) => (
              <DropItem key={t.name} mono active={rev === t.name} onSelect={() => scopeTo(id, t.name, repoRoot)}>
                {t.name}
              </DropItem>
            ))}
          </DropSub>
        </Dropdown>
        <Dropdown label="User" value={author || "anyone"} active={!!author} title="the authors seen in the log so far — or type author:name in the box">
          <DropItem active={!author} onSelect={() => askLog(id, 0, { author: "" })}>
            anyone
          </DropItem>
          {authors.length > 0 && <DropSeparator />}
          {authors.map((a) => (
            <DropItem key={a} active={author === a} onSelect={() => askLog(id, 0, { author: a })}>
              {a}
            </DropItem>
          ))}
        </Dropdown>
        <Dropdown label="Date" value={dateValue} active={since !== null || until !== null} title="a commit-date range">
          {DATE_PRESETS.map((d) => (
            <DropItem key={d.value} active={preset === d.value} onSelect={() => askLog(id, 0, presetRange(d.value))}>
              {d.label}
            </DropItem>
          ))}
        </Dropdown>
        <PopoverPrimitive.Root modal={false}>
          <PopoverPrimitive.Trigger asChild>
            <button
              type="button"
              title="one path — a file's history follows renames, a directory's does not"
              className={cn(
                "inline-flex h-5 shrink-0 items-center gap-1 rounded-sm border border-transparent px-1.5 text-2xs whitespace-nowrap",
                interactive,
                "text-[var(--dim)] hover:border-[var(--border)] data-[state=open]:bg-[var(--state-focus)]",
              )}
            >
              <span>Paths:</span>
              <span className={cn("max-w-40 truncate font-mono", path ? "text-[var(--amber)]" : "text-[var(--text)]")}>{path || "any"}</span>
            </button>
          </PopoverPrimitive.Trigger>
          <PopoverPrimitive.Portal>
            <PopoverPrimitive.Content align="start" sideOffset={4} collisionPadding={8} className={cn(surface.overlay, "z-50 w-80 p-2")}>
              <Input
                value={pathText}
                onChange={setPathText}
                mono
                autoFocus
                placeholder="path — Enter"
                ariaLabel="path"
                onKeyDown={(e) => {
                  if (e.key === "Enter") askLog(id, 0, { path: pathText.trim() });
                }}
              />
              <Dim className="mt-1 block text-2xs">
                Relative to the repository. One path, because <code>--follow</code> is defined for one; a directory works, without rename following.
              </Dim>
              {path && (
                <button
                  type="button"
                  onClick={() => {
                    setPathText("");
                    askLog(id, 0, { path: "" });
                  }}
                  className={cn(interactive, "mt-1 rounded-sm text-2xs text-[var(--dim)] underline")}
                >
                  any path
                </button>
              )}
            </PopoverPrimitive.Content>
          </PopoverPrimitive.Portal>
        </PopoverPrimitive.Root>
        <span className="mx-0.5 h-3 w-px bg-[var(--border)]" />
        <button
          type="button"
          aria-pressed={only.session}
          title="only commits that land in this session's lifetime and touch files it edited — a hint, not an author column (R-D13)"
          onClick={() => setOnly({ ...only, session: !only.session })}
          className={cn(
            "inline-flex h-5 items-center gap-1 rounded-sm px-1.5 text-2xs",
            interactive,
            only.session ? "bg-[var(--state-focus)] text-[var(--text-strong)]" : "text-[var(--dim)] hover:text-[var(--text)]",
          )}
        >
          <span className="inline-block h-1.5 w-1.5 rounded-full bg-[var(--blue)]" /> session
        </button>
        <button
          type="button"
          aria-pressed={only.read}
          title="only commits every hunk of which a human has read — known once a commit's diff has been fetched (R-D17)"
          onClick={() => setOnly({ ...only, read: !only.read })}
          className={cn(
            "inline-flex h-5 items-center rounded-sm px-1.5 text-2xs",
            interactive,
            only.read ? "bg-[var(--state-focus)] text-[var(--text-strong)]" : "text-[var(--dim)] hover:text-[var(--text)]",
          )}
        >
          read
        </button>
      </div>
      {unrun ? (
        <Dim className="block px-2 pb-1 text-2xs">press Enter to run this filter{looksLikeSha(text) ? " — a hash selects that commit" : ""}</Dim>
      ) : force.length > 0 ? (
        <div className="flex flex-wrap items-center gap-1 px-2 pb-1">
          {force.map((f) => (
            <Chip key={f} color="var(--amber)">
              {f}
            </Chip>
          ))}
          <button
            type="button"
            onClick={() => {
              setText("");
              setPathText("");
              askLog(id, 0, emptyQuery);
            }}
            className={cn(interactive, "rounded-sm text-2xs text-[var(--dim)] hover:text-[var(--text)]")}
          >
            clear
          </button>
        </div>
      ) : null}
    </div>
  );
}

/** The remotes a list of `origin/x` names belongs to, in order of appearance. */
function remoteNames(names: string[]): string[] {
  const out: string[] = [];
  for (const n of names) {
    const r = n.split("/")[0];
    if (!out.includes(r)) out.push(r);
  }
  return out;
}
