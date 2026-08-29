//! The reading guide: which changed file to read first, and why. `R-O3`.
//!
//! An agent-made diff is 22 files of which three matter, and finding those
//! three is currently done by scrolling. This asks a model to order them.
//!
//! **This module is shared with `--bin judge` on purpose.** The harness that
//! decided whether this was worth building must grade *the thing that ships*.
//! Two copies of the prompt would drift, and the day they did, the evidence in
//! `R-O2` would be about a feature that no longer exists.
//!
//! ## Two rules from the corpus, not from taste
//!
//! **A second ordering, never a blend.** [Pillar K](../../../docs/product/roadmap.md#k-explicitly-not)
//! settled this in advance: either honest keyword heuristics or real analysis,
//! never a weighted mix, because a mix looks authoritative while still being
//! wrong. So the keyword order is what shows with no model, this is what you
//! switch to, and the reason is visible wherever the ordering is used.
//!
//! **Everything unranked is appended, marked.** `--bin judge`'s first corpus
//! run: on 60-file diffs `claude-opus-5` ranked about sixteen and silently
//! dropped 44 and 40, where the local Qwen dropped 1 of 60 and
//! `claude-sonnet-5` 1 of 48. Shortlisting is what a reading guide is *for* —
//! doing it silently is how a guide hides two thirds of a change. So the model
//! orders what it names, and the rest follow in keyword order with
//! `ranked: false`. A file the guide does not mention is still on the screen.

use mogeung_core::change::FileChange;
use mogeung_core::wire::GuideFile;

/// The most of one file the model is ever shown.
///
/// The number is the design, not a detail: the guide must order the files from
/// **less** than the reader has, or it is not saving the scroll it exists to
/// save. Forty lines is about a screen.
pub const DIFF_LINES_PER_FILE: usize = 40;

/// The whole prompt's line budget, shared out between the files.
///
/// A per-file cap alone is not a bound — 60 files at 40 lines is 2,400 lines,
/// and measured against a real 280-file session that prompt came back after
/// **78 seconds with nothing at all**. A guide that fails on exactly the large
/// diffs it exists for is not a guide.
///
/// So the budget is on the total and the per-file share falls out of it. A
/// three-file change still gets a screen each; a sixty-file one gets fifteen
/// lines each, which is enough to tell a rename from a rewrite — which is all
/// the ordering needs.
pub const TOTAL_DIFF_LINES: usize = 900;

/// Never less than this, however many files there are. Below about four lines
/// a hunk says nothing at all, and a file the model cannot see is a file it
/// ranks by its path — which is what the keyword scorer already does.
pub const MIN_LINES_PER_FILE: usize = 4;

/// How many lines of each file to show, for a change of this size.
pub fn lines_per_file(file_count: usize) -> usize {
    if file_count == 0 {
        return DIFF_LINES_PER_FILE;
    }
    (TOTAL_DIFF_LINES / file_count).clamp(MIN_LINES_PER_FILE, DIFF_LINES_PER_FILE)
}

/// Files above this are not asked about at all.
///
/// The guide is for the 22-file diff where three matter. A 400-file one is a
/// different problem and a prompt nobody should pay for.
pub const MAX_FILES: usize = 60;

/// The files worth asking about.
///
/// `Noise` is dropped — lockfiles and generated output are already collapsed
/// in the Changes view, so including them would spend prompt on a decision the
/// keyword scorer has already made correctly.
pub fn askable(files: &[FileChange]) -> Vec<&FileChange> {
    files
        .iter()
        .filter(|f| {
            !f.truncated
                && !f
                    .flags
                    .iter()
                    .any(|fl| matches!(fl, mogeung_core::change::RiskFlag::Noise))
        })
        .take(MAX_FILES)
        .collect()
}

