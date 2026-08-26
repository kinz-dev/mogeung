//! Telling you a session needs attention when you are not looking at mogeung.
//!
//! Roadmap `R-C1` (macOS notification) and `R-C4` (push to a phone).
//!
//! The hard part is not delivery, it is **not being annoying**. A notifier that
//! fires on every scan trains you to dismiss it, and then the one that mattered
//! gets dismissed too — the same failure the format canary had to learn (see
//! `docs/design/health-and-canary.md`).
//!
//! So the rule here is: notify on the *transition* into needing you, once, per
//! session. Never on a state that is merely continuing.

use mogeung_core::attention::{AttentionItem, AttentionReason};
use mogeung_core::session::SessionId;
use std::collections::HashMap;

/// Where a notification should go.
#[derive(Debug, Clone, Default)]
pub struct NotifyConfig {
    /// Post a desktop banner: osascript on macOS, notify-send on Linux.
    pub desktop: bool,
    /// POST the message to this URL (ntfy.sh style: the body *is* the message).
    pub push_url: Option<String>,
}

impl NotifyConfig {
    pub fn enabled(&self) -> bool {
        self.desktop || self.push_url.is_some()
    }
}

/// One thing worth telling the user, already rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub session_id: SessionId,
    pub title: String,
    pub body: String,
}

/// Remembers what it has already said, so it does not repeat itself.
#[derive(Default)]
pub struct Notifier {
    /// Last reason we notified about, per session.
    announced: HashMap<SessionId, AttentionReason>,
    cfg: NotifyConfig,
}

impl Notifier {
    pub fn new(cfg: NotifyConfig) -> Self {
        Notifier {
            announced: HashMap::new(),
            cfg,
        }
    }

    pub fn config(&self) -> &NotifyConfig {
        &self.cfg
    }

    /// Decide what to announce for the current queue.
    ///
    /// Pure: it returns the notifications and updates the seen-state, but sends
    /// nothing. That keeps the interesting logic — *when do we speak?* —
    /// testable without a desktop or a network.
    pub fn diff(&mut self, queue: &[AttentionItem], label_of: impl Fn(&str) -> String) -> Vec<Notification> {
        let mut out = Vec::new();
        let mut seen: HashMap<SessionId, AttentionReason> = HashMap::new();

        for item in queue {
            seen.insert(item.session_id.clone(), item.reason);

            if !item.reason.needs_human() {
                continue;
            }
            // Already told you about this exact state.
            if self.announced.get(&item.session_id) == Some(&item.reason) {
                continue;
            }

            let title = match item.reason {
                AttentionReason::AwaitingPermission => "Needs your approval",
                AttentionReason::AwaitingInput => "Waiting for you",
                AttentionReason::Failed => "Session failed",
                AttentionReason::NeedsReview => "Changes to review",
                AttentionReason::Stalled => "Session has gone quiet",
                _ => continue,
            };
            out.push(Notification {
                session_id: item.session_id.clone(),
                title: title.to_string(),
                body: format!("{} — {}", label_of(&item.session_id), item.detail),
            });
        }

        // Sessions that dropped out of the queue entirely are forgotten, so
        // that a session which needs you *again* later is announced again.
        self.announced = seen;
        out
    }

    /// Deliver. Best-effort and non-blocking-ish: a failed notification must
    /// never disturb the scan loop.
    pub fn send(&self, n: &Notification) {
        if self.cfg.desktop {
            // Runtime-selected (`cfg!`, not `#[cfg]`) so both platforms'
            // argument builders compile — and stay testable — everywhere.
            if cfg!(target_os = "macos") {
                let script = format!(
                    "display notification {} with title {}",
                    applescript_string(&n.body),
                    applescript_string(&format!("mogeung — {}", n.title))
                );
                let _ = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(script)
                    .status();
            } else {
                // notify-send takes arguments, not code: no shell is involved
                // and nothing is escaped, because nothing is interpreted.
                match std::process::Command::new("notify-send")
                    .args(notify_send_args(&n.title, &n.body))
                    .spawn()
                {
                    // Reaped off-thread so a slow notification daemon cannot
                    // stall the scan loop, and no zombie is left behind.
                    Ok(mut child) => {
                        std::thread::spawn(move || {
                            let _ = child.wait();
                        });
                    }
                    // notify-send absent (a headless box) is ordinary.
                    Err(_) => {}
                }
            }
        }
        if let Some(url) = &self.cfg.push_url {
            // curl rather than an HTTP client dependency: this is one POST on a
            // rare event, and shelling out cannot poison the async runtime.
            let _ = std::process::Command::new("curl")
                .args([
                    "-fsS",
                    "-m",
                    "10",
                    "-H",
                    &format!("Title: mogeung: {}", n.title),
                    "-d",
                    &n.body,
                    url,
                ])
                .output();
        }
    }
}

