/**
 * Every scratch file, in one list. `R-L6`.
 *
 * Asked 2026-09-08: *"I think we need to panel to show the scratch files. The
 * current files panel didn't show it."* The Files tool is right not to show
 * them — it browses the **session's worktree**, and a scratch file is not in
 * anybody's worktree; it is in `~/.mogeung/scratch` on the daemon's machine and
 * belongs to no session at all. So the answer is a list of its own rather than
 * a branch in that tree.
 *
 * **Deliberately a list and not a manager.** [Feature 0039](../../../../docs/features/0039-scratch-files.md)
 * put *"a list, search or delete in the window"* out of scope on the argument
 * that anything more is a sign these are becoming documents, and
 * [ADR-0035](../../../../docs/decisions/0035-the-editor-writes-scratch-files-and-nothing-else.md)
 * says where documents live. That argument is about **managing** them; it is
 * not an argument that a file you made yesterday should be unreachable without
 * remembering a chord. So: names, newest first, click to open. No rename, no
 * delete, no search — `rm` still deletes one, and the day this list wants a
 * filter is the day the ADR's warning is worth re-reading.
 *
 * The names arrive on the `scratches` broadcast, which the picker already asks
 * for. This asks again on mount because a file can be created — or removed with
 * `rm` — while the rail is shut, and a stale list is worse here than an empty
 * one: it offers you a file that is not there.
 */

import { useEffect } from "react";
import { FilePlus2 } from "lucide-react";
import { useStore } from "@/store";
import { Button, Dim, Empty, Mono, Row } from "@/ui/primitives";
import { openScratch, scratchPaneId } from "@/lib/scratch";
// `languageOf` is the explorer's — one answer to "what language is this file"
// for every pane that asks, rather than a scratch-shaped copy of it.
import { languageOf } from "@/lib/explorer";

export function ScratchTool() {
  const names = useStore((s) => s.scratch.names);
  const send = useStore((s) => s.send);
  const activePane = useStore((s) => s.activePane);

  useEffect(() => {
    send({ cmd: "scratch_list" });
  }, [send]);

  // The rail draws no per-tool header controls, so the one action this panel
  // has lives in the body. It opens the same picker the chord does rather than
  // creating anything itself: the daemon mints every name (ADR-0035), and a
  // second way to ask for a file would be a second place to get that wrong.
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
        {names.map((name) => (
          <Row
            key={name}
            // The pane id is what the centre calls this file, so a scratch file
            // already open reads as selected here without a second source of
            // truth about which one you are looking at.
            selected={activePane === scratchPaneId(name)}
            onClick={() => openScratch(name)}
            className="flex items-center gap-2 px-2 py-1"
          >
            <Mono className="min-w-0 flex-1 truncate text-xs">{name}</Mono>
            <Dim className="shrink-0 text-2xs">{languageOf(name)}</Dim>
          </Row>
        ))}
      </div>
    </div>
  );
}