/// The question.
pub fn prompt(files: &[&FileChange]) -> String {
    let mut s = String::from(
        "You are given the changed files of one commit-in-progress. Order them by what a \
         reviewer should read FIRST to understand what this change does.\n\n\
         Answer in exactly this form and nothing else:\n\n\
         SUMMARY: one short paragraph. Say what carries the change, and what is mechanical.\n\
         <path> | <reason in at most 12 words>\n\
         <path> | <reason in at most 12 words>\n\n\
         One line per file, most important first. Use the paths exactly as given. \
         Rank by what carries the change. Put mechanical edits last: renames, formatting, \
         generated files, and edits that only follow from another file.\n\n",
    );
    let budget = lines_per_file(files.len());
    for f in files {
        s.push_str(&format!("--- {} (+{} -{})\n", f.path, f.insertions, f.deletions));
        let mut shown = 0;
        for h in &f.hunks {
            for line in h.lines.iter() {
                if shown >= budget {
                    break;
                }
                s.push_str(line);
                s.push('\n');
                shown += 1;
            }
            if shown >= budget {
                break;
            }
        }
        s.push('\n');
    }
    s
}

/// The model's summary paragraph, if it wrote one.
pub fn parse_summary(text: &str) -> String {
    for line in text.lines() {
        let t = line.trim().trim_start_matches(['-', '*', '#', ' ']);
        if let Some(rest) = t.strip_prefix("SUMMARY:").or_else(|| t.strip_prefix("Summary:")) {
            return rest.trim().to_string();
        }
    }
    String::new()
}

/// Read the order back out of whatever the model wrote.
///
/// Forgiving about shape and strict about membership. A model returns
/// `1. \u{60}src/foo.rs\u{60}` about as often as a bare path, so the path is matched as a
/// substring — but a path this diff does not contain is **dropped**, because a
/// model that invents one has not ordered this change, and accepting it would
/// let a guide list a file the reader cannot open.
pub fn parse_order(text: &str, files: &[&FileChange]) -> Vec<(String, String)> {
    let known: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['-', '*', '#', ' ']);
        if line.starts_with("SUMMARY:") || line.starts_with("Summary:") {
            continue;
        }
        let (left, why) = match line.split_once('|') {
            Some((l, r)) => (l.trim(), r.trim()),
            None => (line, ""),
        };
        let Some(hit) = known.iter().find(|k| left.contains(*k)) else {
            continue;
        };
        // Named twice is ranked once: the second mention would silently push
        // everything after it down a place.
        if seen.insert(hit.to_string()) {
            out.push((hit.to_string(), why.to_string()));
        }
    }
    out
}

