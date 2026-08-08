/**
 * One open file, read. `R-B24`, workbench behaviour by `R-B25`, one pane each
 * since `R-B53`.
 *
 * **A viewer, and permanently so.** Pillar K puts an editor under "explicitly
 * not", and it is a property of the protocol rather than of this pane: the
 * daemon offers `fetch_file` and nothing that writes one back. Monaco is here
 * in `readOnly` mode because a read-only editor is strictly a better *viewer*
 * than a hand-rolled one — folding, the find widget, go-to-line, bracket
 * matching, a minimap and column selection all arrive free — not because
 * editing is a step away.
 *
 * **No tree.** It lives in the rail (`R-B41`) so it can be read with any pane
 * forward. **And no tab strip**: every open file is a pane of its own, so
 * dockview's tabs *are* the file tabs and the internal two-way split it used to
 * carry is dockview's splitting instead. What is left here is one file.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import {
  WrapText,
  ListTree,
  Eye,
  UserRound,
  Bookmark,
  ChevronUp,
  ChevronDown,
} from "lucide-react";
import { useStore } from "@/store";
import { usePaneId } from "@/lib/paneScope";
import { parseFilePaneId } from "@/lib/panes";
import { Dim, Empty, IconButton } from "@/ui/primitives";
import { explorerFetch, languageOf } from "@/lib/explorer";
import { base } from "@/lib/format";
import { defineMogeungThemes, monacoTheme } from "@/lib/monaco-theme";
import { outline, symbolGlyph } from "@/lib/symbols";
import { Input } from "@/ui/primitives";
import ReactMarkdown from "react-markdown";
import { Mermaid } from "@/ui/Mermaid";
import type { ThemeMode } from "@/store/prefs";
import remarkGfm from "remark-gfm";
import { renderedLines } from "@/lib/markdown";
import { best } from "@/lib/search";

/**
 * The rendered preview, and a find that searches what it says. `R-B38`.
 *
 * `R-B29` shipped the preview and left `Ctrl+F` searching the *source* behind
 * it — Monaco's find, on a buffer nobody is looking at. So the words on the
 * screen were the one thing that could not be searched, and the syntax you
 * cannot see was matchable.
 *
 * Hits are in **document order**, not score order. The winner is still named,
 * which is the rule `search.ts` was built on, but a find bar's arrows have to
 * walk down the page — a "next" that jumped to a better match further up is a
 * next that lost you.
 *
 * What this does **not** do, said plainly because it is the obvious next
 * expectation: it does not highlight the match in the rendering or scroll to
 * it. `react-markdown` owns that DOM and marking it up means walking text
 * nodes it may re-create at any render. The payoff here is the source line —
 * the row asks for a hit you can go and edit, and that is what the arrow keys
 * and the button deliver.
 */
