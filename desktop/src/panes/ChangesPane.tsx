/**
 * What this session changed, and how much of it you have read.
 *
 * The diff base is HEAD as it stood when the session was first seen, pinned
 * once — which is also why a database carried over from an older build can make
 * a diff look wrong (`start.sh --fresh` exists for exactly that).
 */

import { useEffect } from "react";
import { CheckCheck, EyeOff, RefreshCw } from "lucide-react";
import { useStore } from "@/store";
import { Checkbox, Dim, Empty, IconButton, PaneHeader } from "@/ui/primitives";
import { DiffList } from "@/ui/DiffView";

export function ChangesPane() {
  const id = useStore((s) => s.selected);
  const change = useStore((s) => (s.selected ? s.changes[s.selected] : undefined));
  const send = useStore((s) => s.send);
  const prefs = useStore((s) => s.prefs);
  const setPrefs = useStore((s) => s.setPrefs);

  useEffect(() => {
    if (id && !change) send({ cmd: "refresh_change", session_id: id });
  }, [id, change, send]);

  if (!id) return <Empty>select a session</Empty>;
  if (!change) return <Empty>computing the diff…</Empty>;

  if (change.error) {
    return (
      <Empty hint={change.error}>the diff could not be computed</Empty>
    );
  }

  const total = change.files.reduce((n, f) => n + f.hunks.length, 0);
  const read = change.files.reduce((n, f) => n + f.hunks.filter((h) => h.reviewed).length, 0);

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <PaneHeader title="Changes" hint="read-only: the daemon never writes a worktree file">
        <Dim className="text-2xs">
          {read}/{total} hunks read
        </Dim>
        <Checkbox checked={prefs.hideReviewed} onChange={(v) => setPrefs({ hideReviewed: v })} label="hide read" />
        <Checkbox
          checked={prefs.hideNoise}
          onChange={(v) => setPrefs({ hideNoise: v })}
          label="hide noise"
          title="lockfiles, generated output — scored below zero and already read"
        />
        <IconButton title="mark every hunk read" onClick={() => send({ cmd: "review_all", session_id: id })}>
          <CheckCheck size={13} />
        </IconButton>
        <IconButton title="recompute from disk" onClick={() => send({ cmd: "refresh_change", session_id: id })}>
          <RefreshCw size={13} />
        </IconButton>
      </PaneHeader>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {change.files.length === 0 ? (
          <Empty hint="this session has not touched the worktree, or the changes are already committed">
            nothing changed
          </Empty>
        ) : (
          <DiffList files={change.files} sessionId={id} />
        )}
      </div>

      <div className="flex h-5 shrink-0 items-center gap-2 border-t border-[var(--border)] px-2">
        <span className="text-2xs text-[var(--add-fg)]">+{change.insertions}</span>
        <span className="text-2xs text-[var(--del-fg)]">−{change.deletions}</span>
        <Dim className="text-2xs">{change.files.length} file(s)</Dim>
        {prefs.hideReviewed && read > 0 && (
          <Dim className="ml-auto flex items-center gap-1 text-2xs">
            <EyeOff size={9} /> {read} read hunk(s) hidden
          </Dim>
        )}
      </div>
    </div>
  );
}
