//! Token burn and rate-limit visibility. Pillar G.
//!
//! Tokens, never dollars — ADR-0005. And no invented quotas: the CLI exposes
//! no rate-limit telemetry at all (a 2026-07-29 sweep of 235 transcripts found
//! none — see feature 0015), so everything here is either a measured count or
//! an estimate that says it is one.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One day's burn across every session. `day` is the local date, `YYYY-MM-DD`,
/// because "how much did I burn today" is a local-calendar question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayBurn {
    pub day: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// Distinct sessions that burned anything that day.
    pub sessions: u32,
}

/// One repo's total burn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoBurn {
    /// Last path segment of the session cwd — a display key, not an identity.
    pub repo: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub sessions: u32,
}

/// One session's total burn. Subagent transcripts are folded into their
/// parent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBurn {
    pub session_id: String,
    pub repo: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// A moment a session hit the five-hour limit, recognised by the synthetic
/// assistant message the CLI writes (`message.model == "<synthetic>"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitHit {
    pub at: DateTime<Utc>,
    pub session_id: String,
    /// The reset phrase from the message, e.g. `8pm (Europe/London)`, when the
    /// text carried one. Verbatim — mogeung does not parse clock times out of
    /// prose it does not control.
    pub resets: Option<String>,
    /// Output tokens burned across *all* sessions in the five hours before the
    /// hit, at hour granularity. The raw material of the warning estimate.
    pub window_tokens_out: u64,
}

/// Everything a client needs to render burn and the limit picture.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageReport {
    /// Newest first, capped.
    pub days: Vec<DayBurn>,
    /// Largest first.
    pub repos: Vec<RepoBurn>,
    /// Largest first, capped.
    pub sessions: Vec<SessionBurn>,
    /// Burn in the trailing five hours, at hour granularity.
    pub window_tokens_in: u64,
    pub window_tokens_out: u64,
    /// Limit hits seen in the corpus, newest first.
    pub limit_hits: Vec<LimitHit>,
    /// Output tokens after which past windows have hit the limit — the
    /// smallest `window_tokens_out` among known hits. **An estimate derived
    /// from history, not a quota**: the CLI publishes no quota, and any UI
    /// showing this number must label it estimated. `None` until a hit has
    /// been observed, because guessing would be worse than silence.
    pub est_window_limit_out: Option<u64>,
    pub generated_at: Option<DateTime<Utc>>,
    pub files_scanned: u32,
    /// Files that could not be read or parsed at all. Non-zero is a health
    /// question, not a rendering question, but the client should say it.
    pub files_skipped: u32,
}

impl UsageReport {
    /// How close the trailing window is to the estimated limit, 0.0–1.0+,
    /// when an estimate exists.
    pub fn window_fraction(&self) -> Option<f64> {
        let est = self.est_window_limit_out?;
        if est == 0 {
            return None;
        }
        Some(self.window_tokens_out as f64 / est as f64)
    }
}
