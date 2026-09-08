/**
 * A scratch file, open and writable. `R-L5`, ADR-0035.
 *
 * The only pane in this window where Monaco is not read-only, and the reason
 * that is allowed is in `scratch.ts`: the pane holds a **name**, the daemon
 * holds the directory, and nothing here can name a path.
 *
 * **Saved as you type**, the way IntelliJ's scratch files are. Every edit
 * restarts a short timer; when it fires, the whole body goes to the daemon in
 * one `scratch_write`. No dirty flag to manage and nothing to lose on close:
 * the pane flushes whatever is pending when it unmounts, and `Ctrl+S` flushes
 * now for the hand that cannot leave a file unsaved. The tab's status says
 * which of the two states you are in, because a save with no visible effect
 * is one you press twice.
 *
 * **Uncontrolled on purpose.** `FilePane` hands Monaco `value` and lets the
 * library `setValue` on every reload, which is right for a viewer that must
 * follow the file on disk. An editor that did that would put the caret at the
 * top every time the daemon echoed a save. So the body goes in once, as
 * `defaultValue`, and the daemon's copy is never pushed back over the one
 * being typed into. A file edited outside mogeung while its pane is open is
 * therefore overwritten by the next keystroke here — stated rather than
 * hidden, and the same rule every editor with autosave has.
 */

import Editor, { type OnMount } from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { useEffect, useRef, useState } from "react";
import { useStore } from "@/store";
import { usePaneId } from "@/lib/paneScope";
import { fetchScratch, forgetScratch, parseScratchPaneId, scratchPath } from "@/lib/scratch";
import { languageOf } from "@/lib/explorer";
import { defineMogeungThemes, monacoTheme } from "@/lib/monaco-theme";
import { Dim, Empty, Mono } from "@/ui/primitives";

/** How long after the last keystroke the body goes to the daemon. */
export const SAVE_DELAY_MS = 400;

export function ScratchPane() {
  const paneId = usePaneId();
  const name = paneId ? parseScratchPaneId(paneId) : null;
  if (!name) return <Empty>no scratch file</Empty>;
  return <Scratch name={name} />;
}

function Scratch({ name }: { name: string }) {
  const file = useStore((s) => s.scratch.files[name]);
  const send = useStore((s) => s.send);
  const theme = useStore((s) => s.prefs.theme);
  // The Code pane's factor, so a scratch file reads at the size files do —
  // its own key first, for a hand that wants them different.
  const zoom = useStore((s) => s.prefs.zoom["scratch"] ?? s.prefs.zoom["file"] ?? 1);
  const editorRef = useRef<editor.IStandaloneCodeEditor | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // What the daemon has been sent, so an acknowledged save can be told apart
  // from one still in flight, and a flush with nothing new sends nothing.
  const sent = useRef<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    fetchScratch(name);
    return () => forgetScratch(name);
  }, [name]);

  const flush = () => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    const body = editorRef.current?.getValue();
    if (body === undefined || body === sent.current) return;
    sent.current = body;
    send({ cmd: "scratch_write", name, content: body });
  };

  const onChange = () => {
    setPending(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(flush, SAVE_DELAY_MS);
  };

  // Whatever is pending goes when the pane closes. The ref is read at
  // unmount, so this cannot capture a stale `flush`.
  const flushRef = useRef(flush);
  flushRef.current = flush;
  useEffect(() => () => flushRef.current(), []);

  // The daemon's acknowledgement clears *pending* only if nothing has been
  // typed since the acknowledged body left.
  const saved = file?.saved ?? 0;
  useEffect(() => {
    if (saved > 0 && editorRef.current?.getValue() === sent.current) setPending(false);
  }, [saved]);

  const onMount: OnMount = (ed, monaco) => {
    editorRef.current = ed;
    sent.current = ed.getValue();
    defineMogeungThemes(monaco);
    monaco.editor.setTheme(monacoTheme(theme));
    // `Ctrl+S` saves now. Not a chord the keymap knows: it belongs to the
    // editor, and it must not reach the pty when a terminal has focus.
    ed.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, flush);
    ed.focus();
  };

  if (!file || file.content === null) return <Empty>loading {name}…</Empty>;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center gap-2 border-b border-[var(--border)] px-2 py-1 text-xs">
        <Mono className="text-xs">{scratchPath(name)}</Mono>
        <Dim className="text-2xs">{languageOf(name)}</Dim>
        <span className="flex-1" />
        <span className="text-2xs text-[var(--dim)]" data-testid="scratch-status">
          {pending ? "unsaved — saving as you type" : "saved"}
        </span>
      </div>
      {/*
        **This editor is writable, and the keymap has to know.** `focusOwns`
        gives a read-only Monaco only the navigation keys, so every other bare
        binding still reaches the window — right for a viewer, and wrong the
        moment you can type. Marked on the container rather than sniffed from
        Monaco's internals: it is the pane that knows whether it is an editor,
        and a `closest()` on a data attribute cannot be broken by a library
        upgrade. `R-L5`, and the check `keymap.ts` said would be needed.
      */}
      <div className="min-h-0 flex-1" data-editor="writable">
        <Editor
          path={`scratch/${name}`}
          language={languageOf(name)}
          defaultValue={file.content}
          onChange={onChange}
          onMount={onMount}
          theme={monacoTheme(theme)}
          options={{
            readOnly: false,
            fontSize: 12 * zoom,
            fontFamily: "var(--font-mono)",
            lineNumbers: "on",
            minimap: { enabled: false },
            wordWrap: "off",
            scrollBeyondLastLine: false,
            renderWhitespace: "selection",
            smoothScrolling: true,
            bracketPairColorization: { enabled: true },
            folding: true,
            contextmenu: true,
            automaticLayout: true,
          }}
        />
      </div>
    </div>
  );
}
