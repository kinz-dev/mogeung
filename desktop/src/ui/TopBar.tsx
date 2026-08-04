/**
 * The top strip: connection, scope of what you are watching, and the actions
 * that are not about one session.
 *
 * No wordmark — the icon says it in 16px, and a bar this dense should spend its
 * width on the queue rather than on the app's own name spelled out. The icon
 * itself is not decoration: `decorations: false` means this strip *is* the
 * title bar, so without it the window wears the mascot in the taskbar and
 * nowhere you can see while using it.
 */

import { Activity, Command, Keyboard, Moon, RefreshCw, Sun, Monitor, HeartPulse, SquareTerminal } from "lucide-react";
import { useStore } from "@/store";
import { Chip, Dim, IconButton, Tooltip } from "@/ui/primitives";
import { isSameMachine } from "@/wire/types";
import { showPane } from "@/lib/panes";
import { WindowControls } from "@/ui/WindowControls";

const THEMES = ["dark", "light", "system"] as const;

export function TopBar() {
  const conn = useStore((s) => s.conn);
  const daemon = useStore((s) => s.daemon);
  const send = useStore((s) => s.send);
  const prefs = useStore((s) => s.prefs);
  const setPrefs = useStore((s) => s.setPrefs);
  const sessions = useStore((s) => s.sessions);
  const url = useStore((s) => s.url);
  // Subscribed, not read once: `getState()` in a render gives a value that
  // never updates, so the button's lit state was permanently whatever it was
  // when the bar first mounted.
  const showTerminal = useStore((s) => s.showTerminal);
  const rescanning = useStore((s) => s.rescanning);
  const daemonStatus = useStore((s) => s.daemonStatus);

  const live = Object.values(sessions).filter((s) => s.alive).length;
  const dot = conn === "open" ? "var(--green)" : conn === "connecting" ? "var(--amber)" : "var(--red)";

  // Local or remote is decided by identity, never by the address: an ssh -L
  // tunnel makes a remote daemon answer on 127.0.0.1. `R-I5`.
  const machineId = localStorage.getItem("mogeung.machine-id");
  const remote = daemon !== null && !isSameMachine(daemon, machineId);

  const ThemeIcon = prefs.theme === "dark" ? Moon : prefs.theme === "light" ? Sun : Monitor;

  return (
    // `data-tauri-drag-region` turns this strip into the window's title bar.
    // Only the element carrying the attribute drags, so every button below is
    // still clickable — and a double-click maximises, which is the gesture
    // people expect from a title bar and the one lost with the OS decorations.
    <div
      data-tauri-drag-region
      className="flex h-8 shrink-0 items-center gap-2 border-b border-[var(--border)] px-2.5"
    >
      {/* `draggable={false}` or the image itself becomes an HTML5 drag source
          and the window-drag gesture dies wherever the icon is. */}
      <img
        src="/mogeung.png"
        alt="mogeung"
        title="mogeung"
        draggable={false}
        data-tauri-drag-region
        className="h-4 w-4 shrink-0 select-none"
      />

      <Tooltip
        content={
          conn === "open"
            ? `connected to ${url}${daemon ? `\n${daemon.host ?? "unknown host"} · ${daemon.version}` : ""}`
            : `${conn} — ${url}`
        }
      >
        <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: dot }} />
      </Tooltip>

      {daemonStatus?.mode === "hosting" && (
        <Chip
          color="var(--green)"
          title={
            "This window is hosting the daemon. Closing it stops watching — run mogeungd " +
            "separately if you want notifications to continue."
          }
        >
          hosting
        </Chip>
      )}

      {daemon && (
        <span className="text-xs">
          {remote ? (
            <span className="text-[var(--amber)]" title="another machine — local actions are refused">
              watching {daemon.host ?? "remote"}
            </span>
          ) : (
            <Dim>{daemon.host ?? "local"}</Dim>
          )}
        </span>
      )}

      <Dim className="text-xs">
        {live} live · {Object.keys(sessions).length} known
      </Dim>

      <div className="ml-auto flex items-center gap-0.5">
        <IconButton title="command palette  (Ctrl+K)" onClick={() => useStore.setState({ paletteOpen: true })}>
          <Command size={13} />
        </IconButton>
        <IconButton
          title="your own shells, across the bottom  (Ctrl+`)"
          active={showTerminal}
          onClick={() => useStore.setState({ showTerminal: !showTerminal })}
        >
          <SquareTerminal size={13} />
        </IconButton>
        <IconButton
          title={rescanning ? "scanning…" : "rescan now  (Alt+R)"}
          active={rescanning}
          onClick={() => {
            // The spin *is* the feedback. A scan takes under a second and
            // changes nothing visible when nothing moved, so without this the
            // button reads as dead — which is exactly how it was reported.
            useStore.setState({ rescanning: true });
            send({ cmd: "rescan" });
            // A daemon that never answers must not leave it spinning for ever.
            window.setTimeout(() => useStore.setState({ rescanning: false }), 4000);
          }}
        >
          <RefreshCw size={13} className={rescanning ? "animate-spin" : undefined} />
        </IconButton>
        <IconButton
          title="what mogeung can and cannot see  (Alt+H)"
          onClick={() => {
            send({ cmd: "fetch_health" });
            useStore.setState({ showHealth: true });
          }}
        >
          <HeartPulse size={13} />
        </IconButton>
        <IconButton
          title="token burn and limits — opens Insight"
          onClick={() => {
            // It used to fetch and show nothing, which is the same as doing
            // nothing. The charts that render this live in Insight, so send
            // *and* go there.
            send({ cmd: "fetch_usage" });
            showPane("insight", "Insight");
          }}
        >
          <Activity size={13} />
        </IconButton>
        <IconButton title="keyboard shortcuts  (Alt+K)" onClick={() => useStore.setState({ showKeymap: true })}>
          <Keyboard size={13} />
        </IconButton>
        <IconButton
          title={`theme: ${prefs.theme}  (Alt+T)`}
          onClick={() => setPrefs({ theme: THEMES[(THEMES.indexOf(prefs.theme) + 1) % THEMES.length] })}
        >
          <ThemeIcon size={13} />
        </IconButton>
        <WindowControls />
      </div>
    </div>
  );
}
