/**
 * Name a session yourself. `R-B26`.
 *
 * A derived title is whatever the agent last said; a label is what *you* decided
 * this session is for, and that is the thing you triage by. The badge and the
 * `label:` filter both follow it.
 *
 * Client-side and machine-scoped (`R-I11`): a session id from the dev box means
 * nothing on the laptop, so labels are keyed by the daemon's `machine_id`.
 *
 * Empty removes. That is the only way back, and it is worth being a rule rather
 * than a separate button: one field, Enter saves, blank clears.
 */

import { useEffect, useState } from "react";
import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Input } from "@/ui/primitives";
import { sessionLabel } from "@/wire/types";

export function LabelWindow() {
  const editing = useStore((s) => s.labelEditing);
  const scoped = useStore((s) => s.scoped());
  const setScoped = useStore((s) => s.setScoped);
  const session = useStore((s) => (s.labelEditing ? s.sessions[s.labelEditing] : null));
  const [text, setText] = useState("");

  // Pre-filled with what it is called now, so editing is the same gesture as
  // naming rather than a retype.
  useEffect(() => {
    if (editing) setText(scoped.labels[editing] ?? "");
  }, [editing, scoped.labels]);

  if (!editing) return null;

  const close = () => useStore.setState({ labelEditing: null });

  const save = () => {
    const value = text.trim();
    const labels = { ...scoped.labels };
    if (value) labels[editing] = value;
    else delete labels[editing];
    setScoped({ labels });
    close();
  };

  return (
    <Dialog
      title={scoped.labels[editing] ? "Edit label" : "Label this session"}
      subtitle={session ? sessionLabel(session) : undefined}
      onClose={close}
    >
      <div className="space-y-2 p-3">
        <Input
          value={text}
          onChange={setText}
          autoFocus
          placeholder="what is this session for?"
          ariaLabel="session label"
          onKeyDown={(e) => {
            if (e.key === "Enter") save();
            if (e.key === "Escape") close();
          }}
        />
        <Dim className="block text-2xs">
          Shown as a badge in the queue, and filterable as <code className="font-mono">label:…</code>.
          Leave it empty to remove.
        </Dim>
        <div className="flex justify-end gap-1">
          <Button variant="ghost" onClick={close}>
            cancel
          </Button>
          <Button variant="solid" onClick={save}>
            {text.trim() ? "save" : "remove"}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
