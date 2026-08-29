//! Completing a command line from what has actually been run. `R-O12`, `A40`.
//!
//! **The corpus is the whole argument.** `zsh-autosuggestions`, fish and atuin
//! all complete from a shell history, instantly and for free, and a suggestion
//! that is merely *as good* loses on every axis that is not quality — the trap
//! [A35](../../../docs/product/assumptions.md) named for the reading guide.
//! What mogeung has that none of them do is every `Bash` call an **agent** made
//! on your behalf, in a named repository: the larger half of what has run on
//! this machine, and text no shell has ever seen.
//!
//! ## Local, and that is a fence rather than an optimisation
//!
//! A command line is the most secret-carrying text in this corpus —
//! `export TOKEN=…`, hostnames, one-off credentials — so the prefix you are
//! typing is **never sent anywhere**, not even to be embedded. Ranking here is
//! arithmetic over text mogeung already holds. The model's half of `R-O12` is a
//! *question* you deliberately typed, which is a different act with a different
//! risk, and it lives elsewhere.
//!
//! ## What the ranking is, and what it deliberately is not
//!
//! Prefix first, then this repository, then how often, then how recently. No
//! fuzzy subsequence matching in the first cut: `gto` matching `git checkout`
//! is the kind of cleverness that makes a list feel magic in a demo and
//! untrustworthy at the fifth row, and `--bin judge --complete` can say whether
//! it is needed rather than the author guessing.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

/// One candidate command, with the evidence that it is one.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub command: String,
    /// How many times it has been run, across the corpus.
    pub count: u32,
    /// The most recent time it ran, when known.
    pub last: Option<DateTime<Utc>>,
    /// It ran in the repository being completed in.
    pub here: bool,
    /// Where it came from — `agent` or `shell`. Shown on the row, because *an
    /// agent ran this 12×* and *you ran this once* are different claims.
    pub source: &'static str,
}

/// Fold a stream of `(command, repo, when, source)` into ranked candidates.
///
/// `repo` is the repository being completed in; a command from elsewhere still
/// appears, marked, because the same tool is often run from a sibling checkout
/// and hiding it would be a lie of omission rather than a filter.
pub fn candidates(
    corpus: impl IntoIterator<Item = (String, String, Option<DateTime<Utc>>, &'static str)>,
    repo: &str,
    prefix: &str,
) -> Vec<Candidate> {
    let needle = prefix.trim_start();
    let mut by_command: HashMap<String, Candidate> = HashMap::new();
    for (command, from, when, source) in corpus {
        if !needle.is_empty() && !command.starts_with(needle) {
            continue;
        }
        // Its own last invocation, and whether *any* of them was here: a
        // command run once in this repo and fifty times elsewhere is still one
        // you want offered here.
        let here = !repo.is_empty() && !from.is_empty() && (from == repo || from.starts_with(repo));
        let e = by_command.entry(command.clone()).or_insert(Candidate {
            command,
            count: 0,
            last: None,
            here: false,
            source,
        });
        e.count += 1;
        e.here |= here;
        if when > e.last {
            e.last = when;
        }
    }
    let mut out: Vec<Candidate> = by_command.into_values().collect();
    out.sort_by(|a, b| {
        // This repository first, because a command is a thing you run *here*.
        b.here
            .cmp(&a.here)
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| b.last.cmp(&a.last))
            // Shorter last on a tie: the shorter of two equally-run commands is
            // the one you are more likely to be part-way through typing.
            .then_with(|| a.command.len().cmp(&b.command.len()))
            .then_with(|| a.command.cmp(&b.command))
    });
    out
}

/// `~/.zsh_history` and friends, oldest first — the **baseline**, not a source.
///
/// Read only by the harness, and only so that `A40` is measured against what
/// you already have rather than against nothing: every shell completion tool
/// starts from this file, so a corpus that does not beat it has not earned a
/// panel. The extended format is `: <epoch>:<elapsed>;<command>`; a plain line
/// is a command on its own.
pub fn shell_history(path: &std::path::Path) -> Vec<(String, Option<DateTime<Utc>>)> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    // Lossy on purpose: a history file accumulates whatever was pasted into it
    // over years, and one invalid byte must not cost the whole baseline.
    let text = String::from_utf8_lossy(&bytes);
    let mut out = Vec::new();
    for line in text.lines() {
        let (when, cmd) = match line.strip_prefix(": ") {
            Some(rest) => match rest.split_once(';') {
                Some((meta, cmd)) => (
                    meta.split(':').next().and_then(|s| s.trim().parse::<i64>().ok()),
                    cmd,
                ),
                None => (None, rest),
            },
            None => (None, line),
        };
        let cmd = cmd.trim();
        if cmd.is_empty() || cmd.contains('\n') {
            continue;
        }
        out.push((
            cmd.to_string(),
            when.and_then(|t| DateTime::from_timestamp(t, 0)),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(secs, 0)
    }

    fn corpus() -> Vec<(String, String, Option<DateTime<Utc>>, &'static str)> {
        vec![
            ("cargo test --workspace".into(), "/w/mogeung".into(), at(100), "agent"),
            ("cargo test --workspace".into(), "/w/mogeung".into(), at(300), "agent"),
            ("cargo build".into(), "/w/other".into(), at(400), "agent"),
            ("cargo test -p core".into(), "/w/other".into(), at(200), "agent"),
        ]
    }

    #[test]
    fn a_command_run_here_outranks_one_run_more_often_elsewhere() {
        let out = candidates(corpus(), "/w/mogeung", "cargo ");
        assert_eq!(out[0].command, "cargo test --workspace");
        assert!(out[0].here);
        assert_eq!(out[0].count, 2, "the two runs fold into one row");
        assert_eq!(out[0].last, at(300), "and it keeps the most recent");
        // Still offered, and still marked as elsewhere — the same tool is often
        // run from a sibling checkout.
        assert!(out.iter().any(|c| c.command == "cargo build" && !c.here));
    }

    #[test]
    fn the_prefix_is_a_prefix_and_nothing_cleverer() {
        // Both `cargo test` commands match `cargo test -`, because
        // `--workspace` begins with a dash — the first version of this test
        // asserted one row and was wrong about its own fixture.
        let out = candidates(corpus(), "/w/mogeung", "cargo test -");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].command, "cargo test --workspace", "this repo, twice");

        // And nothing cleverer: an initialism matches nothing. `ct` finding
        // `cargo test` is the kind of magic that is delightful in a demo and
        // untrustworthy at the fifth row, so `--bin judge --complete` gets to
        // decide whether it is needed rather than the author guessing.
        assert!(candidates(corpus(), "/w/mogeung", "ct").is_empty());
    }

    #[test]
    fn an_empty_prefix_offers_the_repository_first() {
        let out = candidates(corpus(), "/w/other", "");
        assert!(out[0].here, "{:?}", out[0]);
    }

    /// zsh writes `: <epoch>:<elapsed>;<command>` when `EXTENDED_HISTORY` is
    /// on and a bare line when it is not. Both are real on real machines.
    #[test]
    fn both_shapes_of_shell_history_are_read() {
        let dir = std::env::temp_dir().join(format!("mogeung-hist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        let p = dir.join("hist");
        std::fs::write(&p, ": 1787990851:0;sudo ./install.sh\nplain command\n\n").expect("write");
        let got = shell_history(&p);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "sudo ./install.sh");
        assert_eq!(got[0].1, at(1787990851));
        assert_eq!(got[1].0, "plain command");
        assert_eq!(got[1].1, None, "a bare line carries no time, and does not pretend to");
    }
}
