//! What mogeung can see — and, more importantly, what it cannot.
//!
//! Everything mogeung knows comes from two undocumented file formats. The
//! parser ignores what it does not recognise rather than crashing, which means
//! the realistic failure is **seeing less than we should** — and that looks
//! exactly like a quiet day.
//!
//! These types exist to make that failure loud. They are the wire form of
//! roadmap `R-A1`, `R-A2`, `R-A4` and `R-A5`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What one transcript line turned out to be.
///
/// The distinction that matters is between [`Ignored`](LineClass::Ignored) and
/// [`Unknown`](LineClass::Unknown). Both produce no data; only one of them means
/// the format moved under us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineClass {
    /// Understood, and it produced something.
    Parsed,
    /// A type we know about and deliberately do not surface.
    Ignored,
    /// A type we handle, but this line yielded nothing. Normal in small
    /// numbers; a spike means the shape of a type we rely on has changed.
    Barren,
    /// A `type` we have never seen. This is the canary.
    Unknown,
    /// Not JSON, or no `type` field at all.
    Malformed,
}

/// Something worth telling the user about, in plain language.
///
/// Alerts are facts, not thresholds. A never-before-seen event type is
/// interesting the first time it appears; there is nothing to smooth or tune.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Alert {
    /// A transcript event type nobody has classified.
    UnknownEventType { event_type: String, count: u64 },
    /// Lines that were not valid JSON, or carried no `type`.
    MalformedLines { count: u64 },
    /// Claude Code changed version while we were watching. Not a problem in
    /// itself — but it is when formats move, so it is worth a look.
    VersionChanged { from: String, to: String },
    /// A transcript was too big to read whole, so its early history was never
    /// seen. Stated rather than silent.
    HistorySkipped {
        session_id: String,
        skipped_bytes: u64,
    },
    /// A `launch.json` / `tasks.json` type nobody has classified. `R-N2`.
    ///
    /// Separate from [`Alert::UnknownEventType`] because the consequence is
    /// different: a transcript event we do not know is **dropped**, where a run
    /// configuration we do not know is still **listed**, named and refused. So
    /// this is a prompt to decide, not a warning that the board is lying.
    UnknownRunConfigType { ty: String, count: u64 },
}

impl Alert {
    /// One line, written for a human who has not read the source.
    pub fn message(&self) -> String {
        match self {
            Alert::UnknownEventType { event_type, count } => format!(
                "Unrecognised transcript event {event_type:?} seen {count}× — \
                 mogeung is skipping it. The format may have changed."
            ),
            Alert::MalformedLines { count } => format!(
                "{count} transcript line(s) could not be read as JSON. \
                 Anything they contained is missing from the board."
            ),
            Alert::VersionChanged { from, to } => format!(
                "Claude Code is now {to}, was {from}. Formats move on upgrades — \
                 check the board still looks right."
            ),
            Alert::HistorySkipped {
                session_id,
                skipped_bytes,
            } => format!(
                "Session {} was too large to read whole; {} of earlier history \
                 was skipped and is not on the board.",
                short_id(session_id),
                human_bytes(*skipped_bytes)
            ),
            Alert::UnknownRunConfigType { ty, count } => format!(
                "Run configuration type {ty:?} seen {count}× — mogeung lists it but \
                 cannot run it. Classify it in runconfig::HANDLED or KNOWN_IGNORED."
            ),
        }
    }

    /// Whether this warrants interrupting the user's attention.
    ///
    /// A skipped tail is a stated limitation, not a fault; the other three mean
    /// mogeung may be lying by omission.
    pub fn is_urgent(&self) -> bool {
        // A listed-but-unrunnable configuration is a decision waiting to be
        // taken, not a board that is wrong — the entry is on screen with its
        // type named, which is the whole of ADR-0026's "hide nothing".
        !matches!(
            self,
            Alert::HistorySkipped { .. } | Alert::UnknownRunConfigType { .. }
        )
    }
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

pub fn human_bytes(n: u64) -> String {
    const K: u64 = 1024;
    match n {
        0..K => format!("{n} B"),
        _ if n < K * K => format!("{:.0} KB", n as f64 / K as f64),
        _ if n < K * K * K => format!("{:.1} MB", n as f64 / (K * K) as f64),
        _ => format!("{:.1} GB", n as f64 / (K * K * K) as f64),
    }
}

/// A snapshot of how well mogeung is reading the world.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub last_scan: Option<DateTime<Utc>>,
    pub scans: u64,

    /// Sessions on the board, and how many are live right now.
    pub sessions_known: u64,
    pub sessions_live: u64,
    pub transcripts_found: u64,

    pub lines_seen: u64,
    pub lines_parsed: u64,
    pub lines_ignored: u64,
    pub lines_barren: u64,
    pub lines_unknown: u64,
    pub lines_malformed: u64,

    /// Every unrecognised `type`, with how often it has appeared.
    pub unknown_types: Vec<(String, u64)>,
    /// Every Claude Code version seen anywhere in the watched history, sorted.
    ///
    /// Historical only. A fortnight of transcripts routinely spans a dozen
    /// releases, so this is context, not a signal.
    pub versions_seen: Vec<String>,
    /// The version behind the most recent session activity — what you are
    /// actually running now.
    pub current_version: Option<String>,
    /// Bytes of transcript history skipped because a file was over the cap.
    pub history_skipped_bytes: u64,
    /// The cap itself, so the number above has meaning.
    pub max_transcript_bytes: u64,

    pub alerts: Vec<Alert>,

