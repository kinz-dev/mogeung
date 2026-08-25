/**
 * One pty, rendered. `R-M3`.
 *
 * xterm.js in front, a pty held by the Tauri process behind. The pane holds a
 * **view** of a tmux session rather than the session itself, which is what
 * keeps [ADR-0010](../../docs/decisions/0010-attach-a-terminal-never-own-one.md)
 * and [ADR-0011](../../docs/decisions/0011-own-a-shell-never-an-agent.md) true:
 * unmounting detaches, and whatever was running is still running.
 */

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Terminal as Xterm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { isTauri, onPtyClosed, onPtyData, ptyClose, ptyOpen, ptyResize, ptyWrite } from "@/lib/tauri";
import { clipboardIntent, decodeOsc52, readClipboard, writeClipboard } from "@/lib/clipboard";
import { usePaneVisible } from "@/lib/paneScope";
import { ContextMenu, MenuItem, MenuLabel } from "@/ui/Menu";

import { useStore } from "@/store";

/**
 * Shift+Enter must insert a newline rather than submit the turn.
 *
 * A terminal has no way to express Shift+Enter — Return is one byte and the
 * shift is lost before the pty sees it — so Claude Code's `/terminal-setup`
 * works around it by having the *terminal* send a different byte. This value
 * was read from a real iTerm2 mapping rather than guessed: `\n`, `\x1b\r` and
 * `\x1b[13;2u` all look equally plausible and cost a round trip each to try.
 */
const SHIFT_ENTER = "\n";

/**
 * The CSS zoom in force above this pane, or 1.
 *
 * A terminal maps a pointer to a cell by measuring in device pixels, and a CSS
 * `zoom` on an ancestor leaves that measurement in a different space from the
 * mouse coordinates — the selection then lands further off the further you
 * drag. `applyAppZoom` uses the *webview's* zoom now precisely so this returns
 * 1, but it falls back to CSS when the capability is missing, and this is what
 * makes that fallback survivable rather than silently broken.
 */
function ambientCssZoom(): number {
  const raw = getComputedStyle(document.documentElement).getPropertyValue("zoom").trim();
  const n = Number.parseFloat(raw);
  return Number.isFinite(n) && n > 0 ? n : 1;
}

/**
 * Closes still in flight, by pty id.
 *
 * `ptyClose` and `ptyOpen` are separate async commands on a multithreaded
 * runtime, so a hide's close can land *after* the reveal's open for the same
 * id — killing the fresh pty and leaving a visible pane dead until the next
 * flap. Rare on unmount; `R-J58` made the sequence as common as a tab
 * switch, and dockview can flap visibility during layout restore. Module
 * scope, because a StrictMode remount replaces the component instance and a
 * ref would forget the close it still has to wait for.
 */
const ptyClosing = new Map<string, Promise<unknown>>();

/** mogeung's palette, as xterm wants it. */
function themeFor(dark: boolean) {
  const v = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return {
    // Darker than the pane it sits in, on purpose and by request: a terminal
    // is a window onto another machine's output, and giving it the same
    // surface as the chrome around it makes the two read as one thing. The
    // depth is what says *this part is not the app talking*.
    background: v("--terminal-bg") || v("--bg-panel") || (dark ? "#0e0f12" : "#f7f8fa"),
    foreground: v("--text") || (dark ? "#d4d7dd" : "#25282e"),
    cursor: v("--blue"),
    selectionBackground: v("--selection-bg"),
    black: dark ? "#141518" : "#e8eaee",
    red: v("--red"),
    green: v("--green"),
    yellow: v("--amber"),
    blue: v("--blue"),
    magenta: v("--purple"),
    cyan: v("--graph-4"),
    white: v("--text"),
    brightBlack: v("--dim"),
  };
}

export interface TerminalProps {
  /** Stable across re-renders: it is the pty's identity on the Rust side. */
  id: string;
  /** Full argv. `null` means "there is nothing to run", and it says why. */
  command: string[] | null;
  cwd?: string | null;
  /** Shown instead of the terminal when `command` is null. */
  refusal?: React.ReactNode;
}

