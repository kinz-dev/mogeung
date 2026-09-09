/**
 * Every scratch file, in one list, with the operations a file has. `R-L6`,
 * `R-L7`.
 *
 * Asked 2026-09-08: *"I think we need to panel to show the scratch files. The
 * current files panel didn't show it."* The Files tool is right not to show
 * them — it browses the **session's worktree**, and a scratch file is in
 * nobody's worktree; it is in `~/.mogeung/scratch` on the daemon's machine and
 * belongs to no session at all. So the answer is a list of its own rather than
 * a branch in that tree.
 *
 * Asked again 2026-09-09: *"enhance the scratch path panel with right-click
 * menu to support all file related operations."* That reverses the line
 * [feature 0039](../../../../docs/features/0039-scratch-files.md) drew and
 * [ADR-0035](../../../../docs/decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)'s
 * *Revisit if* predicted — *"scratch files turn out to want a list, a search or
 * a delete in the window"* — so the ADR carries a dated amendment saying what
 * changed and what did not. What did not: the daemon still checks every name
 * and still owns every write, and rename is the **one** verb where the window
 * proposes a name.
 *
 * **The keyboard reaches all of it** (`R-L8`, asked 2026-09-09): a row can be
 * selected, `F2` renames it and `Delete` asks before removing it. Arrow keys
 * move the selection, because a list whose keys only work once you have
 * clicked something is a list you still have to reach for the mouse to use.
 *
 * `F2` needed the keymap's permission and did not merely take it. Window-wide
 * `F2` is *Label the selected session*, and `focusOwns` hands bare keys to
 * whatever has focus — so in a list of files it would have opened the session
 * label dialog, which is the `j` collision one key over. The panel says
 * `data-owns-keys="F2 Delete"` and the keymap honours that.
 *
 * **Two operations are inline rather than in a dialog.** Renaming edits the row
 * you right-clicked, and deleting turns that row into its own confirmation.
 * Both keep the answer where the question was asked, which a modal cannot; and
 * the delete asks at all because it is the only thing in this panel that
 * cannot be undone.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, FilePlus2, FolderPlus } from "lucide-react";
import { useStore } from "@/store";
import { Button, Dim, Empty, Input, Mono, Row } from "@/ui/primitives";
import { ContextMenu, MenuItem, MenuLabel, MenuSeparator } from "@/ui/Menu";
import { movedInto, moveTargets, treeRows } from "@/lib/scratchTree";
import { copyPath } from "@/lib/clipboard";
import { openScratch, scratchPaneId, scratchPath } from "@/lib/scratch";
// `languageOf` is the explorer's — one answer to "what language is this file"
// for every pane that asks, rather than a scratch-shaped copy of it.
import { languageOf } from "@/lib/explorer";

/**
 * Which row, if any, is mid-operation.
 *
 * `newFolder` names the **parent** the folder will be made in rather than a
 * row that exists, which is why it is here rather than in a separate piece of
 * state: only one of these can be true at a time, and three booleans that must
 * not both be set is a bug waiting to be written.
 */
type Busy =
  | { name: string; kind: "rename" | "delete" | "rmdir" }
  | { name: string; kind: "newFolder" }
  | null;

