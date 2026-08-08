//! What the tokens **would have cost** at API rates. `R-J21`.
//!
//! [ADR-0024](../../../docs/decisions/0024-equivalent-cost-in-dollars.md)
//! supersedes ADR-0005 and permits a dollar figure here on one condition: it
//! is labelled *equivalent API cost*, never *spend*. On a subscription — which
//! is what every session in this corpus runs on — no money changes hands per
//! token, and a number presented as money charged would be a lie however
//! carefully it were computed.
//!
//! **The staleness is real and is the price of the feature.** ADR-0005's
//! objection was that an embedded price sheet goes out of date, and that has
//! not stopped being true; it is answered rather than refuted, by three
//! things. `RATES_AS_OF` ships beside the numbers and every surface that shows
//! a dollar figure shows that date. A model with no row here is **unpriced**
//! rather than free — its tokens are counted and named, so a new model shows
//! up as a gap in the total instead of silently costing nothing. And rates
//! carry the day they apply to, so a rate that changes does not retroactively
//! rewrite last month.

use crate::usage::TokenSplit;

/// The day these rates were read from Anthropic's pricing, `YYYY-MM-DD`.
///
/// Shown by every client that shows a dollar. Bump it in the same commit that
/// touches a number below — a table whose date lags its contents is worse than
/// one that is honestly out of date.
pub const RATES_AS_OF: &str = "2026-08-08";

/// Dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rate {
    pub input: f64,
    pub output: f64,
}

/// **Cache tokens are not input tokens**, and this is the whole reason the
/// daemon had to learn a four-way split before it could cost anything.
///
/// A cache read is a tenth of fresh input; a five-minute cache write is a
/// quarter more than fresh, and an hour write is double. The old fold summed
/// all three into one `tokens_in`, and a costing built on that figure would
/// have priced every cached read at ten times what it costs. Not a rounding
/// error: this machine's corpus reads **2.14 billion** cached tokens against
/// 627k fresh ones — 3,400 to 1 — which puts the whole history at ~$12,250
/// instead of ~$2,602.
const CACHE_READ: f64 = 0.10;
const CACHE_WRITE_5M: f64 = 1.25;
const CACHE_WRITE_1H: f64 = 2.00;

/// Standard rates, per million tokens.
///
/// **Only models whose rates are published.** Nothing is inferred from a
/// family resemblance: an unlisted model is `None` from `rate`, which the
/// report carries as an unpriced model rather than as zero dollars. Guessing
/// that a `claude-opus-4-5` costs what a `4-6` costs would be inventing a
/// number and presenting it in the same font as a real one.
const STANDARD: &[(&str, f64, f64)] = &[
    ("claude-fable-5", 10.0, 50.0),
    ("claude-mythos-5", 10.0, 50.0),
    ("claude-opus-5", 5.0, 25.0),
    ("claude-opus-4-8", 5.0, 25.0),
    ("claude-opus-4-7", 5.0, 25.0),
    ("claude-opus-4-6", 5.0, 25.0),
    ("claude-sonnet-5", 3.0, 15.0),
    ("claude-sonnet-4-6", 3.0, 15.0),
    ("claude-haiku-4-5", 1.0, 5.0),
];

/// Fast mode is the same model at a different price — Opus 5 and Opus 4.8
/// only, at the Fable tier.
///
/// Nothing in this corpus uses it (`usage.speed` is `standard` on all 9,018
/// messages that carry the field), so this table earns its place by what it
/// prevents rather than what it computes: without it, the day someone presses
/// `/fast` their spend would be reported at half.
const FAST: &[(&str, f64, f64)] = &[("claude-opus-5", 10.0, 50.0), ("claude-opus-4-8", 10.0, 50.0)];

/// Introductory rates, in force **through** the given local day inclusive.
///
/// Priced per day rather than as one current rate, which is why the fold keys
/// by day: an intro rate that expires must not silently reprice the weeks it
/// was actually in force. Sonnet 5 is $2/$10 until 2026-08-31 and $3/$15 after.
const INTRO: &[(&str, f64, f64, &str)] = &[("claude-sonnet-5", 2.0, 10.0, "2026-08-31")];

