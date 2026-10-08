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
pub const RATES_AS_OF: &str = "2026-10-08";

/// Dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rate {
    pub input: f64,
    pub output: f64,
    /// What a cache read costs, as a multiple of `input`.
    ///
    /// **Per model since Fable 5.1**, which reads its cache at 0.025× and is
    /// why this stopped being one constant: Opus 5.5 and Sonnet 5.5 read at
    /// 0.05×, everything else at 0.1×. Left as a constant, the two models this
    /// machine uses most would have had every cached read priced at two to four
    /// times what it costs — and cached reads are most of what it reads.
    pub cache_read: f64,
}

/// **Cache tokens are not input tokens**, and this is the whole reason the
/// daemon had to learn a four-way split before it could cost anything.
///
/// A five-minute cache write is a quarter more than fresh input, and an hour
/// write is double — the same on every model. A cache read is a fraction of
/// fresh input that varies by model ([`Rate::cache_read`]). The old fold
/// summed all three into one `tokens_in`, and a costing built on that figure
/// would have priced every cached read as fresh. Not a rounding error: this
/// machine's corpus reads **2.14 billion** cached tokens against 627k fresh
/// ones — 3,400 to 1 — which puts the whole history at ~$12,250 instead of
/// ~$2,602.
const CACHE_READ: f64 = 0.10;
const CACHE_WRITE_5M: f64 = 1.25;
const CACHE_WRITE_1H: f64 = 2.00;

/// Standard rates: model id, input and output per million tokens, cache-read
/// multiple.
///
/// **Only models whose rates are published**, under the ids the API writes.
/// Nothing is inferred from a family resemblance: an unlisted model is `None`
/// from `rate`, which the report carries as an unpriced model rather than as
/// zero dollars. Guessing that `claude-mythos-preview` costs what `mythos-5`
/// costs would be inventing a number and presenting it in the same font as a
/// real one.
///
/// **Models before the 4.6 generation are dated**, and the dated id is what a
/// transcript records — `claude-haiku-4-5-20251001`, not `claude-haiku-4-5`.
/// Only the alias was here until 2026-10-08, so 126 Haiku 4.5 messages went
/// unpriced. Both spellings are listed; the ids are copied from Anthropic's
/// model-deprecations table rather than from a pattern.
const STANDARD: &[(&str, f64, f64, f64)] = &[
    ("claude-fable-5-1", 10.0, 50.0, 0.025),
    ("claude-mythos-5-1", 10.0, 50.0, 0.025),
    ("claude-fable-5", 10.0, 50.0, CACHE_READ),
    ("claude-mythos-5", 10.0, 50.0, CACHE_READ),
    ("claude-opus-5-5", 4.0, 20.0, 0.05),
    ("claude-opus-5", 5.0, 25.0, CACHE_READ),
    ("claude-opus-4-8", 5.0, 25.0, CACHE_READ),
    ("claude-opus-4-7", 5.0, 25.0, CACHE_READ),
    ("claude-opus-4-6", 5.0, 25.0, CACHE_READ),
    ("claude-opus-4-5", 5.0, 25.0, CACHE_READ),
    ("claude-opus-4-5-20251101", 5.0, 25.0, CACHE_READ),
    ("claude-opus-4-1", 15.0, 75.0, CACHE_READ),
    ("claude-opus-4-1-20250805", 15.0, 75.0, CACHE_READ),
    ("claude-opus-4-0", 15.0, 75.0, CACHE_READ),
    ("claude-opus-4-20250514", 15.0, 75.0, CACHE_READ),
    ("claude-sonnet-5-5", 2.0, 10.0, 0.05),
    // $2/$10 was announced as introductory through 2026-08-31, then made the
    // standard price before the rise to $3/$15 happened. See `INTRO`.
    ("claude-sonnet-5", 2.0, 10.0, CACHE_READ),
    ("claude-sonnet-4-6", 3.0, 15.0, CACHE_READ),
    ("claude-sonnet-4-5", 3.0, 15.0, CACHE_READ),
    ("claude-sonnet-4-5-20250929", 3.0, 15.0, CACHE_READ),
    ("claude-sonnet-4-0", 3.0, 15.0, CACHE_READ),
    ("claude-sonnet-4-20250514", 3.0, 15.0, CACHE_READ),
    // Haiku 5.5 is priced by prompt length; this is the short tier. See `LONG`.
    ("claude-haiku-5-5", 0.10, 0.50, CACHE_READ),
    ("claude-haiku-4-5", 1.0, 5.0, CACHE_READ),
    ("claude-haiku-4-5-20251001", 1.0, 5.0, CACHE_READ),
    ("claude-3-5-haiku-20241022", 0.80, 4.0, CACHE_READ),
];