export function ScratchTool() {
  const names = useStore((s) => s.scratch.names);
  const folders = useStore((s) => s.scratch.folders);
  const send = useStore((s) => s.send);
  const activePane = useStore((s) => s.activePane);
  const [busy, setBusy] = useState<Busy>(null);
  const [draft, setDraft] = useState("");
  /**
   * The row the keyboard is pointed at.
   *
   * Deliberately **not** the same thing as "open in the centre", which is what
   * the row's `selected` styling has always meant. You can be about to rename
   * a file you have not opened, and opening every file you arrow past would
   * fill the dock.
   */
  const [cursor, setCursor] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(() => new Set());
  const rowRefs = useRef(new Map<string, HTMLElement>());

  // The rows actually on screen, which is also the order the arrows walk.
  const rows = treeRows(names, folders, collapsed);
  const paths = rows.map((r) => r.path);

  useEffect(() => {
    send({ cmd: "scratch_list" });
  }, [send]);

  // A cursor on a file that has gone — renamed here, deleted in another window,
  // `rm`'d — has to land somewhere real, or `F2` renames nothing and says
  // nothing about why.
  useEffect(() => {
    if (cursor && !paths.includes(cursor)) setCursor(paths[0] ?? null);
  }, [paths, cursor]);

  // A row that goes away under an open rename box — another window deleted it,
  // or `rm` did — leaves an editor for a file that is not there.
  useEffect(() => {
    // `newFolder` names a parent that may be the root (`""`), and a `rmdir`
    // names a folder rather than a file — so this asks about the rows, and
    // lets the root through.
    if (!busy) return;
    if (busy.kind === "newFolder") {
      if (busy.name !== "" && !folders.includes(busy.name)) setBusy(null);
      return;
    }
    if (!paths.includes(busy.name)) setBusy(null);
  }, [paths, folders, busy]);

  const startRename = useCallback((name: string) => {
    setDraft(name);
    setBusy({ name, kind: "rename" });
  }, []);

  /** Move the cursor, and take the DOM focus with it so the keys keep arriving. */
  const move = useCallback(
    (from: string | null, step: number) => {
      if (paths.length === 0) return;
      const at = from ? paths.indexOf(from) : -1;
      const next = paths[Math.min(Math.max(at + step, 0), paths.length - 1)] ?? paths[0];
      setCursor(next);
      rowRefs.current.get(next)?.focus();
    },
    [paths],
  );

  /**
   * The panel's own keys. `R-L8`.
   *
   * Guarded on the target first, and that guard is the whole lesson of the `j`
   * report: a `Delete` typed into the rename box is **text**, and a handler on
   * the container sees it because the box is inside the container.
   */
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.target instanceof HTMLInputElement) return;
    // While a row is asking whether to delete it, the keys belong to that
    // question — starting a rename underneath it would leave two answers open.
    if (busy?.kind === "delete") return;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(cursor, 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(cursor, -1);
    } else if (e.key === "Enter" && cursor) {
      e.preventDefault();
      if (folders.includes(cursor)) toggle(cursor);
      else openScratch(cursor);
    } else if (e.key === "F2" && cursor) {
      e.preventDefault();
      // Only files are renamed here: a folder rename is a move of everything
      // under it, which the daemon has no single verb for, so the panel does
      // not offer a gesture it cannot honour.
      if (!folders.includes(cursor)) startRename(cursor);
    } else if (e.key === "Delete" && cursor) {
      e.preventDefault();
      setBusy({ name: cursor, kind: folders.includes(cursor) ? "rmdir" : "delete" });
    }
  };

  const toggle = (folder: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (!next.delete(folder)) next.add(folder);
      return next;
    });

  const commitRename = () => {
    if (!busy) return;
    const to = draft.trim();
    setBusy(null);
    // Unchanged, or emptied and abandoned: not a rename, and asking the daemon
    // to do nothing would still cost a broadcast and a redraw.
    if (!to || to === busy.name) return;
    send({ cmd: "scratch_rename", name: busy.name, to });
  };

  const startNewFolder = (parent: string) => {
    setDraft("");
    setBusy({ name: parent, kind: "newFolder" });
  };

  const commitNewFolder = () => {
    if (busy?.kind !== "newFolder") return;
    const leaf = draft.trim();
    const parent = busy.name;
    setBusy(null);
    if (!leaf) return;
    send({ cmd: "scratch_mkdir", path: parent === "" ? leaf : `${parent}/${leaf}` });
  };

  const newFile = (
    <div className="flex items-center gap-1 border-b border-[var(--border)] px-2 py-1">
      <Button
        variant="outline"
        onClick={() => useStore.setState({ paletteOpen: true, paletteMode: "scratch" })}
      >
        <FilePlus2 size={11} /> new scratch file
      </Button>
      <Button variant="outline" onClick={() => startNewFolder("")}>
        <FolderPlus size={11} /> new folder
      </Button>
    </div>
  );

  if (names.length === 0) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        {newFile}
        <Empty hint="Ctrl+Alt+Shift+Insert makes one — pick a language and it opens here and on disk">
          no scratch files
        </Empty>
      </div>
    );
  }

  return (
    // `data-owns-keys` is a claim the keymap honours (`focusOwns`), not a hint.
    // Without it `F2` here opens *Label the selected session*, because a bare
    // key belongs to whatever has focus and a `div` is not a text box. `Delete`
    // is claimed too though nothing binds it today: a future binding would
    // otherwise take it silently, which is exactly how `j` happened.
    <div
      className="flex min-h-0 flex-1 flex-col outline-none"
      data-owns-keys="F2 Delete"
      onKeyDown={onKeyDown}
    >
      {newFile}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {/*
          A new folder at the **root** has no row to appear under — the root is
          not a row — so its editor is drawn here. Missed on the first pass and
          caught by the test: the button opened nothing at all.
        */}
        {busy?.kind === "newFolder" && busy.name === "" && (
          <RenameRow
            value={draft}
            onChange={setDraft}
            onCommit={commitNewFolder}
            onCancel={() => setBusy(null)}
            hint="Enter makes the folder, Escape leaves it."
          />
        )}
        {rows.map((row) => {
          const name = row.path;
          const indent = { paddingLeft: `${row.depth * 12 + 8}px` };

          if (busy?.name === name && busy.kind === "rename") {
            return (
              <RenameRow
                key={name}
                value={draft}
                onChange={setDraft}
                onCommit={commitRename}
                onCancel={() => setBusy(null)}
                hint="Enter renames, Escape leaves it. The daemon refuses a name that is already here."
              />
            );
          }

          // Deleting a file, and removing a folder, ask the same way and mean
          // very different things — so the folder's question names the count.
          if (busy?.name === name && (busy.kind === "delete" || busy.kind === "rmdir")) {
            const inside = names.filter((n) => n.startsWith(`${name}/`)).length;
            return (
              <div
                key={name}
                className="flex items-center gap-2 border-b border-[var(--border)] py-1 pr-2"
                style={indent}
              >
                <Mono className="min-w-0 flex-1 truncate text-xs">
                  {row.name}
                  {busy.kind === "rmdir" && (
                    <Dim className="ml-1 text-2xs">
                      {inside === 0 ? "(empty)" : `and ${inside} file${inside === 1 ? "" : "s"}`}
                    </Dim>
                  )}
                </Mono>
                <Button
                  variant="outline"
                  onClick={() => {
                    send(
                      busy.kind === "rmdir"
                        ? { cmd: "scratch_rmdir", path: name }
                        : { cmd: "scratch_delete", name },
                    );
                    setBusy(null);
                  }}
                >
                  delete
                </Button>
                <Button variant="outline" onClick={() => setBusy(null)}>
                  keep
                </Button>
              </div>
            );
          }

          const children =
            busy?.kind === "newFolder" && busy.name === name ? (
              <RenameRow
                key={`${name}/+`}
                value={draft}
                onChange={setDraft}
                onCommit={commitNewFolder}
                onCancel={() => setBusy(null)}
                hint="Enter makes the folder, Escape leaves it."
                indent={row.depth + 1}
              />
            ) : null;

          if (row.kind === "folder") {
            const shut = collapsed.has(name);
            return (
              <div key={name}>
                <ContextMenu
                  trigger={
                    <Row
                      aria-label={`folder ${name}`}
                      onClick={() => {
                        setCursor(name);
                        toggle(name);
                      }}
                      className={`flex items-center gap-1 py-1 pr-2 outline-none${
                        cursor === name ? " ring-1 ring-inset ring-[var(--ring)]" : ""
                      }`}
                      style={indent}
                      tabIndex={cursor === name || (cursor === null && paths[0] === name) ? 0 : -1}
                      onFocus={() => setCursor(name)}
                      ref={(el: HTMLElement | null) => {
                        if (el) rowRefs.current.set(name, el);
                        else rowRefs.current.delete(name);
                      }}
                    >
                      {shut ? <ChevronRight size={11} /> : <ChevronDown size={11} />}
                      <Mono className="min-w-0 flex-1 truncate text-xs">{row.name}</Mono>
                    </Row>
                  }
                >
                  <MenuLabel>{name}</MenuLabel>
                  <MenuItem onSelect={() => startNewFolder(name)}>New folder here…</MenuItem>
                  <MenuItem
                    onSelect={() =>
                      useStore.setState({
                        paletteOpen: true,
                        paletteMode: "scratch",
                        scratchFolder: name,
                      })
                    }
                  >
                    New scratch file here…
                  </MenuItem>
                  <MenuSeparator />
                  <MenuItem onSelect={() => void copyPath(scratchPath(name), "full path")}>
                    Copy full path
                  </MenuItem>
                  <MenuSeparator />
                  {/* The most destructive thing in the window, so it asks and
                      says how many files go with it. */}
                  <MenuItem danger onSelect={() => setBusy({ name, kind: "rmdir" })}>
                    Delete folder…
                  </MenuItem>
                </ContextMenu>
                {children}
              </div>
            );
          }

          const targets = moveTargets(folders, name);
          return (
            <ContextMenu
              key={name}
              trigger={
                <Row
                  // The pane id is what the centre calls this file, so a scratch
                  // file already open reads as selected here without a second
                  // source of truth about which one you are looking at. The
                  // **cursor** is a different question and shows as a ring, so a
                  // file that is open and a file you are about to rename do not
                  // look alike.
                  selected={activePane === scratchPaneId(name)}
                  onClick={() => {
                    setCursor(name);
                    openScratch(name);
                  }}
                  className={`flex items-center gap-2 py-1 pr-2 outline-none${
                    cursor === name ? " ring-1 ring-inset ring-[var(--ring)]" : ""
                  }`}
                  style={indent}
                  tabIndex={cursor === name || (cursor === null && paths[0] === name) ? 0 : -1}
                  onFocus={() => setCursor(name)}
                  ref={(el: HTMLElement | null) => {
                    if (el) rowRefs.current.set(name, el);
                    else rowRefs.current.delete(name);
                  }}
                >
                  <Mono className="min-w-0 flex-1 truncate text-xs">{row.name}</Mono>
                  <Dim className="shrink-0 text-2xs">{languageOf(name)}</Dim>
                </Row>
              }
            >
              <MenuLabel>{name}</MenuLabel>
              <MenuItem onSelect={() => openScratch(name)}>Open</MenuItem>
              <MenuItem onSelect={() => startRename(name)}>Rename…</MenuItem>
              <MenuItem onSelect={() => send({ cmd: "scratch_duplicate", name })}>
                Duplicate
              </MenuItem>
              {targets.length > 0 && <MenuSeparator />}
              {/*
                **A move is a rename**, which is why there is no move verb: the
                daemon's rename takes a path, so putting a file in another folder
                is renaming it to a path in that folder. Listed rather than
                dragged because a drop target inside a rail panel is a much
                bigger thing to get right, and this works from the keyboard.
              */}
              {targets.map((folder) => {
                const to = movedInto(name, folder);
                if (!to) return null;
                return (
                  <MenuItem
                    key={folder || "/"}
                    onSelect={() => send({ cmd: "scratch_rename", name, to })}
                  >
                    Move to {folder === "" ? "the top level" : folder}
                  </MenuItem>
                );
              })}
              <MenuSeparator />
              <MenuItem onSelect={() => void copyPath(scratchPath(name), "full path")}>
                Copy full path
              </MenuItem>
              <MenuItem onSelect={() => void copyPath(name, "file name")}>Copy file name</MenuItem>
              <MenuSeparator />
              {/*
                The only thing here that cannot be undone. It asks in the row
                rather than doing it: `rm` was the delete before this row, and
                `rm` at least makes you type the name.
              */}
              <MenuItem danger onSelect={() => setBusy({ name, kind: "delete" })}>
                Delete…
              </MenuItem>
            </ContextMenu>
          );
        })}
      </div>
    </div>
  );
}

