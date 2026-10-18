//! Does a seat's process actually get the `PATH` it was handed? `R-P17`.
//!
//! The unit tests in `seat.rs` pin the environment `compose` builds and the
//! argv `tmux_argv` makes from it. Both were right, and a seat still could not
//! find `mogeung-send`: tmux gives a new session's process the `PATH` of the
//! client that asked for it, over `-e PATH=…`, while every other `-e` arrives.
//! Only a real tmux says what reaches the process, so this asks one.
//!
//! **It never touches a session mogeung knows about.** It makes its own, reads
//! what the process inside saw, and kills it. Skipped, not failed, where there
//! is no tmux — the same terms as `send_tmux.rs`.

use std::process::Command;

fn tmux(args: &[String]) -> Option<String> {
    let out = Command::new("tmux").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn have_tmux() -> bool {
    tmux(&["-V".to_string()]).is_some()
}

#[test]
fn a_seats_process_gets_the_path_it_was_handed_and_the_rest_of_its_environment() {
    if !have_tmux() {
        eprintln!("no tmux on this machine — skipping");
        return;
    }
    let name = format!("mogeung-seattest-{}", std::process::id());
    let dir = std::env::temp_dir().join(&name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    let seen = dir.join("seen.txt");

    // What `compose` hands a seat with `curl`: its own `bin` first, then the
    // daemon's `PATH`, and the variables the send command reads.
    let bin = dir.join("bin");
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default());
    let env = vec![("PATH".to_string(), path), ("MOGEUNG_SEAT_URL".to_string(), "http://seat.test".to_string())];
    // A shell in place of the CLI, writing what it was started with. `sleep`
    // keeps the session up until the file is read.
    let argv = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!("printf '%s\\n%s\\n' \"$PATH\" \"$MOGEUNG_SEAT_URL\" > '{}'; sleep 5", seen.display()),
    ];
    let started = tmux(&mogeungd::seat::tmux_argv(&name, "seat", &dir, &env, &argv));
    assert!(started.is_some(), "could not start a throwaway tmux session");

    let mut got = String::new();
    for _ in 0..50 {
        got = std::fs::read_to_string(&seen).unwrap_or_default();
        if got.lines().count() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Killed before any assertion, so a failure leaves nothing behind.
    let _ = tmux(&["kill-session".to_string(), "-t".to_string(), format!("={name}")]);
    let _ = std::fs::remove_dir_all(&dir);

    let mut lines = got.lines();
    let seen_path = lines.next().unwrap_or_default();
    let seen_url = lines.next().unwrap_or_default();
    assert_eq!(seen_url, "http://seat.test", "the other variables arrive by `-e`: {got:?}");
    assert!(
        seen_path.starts_with(&format!("{}:", bin.display())),
        "the seat's own bin is not first on the PATH its process saw: {seen_path:?}"
    );
}
