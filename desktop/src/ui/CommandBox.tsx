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
 * | **it writes; running it is a second, different keypress** | on an **empty** box, Enter puts the text in the line and stops; <kbd>Alt</kbd>+<kbd>Enter</kbd> also sends the newline. Type something and Enter asks instead — see *refining* |
 * | **a drafted command says it is drafted** | *written by qwen · never run here* is not decoration: the hazard of this feature is a plausible line one keypress from a real shell |
 *
 * **Refining, and why one key does both.** With a command on screen, whatever
 * you type is a change to it — *without the pipe*, *use ripgrep* — because the
 * second ask of a session almost never is a different question. So **what is in
 * the box decides**: text means ask, empty means take what is there. That rule
 * was a bug first — Enter accepted the old command instead of asking for the
 * change just typed, which would have made the feature unusable for the case it
 * was built for. <kbd>Esc</kbd> on an empty box starts fresh; <kbd>Esc</kbd>
 * with text just closes.
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
import { CornerDownLeft, HelpCircle, Wand2, X } from "lucide-react";
import { useStore } from "@/store";
import { Dim, Mono } from "@/ui/primitives";
import { cn } from "@/lib/cn";
import { interactive } from "@/ui/styles";
import { looksDestructive, placeholderAt } from "@/lib/command";

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
  const explainCommand = useStore((s) => s.explainCommand);
  const history = useStore((s) => s.commandHistory);
  const [question, setQuestion] = useState("");
  /** How far back up-arrow has walked. `-1` is the live box. */
  const [back, setBack] = useState(-1);
  const box = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) box.current?.focus();
  }, [open]);

  if (!open) return null;

  // Closing keeps the draft: reopening shows what you last got, which is what
  // *"remember what you asked"* meant in practice. Esc on an **empty** box is
  // the one that clears, so leaving and starting fresh are still two different
  // gestures.
  const close = (forget = false) =>
    useStore.setState({ showCommandBox: false, ...(forget ? { commandDraft: null } : {}) });
  const usable = !!model && model.configured && model.allowed && model.chat_allowed;
  const secs = draft?.pending && draft.started ? Math.floor((Date.now() - draft.started) / 1000) : 0;
  // A command the model itself marked incomplete. `Alt+Enter` will not run one:
  // that is not a safety judgement about the command, it is the command saying
  // it is not finished.
  const hasPlaceholder = !!draft?.command && placeholderAt(draft.command) !== null;

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
            !usable
              ? (model?.refusal ?? "no model configured")
              : draft?.command
                ? "change it — e.g. without the pipe, or use ripgrep  (Esc to start fresh)"
                : "what do you want to do?  e.g. grep xyz and sort by the first column"
          }
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              close(question.trim().length === 0);
              return;
            }
            // Up and down walk what you have asked before — the second ask is
            // usually a variation on the first, and retyping it was the point
            // of this.
            if (e.key === "ArrowUp" || e.key === "ArrowDown") {
              if (history.length === 0) return;
              e.preventDefault();
              const next =
                e.key === "ArrowUp"
                  ? Math.min(back + 1, history.length - 1)
                  : Math.max(back - 1, -1);
              setBack(next);
              setQuestion(next < 0 ? "" : history[history.length - 1 - next]);
              return;
            }
            if (e.key !== "Enter") return;
            e.preventDefault();
            // **What is in the box decides.** Text in it means you want that:
            // a first question, or a change to the command already there. An
            // empty box means you want what is on screen, so Enter takes it.
            //
            // Found by writing the refine test: with an answer showing, Enter
            // accepted the old command instead of asking for the change that
            // had just been typed — the feature would have been unusable for
            // the exact case it was built for.
            if (question.trim().length === 0 && draft && !draft.pending && draft.command) {
              // `Alt` runs it as well. Two keys rather than one, and the
              // command has been on screen since before either was pressed —
              // except where it carries a placeholder, which is the model
              // saying the command is unfinished rather than mogeung judging
              // it unsafe.
              onAccept(draft.command, e.altKey && !hasPlaceholder);
              close(true);
              return;
            }
            askForCommand(question, repo, "bash");
            setQuestion("");
            setBack(-1);
          }}
          className="h-5 min-w-0 flex-1 rounded-sm border border-[var(--border)] bg-[var(--bg-panel)] px-1 text-2xs outline-none focus:border-[var(--ring)] disabled:opacity-50"
        />
        <button
          type="button"
          title="close (Esc)"
          onClick={() => close(true)}
          className={cn(interactive, "rounded-sm p-0.5 text-[var(--dim)]")}
        >
          <X size={11} />
        </button>
      </div>

      {draft?.pending && (
        <Dim className="mt-0.5 block text-2xs italic">
          {draft.refined ? "revising it" : "writing it"}…{secs > 0 ? ` ${secs}s` : ""}
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
                close(true);
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
          {/*
            The explanation, asked for rather than volunteered — a second model
            call, and a line of prose under every answer would be paid on every
            ask by everyone who already reads shell.
          */}
          {draft.explainPending && (
            <Dim className="mt-0.5 block text-2xs italic">reading it…</Dim>
          )}
          {draft.explanation && (
            <Dim className="mt-0.5 block text-2xs">{draft.explanation}</Dim>
          )}
          <div className="mt-0.5 flex items-center gap-2">
            <Dim className="text-2xs">
              written by {draft.model || "the model"} · never run here · Enter puts it in the
              line{hasPlaceholder ? " (cursor on the placeholder)" : ""} ·{" "}
              {hasPlaceholder ? "fill it in before running" : "Alt+Enter puts it in and runs it"}
            </Dim>
            {!draft.explanation && !draft.explainPending && (
              <button
                type="button"
                title="what does this command do?"
                onClick={() => explainCommand("bash")}
                className={cn(interactive, "ml-auto flex items-center gap-1 rounded-sm px-1 text-2xs text-[var(--dim)]")}
              >
                <HelpCircle size={10} /> what does it do?
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
