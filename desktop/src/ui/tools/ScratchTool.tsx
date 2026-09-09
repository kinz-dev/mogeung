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
 * **Two operations are inline rather than in a dialog.** Renaming edits the row
 * you right-clicked, and deleting turns that row into its own confirmation.
 * Both keep the answer where the question was asked, which a modal cannot; and
 * the delete asks at all because it is the only thing in this panel that
 * cannot be undone.
 */

import { useEffect, useRef, useState } from "react";
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

  useEffect(() => {
    send({ cmd: "scratch_list" });
  }, [send]);

  // A row that goes away under an open rename box — another window deleted it,
  // or `rm` did — leaves an editor for a file that is not there.
  useEffect(() => {
    if (busy && !names.includes(busy.name)) setBusy(null);
  }, [names, busy]);

  const startRename = (name: string) => {
    setDraft(name);
    setBusy({ name, kind: "rename" });
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
    <div className="flex min-h-0 flex-1 flex-col">
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
                  // source of truth about which one you are looking at.
                  selected={activePane === scratchPaneId(name)}
                  onClick={() => openScratch(name)}
                  className="flex items-center gap-2 px-2 py-1"
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
