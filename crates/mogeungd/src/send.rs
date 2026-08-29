//! Deliver text into one session's own tmux pane. `R-B54`,
//! [ADR-0035](../../../docs/decisions/0035-a-human-may-press-send.md).
//!
//! **This is the only code in mogeung that reaches an agent's input**, and the
//! decision that permits it is narrow enough to state here: a human clicks, a
//! human confirms, and the text they are looking at goes into the pane of the
//! session they were reading — once. Nothing reads the reply, and nothing sends
//! twice. There is no path from an agent's output back to an agent's input; that
//! path is what v0.1 was, and [ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md)'s
//! finding about it is carried forward unchanged.
//!
//! ## Why this is not the keystroke injection two ADRs rejected
//!
//! ADR-0008 refused `osascript` keystrokes into whatever happens to be focused —
//! *"a footgun with no good failure mode"* — and it was right, because that
//! mechanism cannot name its target. This one can:
//! [ADR-0010](../../../docs/decisions/0010-attach-a-terminal-never-own-one.md)
//! already resolves which tmux pane belongs to which session, by walking process
//! ancestry, so the text is addressed to `%17` and cannot arrive anywhere else.
//! Focus is not consulted and no keystroke is synthesised.
//!
//! ## Three invocations, and each is load-bearing
//!
//! | | why |
//! | --- | --- |
//! | `load-buffer -b mogeung -` | the text arrives on **stdin**, so no shell, no quoting, and no length limit worth worrying about |
//! | `paste-buffer -b mogeung -t <pane> -d -p` | `-p` is **bracketed paste**: the whole block arrives as one paste rather than as lines a TUI could read as separate answers. `-d` drops our buffer so the user's paste stack is untouched |
//!
//! `-p` is conditional in tmux — the markers are inserted **only if the program
//! in the pane has requested bracketed paste mode**. That is better than it
//! sounds and `tests/send_tmux.rs` had to be corrected to say so: a line editor
//! (Claude Code's, and every shell with one) gets its block, and a program that
//! has never heard of the sequence is handed plain bytes rather than escape
//! codes it would print.
//! | `send-keys -t <pane> Enter` | the commit, separate on purpose — bracketed paste deliberately does not submit, so this is the one line to delete if ADR-0035's *revisit if* ever fires |
//!
//! The hazard ADR-0035 states rather than hides: mogeung cannot see the pane's
//! screen (a TUI's prompts never reach the transcript), so an `Enter` can land
//! on a menu. Bracketed paste narrows it to one keystroke's worth of risk; the
//! window's confirmation carries the rest.

