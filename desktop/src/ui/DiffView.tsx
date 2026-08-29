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
import { useMemo, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, FileText, Flag, Zap } from "lucide-react";
import { useStore } from "@/store";
import { Chip, Dim, IconButton, Mono } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { openFile } from "@/lib/explorer";
import { FileIcon } from "@/ui/FileIcon";
import { highlight, hunkStart, pairs, sideBySide, wordDiff, type Tok } from "@/lib/diff";
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
function LineText({
  line,
  other,
  syntax,
  wordDiffOn,
}: {
  line: string;
  other?: string;
  syntax: boolean;
  wordDiffOn: boolean;
}) {
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

/**
 * One line of a unified hunk. The prefix is the whole of the classification.
 *
 * Memoized on primitives — line text and the two colouring prefs — so a store
 * write that touches neither skips every line of every mounted hunk. The prefs
 * arrive as props rather than subscriptions: a diff holds thousands of lines,
 * and one selector per pref per line meant every store write ran them all.
 */
const DiffLine = React.memo(function DiffLine({
  line,
  other,
  syntax,
  wordDiffOn,
}: {
  line: string;
  other?: string;
  syntax: boolean;
  wordDiffOn: boolean;
}) {
  const kind = line[0];
  const bg =
    kind === "+" ? "var(--add-bg)" : kind === "-" ? "var(--del-bg)" : undefined;
  const fg =
    kind === "+" ? "var(--add-fg)" : kind === "-" ? "var(--del-fg)" : "var(--ctx-fg)";
  return (
    <div className="whitespace-pre px-2 font-mono text-sm leading-[1.45]" style={{ background: bg, color: fg }}>
      {line ? <LineText line={line} other={other} syntax={syntax} wordDiffOn={wordDiffOn} /> : " "}
    </div>
  );
});

/**
 * One line of one side, with its number in the file.
 *
 * The number sits in a `sticky` gutter rather than in a column of its own, so
 * it stays put while a long line scrolls under it — and so the two halves need
 * no agreement about gutter width to stay in step.
 *
 * An absent side draws an empty row with a faint wash. It is not decoration:
 * without it the rows after a lopsided run would slide up one column, and the
 * whole point of this view is that a row means the same line on both sides.
 */
const SplitRow = React.memo(function SplitRow({
  line,
  no,
  other,
  syntax,
  wordDiffOn,
}: {
  line: string | null;
  no: number | null;
  other: string | null;
  syntax: boolean;
  wordDiffOn: boolean;
}) {
  const kind = line?.[0];
  const bg =
    kind === "+"
      ? "var(--add-bg)"
      : kind === "-"
        ? "var(--del-bg)"
        : line === null
          ? "var(--bg-faint)"
          : undefined;
  const fg =
    kind === "+" ? "var(--add-fg)" : kind === "-" ? "var(--del-fg)" : "var(--ctx-fg)";
  return (
    <div className="flex font-mono text-sm leading-[1.45]" style={{ background: bg, color: fg }}>
      <span
        className="sticky left-0 z-10 flex w-11 shrink-0 items-center justify-end px-1.5 text-2xs text-[var(--noise)] select-none"
        style={{ background: bg ?? "var(--bg-panel)" }}
      >
        {no ?? " "}
      </span>
      <span className="whitespace-pre pr-3 pl-1">
        {line === null ? (
          " "
        ) : (
          <LineText line={line} other={other ?? undefined} syntax={syntax} wordDiffOn={wordDiffOn} />
        )}
      </span>
    </div>
  );
});

/**
 * Side by side: the file as it was on the left, as it is on the right. `R-D6`.
 *
 * **Two scrollers, moved together.** One shared scroller would let a long line
 * on one side push the other side off the screen, and a scroller per side that
 * moved independently would break the one promise this view makes — that a row
 * is the same line on both halves. So each half scrolls its own text and each
 * tells the other where it went.
 */
const SplitLines = React.memo(function SplitLines({
  lines,
  header,
  syntax,
  wordDiffOn,
}: {
  lines: string[];
  header: string;
  syntax: boolean;
  wordDiffOn: boolean;
}) {
  const rows = useMemo(() => sideBySide(lines, hunkStart(header)), [lines, header]);
  const left = useRef<HTMLDivElement>(null);
  const right = useRef<HTMLDivElement>(null);

  // Comparing before assigning is what stops the echo: an equal write fires no
  // scroll event, so the two sides settle instead of handing the event back.
  const follow = (from: React.RefObject<HTMLDivElement | null>, to: React.RefObject<HTMLDivElement | null>) => () => {
    const a = from.current;
    const b = to.current;
    if (!a || !b || a.scrollLeft === b.scrollLeft) return;
    b.scrollLeft = a.scrollLeft;
  };

  return (
    <div className="grid grid-cols-2 bg-[var(--bg-panel)]">
      <div
        ref={left}
        onScroll={follow(left, right)}
        className="min-w-0 overflow-x-auto border-r border-[var(--border)]"
      >
        {rows.map((r, i) => (
          <SplitRow key={i} line={r.left} no={r.leftNo} other={r.right} syntax={syntax} wordDiffOn={wordDiffOn} />
        ))}
      </div>
      <div ref={right} onScroll={follow(right, left)} className="min-w-0 overflow-x-auto">
        {rows.map((r, i) => (
          <SplitRow key={i} line={r.right} no={r.rightNo} other={r.left} syntax={syntax} wordDiffOn={wordDiffOn} />
        ))}
      </div>
    </div>
  );
});

const HunkBlock = React.memo(function HunkBlock({
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
  const syntax = useStore((s) => s.prefs.syntax);
  const wordDiffOn = useStore((s) => s.prefs.wordDiff);
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
        // Split scrolls each half itself, so an outer scroller here would be a
        // second one wrapping the first.
        <div className={cn(!split && "overflow-x-auto")}>
          {split ? (
            <SplitLines lines={hunk.lines} header={hunk.header} syntax={syntax} wordDiffOn={wordDiffOn} />
          ) : (
            hunk.lines.map((l, i) => {
              // The line this one replaces, when the hunk pairs them — that is
              // what a word diff needs and what a lone `+` cannot have.
              const twin = paired.get(i);
              return (
                <DiffLine
                  key={i}
                  line={l}
                  other={twin === undefined ? undefined : hunk.lines[twin]}
                  syntax={syntax}
                  wordDiffOn={wordDiffOn}
                />
              );
            })
          )}
        </div>
      )}
    </div>
  );
});

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

