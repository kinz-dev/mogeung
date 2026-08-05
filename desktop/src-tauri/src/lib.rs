//! The desktop shell.
//!
//! This is the half of the client that has to be native, and it is deliberately
//! thin — everything else is the web view talking to the daemon over a socket.
//! Three jobs:
//!
//! 1. **Hold the ptys.** `R-B18`'s attached terminal and `R-B31`/`R-B33`'s
//!    shells both live here rather than in the daemon, which is precisely what
//!    keeps [ADR-0010] and [ADR-0011] true: what this process holds is a *view*
//!    of a tmux session, so closing the window detaches instead of killing.
//!    The daemon is never told, because there is nothing it could correctly do
//!    with the information.
//! 2. **The global shortcut** that raises the window (`R-B10`).
//! 3. **Window geometry**, which rides in our own preferences rather than a
//!    plugin's store — the same reasoning `R-J1` used to refuse eframe's
//!    persistence: two stores holding the same kind of thing is how they drift.
//!
//! [ADR-0010]: ../../../docs/decisions/0010-attach-a-terminal-never-own-one.md
//! [ADR-0011]: ../../../docs/decisions/0011-own-a-shell-never-an-agent.md

mod daemon;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serde::Serialize;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// One live pty, and everything needed to stop it.
///
/// The last two fields are the fix for a real bug, and the reason they exist is
/// not obvious from the types. `try_clone_reader` hands the reader thread its
/// **own duplicated descriptor** — so dropping `writer` and `master` does not
/// close the pty, the child keeps running, and the thread keeps emitting under
/// an id the window has since reused. Switching away from a session and back
/// then leaves two emitters on one id, then three, then four: typing `a`
/// produced `aaaa`, with one more character per switch.
///
/// So closing has to *say so* rather than merely letting go of a handle.
struct Pty {
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    /// Kills the process we spawned. For `tmux attach` that is the **client**,
    /// so this detaches rather than ending the session — ADR-0010 intact.
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    /// Read by the reader thread before every emit, so a read already in flight
    /// when this closes cannot deliver into a reused id.
    stop: Arc<AtomicBool>,
}

impl Drop for Pty {
    fn drop(&mut self) {
        // Order matters: raise the flag first, so a read that returns during
        // the kill has already lost the right to emit.
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.killer.kill();
    }
}

#[derive(Default)]
struct Ptys(Mutex<HashMap<String, Pty>>);

#[cfg(test)]
mod pty_tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Closing a pty has to **end the process**, not merely drop our handles.
    ///
    /// This is the bug that survived two earlier fixes, because both were in the
    /// wrong layer. `try_clone_reader` gives the reader thread its own
    /// descriptor, so dropping the writer and the master leaves the child alive
    /// and the thread emitting — under an id the window reuses the next time you
    /// switch back to that session. One orphan per switch, and typing `a` came
    /// out as `aaaa`.
    ///
    /// Asserting on the *child* rather than on the flag is the point: a flag
    /// that is set while the process keeps running is exactly the state that
    /// produced the bug.
    #[test]
    fn closing_a_pty_ends_the_process_it_started() {
        let pair = NativePtySystem::default()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty");

        let mut cmd = CommandBuilder::new("sleep");
        cmd.arg("120");
        let mut child = pair.slave.spawn_command(cmd).expect("spawn");
        drop(pair.slave);

        let killer = child.clone_killer();
        let stop = Arc::new(AtomicBool::new(false));
        let writer = pair.master.take_writer().expect("writer");

        let pty = Pty {
            writer,
            master: pair.master,
            killer,
            stop: Arc::clone(&stop),
        };
        drop(pty);

        assert!(stop.load(Ordering::SeqCst), "the reader must be told to stop");

        // Polled rather than `wait()`ed, so a regression fails in two seconds
        // instead of hanging the suite for two minutes.
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() > deadline => {
                    let _ = child.kill();
                    panic!("the child outlived the pty that owned it");
                }
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

#[derive(Clone, Serialize)]
struct Chunk {
    id: String,
    data: String,
}

/// Open a pty and stream it to the web view under `id`.
///
/// `command` is what to run — `tmux attach -t …` for the Agent pane, a login
/// shell for the terminal panel, or `ssh -t <target> tmux …` when the daemon is
/// describing another machine (`R-I6`). The caller decides; this only spawns
/// what it is handed, which is the same division the egui client had.
#[tauri::command]
fn pty_open(
    app: tauri::AppHandle,
    state: tauri::State<'_, Ptys>,
    id: String,
    command: Vec<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    if command.is_empty() {
        return Err("nothing to run".into());
    }
    let pair = NativePtySystem::default()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let mut cmd = CommandBuilder::new(&command[0]);
    for arg in &command[1..] {
        cmd.arg(arg);
    }
    if let Some(dir) = cwd {
        cmd.cwd(dir);
    }
    // A terminal that does not say what it is gets treated as a dumb one, and
    // Claude Code's TUI is anything but.
    cmd.env("TERM", "xterm-256color");

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    // Taken before the child moves into the reader thread: killing it is how
    // this pty is closed, and the thread is exactly where we cannot reach.
    let killer = child.clone_killer();
    let stop = Arc::new(AtomicBool::new(false));

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

    let emit_id = id.clone();
    let thread_stop = Arc::clone(&stop);
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    // Checked *after* the read, because the read is where this
                    // thread spends its life: by the time bytes arrive the pane
                    // may be long gone and its id given to another session.
                    if thread_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    // Lossy on purpose: a pty carries bytes, and a partial
                    // multi-byte sequence at a chunk boundary must not take
                    // the stream down. Degrade, never panic — the rule the
                    // transcript parsers already follow.
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    if app.emit("pty:data", Chunk { id: emit_id.clone(), data }).is_err() {
                        break;
                    }
                }
            }
        }
        let _ = child.wait();
        // A pane that was replaced does not want an "it closed" for an id that
        // now belongs to something else.
        if !thread_stop.load(Ordering::SeqCst) {
            let _ = app.emit("pty:closed", emit_id);
        }
    });

    // Replacing an entry drops the old `Pty`, which closes that pty and ends
    // its reader thread. Doing it explicitly rather than relying on `insert`'s
    // return value being dropped, because "the old one goes away" is the
    // property that stops two threads emitting under one id — and a reader
    // that outlives its pane writes every byte twice.
    let mut ptys = state.0.lock().unwrap();
    drop(ptys.remove(&id));
    ptys.insert(
        id,
        Pty {
            writer,
            master: pair.master,
            killer,
            stop,
        },
    );
    Ok(())
}