function MarkdownPreview({
  content,
  onEditSource,
  theme,
  zoom,
}: {
  content: string;
  onEditSource: (line: number) => void;
  theme: ThemeMode;
  /**
   * Ctrl+wheel, applied here rather than by `ZoomPane`. `R-J26`.
   *
   * The file pane is registered `scale={false}` because Monaco takes the
   * factor as a **font size** — CSS `zoom` over an editor that measures in
   * device pixels puts a click at the wrong character. The preview is not
   * Monaco and has no such reader, so with the wrapper's hands off it, the
   * factor reached nothing at all and zooming a rendered document did
   * nothing. It applies the factor itself.
   */
  zoom: number;
}) {
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const [finding, setFinding] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const hits = useMemo(() => {
    const q = query.trim();
    if (!q) return [];
    return renderedLines(content)
      .map((l) => ({ ...l, hit: best(q, l.text) }))
      .filter((l): l is typeof l & { hit: NonNullable<typeof l.hit> } => l.hit !== null);
  }, [content, query]);

  // A shorter query can leave the cursor past the end of the new results.
  const cursor = hits.length === 0 ? 0 : Math.min(at, hits.length - 1);
  const current = hits[cursor];

  /**
   * `Ctrl+F` opens it, and only from inside this preview.
   *
   * Scoped the same way Notes scopes its own find, and here it is not merely
   * tidy: in *source* mode this pane hands `Ctrl+F` to Monaco's find widget,
   * which is the right owner of it. A window-wide binding would have to take
   * it back from the editor to give it to a preview that is usually off.
   */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.key === "f" && (e.ctrlKey || e.metaKey))) return;
      const root = rootRef.current;
      if (!root || !root.contains(document.activeElement)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      setFinding(true);
      requestAnimationFrame(() => inputRef.current?.select());
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const step = (down: boolean) => {
    if (hits.length === 0) return;
    setAt((n) => (down ? (n + 1) % hits.length : (n - 1 + hits.length) % hits.length));
  };

  return (
    // `tabIndex` so the preview can *hold* focus: `Ctrl+F` is scoped by asking
    // whether the active element is inside here, and a div full of prose has
    // nothing focusable in it until something says so.
    <div ref={rootRef} tabIndex={-1} className="flex min-h-0 flex-1 flex-col outline-none">
      {finding && (
        <div className="flex shrink-0 items-center gap-1.5 border-b border-[var(--border)] px-2 py-1">
          <Input
            inputRef={inputRef}
            value={query}
            onChange={(v) => {
              setQuery(v);
              setAt(0);
            }}
            placeholder="find in the preview"
            ariaLabel="find in the preview"
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setFinding(false);
                setQuery("");
              }
              if (e.key === "Enter") {
                e.preventDefault();
                step(!e.shiftKey);
              }
            }}
          />
          <Dim className="shrink-0 text-2xs whitespace-nowrap">
            {query.trim() ? `${hits.length === 0 ? 0 : cursor + 1}/${hits.length}` : ""}
          </Dim>
          {current && (
            <>
              {/* The engine names itself — the rule `search.ts` exists for. */}
              <Dim
                className="shrink-0 text-2xs"
                title="which of the engines matched — exact, all-words, or subsequence"
              >
                {current.hit.engine}
              </Dim>
              <button
                type="button"
                onClick={() => onEditSource(current.line)}
                title="open the source at this line — the only useful thing to do with a hit"
                className="shrink-0 rounded-sm border border-[var(--border)] px-1.5 py-px text-2xs outline-none transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] hover:border-[var(--border-hover)] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--ring)]"
              >
                line {current.line}
              </button>
            </>
          )}
          <IconButton title="previous match  (Shift+Enter)" disabled={hits.length === 0} onClick={() => step(false)}>
            <ChevronUp size={12} />
          </IconButton>
          <IconButton title="next match  (Enter)" disabled={hits.length === 0} onClick={() => step(true)}>
            <ChevronDown size={12} />
          </IconButton>
        </div>
      )}
      {current && (
        <div className="shrink-0 truncate border-b border-[var(--border)] bg-[var(--bg-faint)] px-2 py-1 text-2xs text-[var(--dim)]">
          {current.text}
        </div>
      )}
      {/* **The document scales, the chrome does not.** The find bar above is
          a control rather than content, and a filter box that grows with the
          prose is a control you then have to hunt for — the same reason the
          queue is not inside anything zoomable. */}
      <div
        data-testid="preview-body"
        className="min-h-0 flex-1 overflow-auto px-3 py-2"
        style={zoom === 1 ? undefined : { zoom }}
      >
        <div className="prose-mogeung">
          {/* **Only the fence is intercepted.** `code` fires for inline spans
              too, and an inline `mermaid` is a word rather than a diagram —
              the `language-mermaid` class is what tells them apart, and
              anything else falls through to the code block it always was.

              Deliberately the file pane alone (`R-J22`): `Mermaid` carries the
              note on what extending it to the Transcript, Notes and Kit would
              have to answer first. */}
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
              code({ className, children, ...rest }) {
                if (/\blanguage-mermaid\b/.test(className ?? "")) {
                  return <Mermaid chart={String(children).trimEnd()} theme={theme} />;
                }
                return (
                  <code className={className} {...rest}>
                    {children}
                  </code>
                );
              },
            }}
          >
            {content}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  );
}

/**
 * One file, in a pane of its own. `R-B53`.
 *
 * **The session comes from the pane, not from the selection.** That one line is
 * the whole of "a file pane stays put": clicking another session in the queue
 * changes `selected` and this keeps rendering what it was opened with, so a
 * file from one repo can sit beside an agent working in another.
 */
