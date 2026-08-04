/**
 * The follow-up prompt window. `R-B15`'s shape, ADR-0003's rule.
 *
 * **mogeung writes it. You paste it.** There is no send button and there is no
 * wire message behind one: writing into a session's terminal is steering, and
 * steering is what made v0.1 worse than the terminal it wrapped. The clipboard
 * is the widest part of this pipe on purpose, because a human is on the far end
 * deciding whether the text is right.
 */

import { useState } from "react";
import { ClipboardCopy, X } from "lucide-react";
import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, IconButton, Input, Mono } from "@/ui/primitives";
import { buildPrompt } from "@/lib/prompt";

export function PromptWindow() {
  const open = useStore((s) => s.showPrompt);
  const flagged = useStore((s) => s.flagged);
  const [note, setNote] = useState("");
  const [copied, setCopied] = useState(false);

  if (!open) return null;
  const close = () => useStore.setState({ showPrompt: false });
  const text = buildPrompt(note, flagged);

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

        <Dim className="mt-2 block text-2xs">preview</Dim>
        <pre className="max-h-48 overflow-auto rounded-sm border border-[var(--border)] bg-[var(--bg)] p-1 font-mono text-2xs whitespace-pre-wrap">
          {text}
        </pre>

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
          <Button
            variant="outline"
            onClick={() => {
              useStore.setState({ flagged: [] });
              setNote("");
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
