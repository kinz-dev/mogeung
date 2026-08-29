/**
 * What this session changed, and how much of it you have read.
 *
 * The diff base is HEAD as it stood when the session was first seen, pinned
 * once — which is also why a database carried over from an older build can make
 * a diff look wrong (`start.sh --fresh` exists for exactly that).
 */

import { useEffect, useMemo, useState } from "react";
import { CheckCheck, Columns2, EyeOff, ListOrdered, Palette, RefreshCw, TextCursorInput } from "lucide-react";
import { useStore, type ReadingGuide } from "@/store";
import { summaryDisagrees } from "@/store/changes";
import { Checkbox, Dim, Empty, IconButton, PaneHeader } from "@/ui/primitives";
import { DiffList } from "@/ui/DiffView";
import { interactive } from "@/ui/styles";
import { cn } from "@/lib/cn";

/**
 * What the guide said, above the files it ordered. `R-O3`.
 *
 * The summary is the half of the row that is not an ordering: *what carries
 * the change and what is mechanical*. It sits above the list because it is
 * about the whole diff, and the per-file reasons sit on the files.
 *
 * The counts are stated rather than implied. A reader who cannot tell how much
 * of the diff the model actually looked at has to trust the order completely
 * or not at all, and neither is right.
 */
function GuideNote({
  guide,
  ranked,
  total,
  onRetry,
}: {
  guide: ReadingGuide | undefined;
  ranked: number;
  total: number;
  onRetry: () => void;
}) {
  if (!guide || guide.pending) {
    return (
      <Dim className="block border-b border-[var(--border)] px-2 py-1 text-2xs italic">
        reading the diff…
      </Dim>
    );
  }
  if (guide.error) {
    return (
      <div className="border-b border-[var(--border)] px-2 py-1 text-2xs text-[var(--red)]">
        {guide.error}{" "}
        <button
          type="button"
          // `interactive` for the focus ring — `styles.test.ts` refuses a
          // button without one, and in a keyboard-driven window it is right to.
          className={cn(interactive, "rounded-sm underline")}
          onClick={onRetry}
        >
          try again
        </button>
      </div>
    );
  }
  return (
    <div className="border-b border-[var(--border)] px-2 py-1">
      {guide.summary && <div className="text-2xs">{guide.summary}</div>}
      <Dim className="mt-0.5 block text-2xs">
        {ranked} of {total} file(s) ordered by {guide.model || "the model"}
        {guide.elapsed_ms ? ` · ${(guide.elapsed_ms / 1000).toFixed(1)}s` : ""}
        {ranked < total ? " — the rest follow in risk order, unranked" : ""}
      </Dim>
    </div>
  );
}