/// Fast mode is the same model at a different price — Opus 5.5, Opus 5 and
/// Opus 4.8 only. Caching multiples apply on top, the model's own.
///
/// Nothing in this corpus uses it (`usage.speed` is `standard` on all 57,801
/// messages that carry the field, 2026-10-08), so this table earns its place
/// by what it prevents rather than what it computes: without it, the day
/// someone presses `/fast` their spend would be reported at half.
const FAST: &[(&str, f64, f64, f64)] = &[
    ("claude-opus-5-5", 8.0, 40.0, 0.05),
    ("claude-opus-5", 10.0, 50.0, CACHE_READ),
    ("claude-opus-4-8", 10.0, 50.0, CACHE_READ),
];

/// Rates that depend on how long the prompt is: model, the prompt size above
/// which they apply, input, output, cache-read multiple.
///
/// Haiku 5.5 alone, and alone among current models it does not price its
/// whole context window flat: a prompt **over** 100,000 tokens pays $0.50/$2.50
/// rather than $0.10/$0.50, on every token of the request. That is decided per
/// message, so the fold asks [`long_prompt`] while it still has one message in
/// hand — a day's total cannot say which of its prompts were long. None of the
/// 135 Haiku 5.5 messages here were, on 2026-10-08; the tier is here so that the
/// first one is not priced at a fifth.
const LONG: &[(&str, u64, f64, f64, f64)] = &[("claude-haiku-5-5", 100_000, 0.50, 2.50, CACHE_READ)];

/// Introductory rates, in force **through** the given local day inclusive:
/// model, input, output, last day.
///
/// Priced per day rather than as one current rate, which is why the fold keys
/// by day: an intro rate that expires must not silently reprice the weeks it
/// was actually in force.
///
/// **Empty since 2026-10-08.** Sonnet 5's $2/$10 was its one row, ending
/// 2026-08-31, and Anthropic then kept that price as the standard one, so the
/// rise to $3/$15 this row priced every day from September at never happened.
/// The mechanism stays for the next launch price; the row went because the
/// expiry it described did not.
const INTRO: &[(&str, f64, f64, &str)] = &[];

/// Does a message of `prompt_tokens` pay `model`'s long-prompt rate?
///
/// `prompt_tokens` is everything the request sent: fresh input, cache reads
/// and cache writes together. `false` for every model without a long tier.
pub fn long_prompt(model: &str, prompt_tokens: u64) -> bool {
    LONG.iter().any(|&(m, over, ..)| m == model && prompt_tokens > over)
}

/// The rate for a model on a given local day, or `None` if we do not price it.
///
/// `day` is `YYYY-MM-DD`, which compares lexicographically in date order — the
/// one thing that format is good for, and why the fold stores it. `long` is
/// [`long_prompt`]'s answer for the messages being priced.
pub fn rate(model: &str, fast: bool, long: bool, day: &str) -> Option<Rate> {
    let row = |(input, output, cache_read)| Rate { input, output, cache_read };
    if !fast {
        if let Some(&(_, input, output, until)) = INTRO.iter().find(|(m, ..)| *m == model) {
            if day <= until {
                return Some(row((input, output, CACHE_READ)));
            }
        }
        if long {
            if let Some(&(_, _, input, output, cache)) = LONG.iter().find(|(m, ..)| *m == model) {
                return Some(row((input, output, cache)));
            }
        }
    }
    let table = if fast { FAST } else { STANDARD };
    table
        .iter()
        .find(|(m, ..)| *m == model)
        .map(|&(_, input, output, cache)| row((input, output, cache)))
}

