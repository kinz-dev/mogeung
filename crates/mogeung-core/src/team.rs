//! A team of seats, and what each seat is. `R-P3`, feature 0051.
//!
//! The contract half of Gau Geung mode: the shape the window edits and the
//! shape the launcher (`R-P4`) reads. It lives in core for the reason
//! [`run`](crate::run) does — the daemon and every client have to agree about
//! it — and it has no I/O for the same reason: keeping one is
//! `mogeungd::team`'s job, and starting one is the launcher's.
//!
//! # Named, never supplied
//!
//! [ADR-0025](../../../docs/decisions/0025-run-a-process-you-named-never-an-agent.md)'s
//! 2026-10-09 amendment lets a *seat definition* name an agent CLI, and keeps
//! clause 1 whole: the wire carries a definition the daemon already holds under
//! `~/.mogeung`, never a command line. So a [`Seat`] names its CLI as a
//! [`SessionSource`] — the exhaustive four of
//! [ADR-0029](../../../docs/decisions/0029-an-agent-cli-is-a-variant-not-a-plugin.md)
//! — and nothing here is a program, an argument or a flag. The launcher
//! composes those from the variant, through a match a fifth CLI breaks at
//! compile time.
//!
//! # Two layers
//!
//! Decided 2026-10-09: a team carries a guidance every seat gets and a default
//! permission mode; a seat adds its own guidance after the team's and may
//! override the mode. That is why [`Seat::permission`] is an `Option` — `None`
//! is *the team's*, not *none* — and why the two guidance strings are never
//! merged here. The launcher appends them in that order, and the mirror shows
//! them apart, so the shared part is written once rather than four times.
//!
//! # Policy is declared text
//!
//! Every automatic send a team can make is written down here before it can
//! fire — a [`Wake`]'s message, a [`Watchdog`]'s nudge — which is fence 3 of
//! [ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md)'s
//! amendment: *automatic sends are declared text*, readable in the definition
//! and in its markdown mirror. The hop budget and the hourly cap are fence 4's
//! numbers, with the defaults the owner set: three hops, sixty an hour.

use crate::session::SessionSource;
use serde::{Deserialize, Serialize};

/// The four permission modes a seat can be started in, spelled the way Claude
/// Code spells them because that CLI has all four and names them best.
///
/// The launcher maps each to every CLI's own flag — `--permission-mode` for
/// Claude Code, `--approval-mode` for Qwen Code, `--ask-for-approval` and
/// `--sandbox` for Codex, `--auto` for opencode — in `R-P4`'s exhaustive
/// match. A CLI with no equivalent of one (Codex has no plan mode) is refused
/// there, in words, rather than started in something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Ask before every edit and command. The safe default for a definition
    /// nobody has thought about yet.
    #[default]
    Default,
    /// Edit files without asking; still ask before a command.
    AcceptEdits,
    /// Read and plan only; no edits until the plan is approved.
    Plan,
    /// Ask nothing. The `--dangerously-…` flag, said plainly.
    BypassPermissions,
}

impl PermissionMode {
    pub const ALL: [PermissionMode; 4] = [
        PermissionMode::Default,
        PermissionMode::AcceptEdits,
        PermissionMode::Plan,
        PermissionMode::BypassPermissions,
    ];

    /// Claude Code's own word, which is also the wire spelling's camel form.
    pub fn label(self) -> &'static str {
        match self {
            PermissionMode::Default => "default",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Plan => "plan",
            PermissionMode::BypassPermissions => "bypassPermissions",
        }
    }
}

/// What happens to a model-composed handover when a seat restarts or passes
/// the baton. `R-P10`, ADR-0003's later amendment of 2026-10-09.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Handover {
    /// Composed and shown on the seat's row; delivered after you have read it.
    /// The default, because the first handovers are the ones worth reading.
    #[default]
    AfterRead,
    /// Composed and delivered as an automatic send, counted against the cap.
    Automatic,
    /// No handover: the seat restarts cold and the row says so.
    None,
}

