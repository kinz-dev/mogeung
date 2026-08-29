/**
 * Ask for a shell command in words. `R-O12`, `A41`.
 *
 * **This is the half of `R-O12` that survived its own harness.** The row was
 * filed as completion from the corpus of commands your agents ran, and
 * `--bin judge --complete` measured that before anything was drawn: **0 of 57**
 * held-out commands predicted, against a shell history's 22, because 11,043 of
 * 11,359 agent commands are distinct. Prediction is repetition and there was
 * none. So there is no ghost text here and no ranked corpus list — there would
 * be nothing honest to put in either.
 *
 * What is here is what was asked for second: *"I just want to ask a quick
 * question in the terminal and let ai write the command"*.
 *
 * Three properties, and each is a refusal:
 *
 * | | |
 * | --- | --- |
 * | **your line is never read** | a prefix you are part-way through typing can carry `export TOKEN=…`. Only the sentence you deliberately wrote is sent |
 * | **it writes; running it is a second, different keypress** | Enter puts the text in the line and stops. <kbd>Alt</kbd>+<kbd>Enter</kbd> puts it in *and* sends the newline, asked for on 2026-08-29 — see below |
 * | **a drafted command says it is drafted** | *written by qwen · never run here* is not decoration: the hazard of this feature is a plausible line one keypress from a real shell |
 *
 * **On <kbd>Alt</kbd>+<kbd>Enter</kbd>, and which fence it is not.** This box
 * writes into **your own shell**, which
 * [ADR-0011](../../../docs/decisions/0011-own-a-shell-never-an-agent.md) says
 * is yours — mogeung never starts an agent in one. ADR-0003 and its amendment
 * are about text reaching an **agent's** input, and nothing here does: running
 * `ls` in your shell on a chord you pressed is the same act as typing it, with
 * the typing done for you. So the fence that moved is this module's own
 * sentence, and it moved by request rather than by drift, which is why it is
 * rewritten above instead of quietly deleted.
 *
 * It stays two keys, not one: plain Enter still only writes, the command is on
 * screen before either key can be pressed, and a line marked as destructive is
 * marked before you choose. What is deliberately **not** built is a refusal for
 * those — a pattern list is not a security boundary, and gating a key on one
 * would be the sometimes-right guard
 * [pillar K](../../../docs/product/roadmap.md#k-explicitly-not) forbids: it
 * would teach you that an unmarked command is safe.
 */

import { useEffect, useRef, useState } from "react";
import { CornerDownLeft, Wand2, X } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Mono } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { interactive } from "@/ui/styles";
import { looksDestructive } from "@/lib/command";

export function CommandBox({
  repo,
  onAccept,
}: {
  /** The shell's working directory, for the ask. */
  repo: string | null;
  /** Put this in the terminal's line. `run` also sends the newline. */
  onAccept: (command: string, run: boolean) => void;
}) {
  const open = useStore((s) => s.showCommandBox);
  const draft = useStore((s) => s.commandDraft);
  const askForCommand = useStore((s) => s.askForCommand);
  const model = useStore((s) => s.health?.model ?? null);
  const [question, setQuestion] = useState("");
  const box = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) box.current?.focus();
  }, [open]);

  if (!open) return null;

  const close = () => useStore.setState({ showCommandBox: false, commandDraft: null });
  const usable = !!model && model.configured && model.allowed && model.chat_allowed;
  const secs = draft?.pending && draft.started ? Math.floor((Date.now() - draft.started) / 1000) : 0;

  return (
    <div className="border-b border-[var(--border)] bg-[var(--bg)] px-2 py-1">
      <div className="flex items-center gap-1">
        <Wand2 size={11} className="shrink-0 text-[var(--dim)]" />
        <input
          ref={box}
          value={question}
          disabled={!usable}
          spellCheck={false}
          aria-label="ask for a command"
          placeholder={
            usable
              ? "what do you want to do?  e.g. grep xyz and sort by the first column"
              : (model?.refusal ?? "no model configured")
          }
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              close();
              return;
            }
            if (e.key !== "Enter") return;
            e.preventDefault();
            // Enter asks while there is no answer, and accepts once there is.
            // The same key for both, because the second press is the natural
            // continuation of the first and a separate accept key is a thing to
            // learn for no reason.
            if (draft && !draft.pending && draft.command) {
              // `Alt` runs it as well. Two keys rather than one, and the
              // command has been on screen since before either was pressed.
              onAccept(draft.command, e.altKey);
              close();
              return;
            }
            askForCommand(question, repo, "bash");
          }}
          className="h-5 min-w-0 flex-1 rounded-sm border border-[var(--border)] bg-[var(--bg-panel)] px-1 text-2xs outline-none focus:border-[var(--ring)] disabled:opacity-50"
        />
        <button
          type="button"
          title="close (Esc)"
          onClick={close}
          className={cn(interactive, "rounded-sm p-0.5 text-[var(--dim)]")}
        >
          <X size={11} />
        </button>
      </div>

      {draft?.pending && (
        <Dim className="mt-0.5 block text-2xs italic">
          writing it…{secs > 0 ? ` ${secs}s` : ""}
        </Dim>
      )}
      {draft?.error && (
        <div className="mt-0.5 text-2xs text-[var(--red)]">{draft.error}</div>
      )}
      {draft && !draft.pending && !draft.error && !draft.command && (
        <Dim className="mt-0.5 block text-2xs">
          the model would not write one command for that — ask for something smaller, or
          write it yourself
        </Dim>
      )}
      {draft && !draft.pending && draft.command && (
        <div className="mt-1">
          <div className="flex items-center gap-2">
            <Mono className="min-w-0 flex-1 truncate text-xs text-[var(--text-strong)]">
              {draft.command}
            </Mono>
            <button
              type="button"
              onClick={() => {
                onAccept(draft.command, false);
                close();
              }}
              className={cn(
                interactive,
                "flex shrink-0 items-center gap-1 rounded-sm border border-[var(--border)] px-1 text-2xs",
              )}
            >
              <CornerDownLeft size={10} /> put it in the line
            </button>
          </div>
          {/*
            Never *"safe"* — this is a rendering decision, not a check. A list
            of patterns is not a security boundary, and the reason to mark
            anything at all is that the hazard of this feature is a
            plausible-looking line one keypress away from a real shell.
          */}
          {looksDestructive(draft.command) && (
            <div className="mt-0.5 text-2xs text-[var(--amber)]">
              this one deletes, overwrites or elevates — read it before you press Enter
            </div>
          )}
          <Dim className="mt-0.5 block text-2xs">
            written by {draft.model || "the model"} · never run here · Enter puts it in the
            line · Alt+Enter puts it in and runs it
          </Dim>
        </div>
      )}
    </div>
  );
}
