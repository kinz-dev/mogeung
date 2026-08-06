/**
 * What Claude Code has been *told* — the memory it saved and the skills it can
 * run. `R-F14`, `R-F15`.
 *
 * Everything else in Insight looks at what agents **did**. This is the other
 * half, and it is the half you edit: a skill is instructions the next session
 * will follow, a memory is something an agent decided to remember about you.
 * Claude Code shows you their names; what nobody has is the *content*, side by
 * side, at the moment you are deciding whether it is still true.
 *
 * **Read only.** `CLAUDE.md` says never write to `~/.claude`, and a panel that
 * could edit a skill would be a panel that changes every session on this
 * machine. The path is shown for exactly that reason: this tells you *what* to
 * change and where, and your editor does the changing.
 */

import { useEffect, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useStore } from "@/store";
import { Chip, Dim, Empty, Input, Mono, Row } from "@/ui/primitives";
import { rank, winner } from "@/lib/search";
import { stamp } from "@/lib/format";
import { cn } from "@/lib/cn";
import type { KitEntry, KitKind } from "@/wire/types";

/** Bytes, for a list where "is this the long one" is the question being asked. */
function size(bytes: number): string {
  return bytes < 1024 ? `${bytes} B` : `${Math.round(bytes / 1024)} KB`;
}

const SCOPE_COLOR: Record<string, string> = {
  user: "var(--blue)",
  plugin: "var(--purple)",
  project: "var(--green)",
};

export function KitView({ kind }: { kind: KitKind }) {
  const kit = useStore((s) => s.kit);
  const loaded = useStore((s) => s.kitLoaded);
  const doc = useStore((s) => s.kitDoc);
  const send = useStore((s) => s.send);
  const [filter, setFilter] = useState("");
  const [openPath, setOpenPath] = useState<string | null>(null);
  /** Long enough that a slow answer is not called a missing one. */
  const [late, setLate] = useState(false);

  // Asked for once per mount rather than on a timer: these are files you edit
  // by hand every few days, and a poll would be a scan of `~/.claude` for
  // nothing. The list refreshes when you come back to the view.
  useEffect(() => {
    send({ cmd: "fetch_kit" });
    const t = window.setTimeout(() => setLate(true), 3000);
    return () => window.clearTimeout(t);
  }, [send]);

  /**
   * The shared engines rather than a substring test. `R-F10`.
   *
   * This began as three `.includes()` calls, which is exactly the behaviour the
   * row was written against: it cannot find `qcommit` from `q-commit`, it
   * cannot match two words that are both present but not adjacent, and it
   * leaves the list in whatever order it arrived — so a perfect match on a name
   * can sit below a coincidence in a description. `rank` runs substring,
   * all-words and subsequence together and orders by the winner.
   *
   * Name, description and project are concatenated, which is deliberate and
   * has a cost worth stating: a query matching a name *and* a description
   * scores as one long haystack rather than being credited to the better field.
   * Fixing that means a per-field weighting, which is `scorePath`'s job for
   * paths and is more machinery than a filter box has earned here.
   */
  const ranked = useMemo(
    () =>
      rank(
        kit.filter((e) => e.kind === kind),
        filter,
        (e) => `${e.name} ${e.description} ${e.project ?? ""}`,
      ),
    [kit, kind, filter],
  );
  const rows = useMemo(() => ranked.map((r) => r.item), [ranked]);
  const engine = winner(ranked);

  const open = (e: KitEntry) => {
    setOpenPath(e.path);
    send({ cmd: "fetch_kit_doc", path: e.path });
  };

  // The answer carries its own path, so a slow read cannot render under a file
  // you have since clicked away from.
  const shown = doc && doc.path === openPath ? doc : null;

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex w-72 shrink-0 flex-col border-r border-[var(--border)]">
        <div className="flex shrink-0 items-center gap-1.5 px-2 py-1">
          <Input
            value={filter}
            onChange={setFilter}
            placeholder={kind === "skill" ? "filter skills" : "filter memories"}
            ariaLabel={kind === "skill" ? "filter skills" : "filter memories"}
          />
          {filter.trim() && engine && (
            <Dim
              className="shrink-0 text-2xs"
              title="which engine won — exact substring, all-words, or subsequence"
            >
              {engine}
            </Dim>
          )}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {rows.length === 0 ? (
            /*
              Three states, and the third one is why this is not a one-line
              empty. A list can be empty because the answer has not arrived,
              because the answer *was* empty, or because nothing answered at
              all — and the last one is what a window talking to a daemon older
              than itself looks like, which ADR-0009 makes an ordinary thing to
              be sitting in front of. Reported 2026-08-06 as a Memory panel that
              said "reading ~/.claude…" for ever; the daemon it was asking had
              been running since before `fetch_kit` existed.
            */
            <Empty
              hint={
                !loaded && late
                  ? "a daemon older than this window does not know how to answer — restart mogeung, or the daemon you left running"
                  : kind === "skill"
                    ? "skills live in ~/.claude/skills and in installed plugins"
                    : "memory lives in ~/.claude/projects/<project>/memory"
              }
            >
              {!loaded
                ? late
                  ? "no answer from the daemon"
                  : "reading ~/.claude…"
                : kit.length === 0
                  ? "nothing found under ~/.claude"
                  : "nothing matches"}
            </Empty>
          ) : (
            rows.map((e) => (
              <Row
                key={e.path}
                selected={openPath === e.path}
                onClick={() => open(e)}
                className="flex-col items-start gap-0.5 border-b border-[var(--border)] py-1"
              >
                <div className="flex w-full items-center gap-1">
                  <span className="truncate text-sm text-[var(--text-strong)]">{e.name}</span>
                  <Chip color={SCOPE_COLOR[e.scope] ?? "var(--dim)"}>{e.scope}</Chip>
                </div>
                {e.description && (
                  <div className="line-clamp-2 text-2xs text-[var(--dim)]">{e.description}</div>
                )}
                <div className="flex w-full items-center gap-2 text-2xs">
                  {e.project && <Dim className="truncate">{e.project}</Dim>}
                  <Dim className="ml-auto shrink-0">{size(e.bytes)}</Dim>
                  {e.modified && <Dim className="shrink-0">{stamp(e.modified)}</Dim>}
                </div>
              </Row>
            ))
          )}
        </div>
      </div>

      <div className="flex min-w-0 flex-1 flex-col">
        {!shown ? (
          <Empty hint={kind === "skill" ? "and the file to edit it in" : "and where it was written"}>
            pick one to read it
          </Empty>
        ) : (
          <>
            <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border)] px-2 py-1">
              {/* The path, prominently. This pane cannot edit these files and
                  should not pretend otherwise — what it owes you is where to
                  go. */}
              <span className="min-w-0 truncate" title={shown.path}>
                <Mono className="text-2xs text-[var(--dim)]">{shown.path}</Mono>
              </span>
              {shown.truncated && (
                <Chip color="var(--amber)" title="the file is longer than this — open it to see the rest">
                  truncated
                </Chip>
              )}
            </div>
            <div className={cn("min-h-0 flex-1 overflow-y-auto px-3 py-2")}>
              <div className="prose-mogeung text-sm">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{shown.body}</ReactMarkdown>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