/// Equivalent API cost in dollars, or `None` when the model has no rate.
///
/// `None` is deliberately not `Some(0.0)`: a caller that cannot tell them
/// apart will report an unpriced model as free, which is the exact failure
/// this whole module is arranged to avoid.
pub fn cost_usd(model: &str, fast: bool, long: bool, day: &str, t: &TokenSplit) -> Option<f64> {
    let r = rate(model, fast, long, day)?;
    let per = |tokens: u64, dollars_per_million: f64| tokens as f64 * dollars_per_million / 1e6;
    Some(
        per(t.fresh_in, r.input)
            + per(t.cache_read, r.input * r.cache_read)
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
        let c = cost_usd("claude-opus-5", false, false, "2026-08-08", &t).unwrap();
        assert!((c - (5.0 + 0.5 + 6.25 + 10.0 + 25.0)).abs() < 1e-9, "{c}");
    }

    /// The bug this split exists to prevent: cached reads billed as fresh
    /// input would overstate a cache-heavy day tenfold.
    #[test]
    fn a_cache_read_is_a_tenth_of_fresh_input() {
        let fresh = cost_usd("claude-opus-5", false, false, "2026-08-08", &split(1_000_000, 0, 0, 0, 0));
        let read = cost_usd("claude-opus-5", false, false, "2026-08-08", &split(0, 1_000_000, 0, 0, 0));
        assert!((fresh.unwrap() / read.unwrap() - 10.0).abs() < 1e-9);
    }

    /// **Except where it is not.** Fable 5.1 reads its cache at 2.5% of input
    /// and Opus 5.5 at 5% — the pricing page's own figures are $0.25 and $0.20
    /// per million, which is what these must come to.
    #[test]
    fn the_newest_models_read_their_cache_cheaper() {
        let read = |m: &str| cost_usd(m, false, false, "2026-10-08", &split(0, 1_000_000, 0, 0, 0)).unwrap();
        assert!((read("claude-fable-5-1") - 0.25).abs() < 1e-9);
        assert!((read("claude-opus-5-5") - 0.20).abs() < 1e-9);
        assert!((read("claude-sonnet-5-5") - 0.10).abs() < 1e-9);
        // Fable 5 is the old multiple: $10 × 0.1.
        assert!((read("claude-fable-5") - 1.0).abs() < 1e-9);
    }

    /// What this machine's transcripts were writing on 2026-10-08 and the
    /// table could not price — every one of them was in `unpriced_models`.
    #[test]
    fn prices_the_models_in_use() {
        for m in [
            "claude-opus-5-5",
            "claude-fable-5-1",
            "claude-sonnet-5-5",
            "claude-haiku-5-5",
            "claude-haiku-4-5-20251001",
        ] {
            assert!(is_priced(m, false), "{m}");
        }
        assert_eq!(rate("claude-opus-5-5", false, false, "2026-10-08").unwrap().input, 4.0);
        assert_eq!(rate("claude-haiku-4-5-20251001", false, false, "2026-10-08").unwrap().output, 5.0);
    }

    /// The rise to $3/$15 was cancelled, so a September day must not pay it.
    /// The table charged it from 2026-09-01 until 2026-10-08.
    #[test]
    fn sonnet_5_stays_at_its_launch_price() {
        for day in ["2026-08-15", "2026-09-01", "2026-10-08"] {
            let r = rate("claude-sonnet-5", false, false, day).unwrap();
            assert_eq!((r.input, r.output), (2.0, 10.0), "{day}");
        }
    }

    /// Haiku 5.5 over 100k prompt tokens pays five times as much, on every token.
    #[test]
    fn a_long_haiku_prompt_pays_the_long_rate() {
        assert!(!long_prompt("claude-haiku-5-5", 100_000));
        assert!(long_prompt("claude-haiku-5-5", 100_001));
        // No other model has a tier, however long the prompt.
        assert!(!long_prompt("claude-opus-5-5", 900_000));
        let short = rate("claude-haiku-5-5", false, false, "2026-10-08").unwrap();
        let long = rate("claude-haiku-5-5", false, true, "2026-10-08").unwrap();
        assert_eq!((short.input, short.output), (0.10, 0.50));
        assert_eq!((long.input, long.output), (0.50, 2.50));
        // A model without a tier ignores the flag rather than going unpriced.
        assert_eq!(rate("claude-opus-5", false, true, "2026-10-08").unwrap().input, 5.0);
    }

    /// Fast mode is the same model at a premium, and only where it exists.
    #[test]
    fn fast_mode_is_priced_where_it_is_offered() {
        assert_eq!(rate("claude-opus-5", true, false, "2026-08-08").unwrap().input, 10.0);
        let fast = rate("claude-opus-5-5", true, false, "2026-10-08").unwrap();
        assert_eq!((fast.input, fast.output, fast.cache_read), (8.0, 40.0, 0.05));
        assert!(rate("claude-sonnet-5", true, false, "2026-08-08").is_none());
    }

    /// **Unpriced is not free.** A model we have no rate for must be
    /// distinguishable from one that cost nothing.
    #[test]
    fn an_unknown_model_has_no_price_rather_than_a_zero_one() {
        let t = split(1_000_000, 0, 0, 0, 1_000_000);
        assert_eq!(cost_usd("claude-nonesuch-9", false, false, "2026-08-08", &t), None);
        assert!(!is_priced("claude-nonesuch-9", false));
        // Not on the pricing page, so not guessed from its family.
        assert!(!is_priced("claude-mythos-preview", false));
    }
}
