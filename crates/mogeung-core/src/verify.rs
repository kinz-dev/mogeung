//! Verification — an agent's claims held against transcript evidence.
//! Pillar E (`R-E1`–`R-E5`).
//!
//! Everything here is honest-heuristic by design: commands are classified by
//! recognisable patterns, claims by phrase shapes, and every binding says how
//! it matched. Presenting judgment as authority is the pillar-K failure mode
//! this module exists to avoid.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What a Bash command was, verification-wise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyKind {
    /// Ranked lowest so `max` picks the stronger claim in `a && b` chains.
    Build,
    Lint,
    Typecheck,
    Coverage,
    Test,
}

impl VerifyKind {
    pub fn label(&self) -> &'static str {
        match self {
            VerifyKind::Test => "test",
            VerifyKind::Coverage => "coverage",
            VerifyKind::Typecheck => "typecheck",
            VerifyKind::Lint => "lint",
            VerifyKind::Build => "build",
        }
    }
}

/// One verification-shaped command a session ran, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyRun {
    pub kind: VerifyKind,
    /// The command, one-lined and clipped.
    pub command: String,
    pub at: DateTime<Utc>,
    /// `None` while the tool call has not come back. A run that never comes
    /// back stays `None` — an honest unknown, not a pass.
    #[serde(default)]
    pub ok: Option<bool>,
    /// The `tool_use` id, kept so the eventual `tool_result` can find this
    /// run. Wire-visible but of no display interest.
    #[serde(default)]
    pub tool_use_id: String,
}

/// A "tests pass"-shaped assertion in assistant prose, bound to evidence when
/// any exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// The sentence, clipped.
    pub text: String,
    pub at: DateTime<Utc>,
    /// What the claim sounds like it is about.
    pub kind: VerifyKind,
    /// How the claim was matched to evidence — a human-readable statement of
    /// the heuristic's work, e.g. `matched: cargo test, exited ok` or
    /// `no matching run seen before this claim`. Never blank.
    pub evidence: String,
    /// The bound run ended in failure while the prose claims success — the
    /// single most useful bit in this module.
    #[serde(default)]
    pub contradicted: bool,
}

/// Coverage on the changed lines of one file — `R-E5`. Only lines the diff
/// touched count; whole-repo coverage is someone else's number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCoverage {
    pub path: String,
    pub changed_covered: u32,
    pub changed_missed: u32,
}

/// One run of a repo's configured signal command — `R-E2`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalRun {
    pub command: String,
    pub started_at: DateTime<Utc>,
    pub secs: i64,
    /// Process exit code; `None` when it could not even start.
    pub exit: Option<i32>,
    /// Last lines of combined output, clipped.
    pub tail: String,
    /// Present only when an lcov file appeared and a session diff existed to
    /// intersect it with. Absent means "no data", and the UI says exactly
    /// that — a made-up number would be worse. `R-E5`.
    #[serde(default)]
    pub coverage: Vec<FileCoverage>,
}
