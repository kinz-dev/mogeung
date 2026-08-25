use crate::verify::{Claim, VerifyRun};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Claude Code's own session id. mogeung does not mint identifiers any more —
/// it observes sessions you started yourself, so their identity is theirs.
pub type SessionId = String;

/// Which agent CLI a session belongs to. `R-I1` — the test of whether the
/// Session model generalises (A23) — and `R-I15`, which is the first source to
/// put real sessions through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    #[default]
    ClaudeCode,
    Codex,
    QwenCode,
}

impl SessionSource {
    pub fn label(&self) -> &'static str {
        match self {
            SessionSource::ClaudeCode => "claude",
            SessionSource::Codex => "codex",
            SessionSource::QwenCode => "qwen",
        }
    }

    /// Does this source's liveness come from Claude Code's `sessions/<pid>.json`
    /// registry?
    ///
    /// Named for the property rather than for the variant. The scan loop used
    /// to ask `source == Codex`, which *meant* "not Claude" — a comparison that
    /// silently gives the wrong answer the moment a third source exists, and
    /// gives it by marking every one of that source's sessions dead on every
    /// pass. Qwen Code has a registry of its own, in its own directory with its
    /// own key names, so the answer is still no.
    pub fn in_claude_live_registry(&self) -> bool {
        match self {
            SessionSource::ClaudeCode => true,
            SessionSource::Codex | SessionSource::QwenCode => false,
        }
    }

    /// Is this source's history built by incrementally folding lines through
    /// `adapter::parse_line`?
    ///
    /// Only Claude sessions are, which is why only they can carry the
    /// re-ingestion damage `repair_reingested_history` exists to undo. Folding
    /// another source's transcript through the Claude adapter would invent
    /// history rather than repair it.
    pub fn has_claude_event_history(&self) -> bool {
        match self {
            SessionSource::ClaudeCode => true,
            SessionSource::Codex | SessionSource::QwenCode => false,
        }
    }
}

/// First-party liveness, read from `~/.claude/sessions/<pid>.json`.
///
/// This is the single most valuable thing the observer model buys: the CLI
/// tells us whether a session is working or waiting, so "needs you" stops being
/// a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveStatus {
    /// Actively working.
    Busy,
    /// Alive but waiting — almost always waiting for you to type something.
    Idle,
    /// Registry reported something we do not model yet.
    Unknown,
}

impl LiveStatus {
    pub fn parse(s: &str) -> Self {
        match s {
            "busy" => LiveStatus::Busy,
            "idle" => LiveStatus::Idle,
            _ => LiveStatus::Unknown,
        }
    }
}

/// A tool call the agent made that has not come back yet.
///
/// The registry says `idle` both when an agent finished its turn and when it is
/// sitting on a permission prompt. Those need very different responses from you,
/// and an unmatched `tool_use` is what tells them apart — see `R-B4`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTool {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub at: DateTime<Utc>,
}

/// A file this session touched, with when. Cumulative `touched_files` cannot
/// answer "are two agents fighting over this *right now*" — `R-B3` needs recency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Touch {
    pub path: String,
    pub at: DateTime<Utc>,
}

/// Two live sessions editing the same file at the same time.
///
/// Only the observer model can see this: it needs a view across sessions that
/// none of them has. `R-B3`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collision {
    pub other: SessionId,
    pub other_label: String,
    pub path: String,
}