/**
 * The rename editor, which is the row it replaces.
 *
 * Its own component for the focus: an input that appears where a row was has
 * to take the caret, or the first thing you type goes to whatever had focus
 * before — which here is the list, and `j` used to be a shortcut.
 */
function RenameRow({
  value,
  onChange,
  onCommit,
  onCancel,
  hint,
  indent = 0,
}: {
  value: string;
  onChange: (v: string) => void;
  onCommit: () => void;
  onCancel: () => void;
  /** What Enter and Escape will do — different for a rename and a new folder. */
  hint: string;
  /** Depth, so a new folder's box appears under the folder it will go in. */
  indent?: number;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  return (
    <div
      className="border-b border-[var(--border)] py-1 pr-2"
      style={{ paddingLeft: `${indent * 12 + 8}px` }}
    >
      <Input
        inputRef={ref}
        value={value}
        mono
        ariaLabel="new name"
        onChange={onChange}
        onKeyDown={(e) => {
          if (e.key === "Enter") onCommit();
          // Stopped here rather than left to bubble: Escape closes the rail's
          // panels, so an abandoned rename would take the whole tool with it.
          if (e.key === "Escape") {
            e.stopPropagation();
            onCancel();
          }
        }}
      />
      <Dim className="mt-0.5 block text-2xs">{hint}</Dim>
    </div>
  );
}
