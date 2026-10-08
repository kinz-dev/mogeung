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
use serde::{Deserialize, Serialize};

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

/// Models with no published rate, **on purpose**.
///
/// The price table's `KNOWN_IGNORED`: a model listed here is expected to be
/// unpriced, so using it raises no alert. `claude-mythos-preview` is on the
/// deprecations page and not on the pricing page — limited access, priced by
/// arrangement — and guessing it costs what Mythos 5 costs is the inference
/// [`STANDARD`] refuses to make.
const KNOWN_UNPRICED: &[&str] = &["claude-mythos-preview"];

/// Should a model with no rate be raised as a gap in the price table?
///
/// Only Anthropic's models, which have a published price somewhere to go and
/// read. A non-Claude model reached through Claude Code — an endpoint behind a
/// proxy — has no Anthropic price and never will, so an alert about it would
/// be one that can never be answered.
pub fn unpriced_is_a_gap(model: &str) -> bool {
    model.starts_with("claude-") && !KNOWN_UNPRICED.contains(&model)
}

// ---------------------------------------------------------------------------
// Reading Anthropic's pricing page. `R-J124`, feature 0050.
//
// **Compare-only.** What is read here is checked against the tables above and
// the differences reported; nothing read here prices anything. That waits on
// A49 and A50 — that the page stays readable, and that its names map to ids —
// which one reading cannot settle.
// ---------------------------------------------------------------------------

/// The host a reading reaches, and the value of the consent that allows it.
pub const PRICING_HOST: &str = "platform.claude.com";
/// The pricing page as markdown — 49 KB, against 943 KB of HTML.
pub const PRICING_URL: &str = "https://platform.claude.com/docs/en/about-claude/pricing.md";
/// The one published list of API ids, which is what checks the name rule.
pub const DEPRECATIONS_URL: &str =
    "https://platform.claude.com/docs/en/about-claude/model-deprecations.md";

/// One model's rate as the pricing page states it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageRate {
    /// The API id, mapped from the page's name and found on the deprecations page.
    pub model: String,
    pub input: f64,
    pub output: f64,
    /// Cache hit ÷ input, from the page's own columns.
    pub cache_read: f64,
    /// A long-prompt row: the prompt size it applies **above**.
    #[serde(default)]
    pub over: Option<u64>,
}

/// What one reading of the two pages found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub standard: Vec<PageRate>,
    pub fast: Vec<PageRate>,
    /// Names on the pricing page that map to no published id — listed, never
    /// guessed. Today: `Claude Haiku 3.5`, whose id predates the naming rule.
    pub unmapped: Vec<String>,
}

/// The lines under a heading, up to the next heading of the same level or
/// higher.
fn section<'a>(md: &'a str, heading: &str) -> Option<&'a str> {
    let level = heading.chars().take_while(|c| *c == '#').count();
    let mut offset = 0usize;
    let mut start = None;
    for line in md.split_inclusive('\n') {
        let text = line.trim_end();
        if let Some(from) = start {
            let hashes = text.chars().take_while(|c| *c == '#').count();
            if hashes > 0 && hashes <= level && text[hashes..].starts_with(' ') {
                return md.get(from..offset);
            }
        } else if text == heading {
            start = Some(offset + line.len());
        }
        offset += line.len();
    }
    start.and_then(|from| md.get(from..))
}

