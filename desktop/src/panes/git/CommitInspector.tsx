/**
 * The right pane: what the selected thing changed, as a tree, and what it
 * is. `R-D26`, rebuilding `R-D18`'s shape.
 *
 * With nothing focused the details sit beneath the tree — subject, body
 * folded behind *more*, hash, author with email, date, committer when
 * different, the signature state, and the branches that contain it.
 * **Selecting a file replaces the details with that file's diff**: the same
 * `DiffList` the Changes pane uses, read marks and all, filtered to that one
 * path before it is handed over — which is how the hunk keys, the read count
 * and the patch text follow for free. The root row is every file.
 *
 * A range or a stash arrives here too, with a label and no details.
 */

import { useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight, ExternalLink, Folder } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Empty, IconButton, Mono } from "@/ui/primitives";
import { showDiffPane } from "@/lib/panes";
import { DiffList } from "@/ui/DiffView";
import { FileIcon } from "@/ui/FileIcon";
import { fileTree, visible, type FileNode } from "@/lib/gitTree";
import { signatureWord } from "@/lib/gitFilter";
import { selectCommit, selectFile } from "@/lib/gitActions";
import { openFile } from "@/lib/explorer";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import { interactive, row as rowCls, rowSelected } from "@/ui/styles";

/** The whole diff, as the root row's value. `null` is the details view. */
export const ALL_FILES = "*";
const FOLD_LINES = 6;

