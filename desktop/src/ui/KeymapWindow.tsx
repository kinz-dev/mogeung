/**
 * Every binding, by name — and rebindable. `R-B12`.
 *
 * `A13` is `SUPPORTED`: the bindings are how this is driven, so a binding you
 * cannot discover is one you do not use, and a binding you cannot *change* is
 * one you work around. Both halves matter.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { RotateCcw, X } from "lucide-react";
import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Empty, IconButton, Input, Kbd } from "@/ui/primitives";
import { ACTIONS, bindingsFor, chordFromEvent, rebind } from "@/lib/keymap";
import { scorePath } from "@/lib/search";
import { cn } from "@/lib/cn";

export function KeymapWindow() {
  const open = useStore((s) => s.showKeymap);
  const overrides = useStore((s) => s.prefs.keymap);
  const setPrefs = useStore((s) => s.setPrefs);
  const [q, setQ] = useState("");
  const [recording, setRecording] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  // Held in a ref so the capture effect can depend on the recorded action
  // alone. Depending on `overrides` would re-subscribe on every change, which
  // is the trap the dialog's focus effect already fell into once.
  const overridesRef = useRef(overrides);
  overridesRef.current = overrides;

  /**
   * While recording, this window owns **every** key.
   *
   * Capture *and* `stopImmediatePropagation`, because the whole point is to
   * rebind a chord that already means something: pressing `Ctrl+K` to rebind it
   * has to record `Ctrl+K`, not open the palette. Anything less makes exactly
   * the keys worth rebinding the ones you cannot.
   */
  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Escape") {
        setRecording(null);
        return;
      }
      const chord = chordFromEvent(e);
      // A bare modifier is not a chord — keep waiting for the key it precedes.
      if (!chord) return;
      const { next, stoleFrom } = rebind(overridesRef.current, recording, chord);
      setPrefs({ keymap: next });
      setNote(stoleFrom ? `${chord} was “${stoleFrom}” — reassigned, and that is now unbound` : null);
      setRecording(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, setPrefs]);

  const groups = useMemo(() => {
    const query = q.trim();
    const rows = query
      ? ACTIONS.map((a) => ({ a, s: scorePath(query, `${a.group} ${a.label}`) }))
          .filter((x): x is { a: (typeof ACTIONS)[number]; s: number } => x.s !== null)
          .sort((x, y) => y.s - x.s)
          .map((x) => x.a)
      : ACTIONS;
    const out = new Map<string, typeof ACTIONS>();
    for (const a of rows) out.set(a.group, [...(out.get(a.group) ?? []), a]);
    return [...out];
  }, [q]);

  if (!open) return null;

  const changed = Object.keys(overrides).length;

  return (
    <Dialog
      title="Keyboard"
      subtitle={`${ACTIONS.length} actions${changed ? ` · ${changed} changed` : ""}`}
      onClose={() => {
        setRecording(null);
        useStore.setState({ showKeymap: false });
      }}
      wide
    >
      <div className="sticky top-0 z-10 space-y-1 border-b border-[var(--border)] bg-[var(--bg-raised)] p-2">
        <Input value={q} onChange={setQ} placeholder="filter actions" autoFocus />
        <div className="flex items-center gap-2">
          <Dim className="text-2xs">
            {recording ? "press a chord — Escape cancels" : "click a key to rebind it"}
          </Dim>
          {changed > 0 && (
            <Button
              variant="outline"
              size="sm"
              className="ml-auto"
              title="put every binding back to the one it shipped with"
              onClick={() => {
                setPrefs({ keymap: {} });
                setNote(null);
              }}
            >
              reset all
            </Button>
          )}
        </div>
        {note && <div className="text-2xs text-[var(--amber)]">{note}</div>}
      </div>

      <div className="p-2">
        {groups.length === 0 ? (
          <Empty>nothing matches</Empty>
        ) : (
          groups.map(([group, actions]) => (
            <div key={group} className="mb-3">
              <div className="mb-1 text-2xs font-semibold tracking-wider text-[var(--dim)] uppercase">
                {group}
              </div>
              {actions.map((a) => {
                const keys = bindingsFor(a, overrides);
                const custom = a.id in overrides;
                return (
                  <div
                    key={a.id}
                    className="group flex items-center gap-3 rounded-sm px-2 py-1 text-sm hover:bg-[var(--state-hover)]"
                  >
                    <span className="min-w-0 flex-1 truncate">
                      {a.label}
                      {custom && (
                        <span className="ml-1 text-2xs text-[var(--amber)]" title="changed from the default">
                          ·
                        </span>
                      )}
                    </span>

                    <button
                      type="button"
                      onClick={() => {
                        setNote(null);
                        setRecording(a.id);
                      }}
                      title="click, then press the chord you want"
                      className={cn(
                        "flex shrink-0 items-center gap-1 rounded-sm px-1 py-0.5 outline-none",
                        "transition-colors duration-[var(--dur-fast)] ease-[var(--ease-standard)]",
                        "focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2",
                        recording === a.id
                          ? "bg-[var(--selection-bg)] text-[var(--text-strong)]"
                          : "hover:bg-[var(--state-focus)]",
                      )}
                    >
                      {recording === a.id ? (
                        <span className="animate-pulse text-2xs">press a chord…</span>
                      ) : keys.length === 0 ? (
                        <Dim className="text-2xs italic">unbound</Dim>
                      ) : (
                        keys.map((k) => <Kbd key={k}>{k}</Kbd>)
                      )}
                    </button>

                    <span className="flex w-12 shrink-0 justify-end gap-0.5">
                      {keys.length > 0 && (
                        <IconButton
                          title="unbind"
                          className="opacity-0 group-hover:opacity-100"
                          onClick={() => setPrefs({ keymap: { ...overrides, [a.id]: [] } })}
                        >
                          <X size={11} />
                        </IconButton>
                      )}
                      {custom && (
                        <IconButton
                          title={`back to ${a.keys.join(" / ") || "unbound"}`}
                          onClick={() => {
                            const next = { ...overrides };
                            delete next[a.id];
                            setPrefs({ keymap: next });
                          }}
                        >
                          <RotateCcw size={11} />
                        </IconButton>
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
          ))
        )}
        <Dim className="block px-2 text-2xs">
          Stored with your preferences. An old{" "}
          <code className="font-mono">~/.mogeung/keymap.json</code> is left alone — it belongs to the
          retired egui window and is keyed by its action names.
        </Dim>
      </div>
    </Dialog>
  );
}
