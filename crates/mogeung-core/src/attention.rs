use crate::run::{Run, RunId, RunStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Why a run is asking for your attention, in priority order.
///
/// The ordering of this enum *is* the product decision (CONCEPT.md A2): with
/// four agents running you should never have to decide where to look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReason {
    /// Nothing wanted. Sorts last.
    Idle,
    /// Working normally.
    Running,
    /// Spending money without producing a diff.
    BurningNoProgress,
    /// Running, but silent for a long time. Probably stuck or waiting on something.
    Stalled,
    /// Finished, diff not yet read by a human.
    NeedsReview,
    /// Exited badly.
    Failed,
    /// Tried to use tools it was not allowed to. It needs a decision from you.
    Blocked,
}

impl AttentionReason {
    /// Base score. Higher wins. Gaps are wide enough that within-tier tiebreakers
    /// (age, cost) can never promote a run past a more urgent tier.
    pub fn base_score(&self) -> i64 {
        match self {
            AttentionReason::Blocked => 1000,
            AttentionReason::Failed => 900,
            AttentionReason::NeedsReview => 800,
            AttentionReason::Stalled => 700,
            AttentionReason::BurningNoProgress => 600,
            AttentionReason::Running => 100,
            AttentionReason::Idle => 0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AttentionReason::Blocked => "BLOCKED",
            AttentionReason::Failed => "FAILED",
            AttentionReason::NeedsReview => "REVIEW",
            AttentionReason::Stalled => "STALLED",
            AttentionReason::BurningNoProgress => "BURNING",
            AttentionReason::Running => "running",
            AttentionReason::Idle => "idle",
        }
    }

