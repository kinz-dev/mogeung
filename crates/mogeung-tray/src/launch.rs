//! Raising or launching the window from the tray.
//!
//! A pure attempts table, then best-effort spawns where the first success
//! wins. The window attaches to a running daemon and detaches from us at
//! once, so "launch" and "raise" are the same spawn — the window process
//! sorts out which one it is.
//!
//! The window is the Tauri client, `mogeung-desktop`, since the egui one was
//! retired ([ADR-0020](../../../docs/decisions/0020-the-egui-client-is-retired.md)).
//! A packaged install names it `mogeung`, so both names are tried: the tray
//! is a separate binary that can be older or newer than whatever window is
//! on the machine, and guessing wrong costs the tray its only action.

use std::path::Path;

/// One way the window might launch: program and arguments.
type Attempt = (String, Vec<String>);

/// The launch attempts, in order: the binaries next to this executable first
/// — the set that was built and installed together — then whatever is on
/// PATH. Split from the spawning so the table is testable without a desktop.
pub fn attempts(exe_dir: Option<&Path>) -> Vec<Attempt> {
    // `mogeung-desktop` is the cargo binary; `mogeung` is what the bundler
    // installs it as. Neither is more likely than the other, and trying both
    // costs one failed spawn.
    const NAMES: [&str; 2] = ["mogeung-desktop", "mogeung"];
    let mut list = Vec::new();
    if let Some(dir) = exe_dir {
        for name in NAMES {
            list.push((dir.join(name).to_string_lossy().into_owned(), vec![]));
        }
    }
    for name in NAMES {
        list.push((name.to_string(), vec![]));
    }
    list
}

/// Best-effort launch. Returns the error for the caller to log rather than
/// failing silently — a missing window binary is a real thing to say.
pub fn open_window() -> Result<(), String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let list = attempts(exe_dir.as_deref());
    let mut tried = Vec::new();
    for (program, args) in &list {
        match std::process::Command::new(program).args(args).spawn() {
            Ok(mut child) => {
                // Reaped off-thread, or every launch leaves a zombie — the
                // same lesson `open_in` learned live.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            Err(_) => tried.push(program.clone()),
        }
    }
    Err(format!(
        "could not launch the window; tried: {}",
        tried.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::attempts;
    use std::path::Path;

    #[test]
    fn the_sibling_binaries_are_preferred_and_path_is_the_fallback() {
        let list = attempts(Some(Path::new("/opt/mogeung/bin")));
        assert_eq!(list[0].0, "/opt/mogeung/bin/mogeung-desktop");
        assert_eq!(list[1].0, "/opt/mogeung/bin/mogeung");
        assert_eq!(list.last().unwrap().0, "mogeung");
        // The window detaches itself; the tray passes no arguments that
        // could steer it. Observer of the observer, all the way down.
        assert!(list.iter().all(|(_, args)| args.is_empty()));
    }

    #[test]
    fn no_exe_dir_still_leaves_a_way_in() {
        let list = attempts(None);
        assert_eq!(
            list,
            vec![
                ("mogeung-desktop".to_string(), vec![]),
                ("mogeung".to_string(), vec![]),
            ]
        );
    }
}