/// The argv for a Linux desktop banner. Pure, so the hostile-title test can
/// pin the shape without a desktop.
///
/// The `--` matters: titles come from session labels and transcript detail,
/// and a label starting with `-` would otherwise be read as an option. After
/// it, title and body ride as two verbatim arguments — notify-send has no
/// code to inject into, which is the whole safety argument here.
fn notify_send_args(title: &str, body: &str) -> Vec<String> {
    vec![
        "--app-name=mogeung".to_string(),
        "--".to_string(),
        format!("mogeung — {title}"),
        body.to_string(),
    ]
}

/// Quote a string for AppleScript. Backslashes first, then quotes.
fn applescript_string(s: &str) -> String {
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    // Newlines inside an AppleScript literal are a syntax error.
    let flat = escaped.replace('\n', " ").replace('\r', " ");
    format!("\"{flat}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, reason: AttentionReason) -> AttentionItem {
        AttentionItem {
            session_id: id.into(),
            reason,
            score: reason.base_score(),
            detail: "because".into(),
            since: None,
        }
    }

    fn label(id: &str) -> String {
        format!("session {id}")
    }

    #[test]
    fn announces_a_transition_into_needing_you() {
        let mut n = Notifier::default();
        let out = n.diff(&[item("a", AttentionReason::AwaitingInput)], label);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Waiting for you");
        assert!(out[0].body.contains("session a"));
    }

    /// The rule that makes this usable. Without it, every 1.5s scan re-announces
    /// every waiting session and you turn notifications off within a minute.
    #[test]
    fn does_not_repeat_itself_while_the_state_persists() {
        let mut n = Notifier::default();
        let q = vec![item("a", AttentionReason::AwaitingInput)];
        assert_eq!(n.diff(&q, label).len(), 1);
        for _ in 0..20 {
            assert!(n.diff(&q, label).is_empty());
        }
    }

    #[test]
    fn a_change_of_reason_is_worth_saying() {
        let mut n = Notifier::default();
        assert_eq!(n.diff(&[item("a", AttentionReason::AwaitingInput)], label).len(), 1);
        // Escalated from "waiting for a task" to "blocked on approval".
        let out = n.diff(&[item("a", AttentionReason::AwaitingPermission)], label);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Needs your approval");
    }

    #[test]
    fn going_quiet_then_needing_you_again_is_announced_again() {
        let mut n = Notifier::default();
        assert_eq!(n.diff(&[item("a", AttentionReason::AwaitingInput)], label).len(), 1);
        // Back to work: nothing to say.
        assert!(n.diff(&[item("a", AttentionReason::Running)], label).is_empty());
        // Waiting once more — this is new information.
        assert_eq!(n.diff(&[item("a", AttentionReason::AwaitingInput)], label).len(), 1);
    }

    #[test]
    fn sessions_that_do_not_need_you_are_never_announced() {
        let mut n = Notifier::default();
        let q = vec![
            item("a", AttentionReason::Running),
            item("b", AttentionReason::Idle),
        ];
        assert!(n.diff(&q, label).is_empty());
    }

    /// notify-send receives arguments, not code — so a hostile title must
    /// arrive *verbatim*, one string, inside a single argv slot. Any escaping,
    /// splitting, or shell in the path would show up here as a mismatch.
    #[test]
    fn notify_send_gets_hostile_text_verbatim_with_no_shell() {
        let title = r#"$(rm -rf /) `touch /pwned` "quoted" 'single'"#;
        let body = r#"`id` $(reboot) ; rm -rf / # "all" of it"#;
        let args = notify_send_args(title, body);

        assert_eq!(args.len(), 4, "app-name, --, title, body — nothing more");
        assert_eq!(args[0], "--app-name=mogeung");
        assert_eq!(args[1], "--", "a title starting with `-` must not become an option");
        assert_eq!(args[2], format!("mogeung — {title}"), "title verbatim, one arg");
        assert_eq!(args[3], body, "body verbatim, one arg");

        // No argument is a shell or asks one to interpret anything.
        for a in &args {
            assert_ne!(a, "sh");
            assert_ne!(a, "bash");
            assert_ne!(a, "-c");
        }
        // The hostile text was not escaped into something else: exactly one
        // argument contains each payload, character for character.
        assert_eq!(args.iter().filter(|a| a.contains("$(rm -rf /)")).count(), 1);
        assert_eq!(args.iter().filter(|a| a.contains("`touch /pwned`")).count(), 1);
    }

    #[test]
    fn applescript_quoting_survives_a_hostile_prompt() {
        let s = applescript_string("he said \"hi\\bye\"\nand left");
        assert!(!s[1..s.len() - 1].contains('\n'));
        assert_eq!(s, r#""he said \"hi\\bye\" and left""#);
    }
}
