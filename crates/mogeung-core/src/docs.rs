//! Doc-sprawl report types. Pillar H (`R-H1`–`R-H5`), feature 0022.
//!
//! Shapes only — the scanner lives in `mogeungd::docscan`. Two rules carry
//! over from `scripts/check-docs.sh` and `scripts/gen-status.sh`, this
//! pillar's toy-scale ancestors:
//!
//! - **Evidence, always.** Every classification, staleness flag and GC
//!   proposal names the observation it rests on. A judgement whose reasoning
//!   is invisible gets ignored, which costs more than it ever catches.
//! - **Proposals only.** Nothing in this report is an instruction to act;
//!   mogeung never writes to a watched repo (ADR-0003 applied to docs).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What a markdown file *is*, as far as heuristics can tell. `Unknown` is an
/// honest answer, not a failure — the evidence string says what was tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    Readme,
    Changelog,
    /// Architecture decision record: `docs/decisions/`-shaped path, or a
    /// numbered file carrying a Status/Superseded section.
    Adr,
    /// Feature spec / RFC — `docs/features/`, `docs/specs/`, `rfcs/`.
    Spec,
    /// Checklist-heavy, or named `PLAN.md`/`TODO.md` — the classic agent
    /// dropping this pillar exists to account for.
    Plan,
    Note,
    /// Carries a "generated / do not edit" marker, or is `STATUS.md`-like.
    /// Derived files are never GC candidates: regenerating fixes them.
    Generated,
    /// `CLAUDE.md` / `AGENTS.md` / `GEMINI.md` — agent instructions.
    Instruction,
    #[default]
    Unknown,
}

/// One markdown file's inventory row: what it is, when it lived, and how it
/// sits in the link graph. `R-H1`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocInfo {
    /// Repo-relative, `/`-joined.
    pub path: String,
    pub kind: DocKind,
    /// Why it was classified that way — a short human-readable observation,
    /// never empty. No black boxes.
    pub evidence: String,
    /// Frontmatter `title:`, else the first `# ` heading.
    pub title: Option<String>,
    /// Frontmatter `status:` verbatim, when present.
    pub status: Option<String>,
    /// Frontmatter `updated:` verbatim, when present.
    pub updated: Option<String>,
    /// Unix epoch of the commit that added the file (git author date), or the
    /// fs mtime when git could not answer.
    pub created_epoch: Option<i64>,
    /// Unix epoch of the last commit touching the file (git author date), or
    /// the fs mtime when git could not answer.
    pub edited_epoch: Option<i64>,
    /// True when the epochs above came from git history. False means fs
    /// mtime — still a date, but not provenance.
    pub dates_from_git: bool,
    /// Full on-disk size, even when scanning clipped the content.
    pub bytes: u64,
    /// Repo-relative `.md` files this doc links to (relative links resolved,
    /// existing targets only).
    pub links_out: Vec<String>,
    /// How many other docs in the set link here.
    pub links_in: u32,
    /// No inbound links from any other doc in the set.
    pub orphan: bool,
}

/// A doc that references a repo path whose history is newer than the doc's
/// own last edit. `R-H2`. Author dates on both sides — committer dates reset
/// on every rebase, and a staleness check that cries wolf after a rebase
/// teaches you to ignore it (see `scripts/check-docs.sh`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StaleDoc {
    /// The doc that has drifted.
    pub doc: String,
    /// The repo path it references, which moved after the doc last did.
    pub referenced_path: String,
    /// The doc's last edit (git author date, unix epoch).
    pub doc_edited_epoch: i64,
    /// The referenced path's last edit (git author date, unix epoch).
    pub path_edited_epoch: i64,
    /// Commits touching the referenced path since the doc's last edit,
    /// counted on author dates. Capped by the scanner's log window, so a
    /// large value is a floor, not an exact count.
    pub commits_since: u32,
}

/// What the GC list proposes a human do. Proposals only — mogeung contains
/// no code path that could carry any of these out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GcAction {
    Archive,
    Merge,
    #[default]
    Review,
}

/// One GC proposal with the observation that produced it. A doc may appear
/// more than once, once per independent reason. `R-H3`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GcCandidate {
    pub path: String,
    pub action: GcAction,
    pub evidence: String,
}

/// One checklist item from a plan-shaped doc, bound to evidence where that
/// is honestly possible. `R-H4`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanItem {
    /// Item text, continuation lines joined, clipped.
    pub text: String,
    pub checked: bool,
    /// Repo paths the item names that actually exist. Empty when it names
    /// none we could find.
    pub paths: Vec<String>,
    /// True only when the item names existing repo paths *and* git history
    /// was available to consult. An item naming no path gets `false` and
    /// **no verdict** — absence of evidence is not evidence of absence.
    pub verifiable: bool,
    /// Checked, verifiable, and yet no commit touched any named path since
    /// the doc was created. The box was ticked; the repo does not show it.
    pub claimed_unevidenced: bool,
}

/// A doc's checklist, extracted and evidence-checked. `R-H4`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanCheck {
    pub doc: String,
    pub checked: u32,
    pub unchecked: u32,
    /// Capped by the scanner; counts above are over the full doc.
    pub items: Vec<PlanItem>,
}

/// One root-level instruction file, sized and dated for the side-by-side.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InstructionFile {
    pub path: String,
    pub bytes: u64,
    /// Git author date of the last edit, else fs mtime.
    pub edited_epoch: Option<i64>,
    /// Distinct non-empty trimmed lines — the unit of comparison.
    pub distinct_lines: u32,
}

/// Pairwise drift between two instruction files. Line-set comparison on
/// trimmed non-empty lines; samples are clipped, counts are complete.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InstructionPair {
    pub a: String,
    pub b: String,
    pub shared_lines: u32,
    /// `shared / union` of the two line sets, 0.0–1.0.
    pub shared_ratio: f32,
    pub only_in_a_total: u32,
    pub only_in_b_total: u32,
    /// Up to 20 sample lines found only in `a`, clipped, in file order.
    pub only_in_a: Vec<String>,
    /// Up to 20 sample lines found only in `b`, clipped, in file order.
    pub only_in_b: Vec<String>,
}

/// The data for the instruction-hub view: which agent-instruction files the
/// repo root carries and how they disagree. `R-H5`. mogeung shows the drift
/// and offers copy-out; it never writes the canonical copy anywhere.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InstructionDrift {
    pub files: Vec<InstructionFile>,
    pub pairs: Vec<InstructionPair>,
}

/// Everything a client needs to render the Docs view for one repo.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocInventory {
    /// Every markdown file found, sorted by path.
    pub docs: Vec<DocInfo>,
    /// Doc × referenced-path pairs that have drifted. `R-H2`.
    pub stale: Vec<StaleDoc>,
    /// Proposals only. `R-H3`.
    pub gc: Vec<GcCandidate>,
    /// One entry per doc that carries checklist items. `R-H4`.
    pub plans: Vec<PlanCheck>,
    /// Present only when more than one instruction file exists at the repo
    /// root — one file has nothing to drift from. `R-H5`.
    pub instruction_drift: Option<InstructionDrift>,
    /// The walk hit the file-count cap, or a doc exceeded the per-file size
    /// cap and was scanned clipped.
    pub truncated: bool,
    pub generated_at: Option<DateTime<Utc>>,
}
