//! Cross-session intelligence types. Pillar F (`R-F1`–`R-F9`).
//!
//! Everything here is evidence shaped for the wire: counts, verbatim clips
//! and timestamps mined from transcripts and `history.jsonl`. Nothing is a
//! summary of assistant prose, and nothing claims authority — decision
//! extraction ships as *candidates* with the pattern that matched, because
//! presenting judgment as authority is forbidden (pillar K).
//!
//! Pure data: no IO, no async — the core-crate rule.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Search (R-F1)
// ---------------------------------------------------------------------------

/// Where a search hit came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    /// Any transcript line that is not a human prompt — assistant text, tool
    /// traffic, subagent chatter, metadata.
    #[default]
    Transcript,
    /// A line of `history.jsonl` — the prompt box's own log.
    History,
    /// A human prompt line inside a transcript, split out so a client can
    /// offer "search only what I typed".
    Prompt,
}

/// One line that matched the query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub source: SearchSource,
    /// The owning session. Subagent transcripts attribute to their parent
    /// session (the directory name), so a hit is always openable.
    pub session_id: String,
    /// 1-based line number in the file the hit was found in.
    pub line: u64,
    /// The line's own timestamp, when it carried one.
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    /// The line's `type` (`user`, `assistant`, …), or `prompt` for history
    /// hits. A label, not a schema promise.
    #[serde(default)]
    pub role: String,
    /// A clip of the raw line around the match, ≤ ~200 chars, char-boundary
    /// safe. Raw evidence — no reformatting.
    pub preview: String,
}

/// Everything one search pass produced.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// The total cap was reached and the scan stopped early.
    #[serde(default)]
    pub truncated: bool,
    /// Files the pass looked at (including `history.jsonl` when readable).
    #[serde(default)]
    pub files_scanned: u32,
    /// Files whose own per-file cap was hit — their tail went unsearched.
    #[serde(default)]
    pub files_truncated: u32,
}

// ---------------------------------------------------------------------------
// history.jsonl
// ---------------------------------------------------------------------------

/// One pasted attachment recorded alongside a prompt. Both variants are real
/// in the corpus: inline `content`, or only a `contentHash` pointing at
/// content stored elsewhere.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PastedContent {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_hash: Option<String>,
}

/// One line of `history.jsonl` — a prompt as typed, with where and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The prompt text, verbatim.
    pub display: String,
    /// Unix milliseconds, as the file stores it.
    pub timestamp_ms: i64,
    /// Absolute cwd of the session the prompt was typed into.
    pub project: String,
    pub session_id: String,
    #[serde(default)]
    pub pasted: Vec<PastedContent>,
}

/// A parsed `history.jsonl`, with the parse's health on the side.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    /// In file order, which the corpus keeps strictly timestamp-ordered.
    pub entries: Vec<HistoryEntry>,
    /// Lines skipped because they did not parse or lacked required keys.
    /// Non-zero is a schema-drift question, not a rendering question.
    #[serde(default)]
    pub malformed_lines: u32,
}

// ---------------------------------------------------------------------------
// Day digest (R-F3)
// ---------------------------------------------------------------------------

/// One session's measurable activity on one local day. Counts only — the
/// spec forbids summarising assistant self-reports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionDigest {
    pub session_id: String,
    /// The CLI's generated title, when an `ai-title` line exists.
    #[serde(default)]
    pub title: Option<String>,
    /// Last path segment of the session's cwd — display attribution only.
    #[serde(default)]
    pub repo: String,
    /// Human prompts that day (subagent traffic excluded — those prompts are
    /// the parent's tool calls, not a human typing).
    pub turns: u32,
    /// Tool calls that day, subagents included and attributed to the parent.
    pub tool_calls: u32,
    /// Files whose edit tools or file-history deltas named them, deduped.
    #[serde(default)]
    pub files_touched: Vec<String>,
    /// API-error lines (`isApiErrorMessage` or a non-null `error` field).
    pub errors: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// Every session active on one local day. Evidence, never narrative.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayDigest {
    /// Local `YYYY-MM-DD` — "what did I do Tuesday" is a local question.
    pub day: String,
    /// Largest burn first.
    pub sessions: Vec<SessionDigest>,
    #[serde(default)]
    pub files_scanned: u32,
}

// ---------------------------------------------------------------------------
// Recurring failures (R-F4)
// ---------------------------------------------------------------------------

