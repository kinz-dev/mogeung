use serde::{Deserialize, Serialize};

/// Why a hunk or file was flagged as worth reading first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskFlag {
    Auth,
    Secrets,
    Migration,
    Money,
    Concurrency,
    ErrorHandling,
    NetworkIo,
    Dependency,
    CiConfig,
    Infra,
    DeletedTest,
    PublicApi,
    UnsafeCode,
    LargeDeletion,
    /// Lockfiles, generated code, fixtures, formatting. Collapsed by default.
    Noise,
}

impl RiskFlag {
    pub fn label(&self) -> &'static str {
        match self {
            RiskFlag::Auth => "auth",
            RiskFlag::Secrets => "secrets",
            RiskFlag::Migration => "migration",
            RiskFlag::Money => "money",
            RiskFlag::Concurrency => "concurrency",
            RiskFlag::ErrorHandling => "error-handling",
            RiskFlag::NetworkIo => "network",
            RiskFlag::Dependency => "dependency",
            RiskFlag::CiConfig => "ci",
            RiskFlag::Infra => "infra",
            RiskFlag::DeletedTest => "deleted-test",
            RiskFlag::PublicApi => "public-api",
            RiskFlag::UnsafeCode => "unsafe",
            RiskFlag::LargeDeletion => "large-deletion",
            RiskFlag::Noise => "noise",
        }
    }

    /// Contribution to the risk score. Tuned so that one strong signal
    /// (a migration, a secret) outranks any amount of ordinary churn.
    pub fn weight(&self) -> i32 {
        match self {
            RiskFlag::Secrets => 100,
            RiskFlag::Auth => 90,
            RiskFlag::Migration => 85,
            RiskFlag::Money => 85,
            RiskFlag::Infra => 70,
            RiskFlag::CiConfig => 65,
            RiskFlag::DeletedTest => 60,
            RiskFlag::UnsafeCode => 60,
            RiskFlag::Concurrency => 55,
            RiskFlag::Dependency => 50,
            RiskFlag::PublicApi => 40,
            RiskFlag::ErrorHandling => 30,
            RiskFlag::NetworkIo => 25,
            RiskFlag::LargeDeletion => 35,
            RiskFlag::Noise => -60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Noise,
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn from_score(score: i32) -> Self {
        match score {
            i32::MIN..=-1 => RiskLevel::Noise,
            0..=29 => RiskLevel::Low,
            30..=69 => RiskLevel::Medium,
            _ => RiskLevel::High,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RiskLevel::Noise => "noise",
            RiskLevel::Low => "low",
            RiskLevel::Medium => "med",
            RiskLevel::High => "HIGH",
        }
    }
}

/// A single diff hunk.
///
/// `anchor` is the load-bearing field: it is a hash of the hunk's *content*,
/// not its position. Reviewing a hunk records the anchor. When the agent
/// rewrites the file, unchanged hunks keep their anchor and stay reviewed;
/// only genuinely new or rewritten content comes back to you. This is the
/// "never read the same code twice" property (CONCEPT.md B3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub anchor: String,
    /// The `@@ ... @@` line, kept for display.
    pub header: String,
    /// Raw unified-diff body lines, including leading ` `, `+`, `-`.
    pub lines: Vec<String>,
    pub insertions: u32,
    pub deletions: u32,
    pub flags: Vec<RiskFlag>,
    pub score: i32,
    pub reviewed: bool,
}

impl Hunk {
    pub fn risk(&self) -> RiskLevel {
        RiskLevel::from_score(self.score)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub insertions: u32,
    pub deletions: u32,
    pub hunks: Vec<Hunk>,
    pub flags: Vec<RiskFlag>,
    pub score: i32,
    /// True when the diff was omitted (binary, or over the size cap).
    pub truncated: bool,
}

impl FileChange {
    pub fn risk(&self) -> RiskLevel {
        RiskLevel::from_score(self.score)
    }

    pub fn reviewed_hunks(&self) -> usize {
        self.hunks.iter().filter(|h| h.reviewed).count()
    }

    pub fn fully_reviewed(&self) -> bool {
        !self.hunks.is_empty() && self.hunks.iter().all(|h| h.reviewed)
    }
}

/// The net change produced by a run, ordered for reading rather than alphabetically.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Change {
    pub files: Vec<FileChange>,
    pub insertions: u32,
    pub deletions: u32,
    /// Set when the diff could not be computed (repo gone, base commit missing).
    pub error: Option<String>,
}

impl Change {
    pub fn total_hunks(&self) -> usize {
        self.files.iter().map(|f| f.hunks.len()).sum()
    }

    pub fn reviewed_hunks(&self) -> usize {
        self.files.iter().map(|f| f.reviewed_hunks()).sum()
    }

    pub fn unreviewed_hunks(&self) -> usize {
        self.total_hunks() - self.reviewed_hunks()
    }

    /// Fraction of hunks a human has actually looked at. The per-run seed of the
    /// review-debt meter (CONCEPT.md B9).
    pub fn review_progress(&self) -> f32 {
        let total = self.total_hunks();
        if total == 0 {
            return 1.0;
        }
        self.reviewed_hunks() as f32 / total as f32
    }
}
