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
    /// Hit the five-hour rate limit and cannot proceed until it resets.
    ///
    /// Below `Failed`: a failure might be fixable right now, a limit is not —
    /// but you still need to know four sessions just went dark at once. `R-G1`.
    RateLimited,
    /// Hit an API error or ended badly.
    Failed,
    /// Alive and idle: it is waiting for you to type something.
    ///
    /// Unlike v0.1's inferred "blocked", this comes straight from Claude Code's
    /// own live registry, so it is fact rather than heuristic.
    AwaitingInput,
    /// Alive, idle, and holding an unanswered tool call: it is sitting on a
    /// permission prompt.
    ///
    /// Ranked above `AwaitingInput` because the work is *already in flight* and
    /// stopped. A session waiting for a new instruction has finished what you
    /// asked; this one cannot finish until you answer. `R-B4`.
    AwaitingPermission,
}

impl AttentionReason {
    /// Higher wins. Gaps are wide enough that within-tier tiebreakers can never
    /// promote a session past a more urgent tier.
    pub fn base_score(&self) -> i64 {
        match self {
            AttentionReason::AwaitingPermission => 1100,
            AttentionReason::AwaitingInput => 1000,
            AttentionReason::Failed => 900,
            AttentionReason::RateLimited => 850,
            AttentionReason::NeedsReview => 800,
            AttentionReason::Stalled => 700,
            AttentionReason::Running => 100,
            AttentionReason::Idle => 0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AttentionReason::AwaitingPermission => "APPROVE",
            AttentionReason::AwaitingInput => "WAITING",
            AttentionReason::Failed => "FAILED",
            AttentionReason::RateLimited => "LIMIT",
            AttentionReason::NeedsReview => "REVIEW",
            AttentionReason::Stalled => "STALLED",
            AttentionReason::Running => "running",
            AttentionReason::Idle => "idle",
        }
    }