export function TerminalView({ id, command, cwd, refusal }: TerminalProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  /** The geometry the pty was last told about, so a no-op resize stays a no-op. */
  const lastSize = useRef<{ cols: number; rows: number } | null>(null);
  /** The live instance, so font and theme can be changed without a teardown. */
  const termRef = useRef<Xterm | null>(null);
  const resyncRef = useRef<() => void>(() => {});
  const fontPx = useStore((s) => s.prefs.terminalFontPx);
  const theme = useStore((s) => s.prefs.theme);
  const appZoom = useStore((s) => s.prefs.appZoom);
  const pushError = useStore((s) => s.pushError);
  const [exited, setExited] = useState(false);
  /** xterm's selection, read when the menu opens — it lives in xterm, not the DOM. */
  const [selection, setSelection] = useState("");
  // Read from the document rather than from the factor, because what matters
  // is whether a CSS zoom is *in force* — with the webview zoom in use it is
  // not, whatever the stored factor says. Recomputed when the factor changes,
  // which is the only thing that can turn one on.
  const undoCssZoom = useMemo(() => {
    const z = ambientCssZoom();
    return Math.abs(z - 1) < 0.001 ? undefined : ({ zoom: 1 / z } as CSSProperties);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [appZoom]);

  // A hidden pane holds no pty. `R-J58`. A background Agent tab is a live
  // `tmux attach`, and a busy TUI redraws continuously — every frame crossed
  // the pty, the batching emitter and the IPC bridge to be parsed by an xterm
  // nobody could see, per hidden pane, for as long as the tab stayed behind.
  // Hiding tears the attach down (tmux keeps the session — ADR-0010's rule,
  // same as unmounting); revealing re-attaches and tmux redraws the screen
  // whole. What the trade costs is this view's own scrollback across a
  // hide/reveal — tmux copy-mode still holds the real history.
  const visible = usePaneVisible();

  useEffect(() => {
    if (!command || !visible || !hostRef.current || !isTauri()) return;
    setExited(false);

    const dark =
      theme === "dark" || (theme === "system" && !window.matchMedia("(prefers-color-scheme: light)").matches);

    const term = new Xterm({
      fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim() || "monospace",
      fontSize: fontPx,
      theme: themeFor(dark),
      cursorBlink: true,
      // Generous, because tmux copy-mode is how you read back a long build and
      // the pane is where you would otherwise have to leave to do it.
      scrollback: 10_000,
      allowProposedApi: true,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(hostRef.current);
    termRef.current = term;
    fit.fit();

    // Everything typed goes to the pty untouched. A terminal that swallows
    // shortcuts is a terminal you cannot run a terminal program in — and the
    // one thing this pane exists for is answering a prompt Claude Code is
    // showing.
    const typed = term.onData((data) => void ptyWrite(id, data, "xterm").catch(() => {}));
    const keys = term.attachCustomKeyEventHandler((e) => {
      if (e.type === "keydown" && e.key === "Enter" && e.shiftKey) {
        // **`preventDefault` is not decoration here.** Returning `false` makes
        // xterm return from `_keyDown` *before* it calls `preventDefault`, so
        // the browser's own default action still runs — and the default action
        // for Shift+Enter, with focus in the hidden `<textarea>` xterm types
        // through, is to insert a line break into it. Every other branch below
        // already prevents its default; this one did not, and a stray `\n` in
        // that textarea is the leading suspect for the newline that appears in
        // the agent's prompt without anyone typing it.
        e.preventDefault();
        void ptyWrite(id, SHIFT_ENTER, "shift-enter").catch(() => {});
        return false;
      }
      /**
       * `Alt+Escape` hands the keyboard back to the window. `R-B51`.
       *
       * Caught **here**, inside xterm's own handler, rather than left to the
       * keymap — because this is the one binding that has to work when the
       * terminal is winning. Everything else in `ACTIONS` is a chord, and
       * chords already fire from a focused pane (the keymap listens in
       * capture); what does not work, correctly, is every **bare** key, because
       * `focusOwns` gives those to whatever has focus. That is the right rule —
       * a terminal must receive `j` — and it is also why there has to be a way
       * out that is not the mouse.
       *
       * Returning `false` keeps it off the pty. An Escape-bearing chord would
       * otherwise reach a TUI as `ESC`, which Claude Code reads as a cancel, so
       * a release that also forwarded the keystroke would interrupt the agent
       * on its way past. Plain `Escape` is untouched and still cancels — that
       * is the one that had to keep working.
       *
       * The chord itself lives in `focus.ts`, deliberately: it is named in the
       * keymap too, and two literals would drift silently.
       */
      /**
       * **Anything the window already claimed does not also reach the pty.**
       *
       * The keymap listens on `window` in the capture phase and calls
       * `preventDefault` on every chord it handles, so by the time xterm's
       * handler runs, `defaultPrevented` *is* the answer to "was this the
       * app's?". Reading it beats re-listing the chords here, for two reasons
       * that both cost something before this existed:
       *
       * **It fixed a double-fire.** `R-B52`'s move was matched here *and* in
       * the keymap, so one press moved two panes — reported as focus flipping
       * between the outer two panes and never landing on the middle one. The
       * action belongs to the keymap; this side's whole job is to keep the
       * keystroke off the pty.
       *
       * **It survives a rebind.** A hard-coded list would answer to the shipped
       * chord after the user had changed it, which is the silent kind of wrong.
       *
       * And it closes a bug nobody had reported yet: every chord — `Alt+2`,
       * `Alt+A` — was being handled by the window *and* forwarded to the agent
       * as an `ESC`-prefixed sequence.
       */
      if (e.type === "keydown" && e.defaultPrevented) return false;
      // Copy and paste, which xterm implements neither of. See `clipboard.ts`
      // for why the paste chords are split and why `Ctrl+C` is not among them.
      const intent = clipboardIntent(e);
      if (intent === "copy") {
        const selection = term.getSelection();
        // Swallowed even when there is nothing to copy: `Ctrl+Shift+C` would
        // otherwise reach the pty as a bare `^C` and interrupt whatever is
        // running, which is a violent answer to a copy with no selection.
        e.preventDefault();
        if (selection) {
          void writeClipboard(selection).catch((err) => pushError(`could not copy: ${String(err)}`));
        }
        return false;
      }
      if (intent === "paste-read") {
        e.preventDefault();
        void readClipboard()
          // `term.paste` rather than `ptyWrite`: bracketed-paste mode is what
          // tells the TUI on the other end that this arrived at once rather
          // than being typed, and it is xterm that knows whether it is on.
          .then((text) => text && term.paste(text))
          .catch((err) => pushError(`could not paste: ${String(err)} — try Ctrl+V`));
        return false;
      }
      // `paste-native`: decline it *without* preventing the default, so the
      // webview's own paste runs and xterm forwards what lands in its textarea.
      if (intent === "paste-native") return false;
      return true;
    });
    void keys;

    // `OSC 52` — the program's own route to the clipboard, which xterm.js
    // leaves unimplemented. See `decodeOsc52` for why this is the hop that was
    // missing and why a *read* is refused rather than answered.
    term.parser.registerOscHandler(52, (data) => {
      const req = decodeOsc52(data);
      if (!req) return false;
      if (req.kind === "read") return true;
      void writeClipboard(req.text).catch((err) =>
        pushError(`the session tried to copy and could not: ${String(err)}`),
      );
      return true;
    });

    let unlistenData: (() => void) | null = null;
    let unlistenClosed: (() => void) | null = null;
    let disposed = false;

    void (async () => {
      try {
        // A close for this id may still be in flight from the hide (or
        // unmount) that preceded this open; opening before it lands lets it
        // kill the pty we are about to create. Wait it out first.
        const prior = ptyClosing.get(id);
        if (prior) {
          await prior;
          ptyClosing.delete(id);
        }
        if (disposed) return;
        // Every `await` here is a window in which the cleanup below may already
        // have run — React's StrictMode mounts, unmounts and remounts an effect
        // in development, and the unmount happens while these promises are
        // still in flight. Registering a listener *after* cleanup leaves it
        // attached for ever, and the remount adds a second one: every byte from
        // the pty is then written twice, which is why one keystroke appeared as
        // `cc`. So each resolution re-checks and undoes itself.
        const un = await onPtyData((who, data) => {
          if (who === id) term.write(data);
        });
        if (disposed) {
          un();
          return;
        }
        unlistenData = un;

        const unClosed = await onPtyClosed((who) => {
          if (who === id) setExited(true);
        });
        if (disposed) {
          unClosed();
          return;
        }
        unlistenClosed = unClosed;

        if (disposed) return;
        // Fit once more immediately before opening. Between mount and here we
        // have awaited twice, and the pane has usually finished laying out in
        // that time — opening with the pre-layout size gives the pty one
        // geometry and the TUI another, which is what makes a cursor land in
        // the wrong column and appear to jump.
        safeFit();
        await ptyOpen(id, command, cwd ?? null, term.cols, term.rows);
        lastSize.current = { cols: term.cols, rows: term.rows };
        // Opened after the teardown: close it, or the pty and its reader thread
        // outlive the pane that asked for them.
        if (disposed) ptyClosing.set(id, ptyClose(id).catch(() => {}));
      } catch (e) {
        pushError(`could not open the terminal: ${String(e)}`);
      }
    })();

    /**
     * Re-fit, but only when there is something to fit to.
     *
     * `fit()` on a zero-sized element computes a nonsense geometry, and xterm
     * happily adopts it — which is how a pane that is briefly hidden comes back
     * with its columns wrong.
     */
    function safeFit() {
      const el = hostRef.current;
      if (!el || el.clientWidth < 8 || el.clientHeight < 8) return;
      try {
        fit.fit();
      } catch {
        /* a fit during teardown is not worth reporting */
      }
    }

    /**
     * Tell the pty only when the geometry actually changed.
     *
     * `fit()` writes to the DOM, which fires `ResizeObserver` again — so an
     * observer that re-fits and re-sends unconditionally is a feedback loop.
     * Every lap re-sends `SIGWINCH`, Claude Code's TUI redraws from scratch,
     * and the cursor walks forward and back while you type. Comparing against
     * the last size sent breaks the loop at its only stable point.
     */
    resyncRef.current = syncSize;

    function syncSize() {
      safeFit();
      const { cols, rows } = term;
      const last = lastSize.current;
      if (last && last.cols === cols && last.rows === rows) return;
      lastSize.current = { cols, rows };
      void ptyResize(id, cols, rows).catch(() => {});
    }

    // Coalesced to one per frame: a drag across the splitter fires the observer
    // dozens of times, and the pty only needs the size you stopped at.
    let pending = 0;
    /**
     * Ctrl+wheel over a terminal changes its **font**, not the pane's CSS zoom.
     *
     * A terminal measures a character cell in device pixels to decide how many
     * columns it has; scaling it with CSS `zoom` leaves that measurement fighting
     * the transform and the grid drifts out of alignment. The egui client kept a
     * separate terminal font size for exactly this reason, and this keeps that
     * split. Propagation is stopped so the enclosing `ZoomPane` does not also
     * scale what has already been handled.
     */
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      e.stopPropagation();
      const { prefs, setPrefs } = useStore.getState();
      const next = Math.min(28, Math.max(8, prefs.terminalFontPx + (e.deltaY < 0 ? 1 : -1)));
      if (next !== prefs.terminalFontPx) setPrefs({ terminalFontPx: next });
    };
    // **Capture**, and this is the same lesson as the keyboard one layer up:
    // xterm attaches its own wheel handler to scroll the buffer, and it stops
    // propagation for the ones it handles — so a listener waiting on the way
    // *up* never sees Ctrl+wheel at all. Capturing on the host div, which is an
    // ancestor of everything xterm builds, means this runs first.
    hostRef.current.addEventListener("wheel", onWheel, { passive: false, capture: true });

    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(pending);
      pending = requestAnimationFrame(syncSize);
    });
    ro.observe(hostRef.current);

    return () => {
      disposed = true;
      cancelAnimationFrame(pending);
      hostRef.current?.removeEventListener("wheel", onWheel, { capture: true });
      ro.disconnect();
      typed.dispose();
      unlistenData?.();
      unlistenClosed?.();
      // Detaches. tmux keeps the session; this only drops our view of it.
      // Tracked, so the next open of this id waits for it to land.
      ptyClosing.set(id, ptyClose(id).catch(() => {}));
      term.dispose();
      termRef.current = null;
    };
    // `command` is an array rebuilt each render; join it so a genuinely
    // unchanged command does not tear the pty down every time.
    //
    // **`fontPx` and `theme` are deliberately absent.** They were here, and
    // that made Ctrl+wheel over the pane detach and re-attach tmux once per
    // notch: a font size is a property of the view, and tearing down the pty to
    // change one throws away the session in order to restyle the window. Both
    // are applied to the live terminal below instead.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id, command?.join(" "), cwd, visible]);

  // Font and theme, changed in place. A resync follows because the column count
  // is a function of the cell size — a bigger font is fewer columns, and the
  // pty has to be told or the TUI draws to a width the terminal no longer has.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.fontSize = fontPx;
    const dark =
      theme === "dark" ||
      (theme === "system" && !window.matchMedia("(prefers-color-scheme: light)").matches);
    term.options.theme = themeFor(dark);
    resyncRef.current();
  }, [fontPx, theme]);

  if (!isTauri()) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 p-6 text-center">
        <div className="text-sm text-[var(--dim)]">the terminal needs the desktop build</div>
        <div className="text-xs text-[var(--dim)] opacity-70">
          a browser tab has no pty to hold — run <code className="font-mono">npm run tauri dev</code>
        </div>
      </div>
    );
  }

  if (!command) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 p-6 text-center">
        {refusal ?? <div className="text-sm text-[var(--dim)]">nothing to attach to</div>}
      </div>
    );
  }

  // `tabIndex` and `data-focus-host`: where the keyboard lands when you let go
  // of the terminal (`R-B51`). Focusable without joining the tab order, and an
  // **ancestor** of the terminal rather than a sibling — which is the part that
  // matters, because `focusOwns` asks `closest(".xterm")`, so focus parked here
  // reads as outside the terminal and every bare binding starts working again.
  // Somewhere neutral, deliberately: the alternative was throwing focus at the
  // queue, which would silently rebind `j`/`k` to moving between sessions the
  // moment you escaped a pane.
  return (
    <div
      className="relative h-full min-h-0 bg-[var(--terminal-bg)] outline-none"
      style={undoCssZoom}
      tabIndex={-1}
      data-focus-host
    >
      {/* **The webview's own right-click menu is useless over a terminal.**
          xterm draws its selection itself rather than making a DOM one, so the
          browser has nothing to copy and offers a greyed-out `Copy` — reported
          2026-08-05 as *"after select text I can't right-click copy"*. This
          menu reads the selection from xterm, where it actually lives. */}
      <ContextMenu
        onOpenChange={(open) => open && setSelection(termRef.current?.getSelection() ?? "")}
        trigger={
          // `data-owns-zoom`: this subtree handles its own Ctrl+wheel, and the
          // enclosing `ZoomPane` must keep its hands off. See the note by the
          // wheel handler above — a CSS-scaled terminal selects the wrong text.
          //
          // The `zoom` above undoes an *ancestor's* CSS zoom, which is a
          // different hole and the one that actually bit: a pane wrapper can be
          // told to keep its hands off, and the document root cannot.
          <div ref={hostRef} data-owns-zoom className="h-full w-full" />
        }
      >
        <MenuItem
          disabled={!selection}
          onSelect={() => {
            if (!selection) return;
            void writeClipboard(selection).catch((err) => pushError(`could not copy: ${String(err)}`));
          }}
        >
          Copy <span className="text-[var(--dim)]">Ctrl+Shift+C</span>
        </MenuItem>
        <MenuItem
          onSelect={() =>
            void readClipboard()
              .then((text) => text && termRef.current?.paste(text))
              .catch((err) => pushError(`could not paste: ${String(err)} — try Ctrl+V`))
          }
        >
          Paste <span className="text-[var(--dim)]">Ctrl+V</span>
        </MenuItem>
        {!selection && (
          // The gesture is not obvious and the failure is silent: Claude Code
          // turns mouse reporting on, so a plain drag belongs to *it* and never
          // reaches xterm's selection. Shift is what takes the mouse back.
          <MenuLabel>hold Shift while dragging — the session owns a plain drag</MenuLabel>
        )}
      </ContextMenu>
      {exited && (
        <div className="absolute inset-x-0 bottom-0 bg-[var(--bg-raised)] px-2 py-1 text-2xs text-[var(--dim)]">
          the terminal exited — tmux keeps the session, so reopening this pane re-attaches
        </div>
      )}
    </div>
  );
}
