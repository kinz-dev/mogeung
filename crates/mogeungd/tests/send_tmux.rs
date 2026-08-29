//! Does the text actually land in the pane, and does Enter actually end the
//! line? `R-B54`, [ADR-0035](../../../docs/decisions/0035-a-human-may-press-send.md).
//!
//! The unit tests in `send.rs` pin the argv, which is the part that is silently
//! wrong when it is wrong — `-p` and `-d` are one character each. They cannot
//! tell you whether tmux does what the flags say. This can, and it costs a
//! throwaway tmux session.
//!
//! **It never touches a session mogeung knows about.** It makes its own, sends
//! into that, and kills it. An agent's input is not a thing a test suite gets to
//! write to, which is the same rule the feature itself is built on.
//!
//! Skipped, not failed, where there is no tmux: this suite is meant to be free
//! everywhere, and a missing multiplexer is a machine fact rather than a defect.
//!
//! **What running it corrected, twice.** The first version killed the session
//! the instant `send` returned and read an empty file — tmux queues input into
//! the pty and returns, so the receiver had not read it yet. The second
//! asserted that `ESC[200~` was in the received bytes, and it was not: tmux's
//! `-p` inserts the bracketed-paste markers **only if the application in the
//! pane has requested bracketed paste mode**, which `cat` never does and a
//! line editor like Claude Code's does. That is a better property than the one
//! asserted for — the flag cannot put escape codes into a program that would
//! not understand them — and it is why this test asserts delivery rather than
//! markers.

use std::process::Command;

fn tmux(args: &[&str]) -> Option<String> {
    let out = Command::new("tmux").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn have_tmux() -> bool {
    tmux(&["-V"]).is_some()
}

#[test]
fn the_text_lands_in_the_pane_and_enter_ends_the_line() {
    if !have_tmux() {
        eprintln!("no tmux on this machine — skipping");
        return;
    }
    let name = format!("mogeung-sendtest-{}", std::process::id());
    let dir = std::env::temp_dir().join(&name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    let landed = dir.join("landed.txt");

    // `cat`, not a shell: it writes exactly what it is given and flushes on the
    // newline, so the file is evidence of both halves — the paste, and the
    // Enter that ended the line. Detached, its own server-side session,
    // attached to nothing.
    let started = tmux(&[
        "new-session",
        "-d",
        "-s",
        &name,
        "-c",
        &dir.to_string_lossy(),
        &format!("cat > {}", landed.display()),
    ]);
    assert!(started.is_some(), "could not start a throwaway tmux session");

    let target = format!("={name}:0.0");
    let sent = mogeungd::send::send(&target, "mogeung-was-here");

    // Polled **before** the session is killed, and on content rather than on
    // the path. Both halves of that were false failures first: tmux queues
    // input into the pty and returns, so a session killed the instant `send`
    // returns takes the text with it; and `cat > file` creates the file when it
    // starts, so waiting for the path proves nothing.
    let mut got = String::new();
    for _ in 0..50 {
        got = std::fs::read_to_string(&landed).unwrap_or_default();
        if got.contains("mogeung-was-here") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Killed before any assertion: a panic in between would leave a tmux
    // session behind on somebody's machine, and this test is not worth that.
    let _ = tmux(&["kill-session", "-t", &format!("={name}")]);
    let _ = std::fs::remove_dir_all(&dir);
    sent.expect("send");

    // The text: `-t` aimed at the right pane, and the buffer was pasted.
    assert!(got.contains("mogeung-was-here"), "nothing arrived: {got:?}");
    // And it only reached the file because a newline ended the line, which is
    // the `send-keys Enter`. Without it `cat` would still be holding the text.
    assert!(got.contains('\n'), "the Enter never landed: {got:?}");
    // No escape codes reached a program that never asked for them: `-p` is
    // conditional on the application requesting bracketed paste, so it cannot
    // corrupt a receiver that does not speak it. Asserted because the opposite
    // was assumed first.
    assert!(!got.contains('\u{1b}'), "escape codes reached a plain reader: {got:?}");
}

/// A target this daemon never resolved is refused before tmux is asked, so a
/// bad target is a sentence rather than a usage message from a subprocess.
#[test]
fn a_target_that_is_not_ours_is_refused_without_running_tmux() {
    let e = mogeungd::send::send("-C", "echo no").expect_err("should refuse");
    assert!(e.to_string().contains("not a tmux target"), "{e}");
}
