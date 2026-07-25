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
    /// Post a macOS banner via osascript.
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
            let script = format!(
                "display notification {} with title {}",
                applescript_string(&n.body),
                applescript_string(&format!("mogeung — {}", n.title))
            );
            let _ = std::process::Command::new("osascript")
                .arg("-e")
                .arg(script)
                .status();
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

    #[test]
    fn applescript_quoting_survives_a_hostile_prompt() {
        let s = applescript_string("he said \"hi\\bye\"\nand left");
        assert!(!s[1..s.len() - 1].contains('\n'));
        assert_eq!(s, r#""he said \"hi\\bye\" and left""#);
    }
}
