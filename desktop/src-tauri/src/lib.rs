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
use std::path::{Path, PathBuf};
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

#[cfg(test)]
mod export_tests {
    use super::*;

    /// The name is built from a session title, which is **the agent's text**.
    /// Anything it can write, it can write into a filename.
    #[test]
    fn a_title_cannot_escape_the_directory() {
        assert_eq!(safe_name("../../etc/passwd"), "etc-passwd");
        assert_eq!(safe_name("a/b/c.md"), "a-b-c.md");
        assert_eq!(safe_name("with\nnewline"), "with-newline");
        assert_eq!(safe_name("..."), "export");
        assert_eq!(safe_name(""), "export");
    }

    /// Two different titles must not collapse onto one file, or an export
    /// silently lands on top of a different session's.
    #[test]
    fn different_titles_stay_different_names() {
        assert_ne!(safe_name("fix: the queue"), safe_name("fix: the diff"));
    }

    /// Exporting twice is the ordinary case — export, the agent works on, export
    /// again — and the second must not destroy the first.
    #[test]
    fn a_second_export_does_not_overwrite_the_first() {
        let dir = std::env::temp_dir().join(format!("mog-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = free_path(&dir, "session.md");
        assert_eq!(first.file_name().unwrap(), "session.md");
        std::fs::write(&first, "one").unwrap();

        let second = free_path(&dir, "session.md");
        assert_eq!(second.file_name().unwrap(), "session-2.md");
        std::fs::write(&second, "two").unwrap();

        assert_eq!(std::fs::read_to_string(&first).unwrap(), "one");
        assert_eq!(free_path(&dir, "session.md").file_name().unwrap(), "session-3.md");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A path you chose in a dialog is written where you said, replacing what
    /// is there — the dialog already asked. Writing `report-2.md` after you
    /// answered "yes, replace it" is the window overruling you.
    #[test]
    fn a_chosen_path_is_honoured_exactly() {
        let dir = std::env::temp_dir().join(format!("mog-chosen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let chosen = dir.join("report.md");
        std::fs::write(&chosen, "old").unwrap();

        let target = export_target(Some(chosen.to_str().unwrap()), "ignored.md").unwrap();
        assert_eq!(target, chosen, "a chosen path must not be renamed around");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The name is only sanitised on the fallback route. On the chosen route it
    /// is not ours to touch: the user typed it into their own file manager.
    #[test]
    fn a_missing_directory_is_reported_rather_than_created() {
        let nowhere = std::env::temp_dir().join("mog-not-a-dir-xyz").join("f.md");
        let err = export_target(Some(nowhere.to_str().unwrap()), "f.md").unwrap_err();
        assert!(err.contains("not a directory"), "{err}");
    }

    #[test]
    fn a_name_with_no_extension_still_suffixes_sanely() {
        let dir = std::env::temp_dir().join(format!("mog-export-noext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(free_path(&dir, "plain"), "x").unwrap();
        assert_eq!(free_path(&dir, "plain").file_name().unwrap(), "plain-2");
        std::fs::remove_dir_all(&dir).ok();
    }
}

/// A trace of everything this process does to a pty. Opt-in, and off unless
/// `MOGEUNG_PTY_LOG` names a file to append to.
///
/// It exists because a report — *switching sessions inserts a newline into the
/// agent's prompt* — survived being reasoned about. Reading says it cannot
/// happen: the only bare `\n` this client can send is the Shift+Enter branch in
/// `Terminal.tsx`, xterm.js emits no LF of its own (every unsolicited reply it
/// can send is an escape sequence), tmux injects nothing into a pane on attach
/// or resize (measured, with a byte logger), and the daemon never writes to a
/// session at all. A newline arrives anyway. When reading and the code
/// disagree, watch: this records the bytes with the pty they went to and the
/// call site that sent them, so one reproduction names the culprit.
fn trace(op: &str, id: &str, detail: &str) {
    let Ok(path) = std::env::var("MOGEUNG_PTY_LOG") else {
        return;
    };
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // Best-effort throughout: a diagnostic that can take the window down with
    // it is worse than no diagnostic.
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{ms} {op:<6} id={id} {detail}");
    }
}

/// Where an exported file goes. `R-B43`.
///
/// The user's downloads directory, because that is where a thing you asked a
/// window to save for you is looked for — `$XDG_DOWNLOAD_DIR` when the desktop
/// says so, then `~/Downloads`, and `~/.mogeung/exports` as the last resort so
/// a machine with neither still has an answer rather than an error.
///
/// This is the **fallback**, not the usual route: `plugin-dialog` asks you
/// where to save and `export_target` honours what you picked. It is what
/// answers when there is no picker to ask — the plugin absent, or refusing on a
/// desktop with no portal — so a save never evaporates for want of a dialog.
///
/// The picker was added on 2026-08-06, one commit after this shipped without
/// one. The argument against it was that a dialog plus a filesystem plugin
/// would hand the webview a general write verb; what changed is that only the
/// **dialog** was added. The path comes back to a command this shell owns, so
/// the one file the window can write is still one you named yourself.
fn export_dir() -> PathBuf {
    if let Ok(d) = std::env::var("XDG_DOWNLOAD_DIR") {
        let p = PathBuf::from(d);
        if p.is_dir() {
            return p;
        }
    }
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()));
    let downloads = home.join("Downloads");
    if downloads.is_dir() {
        return downloads;
    }
    home.join(".mogeung").join("exports")
}

/// A file name safe to join onto a directory.
///
/// The name is built from a session's title, which is **the agent's text** —
/// it can hold slashes, `..`, newlines, anything. Everything outside a known
/// safe set becomes a dash rather than being dropped, so two different titles
/// cannot collapse onto one name and no title can escape the directory.
pub fn safe_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(96)
        .collect();
    let trimmed = cleaned.trim_matches(['-', '.'].as_ref()).to_string();
    if trimmed.is_empty() {
        "export".to_string()
    } else {
        trimmed
    }
}

/// The first free path for `name` in `dir`, suffixing rather than overwriting.
///
/// Exporting the same transcript twice is the ordinary case — you export, the
/// agent keeps working, you export again — and the second one silently
/// replacing the first would destroy the copy you took precisely because you
/// wanted to keep it.
pub fn free_path(dir: &Path, name: &str) -> PathBuf {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    let first = dir.join(format!("{stem}{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let candidate = dir.join(format!("{stem}-{n}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{ext}"))
}

/// Where a save goes, given what the picker said.
///
/// Two routes, and the difference in overwrite behaviour is deliberate:
///
/// - **A path you chose** is written exactly as chosen, replacing what is
///   there. The native dialog already asked before returning a name that
///   exists, and asking twice — or quietly writing `report-2.md` when you said
///   `report.md` — is the window overruling an answer you already gave.
/// - **No path** means the picker was unavailable or declined, so this falls
///   back to the downloads directory, sanitises the name, and *never*
///   overwrites: nothing asked you anything, so nothing may destroy anything.
pub fn export_target(path: Option<&str>, name: &str) -> Result<PathBuf, String> {
    if let Some(p) = path {
        let p = PathBuf::from(p);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !parent.is_dir() {
                return Err(format!("{} is not a directory", parent.display()));
            }
        }
        return Ok(p);
    }
    let dir = export_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    Ok(free_path(&dir, &safe_name(name)))
}

/// Write text to a file and answer with the path written.
///
/// `path` is what the native save dialog returned, or `None` when there was no
/// dialog to ask. The path goes back to the window either way, because a save
/// you cannot find is a save that did not happen — the pane says where it went
/// rather than flashing "done".
#[tauri::command]
async fn export_text(name: String, contents: String, path: Option<String>) -> Result<String, String> {
    let target = export_target(path.as_deref(), &name)?;
    std::fs::write(&target, contents)
        .map_err(|e| format!("could not write {}: {e}", target.display()))?;
    Ok(target.display().to_string())
}

/// Open a pty and stream it to the web view under `id`.
///
/// `command` is what to run — `tmux attach -t …` for the Agent pane, a login
/// shell for the terminal panel, or `ssh -t <target> tmux …` when the daemon is
/// describing another machine (`R-I6`). The caller decides; this only spawns
/// what it is handed, which is the same division the egui client had.
///
/// `async`, like every command here: a non-async Tauri command runs on the
/// main thread, and the main thread is the window. Spawning a process there
/// is a stutter; blocking there on a full pty is a hang.
#[tauri::command]
async fn pty_open(
    app: tauri::AppHandle,
    state: tauri::State<'_, Ptys>,
    id: String,
    command: Vec<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    trace("open", &id, &format!("{cols}x{rows} {command:?}"));
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

    // Two threads, not one: the reader feeds a channel, and the emitter
    // drains it. Emitting is the expensive half — every `pty:data` is JSON
    // through Tauri IPC into the webview's main thread — and a busy TUI
    // redrawing itself produces a stream of 8 KiB reads. With one thread per
    // read per emit, the webview choked on the message rate. The emitter
    // instead batches whatever accumulated while the previous emit was in
    // flight: zero added latency when output is light (a keystroke echo is
    // sent the moment it arrives), automatic coalescing exactly when output
    // is heavy.
    let emit_id = id.clone();
    let reader_stop = Arc::clone(&stop);
    let emitter_stop = Arc::clone(&stop);
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    // Checked *after* the read, because the read is where this
                    // thread spends its life: by the time bytes arrive the pane
                    // may be long gone and its id given to another session.
                    if reader_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
        // Reaped before `tx` drops, so the emitter's "channel closed" below
        // means the child is truly gone, not merely quiet.
        let _ = child.wait();
    });
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut bytes = first;
            // Take whatever else is already waiting, without blocking — the
            // batch grows only while the pty outpaces the webview. Capped so
            // a firehose becomes several large messages, not one enormous one.
            while bytes.len() < 512 * 1024 {
                match rx.try_recv() {
                    Ok(more) => bytes.extend_from_slice(&more),
                    Err(_) => break,
                }
            }
            if emitter_stop.load(Ordering::SeqCst) {
                return;
            }
            // Lossy on purpose: a pty carries bytes, and a partial multi-byte
            // sequence at a chunk boundary must not take the stream down.
            // Degrade, never panic — the rule the transcript parsers already
            // follow.
            let data = String::from_utf8_lossy(&bytes).to_string();
            if app.emit("pty:data", Chunk { id: emit_id.clone(), data }).is_err() {
                return;
            }
        }
        // A pane that was replaced does not want an "it closed" for an id that
        // now belongs to something else.
        if !emitter_stop.load(Ordering::SeqCst) {
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

/// Called once per keystroke. `async` is what keeps that keystroke off the
/// main thread — a wedged pty whose buffer is full blocks this write, and
/// before the change it blocked the whole window with it.
#[tauri::command]
async fn pty_write(
    state: tauri::State<'_, Ptys>,
    id: String,
    data: String,
    origin: Option<String>,
) -> Result<(), String> {
    trace(
        "write",
        &id,
        &format!(
            "from={} bytes={:?}",
            origin.as_deref().unwrap_or("?"),
            data
        ),
    );
    let mut ptys = state.0.lock().unwrap();
    let pty = ptys.get_mut(&id).ok_or("no such terminal")?;
    pty.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    pty.writer.flush().map_err(|e| e.to_string())
}

#[tauri::command]
async fn pty_resize(state: tauri::State<'_, Ptys>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    trace("resize", &id, &format!("{cols}x{rows}"));
    let ptys = state.0.lock().unwrap();
    let pty = ptys.get(&id).ok_or("no such terminal")?;
    pty.master
        .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())
}

/// Drop the pty. For a tmux-backed session this **detaches**; the session keeps
/// running and is reachable from any terminal. That is the whole of ADR-0010.
#[tauri::command]
async fn pty_close(state: tauri::State<'_, Ptys>, id: String) -> Result<(), String> {
    trace("close", &id, "");
    state.0.lock().unwrap().remove(&id);
    Ok(())
}

/// This machine's id, so the client can tell a local daemon from a remote one
/// by identity rather than by the address it dialled (`R-I5`). Written once by
/// the daemon; read here, never invented — an id we made up would answer the
/// question wrongly and confidently.
#[tauri::command]
async fn machine_id() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    std::fs::read_to_string(format!("{home}/.mogeung/machine-id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Open a **loopback** URL in the system browser. `R-O10`.
///
/// The one thing this shell can launch that is not a pty, and it is deliberately
/// the narrowest possible version of that: `http://127.0.0.1:<port>/…` and
/// nothing else. It exists for one button — llmproxy's admin interface, whose
/// port is random and therefore unguessable — and adding the general opener
/// plugin instead would have handed the webview *"open anything"*, which is a
/// different capability entirely from *"open the local page we just told you
/// about"*.
///
/// The check is on the parsed **host**, not on a string prefix: `http://127.0.0.1.evil.com/`
/// starts with the right characters and is not this machine. Refusing rather
/// than sanitising, because there is exactly one caller and it has a valid URL.
#[tauri::command]
async fn open_local_url(url: String) -> Result<(), String> {
    is_loopback_http(&url)?;

    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(target_os = "windows")]
    let program = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let program = "xdg-open";

    std::process::Command::new(program)
        .arg(&url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open {url}: {e}"))
}

/// The whole of `open_local_url`'s decision, split out so it can be tested
/// without launching a browser.
fn is_loopback_http(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or("only http:// is opened here")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // `user@host` first, then strip a port. Both matter: `evil.com@127.0.0.1`
    // and `127.0.0.1.evil.com:80` are the two shapes that read as loopback to
    // a careless check and are not.
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
    let local = host == "localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false);
    if local {
        Ok(())
    } else {
        Err(format!("{host} is not this machine — refusing to open it"))
    }
}

#[cfg(test)]
mod open_url_tests {
    use super::is_loopback_http;

    /// This is the narrowest opener that does the job, and the test is what
    /// keeps it narrow. `R-O10`. Every refusal here is a URL that reads as
    /// loopback to a string-prefix check and is not.
    #[test]
    fn only_this_machine_over_plain_http() {
        for ok in [
            "http://127.0.0.1:41235/",
            "http://localhost:8080/admin",
            "http://[::1]:41235/",
            "http://127.0.0.1:41235/x?y=1#z",
        ] {
            assert!(is_loopback_http(ok).is_ok(), "{ok} is this machine");
        }
        for bad in [
            "http://127.0.0.1.evil.com/",     // prefix that is not the host
            "http://evil.com@127.0.0.1.x/",   // userinfo hiding the real host
            "http://example.com/",
            "https://127.0.0.1/",             // only http, so there is one shape to check
            "file:///etc/passwd",
            "javascript:alert(1)",
            "",
        ] {
            assert!(is_loopback_http(bad).is_err(), "{bad} must be refused");
        }
    }
}

/// Attach to a running daemon, or take the port and host one. `R-?`/ADR-0009.
///
/// Called by the window before it connects, and **idempotent**: the answer is
/// computed once and replayed, because a reconnect must not try to bind a port
/// this process is already serving on.
/// `async` matters most here: the attached path holds a TCP connect and a
/// read with 1.5 s timeouts each — up to ~3 s of frozen window at startup
/// when it ran on the main thread.
#[tauri::command]
async fn daemon_acquire(state: tauri::State<'_, DaemonOnce>, addr: String) -> Result<daemon::Status, ()> {
    let mut held = state.0.lock().unwrap();
    if let Some(status) = held.as_ref() {
        return Ok(status.clone());
    }
    let status = daemon::acquire(&addr);
    eprintln!("{}", status.detail(&addr));
    *held = Some(status.clone());
    Ok(status)
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
        // The save dialog, and **only** the dialog: the plugin picks a path and
        // this shell's own `export_text` does the writing. Adding the fs plugin
        // instead would have given the webview a general write verb, where this
        // way the only file it can write is one you named in a native picker.
        .plugin(tauri_plugin_dialog::init())
        .manage(Ptys::default())
        .manage(DaemonOnce::default())
        .invoke_handler(tauri::generate_handler![
            pty_open,
            pty_write,
            pty_resize,
            pty_close,
            export_text,
            machine_id,
            open_local_url,
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
        // `build` then `run`, rather than `run(context)`, for one event: on
        // exit, stop the llmproxy this window started. The hosted daemon's own cleanup is
        // unreachable — it is handed a shutdown future that never resolves
        // (ADR-0009) — so without this the proxy outlives the window that
        // started it, holding a borrowed OAuth token on a known port. `R-O10`.
        .build(tauri::generate_context!())
        .expect("error while running mogeung")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                daemon::stop_hosted_proxy();
            }
        });
}