    pub fn needs_human(&self) -> bool {
        matches!(
            self,
            AttentionReason::AwaitingPermission
                | AttentionReason::AwaitingInput
                | AttentionReason::Failed
                | AttentionReason::RateLimited
                | AttentionReason::NeedsReview
                | AttentionReason::Stalled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    // Snooze wins over everything, including failure. "Stop telling me about
    // this one" has to mean it, or you would never trust it enough to use it.
    // The row stays visible with a badge; it just stops competing for the top.
    if let Some(left) = s.snooze_remaining(now) {
        return AttentionItem {
            session_id: s.id.clone(),
            reason: AttentionReason::Idle,
            score: AttentionReason::Idle.base_score(),
            detail: format!("snoozed — {} left", fmt_dur(left)),
        };
    }

    let (reason, detail) = if let Some(err) = &s.error {
        (AttentionReason::Failed, err.clone())
    } else if s.alive {
        match s.live_status {
            // A limit-hit session looks idle to the registry, but "waiting for
            // you" would be a lie — no amount of typing helps until the reset.
            _ if s.limit_hit_at.is_some() => {
                let resets = s
                    .limit_resets
                    .as_deref()
                    .map(|r| format!(" — resets {r}"))
                    .unwrap_or_default();
                (
                    AttentionReason::RateLimited,
                    format!("hit the session limit{resets}"),
                )
            }
            // An unanswered tool call means it is blocked on a prompt, not
            // merely finished. Checked before the plain-idle case.
            Some(LiveStatus::Idle) if s.awaiting_permission().is_some() => {
                let tool = s.awaiting_permission().expect("checked above");
                let waited = s.waiting_secs(now).unwrap_or(0);
                let what = if tool.summary.is_empty() {
                    tool.name.clone()
                } else {
                    format!("{}: {}", tool.name, tool.summary)
                };
                (
                    AttentionReason::AwaitingPermission,
                    format!("needs approval for {what} — {}", fmt_dur(waited)),
                )
            }
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
        AttentionReason::AwaitingInput | AttentionReason::AwaitingPermission => {
            s.waiting_secs(now).unwrap_or(0)
        }
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
            announced_dirs: vec![],
        }
    }

    fn open_tool(name: &str, summary: &str, now: DateTime<Utc>) -> crate::session::OpenTool {
        crate::session::OpenTool {
            id: "toolu_1".into(),
            name: name.into(),
            summary: summary.into(),
            at: now,
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

    /// R-B4. Both of these are "idle" to the registry, and they need opposite
    /// responses from you: one wants a decision about work already in flight,
    /// the other wants a new instruction.
    #[test]
    fn a_permission_prompt_outranks_waiting_for_a_new_instruction() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();

        let mut blocked = sess(true, Some(LiveStatus::Idle), 10, now);
        blocked.id = "blocked".into();
        blocked.open_tools = vec![open_tool("Bash", "rm -rf build/", now)];

        // Waiting far longer, and still ranked below: the tier gap is absolute.
        let mut finished = sess(true, Some(LiveStatus::Idle), 9_000, now);
        finished.id = "finished".into();

        let item = classify(&blocked, now, &cfg);
        assert_eq!(item.reason, AttentionReason::AwaitingPermission);
        assert!(
            item.detail.contains("rm -rf build/"),
            "the queue must say what it wants approved, got: {}",
            item.detail
        );

        let ranked = rank(&[finished, blocked], now, &cfg);
        assert_eq!(ranked[0].session_id, "blocked");
    }

    #[test]
    fn an_open_tool_on_a_busy_session_is_just_work_in_progress() {
        let now = Utc::now();
        let mut s = sess(true, Some(LiveStatus::Busy), 5, now);
        s.open_tools = vec![open_tool("Bash", "cargo test", now)];
        // The tool has not come back yet — nobody needs to do anything.
        assert_eq!(
            classify(&s, now, &AttentionConfig::default()).reason,
            AttentionReason::Running
        );
        assert!(s.awaiting_permission().is_none());
    }

    /// R-G1. A limit-hit session must not masquerade as "waiting for you" —
    /// typing at it does nothing until the reset — but it must not be silent
    /// either, because several sessions usually go dark at once.
    #[test]
    fn a_limit_hit_session_is_neither_waiting_nor_idle() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let mut s = sess(true, Some(LiveStatus::Idle), 30, now);
        s.limit_hit_at = Some(now);
        s.limit_resets = Some("8pm (Europe/London)".into());

        let item = classify(&s, now, &cfg);
        assert_eq!(item.reason, AttentionReason::RateLimited);
        assert_eq!(item.reason.label(), "LIMIT");
        assert!(item.reason.needs_human());
        assert!(
            item.detail.contains("resets 8pm"),
            "the queue must say when it comes back: {}",
            item.detail
        );

        // Below a failure (maybe fixable now), above unread review.
        let mut failed = sess(false, None, 30, now);
        failed.error = Some("boom".into());
        let mut review = sess(false, None, 30, now);
        review.files_changed = 2;
        assert!(classify(&failed, now, &cfg).score > item.score);
        assert!(item.score > classify(&review, now, &cfg).score);
    }

    /// R-B5. Snooze has to beat *everything*, or you would never trust it.
    #[test]
    fn snoozing_silences_even_a_failure() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let mut s = sess(true, Some(LiveStatus::Idle), 60, now);
        s.error = Some("api error".into());
        s.snoozed_until = Some(now + chrono::Duration::minutes(10));

        let item = classify(&s, now, &cfg);
        assert_eq!(item.reason, AttentionReason::Idle);
        assert!(!item.reason.needs_human());
        assert!(item.detail.starts_with("snoozed"), "{}", item.detail);
    }

    #[test]
    fn an_expired_snooze_stops_suppressing() {
        let now = Utc::now();
        let mut s = sess(true, Some(LiveStatus::Idle), 60, now);
        s.snoozed_until = Some(now - chrono::Duration::seconds(1));
        assert!(!s.is_snoozed(now));
        assert_eq!(
            classify(&s, now, &AttentionConfig::default()).reason,
            AttentionReason::AwaitingInput
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
