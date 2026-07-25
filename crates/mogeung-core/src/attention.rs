use crate::session::{LiveStatus, Session, SessionId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Why a session is asking for your attention, in priority order.
///
/// The ordering of this enum *is* the product decision: with several agents
/// running you should never have to decide where to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReason {
    /// Nothing wanted. Sorts last.
    Idle,
    /// Working normally.
    Running,
    /// Alive and busy, but silent long enough to be suspicious.
    Stalled,
    /// Exited, and left changes nobody has read.
    NeedsReview,
    /// Hit an API error or ended badly.
    Failed,
    /// Alive and idle: it is waiting for you to type something.
    ///
    /// Unlike v0.1's inferred "blocked", this comes straight from Claude Code's
    /// own live registry, so it is fact rather than heuristic.
    AwaitingInput,
}

impl AttentionReason {
    /// Higher wins. Gaps are wide enough that within-tier tiebreakers can never
    /// promote a session past a more urgent tier.
    pub fn base_score(&self) -> i64 {
        match self {
            AttentionReason::AwaitingInput => 1000,
            AttentionReason::Failed => 900,
            AttentionReason::NeedsReview => 800,
            AttentionReason::Stalled => 700,
            AttentionReason::Running => 100,
            AttentionReason::Idle => 0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AttentionReason::AwaitingInput => "WAITING",
            AttentionReason::Failed => "FAILED",
            AttentionReason::NeedsReview => "REVIEW",
            AttentionReason::Stalled => "STALLED",
            AttentionReason::Running => "running",
            AttentionReason::Idle => "idle",
        }
    }

    pub fn needs_human(&self) -> bool {
        matches!(
            self,
            AttentionReason::AwaitingInput
                | AttentionReason::Failed
                | AttentionReason::NeedsReview
                | AttentionReason::Stalled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionItem {
    pub session_id: SessionId,
    pub reason: AttentionReason,
    pub score: i64,
    /// One line explaining the ranking, so the heuristic is never a black box.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AttentionConfig {
    /// Silence (seconds) after which a busy session is considered stalled.
    pub stall_secs: i64,
    /// Sessions that exited without touching a file are not worth queueing.
    pub review_needs_changes: bool,
}

impl Default for AttentionConfig {
    fn default() -> Self {
        Self {
            stall_secs: 300,
            review_needs_changes: true,
        }
    }
}

pub fn classify(s: &Session, now: DateTime<Utc>, cfg: &AttentionConfig) -> AttentionItem {
    let silent = s.seconds_since_activity(now);

    let (reason, detail) = if let Some(err) = &s.error {
        (AttentionReason::Failed, err.clone())
    } else if s.alive {
        match s.live_status {
            Some(LiveStatus::Idle) => {
                let waited = s.waiting_secs(now).unwrap_or(0);
                (
                    AttentionReason::AwaitingInput,
                    format!("waiting for you — {}", fmt_dur(waited)),
                )
            }
            _ if silent >= cfg.stall_secs => (
                AttentionReason::Stalled,
                format!("busy but silent for {}", fmt_dur(silent)),
            ),
            _ => (
                AttentionReason::Running,
                s.last_activity
                    .clone()
                    .unwrap_or_else(|| "working".to_string()),
            ),
        }
    } else if s.reviewed {
        (AttentionReason::Idle, "reviewed".to_string())
    } else if s.files_changed == 0 && cfg.review_needs_changes {
        (AttentionReason::Idle, "ended with no changes".to_string())
    } else {
        (
            AttentionReason::NeedsReview,
            format!(
                "{} file(s), +{} -{} unread",
                s.files_changed, s.insertions, s.deletions
            ),
        )
    };

    // Within a tier, the longest wait goes first. Capped so it can never leak
    // into the tier above.
    let waited = match reason {
        AttentionReason::AwaitingInput => s.waiting_secs(now).unwrap_or(0),
        _ => s.duration_secs(now),
    };
    let tiebreak = (waited / 30).clamp(0, 99);

    AttentionItem {
        session_id: s.id.clone(),
        reason,
        score: reason.base_score() + tiebreak,
        detail,
    }
}

pub fn rank(sessions: &[Session], now: DateTime<Utc>, cfg: &AttentionConfig) -> Vec<AttentionItem> {
    let mut items: Vec<AttentionItem> = sessions.iter().map(|s| classify(s, now, cfg)).collect();
    items.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.session_id.cmp(&b.session_id))
    });
    items
}

pub fn fmt_dur(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess(alive: bool, status: Option<LiveStatus>, secs_ago: i64, now: DateTime<Utc>) -> Session {
        Session {
            id: format!("s{secs_ago}"),
            title: None,
            name: None,
            last_prompt: None,
            cwd: "/r".into(),
            repo_root: None,
            git_branch: None,
            pid: alive.then_some(1),
            alive,
            live_status: status,
            version: None,
            started_at: now - chrono::Duration::seconds(secs_ago),
            last_event_at: now - chrono::Duration::seconds(secs_ago),
            status_since: Some(now - chrono::Duration::seconds(secs_ago)),
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
        }
    }

    #[test]
    fn an_idle_live_session_is_waiting_for_you() {
        let now = Utc::now();
        let s = sess(true, Some(LiveStatus::Idle), 30, now);
        assert_eq!(
            classify(&s, now, &AttentionConfig::default()).reason,
            AttentionReason::AwaitingInput
        );
    }

    #[test]
    fn waiting_outranks_everything_else() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let waiting = sess(true, Some(LiveStatus::Idle), 5, now);

        let mut ancient_review = sess(false, None, 100_000, now);
        ancient_review.files_changed = 40;
        let mut failed = sess(false, None, 50_000, now);
        failed.error = Some("api error".into());

        let ranked = rank(&[ancient_review, failed, waiting.clone()], now, &cfg);
        assert_eq!(ranked[0].session_id, waiting.id);
        assert_eq!(ranked[0].reason, AttentionReason::AwaitingInput);
    }

    #[test]
    fn a_busy_but_silent_session_is_stalled() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let s = sess(true, Some(LiveStatus::Busy), cfg.stall_secs + 60, now);
        assert_eq!(classify(&s, now, &cfg).reason, AttentionReason::Stalled);
    }

    #[test]
    fn a_busy_recent_session_is_merely_running() {
        let now = Utc::now();
        let s = sess(true, Some(LiveStatus::Busy), 5, now);
        let item = classify(&s, now, &AttentionConfig::default());
        assert_eq!(item.reason, AttentionReason::Running);
        assert!(!item.reason.needs_human());
    }

    #[test]
    fn an_exited_session_with_changes_wants_review() {
        let now = Utc::now();
        let mut s = sess(false, None, 60, now);
        s.files_changed = 3;
        assert_eq!(
            classify(&s, now, &AttentionConfig::default()).reason,
            AttentionReason::NeedsReview
        );
    }

    #[test]
    fn an_exited_session_that_changed_nothing_stays_quiet() {
        let now = Utc::now();
        let s = sess(false, None, 60, now);
        assert_eq!(
            classify(&s, now, &AttentionConfig::default()).reason,
            AttentionReason::Idle
        );
    }

    #[test]
    fn longer_waits_sort_first_within_a_tier() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let brief = sess(true, Some(LiveStatus::Idle), 30, now);
        let long = sess(true, Some(LiveStatus::Idle), 3600, now);
        let ranked = rank(&[brief.clone(), long.clone()], now, &cfg);
        assert_eq!(ranked[0].session_id, long.id);
    }
}
