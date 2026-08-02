//! Ranked text matching, shared by the panels that search. `R-B36`, `R-F10`.
//!
//! # What was asked for, and what this is
//!
//! The ask (2026-08-02) was four engines in parallel — substring, regex, `rg`
//! and fuzzy — showing whichever answered best. Two of the four are not here,
//! and the reasons are different:
//!
//! - **`rg` and `fzf` are refused.** They are external binaries that may not be
//!   installed, and every other external dependency in this project is either
//!   required up front (`git`) or degrades to a named fallback (`tmux`). A
//!   search box that is fast on the author's machine and silently absent on
//!   anyone else's is worse than one that is merely fast enough everywhere.
//!   Speed was the stated motive, and the corpus here is one transcript or one
//!   result set — small enough that the process boundary would cost more than
//!   the matching does.
//! - **Regex is deferred**, not refused. It needs a new crate; that is a
//!   dependency decision worth making deliberately rather than in passing, and
//!   the two engines below cover what the ask was actually reaching for.
//!
//! # How "best" is decided
//!
//! Four rankings over one corpus do not compose by themselves, which was the
//! open question in `R-F10`. The answer taken here is the narrow one: every
//! engine scores on the **same scale**, the highest score wins, and the winner
//! **says which engine it was**. A search that silently prefers one engine is
//! worse than one that tells you why it matched — and naming the engine is
//! what makes a wrong ranking a bug report rather than a shrug.

/// Which engine produced a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// The query appears verbatim.
    Substring,
    /// The query's characters appear in order, but not adjacently.
    Fuzzy,
}

impl Engine {
    pub fn label(self) -> &'static str {
        match self {
            Engine::Substring => "exact",
            Engine::Fuzzy => "fuzzy",
        }
    }
}

/// One hit, on a scale shared by every engine so they can be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub score: i32,
    pub engine: Engine,
    /// Byte offset of the first matched character, for scrolling to it.
    pub at: usize,
}

/// Smart case, as every tool worth using does it: a lowercase query ignores
/// case, and a query with any uppercase in it means you typed that on purpose.
fn case_sensitive(query: &str) -> bool {
    query.chars().any(|c| c.is_uppercase())
}

/// The best hit for `query` in `hay`, or `None` if nothing matched.
///
/// Runs every engine and keeps the highest score. They are cheap enough that
/// choosing between them up front would cost more than running both.
pub fn best(query: &str, hay: &str) -> Option<Hit> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    [substring(q, hay), fuzzy(q, hay)]
        .into_iter()
        .flatten()
        .max_by_key(|h| h.score)
}

/// Verbatim. Scores highest, and higher still at a word boundary — `cache` in
/// "cache_key" is more likely what you meant than the one inside "uncached".
fn substring(q: &str, hay: &str) -> Option<Hit> {
    let sensitive = case_sensitive(q);
    let (needle, straw) = if sensitive {
        (q.to_string(), hay.to_string())
    } else {
        (q.to_lowercase(), hay.to_lowercase())
    };
    let at = straw.find(&needle)?;
    let mut score = 1000;
    // At the start of a word beats the middle of one.
    let boundary = at == 0
        || straw[..at]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_alphanumeric());
    if boundary {
        score += 200;
    }
    // Earlier is better, but only mildly — a late match is still exact.
    score -= (at.min(400) / 4) as i32;
    Some(Hit {
        score,
        engine: Engine::Substring,
        at,
    })
}

/// Characters in order with gaps, the way a fuzzy finder matches. Scores
/// below any exact hit by construction, so it only ever wins when nothing
/// matched verbatim.
///
/// Compactness is what separates a good fuzzy hit from a coincidence: `gitcmt`
/// against "git_commit" is worth showing, and against "**g**ather **i**nput
/// **t**hen **c**ompute **m**any **t**hings" is not.
fn fuzzy(q: &str, hay: &str) -> Option<Hit> {
    let sensitive = case_sensitive(q);
    let norm = |s: &str| -> Vec<char> {
        if sensitive {
            s.chars().collect()
        } else {
            s.to_lowercase().chars().collect()
        }
    };
    let needle = norm(q);
    let straw = norm(hay);
    if needle.is_empty() || straw.len() < needle.len() {
        return None;
    }

    let mut first: Option<usize> = None;
    let mut last = 0usize;
    let mut ni = 0usize;
    for (i, c) in straw.iter().enumerate() {
        if *c == needle[ni] {
            if first.is_none() {
                first = Some(i);
            }
            last = i;
            ni += 1;
            if ni == needle.len() {
                break;
            }
        }
    }
    if ni < needle.len() {
        return None;
    }
    let first = first?;
    let span = last - first + 1;
    // 900 keeps every fuzzy hit under every substring hit. The penalty is the
    // slack — how much more than the query the match had to stretch over.
    let slack = (span - needle.len()) as i32;
    let score = 900 - slack.min(880);
    // `first` counts characters; callers want a byte offset to scroll to.
    let at = straw
        .iter()
        .take(first)
        .map(|c| c.len_utf8())
        .sum::<usize>();
    Some(Hit {
        score,
        engine: Engine::Fuzzy,
        at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_hit_always_beats_a_fuzzy_one() {
        let exact = best("cache", "the cache is warm").unwrap();
        let fuzz = best("cache", "coalesce and check every entry").unwrap();
        assert_eq!(exact.engine, Engine::Substring);
        assert_eq!(fuzz.engine, Engine::Fuzzy);
        assert!(
            exact.score > fuzz.score,
            "verbatim must win: {exact:?} vs {fuzz:?}"
        );
    }

    #[test]
    fn a_word_start_beats_the_middle_of_a_word() {
        let start = best("cache", "cache_key").unwrap();
        let middle = best("cache", "uncached").unwrap();
        assert!(start.score > middle.score, "{start:?} vs {middle:?}");
    }

    /// The property that keeps fuzzy from being noise: a tight match is worth
    /// showing and a match spread across a paragraph is a coincidence.
    #[test]
    fn a_compact_fuzzy_match_beats_a_scattered_one() {
        let tight = best("gitcmt", "git_commit").unwrap();
        let loose = best("gitcmt", "gather input then compute many things").unwrap();
        assert_eq!(tight.engine, Engine::Fuzzy);
        assert_eq!(loose.engine, Engine::Fuzzy);
        assert!(tight.score > loose.score, "{tight:?} vs {loose:?}");
    }

    #[test]
    fn smart_case_matches_what_you_typed() {
        // Lowercase is case-insensitive.
        assert!(best("cache", "The Cache").is_some());
        // Any uppercase means you meant it.
        assert!(best("Cache", "the cache").is_none());
        assert!(best("Cache", "the Cache").is_some());
    }

    #[test]
    fn nothing_matches_nothing() {
        assert!(best("", "anything").is_none());
        assert!(best("   ", "anything").is_none());
        assert!(best("zebra", "the cache is warm").is_none());
        // A query longer than the text cannot match, even fuzzily.
        assert!(best("a very long query indeed", "short").is_none());
    }

    /// The offset is handed straight to a scroll, so a multi-byte character
    /// before the hit must not shift it.
    #[test]
    fn the_offset_is_a_byte_offset() {
        let hay = "café and the cache";
        let h = best("cache", hay).unwrap();
        assert_eq!(&hay[h.at..h.at + 5], "cache");
    }
}