/// An error shape seen in several distinct sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringFailure {
    /// The normalised form the group was keyed on (lowercased, digits → `#`,
    /// paths → `<path>`, whitespace collapsed). Shown so the grouping can be
    /// audited, because a normalisation can lie by merging too much.
    pub normalized: String,
    /// One verbatim example, clipped — the evidence behind the group.
    pub example: String,
    /// Distinct sessions the shape appeared in, sorted.
    pub sessions: Vec<String>,
    /// Total occurrences across those sessions.
    pub count: u32,
}

// ---------------------------------------------------------------------------
// Prompt library (R-F6)
// ---------------------------------------------------------------------------

/// A cluster of near-duplicate prompts — the reuse list's row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptCluster {
    /// The most recent member, verbatim.
    pub representative: String,
    pub count: u32,
    pub first_used: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
    /// Distinct projects the prompt was used in, sorted.
    #[serde(default)]
    pub projects: Vec<String>,
}

// ---------------------------------------------------------------------------
// Analytics (R-F5)
// ---------------------------------------------------------------------------

/// A count on one local day.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayCount {
    pub day: String,
    pub count: u32,
}

/// One project's totals from prompt history.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectCount {
    /// The absolute cwd, as `history.jsonl` records it.
    pub project: String,
    pub prompts: u32,
    /// Distinct sessions prompted in this project.
    pub sessions: u32,
    /// Whether a transcript directory for this project still exists — the
    /// history→directory mapping is lossy (54 projects vs 37 dirs on the
    /// reference machine), and a client should say when it cannot drill in.
    #[serde(default)]
    pub has_transcripts: bool,
}

/// Prompt-history analytics. Tokens per day are deliberately absent — the
/// usage module (pillar G) owns burn, and two modules computing one number
/// is how dashboards drift apart.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Analytics {
    /// Ascending by day.
    pub prompts_per_day: Vec<DayCount>,
    /// Distinct sessions prompted per day, ascending.
    pub sessions_per_day: Vec<DayCount>,
    /// Most-prompted first.
    pub projects: Vec<ProjectCount>,
    /// Prompts by local hour of day, `[0]` = midnight–1am.
    pub hour_histogram: [u32; 24],
}

// ---------------------------------------------------------------------------
// Subagent tree (R-F8)
// ---------------------------------------------------------------------------

/// One `subagents/agent-*.jsonl` file under a session, summarised.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SubagentNode {
    /// The file stem, e.g. `agent-a1b2c3`.
    pub agent_id: String,
    pub lines: u32,
    #[serde(default)]
    pub first_ts: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_ts: Option<DateTime<Utc>>,
    /// The last thing the agent did — final tool call (`Name: summary`) or a
    /// clip of its final text.
    #[serde(default)]
    pub last_activity: Option<String>,
    pub tokens_out: u64,
}

// ---------------------------------------------------------------------------
// Decision candidates (R-F7)
// ---------------------------------------------------------------------------

/// A decision-shaped sentence found by pattern, offered for a human to skim.
/// A *candidate*: `pattern` names exactly which honest pattern matched, so
/// the UI can never present the extraction as more than a regex's opinion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionCandidate {
    /// 1-based line in the transcript.
    pub line: u64,
    #[serde(default)]
    pub timestamp: Option<DateTime<Utc>>,
    /// The pattern that matched, e.g. `instead of`.
    pub pattern: String,
    /// The sentence, clipped to ≤ ~300 chars.
    pub text: String,
}

// ---------------------------------------------------------------------------
// Turns near a moment (R-F9)
// ---------------------------------------------------------------------------

/// A human turn or assistant text line near a point in time — the anchor for
/// "open the transcript at the commit".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearbyTurn {
    /// 1-based line in the transcript.
    pub line: u64,
    pub timestamp: DateTime<Utc>,
    /// `user` or `assistant`.
    pub role: String,
    pub preview: String,
}

// ---------------------------------------------------------------------------
// Prompt-blame (R-F2)
// ---------------------------------------------------------------------------

/// One session's connection to a file — who touched it and what they were
/// told. Heuristic, and each row says how it matched: file attribution is
/// A8's known weakness (two sessions in one worktree cannot be separated
/// beyond their tool calls), so the match rule travels with the answer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FileSession {
    pub session_id: String,
    /// The session's best label at answer time.
    pub label: String,
    /// The most recent prompt the session had when it touched the file —
    /// the prompt that *probably* drove the edit, and the row says so.
    pub prompt: Option<String>,
    /// When the session last touched the file, when known.
    pub at: Option<DateTime<Utc>>,
    /// How this row matched, e.g. `edit-tool path match` — auditable, never
    /// implicit.
    pub matched: String,
}