export const FileDiff = React.memo(function FileDiff({
  file,
  sessionId,
  reason,
  defaultOpen = true,
}: {
  file: FileChange;
  sessionId: string;
  /** The guide's line for this file, `""` when it ranked nothing here. */
  reason?: string;
  /** Whether this file starts expanded. `false` on a large diff — see
   *  `EXPAND_UP_TO`. Clicking the header still opens it, as it always did. */
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const send = useStore((s) => s.send);
  const asked = useStore((s) => s.radius?.path === file.path && s.radius?.session_id === sessionId);
  const risk = riskFromScore(file.score);
  const readCount = file.hunks.filter((h) => h.reviewed).length;

  // Layout and paint are skipped while the section is off screen —
  // `content-visibility` is the windowing this list gets. A real virtualiser
  // would fight the collapsible sections and per-hunk review marks for
  // little extra: the rows are memoized, so what remains off screen was
  // costing layout, and now costs an estimate. The estimate only shapes the
  // scrollbar before first paint; degrades to nothing on engines without it.
  const estimate = 40 + file.hunks.reduce((n, h) => n + 28 + h.lines.length * 20, 0);
  return (
    <div
      className="border-b border-[var(--border)]"
      style={
        {
          contentVisibility: "auto",
          containIntrinsicSize: `auto ${Math.min(estimate, 20_000)}px`,
        } as React.CSSProperties
      }
    >
      <div
        onClick={() => setOpen(!open)}
        className="flex cursor-default items-center gap-2 bg-[var(--bg-raised)] px-2 py-1 hover:bg-[var(--bg-faint)]"
      >
        {open ? <ChevronDown size={12} className="text-[var(--dim)]" /> : <ChevronRight size={12} className="text-[var(--dim)]" />}
        <FileIcon name={file.path.split("/").pop() ?? file.path} size={12} className="shrink-0" />
        <Mono className="truncate text-sm text-[var(--text-strong)]">{file.path}</Mono>
        {file.old_path && <Dim className="truncate text-2xs">← {file.old_path}</Dim>}
        {/*
          The reason travels with the file, which is `attention-ranking.md`'s
          rule: an ordering whose reason lives somewhere else is a black box.
          `unranked` is said out loud rather than left blank — a file the model
          skipped and a guide that is switched off must not look the same.
        */}
        {reason !== undefined &&
          (reason ? (
            <Dim className="truncate text-2xs italic">— {reason}</Dim>
          ) : (
            <Dim className="shrink-0 text-2xs opacity-60">unranked</Dim>
          ))}
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
});

/**
 * How many files a diff may have before it opens collapsed.
 *
 * Twelve is about a screen of header rows: past that you are scanning a list
 * to decide where to start, which is exactly what the reading guide is for,
 * and every hunk being in the DOM buys nothing but the wait.
 */
const EXPAND_UP_TO = 12;

export function DiffList({
  files,
  sessionId,
  reasons,
}: {
  files: FileChange[];
  sessionId: string;
  /**
   * One line per file from the reading guide, keyed by path. `R-O3`.
   *
   * Absent when the guide is off, which is the ordinary case — this pane is
   * exactly what it was without it. A file missing from a present map is one
   * the model did not rank, and it says so rather than showing nothing, since
   * *the model ignored this* and *the guide is off* must not look alike.
   */
  reasons?: Map<string, string>;
}) {
  const hideReviewed = useStore((s) => s.prefs.hideReviewed);
  const shown = hideReviewed
    ? files.filter((f) => !(f.hunks.length > 0 && f.hunks.every((h) => h.reviewed)))
    : files;
  // A big diff opens as a **list**, not as every hunk of every file at once.
  //
  // Reported 2026-08-29 as *"the Changes pane is very slow… it shows all the
  // files all expanded"*. Nothing here is virtualised, so a session whose base
  // is a few days back can put hundreds of files and every one of their hunks
  // into the DOM in one go — 280 files was a real one on this machine.
  //
  // Above the threshold the header row is still the whole scannable answer:
  // path, ±, risk, hunks read, and the guide's reason when it is on. Below it,
  // nothing changes — a handful of files is a diff you came to read.
  const defaultOpen = shown.length <= EXPAND_UP_TO;
  return (
    <>
      {shown.map((f) => (
        <FileDiff
          key={`${f.path}@${f.old_path ?? ""}`}
          file={f}
          sessionId={sessionId}
          reason={reasons ? (reasons.get(f.path) ?? "") : undefined}
          defaultOpen={defaultOpen}
        />
      ))}
    </>
  );
}
