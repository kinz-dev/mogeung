/**
 * Every action by name, and every file by name. `R-B21`, `R-B25`.
 *
 * The one binding worth memorising, because it is the one that means you never
 * have to memorise another.
 *
 * It stays, and the rail's Search stays too — they do different jobs. This is
 * for typing and jumping; the panel is for reading results. IntelliJ ships both
 * for that reason, and so does this.
 */

import { useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { Command } from "cmdk";
import type { DockviewApi } from "dockview";
import { useStore } from "@/store";
import { ACTIONS, bindingsFor, formatChord } from "@/lib/keymap";
import { scorePath } from "@/lib/search";
import { openFile } from "@/lib/explorer";
import { Dim, Kbd, Mono } from "@/ui/primitives";
import { FileIcon } from "@/ui/FileIcon";
import { base, shortDir } from "@/lib/format";

export function Palette({ dock }: { dock: RefObject<DockviewApi | null> }) {
  const open = useStore((s) => s.paletteOpen);
  const mode = useStore((s) => s.paletteMode);
  const id = useStore((s) => s.selected);
  const tree = useStore((s) => (s.selected ? s.explorer[s.selected]?.treePaths : undefined));
  const send = useStore((s) => s.send);
  // The binding in force, not the one that shipped: the palette is where an
  // action is found, so it is where a rebind has to show — and on a Mac the
  // shipped `keys` are not the ones this window is listening for at all.
  const overrides = useStore((s) => s.prefs.keymap);
  const patchExplorer = useStore((s) => s.patchExplorer);
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // The list is re-requested each time it is opened: worktrees move under live
  // sessions, and a walk is cheap next to showing stale files.
  useEffect(() => {
    if (!open || mode !== "files" || !id) return;
    patchExplorer(id, { treePending: true });
    send({ cmd: "list_tree", session_id: id });
  }, [open, mode, id, send, patchExplorer]);

  useEffect(() => {
    if (open) setQuery("");
  }, [open, mode]);

  const files = useMemo(() => {
    if (mode !== "files" || !tree) return [];
    const q = query.trim();
    if (!q) return tree.slice(0, 60);
    const scored = tree
      .map((p) => ({ p, s: scorePath(q, p) }))
      .filter((x): x is { p: string; s: number } => x.s !== null);
    scored.sort((a, b) => b.s - a.s || a.p.localeCompare(b.p));
    return scored.slice(0, 100).map((x) => x.p);
  }, [mode, tree, query]);

  const close = () => useStore.setState({ paletteOpen: false });

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-[12vh]"
      onClick={close}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-[620px] max-w-[92vw] overflow-hidden rounded-md border border-[var(--window-stroke)] bg-[var(--bg-raised)] shadow-[var(--elev-3)]"
      >
        <Command shouldFilter={mode === "actions"} loop>
          <div className="flex items-center gap-2 border-b border-[var(--border)] px-3">
            <Dim className="text-2xs tracking-wider uppercase">{mode === "files" ? "file" : "action"}</Dim>
            <Command.Input
              ref={inputRef}
              autoFocus
              value={query}
              onValueChange={setQuery}
              placeholder={mode === "files" ? "open a file by name…" : "type an action…"}
              className="flex-1 bg-transparent py-2.5 text-base outline-none placeholder:text-[var(--dim)]"
              onKeyDown={(e) => {
                if (e.key === "Escape") close();
                // Tab flips between the two modes, so the wrong one is one key
                // to fix rather than a close-and-reopen.
                if (e.key === "Tab") {
                  e.preventDefault();
                  useStore.setState({ paletteMode: mode === "files" ? "actions" : "files" });
                }
              }}
            />
            <Dim className="text-2xs">Tab switches</Dim>
          </div>

          <Command.List className="max-h-[52vh] overflow-y-auto p-1">
            <Command.Empty className="px-3 py-6 text-center text-sm text-[var(--dim)]">
              {mode === "files" && !id
                ? "no session selected — a worktree belongs to one"
                : mode === "files" && !tree
                  ? "walking the worktree…"
                  : "nothing matches"}
            </Command.Empty>

            {mode === "actions" &&
              Object.entries(
                ACTIONS.reduce<Record<string, typeof ACTIONS>>((acc, a) => {
                  (acc[a.group] ??= []).push(a);
                  return acc;
                }, {}),
              ).map(([group, actions]) => (
                <Command.Group
                  key={group}
                  heading={group}
                  className="[&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1 [&_[cmdk-group-heading]]:text-2xs [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:text-[var(--dim)] [&_[cmdk-group-heading]]:uppercase"
                >
                  {actions.map((a) => (
                    <Command.Item
                      key={a.id}
                      value={`${a.label} ${a.id}`}
                      onSelect={() => {
                        close();
                        a.run(dock.current);
                      }}
                      className="flex cursor-default items-center gap-2 rounded-sm px-2 py-1 text-sm data-[selected=true]:bg-[var(--selection-bg)] data-[selected=true]:text-[var(--text-strong)]"
                    >
                      <span className="flex-1">{a.label}</span>
                      {bindingsFor(a, overrides)[0] && <Kbd>{formatChord(bindingsFor(a, overrides)[0])}</Kbd>}
                    </Command.Item>
                  ))}
                </Command.Group>
              ))}

            {mode === "files" &&
              files.map((p) => (
                <Command.Item
                  key={p}
                  value={p}
                  onSelect={() => {
                    close();
                    if (id) openFile(id, p, { pin: true });
                  }}
                  className="flex cursor-default items-center gap-1 rounded-sm px-2 py-1 text-sm data-[selected=true]:bg-[var(--selection-bg)] data-[selected=true]:text-[var(--text-strong)]"
                >
                  <FileIcon name={base(p)} size={12} className="shrink-0" />
                  <Mono className="text-sm">{base(p)}</Mono>
                  <Dim className="truncate text-2xs">{shortDir(p)}</Dim>
                </Command.Item>
              ))}
          </Command.List>
        </Command>
      </div>
    </div>
  );
}
