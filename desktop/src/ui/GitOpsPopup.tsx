/**
 * The Git Operations popup. `R-D31`, on `Alt+` `.
 *
 * IntelliJ's *VCS Operations Popup*, over the verbs mogeung has: a numbered
 * list you run with a digit. The list itself is [`lib/gitOps.ts`](../lib/gitOps.ts)
 * — this file is the keyboard, the drawing, and the one confirmation the list
 * can ask for.
 *
 * **A digit is the whole point.** The arrows and `Enter` work, and are the
 * fallback; what makes a popup like this worth a key at all is that `Alt+` `
 * then `1` is two keystrokes with no reading in between, the way `Alt+9`, a
 * tab, a row and a click is not. So the digits are drawn in a column of their
 * own, and an entry that cannot run keeps its number — a list that renumbered
 * itself as files were selected would make the muscle memory a lie.
 *
 * **An unavailable entry still answers.** Pressing its digit says why, in the
 * notices, rather than doing nothing: a popup that ignores a key you pressed
 * is indistinguishable from one that has crashed.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useStore } from "@/store";
import { GIT_OPS, opsContext, type GitOp, type OpAsk } from "@/lib/gitOps";
import { bindingsFor, formatChord, ACTIONS } from "@/lib/keymap";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Kbd } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { interactive, rowSelected } from "@/ui/styles";

/** The chord this window is actually listening for, for an action id. */
function chordFor(id: string | undefined, overrides: Record<string, string[]>): string | null {
  if (!id) return null;
  const action = ACTIONS.find((a) => a.id === id);
  const keys = action ? bindingsFor(action, overrides)[0] : null;
  return keys ? formatChord(keys) : null;
}

export function GitOpsPopup() {
  const open = useStore((s) => s.gitPopup === "ops");
  const overrides = useStore((s) => s.prefs.keymap);
  const [cursor, setCursor] = useState(0);
  const [confirm, setConfirm] = useState<Extract<OpAsk, { ask: "confirm" }> | null>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Read **once per opening**, deliberately: see `OpsContext`. A list whose
  // numbering moved while it was on screen would be a list you cannot learn.
  const ctx = useMemo(() => (open ? opsContext() : null), [open]);

  useEffect(() => {
    if (!open) {
      setConfirm(null);
      return;
    }
    setCursor(0);
    // The list itself takes the keyboard: there is no input in this popup, and
    // a digit that reached the queue behind it would move the selection.
    listRef.current?.focus();
  }, [open]);

  if (!open || !ctx) return null;

  const close = () => useStore.setState({ gitPopup: null });

  const perform = (op: GitOp) => {
    const why = op.unavailable(ctx);
    if (why) {
      // Said rather than swallowed. The popup stays open, because the answer
      // to "no file is selected" is often the next entry down.
      useStore.getState().pushError(`${op.label} — ${why}`);
      return;
    }
    const asked = op.run(ctx);
    if (asked?.ask === "branches") {
      useStore.setState({ gitPopup: "branches" });
      return;
    }
    if (asked?.ask === "confirm") {
      setConfirm(asked);
      return;
    }
    close();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (confirm) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const d = e.key === "ArrowDown" ? 1 : -1;
      setCursor((c) => (c + d + GIT_OPS.length) % GIT_OPS.length);
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      perform(GIT_OPS[cursor]);
      return;
    }
    // The digits, and only the digits: a letter is not a shortcut here, so it
    // falls through to the browser rather than being eaten silently.
    const op = GIT_OPS.find((o) => o.digit === e.key);
    if (op) {
      e.preventDefault();
      perform(op);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[14vh]" onClick={close}>
      {/* Focused on mount by the effect above, so the first digit lands here
          rather than in whatever had the keyboard when the chord fired. */}
      <div
        ref={listRef}
        role="menu"
        aria-label="Git operations"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
        className="w-[420px] max-w-[92vw] overflow-hidden rounded-md border border-[var(--window-stroke)] bg-[var(--bg-raised)] py-1 shadow-[var(--elev-3)] outline-none"
      >
        <div className="px-3 pb-1 text-center text-xs font-semibold text-[var(--text-strong)]">
          Git Operations
        </div>
        <Dim className="block px-3 pb-1 text-2xs tracking-wider uppercase">Git</Dim>

        {GIT_OPS.map((op, i) => {
          const why = op.unavailable(ctx);
          const chord = chordFor(op.action, overrides);
          const first = op.digit === null && GIT_OPS[i - 1]?.digit !== null;
          return (
            <div key={op.id}>
              {first && <div className="my-1 border-t border-[var(--border)]" />}
              <button
                type="button"
                role="menuitem"
                aria-disabled={!!why}
                title={why ?? undefined}
                onMouseEnter={() => setCursor(i)}
                onClick={() => perform(op)}
                className={cn(
                  interactive,
                  "flex w-full items-center gap-2 px-3 py-[3px] text-left text-sm",
                  i === cursor && rowSelected,
                  why && "text-[var(--dim)]",
                )}
              >
                <span className="w-3 shrink-0 text-center text-xs tabular-nums text-[var(--dim)]">
                  {op.digit ?? ""}
                </span>
                <span className="flex-1 truncate">{op.label}</span>
                {op.note && <Dim className="truncate text-2xs">{op.note}</Dim>}
                {chord && <Kbd>{chord}</Kbd>}
              </button>
            </div>
          );
        })}
      </div>

      {confirm && (
        <Dialog title={confirm.title} subtitle="git keeps no copy — it cannot bring this back" onClose={() => setConfirm(null)}>
          <div className="flex flex-col gap-3 px-3 py-3" onClick={(e) => e.stopPropagation()}>
            <Dim className="text-xs whitespace-pre-wrap">{confirm.body}</Dim>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                className="border-[var(--red)] text-[var(--red)]"
                onClick={() => {
                  confirm.then();
                  setConfirm(null);
                  close();
                }}
              >
                Roll it back
              </Button>
              <Button variant="outline" onClick={() => setConfirm(null)}>
                Keep it
              </Button>
            </div>
          </div>
        </Dialog>
      )}
    </div>
  );
}