#[tauri::command]
fn pty_write(state: tauri::State<'_, Ptys>, id: String, data: String) -> Result<(), String> {
    let mut ptys = state.0.lock().unwrap();
    let pty = ptys.get_mut(&id).ok_or("no such terminal")?;
    pty.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    pty.writer.flush().map_err(|e| e.to_string())
}

#[tauri::command]
fn pty_resize(state: tauri::State<'_, Ptys>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    let ptys = state.0.lock().unwrap();
    let pty = ptys.get(&id).ok_or("no such terminal")?;
    pty.master
        .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())
}

/// Drop the pty. For a tmux-backed session this **detaches**; the session keeps
/// running and is reachable from any terminal. That is the whole of ADR-0010.
#[tauri::command]
fn pty_close(state: tauri::State<'_, Ptys>, id: String) {
    state.0.lock().unwrap().remove(&id);
}

/// This machine's id, so the client can tell a local daemon from a remote one
/// by identity rather than by the address it dialled (`R-I5`). Written once by
/// the daemon; read here, never invented — an id we made up would answer the
/// question wrongly and confidently.
#[tauri::command]
fn machine_id() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    std::fs::read_to_string(format!("{home}/.mogeung/machine-id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Attach to a running daemon, or take the port and host one. `R-?`/ADR-0009.
///
/// Called by the window before it connects, and **idempotent**: the answer is
/// computed once and replayed, because a reconnect must not try to bind a port
/// this process is already serving on.
#[tauri::command]
fn daemon_acquire(state: tauri::State<'_, DaemonOnce>, addr: String) -> daemon::Status {
    let mut held = state.0.lock().unwrap();
    if let Some(status) = held.as_ref() {
        return status.clone();
    }
    let status = daemon::acquire(&addr);
    eprintln!("{}", status.detail(&addr));
    *held = Some(status.clone());
    status
}

#[derive(Default)]
struct DaemonOnce(Mutex<Option<daemon::Status>>);

/// The system-wide key that brings the window to the front (`R-B10`).
///
/// Low collision risk, and reachable one-handed. A global shortcut is stolen
/// from **every** application, so the default has to be something almost
/// nothing else claims: `Cmd+Shift+M` is the obvious mnemonic and several
/// editors use it; `Ctrl+Cmd+M` is not. Inherited unchanged from the egui
/// client so the key that has been in this user's fingers since `R-B10` keeps
/// working across [ADR-0020].
///
/// `Cmd` parses to SUPER, which is Command on macOS and the Windows key on
/// Linux — the same mapping the old client got from the same underlying crate.
///
/// Registering a shortcut macOS reserves for itself (`Cmd+Space`, `Cmd+Tab`)
/// **succeeds** and then never fires, because the system consumes the key
/// first. There is no way to detect that from here.
///
/// [ADR-0020]: ../../../docs/decisions/0020-the-egui-client-is-retired.md
const HOTKEY: &str = "Ctrl+Cmd+M";

/// Bring the window forward from wherever it is — minimised, behind a full
/// screen terminal, on another workspace. Unminimise first: `set_focus` on a
/// minimised window is a no-op on every platform, which reads as the shortcut
/// being broken rather than as the window being iconified.
fn raise(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                // Only `Pressed`: every shortcut also reports `Released`, and
                // acting on both raises the window twice per press.
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        raise(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_notification::init())
        .manage(Ptys::default())
        .manage(DaemonOnce::default())
        .invoke_handler(tauri::generate_handler![
            pty_open,
            pty_write,
            pty_resize,
            pty_close,
            machine_id,
            daemon_acquire
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title("mogeung");
            }
            // Never fatal, and reported rather than swallowed: a shortcut
            // another application already owns is an ordinary thing to hit,
            // and it must not stop mogeung opening. The old client put this in
            // its error strip; here it goes to stderr, which is where the rest
            // of this process already talks.
            match HOTKEY.parse::<Shortcut>() {
                Ok(shortcut) => {
                    if let Err(e) = app.global_shortcut().register(shortcut) {
                        eprintln!(
                            "could not register {HOTKEY} — another application probably owns it ({e})"
                        );
                    }
                }
                Err(e) => eprintln!("{HOTKEY:?} is not a valid shortcut: {e}"),
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mogeung");
}