export function ChangesPane() {
  const id = useStore((s) => s.selected);
  const change = useStore((s) => (s.selected ? s.changes[s.selected] : undefined));
  const summary = useStore((s) => (s.selected ? s.changeSummaries[s.selected] : undefined));
  const send = useStore((s) => s.send);
  const guide = useStore((s) => (s.selected ? s.guides[s.selected] : undefined));
  const askReadingGuide = useStore((s) => s.askReadingGuide);
  // Local, not a preference: the guide is about *this* diff, so leaving it on
  // would show the next session its risk order under a heading claiming a
  // model had read it.
  const [guideOn, setGuideOn] = useState(false);
  useEffect(() => setGuideOn(false), [id]);
  // Field-by-field, never the whole prefs object: this pane sits above every
  // line of the diff, and an unrelated pref write (terminal font, pane zoom)
  // re-rendering it re-renders all of them.
  const hideReviewed = useStore((s) => s.prefs.hideReviewed);
  const hideNoise = useStore((s) => s.prefs.hideNoise);
  const sideBySide = useStore((s) => s.prefs.sideBySide);
  const wordDiff = useStore((s) => s.prefs.wordDiff);
  const syntax = useStore((s) => s.prefs.syntax);
  const setPrefs = useStore((s) => s.setPrefs);

  // This pane is the only reader of hunk bodies, so it is the one that pulls
  // them: on first sight of a session, and whenever the broadcast summary
  // says the diff moved past what it holds. Un-forced, so the daemon answers
  // from its cache — the recompute already happened on the scan path.
  useEffect(() => {
    if (!id) return;
    if (!change || (summary && summaryDisagrees(summary, change))) {
      send({ cmd: "refresh_change", session_id: id });
    }
  }, [id, change, summary, send]);


  /**
   * The reading order, when one has been asked for. `R-O3`.
   *
   * A **second ordering you switch to**, never a blend with the keyword one —
   * pillar K settled that in advance, because a weighted mix would look
   * authoritative while still being wrong. So with `guide` off, or with no
   * model, this pane is exactly what it was.
   *
   * Files the model did not name are still here, after the ones it did. That
   * is not politeness: `--bin judge` found `claude-opus-5` ranking sixteen
   * files of sixty and saying nothing about the other 44, so rendering its
   * list as the diff would hide them.
   *
   * **Above the early returns, and that is not style.** These were below them
   * when this shipped, so the pane called three hooks with no session selected
   * and five with one — React's *rendered more hooks than during the previous
   * render*, which unmounts the tree and leaves the pane blank for good.
   * Reported 2026-08-29. Every hook in this component must run on every path.
   */
  const files = change?.files;
  const ordered = useMemo(() => {
    if (!files) return [];
    if (!guideOn || !guide || guide.files.length === 0) return files;
    const byPath = new Map(files.map((f) => [f.path, f]));
    const out: typeof files = [];
    for (const g of guide.files) {
      const f = byPath.get(g.path);
      if (f) {
        out.push(f);
        byPath.delete(g.path);
      }
    }
    // Anything the guide never saw at all — noise, and files past its cap.
    // Dropping them here would hide files for a second, different reason.
    for (const f of byPath.values()) out.push(f);
    return out;
  }, [guideOn, guide, files]);

  const reasons = useMemo(() => {
    const m = new Map<string, string>();
    for (const g of guide?.files ?? []) if (g.ranked && g.reason) m.set(g.path, g.reason);
    return m;
  }, [guide]);

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
        <Checkbox checked={hideReviewed} onChange={(v) => setPrefs({ hideReviewed: v })} label="hide read" />
        <Checkbox
          checked={hideNoise}
          onChange={(v) => setPrefs({ hideNoise: v })}
          label="hide noise"
          title="lockfiles, generated output — scored below zero and already read"
        />
        {/*
          The guide is asked for, never automatic. It costs a model call of up
          to a minute, and ADR-0031 clause 6 keeps model work off anything that
          runs on its own — a pane that quietly spent a minute of somebody's
          plan every time you selected a session would be the wrong default
          however good the ordering.
        */}
        <IconButton
          title={
            guideOn
              ? "back to risk order"
              : "ask a model which file to read first  (R-O3)"
          }
          active={guideOn}
          disabled={guide?.pending}
          onClick={() => {
            if (guideOn) {
              setGuideOn(false);
              return;
            }
            setGuideOn(true);
            if (!guide || guide.error) askReadingGuide(id);
          }}
        >
          <ListOrdered size={13} />
        </IconButton>
        <IconButton
          title="side by side — the removed file left, the added right  (R-D6)"
          active={sideBySide}
          onClick={() => setPrefs({ sideBySide: !sideBySide })}
        >
          <Columns2 size={13} />
        </IconButton>
        <IconButton
          title="word diff — mark only the part of a changed line that moved  (R-D5)"
          active={wordDiff}
          onClick={() => setPrefs({ wordDiff: !wordDiff })}
        >
          <TextCursorInput size={13} />
        </IconButton>
        <IconButton
          title="syntax colour  (R-D4). A tokenizer, not a parser — it will mis-colour things"
          active={syntax}
          onClick={() => setPrefs({ syntax: !syntax })}
        >
          <Palette size={13} />
        </IconButton>
        <IconButton title="mark every hunk read" onClick={() => send({ cmd: "review_all", session_id: id })}>
          <CheckCheck size={13} />
        </IconButton>
        <IconButton
          title="recompute from disk"
          onClick={() => send({ cmd: "refresh_change", session_id: id, force: true })}
        >
          <RefreshCw size={13} />
        </IconButton>
      </PaneHeader>

      <div className="min-h-0 flex-1 overflow-y-auto">
        {change.files.length === 0 ? (
          <Empty hint="this session has not touched the worktree, or the changes are already committed">
            nothing changed
          </Empty>
        ) : (
          <>
            {guideOn && (
              <GuideNote
                guide={guide}
                ranked={reasons.size}
                total={ordered.length}
                onRetry={() => askReadingGuide(id)}
              />
            )}
            <DiffList files={ordered} sessionId={id} reasons={guideOn ? reasons : undefined} />
          </>
        )}
      </div>

      <div className="flex h-5 shrink-0 items-center gap-2 border-t border-[var(--border)] px-2">
        <span className="text-2xs text-[var(--add-fg)]">+{change.insertions}</span>
        <span className="text-2xs text-[var(--del-fg)]">−{change.deletions}</span>
        <Dim className="text-2xs">{change.files.length} file(s)</Dim>
        {hideReviewed && read > 0 && (
          <Dim className="ml-auto flex items-center gap-1 text-2xs">
            <EyeOff size={9} /> {read} read hunk(s) hidden
          </Dim>
        )}
      </div>
    </div>
  );
}