/// The rate for a model on a given local day, or `None` if we do not price it.
///
/// `day` is `YYYY-MM-DD`, which compares lexicographically in date order — the
/// one thing that format is good for, and why the fold stores it.
pub fn rate(model: &str, fast: bool, day: &str) -> Option<Rate> {
    if let Some(&(_, input, output, until)) = INTRO.iter().find(|(m, ..)| *m == model) {
        if !fast && day <= until {
            return Some(Rate { input, output });
        }
    }
    let table = if fast { FAST } else { STANDARD };
    table
        .iter()
        .find(|(m, ..)| *m == model)
        .map(|&(_, input, output)| Rate { input, output })
}

/// Equivalent API cost in dollars, or `None` when the model has no rate.
///
/// `None` is deliberately not `Some(0.0)`: a caller that cannot tell them
/// apart will report an unpriced model as free, which is the exact failure
/// this whole module is arranged to avoid.
pub fn cost_usd(model: &str, fast: bool, day: &str, t: &TokenSplit) -> Option<f64> {
    let r = rate(model, fast, day)?;
    let per = |tokens: u64, dollars_per_million: f64| tokens as f64 * dollars_per_million / 1e6;
    Some(
        per(t.fresh_in, r.input)
            + per(t.cache_read, r.input * CACHE_READ)
            + per(t.cache_write_5m, r.input * CACHE_WRITE_5M)
            + per(t.cache_write_1h, r.input * CACHE_WRITE_1H)
            + per(t.out, r.output),
    )
}

/// Is this model priced at all? What the client asks to explain a gap.
pub fn is_priced(model: &str, fast: bool) -> bool {
    let table = if fast { FAST } else { STANDARD };
    table.iter().any(|(m, ..)| *m == model)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(fresh: u64, read: u64, w5: u64, w1h: u64, out: u64) -> TokenSplit {
        TokenSplit {
            fresh_in: fresh,
            cache_read: read,
            cache_write_5m: w5,
            cache_write_1h: w1h,
            out,
        }
    }

    /// A million of each bucket, so every multiplier is legible as itself.
    #[test]
    fn prices_each_bucket_at_its_own_multiple() {
        let t = split(1_000_000, 1_000_000, 1_000_000, 1_000_000, 1_000_000);
        // Opus 5: $5 in, $25 out. read 0.5, write5m 6.25, write1h 10.
        let c = cost_usd("claude-opus-5", false, "2026-08-08", &t).unwrap();
        assert!((c - (5.0 + 0.5 + 6.25 + 10.0 + 25.0)).abs() < 1e-9, "{c}");
    }

    /// The bug this split exists to prevent: cached reads billed as fresh
    /// input would overstate a cache-heavy day tenfold.
    #[test]
    fn a_cache_read_is_a_tenth_of_fresh_input() {
        let fresh = cost_usd("claude-opus-5", false, "2026-08-08", &split(1_000_000, 0, 0, 0, 0));
        let read = cost_usd("claude-opus-5", false, "2026-08-08", &split(0, 1_000_000, 0, 0, 0));
        assert!((fresh.unwrap() / read.unwrap() - 10.0).abs() < 1e-9);
    }

    /// An intro rate applies to the days it was in force and no others, which
    /// is why the fold keys by day rather than pricing one running total.
    #[test]
    fn an_intro_rate_expires_without_rewriting_history() {
        let before = rate("claude-sonnet-5", false, "2026-08-31").unwrap();
        let after = rate("claude-sonnet-5", false, "2026-09-01").unwrap();
        assert_eq!(before, Rate { input: 2.0, output: 10.0 });
        assert_eq!(after, Rate { input: 3.0, output: 15.0 });
    }

    /// Fast mode is the same model at double, and only where it exists.
    #[test]
    fn fast_mode_is_priced_where_it_is_offered() {
        assert_eq!(rate("claude-opus-5", true, "2026-08-08").unwrap().input, 10.0);
        assert!(rate("claude-sonnet-5", true, "2026-08-08").is_none());
    }

    /// **Unpriced is not free.** A model we have no rate for must be
    /// distinguishable from one that cost nothing.
    #[test]
    fn an_unknown_model_has_no_price_rather_than_a_zero_one() {
        let t = split(1_000_000, 0, 0, 0, 1_000_000);
        assert_eq!(cost_usd("claude-nonesuch-9", false, "2026-08-08", &t), None);
        assert!(!is_priced("claude-nonesuch-9", false));
    }
}