/// The guide as the window renders it: the model's order, then everything it
/// did not mention, in the order the keyword scorer already had them.
///
/// The append is the rule the corpus bought. See the module note.
pub fn assemble(files: &[&FileChange], order: &[(String, String)]) -> Vec<GuideFile> {
    let mut out: Vec<GuideFile> = order
        .iter()
        .map(|(path, reason)| GuideFile {
            path: path.clone(),
            reason: reason.clone(),
            ranked: true,
        })
        .collect();
    for f in files {
        if !order.iter().any(|(p, _)| p == &f.path) {
            out.push(GuideFile {
                path: f.path.clone(),
                reason: String::new(),
                ranked: false,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::change::FileStatus;

    fn file(path: &str) -> FileChange {
        FileChange {
            path: path.into(),
            old_path: None,
            status: FileStatus::Modified,
            insertions: 1,
            deletions: 0,
            hunks: Vec::new(),
            flags: Vec::new(),
            score: 0,
            truncated: false,
        }
    }

    /// The shapes a model actually returns, taken from real runs. `R-O2`.
    #[test]
    fn an_order_is_read_out_of_whatever_the_model_wrote() {
        let files = [file("src/model.rs"), file("src/wire.rs"), file("README.md")];
        let refs: Vec<&FileChange> = files.iter().collect();
        let text = "SUMMARY: the streaming change.\n\
                    1. `src/wire.rs` | adds the chunk event\n\
                    - src/model.rs — streams the reply\n\
                      README.md | notes it";

        assert_eq!(parse_summary(text), "the streaming change.");
        let got = parse_order(text, &refs);
        assert_eq!(
            got.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["src/wire.rs", "src/model.rs", "README.md"],
            "numbering, bullets and backticks are noise around the path"
        );
        assert_eq!(got[0].1, "adds the chunk event");
        assert_eq!(got[1].1, "", "an em dash is not the separator; it still ranks");
    }

    /// A model that invents a path has not ordered *this* change, and a guide
    /// listing a file the reader cannot open is worse than a short guide.
    #[test]
    fn a_file_this_diff_does_not_have_is_dropped() {
        let files = [file("src/model.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        let got = parse_order("1. src/imaginary.rs | invented\n2. src/model.rs | real", &refs);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "src/model.rs");
    }

    #[test]
    fn a_file_named_twice_is_ranked_once() {
        let files = [file("a.rs"), file("b.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        let got = parse_order("a.rs | first\nb.rs | second\na.rs | again", &refs);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "a.rs");
    }

    /// Nothing usable is an empty order, not silent agreement.
    #[test]
    fn prose_with_no_paths_orders_nothing() {
        let files = [file("a.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        assert!(parse_order("I am sorry, I cannot help with that.", &refs).is_empty());
        assert!(parse_order("", &refs).is_empty());
        assert_eq!(parse_summary("no summary here"), "");
    }

    /// **The rule the corpus bought.** `claude-opus-5` ranked 16 of 60 files
    /// and said nothing about the other 44. Rendering its list as the diff
    /// would hide them, so they are appended and marked.
    #[test]
    fn every_file_the_model_ignored_is_still_on_the_screen() {
        let files = [file("a.rs"), file("b.rs"), file("c.rs"), file("d.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        // The model named two of four, out of keyword order.
        let order = parse_order("c.rs | carries the change\na.rs | follows from it", &refs);

        let guide = assemble(&refs, &order);
        assert_eq!(guide.len(), 4, "nothing is dropped, ever");
        assert_eq!(
            guide.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(),
            ["c.rs", "a.rs", "b.rs", "d.rs"],
            "the model's order first, then the rest as the keyword scorer had them"
        );
        assert_eq!(
            guide.iter().map(|g| g.ranked).collect::<Vec<_>>(),
            [true, true, false, false],
            "and the unranked say so, rather than passing as a judgement"
        );
        assert_eq!(guide[0].reason, "carries the change");
        assert_eq!(guide[2].reason, "", "an unranked file has no reason to show");
    }

    /// A model that answers nothing usable leaves the keyword order intact
    /// rather than an empty pane.
    #[test]
    fn no_order_at_all_is_the_keyword_order_unranked() {
        let files = [file("a.rs"), file("b.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        let guide = assemble(&refs, &[]);
        assert_eq!(guide.len(), 2);
        assert!(guide.iter().all(|g| !g.ranked));
    }

    /// The prompt is bounded by the **whole** change, not by each file.
    ///
    /// Measured, not guessed: 60 files at the per-file cap is 2,400 lines, and
    /// a real 280-file session answered that prompt after 78 seconds with
    /// nothing at all. A guide that fails on the large diffs it exists for is
    /// not a guide.
    #[test]
    fn a_big_change_gets_less_of_each_file_rather_than_a_huge_prompt() {
        assert_eq!(lines_per_file(3), DIFF_LINES_PER_FILE, "a small change gets a screen each");
        assert_eq!(lines_per_file(60), 15, "sixty files share the budget");
        assert!(
            lines_per_file(60) * 60 <= TOTAL_DIFF_LINES,
            "the total is what is bounded"
        );
        // However many files, each one is still worth looking at.
        assert_eq!(lines_per_file(10_000), MIN_LINES_PER_FILE);
        assert_eq!(lines_per_file(0), DIFF_LINES_PER_FILE, "and no division by zero");
    }

    /// Noise is already collapsed by the Changes view, so it is not worth
    /// prompt — and a model re-deriving that decision could get it wrong.
    #[test]
    fn noise_is_not_asked_about() {
        let mut lock = file("Cargo.lock");
        lock.flags.push(mogeung_core::change::RiskFlag::Noise);
        let mut big = file("huge.bin");
        big.truncated = true;
        let files = vec![file("src/a.rs"), lock, big];
        let asked = askable(&files);
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].path, "src/a.rs");
    }
}
