/**
 * A diff, rendered.
 *
 * The daemon computes the diff, the hunks, their anchors and their risk scores;
 * this only draws them. That split is why the client stays a projection — and
 * why the diff is hand-drawn rather than handed to Monaco's diff editor, which
 * wants two whole files where the wire carries unified hunks.
 *
 * Review marks are per **hunk**, keyed by a content hash rather than by
 * position, so a mark survives the agent rewriting the file around it
 * (`R-A5` — verified live: `auth.rs` stayed read while a rewritten `main.rs`
 * came back unread).
 */

import { useState } from "react";
import { Check, ChevronDown, ChevronRight, FileText } from "lucide-react";
import { useStore } from "@/store";
import { Chip, Dim, IconButton, Mono } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { openFile } from "@/lib/explorer";
import { FileIcon } from "@/ui/FileIcon";
import { riskFromScore, type FileChange, type Hunk, type RiskLevel } from "@/wire/types";

function riskColor(level: RiskLevel): string {
  switch (level) {
    case "high":
      return "var(--urgent)";
    case "medium":
      return "var(--amber)";
    case "low":
      return "var(--dim)";
    default:
      return "var(--noise)";
  }
}

function riskLabel(level: RiskLevel): string {
  return level === "high" ? "HIGH" : level === "medium" ? "med" : level;
}

/** One line of a unified hunk. The prefix is the whole of the classification. */
function DiffLine({ line }: { line: string }) {
  const kind = line[0];
  const bg =
    kind === "+" ? "var(--add-bg)" : kind === "-" ? "var(--del-bg)" : undefined;
  const fg =
    kind === "+" ? "var(--add-fg)" : kind === "-" ? "var(--del-fg)" : "var(--ctx-fg)";
  return (
    <div className="whitespace-pre px-2 font-mono text-sm leading-[1.45]" style={{ background: bg, color: fg }}>
      {line || " "}
    </div>
  );
}

function HunkBlock({
  hunk,
  sessionId,
  path,
}: {
  hunk: Hunk;
  sessionId: string;
  path: string;
}) {
  const send = useStore((s) => s.send);
  const hideNoise = useStore((s) => s.prefs.hideNoise);
  const [open, setOpen] = useState(true);
  const risk = riskFromScore(hunk.score);

  if (hideNoise && risk === "noise" && hunk.reviewed) return null;

  return (
    <div className={cn("border-t border-[var(--border)]", hunk.reviewed && "opacity-55")}>
      <div className="flex items-center gap-2 bg-[var(--bg)] px-2 py-0.5">
        <button type="button" onClick={() => setOpen(!open)} className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] text-[var(--dim)]">
          {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
        </button>
        <Mono className="truncate text-2xs text-[var(--dim)]">{hunk.header}</Mono>
        <span className="text-2xs text-[var(--add-fg)]">+{hunk.insertions}</span>
        <span className="text-2xs text-[var(--del-fg)]">−{hunk.deletions}</span>
        {hunk.flags.map((f) => (
          <Chip key={f} color={riskColor(risk)}>
            {f.replace(/_/g, "-")}
          </Chip>
        ))}
        <div className="ml-auto flex items-center gap-1">
          <span className="text-2xs" style={{ color: riskColor(risk) }}>
            {riskLabel(risk)}
          </span>
          <IconButton
            title={hunk.reviewed ? "mark unread" : "mark read  (Space)"}
            active={hunk.reviewed}
            onClick={() =>
              send({
                cmd: "set_hunk_reviewed",
                session_id: sessionId,
                anchor: hunk.anchor,
                reviewed: !hunk.reviewed,
              })
            }
          >
            <Check size={12} />
          </IconButton>
          <IconButton
            title="open this file in the Code pane"
            onClick={() => openFile(sessionId, path, { pin: true })}
          >
            <FileText size={11} />
          </IconButton>
        </div>
      </div>
      {open && (
        <div className="overflow-x-auto">
          {hunk.lines.map((l, i) => (
            <DiffLine key={i} line={l} />
          ))}
        </div>
      )}
    </div>
  );
}

export function FileDiff({ file, sessionId }: { file: FileChange; sessionId: string }) {
  const [open, setOpen] = useState(true);
  const risk = riskFromScore(file.score);
  const readCount = file.hunks.filter((h) => h.reviewed).length;

  return (
    <div className="border-b border-[var(--border)]">
      <div
        onClick={() => setOpen(!open)}
        className="flex cursor-default items-center gap-2 bg-[var(--bg-raised)] px-2 py-1 hover:bg-[var(--bg-faint)]"
      >
        {open ? <ChevronDown size={12} className="text-[var(--dim)]" /> : <ChevronRight size={12} className="text-[var(--dim)]" />}
        <FileIcon name={file.path.split("/").pop() ?? file.path} size={12} className="shrink-0" />
        <Mono className="truncate text-sm text-[var(--text-strong)]">{file.path}</Mono>
        {file.old_path && <Dim className="truncate text-2xs">← {file.old_path}</Dim>}
        <Chip color="var(--dim)">{file.status}</Chip>
        <span className="text-2xs text-[var(--add-fg)]">+{file.insertions}</span>
        <span className="text-2xs text-[var(--del-fg)]">−{file.deletions}</span>
        <div className="ml-auto flex items-center gap-2">
          <Dim className="text-2xs">
            {readCount}/{file.hunks.length} read
          </Dim>
          <span className="text-2xs" style={{ color: riskColor(risk) }}>
            {riskLabel(risk)}
          </span>
        </div>
      </div>
      {open &&
        file.hunks.map((h) => (
          <HunkBlock key={h.anchor} hunk={h} sessionId={sessionId} path={file.path} />
        ))}
      {open && file.truncated && (
        <Dim className="block px-2 py-1 text-2xs">this diff was capped — it goes on past what is shown</Dim>
      )}
    </div>
  );
}

export function DiffList({ files, sessionId }: { files: FileChange[]; sessionId: string }) {
  const hideReviewed = useStore((s) => s.prefs.hideReviewed);
  const shown = hideReviewed
    ? files.filter((f) => !(f.hunks.length > 0 && f.hunks.every((h) => h.reviewed)))
    : files;
  return (
    <>
      {shown.map((f) => (
        <FileDiff key={`${f.path}@${f.old_path ?? ""}`} file={f} sessionId={sessionId} />
      ))}
    </>
  );
}