/// An agent session mogeung is watching.
///
/// Everything here is derived from files Claude Code writes for its own
/// purposes. mogeung never starts, steers or stops a session — that is what
/// made v0.1 feel like a worse terminal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,

    /// Claude Code's generated title for the conversation, when it has made one.
    pub title: Option<String>,
    /// Human-friendly name from the live registry, e.g. "mogeung-95".
    pub name: Option<String>,
    /// Most recent thing you typed.
    pub last_prompt: Option<String>,

    pub cwd: String,
    /// Enclosing git repository, if the cwd is inside one.
    pub repo_root: Option<String>,
    pub git_branch: Option<String>,

    /// Process id while alive. `None` once the session has exited.
    pub pid: Option<u32>,
    pub alive: bool,
    pub live_status: Option<LiveStatus>,
    /// CLI version that wrote the transcript.
    pub version: Option<String>,

    pub started_at: DateTime<Utc>,
    /// Last append to the transcript. Drives stall detection.
    pub last_event_at: DateTime<Utc>,
    /// When the live registry last changed its mind about `live_status`.
    /// Used to measure how long a session has been waiting for you.
    pub status_since: Option<DateTime<Utc>>,

    pub turns: u32,
    pub tool_calls: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub last_activity: Option<String>,

    /// Files this session touched, from Edit/Write tool calls and file-history
    /// records. Lets a diff be attributed to one session even when several
    /// share a working tree.
    pub touched_files: Vec<String>,

    /// Commit the repo was on when mogeung first saw this session. Diffs are
    /// computed against it when known.
    pub base_sha: Option<String>,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,

    pub error: Option<String>,

    /// Absolute path of the `.jsonl` transcript.
    pub transcript_path: String,
    /// True once every hunk in this session's diff has been read.
    pub reviewed: bool,

    // -- Fields added after v0.2. All `serde(default)`: the store keeps whole
    // -- sessions as JSON blobs, so a row written by an older build must still
    // -- load rather than being dropped as unreadable.
    /// Tool calls with no result yet. Non-empty while idle means the session is
    /// sitting on a permission prompt rather than waiting for a new instruction.
    #[serde(default)]
    pub open_tools: Vec<OpenTool>,

    /// Suppress this session from the queue until this time. `R-B5`.
    #[serde(default)]
    pub snoozed_until: Option<DateTime<Utc>>,

    /// Other live sessions editing the same files right now. `R-B3`.
    #[serde(default)]
    pub collisions: Vec<Collision>,

    /// Set when the session keeps repeating the same tool on the same path —
    /// thrashing rather than progress. `R-B7`.
    #[serde(default)]
    pub loop_signal: Option<String>,

    /// Recently touched files, newest last, capped. Feeds collision detection.
    #[serde(default)]
    pub recent_touches: Vec<Touch>,

    /// The tmux pane this session runs in, e.g. `mogeung-app:0.0`, when it was
    /// started under tmux — which `yolomo` does and a bare `claude` does not.
    ///
    /// `None` is the ordinary case, not a failure: it means mogeung can point
    /// you at the session but cannot host it. Resolved by the daemon so a client
    /// never has to work out the mapping for itself. `R-B18`.
    #[serde(default)]
    pub tmux_target: Option<String>,

    /// Recent `tool:path` keys, newest last, capped. Feeds loop detection.
    #[serde(default)]
    pub recent_tools: Vec<String>,

    /// When the session last hit the five-hour rate limit, recognised by the
    /// synthetic assistant message the CLI writes. Cleared by the next human
    /// turn or the next real assistant output — either means it is moving
    /// again. `R-G1`.
    #[serde(default)]
    pub limit_hit_at: Option<DateTime<Utc>>,
    /// The reset phrase from that message, verbatim (e.g. `8pm
    /// (Europe/London)`). Prose from the CLI, not a parsed clock time.
    #[serde(default)]
    pub limit_resets: Option<String>,

    /// Build/test/typecheck-shaped commands this session ran, with outcomes.
    /// Newest last, capped. `R-E1`.
    #[serde(default)]
    pub verify_runs: Vec<VerifyRun>,
    /// "Tests pass"-shaped assertions in assistant prose, each bound to the
    /// evidence that supports it — or visibly to none. `R-E3`.
    #[serde(default)]
    pub claims: Vec<Claim>,

    /// Which CLI wrote this session. Defaults to Claude Code so every row
    /// persisted before `R-I1` still loads as what it was.
    #[serde(default)]
    pub source: SessionSource,

    /// Folders the CLI confirmed this session may work in, from `/add-dir`.
    /// `R-J39`.
    ///
    /// The *CLI agreeing*, not the user asking: the confirmation line is
    /// written after the directory was accepted, so a typo'd `/add-dir` leaves
    /// nothing here. Only ever a **suggestion** — mogeung's own read boundary
    /// still widens exactly when you widen it, in the UI.
    #[serde(default)]
    pub announced_dirs: Vec<String>,
}

impl Session {
    pub fn seconds_since_activity(&self, now: DateTime<Utc>) -> i64 {
        (now - self.last_event_at).num_seconds().max(0)
    }

    pub fn duration_secs(&self, now: DateTime<Utc>) -> i64 {
        let end = if self.alive { now } else { self.last_event_at };
        (end - self.started_at).num_seconds().max(0)
    }

    /// How long this session has been waiting for you, if it is.
    pub fn waiting_secs(&self, now: DateTime<Utc>) -> Option<i64> {
        if self.alive && self.live_status == Some(LiveStatus::Idle) {
            let since = self.status_since.unwrap_or(self.last_event_at);
            Some((now - since).num_seconds().max(0))
        } else {
            None
        }
    }

    pub fn has_diff(&self) -> bool {
        self.files_changed > 0
    }

    /// Best available label, in descending order of usefulness.
    pub fn label(&self) -> String {
        self.title
            .clone()
            .or_else(|| self.last_prompt.clone())
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| format!("session {}", &self.id[..8.min(self.id.len())]))
    }

    pub fn repo_name(&self) -> String {
        let p = self.repo_root.as_deref().unwrap_or(&self.cwd);
        p.rsplit('/').next().unwrap_or(p).to_string()
    }

    /// The tool this session is blocked on, if it is blocked rather than simply
    /// finished.
    ///
    /// Requires *both* an unmatched tool call and an idle registry status. A
    /// busy session with open tools is just working; the tool has not come back
    /// yet and nobody needs to do anything.
    pub fn awaiting_permission(&self) -> Option<&OpenTool> {
        if self.alive && self.live_status == Some(LiveStatus::Idle) {
            self.open_tools.last()
        } else {
            None
        }
    }

    pub fn is_snoozed(&self, now: DateTime<Utc>) -> bool {
        self.snoozed_until.map(|t| t > now).unwrap_or(false)
    }

    pub fn snooze_remaining(&self, now: DateTime<Utc>) -> Option<i64> {
        self.snoozed_until
            .filter(|t| *t > now)
            .map(|t| (t - now).num_seconds())
    }

    /// Changed code and never saw a build/test/typecheck command complete.
    /// A pending-forever run does not count as verification; a *failed* one
    /// does — they checked, and the answer was no. `R-E4`.
    pub fn unverified(&self) -> bool {
        !self.touched_files.is_empty() && !self.verify_runs.iter().any(|r| r.ok.is_some())
    }

    /// Files touched within the last `window_secs`.
    pub fn files_touched_since(&self, now: DateTime<Utc>, window_secs: i64) -> Vec<&str> {
        let cutoff = now - chrono::Duration::seconds(window_secs);
        let mut out: Vec<&str> = self
            .recent_touches
            .iter()
            .filter(|t| t.at >= cutoff)
            .map(|t| t.path.as_str())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}
