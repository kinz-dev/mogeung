//! Raising or launching the window from the tray.
//!
//! Same shape as the window's own `open_in` (mogeung-ui/src/ui.rs): a pure
//! attempts table, then best-effort spawns where the first success wins.
//! `mogeung` itself attaches to a running daemon and detaches from us at
//! once, so "launch" and "raise" are the same spawn — the window process
//! sorts out which one it is.

use std::path::Path;

/// One way the window might launch: program and arguments.
type Attempt = (String, Vec<String>);

/// The launch attempts, in order. The binary next to this executable goes
/// first — the pair that was built and installed together — then whatever
/// `mogeung` is on PATH. Split from the spawning so the table is testable
/// without a desktop.
pub fn attempts(exe_dir: Option<&Path>) -> Vec<Attempt> {
    let mut list = Vec::new();
    if let Some(dir) = exe_dir {
        list.push((dir.join("mogeung").to_string_lossy().into_owned(), vec![]));
    }
    list.push(("mogeung".to_string(), vec![]));
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
    fn the_sibling_binary_is_preferred_and_path_is_the_fallback() {
        let list = attempts(Some(Path::new("/opt/mogeung/bin")));
        assert_eq!(list[0].0, "/opt/mogeung/bin/mogeung");
        assert_eq!(list.last().unwrap().0, "mogeung");
        // The window detaches itself; the tray passes no arguments that
        // could steer it. Observer of the observer, all the way down.
        assert!(list.iter().all(|(_, args)| args.is_empty()));
    }

    #[test]
    fn no_exe_dir_still_leaves_a_way_in() {
        let list = attempts(None);
        assert_eq!(list, vec![("mogeung".to_string(), vec![])]);
    }
}
