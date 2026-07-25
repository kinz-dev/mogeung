//! Review debt and blast radius — how much nobody has read, and what a change
//! might break.
//!
//! Roadmap `R-D8` and `R-D9`. Both are *reading aids*, not analysis: they point
//! at things worth a human look and make no claim to correctness.

use serde::{Deserialize, Serialize};

/// How much of what agents produced in one repo has actually been read.
///
/// Counted in hunks rather than lines or files, because the hunk is the unit
/// the review UI actually checks off — a metric you cannot act on in the same
/// units you measure it in is decoration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReviewDebt {
    pub repo: String,
    /// Sessions that produced changes in this repo.
    pub sessions: u32,
    /// Sessions with at least one unread hunk.
    pub sessions_unread: u32,
    pub hunks_total: u32,
    pub hunks_read: u32,
    pub files_touched: u32,
    /// Files where every hunk is still unread, riskiest first.
    pub worst_files: Vec<DebtFile>,
    /// Insertions across everything unread. The "how big is the hole" number.
    pub unread_insertions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebtFile {
    pub path: String,
    pub session_id: String,
    pub unread_hunks: u32,
    pub score: i32,
}

impl ReviewDebt {
    /// Fraction read, 0.0–1.0. An empty repo counts as fully read: there is
    /// nothing outstanding, and reporting 0% would be a false alarm.
    pub fn progress(&self) -> f32 {
        if self.hunks_total == 0 {
            return 1.0;
        }
        self.hunks_read as f32 / self.hunks_total as f32
    }

    pub fn unread(&self) -> u32 {
        self.hunks_total.saturating_sub(self.hunks_read)
    }

    pub fn headline(&self) -> String {
        if self.hunks_total == 0 {
            return "nothing outstanding".into();
        }
        format!(
            "{} of {} hunks unread across {} session(s)",
            self.unread(),
            self.hunks_total,
            self.sessions
        )
    }
}

/// Where a changed symbol is referenced elsewhere.
///
/// **This is grep, not a compiler.** It matches identifiers textually, so it
/// over-reports common names and misses anything reached dynamically. It is
/// offered as "these files mention it, you may want to look" and the UI must
/// not imply more than that.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlastRadius {
    pub session_id: String,
    /// The file whose changes were analysed.
    pub path: String,
    /// Symbols we recognised as defined or altered in the diff.
    pub symbols: Vec<String>,
    pub references: Vec<Reference>,
    /// True when the search hit its cap and results are incomplete.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub path: String,
    pub line: u32,
    pub text: String,
    pub symbol: String,
    /// Test files are called out: a changed symbol with no test referencing it
    /// is a different kind of risk from one with several.
    pub is_test: bool,
}

impl BlastRadius {
    pub fn tests(&self) -> usize {
        self.references.iter().filter(|r| r.is_test).count()
    }

    pub fn files_affected(&self) -> usize {
        let mut paths: Vec<&str> = self.references.iter().map(|r| r.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        paths.len()
    }

    pub fn headline(&self) -> String {
        if self.symbols.is_empty() {
            return "no named symbols found in this diff".into();
        }
        if self.references.is_empty() {
            return format!(
                "{} symbol(s) changed, referenced nowhere else",
                self.symbols.len()
            );
        }
        format!(
            "{} reference(s) in {} file(s), {} in tests",
            self.references.len(),
            self.files_affected(),
            self.tests()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_repo_is_fully_read_not_zero_percent() {
        let d = ReviewDebt::default();
        assert_eq!(d.progress(), 1.0);
        assert_eq!(d.headline(), "nothing outstanding");
    }

    #[test]
    fn progress_counts_hunks() {
        let d = ReviewDebt {
            hunks_total: 10,
            hunks_read: 4,
            sessions: 2,
            ..Default::default()
        };
        assert!((d.progress() - 0.4).abs() < 1e-6);
        assert_eq!(d.unread(), 6);
        assert!(d.headline().contains("6 of 10"));
    }

    #[test]
    fn blast_radius_distinguishes_no_symbols_from_no_callers() {
        let none = BlastRadius::default();
        assert!(none.headline().contains("no named symbols"));

        let orphan = BlastRadius {
            symbols: vec!["do_thing".into()],
            ..Default::default()
        };
        assert!(orphan.headline().contains("referenced nowhere else"));
    }

    #[test]
    fn blast_radius_counts_distinct_files_not_hits() {
        let b = BlastRadius {
            symbols: vec!["f".into()],
            references: vec![
                Reference { path: "a.rs".into(), line: 1, text: "f()".into(), symbol: "f".into(), is_test: false },
                Reference { path: "a.rs".into(), line: 9, text: "f()".into(), symbol: "f".into(), is_test: false },
                Reference { path: "t.rs".into(), line: 3, text: "f()".into(), symbol: "f".into(), is_test: true },
            ],
            ..Default::default()
        };
        assert_eq!(b.files_affected(), 2);
        assert_eq!(b.tests(), 1);
        assert!(b.headline().contains("3 reference(s) in 2 file(s), 1 in tests"));
    }
}
