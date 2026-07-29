//! Pure derivation from wire messages to what the tray shows.
//!
//! No network and no ksni in this module, so every product decision — what
//! counts as WAITING, what a menu row says — is testable without a desktop.

use mogeung_core::{AttentionItem, AttentionReason, ServerMsg, Session, SessionId};
use std::collections::HashMap;

/// Menu rows get clipped here. A tray menu is a glance, not a reader.
const LABEL_MAX: usize = 48;

/// One waiting session, ready to become a menu row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingEntry {
    pub session_id: SessionId,
    pub label: String,
}

/// The tray's picture of daemon state, folded up one `ServerMsg` at a time.
///
/// The daemon broadcasts every answer to every client, so most of what
/// arrives here — diffs, git pages, file listings for the window — is not
/// ours. `apply` keeps only what the waiting count needs and skips the rest,
/// which is also what makes an unknown or half-parsed message safe: it
/// changes nothing.
#[derive(Default)]
pub struct Model {
    sessions: HashMap<SessionId, Session>,
    queue: Vec<AttentionItem>,
}

impl Model {
    /// Fold one message in. Returns true when the waiting list the tray
    /// renders actually changed.
    pub fn apply(&mut self, msg: &ServerMsg) -> bool {
        let before = self.waiting();
        match msg {
            ServerMsg::Snapshot { sessions, queue } => {
                self.sessions = sessions
                    .iter()
                    .map(|s| (s.id.clone(), s.clone()))
                    .collect();
                self.queue = queue.clone();
            }
            ServerMsg::Queue { queue } => self.queue = queue.clone(),
            ServerMsg::SessionUpdated { session } => {
                self.sessions.insert(session.id.clone(), (**session).clone());
            }
            ServerMsg::SessionRemoved { session_id } => {
                self.sessions.remove(session_id);
                self.queue.retain(|q| &q.session_id != session_id);
            }
            // Everything else on the broadcast is someone else's answer.
            _ => return false,
        }
        self.waiting() != before
    }

    /// The sessions whose attention label is WAITING, in the daemon's own
    /// ranking order.
    ///
    /// WAITING (`AwaitingInput`) only — deliberately not APPROVE, FAILED or
    /// the rest. The spec asks the tray one question, "is anything waiting
    /// for me to type", and a count that quietly means three things stops
    /// being glanceable. If dogfooding says APPROVE belongs here too, this
    /// one match arm is the whole change.
    pub fn waiting(&self) -> Vec<WaitingEntry> {
        self.queue
            .iter()
            .filter(|q| q.reason == AttentionReason::AwaitingInput)
            .map(|q| WaitingEntry {
                session_id: q.session_id.clone(),
                label: clip(&match self.sessions.get(&q.session_id) {
                    Some(s) => s.label(),
                    // A queue row for a session we have not seen yet — a
                    // gap between broadcasts, not an error. Same fallback
                    // shape as `Session::label`.
                    None => format!(
                        "session {}",
                        &q.session_id[..8.min(q.session_id.len())]
                    ),
                }),
            })
            .collect()
    }
}