/// The **first** markdown table under a heading: its header, then its body
/// rows, cells trimmed and the `---` rule dropped.
///
/// The first and only the first, so a second table added under the same
/// heading — a note, a comparison — cannot be read as more models.
fn table(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut rows: Vec<Vec<String>> = text
        .lines()
        .map(str::trim)
        .skip_while(|l| !l.starts_with('|'))
        .take_while(|l| l.starts_with('|'))
        .map(|l| {
            l.trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|cells| !cells.iter().all(|c| c.chars().all(|ch| matches!(ch, ':' | '-' | ' '))))
        .collect();
    let header = if rows.is_empty() { Vec::new() } else { rows.remove(0) };
    (header, rows)
}

fn table_rows(text: &str) -> Vec<Vec<String>> {
    table(text).1
}

/// Does a header name these columns, in this order? Each word is looked for
/// in its own cell, ignoring case — `Base input tokens` answers `input`.
///
/// What A49 is about: a table that keeps six columns but reorders them would
/// otherwise be read as prices in the wrong places, and every invariant below
/// could still hold by coincidence for the columns that did not move.
fn header_is(header: &[String], words: &[&str]) -> bool {
    header.len() == words.len()
        && header.iter().zip(words).all(|(cell, w)| cell.to_lowercase().contains(w))
}

/// The first dollar amount in a cell: `$2 / MTok<sup>3</sup>` is 2.
fn dollars(cell: &str) -> Option<f64> {
    let at = cell.find('$')?;
    let num: String = cell[at + 1..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    num.parse().ok()
}

/// A row's model name, without the parenthetical, and the prompt size a
/// long-prompt row applies above.
fn model_cell(cell: &str) -> (String, Option<u64>) {
    let name = cell.split(" (").next().unwrap_or(cell).trim().to_string();
    let over = cell.find("prompts over ").and_then(|at| {
        let digits: String = cell[at + "prompts over ".len()..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ',')
            .filter(|c| *c != ',')
            .collect();
        digits.parse().ok()
    });
    (name, over)
}

/// A page name's API id, **only** if the published list has it.
///
/// `Claude Opus 5.5` → `claude-opus-5-5`; a model before the 4.6 generation
/// is published dated, so `claude-opus-4-1` finds `claude-opus-4-1-20250805`.
/// More than one candidate is as unmapped as none: picking one would be a
/// guess wearing an id.
fn id_for(name: &str, ids: &[String]) -> Option<String> {
    let rest = name.strip_prefix("Claude ")?;
    let base = format!("claude-{}", rest.to_lowercase().replace([' ', '.'], "-"));
    let found: Vec<&String> = ids
        .iter()
        .filter(|id| {
            id.as_str() == base
                || id
                    .strip_prefix(&format!("{base}-"))
                    .is_some_and(|d| d.len() == 8 && d.chars().all(|c| c.is_ascii_digit()))
        })
        .collect();
    match found.as_slice() {
        [one] => Some((*one).clone()),
        _ => None,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

/// Read the pricing page and the deprecations page into a [`Reading`].
///
/// **An error rejects the whole reading.** The page states two invariants on
/// every row — a five-minute write is 1.25× input, an hour write 2× — and a
/// row that breaks them means the columns no longer mean what the parser
/// thinks. Half a table read wrongly is worse than an old one, so the reader
/// says what it found and keeps nothing (`A4`).
pub fn read_pages(pricing: &str, deprecations: &str) -> Result<Reading, String> {
    let ids: Vec<String> = section(deprecations, "## Model status")
        .map(table_rows)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|r| r.first().cloned())
        .filter(|id| id.starts_with("claude-"))
        .collect();
    if ids.len() < 10 {
        return Err(format!(
            "the deprecations page lists {} model id(s), where ten or more were expected — its table may have moved",
            ids.len()
        ));
    }

    let (header, rows) = table(
        section(pricing, "## Model pricing").ok_or("the pricing page has no \"Model pricing\" section")?,
    );
    if !header_is(&header, &["model", "input", "5m", "1h", "hit", "output"]) {
        return Err(format!(
            "the model pricing table's columns are now {header:?} — the reader expects model, \
             input, 5m write, 1h write, cache hit, output"
        ));
    }
    let mut reading = Reading::default();
    for row in rows {
        if row.len() != 6 {
            return Err(format!(
                "a model pricing row has {} columns where six were expected: {:?}",
                row.len(),
                row.first().map(String::as_str).unwrap_or("")
            ));
        }
        let (name, over) = model_cell(&row[0]);
        let money: Vec<Option<f64>> = row[1..].iter().map(|c| dollars(c)).collect();
        let [Some(input), Some(w5), Some(w1h), Some(hit), Some(output)] = money[..] else {
            return Err(format!("{name}: a price cell held no dollar amount"));
        };
        if !close(w5, input * 1.25) || !close(w1h, input * 2.0) {
            return Err(format!(
                "{name}: cache writes of ${w5} and ${w1h} are not 1.25× and 2× its input of ${input} — \
                 the columns no longer mean what the reader expects"
            ));
        }
        if !(input > 0.0 && hit > 0.0 && hit < input) {
            return Err(format!("{name}: a cache hit of ${hit} against input of ${input} is not a discount"));
        }
        let cache_read = (hit / input * 10_000.0).round() / 10_000.0;
        match id_for(&name, &ids) {
            Some(model) => reading.standard.push(PageRate { model, input, output, cache_read, over }),
            None => {
                if !reading.unmapped.contains(&name) {
                    reading.unmapped.push(name)
                }
            }
        }
    }
    if reading.standard.len() < 10 {
        return Err(format!(
            "{} model(s) read from the pricing table, where ten or more were expected",
            reading.standard.len()
        ));
    }

    // Fast mode is optional: a page without the section is a page on which
    // fast mode has ended, not a page that moved.
    let (header, rows) = section(pricing, "### Fast mode pricing").map(table).unwrap_or_default();
    if !rows.is_empty() && !header_is(&header, &["model", "input", "output"]) {
        return Err(format!("the fast mode table's columns are now {header:?}"));
    }
    for row in rows {
        if row.len() != 3 {
            return Err(format!("a fast mode row has {} columns where three were expected", row.len()));
        }
        let (Some(input), Some(output)) = (dollars(&row[1]), dollars(&row[2])) else {
            return Err(format!("{}: a fast mode cell held no dollar amount", row[0]));
        };
        for name in row[0].split(" / ") {
            let (name, _) = model_cell(name);
            let Some(model) = id_for(&name, &ids) else {
                if !reading.unmapped.contains(&name) {
                    reading.unmapped.push(name);
                }
                continue;
            };
            // Caching multiples apply on top of fast mode, the model's own.
            let cache_read = reading
                .standard
                .iter()
                .find(|r| r.model == model && r.over.is_none())
                .map_or(CACHE_READ, |r| r.cache_read);
            reading.fast.push(PageRate { model, input, output, cache_read, over: None });
        }
    }
    Ok(reading)
}

/// Where a reading disagrees with this build's tables, one sentence each.
///
/// Only one direction: a model on the page that this build prices wrongly or
/// not at all. A model this build lists that the page no longer shows is an
/// alias or a retirement, not a price that is wrong.
pub fn differences(r: &Reading) -> Vec<String> {
    let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let usd = |x: f64| format!("${x}");
    let mut out = Vec::new();
    let mut compare = |kind: &str, page: &PageRate, built: Option<(f64, f64, f64)>| match built {
        None => out.push(format!(
            "{}{kind} is on the pricing page at {}/{} and has no rate in this build",
            page.model,
            usd(page.input),
            usd(page.output)
        )),
        Some((input, output, cache)) if !(same(input, page.input) && same(output, page.output) && same(cache, page.cache_read)) => {
            out.push(format!(
                "{}{kind}: the page says {}/{} with cache reads at {}×; this build has {}/{} at {}×",
                page.model,
                usd(page.input),
                usd(page.output),
                page.cache_read,
                usd(input),
                usd(output),
                cache
            ))
        }
        Some(_) => {}
    };
    for page in &r.standard {
        let built = match page.over {
            None => STANDARD.iter().find(|row| row.0 == page.model).map(|&(_, i, o, c)| (i, o, c)),
            Some(over) => LONG
                .iter()
                .find(|row| row.0 == page.model && row.1 == over)
                .map(|&(_, _, i, o, c)| (i, o, c)),
        };
        let kind = page.over.map(|n| format!(" over {n} prompt tokens")).unwrap_or_default();
        compare(&kind, page, built);
    }
    for page in &r.fast {
        let built = FAST.iter().find(|row| row.0 == page.model).map(|&(_, i, o, c)| (i, o, c));
        compare(" (fast)", page, built);
    }
    out
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

    const PAGE: &str = include_str!("../tests/fixtures/pricing-2026-10-08.md");
    const IDS: &str = include_str!("../tests/fixtures/model-deprecations-2026-10-08.md");

    /// **The oracle.** The page as read on 2026-10-08 and the table typed in
    /// by hand from it that morning (`R-J123`) must agree exactly. A hand edit
    /// the page does not support fails this, and so does a parser change that
    /// misreads the page.
    #[test]
    fn todays_page_agrees_with_the_built_in_table() {
        let r = read_pages(PAGE, IDS).unwrap();
        assert_eq!(differences(&r), Vec::<String>::new());
        assert_eq!(r.standard.len(), 20, "{:?}", r.standard);
        assert_eq!(r.fast.len(), 3);
        // Its id is `claude-3-5-haiku-20241022`, from before the name rule.
        assert_eq!(r.unmapped, vec!["Claude Haiku 3.5".to_string()]);
    }

    #[test]
    fn reads_tiers_dated_ids_and_cache_multiples() {
        let r = read_pages(PAGE, IDS).unwrap();
        let find = |m: &str, over: Option<u64>| r.standard.iter().find(|x| x.model == m && x.over == over).unwrap();
        assert_eq!(find("claude-haiku-5-5", Some(100_000)).input, 0.50);
        assert_eq!(find("claude-haiku-5-5", None).input, 0.10);
        assert_eq!(find("claude-opus-4-1-20250805", None).output, 75.0);
        assert_eq!(find("claude-fable-5-1", None).cache_read, 0.025);
        assert_eq!(find("claude-sonnet-5", None).input, 2.0, "the footnote marker must not leak into the number");
        let fast: Vec<_> = r.fast.iter().map(|x| (x.model.as_str(), x.input, x.cache_read)).collect();
        assert!(fast.contains(&("claude-opus-4-8", 10.0, 0.1)), "{fast:?}");
        assert!(fast.contains(&("claude-opus-5-5", 8.0, 0.05)), "{fast:?}");
    }

    /// A price the page has moved is named, with both sides.
    #[test]
    fn a_changed_price_is_reported() {
        let mut r = read_pages(PAGE, IDS).unwrap();
        r.standard.iter_mut().find(|x| x.model == "claude-opus-5-5").unwrap().input = 6.0;
        let d = differences(&r);
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("claude-opus-5-5") && d[0].contains("$6") && d[0].contains("$4"), "{}", d[0]);
    }

    /// A model released after this build: on both pages, so it maps, and
    /// absent here, so it is the difference that matters most.
    #[test]
    fn a_new_model_is_reported() {
        let row = "| Claude Opus 6 | $5 / MTok | $6.25 / MTok | $10 / MTok | $0.50 / MTok | $25 / MTok |\n";
        let page = PAGE.replacen("| Claude Opus 5.5 ", &format!("{row}| Claude Opus 5.5 "), 1);
        let ids = IDS.replacen("| claude-opus-5-5 ", "| claude-opus-6 | Active | N/A | x |\n| claude-opus-5-5 ", 1);
        let d = differences(&read_pages(&page, &ids).unwrap());
        assert_eq!(d, vec!["claude-opus-6 is on the pricing page at $5/$25 and has no rate in this build".to_string()]);
        // Not on the id list: listed, never guessed.
        let r = read_pages(&page, IDS).unwrap();
        assert!(r.unmapped.contains(&"Claude Opus 6".to_string()));
    }

    /// **Rejected whole**: a page whose columns no longer mean what they did.
    #[test]
    fn a_page_that_has_changed_shape_is_rejected() {
        // Fable 5.1's five-minute write, no longer 1.25× its input.
        let bent = PAGE.replacen("$12.50 / MTok", "$15 / MTok", 1);
        let e = read_pages(&bent, IDS).unwrap_err();
        assert!(e.contains("Fable 5.1"), "{e}");

        let gone = PAGE.replace("## Model pricing", "## Prices");
        assert!(read_pages(&gone, IDS).unwrap_err().contains("Model pricing"));

        let narrow = PAGE.replace("| Output tokens          |", "");
        assert!(read_pages(&narrow, IDS).is_err());

        assert!(read_pages(PAGE, "").unwrap_err().contains("deprecations page"));
    }

    /// Never a panic, whatever comes back from the network.
    #[test]
    fn garbage_is_an_error_not_a_panic() {
        for page in ["", "<!DOCTYPE html><html>", "## Model pricing\n| a |\n|---|\n| $ |", "## Model pricing\n\u{1F600}|$|$"] {
            assert!(read_pages(page, IDS).is_err());
        }
    }

    #[test]
    fn only_anthropic_models_are_a_gap() {
        assert!(unpriced_is_a_gap("claude-opus-9"));
        assert!(!unpriced_is_a_gap("claude-mythos-preview"));
        assert!(!unpriced_is_a_gap("qwen3.8-sglang"));
        assert!(!unpriced_is_a_gap("?"));
    }
}