use anyhow::{Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

/// May a daemon on this address type into a session? ADR-0035 clause 4.
///
/// A pure function of the **bind address**, in the shape `runs_allowed` and
/// `chat_allowed` already have, so that start-up and the per-request gate cannot
/// come to disagree — and taking no token, unlike the write family: a shared
/// secret on a LAN is not a person at this machine.
pub fn allowed(addr: &std::net::SocketAddr) -> bool {
    addr.ip().is_loopback()
}

/// Our own paste buffer, so the user's `prefix ]` still pastes what they copied.
const BUFFER: &str = "mogeung-send";

/// Is this a tmux target we resolved, rather than something shaped like a flag?
///
/// The call is argv, not a shell, so there is no injection to fear — but a
/// target beginning `-` would be read by tmux as an option, and a target with
/// whitespace in it is not one this daemon ever produced. Refusing early makes
/// the failure a sentence instead of a tmux usage message.
pub fn valid_target(target: &str) -> bool {
    !target.is_empty()
        && !target.starts_with('-')
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_:.%@=/".contains(c))
}

/// The three invocations, composed purely so a test can pin them on a machine
/// with no tmux.
pub fn argv(target: &str) -> [Vec<String>; 3] {
    [
        vec!["load-buffer".into(), "-b".into(), BUFFER.into(), "-".into()],
        vec![
            "paste-buffer".into(),
            "-b".into(),
            BUFFER.into(),
            "-t".into(),
            target.into(),
            "-d".into(),
            "-p".into(),
        ],
        vec!["send-keys".into(), "-t".into(), target.into(), "Enter".into()],
    ]
}

/// Put `text` in `target`'s input and press Enter. Once.
pub fn send(target: &str, text: &str) -> Result<()> {
    if text.trim().is_empty() {
        bail!("there is nothing to send");
    }
    if !valid_target(target) {
        bail!("that is not a tmux target this daemon resolved: {target}");
    }
    let [load, paste, enter] = argv(target);

    // stdin, so the text never becomes an argument or a shell word.
    let mut child = Command::new("tmux")
        .args(&load)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("tmux is not runnable here: {e}"))?;
    child
        .stdin
        .as_mut()
        .expect("piped")
        .write_all(text.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("tmux would not take the text: {}", tail(&out.stderr));
    }

    // From here a failure has to say **whether the text landed**, because the
    // two halves are not the same news: a failed paste left the session
    // untouched, and a failed Enter left your instruction sitting in its input
    // box waiting for you to press it.
    let pasted = Command::new("tmux").args(&paste).output()?;
    if !pasted.status.success() {
        bail!(
            "nothing was sent — tmux refused the paste into {target}: {}",
            tail(&pasted.stderr)
        );
    }
    let pressed = Command::new("tmux").args(&enter).output()?;
    if !pressed.status.success() {
        bail!(
            "the text is in {target}'s input but Enter was refused — press it yourself: {}",
            tail(&pressed.stderr)
        );
    }
    Ok(())
}

/// The last line of a tmux complaint, which is the useful one.
fn tail(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .last()
        .unwrap_or("no reason given")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bracketed paste is the whole safety argument for multi-line text, and
    /// `-d` is what keeps the user's own paste stack out of it. Pinned because
    /// both are one character and both are silent when wrong.
    #[test]
    fn the_paste_is_bracketed_and_leaves_no_buffer_behind() {
        let [load, paste, enter] = argv("%17");
        assert_eq!(load, ["load-buffer", "-b", BUFFER, "-"]);
        assert!(paste.contains(&"-p".to_string()), "bracketed paste");
        assert!(paste.contains(&"-d".to_string()), "our buffer, not theirs");
        // By adjacency rather than by index: the pane has to be the value of
        // `-t` and not merely present in the line.
        assert!(paste.windows(2).any(|w| w[0] == "-t" && w[1] == "%17"));
        // Separate, and the one line to delete if the confirmation ever stops
        // being read (ADR-0035's *revisit if*).
        assert_eq!(enter, ["send-keys", "-t", "%17", "Enter"]);
    }

    /// The gate ADR-0035 clause 4 is, and the reason it is not `may_write`:
    /// that one is satisfied by a token, and this door is not.
    #[test]
    fn only_a_loopback_daemon_may_type_into_a_session() {
        let at = |s: &str| allowed(&s.parse().expect("addr"));
        assert!(at("127.0.0.1:7717"));
        assert!(at("[::1]:7717"));
        assert!(!at("0.0.0.0:7717"), "reaches the network, so it reaches your agents");
        assert!(!at("192.168.1.9:7717"));
    }

    #[test]
    fn a_target_shaped_like_a_flag_is_refused() {
        assert!(valid_target("%17"));
        assert!(valid_target("mogeung-mogeung:0.1"));
        assert!(!valid_target("-C"), "tmux would read this as an option");
        assert!(!valid_target(""));
        assert!(!valid_target("a b"), "no target this daemon resolves has a space");
    }

    /// Sending nothing is a bug in the caller, and doing it would press Enter
    /// in somebody's agent for no reason.
    #[test]
    fn empty_text_is_refused_before_tmux_is_touched() {
        let e = send("%17", "  \n ").expect_err("should refuse");
        assert!(e.to_string().contains("nothing to send"));
    }
}
