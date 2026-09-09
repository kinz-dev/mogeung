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
import { FilePlus2 } from "lucide-react";
import { useStore } from "@/store";
import { Button, Dim, Empty, Input, Mono, Row } from "@/ui/primitives";
import { ContextMenu, MenuItem, MenuLabel, MenuSeparator } from "@/ui/Menu";
import { copyPath } from "@/lib/clipboard";
import { openScratch, scratchPaneId, scratchPath } from "@/lib/scratch";
// `languageOf` is the explorer's — one answer to "what language is this file"
// for every pane that asks, rather than a scratch-shaped copy of it.
import { languageOf } from "@/lib/explorer";

/** Which row, if any, is being renamed or is asking before it is deleted. */
type Busy = { name: string; kind: "rename" | "delete" } | null;

export function ScratchTool() {
  const names = useStore((s) => s.scratch.names);
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
  const rowRefs = useRef(new Map<string, HTMLElement>());

  useEffect(() => {
    send({ cmd: "scratch_list" });
  }, [send]);

  // A cursor on a file that has gone — renamed here, deleted in another window,
  // `rm`'d — has to land somewhere real, or `F2` renames nothing and says
  // nothing about why.
  useEffect(() => {
    if (cursor && !names.includes(cursor)) setCursor(names[0] ?? null);
  }, [names, cursor]);

  // A row that goes away under an open rename box — another window deleted it,
  // or `rm` did — leaves an editor for a file that is not there.
  useEffect(() => {
    if (busy && !names.includes(busy.name)) setBusy(null);
  }, [names, busy]);

  const startRename = useCallback((name: string) => {
    setDraft(name);
    setBusy({ name, kind: "rename" });
  }, []);

  /** Move the cursor, and take the DOM focus with it so the keys keep arriving. */
  const move = useCallback(
    (from: string | null, step: number) => {
      if (names.length === 0) return;
      const at = from ? names.indexOf(from) : -1;
      const next = names[Math.min(Math.max(at + step, 0), names.length - 1)] ?? names[0];
      setCursor(next);
      rowRefs.current.get(next)?.focus();
    },
    [names],
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
      openScratch(cursor);
    } else if (e.key === "F2" && cursor) {
      e.preventDefault();
      startRename(cursor);
    } else if (e.key === "Delete" && cursor) {
      e.preventDefault();
      setBusy({ name: cursor, kind: "delete" });
    }
  };

  const commitRename = () => {
    if (!busy) return;
    const to = draft.trim();
    setBusy(null);
    // Unchanged, or emptied and abandoned: not a rename, and asking the daemon
    // to do nothing would still cost a broadcast and a redraw.
    if (!to || to === busy.name) return;
    send({ cmd: "scratch_rename", name: busy.name, to });
  };

  const newFile = (
    <div className="border-b border-[var(--border)] px-2 py-1">
      <Button
        variant="outline"
        onClick={() => useStore.setState({ paletteOpen: true, paletteMode: "scratch" })}
      >
        <FilePlus2 size={11} /> new scratch file
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
        {names.map((name) => {
          if (busy?.name === name && busy.kind === "rename") {
            return (
              <RenameRow
                key={name}
                value={draft}
                onChange={setDraft}
                onCommit={commitRename}
                onCancel={() => setBusy(null)}
              />
            );
          }
          if (busy?.name === name && busy.kind === "delete") {
            return (
              <div key={name} className="flex items-center gap-2 border-b border-[var(--border)] px-2 py-1">
                <Mono className="min-w-0 flex-1 truncate text-xs">{name}</Mono>
                <Button
                  variant="outline"
                  onClick={() => {
                    send({ cmd: "scratch_delete", name });
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

          return (
            <ContextMenu
              key={name}
              trigger={
                <Row
                  // The pane id is what the centre calls this file, so a scratch
                  // file already open reads as selected here without a second
                  // source of truth about which one you are looking at. The
                  // **cursor** is a different question — see `cursor` above —
                  // and shows as a ring rather than as a fill, so a file that is
                  // open and a file you are about to rename do not look alike.
                  selected={activePane === scratchPaneId(name)}
                  onClick={() => {
                    setCursor(name);
                    openScratch(name);
                  }}
                  className={`flex items-center gap-2 px-2 py-1 outline-none${
                    cursor === name ? " ring-1 ring-inset ring-[var(--ring)]" : ""
                  }`}
                  tabIndex={cursor === name || (cursor === null && names[0] === name) ? 0 : -1}
                  onFocus={() => setCursor(name)}
                  ref={(el: HTMLElement | null) => {
                    if (el) rowRefs.current.set(name, el);
                    else rowRefs.current.delete(name);
                  }}
                >
                  <Mono className="min-w-0 flex-1 truncate text-xs">{name}</Mono>
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
}: {
  value: string;
  onChange: (v: string) => void;
  onCommit: () => void;
  onCancel: () => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);

  return (
    <div className="border-b border-[var(--border)] px-2 py-1">
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
      <Dim className="mt-0.5 block text-2xs">
        Enter renames, Escape leaves it. The daemon refuses a name that is already here.
      </Dim>
    </div>
  );
}
