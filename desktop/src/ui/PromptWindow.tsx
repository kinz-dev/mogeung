/**
 * The follow-up prompt window. `R-B15`'s shape, ADR-0003's rule.
 *
 * **mogeung writes it. You paste it.** There is no send button and there is no
 * wire message behind one: writing into a session's terminal is steering, and
 * steering is what made v0.1 worse than the terminal it wrapped. The clipboard
 * is the widest part of this pipe on purpose, because a human is on the far end
 * deciding whether the text is right.
 *
 * **Since `R-O7` a model can draft it**, and the boundary above is unchanged —
 * [ADR-0034](../../../docs/decisions/0034-the-draft-is-a-chat-ask.md) records
 * what that costs and what it does not. Three properties are held on purpose:
 *
 * | | |
 * | --- | --- |
 * | **the raw concatenation is one click away** | a draft that drops something has to be catchable, and the only way to catch it is to read what it was drafted from |
 * | **the draft is asked for, never automatic** | opening this window must not spend a model call, ADR-0031 clause 6 |
 * | **still exactly one action: copy** | drafting composes text in this window; only the clipboard leaves it |
 */

import { useEffect, useState } from "react";
import { ClipboardCopy, SendHorizontal, Wand2, X } from "lucide-react";
import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, IconButton, Input, Mono } from "@/ui/primitives";
import { buildPrompt } from "@/lib/prompt";
import { sessionLabel } from "@/wire/types";

