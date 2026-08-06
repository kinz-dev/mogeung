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
  const doc = useStore((s) => s.kitDoc);
  const send = useStore((s) => s.send);
  const [filter, setFilter] = useState("");
  const [openPath, setOpenPath] = useState<string | null>(null);

  // Asked for once per mount rather than on a timer: these are files you edit
  // by hand every few days, and a poll would be a scan of `~/.claude` for
  // nothing. The list refreshes when you come back to the view.
  useEffect(() => {
    send({ cmd: "fetch_kit" });
  }, [send]);

  const rows = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return kit
      .filter((e) => e.kind === kind)
      .filter(
        (e) =>
          !q ||
          e.name.toLowerCase().includes(q) ||
          e.description.toLowerCase().includes(q) ||
          (e.project ?? "").toLowerCase().includes(q),
      );
  }, [kit, kind, filter]);

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
        <div className="shrink-0 px-2 py-1">
          <Input
            value={filter}
            onChange={setFilter}
            placeholder={kind === "skill" ? "filter skills" : "filter memories"}
            ariaLabel={kind === "skill" ? "filter skills" : "filter memories"}
          />
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {rows.length === 0 ? (
            <Empty
              hint={
                kind === "skill"
                  ? "skills live in ~/.claude/skills and in installed plugins"
                  : "memory lives in ~/.claude/projects/<project>/memory"
              }
            >
              {kit.length === 0 ? "reading ~/.claude…" : "nothing matches"}
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
