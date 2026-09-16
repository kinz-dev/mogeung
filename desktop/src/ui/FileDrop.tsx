/**
 * Drop a file on the window and it opens in the Code pane. `R-J94`.
 *
 * Asked 2026-09-16. The window already reads files — the tree, go-to-file,
 * search, a diff row — but every one of those starts from a session's own
 * worktree, so a file you are *looking at somewhere else* had a path you had
 * to retype. A drag is the shortest way to say which file you mean, and the
 * decisions behind it live in `lib/drop.ts`; this is the listener and the one
 * piece of feedback it owes you.
 *
 * **On the window, not on a drop zone.** The centre is a dockview tree of
 * panes that move, and a target you have to aim at is one more thing to learn
 * about a gesture whose whole appeal is that it needs nothing learnt. The
 * guard is the drag's own `Files` flavour, which no page-internal drag
 * carries — see `isFileDrag`, and `R-J20` for why that distinction is worth
 * being careful about.
 */

import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { dropPaths, isFileDrag, targetForDrop } from "@/lib/drop";
import { openFile } from "@/lib/explorer";
import { base } from "@/lib/format";

/**
 * Past this, a drop is a mistake rather than an intention.
 *
 * Every file opens a pane of its own (`R-B53`), so dragging a folder's worth
 * of them would bury the arrangement you are working in. The ones beyond the
 * cap are named rather than dropped silently.
 */
const MAX_FILES = 8;

/** Open one dropped path, admitting its folder first if that is what it takes. */
export function openDropped(path: string): void {
  const { sessions, selected, explorer, send, pushNotice, pushError } = useStore.getState();
  const target = targetForDrop(path, {
    sessions,
    selected,
    extraRoots: (id) => explorer[id]?.workspace?.dirs ?? [],
  });

  if (!target) {
    // No session at all, or none selected and none holding the file. There is
    // nothing to open it *in*: a file pane belongs to a session, because the
    // daemon reads files through one.
    pushError(`nothing to open ${base(path)} in — select a session first`);
    return;
  }

  if (target.addDir) {
    send({ cmd: "add_workspace_dir", session_id: target.session, path: target.addDir });
    // Said out loud because it persists. `R-J40`'s store is keyed by
    // repository and outlives the session, so a drag has just changed what
    // this checkout can read — which is exactly the sort of thing that should
    // not happen quietly.
    pushNotice(`${target.addDir} joined this session's workspace — remove it in Files`);
  }

  openFile(target.session, target.path, { pin: true });
}

/**
 * The window's file-drop listener, and whether a file is over it right now.
 *
 * `dragover` must be prevented or no `drop` follows — that is the DOM's rule,
 * not a choice — and both are taken in the **capture** phase so a drop over
 * Monaco or xterm means the same thing as a drop over anything else. Nothing
 * else in the window wants an OS file drag, so nothing is being taken away.
 */
export function useFileDrop(): boolean {
  const [over, setOver] = useState(false);

  useEffect(() => {
    const types = (e: DragEvent) => (e.dataTransfer ? Array.from(e.dataTransfer.types) : undefined);

    const onOver = (e: DragEvent) => {
      if (!isFileDrag(types(e))) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      setOver(true);
    };

    // `relatedTarget` is null exactly when the pointer has left the window —
    // a leave between two elements inside it is the ordinary crossing that
    // would otherwise flicker the hint off under the cursor.
    const onLeave = (e: DragEvent) => {
      if (e.relatedTarget === null) setOver(false);
    };

    const onDrop = (e: DragEvent) => {
      if (!isFileDrag(types(e))) return;
      e.preventDefault();
      e.stopPropagation();
      setOver(false);
      const paths = dropPaths(e.dataTransfer);
      if (paths.length === 0) {
        // A drag that said it carried files and then handed over nothing we
        // can read — a clipping from another app, an unsupported flavour.
        useStore.getState().pushError("that drop carried no file path mogeung could read");
        return;
      }
      for (const path of paths.slice(0, MAX_FILES)) openDropped(path);
      if (paths.length > MAX_FILES) {
        useStore
          .getState()
          .pushNotice(`opened the first ${MAX_FILES} of ${paths.length} files`);
      }
    };

    // A drag abandoned with Escape raises neither a leave nor a drop.
    const onEnd = () => setOver(false);

    window.addEventListener("dragover", onOver, true);
    window.addEventListener("dragleave", onLeave, true);
    window.addEventListener("drop", onDrop, true);
    window.addEventListener("dragend", onEnd, true);
    return () => {
      window.removeEventListener("dragover", onOver, true);
      window.removeEventListener("dragleave", onLeave, true);
      window.removeEventListener("drop", onDrop, true);
      window.removeEventListener("dragend", onEnd, true);
    };
  }, []);

  return over;
}

/**
 * What a file drag looks like over the window.
 *
 * `pointer-events-none` throughout, and that is load-bearing rather than
 * tidy: an overlay that took the pointer would replace the element under the
 * cursor mid-drag, and the browser would send the `drop` somewhere else.
 */
export function FileDrop() {
  const over = useFileDrop();
  if (!over) return null;
  return (
    <div className="pointer-events-none absolute inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 border-2 border-dashed border-[var(--blue)] opacity-60" />
      <div className="rounded border border-[var(--border)] bg-[var(--bg)] px-3 py-1.5 text-xs text-[var(--text-strong)] shadow-lg">
        drop to open in the Code pane
      </div>
    </div>
  );
}
