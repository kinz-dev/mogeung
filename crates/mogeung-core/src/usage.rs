//! Token burn, per-model cost, and rate-limit visibility. Pillar G.
//!
//! Tokens **and** equivalent dollars since
//! [ADR-0024](../../../docs/decisions/0024-equivalent-cost-in-dollars.md),
//! which supersedes ADR-0005's "tokens, never dollars" — see `pricing.rs` for
//! what the dollar figure means and the three things that keep it honest. No
//! invented quotas either way: the CLI exposes no rate-limit telemetry at all
//! (a 2026-07-29 sweep of 235 transcripts found none — see feature 0015), so
//! everything here is a measured count, a published rate, or an estimate that
//! says it is one.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Tokens split the way they are **priced**, not the way they are counted.
///
/// The four input buckets cost four different amounts (`pricing.rs`), and
/// until `R-J21` this crate had one `tokens_in` that summed three of them.
/// That was fine while the answer was "how many tokens", and wrong the moment
/// the question became "how much" — in this repo's own corpus the cheap bucket
/// is 40× the expensive one, so the collapsed figure prices a day at roughly
/// ten times what it cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSplit {
    /// Uncached input — the full-price bucket.
    pub fresh_in: u64,
    /// Served from cache, a tenth of fresh.
    pub cache_read: u64,
    /// Written to the five-minute cache, a quarter more than fresh.
    pub cache_write_5m: u64,
    /// Written to the one-hour cache, double fresh.
    pub cache_write_1h: u64,
    pub out: u64,
}

impl TokenSplit {
    pub fn add(&mut self, other: &TokenSplit) {
        self.fresh_in += other.fresh_in;
        self.cache_read += other.cache_read;
        self.cache_write_5m += other.cache_write_5m;
        self.cache_write_1h += other.cache_write_1h;
        self.out += other.out;
    }

    /// Every input bucket together — what the old `tokens_in` meant, kept so
    /// the existing charts and the repo/session rollups do not change meaning.
    pub fn total_in(&self) -> u64 {
        self.fresh_in + self.cache_read + self.cache_write_5m + self.cache_write_1h
    }
}

/// What one model burned, and what that would have cost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelBurn {
    /// The id exactly as the transcript spelled it — `claude-opus-5`. Not
    /// normalised or prettified: this is the join key with the price table,
    /// and a display name that drifts from it hides an unpriced model.
    pub model: String,
    /// Fast mode is the same model at a different price, so it is a different
    /// row rather than a flag folded into one.
    pub fast: bool,
    pub tokens: TokenSplit,
    /// Equivalent API cost. `None` means **no published rate**, which is not
    /// the same as zero — the client says so rather than showing a free model.
    pub cost_usd: Option<f64>,
    /// Assistant messages counted, so a model with a tiny share is still
    /// visibly present rather than rounding to nothing.
    pub messages: u64,
}

/// One day's burn across every session. `day` is the local date, `YYYY-MM-DD`,
/// because "how much did I burn today" is a local-calendar question — and
/// because a rate that changed mid-month must apply to the days it was in
/// force rather than to the whole total.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayBurn {
    pub day: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// Distinct sessions that burned anything that day.
    pub sessions: u32,
    /// Largest cost first. Empty for a day whose transcripts named no model.
    pub by_model: Vec<ModelBurn>,
    /// The priced part of `by_model`, summed. A day with unpriced models
    /// reports less than it burned — `unpriced_models` on the report is how
    /// the client knows to say so.
    pub cost_usd: f64,
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
    /// Every model seen, all time, largest cost first. `R-J21`.
    pub models: Vec<ModelBurn>,
    /// Equivalent API cost of everything priced, all time.
    pub cost_usd_total: f64,
    /// Today's, on the local calendar — the number the ask was actually about.
    pub cost_usd_today: f64,
    /// Models seen with no published rate. **Their tokens are in the token
    /// figures and their dollars are in none of the cost figures**, so a
    /// client showing cost must show this too or it is quietly under-reporting.
    pub unpriced_models: Vec<String>,
    /// The day the price table was read, `YYYY-MM-DD`. Every surface that
    /// shows a dollar shows this.
    pub rates_as_of: String,
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
