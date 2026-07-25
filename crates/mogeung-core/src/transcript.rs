use crate::run::RunId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A structured transcript event.
///
/// Deliberately *not* terminal output. The daemon owns the agent's stdout, so a
/// run is stored as typed events that can be searched, linked and diffed. See
/// CONCEPT.md F4 for why there is no embedded terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEvent {
    pub run_id: RunId,
    /// Monotonic per run, assigned by the daemon on ingest.
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum EventKind {
    /// Session bootstrap: model, tools, cwd.
    Init {
        model: String,
        cwd: String,
        tool_count: usize,
    },
    /// A prompt sent to the agent (initial intent or a follow-up).
    UserPrompt { text: String },
    /// Prose from the agent.
    AssistantText { text: String },
    /// Agent's internal reasoning, when the CLI surfaces it.
    Thinking { text: String },
    ToolUse {
        tool_use_id: String,
        name: String,
        /// One-line human summary, e.g. the bash command or the file path.
        summary: String,
    },
    ToolResult {
        tool_use_id: String,
        is_error: bool,
        preview: String,
    },
    /// Terminal event for one agent turn.
    Result {
        cost_usd: f64,
        num_turns: u32,
        terminal_reason: String,
        is_error: bool,
        text: String,
    },
    /// Something mogeung itself wants on the record (spawn failure, kill, parse error).
    Notice { level: NoticeLevel, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warn,
    Error,
}

impl EventKind {
    /// Short label for the transcript gutter.
    pub fn label(&self) -> &'static str {
        match self {
            EventKind::Init { .. } => "init",
            EventKind::UserPrompt { .. } => "prompt",
            EventKind::AssistantText { .. } => "agent",
            EventKind::Thinking { .. } => "think",
            EventKind::ToolUse { .. } => "tool",
            EventKind::ToolResult { .. } => "result",
            EventKind::Result { .. } => "done",
            EventKind::Notice { .. } => "notice",
        }
    }
}
