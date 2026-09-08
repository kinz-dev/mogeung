/**
 * What a detached pane renders instead of the whole window. `R-B55`,
 * [ADR-0037](../../docs/decisions/0037-a-pane-pops-out-into-a-window-of-its-own.md).
 *
 * This is an **ordinary client** and not a satellite of the main window. It has
 * its own store, its own socket, and — for an Agent pane — its own tmux client.
 * Nothing is shared but the daemon and the shell's pty table, which is exactly
 * what makes it correct to have two: the daemon is the authority and neither
 * window has any (ADR-0001).
 *
 * **No dockview.** A popout holds one pane and cannot gain another, so there is
 * nothing to arrange, and a docking system with one panel in it is chrome
 * charging rent. That also means panes cannot be dragged between the windows —
 * named in the ADR as a cost, not an oversight.
 *
 * The pane is bound by **holding** it on the session named in the URL, rather
 * than by selecting it. A hold is what already means *"this pane shows this
 * session whatever the queue says"*, and this window has no queue to say
 * otherwise — but the store still runs, sessions still end, and an unheld pane
 * would follow a `selected` this window never set.
 */

import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { AgentPane } from "@/panes/AgentPane";
import { PaneScope } from "@/lib/paneScope";
import { useStore } from "@/store";
import { Dim } from "@/ui/primitives";
import { Toasts } from "@/ui/Notices";
import { closeThisWindow, type Popout } from "@/lib/popout";

/** The pane id this window's single pane answers to. */
export function popoutPaneId(p: Popout): string {
  return `popout:${p.kind}`;
}

export default function PopoutApp({ popout }: { popout: Popout }) {
  const theme = useStore((s) => s.prefs.theme);
  const session = useStore((s) => s.sessions[popout.session] ?? null);
  const conn = useStore((s) => s.conn);
  const paneId = popoutPaneId(popout);

  // The same resolution the main window does. Duplicated rather than shared
  // because it is four lines and the alternative is a hook that exists to be
  // called twice.
  useEffect(() => {
    const root = document.documentElement;
    const apply = () => {
      const resolved =
        theme === "system"
          ? window.matchMedia("(prefers-color-scheme: light)").matches
            ? "light"
            : "dark"
          : theme;
      root.setAttribute("data-theme", resolved);
    };
    apply();
    if (theme !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);

  // Held, not selected — see the note at the top. Written once the store is up;
  // `setScoped` keys by machine, so it has to wait for the daemon's identity
  // rather than run on mount.
  const machineId = useStore((s) => s.machineId);
  useEffect(() => {
    if (!machineId) return;
    const { scoped, setScoped } = useStore.getState();
    if (scoped().paneHold[paneId] === popout.session) return;
    setScoped({ paneHold: { ...scoped().paneHold, [paneId]: popout.session } });
  }, [machineId, paneId, popout.session]);

  const title = session?.title?.trim() || popout.session.slice(0, 8);

  return (
    <div className="flex h-full flex-col bg-[var(--bg)]">
      {/*
        The whole strip drags, which is the only way to move a window with no
        decorations. The close button sits on top of it and is still clickable,
        the same arrangement `TopBar` uses.
      */}
      <div
        data-tauri-drag-region
        className="flex h-8 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2.5"
      >
        <img
          src="/mogeung.png"
          alt=""
          draggable={false}
          data-tauri-drag-region
          className="h-4 w-4 shrink-0 select-none"
        />
        <span
          data-tauri-drag-region
          className="min-w-0 flex-1 truncate text-xs font-medium text-[var(--text-strong)]"
        >
          {title}
        </span>
        {conn !== "open" && (
          <Dim className="shrink-0 text-2xs" title="this window has its own socket to the daemon">
            {conn}
          </Dim>
        )}
        <button
          type="button"
          aria-label="close this window"
          title="close — the pane goes back to the main window"
          onClick={() => void closeThisWindow()}
          className="grid h-5 w-5 shrink-0 place-items-center rounded-sm text-[var(--dim)] outline-none hover:bg-[var(--bg-hover)] hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-[var(--ring)]"
        >
          <X size={12} />
        </button>
      </div>

      <div className="min-h-0 flex-1">
        {/*
          `visible` is unconditionally true: this pane is the window, so the
          cost `PaneVisibleContext` exists to manage — an offscreen tab holding
          a pty — cannot arise. A hidden popout is a *minimised window*, which
          is the OS's business and not a reason to drop a tmux client.
        */}
        <PaneScope id={paneId} visible>
          <AgentPane />
        </PaneScope>
      </div>

      <Toasts />
    </div>
  );
}

/**
 * The whole client, deciding which of the two things it is.
 *
 * Exported as a component rather than resolved in `main.tsx` so the branch is
 * testable without a DOM entry point.
 */
export function PopoutBoot({ popout }: { popout: Popout }) {
  // A popout whose session has not arrived yet is the ordinary case for the
  // first tick — the socket opens after the window does. Saying "gone" then
  // would be a lie that corrects itself, which is worse than a pause.
  const [waited, setWaited] = useState(false);
  useEffect(() => {
    const t = window.setTimeout(() => setWaited(true), 4000);
    return () => window.clearTimeout(t);
  }, []);

  const known = useStore((s) => popout.session in s.sessions);
  const conn = useStore((s) => s.conn);

  if (!known && waited && conn === "open") {
    return (
      <div className="flex h-full flex-col bg-[var(--bg)]">
        <div
          data-tauri-drag-region
          className="flex h-8 shrink-0 items-center border-b border-[var(--border)] px-2.5"
        >
          <span data-tauri-drag-region className="flex-1 text-xs text-[var(--dim)]">
            mogeung
          </span>
          <button
            type="button"
            aria-label="close this window"
            onClick={() => void closeThisWindow()}
            className="grid h-5 w-5 place-items-center rounded-sm text-[var(--dim)] outline-none hover:bg-[var(--bg-hover)] hover:text-[var(--text)] focus-visible:outline-2 focus-visible:outline-[var(--ring)]"
          >
            <X size={12} />
          </button>
        </div>
        <div className="grid flex-1 place-items-center p-6 text-center">
          <div className="max-w-sm space-y-2">
            <div className="text-sm text-[var(--text-strong)]">that session has ended</div>
            <Dim className="block text-xs">
              This window was watching <code>{popout.session.slice(0, 8)}</code>, and the daemon no
              longer knows it. Closing this window is all that is left to do.
            </Dim>
          </div>
        </div>
      </div>
    );
  }

  return <PopoutApp popout={popout} />;
}
