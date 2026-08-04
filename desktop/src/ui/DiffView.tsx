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

import * as React from "react";
import { useMemo, useState } from "react";
import { Check, ChevronDown, ChevronRight, FileText, Flag, Zap } from "lucide-react";
import { useStore } from "@/store";
import { Chip, Dim, IconButton, Mono } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { openFile } from "@/lib/explorer";
import { FileIcon } from "@/ui/FileIcon";
import { highlight, pairs, sideBySide, wordDiff, type Tok } from "@/lib/diff";
import { changedLines } from "@/lib/prompt";
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

const TOK_COLOR: Record<Tok, string | undefined> = {
  plain: undefined,
  keyword: "var(--syn-keyword)",
  string: "var(--syn-string)",
  comment: "var(--syn-comment)",
  number: "var(--syn-number)",
  type: "var(--syn-type)",
};

/**
 * A line's text, coloured by whichever of the two aids is on.
 *
 * They are deliberately exclusive per line. A word diff says *what moved* and a
 * highlighter says *what this is*, and drawing both on one changed line means
 * two colour systems arguing over the same characters — the emphasis that
 * matters loses. So a paired change wears the word diff, and everything else
 * takes syntax colour.
 */
function LineText({ line, other }: { line: string; other?: string }) {
  const syntax = useStore((s) => s.prefs.syntax);
  const wordDiffOn = useStore((s) => s.prefs.wordDiff);
  const kind = line[0];
  const changedLine = kind === "+" || kind === "-";

  if (wordDiffOn && changedLine && other !== undefined) {
    // `-` is always the left argument, whichever side we are drawing, so the
    // two lines of a pair mark the same words.
    const [minus, plus] = kind === "-" ? wordDiff(line, other) : wordDiff(other, line);
    const spans = kind === "-" ? minus : plus;
    return (
      <>
        {spans.map((s, i) => (
          <span
            key={i}
            style={
              s.changed
                ? { background: kind === "+" ? "var(--add-emph)" : "var(--del-emph)" }
                : undefined
            }
          >
            {s.text}
          </span>
        ))}
      </>
    );
  }

  if (!syntax) return <>{line || " "}</>;

  // The marker keeps the line's own colour: it is punctuation about the diff,
  // not code, and tokenizing it would call `-` an operator.
  const marker = changedLine || kind === " " ? line.slice(0, 1) : "";
  return (
    <>
      {marker}
      {highlight(line.slice(marker.length)).map((p, i) => (
        <span key={i} style={{ color: TOK_COLOR[p.tok] }}>
          {p.text}
        </span>
      ))}
    </>
  );
}

/** One line of a unified hunk. The prefix is the whole of the classification. */
function DiffLine({ line, other }: { line: string; other?: string }) {
  const kind = line[0];
  const bg =
    kind === "+" ? "var(--add-bg)" : kind === "-" ? "var(--del-bg)" : undefined;
  const fg =
    kind === "+" ? "var(--add-fg)" : kind === "-" ? "var(--del-fg)" : "var(--ctx-fg)";
  return (
    <div className="whitespace-pre px-2 font-mono text-sm leading-[1.45]" style={{ background: bg, color: fg }}>
      {line ? <LineText line={line} other={other} /> : " "}
    </div>
  );
}

