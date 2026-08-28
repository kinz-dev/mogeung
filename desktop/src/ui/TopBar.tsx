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

import { Activity, Bell, BellOff, Command, Flag, HeartPulse, Keyboard, Monitor, Moon, RefreshCw, Rocket, ScreenShare, Settings, SquareTerminal, Sun } from "lucide-react";
import { useStore } from "@/store";
import { useChord } from "@/lib/keymap";
import { Chip, Dim, IconButton, Tooltip } from "@/ui/primitives";
import { isSameMachine } from "@/wire/types";
import { showPane } from "@/lib/panes";
import { WindowControls } from "@/ui/WindowControls";
import { bellState, ensurePermission } from "@/lib/notify";

const THEMES = ["dark", "light", "system"] as const;

export function TopBar() {
  // The chords the buttons advertise, read from the keymap rather than typed
  // into each `title`. `R-J19`. Two of them were wrong before this — the theme
  // button said `Alt+T`, which `R-B47` gave to the Transcript — and on a Mac
  // every one of them named a chord this window is not listening for.
  const paletteChord = useChord("palette");
  const terminalChord = useChord("terminal.toggle");
  const rescanChord = useChord("rescan");
  const healthChord = useChord("health");
  const keymapChord = useChord("keymap");
  const themeChord = useChord("theme");
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
  const bell = bellState(prefs.notify, daemonStatus?.mode === "hosting");
  const flaggedCount = useStore((s) => s.flagged.length);

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
            ? `connected to ${url}${daemon ? `\n${daemon.host ?? "unknown host"} · ${daemon.version}` : ""}\nclick to switch daemons`
            : `${conn} — ${url}\nclick to switch daemons`
        }
      >
        {/* The dot is the connection, so it is also the way to change it. */}
        <button
          type="button"
          aria-label="connections"
          onClick={() => useStore.setState({ showConnections: true })}
          className="grid h-4 w-4 shrink-0 place-items-center rounded-sm outline-none focus-visible:outline-2 focus-visible:outline-[var(--ring)] focus-visible:-outline-offset-2"
        >
          <span className="h-2 w-2 rounded-full" style={{ background: dot }} />
        </button>
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
        <IconButton
          title="start a session — opens a terminal on the daemon's machine, and mogeung watches it like any other"
          onClick={() => useStore.setState({ showLaunch: true })}
        >
          <Rocket size={13} />
        </IconButton>
        <IconButton
          title={
            flaggedCount > 0
              ? `follow-up prompt — ${flaggedCount} flagged hunk(s). mogeung writes it, you paste it`
              : "follow-up prompt — flag hunks in a diff first. Nothing is ever sent to a session"
          }
          active={flaggedCount > 0}
          onClick={() => useStore.setState({ showPrompt: true })}
        >
          <Flag size={13} />
        </IconButton>
        <IconButton
          title="ambient board — big enough to read across a room"
          onClick={() => useStore.setState({ ambient: true })}
        >
          <ScreenShare size={13} />
        </IconButton>
        <IconButton title={`command palette${paletteChord ? `  (${paletteChord})` : ""}`} onClick={() => useStore.setState({ paletteOpen: true })}>
          <Command size={13} />
        </IconButton>
        <IconButton
          title={`your own shells, across the bottom${terminalChord ? `  (${terminalChord})` : ""}`}
          active={showTerminal}
          onClick={() => useStore.setState({ showTerminal: !showTerminal })}
        >
          <SquareTerminal size={13} />
        </IconButton>
        <IconButton
          title={rescanning ? "scanning…" : `rescan now${rescanChord ? `  (${rescanChord})` : ""}`}
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
          title={`what mogeung can and cannot see${healthChord ? `  (${healthChord})` : ""}`}
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
        <IconButton
          title={
            {
              off: "tell me when a session needs me, while this window is in the background",
              here: "banners on — you will be told when a session starts needing you, while this window is in the background",
              elsewhere:
                "banners on, but this window did not start its daemon, so it is not the one that speaks. The daemon announces its own queue when run with --notify — which scripts/start.sh does by default.",
            }[bell]
          }
          active={prefs.notify}
          /**
           * Amber for `elsewhere`, which is the state that was invisible.
           *
           * The tooltip has always explained it and a tooltip is not an answer
           * to *"I turned it on and nothing changed"* — you have to already
           * suspect something to go looking. Amber is what this window uses
           * everywhere else for *on, with a caveat* (the ssh host chip, a
           * collision, a stale doc), so it reads without being learnt.
           */
          className={bell === "elsewhere" ? "text-[var(--amber)] hover:text-[var(--amber)]" : undefined}
          onClick={() => {
            if (prefs.notify) {
              setPrefs({ notify: false });
              return;
            }
            // Permission is asked for on the way in, not at startup: a prompt
            // for a feature nobody has asked for is the overstep.
            void ensurePermission().then((ok) => {
              setPrefs({ notify: ok });
              if (!ok) {
                useStore
                  .getState()
                  .pushError(
                    "the system refused notification permission — banners stay off until it is granted",
                  );
              }
            });
          }}
        >
          {prefs.notify ? <Bell size={13} /> : <BellOff size={13} />}
        </IconButton>
        <IconButton title={`keyboard shortcuts${keymapChord ? `  (${keymapChord})` : ""}`} onClick={() => useStore.setState({ showKeymap: true })}>
          <Keyboard size={13} />
        </IconButton>
        {/* The config file. `R-J79`, and here because a settings window
            reachable only from the palette is one you have to already know
            about — which is the opposite of what it is for. */}
        <IconButton
          title="configuration file (~/.mogeung/config.toml)"
          onClick={() => useStore.setState({ showConfig: true })}
        >
          <Settings size={13} />
        </IconButton>
        <IconButton
          title={`theme: ${prefs.theme}${themeChord ? `  (${themeChord})` : ""}`}
          onClick={() => setPrefs({ theme: THEMES[(THEMES.indexOf(prefs.theme) + 1) % THEMES.length] })}
        >
          <ThemeIcon size={13} />
        </IconButton>
        <WindowControls />
      </div>
    </div>
  );
}
