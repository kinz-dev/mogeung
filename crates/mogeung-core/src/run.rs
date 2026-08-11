//! A run configuration, and the two rules that constrain every one of them.
//! `R-N3`.
//!
//! This is the contract half of Run and Debug: the shape a client is offered
//! and the shape a run request names. It lives in core because the daemon and
//! every client have to agree about it, and it has no I/O for the reason the
//! crate note gives — producing these is
//! [`detect`](../../mogeungd/src/detect.rs)'s job, and reading somebody else's
//! file is [`runconfig`](../../mogeungd/src/runconfig.rs)'s.
//!
//! # The id is the security boundary
//!
//! [ADR-0025](../../../docs/decisions/0025-run-a-process-you-named-never-an-agent.md)
//! clause 1 is *named, not supplied*: the wire carries `run config #3 of
//! session X`, never a command string. So [`RunConfig::id`] is not a
//! convenience — it is the whole reason an unauthenticated loopback port is not
//! a remote shell. It has to be **stable across restarts**, because a client
//! holds one and asks for it later, and **derived from the command** rather
//! than from a counter, because a counter would silently re-point at a
//! different configuration when the list changed underneath it.
//!
//! # Inferred is said out loud
//!
//! [ADR-0026](../../../docs/decisions/0026-other-peoples-run-configurations.md)
//! made detection the source rather than a fallback, and named the cost:
//! *"a wrong inference is now a first impression"*. [`Origin`] is carried all
//! the way to the surface so the panel can say which entries mogeung guessed at
//! and which a human wrote down.

use serde::{Deserialize, Serialize};

/// Where a configuration came from. Shown, not just recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Inferred from a manifest by a closed, compiled-in detector. Labelled as
    /// inferred wherever it is shown.
    Detected,
    /// Read out of `.vscode/launch.json` or `tasks.json` — a human wrote it, so
    /// it wins over a detected entry with the same command.
    VsCode,
}

impl Origin {
    /// The word the panel puts beside an entry.
    pub fn label(self) -> &'static str {
        match self {
            Origin::Detected => "inferred",
            Origin::VsCode => "launch.json",
        }
    }
}

/// One thing mogeung could run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunConfig {
    /// Stable, derived from origin, directory and command. **This is what a run
    /// request names** — see the module note.
    pub id: String,
    /// What to call it. A human's own name when there was one.
    pub name: String,
    /// `argv[0]`, and the only thing ADR-0025 clause 2 gets to inspect.
    pub program: String,
    pub args: Vec<String>,
    /// Relative to the repository root; empty means the root itself. Relative
    /// rather than absolute so a configuration means the same thing to a
    /// remote daemon, where the repository is at a different path.
    pub dir: String,
    pub origin: Origin,
    /// `Some(reason)` when the entry is **listed but cannot be started**, in
    /// words a person can act on.
    ///
    /// `R-N2`'s rule is *hide nothing*: a configuration mogeung cannot run is
    /// still shown, because one that silently vanishes reads as *"mogeung did
    /// not find it"*. There are three ways to end up here and the reason
    /// distinguishes them — a type nobody classified, a type we decline (an
    /// `attach` needs a debugger this cut does not have), and an entry that
    /// names no program at all.
    pub unrunnable: Option<String>,
}

impl RunConfig {
    /// Build one, deriving the id.
    pub fn new(
        origin: Origin,
        name: impl Into<String>,
        program: impl Into<String>,
        args: Vec<String>,
        dir: impl Into<String>,
    ) -> Self {
        let (name, program, dir) = (name.into(), program.into(), dir.into());
        let id = id_for(origin, &dir, &program, &args);
        RunConfig { id, name, program, args, dir, origin, unrunnable: None }
    }

    /// Listed, named, and refused, with the reason shown. `R-N2`.
    pub fn cannot_run(mut self, why: impl Into<String>) -> Self {
        self.unrunnable = Some(why.into());
        self
    }