    /// Every non-Claude agent CLI being watched, one entry each. `R-I15`.
    ///
    /// Replaced the four flat `codex_*` fields below, which could describe
    /// exactly one other agent. Those are still populated, and will be until
    /// no client reads them, because a snapshot is a wire type and dropping a
    /// field is a break.
    #[serde(default)]
    pub agents: Vec<AgentHealth>,

    /// The local model seam, if one is configured. `R-O1`.
    ///
    /// Defaulted, like `agents`, so a snapshot from a daemon built before this
    /// still parses. Never probed on the scan tick — ADR-0030 clause 6 — so
    /// `last_error` and `last_ok_ms` are the residue of asks that happened,
    /// and "reachable" is deliberately not a field: it would be a claim about
    /// a moment that has passed by the time it is rendered.
    #[serde(default)]
    pub model: Option<crate::model::ModelHealth>,
    /// mogeung's own llmproxy in front of the model, when one is asked for.
    /// `R-O10`. `None` from a daemon that predates it.
    #[serde(default)]
    pub proxy: Option<crate::llmproxy::ProxyHealth>,

    // -- Codex (R-I1). Defaulted so an older daemon's snapshot still parses.
    /// A `~/.codex` install exists and is being watched.
    ///
    /// **Superseded by `agents`.** Kept so a client built before `R-I15` keeps
    /// working; new readers should use `agents` and get every CLI, not just
    /// the second one.
    #[serde(default)]
    pub codex_present: bool,
    /// Unarchived Codex threads seen in its index. Zero with `codex_present`
    /// is itself information: present, watched, empty.
    #[serde(default)]
    pub codex_threads: u32,
    /// The index could not be read; the string says why.
    #[serde(default)]
    pub codex_error: Option<String>,
    /// Rollout line kinds the Codex parser did not recognise — its canary,
    /// same philosophy as `unknown_types`. Replaced wholesale each scan.
    #[serde(default)]
    pub codex_unknown: Vec<(String, u64)>,
}

/// What mogeung can see of one other agent CLI's install.
///
/// One of these per non-Claude source. The Claude Code figures stay in
/// `Health`'s own fields: it is not "another agent" to a product whose whole
/// corpus, canary and version tracking were built around it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AgentHealth {
    /// The wire name of the `SessionSource` — `codex`, `qwen`.
    pub source: String,
    /// Its home directory exists and is being watched.
    pub present: bool,
    /// Sessions seen there. Zero with `present` is itself information:
    /// present, watched, empty.
    pub threads: u32,
    /// Its index or home could not be read; the string says why.
    pub error: Option<String>,
    /// Line kinds this CLI's parser did not recognise — its canary, same
    /// philosophy as `unknown_types`. Replaced wholesale each scan.
    pub unknown: Vec<(String, u64)>,
    /// Directories this CLI has been granted, where it has such a notion.
    /// Codex asks per directory and records the answer; a launch into one that
    /// is missing here stops on a prompt, which is invisible when the launch
    /// was headless. Empty for a CLI with no such concept, and
    /// `#[serde(default)]` so a client built before this parses unchanged.
    /// `R-J74`.
    #[serde(default)]
    pub trusted_dirs: Vec<String>,
}

impl Health {
    /// Share of lines mogeung could not account for.
    ///
    /// Deliberately excludes `Ignored`: skipping bookkeeping we have classified
    /// is not blindness. Only unknown and malformed lines represent data we
    /// might have wanted and did not get.
    pub fn blind_ratio(&self) -> f32 {
        if self.lines_seen == 0 {
            return 0.0;
        }
        (self.lines_unknown + self.lines_malformed) as f32 / self.lines_seen as f32
    }

    pub fn urgent_alerts(&self) -> usize {
        self.alerts.iter().filter(|a| a.is_urgent()).count()
    }

    /// A one-line verdict for a status bar.
    pub fn headline(&self) -> String {
        if self.lines_seen == 0 {
            return "No transcript lines read yet".into();
        }
        let urgent = self.urgent_alerts();
        if urgent > 0 {
            return format!("{urgent} thing(s) mogeung cannot see");
        }
        "Reading everything it recognises".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_lines_are_not_blindness() {
        // A board that skipped 900 bookkeeping lines and understood 100 is
        // perfectly healthy; one that failed on 1 is not.
        let healthy = Health {
            lines_seen: 1000,
            lines_parsed: 100,
            lines_ignored: 900,
            ..Default::default()
        };
        assert_eq!(healthy.blind_ratio(), 0.0);

        let blind = Health {
            lines_seen: 1000,
            lines_unknown: 10,
            ..Default::default()
        };
        assert!((blind.blind_ratio() - 0.01).abs() < 1e-6);
    }

    #[test]
    fn a_skipped_tail_is_stated_but_not_urgent() {
        let skipped = Alert::HistorySkipped {
            session_id: "abcdef123456".into(),
            skipped_bytes: 5 * 1024 * 1024,
        };
        assert!(!skipped.is_urgent());
        // The message must name the size, or "some history was skipped" is
        // useless to act on.
        assert!(skipped.message().contains("5.0 MB"));
        assert!(skipped.message().contains("abcdef12"));

        let unknown = Alert::UnknownEventType {
            event_type: "warp-drive".into(),
            count: 3,
        };
        assert!(unknown.is_urgent());
        assert!(unknown.message().contains("warp-drive"));
    }

    #[test]
    fn headline_reports_only_urgent_alerts() {
        let h = Health {
            lines_seen: 10,
            alerts: vec![Alert::HistorySkipped {
                session_id: "x".into(),
                skipped_bytes: 1,
            }],
            ..Default::default()
        };
        assert_eq!(h.headline(), "Reading everything it recognises");
    }
}