/// A seat's event a wake can be declared on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatEvent {
    /// The seat finished a turn (Claude Code's `Stop`, or the equivalent).
    Stop,
    /// The seat's process exited.
    Exit,
}

/// A declared send: when `on_seat` does `on`, `to` is told `text`. `R-P6`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wake {
    /// The seat whose event fires this, by [`Seat::name`].
    pub on_seat: String,
    pub on: SeatEvent,
    /// The seat that receives the text, by name. Must be a different seat.
    pub to: String,
    /// What is typed, verbatim. Declared here, readable before it ever fires.
    pub text: String,
}

/// A declared nudge: a seat silent for `silent_minutes` is told `text`. `R-P6`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchdog {
    /// Which seat, by name; `None` is every seat in the team.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    pub silent_minutes: u32,
    /// Only while the seat is mid-turn. A seat idle at its prompt is waiting
    /// for a person, and nudging that is noise.
    #[serde(default = "yes")]
    pub while_busy: bool,
    pub text: String,
}

fn yes() -> bool {
    true
}

/// One team's rules. Per team, never per seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Agent-to-agent deliveries allowed in a row with no human between.
    /// Zero means every message goes through you. ADR-0003 amendment, fence 4.
    #[serde(default = "default_hop_budget")]
    pub hop_budget: u32,
    /// Automatic sends per hour, across the team. The owner's sixty.
    #[serde(default = "default_hourly_cap")]
    pub hourly_cap: u32,
    #[serde(default)]
    pub handover: Handover,
    /// Press Enter for the one screen shape that asks for it (ADR-0044). Off
    /// until `R-P11`'s week of captures says the shape is read reliably.
    #[serde(default)]
    pub readiness_press: bool,
    #[serde(default)]
    pub wakes: Vec<Wake>,
    #[serde(default)]
    pub watchdogs: Vec<Watchdog>,
}

fn default_hop_budget() -> u32 {
    3
}

fn default_hourly_cap() -> u32 {
    60
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            hop_budget: default_hop_budget(),
            hourly_cap: default_hourly_cap(),
            handover: Handover::default(),
            readiness_press: false,
            wakes: Vec::new(),
            watchdogs: Vec::new(),
        }
    }
}

/// Where a seat works.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Workplace {
    /// A fresh worktree of the team's repository, made at boot and left behind
    /// at stop. The daemon may write there (ADR-0019's amendment).
    #[default]
    Worktree,
    /// The team's own checkout. Nothing of mogeung's is written there.
    Checkout,
}

/// One seat in a team: an agent the daemon will start, in Gau Geung mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seat {
    /// The seat's identity within its team: `reviewer`, `builder`. Lower-case,
    /// digits and dashes, unique in the team — it names a worktree directory
    /// and is what a wake or a send addresses.
    pub name: String,
    /// What the row says, when the name is not enough. Empty means the name.
    #[serde(default)]
    pub label: String,
    /// Which CLI. The exhaustive four; the launcher's match is per variant.
    #[serde(default)]
    pub cli: SessionSource,
    /// The model, in the CLI's own spelling, or `None` for the CLI's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub works_in: Workplace,
    /// `None` is the team's default — see the module note on two layers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<PermissionMode>,
    /// Appended after the team's guidance, as a system-prompt addition and,
    /// in a worktree, as a file.
    #[serde(default)]
    pub guidance: String,
    /// A home of its own under `~/.mogeung` — `CLAUDE_CONFIG_DIR`,
    /// `CODEX_HOME`, `QWEN_HOME` or the XDG pair. Opt-in, with its cost:
    /// a blank seat with its own login and none of your skills, plugins,
    /// memory or MCP servers (ADR-0043 clause 5). Needed for hooks on a Codex
    /// or Qwen seat; optional for Claude Code and opencode.
    #[serde(default)]
    pub own_home: bool,
}