export function CommitInspector({ id, onBack }: { id: string; onBack: () => void }) {
  const selected = useStore((s) => s.git[id]?.selected ?? null);
  const selectedPath = useStore((s) => s.git[id]?.selectedPath ?? null);
  const diff = useStore((s) => s.git[id]?.diff ?? null);
  const detail = useStore((s) => s.git[id]?.detail ?? null);
  const focus = useStore((s) => s.git[id]?.selectedFile ?? null);
  const label = useStore((s) => s.git[id]?.diffLabel ?? null);
  const summary = useStore((s) => s.git[id]?.commits.find((c) => c.sha === selected)?.summary);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const [cursor, setCursor] = useState(0);
  const [more, setMore] = useState(false);
  const [allBranches, setAllBranches] = useState(false);
  const diffRef = useRef<HTMLDivElement>(null);
  const treeRef = useRef<HTMLDivElement>(null);

  const tree = useMemo(() => (diff ? visible(fileTree(diff), collapsed, (r) => r.path) : []), [diff, collapsed]);
  const files = diff ?? [];
  const shown = useMemo(() => {
    if (!focus || focus === ALL_FILES) return files;
    return files.filter((f) => f.path === focus);
  }, [files, focus]);
  const hunks = files.reduce((n, f) => n + f.hunks.length, 0);
  const read = files.reduce((n, f) => n + f.hunks.filter((h) => h.reviewed).length, 0);
  const ins = files.reduce((n, f) => n + f.insertions, 0);
  const del = files.reduce((n, f) => n + f.deletions, 0);
  // A range or a stash has no details to show, so its default is every file.
  const showDiff = focus !== null || (!detail && !!label);
  const fileRows = tree.filter((r) => r.kind === "file");
  // What a pane in the centre could be opened on: a commit's sha, or a
  // range's `from..to`. A stash has no revision a pane could ask for again.
  const rev = selected ?? (label && /^[0-9a-f]+\.\.[0-9a-f]+$/i.test(label) ? label : null);
  const openInCentre = (path?: string) => {
    if (!rev) return;
    const target = path ?? (focus && focus !== ALL_FILES ? focus : "*");
    showDiffPane(id, rev, target);
  };
  /**
   * The file itself, as it stood at this commit, in the Code pane — the
   * double-click gesture asked for on the first day of use (2026-09-11),
   * beside `Ctrl+D` for the diff. A deleted file is read at the parent,
   * where it still exists; a range opens at its `to` end.
   */
  const openFileAt = (r: FileNode) => {
    if (!rev || !r.file) return;
    const at = rev.includes("..") ? rev.split("..")[1] : rev;
    if (r.file.status === "deleted") openFile(id, r.file.old_path ?? r.path, { rev: `${at}^` });
    else openFile(id, r.path, { rev: at });
  };

  const toggleDir = (path: string) =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(path)) n.delete(path);
      else n.add(path);
      return n;
    });

  /** `n`/`p`: the next hunk on screen, and past the last one the next file. */
  const stepHunk = (dir: 1 | -1) => {
    const scroller = diffRef.current;
    if (!scroller) return;
    const els = [...scroller.querySelectorAll<HTMLElement>("[data-hunk]")];
    const top = scroller.scrollTop;
    const next =
      dir === 1
        ? els.find((h) => h.offsetTop > top + 2)
        : [...els].reverse().find((h) => h.offsetTop < top - 2);
    if (next) {
      scroller.scrollTo({ top: next.offsetTop });
      return;
    }
    // Past the end of this file: step into the next one, IntelliJ-style.
    if (focus && focus !== ALL_FILES) {
      const i = fileRows.findIndex((r) => r.path === focus);
      const target = fileRows[i + dir];
      if (target) {
        selectFile(id, target.path);
        setCursor(tree.indexOf(target));
      }
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if ((e.target as HTMLElement).tagName === "BUTTON" && e.key === "Enter") return;
    // Ctrl+D: this file's diff — or every file's — as a pane in the centre.
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "d") {
      e.preventDefault();
      const r = cursor > 0 ? tree[cursor - 1] : null;
      openInCentre(r && r.kind === "file" ? r.path : undefined);
      return;
    }
    if (e.key === "n" || e.key === "p") {
      e.preventDefault();
      stepHunk(e.key === "n" ? 1 : -1);
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      if (focus !== null) selectFile(id, null);
      else onBack();
      return;
    }
    if (tree.length === 0) return;
    // Row 0 of the keyboard's list is the root; the tree's rows follow.
    const count = tree.length + 1;
    if (e.key === "ArrowDown" || e.key === "j") setCursor(Math.min(count - 1, cursor + 1));
    else if (e.key === "ArrowUp" || e.key === "k") setCursor(Math.max(0, cursor - 1));
    else if (e.key === "Enter") {
      // Enter selects; Enter on what is already selected opens it in the
      // centre — the second press is the `R-D30` gesture.
      if (cursor === 0) {
        if (focus === ALL_FILES) openInCentre();
        else selectFile(id, ALL_FILES);
      } else {
        const r = tree[cursor - 1];
        if (r.kind === "dir") toggleDir(r.path);
        else if (focus === r.path) openInCentre();
        else selectFile(id, r.path);
      }
    } else return;
    e.preventDefault();
    (treeRef.current?.children[e.key === "ArrowDown" || e.key === "j" ? Math.min(count - 1, cursor + 1) : Math.max(0, cursor - 1)] as HTMLElement | undefined)?.scrollIntoView?.({
      block: "nearest",
    });
  };

  if (!selected && !selectedPath && !label) {
    return (
      <Empty hint="its files land here as a tree, its message and branches beneath them">
        pick a commit
      </Empty>
    );
  }
  if (!diff) {
    return (
      <Empty hint="the tree and the details land in one answer">
        reading {selected ? selected.slice(0, 10) : label ?? selectedPath}…
      </Empty>
    );
  }

  const body = detail?.message ?? "";
  const bodyLines = body.split("\n");
  const subject = summary ?? bodyLines[0] ?? "";
  // The body is what follows the subject; a message that is only a subject
  // has none, and says nothing rather than repeating itself.
  const rest = bodyLines.slice(1).join("\n").replace(/^\n+/, "");
  const long = rest.split("\n").length > FOLD_LINES;
  const sig = signatureWord(detail?.signature);
  const committerDiffers =
    !!detail && (detail.committer !== detail.author || (detail.committer_email ?? "") !== (detail.author_email ?? ""));

  return (
    <div
      className="flex h-full min-h-0 flex-col outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
      tabIndex={0}
      onKeyDown={onKeyDown}
      aria-label="the selected commit"
    >
      <div className="flex h-7 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
        {label && !detail ? (
          <Dim className="truncate text-2xs" title={label}>
            {label.startsWith("stash") ? label : `comparing ${label}`}
          </Dim>
        ) : null}
        <span className="text-xs text-[var(--text)]">
          {files.length} file{files.length === 1 ? "" : "s"}
        </span>
        <span className="text-2xs text-[var(--add-fg)]">+{ins}</span>
        <span className="text-2xs text-[var(--del-fg)]">−{del}</span>
        <Dim className="ml-auto text-2xs" title="hunks a human has read, across every view that shows them (R-D17)">
          {hunks === 0 ? "no hunks" : `${read}/${hunks} read`}
        </Dim>
      </div>

      {files.length > 0 && (
        <div ref={treeRef} role="tree" aria-label="changed files" className={cn("shrink-0 overflow-y-auto border-b border-[var(--border)] py-0.5", showDiff ? "max-h-[38%]" : "max-h-[46%]")}>
          <TreeRow
            depth={0}
            selected={focus === ALL_FILES}
            cursor={cursor === 0}
            onClick={() => {
              setCursor(0);
              selectFile(id, focus === ALL_FILES ? null : ALL_FILES);
            }}
            title="every file of the diff in one scroll — click again for the details"
          >
            <ChevronDown size={11} className="shrink-0 text-[var(--dim)]" />
            <Folder size={11} className="shrink-0 text-[var(--dim)]" />
            <span className="font-semibold text-[var(--text-strong)]">all files</span>
            <Dim className="ml-auto shrink-0 text-2xs tabular-nums">{files.length}</Dim>
          </TreeRow>
          {tree.map((r, i) => (
            <TreeRow
              key={r.path}
              depth={r.depth + 1}
              selected={r.kind === "file" && focus === r.path}
              cursor={cursor === i + 1}
              title={r.kind === "file" ? `${r.path} — double-click opens the file at this commit; Ctrl+D its diff` : r.path}
              onClick={() => {
                setCursor(i + 1);
                if (r.kind === "dir") toggleDir(r.path);
                else selectFile(id, focus === r.path ? null : r.path);
              }}
              onDoubleClick={() => r.kind === "file" && openFileAt(r)}
            >
              {r.kind === "dir" ? (
                <>
                  {collapsed.has(r.path) ? (
                    <ChevronRight size={11} className="shrink-0 text-[var(--dim)]" />
                  ) : (
                    <ChevronDown size={11} className="shrink-0 text-[var(--dim)]" />
                  )}
                  <Folder size={11} className="shrink-0 text-[var(--dim)]" />
                  <Mono className="truncate text-xs text-[var(--dim)]">{r.label}</Mono>
                  <Dim className="ml-auto shrink-0 text-2xs tabular-nums">{r.count}</Dim>
                </>
              ) : (
                <>
                  <span className="w-[11px] shrink-0" />
                  <FileIcon name={r.label} size={11} className="shrink-0" />
                  <Mono className={cn("truncate text-xs", statusColour(r))}>{r.label}</Mono>
                  <ReadMark file={r} />
                  <span className="ml-auto flex shrink-0 items-center gap-1 text-2xs tabular-nums">
                    <span className="text-[var(--add-fg)]">+{r.insertions}</span>
                    <span className="text-[var(--del-fg)]">−{r.deletions}</span>
                  </span>
                </>
              )}
            </TreeRow>
          ))}
        </div>
      )}

      {showDiff ? (
        <>
          <div className="flex h-6 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2 text-2xs">
            <Mono className="truncate text-[var(--text)]" title={focus && focus !== ALL_FILES ? focus : undefined}>
              {focus && focus !== ALL_FILES ? focus : "all files"}
            </Mono>
            {detail && (
              <Dim className="shrink-0">
                {selected?.slice(0, 8)} · {detail.author} · {stamp(detail.epoch)}
              </Dim>
            )}
            <Dim className="ml-auto shrink-0" title="n and p step through the hunks, and across files">
              n / p
            </Dim>
            {rev && (
              <IconButton
                title="open this diff as a pane in the centre (Ctrl+D) — a long read leaves the dock's height behind, and a pane can pop out (R-D30)"
                onClick={() => openInCentre()}
                className="h-5 w-5"
              >
                <ExternalLink size={11} />
              </IconButton>
            )}
            {detail && (
              <button
                type="button"
                onClick={() => selectFile(id, null)}
                className={cn(interactive, "shrink-0 rounded-sm text-2xs text-[var(--dim)] underline hover:text-[var(--text)]")}
              >
                details
              </button>
            )}
          </div>
          <div ref={diffRef} className="relative min-h-0 flex-1 overflow-y-auto">
            {shown.length === 0 ? (
              <Empty hint="a binary file, or a change git shows no text for">nothing to show</Empty>
            ) : (
              <DiffList files={shown} sessionId={id} />
            )}
          </div>
        </>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-2.5 py-2">
          <div className="text-xs font-semibold text-[var(--text-strong)]" style={{ textWrap: "pretty" } as React.CSSProperties}>
            {subject}
          </div>
          {rest && (
            <div className="relative">
              <pre
                className={cn("m-0 font-sans text-xs leading-4 whitespace-pre-wrap text-[var(--text)]", !more && long && "max-h-24 overflow-hidden")}
                style={{ overflowWrap: "anywhere" }}
              >
                {rest}
              </pre>
              {long && (
                <button
                  type="button"
                  onClick={() => setMore(!more)}
                  className={cn(interactive, "mt-1 rounded-sm text-2xs text-[var(--dim)] underline hover:text-[var(--text)]")}
                >
                  {more ? "less" : "more"}
                </button>
              )}
            </div>
          )}
          {detail ? (
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-0.5 text-2xs">
              <dt className="pt-px font-semibold tracking-wider text-[var(--dim)] uppercase">commit</dt>
              <dd className="m-0 flex min-w-0 items-center gap-2">
                <Mono className="truncate text-[var(--text)]" title={selected ?? undefined}>
                  {selected?.slice(0, 12)}
                </Mono>
                {sig && <span className={cn("shrink-0", sig.bad ? "text-[var(--red)]" : "text-[var(--dim)]")}>· {sig.word}</span>}
              </dd>
              <dt className="pt-px font-semibold tracking-wider text-[var(--dim)] uppercase">author</dt>
              <dd className="m-0 truncate text-[var(--text)]">
                {detail.author}
                {detail.author_email && <Dim> &lt;{detail.author_email}&gt;</Dim>}
              </dd>
              <dt className="pt-px font-semibold tracking-wider text-[var(--dim)] uppercase">date</dt>
              <dd className="m-0 truncate text-[var(--text)]" title={new Date(detail.epoch * 1000).toLocaleString()}>
                {stamp(detail.epoch)}
                {detail.commit_epoch !== detail.epoch && <Dim> · committed {stamp(detail.commit_epoch)}</Dim>}
              </dd>
              {committerDiffers && (
                <>
                  <dt className="pt-px font-semibold tracking-wider text-[var(--dim)] uppercase">committer</dt>
                  <dd className="m-0 truncate text-[var(--text)]">
                    {detail.committer}
                    {detail.committer_email && <Dim> &lt;{detail.committer_email}&gt;</Dim>}
                  </dd>
                </>
              )}
              {detail.parents.length > 0 && (
                <>
                  <dt className="pt-px font-semibold tracking-wider text-[var(--dim)] uppercase">parents</dt>
                  <dd className="m-0 flex flex-wrap gap-1">
                    {detail.parents.map((p) => (
                      <button
                        key={p}
                        type="button"
                        onClick={() => selectCommit(id, p)}
                        className={cn(interactive, "rounded-sm font-mono text-[var(--blue)] hover:underline")}
                      >
                        {p}
                      </button>
                    ))}
                  </dd>
                </>
              )}
              {detail.refs.length > 0 && (
                <>
                  <dt className="pt-px font-semibold tracking-wider text-[var(--dim)] uppercase">refs</dt>
                  <dd className="m-0 truncate text-[var(--text)]">{detail.refs.join(", ")}</dd>
                </>
              )}
              <dt className="pt-px font-semibold tracking-wider text-[var(--dim)] uppercase">in</dt>
              <dd className="m-0 text-[var(--text)]">
                {detail.branches.length === 0 ? (
                  <Dim>no branch — reachable from a tag or the reflog only</Dim>
                ) : (
                  <>
                    {detail.branches.length} branch{detail.branches.length === 1 ? "" : "es"}:{" "}
                    <span className="break-all">{(allBranches ? detail.branches : detail.branches.slice(0, 3)).join(", ")}</span>
                    {detail.branches.length > 3 && (
                      <button
                        type="button"
                        onClick={() => setAllBranches(!allBranches)}
                        className={cn(interactive, "ml-1 rounded-sm text-[var(--dim)] underline hover:text-[var(--text)]")}
                      >
                        {allBranches ? "fewer" : `show all ${detail.branches.length}`}
                      </button>
                    )}
                  </>
                )}
              </dd>
            </dl>
          ) : (
            <Dim className="text-2xs">the header did not arrive — the diff above is whole</Dim>
          )}
        </div>
      )}
    </div>
  );
}

