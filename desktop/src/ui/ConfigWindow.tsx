/**
 * `~/.mogeung/config.toml`, shown and editable. `R-J79`.
 *
 * Asked for 2026-08-28, an hour after the config file turned out to be the
 * only way to reach a setting the window needed — the model endpoint's
 * consent, which a window hosting its own daemon has no argv to receive
 * ([ADR-0031](../../../docs/decisions/0031-consent-to-a-named-host.md)). A
 * setting that can only be changed by leaving the application is a setting
 * most people will not change.
 *
 * **The whole file, as text.** Not a generated form. The file has comments in
 * it — the shipped ones explain what each key costs — and a form would either
 * drop them on the first save or have to become a TOML formatter. Text also
 * means this window cannot fall behind the daemon: a key added in Rust appears
 * in the list below without anyone editing this file, because the daemon sends
 * the keys it actually understands.
 *
 * **The daemon validates, not this.** A file that does not parse is never
 * written, and the complaint comes back from the same parser that will read it
 * at start-up — so *it saved and then did not work* is not a state that exists.
 * `deny_unknown_fields` means a mistyped key is caught here too rather than
 * ignored for ever, which is the failure a config file is worst at reporting.
 *
 * **What takes effect, said plainly.** The daemon reads this file at start-up.
 * The three model keys are re-applied on save, because those are the three the
 * window's hosted daemon reads and the reason this window exists; everything
 * else needs a restart. A dialog that implied otherwise would be worse than
 * one that says nothing.
 *
 * **Read anywhere, edited on loopback only.** The file holds `push_url` and
 * `model_url`, both outbound, so a daemon a second machine can reconfigure is
 * a daemon a second machine can aim. Same shape as the chat refusal and, for
 * the same reason, no flag.
 */

import { useEffect, useState } from "react";
import { RotateCcw, Save } from "lucide-react";
import { useStore } from "@/store";
import { Dialog } from "@/ui/Dialog";
import { Button, Dim, Mono } from "@/ui/primitives";

/** How long "saved" stays on screen. Long enough to read, short enough not to
 *  become a label that is always there and therefore never read. */
const SAVED_FOR_MS = 4000;

export function ConfigWindow() {
  const open = useStore((s) => s.showConfig);
  const config = useStore((s) => s.config);
  const send = useStore((s) => s.send);
  const [draft, setDraft] = useState<string | null>(null);
  const [freshlySaved, setFreshlySaved] = useState(false);

  // Ask on open, every time. The file is editable in a terminal too, and a
  // dialog showing what it said the last time you looked is the kind of stale
  // that gets saved back over somebody's change.
  useEffect(() => {
    if (open) {
      setDraft(null);
      setFreshlySaved(false);
      send({ cmd: "config_get" });
    }
  }, [open, send]);

  // The daemon's text becomes the draft once, when it arrives — and again
  // after a save, because a save re-reads the file and that is what is now
  // true. Typing is not interrupted: `draft` is only replaced when it is null
  // or when the answer is a confirmed save.
  const savedAt = config?.savedAt ?? null;
  useEffect(() => {
    if (!config) return;
    setDraft((d) => (d === null ? config.text : d));
  }, [config]);
  useEffect(() => {
    if (savedAt === null) return;
    setDraft(config?.text ?? "");
    setFreshlySaved(true);
    const t = setTimeout(() => setFreshlySaved(false), SAVED_FOR_MS);
    return () => clearTimeout(t);
    // `config?.text` deliberately absent: this runs on a *save*, not on every
    // message that happens to carry text.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [savedAt]);

  if (!open) return null;
  const close = () => useStore.setState({ showConfig: false });

  const readonly = config?.readonly ?? null;
  const text = draft ?? "";
  const dirty = config !== null && draft !== null && draft !== config.text;

  return (
    <Dialog
      title="Configuration"
      subtitle={config ? <Mono>{config.path}</Mono> : "asking the daemon…"}
      onClose={close}
      wide
    >
      <div className="flex min-h-[24rem] w-[46rem] max-w-full flex-col">
        <Dim className="block text-2xs">
          The daemon reads this at start-up. The model keys are re-applied when you save;
          everything else needs the daemon restarted.
        </Dim>

        {readonly && (
          <div className="mt-2 rounded border border-[var(--amber)] px-2 py-1 text-2xs text-[var(--amber)]">
            {readonly}
          </div>
        )}

        <textarea
          value={text}
          onChange={(e) => setDraft(e.target.value)}
          readOnly={readonly !== null}
          spellCheck={false}
          autoFocus
          placeholder={"# nothing configured yet — every key is optional\n"}
          className="mt-2 min-h-[16rem] flex-1 resize-none rounded border border-[var(--line)] bg-[var(--bg)] p-2 font-mono text-xs outline-none focus-visible:border-[var(--ring)]"
        />

        {/* The complaint from the parser that will read this at start-up, not
            from a second one written for the editor — so a file that saves is
            a file that loads. */}
        {config?.error && (
          <div className="mt-2 rounded border border-[var(--red)] px-2 py-1 font-mono text-2xs whitespace-pre-wrap text-[var(--red)]">
            {config.error}
          </div>
        )}

        {/* From the daemon's own struct, so this cannot drift from what it
            understands — and it is the only discoverable list there is, since
            `deny_unknown_fields` means a guess is an error rather than a
            setting that quietly does nothing. */}
        {config && config.keys.length > 0 && (
          <div className="mt-2">
            <Dim className="block text-2xs">keys this daemon understands</Dim>
            <div className="mt-0.5 flex flex-wrap gap-x-2 gap-y-0.5">
              {config.keys.map((k) => (
                <Mono key={k} className="text-2xs opacity-70">
                  {k}
                </Mono>
              ))}
            </div>
          </div>
        )}

        <div className="mt-3 flex items-center gap-2">
          <Button
            disabled={!config || readonly !== null || !dirty}
            onClick={() => send({ cmd: "config_save", text })}
          >
            <Save className="mr-1 h-3.5 w-3.5" />
            Save
          </Button>
          <Button
            disabled={!dirty}
            onClick={() => setDraft(config?.text ?? "")}
            title="throw away what you typed and show the file again"
          >
            <RotateCcw className="mr-1 h-3.5 w-3.5" />
            Revert
          </Button>
          {freshlySaved ? (
            <Dim className="text-2xs text-[var(--green)]">saved — a copy of the previous file is beside it as config.toml.bak</Dim>
          ) : dirty ? (
            <Dim className="text-2xs">unsaved changes</Dim>
          ) : null}
        </div>
      </div>
    </Dialog>
  );
}