/** Side by side: the removed file on the left, the added one on the right. */
function SplitLines({ lines }: { lines: string[] }) {
  const rows = sideBySide(lines);
  return (
    <div className="grid grid-cols-2">
      {rows.map((r, i) => (
        <React.Fragment key={i}>
          {/* An absent side is a blank cell, not a missing one, or every row
              after an unbalanced hunk would slide up one column. */}
          <div className="border-r border-[var(--border)]">
            {r.left === null ? (
              <div className="px-2 font-mono text-sm leading-[1.45]">{" "}</div>
            ) : (
              <DiffLine line={r.left} other={r.right ?? undefined} />
            )}
          </div>
          <div>
            {r.right === null ? (
              <div className="px-2 font-mono text-sm leading-[1.45]">{" "}</div>
            ) : (
              <DiffLine line={r.right} other={r.left ?? undefined} />
            )}
          </div>
        </React.Fragment>
      ))}
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
  const split = useStore((s) => s.prefs.sideBySide);
  const [open, setOpen] = useState(true);
  const risk = riskFromScore(hunk.score);
  const paired = useMemo(() => pairs(hunk.lines), [hunk.lines]);
  const flaggedHere = useStore((s) =>
    s.flagged.some((f) => f.path === path && f.header === hunk.header),
  );

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
            title={
              flaggedHere
                ? "flagged for the follow-up prompt — press again to unflag"
                : "flag this hunk for a follow-up prompt you will paste yourself"
            }
            active={flaggedHere}
            onClick={() => {
              const flagged = useStore.getState().flagged;
              useStore.setState({
                flagged: flaggedHere
                  ? flagged.filter((f) => !(f.path === path && f.header === hunk.header))
                  : [
                      ...flagged,
                      {
                        sessionId,
                        path,
                        header: hunk.header,
                        note: "",
                        body: changedLines(hunk.lines),
                      },
                    ],
              });
            }}
          >
            <Flag size={11} />
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
          {split ? (
            <SplitLines lines={hunk.lines} />
          ) : (
            hunk.lines.map((l, i) => {
              // The line this one replaces, when the hunk pairs them — that is
              // what a word diff needs and what a lone `+` cannot have.
              const twin = paired.get(i);
              return <DiffLine key={i} line={l} other={twin === undefined ? undefined : hunk.lines[twin]} />;
            })
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Who else touches what this file changed. `R-D9`.
 *
 * The daemon does the finding; this asks and draws. Symbols first because they
 * are the question — *what did you change* — then every reference to them,
 * tests marked, because "the callers are all tests" and "nothing covers this"
 * are the two answers that change what you do next.
 */
function BlastRadiusPanel({ path, sessionId }: { path: string; sessionId: string }) {
  const radius = useStore((s) => s.radius);
  if (!radius || radius.path !== path || radius.session_id !== sessionId) return null;

  if (radius.symbols.length === 0) {
    return (
      <Dim className="block px-2 py-1 text-2xs">
        no changed symbol was recognised in this file — nothing to trace
      </Dim>
    );
  }

  return (
    <div className="border-t border-[var(--border)] bg-[var(--bg)] px-2 py-1">
      <div className="flex flex-wrap items-center gap-1">
        <Dim className="text-2xs">changed:</Dim>
        {radius.symbols.map((s) => (
          <Chip key={s} color="var(--purple)">
            {s}
          </Chip>
        ))}
      </div>
      {radius.references.length === 0 ? (
        <Dim className="mt-1 block text-2xs">
          nothing outside this file mentions them — either it is new, or it is unused
        </Dim>
      ) : (
        <div className="mt-1">
          {radius.references.map((r, i) => (
            <div
              key={`${r.path}:${r.line}:${i}`}
              onClick={() => openFile(sessionId, r.path, { pin: true, line: r.line })}
              title={`${r.path}:${r.line}`}
              className="flex cursor-default items-baseline gap-1 overflow-hidden py-px whitespace-nowrap hover:bg-[var(--bg-faint)]"
            >
              {r.is_test && <Chip color="var(--green)">test</Chip>}
              <Mono className="text-2xs text-[var(--dim)]">
                {r.path}:{r.line}
              </Mono>
              <Mono className="truncate text-2xs">{r.text.trim()}</Mono>
            </div>
          ))}
        </div>
      )}
      {radius.truncated && (
        <Dim className="mt-1 block text-2xs">the search hit its cap — there are more</Dim>
      )}
    </div>
  );
}

export function FileDiff({ file, sessionId }: { file: FileChange; sessionId: string }) {
  const [open, setOpen] = useState(true);
  const send = useStore((s) => s.send);
  const asked = useStore((s) => s.radius?.path === file.path && s.radius?.session_id === sessionId);
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
          <IconButton
            title="blast radius — who else calls or tests what this file changed  (R-D9)"
            active={asked}
            onClick={(e) => {
              // The header row toggles the file open; this must not also fold it.
              e.stopPropagation();
              send({ cmd: "fetch_blast_radius", session_id: sessionId, path: file.path });
            }}
          >
            <Zap size={11} />
          </IconButton>
        </div>
      </div>
      {asked && <BlastRadiusPanel path={file.path} sessionId={sessionId} />}
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