function TreeRow({
  depth,
  selected,
  cursor,
  onClick,
  onDoubleClick,
  title,
  children,
}: {
  depth: number;
  selected: boolean;
  cursor: boolean;
  onClick: () => void;
  onDoubleClick?: () => void;
  title?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      role="treeitem"
      aria-selected={selected}
      data-cursor={cursor || undefined}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      title={title}
      style={{ paddingLeft: 4 + depth * 12 }}
      className={cn(
        rowCls,
        "flex h-5 items-center gap-1 pr-1.5 text-xs whitespace-nowrap",
        selected && rowSelected,
        cursor && "outline-1 -outline-offset-1 outline-[var(--border-hover)]",
      )}
    >
      {children}
    </div>
  );
}

function statusColour(r: FileNode): string {
  switch (r.file?.status) {
    case "added":
      return "text-[var(--add-fg)]";
    case "deleted":
      return "text-[var(--del-fg)]";
    case "renamed":
      return "text-[var(--blue)]";
    default:
      return "text-[var(--text)]";
  }
}

function ReadMark({ file }: { file: FileNode }) {
  const f = file.file;
  if (!f || f.hunks.length === 0) return null;
  const read = f.hunks.filter((h) => h.reviewed).length;
  if (read === 0) return null;
  return (
    <Dim className="shrink-0 rounded-sm border border-[var(--border)] px-1 text-2xs leading-3" title="hunks read">
      {read}/{f.hunks.length}
    </Dim>
  );
}