    /// Does this reason represent work the human must personally do?
    pub fn needs_human(&self) -> bool {
        matches!(
            self,
            AttentionReason::Blocked
                | AttentionReason::Failed
                | AttentionReason::NeedsReview
                | AttentionReason::Stalled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionItem {
    pub run_id: RunId,
    pub reason: AttentionReason,
    pub score: i64,
    /// One line explaining the ranking, so the heuristic is never a black box.
    pub detail: String,
}

/// Tunables for the ranking heuristics.
///
/// These are hand-written on purpose for v0.1. The right long-term answer is
/// probably to learn them from which runs you actually open, but that needs
/// data we do not have yet — and hand-written rules are debuggable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AttentionConfig {
    /// Silence (in seconds) after which a running run is considered stalled.
    pub stall_secs: i64,
    /// Spend (USD) past which a run with no diff is considered to be burning.
    pub burn_usd: f64,
    /// Minimum runtime before the burn rule can fire, so short runs are exempt.
    pub burn_min_secs: i64,
}

impl Default for AttentionConfig {
    fn default() -> Self {
        Self {
            stall_secs: 120,
            burn_usd: 1.00,
            burn_min_secs: 90,
        }
    }
}

/// Classify a single run.
pub fn classify(run: &Run, now: DateTime<Utc>, cfg: &AttentionConfig) -> AttentionItem {
    let silent = run.seconds_since_activity(now);
    let age = run.duration_secs(now);

    let (reason, detail) = match run.status {
        RunStatus::Failed => (
            AttentionReason::Failed,
            run.error
                .clone()
                .unwrap_or_else(|| "run failed".to_string()),
        ),

        RunStatus::AwaitingReview => {
            // A completed run that hit permission denials is a decision for you,
            // not just a diff to read — rank it above ordinary review.
            if run.permission_denials > 0 {
                (
                    AttentionReason::Blocked,
                    format!(
                        "{} tool call(s) denied — needs a permission decision",
                        run.permission_denials
                    ),
                )
            } else if run.files_changed == 0 {
                (
                    AttentionReason::NeedsReview,
                    "finished with no file changes — check whether that is right".to_string(),
                )
            } else {
                (
                    AttentionReason::NeedsReview,
                    format!(
                        "{} file(s), +{} -{} unread",
                        run.files_changed, run.insertions, run.deletions
                    ),
                )
            }
        }

        RunStatus::Starting | RunStatus::Running => {
            if silent >= cfg.stall_secs {
                (
                    AttentionReason::Stalled,
                    format!("silent for {}", fmt_dur(silent)),
                )
            } else if run.cost_usd >= cfg.burn_usd && !run.has_diff() && age >= cfg.burn_min_secs {
                (
                    AttentionReason::BurningNoProgress,
                    format!("${:.2} spent, still no diff", run.cost_usd),
                )
            } else {
                (
                    AttentionReason::Running,
                    run.last_activity
                        .clone()
                        .unwrap_or_else(|| "working".to_string()),
                )
            }
        }

        RunStatus::Reviewed => (AttentionReason::Idle, "reviewed".to_string()),
        RunStatus::Cancelled => (AttentionReason::Idle, "cancelled".to_string()),
    };

    // Within a tier, older waits first, then more expensive. Capped so it can
    // never leak into the tier above.
    let age_bonus = (age / 30).clamp(0, 60);
    let cost_bonus = (run.cost_usd * 4.0) as i64;
    let tiebreak = if reason.needs_human() {
        (age_bonus + cost_bonus).clamp(0, 99)
    } else {
        age_bonus.clamp(0, 99)
    };

    AttentionItem {
        run_id: run.id,
        reason,
        score: reason.base_score() + tiebreak,
        detail,
    }
}

/// Rank every run into the single queue the UI shows.
pub fn rank(runs: &[Run], now: DateTime<Utc>, cfg: &AttentionConfig) -> Vec<AttentionItem> {
    let mut items: Vec<AttentionItem> = runs.iter().map(|r| classify(r, now, cfg)).collect();
    items.sort_by(|a, b| b.score.cmp(&a.score).then(a.run_id.cmp(&b.run_id)));
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
    use crate::run::{AgentKind, PermissionMode, RunId};

    fn run_at(status: RunStatus, secs_ago: i64, now: DateTime<Utc>) -> Run {
        let id = RunId::new();
        Run {
            id,
            root: id,
            intent: "t".into(),
            agent: AgentKind::ClaudeCode,
            model: None,
            permission_mode: PermissionMode::AcceptEdits,
            repo_path: "/r".into(),
            cwd: "/r".into(),
            worktree: false,
            branch: None,
            base_sha: None,
            session_id: None,
            status,
            created_at: now - chrono::Duration::seconds(secs_ago),
            ended_at: None,
            last_event_at: now - chrono::Duration::seconds(secs_ago),
            last_activity: None,
            cost_usd: 0.0,
            num_turns: 0,
            tokens_in: 0,
            tokens_out: 0,
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            permission_denials: 0,
            error: None,
            parent: None,
        }
    }

    #[test]
    fn a_root_run_is_its_own_root() {
        let now = Utc::now();
        let r = run_at(RunStatus::Running, 1, now);
        assert_eq!(r.id, r.root);
    }

    #[test]
    fn urgent_tiers_outrank_everything_below() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();

        // A brand-new failure must still outrank a very old review.
        let failed = run_at(RunStatus::Failed, 1, now);
        let mut old_review = run_at(RunStatus::AwaitingReview, 100_000, now);
        old_review.cost_usd = 500.0;
        old_review.files_changed = 3;

        let ranked = rank(&[old_review.clone(), failed.clone()], now, &cfg);
        assert_eq!(ranked[0].run_id, failed.id);
        assert_eq!(ranked[0].reason, AttentionReason::Failed);
    }

    #[test]
    fn silence_promotes_a_running_run_to_stalled() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let quiet = run_at(RunStatus::Running, cfg.stall_secs + 10, now);
        let item = classify(&quiet, now, &cfg);
        assert_eq!(item.reason, AttentionReason::Stalled);
    }

    #[test]
    fn denied_tools_rank_as_blocked_not_review() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let mut denied = run_at(RunStatus::AwaitingReview, 5, now);
        denied.permission_denials = 2;
        assert_eq!(classify(&denied, now, &cfg).reason, AttentionReason::Blocked);
    }

    #[test]
    fn spend_without_a_diff_is_flagged() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let mut burning = run_at(RunStatus::Running, cfg.burn_min_secs + 5, now);
        burning.cost_usd = cfg.burn_usd + 0.5;
        burning.last_event_at = now; // not stalled, just expensive
        assert_eq!(
            classify(&burning, now, &cfg).reason,
            AttentionReason::BurningNoProgress
        );
    }

    #[test]
    fn reviewed_runs_leave_the_queue() {
        let now = Utc::now();
        let cfg = AttentionConfig::default();
        let done = run_at(RunStatus::Reviewed, 10, now);
        assert!(!classify(&done, now, &cfg).reason.needs_human());
    }
}
