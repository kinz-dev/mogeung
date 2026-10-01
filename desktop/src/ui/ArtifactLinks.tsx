/**
 * The pages a session published on claude.ai, as links. `R-J115`.
 *
 * Asked 2026-10-01: *"showing the artifact link may be at the top of the
 * panel … show the name of the artifact and with a link so that it can popup
 * the browser"*. An artifact is the session's output in a form meant to be
 * read, and the transcript only mentions it in passing — this puts it where
 * the session's other facts are.
 *
 * One artifact is a link. Several are a link to the newest and a list behind
 * it, because a header has room for one title and a session that drew three
 * pages most likely wants the latest.
 */

import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { ChevronDown, FileText } from "lucide-react";
import { useStore } from "@/store";
import { cn } from "@/lib/cn";
import { openArtifact } from "@/lib/tauri";
import { interactive, surface } from "@/ui/styles";
import type { Artifact } from "@/wire/types";

function open(a: Artifact) {
  openArtifact(a.url).catch((e) => useStore.getState().pushError(`could not open ${a.title}: ${e}`));
}

const trigger =
  "inline-flex h-5 min-w-0 items-center gap-1 rounded-sm border border-transparent px-1.5 text-2xs text-[var(--blue)] hover:border-[var(--border)] data-[state=open]:bg-[var(--state-focus)]";

/** For a pane header: the newest artifact, and the rest behind a caret. */
export function ArtifactLinks({ artifacts }: { artifacts: Artifact[] | undefined }) {
  if (!artifacts?.length) return null;
  const [newest, ...rest] = artifacts;

  const title = (
    <>
      <FileText className="h-3 w-3 shrink-0" />
      <span className="max-w-[16rem] truncate">{newest.title}</span>
    </>
  );

  if (rest.length === 0) {
    return (
      <button type="button" className={cn(interactive, trigger)} title={`open ${newest.title} in the browser\n${newest.url}`} onClick={() => open(newest)}>
        {title}
      </button>
    );
  }

  return (
    <DropdownPrimitive.Root modal={false}>
      <DropdownPrimitive.Trigger asChild>
        <button type="button" className={cn(interactive, trigger)} title={`${artifacts.length} artifacts from this session`}>
          {title}
          <span className="shrink-0 text-[var(--dim)]">+{rest.length}</span>
          <ChevronDown className="h-3 w-3 shrink-0 text-[var(--dim)]" />
        </button>
      </DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align="end" sideOffset={4} collisionPadding={8} className={cn(surface.overlay, "z-50 min-w-56 max-w-96 p-1")}>
          <DropdownPrimitive.Label className="px-2 py-1 text-2xs tracking-wider text-[var(--dim)] uppercase">
            Artifacts — newest first
          </DropdownPrimitive.Label>
          {artifacts.map((a) => (
            <DropdownPrimitive.Item
              key={a.id}
              onSelect={() => open(a)}
              title={a.url}
              className={cn(
                "flex cursor-default items-center gap-1.5 rounded-sm px-2 py-1 text-xs outline-none select-none",
                "data-[highlighted]:bg-[var(--state-focus)] data-[highlighted]:text-[var(--text-strong)]",
              )}
            >
              <FileText className="h-3 w-3 shrink-0 text-[var(--blue)]" />
              <span className="truncate">{a.title}</span>
            </DropdownPrimitive.Item>
          ))}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}
