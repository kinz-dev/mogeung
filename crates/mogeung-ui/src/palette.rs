//! The command palette: every action by name, without knowing its key first.
//!
//! # Why this exists
//!
//! mogeung is built for someone who navigates by keyboard, and the honest
//! problem with that is discoverability. A shortcut you have not memorised is
//! indistinguishable from a feature that does not exist — so a keyboard-first
//! tool with thirty bindings quietly ships most of itself to nobody.
//!
//! The palette resolves it without compromising either side. Type a few
//! letters, get the action *and its binding*, so using it teaches you the key
//! you would otherwise have had to be told. It is a discovery surface that
//! makes itself unnecessary.
//!
//! Everything here is driven from `keymap::Action::ALL`, so an action added
//! later appears in the palette with no extra work — and an action that is
//! deliberately unbound is still reachable, which nothing else in the app can
//! say.

use crate::keymap::{Action, Keymap};

/// One row: the action, its score against the query, and where it came from.
pub struct Match {
    pub action: Action,
    pub score: i32,
}

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    pub query: String,
    /// Index into the *current* match list, not into `Action::ALL`.
    pub cursor: usize,
}

impl Palette {
    pub fn open(&mut self) {
        self.open = true;
        self.query.clear();
        self.cursor = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.cursor = 0;
    }

    /// Ranked matches for the current query.
    pub fn matches(&self, keymap: &Keymap) -> Vec<Match> {
        let mut out: Vec<Match> = Action::ALL
            .iter()
            .filter_map(|a| {
                // The binding is searchable too, so "alt+1" finds the pane it
                // switches to — useful when you half-remember the key rather
                // than the name.
                let haystack = format!("{} {} {}", a.label(), a.group(), keymap.describe(*a));
                score(&self.query, &haystack).map(|score| Match { action: *a, score })
            })
            .collect();
        // Descending score, then the canonical order of `Action::ALL` so an
        // empty query shows the list grouped the way the settings window does
        // rather than in an order nobody chose.
        out.sort_by(|a, b| {
            b.score.cmp(&a.score).then_with(|| {
                let ia = Action::ALL.iter().position(|x| x == &a.action);
                let ib = Action::ALL.iter().position(|x| x == &b.action);
                ia.cmp(&ib)
            })
        });
        out
    }

    /// Keep the cursor inside the list however the query changed under it.
    pub fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }

    pub fn move_cursor(&mut self, delta: i32, len: usize) {
        if len == 0 {
            return;
        }
        let last = len as i32 - 1;
        // Wrap, so holding Down does not dead-end at the bottom of a long list.
        let next = self.cursor as i32 + delta;
        self.cursor = if next < 0 {
            last as usize
        } else if next > last {
            0
        } else {
            next as usize
        };
    }
}

/// Fuzzy subsequence score, or `None` when the query is not a subsequence.
///
/// Greedy left-to-right matching is *complete* for subsequence detection — if
/// a match exists, taking the earliest candidate character always finds one —
/// so this cannot report a false miss despite never backtracking.
///
/// The weights encode what people actually aim at when they type into a
/// palette: initials of words, and runs of adjacent characters. "ttt" should
/// find "Show the **T**ranscript **t**ab" ahead of anything that merely
/// contains those letters scattered through it.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();

    let mut qi = 0;
    let mut score = 0;
    let mut prev_matched = false;
    for (i, c) in t.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            let at_word_start = i == 0 || !t[i - 1].is_alphanumeric();
            score += if at_word_start { 8 } else { 1 };
            // Worth at least as much as a word start. With a smaller bonus,
            // typing a whole word lost to a string whose every letter happened
            // to begin one — "term" ranked "t e r m" above "terminal".
            if prev_matched {
                score += 8;
            }
            prev_matched = true;
            qi += 1;
        } else {
            prev_matched = false;
        }
    }
    if qi < q.len() {
        return None;
    }
    // A shorter label containing the query is a better answer than a long one,
    // all else equal.
    Some(score - (t.len() as i32) / 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn a_query_that_is_not_a_subsequence_does_not_match() {
        assert_eq!(score("zzz", "Show the Changes tab"), None);
        assert_eq!(score("abc", "cba"), None);
    }

    /// Greedy matching must not miss a match that exists. `aab` against
    /// `axaby`: the naive worry is that the first `a` is consumed by the wrong
    /// position and the rest fails.
    #[test]
    fn greedy_matching_never_reports_a_false_miss() {
        assert!(score("aab", "axaby").is_some());
        assert!(score("aa", "aba").is_some());
    }

    /// A letter that begins a word is what someone typing initials is aiming
    /// at; the same letter buried mid-word is usually a coincidence.
    #[test]
    fn a_word_start_beats_a_letter_buried_mid_word() {
        let start = score("t", "the tab").unwrap();
        let buried = score("t", "what").unwrap();
        assert!(start > buried, "start {start} should beat buried {buried}");
    }

    /// Typing a whole word must find that word. This failed on the first
    /// weighting: the word-start bonus was larger than the contiguity bonus, so
    /// "term" ranked the nonsense "t e r m" — four word starts — above
    /// "terminal", which is the only answer anyone wanted.
    #[test]
    fn a_contiguous_run_beats_the_same_letters_spread_out() {
        let run = score("term", "terminal").unwrap();
        let every_letter_a_word_start = score("term", "t e r m").unwrap();
        let scattered_mid_word = score("term", "taextrxmx").unwrap();
        assert!(
            run > every_letter_a_word_start,
            "run {run} should beat {every_letter_a_word_start}"
        );
        assert!(
            run > scattered_mid_word,
            "run {run} should beat {scattered_mid_word}"
        );
    }

    /// Case must not matter — nobody types the capital.
    #[test]
    fn matching_ignores_case() {
        assert!(score("TAB", "Show the Changes tab").is_some());
        assert!(score("tab", "Show the Changes TAB").is_some());
    }

    #[test]
    fn the_cursor_wraps_at_both_ends() {
        let mut p = Palette::default();
        p.move_cursor(-1, 3);
        assert_eq!(p.cursor, 2, "up from the top wraps to the bottom");
        p.move_cursor(1, 3);
        assert_eq!(p.cursor, 0, "down from the bottom wraps to the top");
    }

    /// An empty list must not panic or leave the cursor pointing at a row that
    /// is not there — the state you land in when a query matches nothing.
    #[test]
    fn an_empty_match_list_is_survivable() {
        let mut p = Palette {
            cursor: 5,
            ..Default::default()
        };
        p.clamp(0);
        assert_eq!(p.cursor, 0);
        p.move_cursor(1, 0);
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn the_cursor_is_pulled_back_when_the_list_shrinks() {
        let mut p = Palette {
            cursor: 9,
            ..Default::default()
        };
        p.clamp(3);
        assert_eq!(p.cursor, 2);
    }

    /// Every action must be reachable by typing its own label, or the palette
    /// fails at the one job it has. Catches a label and a scorer that disagree.
    #[test]
    fn every_action_is_findable_by_its_own_label() {
        let keymap = Keymap::default();
        for action in Action::ALL {
            let p = Palette {
                open: true,
                query: action.label().to_string(),
                cursor: 0,
            };
            let found = p.matches(&keymap);
            assert_eq!(
                found.first().map(|m| m.action),
                Some(*action),
                "{:?} did not rank first for its own label",
                action
            );
        }
    }
}
