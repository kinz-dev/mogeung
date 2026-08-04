/**
 * The session's worktree, read. `R-B24`, workbench behaviour by `R-B25`.
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
 * forward. What is left here is the half that is genuinely about the file you
 * are reading.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { Pin, X, WrapText, Columns2, ListTree, Eye, UserRound, Bookmark } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Empty, IconButton } from "@/ui/primitives";
import { closeTab, explorerFetch, languageOf } from "@/lib/explorer";
import { cn } from "@/lib/cn";
import { base } from "@/lib/format";
import { defineMogeungThemes, monacoTheme } from "@/lib/monaco-theme";
import { outline, symbolGlyph } from "@/lib/symbols";
import { Input } from "@/ui/primitives";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { FileIcon } from "@/ui/FileIcon";

function TabStrip({ group }: { group: 0 | 1 }) {
  const id = useStore((s) => s.selected);
  const st = useStore((s) => (s.selected ? s.explorer[s.selected] : undefined));
  const patchExplorer = useStore((s) => s.patchExplorer);
  if (!id || !st) return null;

  const tabs = st.open.map((t, i) => ({ t, i })).filter(({ t }) => t.group === group);
  if (tabs.length === 0) return null;

  return (
    <div className="flex h-7 shrink-0 items-center overflow-x-auto border-b border-[var(--border)]">
      {tabs.map(({ t, i }) => (
        <div
          key={`${t.path}@${t.rev ?? ""}`}
          onClick={() => {
            const active = [...st.active] as [number | null, number | null];
            active[group] = i;
            patchExplorer(id, { active, focus: group });
          }}
          onDoubleClick={() => {
            // Double click pins — the preview tab stops being reused.
            patchExplorer(id, { open: st.open.map((x, j) => (j === i ? { ...x, pinned: true } : x)) });
          }}
          onAuxClick={(e) => {
            if (e.button === 1) closeTab(id, i);
          }}
          title={t.rev ? `${t.path} @ ${t.rev}` : t.path}
          className={cn(
            "group flex h-full shrink-0 cursor-default items-center gap-1 border-r border-[var(--border)] px-2 text-xs",
            st.active[group] === i
              ? "bg-[var(--bg-panel)] text-[var(--text-strong)] shadow-[inset_0_2px_0_var(--blue)]"
              : "text-[var(--dim)] hover:bg-[var(--bg-faint)]",
            !t.pinned && "italic",
          )}
        >
          {t.pinned && <Pin size={9} className="opacity-60" />}
          <FileIcon name={base(t.path)} size={11} className="shrink-0" />
          <span>{base(t.path)}</span>
          {t.rev && <span className="text-2xs text-[var(--amber)]">@{t.rev.slice(0, 7)}</span>}
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              closeTab(id, i);
            }}
            className="outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2 transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)] opacity-0 group-hover:opacity-100 hover:text-[var(--text-strong)]"
          >
            <X size={10} />
          </button>
        </div>
      ))}
    </div>
  );
}

function Viewer({ group }: { group: 0 | 1 }) {
  const id = useStore((s) => s.selected);
  const st = useStore((s) => (s.selected ? s.explorer[s.selected] : undefined));
  const theme = useStore((s) => s.prefs.theme);
  const wrapPaths = useStore((s) => s.scoped().editorWrap);
  const setScoped = useStore((s) => s.setScoped);
  const zoom = useStore((s) => s.prefs.zoom["code"] ?? 1);
  const send = useStore((s) => s.send);
  const scoped = useStore((s) => s.scoped());
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Parameters<OnMount>[1] | null>(null);
  const decorations = useRef<editor.IEditorDecorationsCollection | null>(null);
  const [outlineOpen, setOutlineOpen] = useState(false);
  const [outlineFilter, setOutlineFilter] = useState("");
  const [preview, setPreview] = useState(false);
  const [blameOn, setBlameOn] = useState(false);

  const index = st?.active[group] ?? null;
  const tab = index !== null && st ? st.open[index] : null;

  // Go to the line a search hit or a diff row asked for, once, when the body
  // is actually there. Doing it on every render would fight the scrollbar.
  useEffect(() => {
    if (!tab?.gotoLine || !tab.content || !editorRef.current || !id || index === null || !st) return;
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

  if (!tab) {
    return (
      <Empty hint="Alt+4 for the worktree · Ctrl+P to open by name">
        nothing open — read-only, always
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
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-6 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2">
        <Dim className="truncate text-2xs">{tab.path}</Dim>
        {tab.truncated && (
          <span className="shrink-0 text-2xs text-[var(--amber)]" title="the file went past the size cap">
            head only
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
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
      </div>
      <div className="flex min-h-0 flex-1">
      {/* Markdown is *read* here, not previewed beside its source: a viewer's
          job is the rendered thing, and a split would spend half a narrow pane
          on syntax you did not open it for. The toggle goes back. */}
      {preview && isMarkdown ? (
        <div className="min-h-0 flex-1 overflow-auto px-3 py-2">
          <div className="prose-mogeung">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{tab.content}</ReactMarkdown>
          </div>
        </div>
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

export function CodePane() {
  const id = useStore((s) => s.selected);
  const st = useStore((s) => (s.selected ? s.explorer[s.selected] : undefined));
  const patchExplorer = useStore((s) => s.patchExplorer);

  useEffect(() => {
    if (id) explorerFetch(id);
  });

  if (!id) return <Empty>select a session</Empty>;

  const split = !!st?.open.some((t) => t.group === 1);

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--bg-panel)]">
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <TabStrip group={0} />
          <Viewer group={0} />
        </div>
        {split && (
          <div className="flex min-w-0 flex-1 flex-col border-l border-[var(--border)]">
            <TabStrip group={1} />
            <Viewer group={1} />
          </div>
        )}
      </div>
      {st && st.open.length > 0 && (
        <div className="flex h-5 shrink-0 items-center gap-2 border-t border-[var(--border)] px-2">
          <IconButton
            title="send the active file to the other side"
            onClick={() => {
              const i = st.active[st.focus];
              if (i === null) return;
              const open = st.open.map((t, j) => (j === i ? { ...t, group: (t.group === 0 ? 1 : 0) as 0 | 1, pinned: true } : t));
              patchExplorer(id, { open, active: [null, null] });
            }}
          >
            <Columns2 size={11} />
          </IconButton>
          <Dim className="text-2xs">{st.open.length} open</Dim>
        </div>
      )}
    </div>
  );
}