export function PromptWindow() {
  const open = useStore((s) => s.showPrompt);
  const flagged = useStore((s) => s.flagged);
  const draft = useStore((s) => s.promptDraft);
  const draftFollowUp = useStore((s) => s.draftFollowUp);
  const health = useStore((s) => s.health);
  const [note, setNote] = useState("");
  const [copied, setCopied] = useState(false);
  /**
   * Which text the preview is showing.
   *
   * Raw until a draft is asked for, because that is what exists — and it stays
   * the thing one click away afterwards, which is the row's own requirement:
   * what the draft dropped is only inspectable against what it was drafting
   * from.
   */
  const [view, setView] = useState<"raw" | "drafted">("raw");
  /**
   * The confirmation, and the reason it exists. `R-B54`,
   * [ADR-0003's amendment](../../../docs/decisions/0003-observe-do-not-spawn.md).
   *
   * mogeung cannot see what the session's screen is showing — a TUI's prompts
   * never reach the transcript — so an Enter sent on your behalf can land on a
   * menu. Clause 1 makes sending two deliberate acts, and this is the second
   * one. It names the session and shows what will be sent, which is what it can
   * honestly tell you; it cannot tell you what is on the screen.
   */
  const [confirming, setConfirming] = useState(false);
  const sessions = useStore((s) => s.sessions);

  /**
   * The seconds a slow draft owes an explanation for. `R-O11`'s lesson, moved.
   *
   * A local model can reason for half a minute before it writes a word, and a
   * motionless "drafting…" for half a minute reads as a hang rather than as
   * work. The interval runs only while something is out, so a window sitting
   * open re-renders never.
   */
  const [now, setNow] = useState(() => Date.now());
  const pending = draft?.pending ?? false;
  useEffect(() => {
    if (!pending) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [pending]);

  if (!open) return null;
  const close = () => useStore.setState({ showPrompt: false });
  const raw = buildPrompt(note, flagged);

  /**
   * Who this would be sent to. `R-B54`, ADR-0003's amendment, clause 2 and clause 5.
   *
   * One session or none: flags spanning two sessions have an **ambiguous
   * recipient**, and a message with an ambiguous recipient is one the clipboard
   * should carry. And a session with no tmux pane cannot be aimed at at all —
   * ADR-0010's boundary, reused rather than a new one.
   */
  const targets = new Set(flagged.map((f) => f.sessionId));
  const target = targets.size === 1 ? sessions[[...targets][0]] : undefined;
  const canSend = !!target?.tmux_target;
  const whyNotSend =
    flagged.length === 0
      ? "flag something first"
      : targets.size > 1
        ? `these flags come from ${targets.size} sessions — copy it, and paste it where you mean it to go`
        : !target
          ? "that session is not on this daemon any more"
          : "that session is not running under tmux, so there is no pane to send to — start sessions with `yolomo`, or copy and paste it yourself";

  const model = health?.model ?? null;
  // Three different silences, and only one of them is *no*: a daemon that
  // predates pillar O sends no row at all, which must not read as a refusal.
  const usable = !!model && model.configured && model.allowed && model.chat_allowed;
  const why = model
    ? (model.refusal ?? "no model configured")
    : "this daemon does not know about models — it predates pillar O";

  // What copy copies is what you are looking at. Anything else is a window
  // that puts one thing on screen and another on the clipboard.
  const showingDraft = view === "drafted" && !!draft && !draft.error;
  const text = showingDraft ? draft.text : raw;
  const secs = draft?.started ? Math.floor((now - draft.started) / 1000) : 0;

  return (
    <Dialog
      title="Follow-up prompt"
      subtitle="mogeung writes this — you paste it into that session's terminal"
      onClose={close}
      wide
    >
      <div className="min-w-[34rem]">
        <Dim className="block text-2xs">
          Nothing is sent to any session. That would be steering, which is exactly what
          ADR-0003 rules out — the queue tells you who needs you, and the conversation stays
          yours to drive.
        </Dim>

        <div className="mt-2">
          <Dim className="mb-0.5 block text-2xs">what you want done</Dim>
          <Input
            value={note}
            onChange={setNote}
            placeholder="e.g. these three need error handling before I merge"
          />
        </div>

        <Dim className="mt-2 block text-2xs">{flagged.length} flagged hunk(s)</Dim>
        <div className="max-h-40 overflow-y-auto">
          {flagged.map((f, i) => (
            <div key={`${f.path}:${f.header}:${i}`} className="flex items-start gap-1 border-b border-[var(--border)] py-0.5">
              <div className="min-w-0 flex-1">
                <Mono className="block truncate text-2xs text-[var(--dim)]">
                  {f.path} {f.header}
                </Mono>
                <Input
                  value={f.note}
                  ariaLabel={`note for ${f.path}`}
                  placeholder="a note about this one (optional)"
                  onChange={(v) =>
                    useStore.setState({
                      flagged: flagged.map((x, j) => (j === i ? { ...x, note: v } : x)),
                    })
                  }
                />
              </div>
              <IconButton
                title="unflag"
                onClick={() =>
                  useStore.setState({ flagged: flagged.filter((_, j) => j !== i) })
                }
              >
                <X size={11} />
              </IconButton>
            </div>
          ))}
        </div>

        <div className="mt-2 flex items-center gap-1">
          <Dim className="text-2xs">preview</Dim>
          {/*
            Only once there is a second thing to look at. Before that a toggle
            offering "drafted" would be a control that reports a draft exists
            when none does.
          */}
          {draft && (
            <div className="flex items-center gap-1">
              <Button
                size="sm"
                active={view === "drafted"}
                disabled={!!draft.error}
                title={draft.error ? "the draft failed — the raw text is what there is" : "the model's instruction"}
                onClick={() => setView("drafted")}
              >
                drafted
              </Button>
              <Button
                size="sm"
                active={view === "raw"}
                title="what the draft was written from, unchanged — so what it dropped is visible"
                onClick={() => setView("raw")}
              >
                raw
              </Button>
            </div>
          )}
          {view === "drafted" && draft && !draft.pending && !draft.error && (
            <Dim className="ml-auto text-2xs">
              {draft.model}
              {draft.elapsed_ms ? ` · ${(draft.elapsed_ms / 1000).toFixed(1)}s` : ""}
            </Dim>
          )}
        </div>

        {/*
          The failure is shown where the draft would have been rather than
          instead of the window: what you came here for is the raw text, and
          it is still underneath.
        */}
        {draft?.error && (
          <div className="mt-1 rounded-sm border border-[var(--red)] px-2 py-1 text-2xs text-[var(--red)]">
            {draft.error}
          </div>
        )}

        <pre className="max-h-48 overflow-auto rounded-sm border border-[var(--border)] bg-[var(--bg)] p-1 font-mono text-2xs whitespace-pre-wrap">
          {showingDraft && draft.pending && !draft.text
            ? `drafting…${secs > 0 ? ` ${secs}s` : ""}`
            : text}
        </pre>

        {confirming && target && (
          <div className="mt-2 rounded-sm border border-[var(--amber)] px-2 py-1">
            <div className="text-2xs">
              Send to <b>{sessionLabel(target)}</b> in <Mono>{target.tmux_target}</Mono>, and press
              Enter?
            </div>
            <Mono className="mt-0.5 block truncate text-2xs text-[var(--dim)]">
              {text.trim().split("\n")[0]}
            </Mono>
            {/*
              Said plainly rather than buried in an ADR nobody reads at the
              moment of pressing: this is the one thing the confirmation cannot
              check for you.
            */}
            <Dim className="mt-0.5 block text-2xs">
              mogeung cannot see that session's screen — if a permission prompt is up, this
              Enter answers it
            </Dim>
            <div className="mt-1 flex items-center gap-1">
              <Button
                variant="solid"
                onClick={() => {
                  useStore.getState().send({
                    cmd: "send_to_session",
                    session_id: target.id,
                    // Exactly what is on screen — ADR-0003's amendment, clause 2. The same
                    // text the copy button would put on the clipboard.
                    text,
                  });
                  setConfirming(false);
                }}
              >
                send it
              </Button>
              <Button variant="outline" onClick={() => setConfirming(false)}>
                cancel
              </Button>
            </div>
          </div>
        )}

        <div className="mt-2 flex items-center gap-1">
          <Button
            variant="outline"
            onClick={() => {
              void navigator.clipboard?.writeText(text);
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1500);
            }}
          >
            <ClipboardCopy size={11} /> {copied ? "copied" : "copy to clipboard"}
          </Button>
          {/*
            Asked for, never automatic — ADR-0031 clause 6 keeps model work off
            anything that happens on its own, and opening this window is
            something that happens every time you flag a hunk.
          */}
          <Button
            variant="outline"
            disabled={!usable || flagged.length === 0 || draft?.pending}
            title={
              usable
                ? "compose these into one instruction  (R-O7)"
                : why
            }
            onClick={() => {
              setView("drafted");
              draftFollowUp(note);
            }}
          >
            <Wand2 size={11} /> {draft?.pending ? "drafting…" : "draft with the model"}
          </Button>
          {/*
            The one control in mogeung that reaches an agent's input. Two acts,
            never one: this opens the confirmation, and the confirmation sends.
            ADR-0003's amendment, clause 1 — and clause 7, which is why *draft* and *send*
            are separate buttons with the text on screen in between.
          */}
          <Button
            variant="outline"
            disabled={!canSend}
            title={canSend ? `send it to ${sessionLabel(target!)} and press Enter` : whyNotSend}
            onClick={() => setConfirming(true)}
          >
            <SendHorizontal size={11} /> send to session
          </Button>
          <Button
            variant="outline"
            onClick={() => {
              useStore.setState({ flagged: [], promptDraft: null });
              setNote("");
              setView("raw");
              close();
            }}
          >
            clear flags
          </Button>
          <Dim className="text-2xs">then paste it into that session's terminal</Dim>
        </div>
      </div>
    </Dialog>
  );
}