/// Clip to `LABEL_MAX` characters (not bytes — titles carry any language)
/// with an ellipsis.
fn clip(s: &str) -> String {
    if s.chars().count() <= LABEL_MAX {
        return s.to_string();
    }
    let mut out: String = s.chars().take(LABEL_MAX - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mogeung_core::session::LiveStatus;

    fn sess(id: &str) -> Session {
        let now = Utc::now();
        Session {
            id: id.into(),
            title: None,
            name: None,
            last_prompt: None,
            cwd: "/r".into(),
            repo_root: None,
            git_branch: None,
            pid: Some(1),
            alive: true,
            live_status: Some(LiveStatus::Idle),
            version: None,
            started_at: now,
            last_event_at: now,
            status_since: Some(now),
            turns: 0,
            tool_calls: 0,
            tokens_in: 0,
            tokens_out: 0,
            last_activity: None,
            touched_files: vec![],
            base_sha: None,
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            error: None,
            transcript_path: "/t".into(),
            reviewed: false,
            open_tools: vec![],
            snoozed_until: None,
            collisions: vec![],
            loop_signal: None,
            recent_touches: vec![],
            recent_tools: vec![],
            tmux_target: None,
            limit_hit_at: None,
            limit_resets: None,
            verify_runs: Vec::new(),
            claims: Vec::new(),
            source: Default::default(),
        }
    }

    fn item(id: &str, reason: AttentionReason) -> AttentionItem {
        AttentionItem {
            session_id: id.into(),
            reason,
            score: reason.base_score(),
            detail: String::new(),
        }
    }

    #[test]
    fn only_waiting_sessions_are_counted_and_in_queue_order() {
        let mut m = Model::default();
        let mut a = sess("aaaa1111");
        a.title = Some("refactor the parser".into());
        let b = sess("bbbb2222");
        let mut c = sess("cccc3333");
        c.name = Some("mogeung-95".into());

        let changed = m.apply(&ServerMsg::Snapshot {
            sessions: vec![a, b, c],
            queue: vec![
                item("cccc3333", AttentionReason::AwaitingInput),
                item("bbbb2222", AttentionReason::AwaitingPermission),
                item("aaaa1111", AttentionReason::AwaitingInput),
            ],
        });
        assert!(changed);

        let w = m.waiting();
        assert_eq!(w.len(), 2, "APPROVE is not WAITING");
        // The daemon already ranked the queue; the tray must not re-rank.
        assert_eq!(w[0].session_id, "cccc3333");
        assert_eq!(w[0].label, "mogeung-95");
        assert_eq!(w[1].session_id, "aaaa1111");
        assert_eq!(w[1].label, "refactor the parser");
    }

    #[test]
    fn a_queue_update_changes_the_count_and_reports_it() {
        let mut m = Model::default();
        m.apply(&ServerMsg::Snapshot {
            sessions: vec![sess("aaaa1111")],
            queue: vec![item("aaaa1111", AttentionReason::Running)],
        });
        assert!(m.waiting().is_empty());

        assert!(m.apply(&ServerMsg::Queue {
            queue: vec![item("aaaa1111", AttentionReason::AwaitingInput)],
        }));
        assert_eq!(m.waiting().len(), 1);

        // The same queue again is not a change worth re-rendering.
        assert!(!m.apply(&ServerMsg::Queue {
            queue: vec![item("aaaa1111", AttentionReason::AwaitingInput)],
        }));
    }

    #[test]
    fn foreign_broadcast_traffic_is_skipped() {
        let mut m = Model::default();
        m.apply(&ServerMsg::Queue {
            queue: vec![item("aaaa1111", AttentionReason::AwaitingInput)],
        });
        assert!(!m.apply(&ServerMsg::Error {
            message: "bad command".into(),
        }));
        assert!(!m.apply(&ServerMsg::Events { events: vec![] }));
        assert_eq!(m.waiting().len(), 1, "unrelated messages change nothing");
    }

    #[test]
    fn a_queue_row_without_a_session_still_gets_a_row() {
        let mut m = Model::default();
        m.apply(&ServerMsg::Queue {
            queue: vec![item("deadbeef-cafe", AttentionReason::AwaitingInput)],
        });
        assert_eq!(m.waiting()[0].label, "session deadbeef");
    }

    #[test]
    fn removing_a_session_drops_its_row() {
        let mut m = Model::default();
        m.apply(&ServerMsg::Snapshot {
            sessions: vec![sess("aaaa1111")],
            queue: vec![item("aaaa1111", AttentionReason::AwaitingInput)],
        });
        assert_eq!(m.waiting().len(), 1);
        assert!(m.apply(&ServerMsg::SessionRemoved {
            session_id: "aaaa1111".into(),
        }));
        assert!(m.waiting().is_empty());
    }

    #[test]
    fn long_labels_are_clipped_by_characters_not_bytes() {
        let mut m = Model::default();
        let mut s = sess("aaaa1111");
        s.title = Some("옛 한글 문서를 읽는 세션".repeat(8));
        m.apply(&ServerMsg::Snapshot {
            sessions: vec![s],
            queue: vec![item("aaaa1111", AttentionReason::AwaitingInput)],
        });
        let label = &m.waiting()[0].label;
        assert!(label.ends_with('…'));
        assert!(label.chars().count() <= LABEL_MAX);
    }
}