    /// A type neither list has an opinion about. Names the type, because that
    /// is the thing somebody has to make a decision about.
    pub fn unclassified(self, ty: &str) -> Self {
        self.cannot_run(format!(
            "`{ty}` is not a configuration type mogeung knows how to run — it is listed \
             so you can see it exists, and classifying it is a one-line decision"
        ))
    }

    /// The command as a person would type it. Display and **deduplication** —
    /// a human's entry beats a detected one with the same command, and this is
    /// what "the same command" means.
    pub fn command_line(&self) -> String {
        let mut out = self.program.clone();
        for a in &self.args {
            out.push(' ');
            out.push_str(a);
        }
        out
    }

    /// Can this be started? `R-N2`'s unrunnable entries still appear in lists.
    pub fn runnable(&self) -> bool {
        self.unrunnable.is_none()
    }
}

/// `detected:desktop:npm-test`, and it does not move between runs.
///
/// Readable rather than a hash, deliberately: an id shows up in a wire payload,
/// a log line and a bug report, and *"which configuration is `a3f19c`"* is a
/// question with no cheap answer. The directory is part of it because
/// `npm test` in `desktop/` and `npm test` at the root are two different runs.
fn id_for(origin: Origin, dir: &str, program: &str, args: &[String]) -> String {
    let prefix = match origin {
        Origin::Detected => "detected",
        Origin::VsCode => "vscode",
    };
    let mut parts = vec![program.to_string()];
    parts.extend(args.iter().cloned());
    let command = slug(&parts.join("-"));
    let place = if dir.is_empty() { String::new() } else { format!("{}:", slug(dir)) };
    format!("{prefix}:{place}{command}")
}

/// Lowercase, and anything that is not a letter or digit becomes a dash.
fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Where a run is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    /// Finished on its own, with the code it returned.
    Exited,
    /// Stopped by a person. Deliberately not the same as `Exited`: *"you
    /// stopped it"* and *"it failed"* are different answers to **did the tests
    /// pass**, and `R-N7` shows this beside a claim.
    Stopped,
    /// Never started — the program could not be spawned at all.
    Failed,
}

/// One run, as a client sees it. `R-N4`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub config_id: String,
    /// Copied from the configuration, so a client needs only this to draw a row.
    pub name: String,
    pub command: String,
    /// The session this was started from, when it was started from one — the
    /// whole of `R-N7`'s mechanism.
    pub session_id: Option<crate::session::SessionId>,
    pub state: RunState,
    pub exit_code: Option<i32>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    /// How many lines fell off the front of the buffer. **Stated, never
    /// silent** — a log that quietly loses its middle is worse than one that
    /// admits it.
    pub dropped: u64,
}

impl Run {
    /// Did this run actually pass? `None` while it is still going.
    ///
    /// A **stopped** run is not a pass and not a failure: nobody let it finish.
    /// `R-N7` needs that third answer, because showing "stopped" as a red cross
    /// beside a claim would be mogeung asserting something it does not know.
    pub fn passed(&self) -> Option<bool> {
        match self.state {
            RunState::Running => None,
            RunState::Stopped => None,
            RunState::Failed => Some(false),
            RunState::Exited => Some(self.exit_code == Some(0)),
        }
    }
}

/// A line of output, as it arrives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunLine {
    pub run_id: String,
    pub seq: u64,
    pub text: String,
    /// `true` for stderr. Kept apart so a panel can colour it, and interleaved
    /// in arrival order so the sequence still reads like a terminal.
    pub stderr: bool,
}

/// The agent CLIs mogeung must never start. ADR-0025 clause 2.
///
/// Matched on the **program**, not the whole command line, and the limit is
/// known rather than overlooked: a shell script in the repository that goes on
/// to call `claude` walks straight past this. ADR-0025 says so in terms and
/// says what to do if it happens in practice — write a successor, not a quiet
/// widening of the check.
pub const AGENTS: &[&str] = &["claude", "codex", "gemini", "aider", "cursor-agent", "amp"];