function Viewer({ session, path, rev }: { session: string; path: string; rev: string | null }) {
  const id = session;
  const st = useStore((s) => s.explorer[session]);
  const theme = useStore((s) => s.prefs.theme);
  const wrapPaths = useStore((s) => s.scoped().editorWrap);
  const setScoped = useStore((s) => s.setScoped);
  /**
   * The factor Ctrl+wheel over this pane actually writes. `R-J26`.
   *
   * It read `zoom["code"]` and the wrapper writes `zoom["file"]` — `R-B53`
   * dissolved the Code pane into one pane per file and renamed the registry
   * entry, and this reader was left pointing at the old key. So Ctrl+wheel in
   * a file pane has been inert since: the factor was written, stored, clamped
   * and never read by anything.
   *
   * `zoom["code"]` is the fallback rather than being deleted, so a factor set
   * before that rename is inherited once instead of being silently reset —
   * the same courtesy `R-B41` paid the `editor-tree` key.
   */
  const zoom = useStore((s) => s.prefs.zoom["file"] ?? s.prefs.zoom["code"] ?? 1);
  const send = useStore((s) => s.send);
  const scoped = useStore((s) => s.scoped());
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Parameters<OnMount>[1] | null>(null);
  const decorations = useRef<editor.IEditorDecorationsCollection | null>(null);
  const [outlineOpen, setOutlineOpen] = useState(false);
  const [outlineFilter, setOutlineFilter] = useState("");
  const [preview, setPreview] = useState(false);
  const [blameOn, setBlameOn] = useState(false);

  const index = st ? st.open.findIndex((t) => t.path === path && t.rev === rev) : -1;
  const tab = index >= 0 && st ? st.open[index] : null;

  // Go to the line a search hit or a diff row asked for, once, when the body
  // is actually there. Doing it on every render would fight the scrollbar.
  useEffect(() => {
    if (!tab?.gotoLine || !tab.content || !editorRef.current || !id || index < 0 || !st) return;
    editorRef.current.revealLineInCenter(tab.gotoLine);
    editorRef.current.setPosition({ lineNumber: tab.gotoLine, column: 1 });
    useStore
      .getState()
      .patchExplorer(id, { open: st.open.map((t, i) => (i === index ? { ...t, gotoLine: null } : t)) });
  }, [tab?.gotoLine, tab?.content, id, index, st]);

  const ext = tab ? tab.path.slice(tab.path.lastIndexOf(".") + 1).toLowerCase() : "";
  const isMarkdown = ext === "md" || ext === "markdown";
  const blameKey = tab && id ? `${id}:${tab.rev ?? ""}:${tab.path}` : "";
  const blame = useStore((s) => (blameKey ? s.blame[blameKey] : undefined));
  const marks = useMemo(
    () => (id && tab ? scoped.bookmarks.filter(([sid, p]) => sid === id && p === tab.path) : []),
    [scoped.bookmarks, id, tab],
  );

  // Asked for once per (file, revision) and only while the gutter is on: blame
  // is a `git blame` per file, which is real work on the daemon's side and must
  // not happen because a tab was opened.
  useEffect(() => {
    if (!blameOn || !id || !tab || blame) return;
    send({ cmd: "git_blame", session_id: id, path: tab.path, rev: tab.rev });
  }, [blameOn, id, tab, blame, send]);

  /**
   * The gutter and the bookmark marks, as Monaco decorations.
   *
   * Blame is **injected text**, not a real gutter: Monaco's glyph margin holds
   * an icon and nothing else, and a second synchronised scroll pane beside the
   * editor is a lot of machinery to keep aligned. Injected text moves the code
   * right, which is the honest cost of putting words in a margin that has none.
   *
   * Repeats are blank, the way `git blame` reads on a terminal — a column that
   * says the same sha forty times is noise with a value in it.
   */
  useEffect(() => {
    const ed = editorRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco) return;
    if (!decorations.current) decorations.current = ed.createDecorationsCollection([]);

    const next: editor.IModelDeltaDecoration[] = [];
    if (blameOn && blame) {
      let prev = "";
      blame.lines.forEach((l, i) => {
        const repeat = l.sha === prev;
        prev = l.sha;
        const when = new Date(l.epoch * 1000).toISOString().slice(0, 10);
        const text = repeat ? "" : `${l.sha.slice(0, 8)} ${l.author.slice(0, 14)} ${when}`;
        next.push({
          range: new monaco.Range(i + 1, 1, i + 1, 1),
          options: {
            before: { content: text.padEnd(34, " "), inlineClassName: "blame-gutter" },
          },
        });
      });
    }
    for (const [, , line] of marks) {
      next.push({
        range: new monaco.Range(line, 1, line, 1),
        options: { isWholeLine: true, linesDecorationsClassName: "bookmark-mark" },
      });
    }
    decorations.current.set(next);
  }, [blameOn, blame, marks]);

  // A pane whose file is gone from the store — closed from the tree while its
  // tab lingered a frame — says so rather than rendering an empty editor.
  if (!tab) {
    return (
      <Empty hint="it was closed from somewhere else — this pane can go too">
        {base(path)} is no longer open
      </Empty>
    );
  }
  if (tab.content === null) return <Empty>loading {base(tab.path)}…</Empty>;

  const wrap = wrapPaths.includes(tab.path);
  const symbols = outline(tab.content, ext);

  const toggleMark = (line: number) => {
    if (!id) return;
    const has = marks.some(([, , l]) => l === line);
    setScoped({
      bookmarks: has
        ? scoped.bookmarks.filter(([sid, p, l]) => !(sid === id && p === tab.path && l === line))
        : [...scoped.bookmarks, [id, tab.path, line]],
    });
  };

  const onMount: OnMount = (ed, monaco) => {
    editorRef.current = ed;
    monacoRef.current = monaco;
    defineMogeungThemes(monaco);
    monaco.editor.setTheme(monacoTheme(theme));
    // The glyph margin is the gesture: a click in it marks the line. Nothing
    // else lives there, so there is nothing to collide with.
    ed.onMouseDown((e) => {
      if (e.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) return;
      const line = e.target.position?.lineNumber;
      if (line) toggleMark(line);
    });
  };

  return (
    // `h-full`, not `flex-1`, and the difference is the whole pane. `ZoomPane`
    // is a *block* — `flex-1` on a child of it resolves to nothing, so the root
    // sized to its content, the editor below it took `flex-1` of a height that
    // was never distributed, and the file arrived as a sliver under its own
    // toolbar. It worked before `R-B53` because this markup used to be a child
    // of the Code pane's flex column; lifting it to be the pane root took the
    // flex context away with it. Every other pane says `h-full` here.
    <div className="flex h-full min-h-0 flex-col">
      {/*
        The controls row. With one file per pane the dockview tab already names
        the file, so this carries only what you *do* to it — and `head only`,
        which is a fact about the body rather than about the file.
      */}
      <div className="flex h-6 shrink-0 items-center gap-0.5 border-b border-[var(--border)] px-1.5">
        {tab.truncated && (
          <span className="shrink-0 text-2xs text-[var(--amber)]" title="the file went past the size cap">
            head only
          </span>
        )}
          {isMarkdown && (
            <IconButton
              title="read it as markdown rather than as source"
              active={preview}
              onClick={() => setPreview(!preview)}
            >
              <Eye size={12} />
            </IconButton>
          )}
          <IconButton
            title="blame — who last touched each line, and when  (R-D10)"
            active={blameOn}
            onClick={() => setBlameOn(!blameOn)}
          >
            <UserRound size={12} />
          </IconButton>
          <IconButton
            title={
              marks.length > 0
                ? `${marks.length} bookmark(s) in this file — click the glyph margin to add or remove`
                : "bookmarks: click the glyph margin beside a line"
            }
            active={marks.length > 0}
            onClick={() => {
              const line = editorRef.current?.getPosition()?.lineNumber;
              if (line) toggleMark(line);
            }}
          >
            <Bookmark size={12} />
          </IconButton>
          <IconButton
            title="symbol outline  (R-B28). A line scanner, not a parser — it will miss things"
            active={outlineOpen}
            onClick={() => setOutlineOpen(!outlineOpen)}
          >
            <ListTree size={12} />
          </IconButton>
          <IconButton
            title="wrap long lines — per file, because wrap is a property of prose"
            active={wrap}
            onClick={() =>
              setScoped({
                editorWrap: wrap ? wrapPaths.filter((p) => p !== tab.path) : [...wrapPaths, tab.path],
              })
            }
          >
            <WrapText size={12} />
          </IconButton>
      </div>
      <div className="flex min-h-0 flex-1">
      {/* Markdown is *read* here, not previewed beside its source: a viewer's
          job is the rendered thing, and a split would spend half a narrow pane
          on syntax you did not open it for. The toggle goes back. */}
      {preview && isMarkdown ? (
        <MarkdownPreview
          content={tab.content}
          theme={theme}
          zoom={zoom}
          // A hit's whole point is that you can go and change it, so this both
          // leaves the rendering and asks the editor for that line. `gotoLine`
          // is the same channel a search hit and a diff row already use, and
          // the effect above clears it once it has been honoured.
          onEditSource={(line) => {
            setPreview(false);
            if (!id || index === null || !st) return;
            useStore.getState().patchExplorer(id, {
              open: st.open.map((t, i) => (i === index ? { ...t, gotoLine: line } : t)),
            });
          }}
        />
      ) : (
      <div className="min-h-0 flex-1">
        <Editor
          path={`${tab.path}@${tab.rev ?? "worktree"}`}
          language={languageOf(tab.path)}
          value={tab.content}
          onMount={onMount}
          theme={monacoTheme(theme)}
          options={{
            readOnly: true,
            // Says why, rather than just refusing the keystroke.
            readOnlyMessage: { value: "mogeung reads; it never writes a worktree file. Pillar K." },
            domReadOnly: true,
            fontSize: 12 * zoom,
            fontFamily: "var(--font-mono)",
            lineNumbers: "on",
            minimap: { enabled: true, renderCharacters: false },
            wordWrap: wrap ? "on" : "off",
            scrollBeyondLastLine: false,
            renderWhitespace: "selection",
            smoothScrolling: true,
            stickyScroll: { enabled: true },
            bracketPairColorization: { enabled: true },
            occurrencesHighlight: "singleFile",
            folding: true,
            contextmenu: true,
            automaticLayout: true,
            // The margin the bookmark gesture lives in. Off by default in
            // Monaco, and a click target that is not drawn is not a gesture.
            glyphMargin: true,
          }}
        />
      </div>
      )}

      {outlineOpen && (
        <div className="flex w-56 shrink-0 flex-col border-l border-[var(--border)] bg-[var(--bg-panel)]">
          <div className="shrink-0 p-1">
            <Input value={outlineFilter} onChange={setOutlineFilter} placeholder="go to symbol…" />
          </div>
          <div className="min-h-0 flex-1 overflow-auto pb-2">
            {symbols.length === 0 ? (
              <Dim className="block px-2 py-1 text-2xs">
                no symbols recognised in this file
              </Dim>
            ) : (
              symbols
                .filter(
                  (sym) =>
                    !outlineFilter.trim() ||
                    sym.name.toLowerCase().includes(outlineFilter.trim().toLowerCase()),
                )
                .map((sym, i) => (
                  <div
                    key={`${sym.line}:${i}`}
                    onClick={() => {
                      editorRef.current?.revealLineInCenter(sym.line);
                      editorRef.current?.setPosition({ lineNumber: sym.line, column: 1 });
                      editorRef.current?.focus();
                    }}
                    title={`line ${sym.line}`}
                    className="cursor-default overflow-hidden px-2 py-px font-mono text-2xs whitespace-nowrap hover:bg-[var(--bg-faint)]"
                    style={{ paddingLeft: 8 + sym.depth * 10 }}
                  >
                    <span className="text-[var(--dim)]">{symbolGlyph(sym.kind)}</span> {sym.name}
                  </div>
                ))
            )}
          </div>
        </div>
      )}
      </div>
    </div>
  );
}

/**
 * The pane dockview renders for a `file:` panel. `R-B53`.
 *
 * It reads *which* file out of its own panel id rather than being handed props,
 * because that is the only thing dockview persists about a panel and the only
 * thing that survives a drag into another group.
 */
export function FilePane() {
  const paneId = usePaneId();
  const ref = paneId ? parseFilePaneId(paneId) : null;

  useEffect(() => {
    if (ref) explorerFetch(ref.session);
  });

  if (!ref) return <Empty>no file</Empty>;
  return <Viewer session={ref.session} path={ref.path} rev={ref.rev} />;
}