impl Seat {
    pub fn new(name: impl Into<String>, cli: SessionSource) -> Self {
        Seat {
            name: name.into(),
            label: String::new(),
            cli,
            model: None,
            works_in: Workplace::default(),
            permission: None,
            guidance: String::new(),
            own_home: false,
        }
    }

    /// The name on the row.
    pub fn display(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.name
        } else {
            &self.label
        }
    }
}

/// A named set of seats with one policy. The unit of policy, of pause, and of
/// address: a seat talks only to seats of its own team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    /// Minted by the daemon on first save; empty on the wire means *new*.
    #[serde(default)]
    pub id: String,
    pub name: String,
    /// The repository the team works in, absolute. Worktrees are made from
    /// it; a seat in the checkout works in it directly.
    #[serde(default)]
    pub repo: String,
    /// Every seat gets this, before its own.
    #[serde(default)]
    pub guidance: String,
    /// The default a seat inherits unless it overrides.
    #[serde(default)]
    pub permission: PermissionMode,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub seats: Vec<Seat>,
    /// The pause switch: no automatic send while set. Typing in a pane always
    /// works. Part of the definition so a paused team stays paused across a
    /// daemon restart.
    #[serde(default)]
    pub paused: bool,
    /// Unix seconds. Zero on the wire from a client; the daemon stamps both.
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub updated: i64,
}

impl Team {
    /// A seat by name, which is how wakes and sends address one.
    pub fn seat(&self, name: &str) -> Option<&Seat> {
        self.seats.iter().find(|s| s.name == name)
    }

    /// The mode a seat is started in: its own, else the team's.
    pub fn permission_for(&self, seat: &Seat) -> PermissionMode {
        seat.permission.unwrap_or(self.permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seat_without_an_override_takes_the_teams_mode() {
        let mut team = Team {
            id: "t".into(),
            name: "port".into(),
            repo: "/r".into(),
            guidance: String::new(),
            permission: PermissionMode::AcceptEdits,
            policy: Policy::default(),
            seats: vec![Seat::new("builder", SessionSource::Codex)],
            paused: false,
            created: 0,
            updated: 0,
        };
        assert_eq!(team.permission_for(&team.seats[0]), PermissionMode::AcceptEdits);
        team.seats[0].permission = Some(PermissionMode::Plan);
        assert_eq!(team.permission_for(&team.seats[0]), PermissionMode::Plan);
    }

    /// The defaults the owner set are the defaults a definition gets when the
    /// field is absent — a client built before the field existed, or a hand
    /// edit of the JSON, must land on three and sixty rather than zero.
    #[test]
    fn an_absent_policy_field_is_the_owners_default() {
        let p: Policy = serde_json::from_str("{}").unwrap();
        assert_eq!(p.hop_budget, 3);
        assert_eq!(p.hourly_cap, 60);
        assert_eq!(p.handover, Handover::AfterRead);
        assert!(!p.readiness_press);
        let w: Watchdog = serde_json::from_str(r#"{"silent_minutes":15,"text":"status?"}"#).unwrap();
        assert!(w.while_busy);
        assert!(w.seat.is_none());
    }

    /// The wire spelling is snake case throughout, and a seat's `None`
    /// permission is absent rather than `null`, so a definition reads as what
    /// it is in the mirror and in a hand-written file.
    #[test]
    fn the_wire_spelling_is_snake_case_and_omits_the_inherited_mode() {
        let seat = Seat::new("reviewer", SessionSource::ClaudeCode);
        let json = serde_json::to_string(&seat).unwrap();
        assert!(json.contains(r#""cli":"claude_code""#), "{json}");
        assert!(json.contains(r#""works_in":"worktree""#), "{json}");
        assert!(!json.contains("permission"), "{json}");
        assert_eq!(
            serde_json::to_string(&PermissionMode::BypassPermissions).unwrap(),
            r#""bypass_permissions""#
        );
        let back: Seat = serde_json::from_str(&json).unwrap();
        assert_eq!(back, seat);
    }
}