/// Would starting this be starting an agent? ADR-0025 clause 2.
///
/// The file stem is compared, so `/usr/local/bin/claude` and `claude.exe` are
/// both caught — a check that only matched the bare word would be defeated by
/// the ordinary way a configuration names a program.
pub fn is_agent(program: &str) -> bool {
    let stem = std::path::Path::new(program)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(program);
    AGENTS.iter().any(|a| stem.eq_ignore_ascii_case(a))
}

/// Why a run was refused, in words that name the clause.
///
/// ADR-0025's consequences say this outright: *"a refusal path exists that will
/// look like a bug… the refusal has to say which clause it hit."*
pub fn agent_refusal(program: &str) -> String {
    format!(
        "refusing to start `{program}`: mogeung watches agents you started and never starts one \
         (ADR-0025 clause 2). Run it yourself — mogeung will pick the session up on its next scan."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The id is what a run request names, so it must not move between runs.
    #[test]
    fn the_id_is_stable_and_derived_from_the_command() {
        let a = RunConfig::new(Origin::Detected, "test", "cargo", vec!["test".into()], "");
        let b = RunConfig::new(Origin::Detected, "a different name", "cargo", vec!["test".into()], "");
        assert_eq!(a.id, b.id, "the name is not part of identity");
        assert_eq!(a.id, "detected:cargo-test");
    }

    /// `npm test` in `desktop/` and `npm test` at the root are two runs, and an
    /// id that could not tell them apart would start the wrong one.
    #[test]
    fn the_directory_is_part_of_identity() {
        let root = RunConfig::new(Origin::Detected, "t", "npm", vec!["test".into()], "");
        let sub = RunConfig::new(Origin::Detected, "t", "npm", vec!["test".into()], "desktop");
        assert_ne!(root.id, sub.id);
        assert_eq!(sub.id, "detected:desktop:npm-test");
    }

    /// A human's entry and an inferred one are different configurations even
    /// when they run the same thing — which is what lets one beat the other.
    #[test]
    fn origin_is_part_of_identity() {
        let d = RunConfig::new(Origin::Detected, "t", "cargo", vec!["test".into()], "");
        let v = RunConfig::new(Origin::VsCode, "t", "cargo", vec!["test".into()], "");
        assert_ne!(d.id, v.id);
        assert_eq!(d.command_line(), v.command_line(), "…but the command is the same");
    }

    /// ADR-0025 clause 2, and the path form is the one that matters: a
    /// configuration names a program the way a shell would.
    #[test]
    fn an_agent_is_recognised_however_it_is_spelled() {
        assert!(is_agent("claude"));
        assert!(is_agent("/usr/local/bin/claude"));
        assert!(is_agent("claude.exe"));
        assert!(is_agent("CODEX"));
        assert!(!is_agent("cargo"));
        assert!(!is_agent("npm"));
        // Not a substring match: a project of one's own must not be refused.
        assert!(!is_agent("claude-tools"));
        assert!(!is_agent("my-codex-helper"));
    }

    /// The refusal has to name the clause, or it reads as a broken button.
    #[test]
    fn the_refusal_names_the_decision_it_came_from() {
        let msg = agent_refusal("claude");
        assert!(msg.contains("ADR-0025"), "{msg}");
        assert!(msg.contains("claude"), "{msg}");
    }

    /// An unrunnable entry is still an entry — it is listed, it keeps the
    /// human's own name, and it says what it did not understand. `R-N2`'s
    /// *hide nothing* is the rule being pinned here.
    #[test]
    fn an_unrunnable_entry_is_listed_and_names_its_type() {
        let c = RunConfig::new(Origin::VsCode, "Deck 5", "", vec![], "").unclassified("holodeck");
        assert!(!c.runnable());
        assert_eq!(c.name, "Deck 5", "it keeps the human's own name");
        let why = c.unrunnable.expect("a reason");
        assert!(why.contains("holodeck"), "the type has to be named: {why}");
    }
}
