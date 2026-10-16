/**
 * Folders on the machine a session will start on. `R-J142`.
 *
 * Asked 2026-10-16: *"in the new session dialog, if I selected the remote
 * machine that I follow. And when I click the "open" directory button. It
 * always show my own local machine folder."* It always would have: the button
 * opened the OS folder picker, and an OS picker can only show the disk of the
 * computer the window runs on — wrong for a followed machine, and wrong for
 * *this machine* too whenever the daemon being watched is on another box.
 *
 * So the window draws the picker and the daemon whose disk it is answers it:
 * `browse_folders` to the watched daemon, forwarded to a follow by its
 * machine id the way a launch is. Folder names only. Clicking a folder steps
 * into it; *use this folder* puts where you are into the box. It opens at what
 * is typed there — at its nearest folder that exists, so a half-typed path is
 * a place to carry on from rather than an error.
 *
 * Hidden folders are the window's to hide, behind a checkbox, because a
 * `.config` you meant to start in is still a folder you meant to start in.
 */

import { useEffect, useState } from "react";
import { ArrowUp, Folder } from "lucide-react";
import { useStore } from "@/store";
import { Button, Checkbox, Dim, IconButton, Mono, Row } from "@/ui/primitives";

/** How long an answer may take before the browser says it is still waiting. */
const SLOW_MS = 5000;

/** `name` inside `dir`, without doubling the root's slash. */
export function joinPath(dir: string, name: string): string {
  return dir.endsWith("/") ? `${dir}${name}` : `${dir}/${name}`;
}

export function FolderBrowser({
  machine,
  machineName,
  start,
  onChoose,
}: {
  /** `null` is the watched daemon; otherwise a followed machine's id. */
  machine: string | null;
  /** How the window names that machine, for the sentences that do. */
  machineName: string;
  /** What is in the box: where to open. */
  start: string;
  onChoose: (path: string) => void;
}) {
  const browse = useStore((s) => s.browse);
  const browseFolders = useStore((s) => s.browseFolders);
  const [hidden, setHidden] = useState(false);
  const [slow, setSlow] = useState(false);

  // Asked again when the machine changes under it: the folder on screen is
  // the other machine's. Closed with the browser, so a late answer has
  // nothing to land on.
  useEffect(() => {
    browseFolders(machine, start.trim() || "~");
    return () => useStore.setState({ browse: null });
    // `start` only opens the browser; typing afterwards is not navigating.
  }, [machine, browseFolders]);

  const pending = !!browse?.pending;
  useEffect(() => {
    setSlow(false);
    if (!pending) return;
    const t = setTimeout(() => setSlow(true), SLOW_MS);
    return () => clearTimeout(t);
  }, [pending, browse?.asked]);

  const listing = browse?.listing ?? null;
  const dirs = (listing?.dirs ?? []).filter((d) => hidden || !d.startsWith("."));
  const go = (path: string) => browseFolders(machine, path);

  let status: string | null = null;
  if (browse?.outdated) {
    status = `${machineName}'s mogeung is older than this window and cannot list its folders — update it and restart its daemon, or type the path`;
  } else if (browse?.error) {
    status = browse.error;
  } else if (pending && slow) {
    status = `no answer from ${machineName} yet`;
  } else if (pending && !listing) {
    status = `asking ${machineName}…`;
  } else if (listing && dirs.length === 0) {
    status = listing.dirs.length > 0 ? "only hidden folders here" : "no folders here";
  } else if (listing?.truncated) {
    status = `the first ${listing.dirs.length} folders only — type the rest of the path`;
  }

  return (
    <div data-testid="folder-browser" className="w-full rounded-sm border border-[var(--border)] bg-[var(--bg)]">
      <div className="flex items-center gap-1 border-b border-[var(--border)] px-1 py-0.5">
        <IconButton
          title={listing?.parent ? `up to ${listing.parent}` : "already at the top"}
          disabled={!listing?.parent || pending}
          onClick={() => listing?.parent && go(listing.parent)}
        >
          <ArrowUp className="h-3.5 w-3.5" />
        </IconButton>
        <Mono className="flex-1 truncate text-xs" title={listing?.path}>
          {listing?.path ?? "…"}
        </Mono>
        <Checkbox checked={hidden} onChange={setHidden} label="hidden" title="show folders whose name starts with a dot" />
        <Button
          disabled={!listing || pending}
          title={listing ? `start in ${listing.path} on ${machineName}` : "nothing listed yet"}
          onClick={() => listing && onChoose(listing.path)}
        >
          use this folder
        </Button>
      </div>
      {status && <Dim className="block px-2 py-1 text-xs italic">{status}</Dim>}
      {dirs.length > 0 && (
        <div className="max-h-48 overflow-y-auto py-0.5">
          {dirs.map((d) => (
            <Row
              key={d}
              onClick={() => listing && go(joinPath(listing.path, d))}
              className="flex items-center gap-1.5 px-2 py-0.5"
              title={listing ? joinPath(listing.path, d) : d}
            >
              <Folder className="h-3 w-3 shrink-0 opacity-60" />
              <Mono className="flex-1 truncate text-xs">{d}</Mono>
            </Row>
          ))}
        </div>
      )}
    </div>
  );
}
