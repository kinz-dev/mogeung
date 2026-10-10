//! The policy engine: every automatic send a team makes, judged, counted and
//! logged. `R-P6`, feature 0051.
//!
//! [ADR-0003](../../../docs/decisions/0003-observe-do-not-spawn.md)'s
//! 2026-10-09 amendment admitted one path with its eyes open — a path from an
//! agent's output to an agent's input, for a seat and no other session — and
//! put fence 4 across it: *every automatic send is visible, budgeted and
//! stoppable.* This module is fence 4, as data and arithmetic.
//!
//! # What it is, and is not
//!
//! A [`Ledger`] per team and a judge over it. [`Ledger::judge`] answers
//! *deliver* or *hold* for a send of a kind from three facts and nothing
//! else: the team's pause switch, how many hops the chain has travelled since
//! a human last sent, and how many automatic sends landed in the last hour.
//! The counters are **read off the log** — a chain is the run of hop
//! deliveries since the last entry a human caused, an hour is the automatic
//! deliveries newer than sixty minutes — so there is no second state to keep
//! right, and a restarted daemon counts what it counted before.
//!
//! It never sends. `state.rs` owns the one delivery function, which takes a
//! launch record and no other kind of address; this module composes the
//! entry and the verdict, and `send.rs`'s three tmux invocations stay the
//! only code that reaches an agent's input.
//!
//! # The pause is the last thing to fail
//!
//! The judge is handed the definition, and when the caller cannot produce
//! one — a team whose row would not read — it holds, with that as the
//! reason. A daemon that cannot reach its own policy behaves as if paused,
//! which spec 0051 named as a risk and this module takes as a rule.
//!
//! # What counts as a hop
//!
//! [`SendKind::is_hop`] says: a seat's own message, and a wake — one seat's
//! event typed into another, which is the shape a loop with nobody watching
//! takes. A watchdog's nudge is caused by silence, not by another agent, so
//! it counts against the hour and not the chain. A human's send, or a human
//! releasing a held one, ends the chain: the next hop is the first again.

use mogeung_core::session::{HeldSend, LiveStatus};
use mogeung_core::team::{SeatEvent, SendEntry, SendKind, SendStatus, Team, TeamSends, Watchdog};
use std::collections::{HashMap, HashSet};

/// How many entries a team's log keeps, in memory and in the store. Held
/// entries are kept whatever their age: a held send is a question for you.
pub const LOG_KEEP: usize = 200;

/// The hour the cap is counted over, in seconds.
pub const HOUR: i64 = 3600;

/// A seat's message is typed into a terminal; a book is not a message.
pub const MAX_TEXT: usize = 32 * 1024;

/// What the judge said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Type it. `hop` is its place in the chain, 0 when it is not a hop.
    Deliver { hop: u32 },
    /// Keep it for a human, with this reason.
    Hold(String),
}

/// One team's log and the little memory the watchdogs and the exit wake need.
#[derive(Debug, Default)]
pub struct Ledger {
    /// Oldest first. The source of every counter.
    pub log: Vec<SendEntry>,
    /// When each watchdog last nudged each seat: `(seat, index)` → at.
    nudged: HashMap<(String, usize), i64>,
    /// Seats whose exit has already fired a wake, until the team is booted
    /// again. An exit is one event, not one per scan.
    exited: HashSet<String>,
    /// The liveness last seen per seat, so a plain seat's turn ending — the
    /// registry going busy to idle — can be read as its `Stop`.
    last_status: HashMap<String, Option<LiveStatus>>,
}

impl Ledger {
    pub fn from_log(log: Vec<SendEntry>) -> Self {
        let mut l = Ledger { log, ..Default::default() };
        // A watchdog's memory is seeded from the log so a restart does not
        // nudge the same silent seat again at once.
        for e in &l.log {
            if e.kind == SendKind::Watchdog && e.status == SendStatus::Delivered {
                let idx = e.note.as_deref().and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
                l.nudged.insert((e.to.clone(), idx), e.at);
            }
        }
        l
    }

    /// Hops in the chain so far: deliveries that counted as hops since the
    /// last thing a human did — a send, or a release.
    pub fn chain(&self) -> u32 {
        let mut n = 0;
        for e in self.log.iter().rev() {
            match e.status {
                SendStatus::Released => break,
                SendStatus::Delivered if e.kind == SendKind::Human => break,
                SendStatus::Delivered if e.kind.is_hop() => n += 1,
                _ => {}
            }
        }
        n
    }

    /// Automatic deliveries in the last hour.
    pub fn this_hour(&self, now: i64) -> u32 {
        self.log
            .iter()
            .filter(|e| e.status == SendStatus::Delivered && e.kind.is_automatic() && e.at > now - HOUR)
            .count() as u32
    }

    pub fn held(&self) -> impl Iterator<Item = &SendEntry> {
        self.log.iter().filter(|e| e.status == SendStatus::Held)
    }

    /// Deliver or hold, from the definition and the log. `None` for the team
    /// is the daemon unable to read its own policy, and that holds.
    pub fn judge(&self, team: Option<&Team>, kind: SendKind, now: i64) -> Verdict {
        let Some(team) = team else {
            return Verdict::Hold(
                "the team's definition could not be read, so the team is treated as paused".to_string(),
            );
        };
        if kind == SendKind::Human {
            return Verdict::Deliver { hop: 0 };
        }
        if team.paused {
            return Verdict::Hold("the team is paused".to_string());
        }
        let hop = if kind.is_hop() {
            let chain = self.chain();
            let budget = team.policy.hop_budget;
            if budget == 0 {
                return Verdict::Hold("the hop budget is zero — every message goes through you".to_string());
            }
            if chain >= budget {
                return Verdict::Hold(format!(
                    "the hop budget of {budget} is spent — {chain} agent-to-agent deliver{} since you last sent; \
                     your send, or a release, starts the count again",
                    if chain == 1 { "y" } else { "ies" }
                ));
            }
            chain + 1
        } else {
            0
        };
        let hour = self.this_hour(now);
        let cap = team.policy.hourly_cap;
        if hour >= cap {
            return Verdict::Hold(format!(
                "the cap of {cap} automatic send{} an hour is reached",
                if cap == 1 { "" } else { "s" }
            ));
        }
        Verdict::Deliver { hop }
    }

    /// Append, and keep the log to [`LOG_KEEP`] plus whatever is held.
    pub fn record(&mut self, e: SendEntry) {
        self.log.push(e);
        let unheld = self.log.iter().filter(|e| e.status != SendStatus::Held).count();
        let mut excess = unheld.saturating_sub(LOG_KEEP);
        if excess > 0 {
            self.log.retain(|e| {
                if excess > 0 && e.status != SendStatus::Held {
                    excess -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }

    pub fn entry_mut(&mut self, id: &str) -> Option<&mut SendEntry> {
        self.log.iter_mut().find(|e| e.id == id)
    }

    /// What is held for one seat: the count, the oldest's reason, and since
    /// when. The row's detail and clock.
    pub fn held_for(&self, seat: &str) -> Option<HeldSend> {
        let mut held = self.held().filter(|e| e.to == seat);
        let first = held.next()?;
        let count = 1 + held.count() as u32;
        Some(HeldSend {
            count,
            why: first.note.clone().unwrap_or_else(|| "held by the team's policy".into()),
            since: chrono::DateTime::from_timestamp(first.at, 0).unwrap_or_else(chrono::Utc::now),
        })
    }

    /// Is this watchdog due for this seat? Silent long enough, busy if it
    /// must be, and not nudged within the same silence.
    pub fn watchdog_due(&self, seat: &str, idx: usize, d: &Watchdog, silent_secs: i64, busy: Option<bool>, now: i64) -> bool {
        let need = d.silent_minutes as i64 * 60;
        if silent_secs < need {
            return false;
        }
        if d.while_busy && busy != Some(true) {
            return false;
        }
        match self.nudged.get(&(seat.to_string(), idx)) {
            Some(at) if now - at < need => false,
            _ => true,
        }
    }

    pub fn note_nudged(&mut self, seat: &str, idx: usize, now: i64) {
        self.nudged.insert((seat.to_string(), idx), now);
    }

    /// Feed a seat's liveness; answers `Stop` the tick it goes from busy to
    /// idle. For a plain seat, whose hooks say nothing, this is its turn
    /// ending; a hooked seat's `Stop` arrives through its hook and this is
    /// not asked.
    pub fn liveness(&mut self, seat: &str, status: Option<LiveStatus>) -> Option<SeatEvent> {
        let before = self.last_status.insert(seat.to_string(), status);
        match (before, status) {
            (Some(Some(LiveStatus::Busy)), Some(LiveStatus::Idle)) => Some(SeatEvent::Stop),
            _ => None,
        }
    }

    /// True the first time a seat's exit is reported, false after.
    pub fn exit_once(&mut self, seat: &str) -> bool {
        self.exited.insert(seat.to_string())
    }

    /// A seat was booted (again): its exit and its liveness start over.
    pub fn booted(&mut self, seat: &str) {
        self.exited.remove(seat);
        self.last_status.remove(seat);
        self.nudged.retain(|(s, _), _| s != seat);
    }
}

/// Every team's ledger.
#[derive(Debug, Default)]
pub struct Engine {
    teams: HashMap<String, Ledger>,
}

impl Engine {
    /// From the store's log, oldest first.
    pub fn from_log(entries: Vec<SendEntry>) -> Self {
        let mut by_team: HashMap<String, Vec<SendEntry>> = HashMap::new();
        for e in entries {
            by_team.entry(e.team_id.clone()).or_default().push(e);
        }
        Engine {
            teams: by_team.into_iter().map(|(k, v)| (k, Ledger::from_log(v))).collect(),
        }
    }

    pub fn team(&self, id: &str) -> Option<&Ledger> {
        self.teams.get(id)
    }

    pub fn team_mut(&mut self, id: &str) -> &mut Ledger {
        self.teams.entry(id.to_string()).or_default()
    }

    /// Team ids with a ledger, for the wire.
    pub fn team_ids(&self) -> impl Iterator<Item = &String> {
        self.teams.keys()
    }

    /// The counters and the log for one team, against its definition.
    pub fn summary(&self, team: &Team, now: i64) -> TeamSends {
        let empty = Ledger::default();
        let l = self.teams.get(&team.id).unwrap_or(&empty);
        TeamSends {
            team_id: team.id.clone(),
            team: team.name.clone(),
            chain: l.chain(),
            hop_budget: team.policy.hop_budget,
            this_hour: l.this_hour(now),
            hourly_cap: team.policy.hourly_cap,
            paused: team.paused,
            held: l.held().count() as u32,
            log: l.log.clone(),
        }
    }
}

/// A fresh entry, status to be decided by the judge.
pub fn entry(
    team_id: &str,
    kind: SendKind,
    from: Option<&str>,
    to: &str,
    text: &str,
    why: &str,
    now: i64,
) -> SendEntry {
    SendEntry {
        id: crate::notes::new_id(),
        team_id: team_id.to_string(),
        at: now,
        kind,
        from: from.map(str::to_string),
        to: to.to_string(),
        text: text.to_string(),
        why: why.to_string(),
        status: SendStatus::Refused,
        hop: 0,
        note: None,
    }
}

/// The status line's tail: the chain and the hour against their budgets,
/// and *paused* when it is. `hooks::status_text` shows it on the seat.
pub fn counters_text(sends: &TeamSends) -> String {
    let mut s = format!(
        "hops {}/{} · {}/{} this hour",
        sends.chain, sends.hop_budget, sends.this_hour, sends.hourly_cap
    );
    if sends.paused {
        s.push_str(" · paused");
    }
    if sends.held > 0 {
        s.push_str(&format!(" · {} held", sends.held));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::session::SessionSource;
    use mogeung_core::team::{Policy, Seat};

    fn team(hop_budget: u32, hourly_cap: u32) -> Team {
        Team {
            id: "t1".into(),
            name: "port".into(),
            repo: "/r".into(),
            guidance: String::new(),
            permission: Default::default(),
            policy: Policy { hop_budget, hourly_cap, ..Policy::default() },
            seats: vec![Seat::new("a", SessionSource::ClaudeCode), Seat::new("b", SessionSource::Codex)],
            paused: false,
            created: 0,
            updated: 0,
        }
    }

    fn delivered(l: &mut Ledger, kind: SendKind, at: i64) {
        let mut e = entry("t1", kind, Some("a"), "b", "hi", "test", at);
        e.status = SendStatus::Delivered;
        e.hop = if kind.is_hop() { l.chain() + 1 } else { 0 };
        l.record(e);
    }

    /// Fence 4's budget, in a table: three hops deliver, the fourth is held,
    /// a human's send starts the count again, and a release counts as one.
    #[test]
    fn the_fourth_hop_is_held_and_a_human_starts_the_count_again() {
        let t = team(3, 60);
        let mut l = Ledger::default();
        let now = 1_000_000;
        for expect in 1..=3 {
            assert_eq!(l.judge(Some(&t), SendKind::Agent, now), Verdict::Deliver { hop: expect });
            delivered(&mut l, SendKind::Agent, now);
        }
        assert_eq!(l.chain(), 3);
        let Verdict::Hold(why) = l.judge(Some(&t), SendKind::Agent, now) else {
            panic!("the fourth hop must be held")
        };
        assert!(why.contains("hop budget of 3 is spent"), "{why}");
        // A wake is a hop too.
        assert!(matches!(l.judge(Some(&t), SendKind::Wake, now), Verdict::Hold(_)));
        // A watchdog is not a hop, and still goes.
        assert_eq!(l.judge(Some(&t), SendKind::Watchdog, now), Verdict::Deliver { hop: 0 });

        delivered(&mut l, SendKind::Human, now);
        assert_eq!(l.chain(), 0, "your send starts the count again");
        assert_eq!(l.judge(Some(&t), SendKind::Agent, now), Verdict::Deliver { hop: 1 });

        // A release is a human's act with the same effect.
        delivered(&mut l, SendKind::Agent, now);
        delivered(&mut l, SendKind::Agent, now);
        assert_eq!(l.chain(), 2);
        let mut e = entry("t1", SendKind::Agent, Some("a"), "b", "x", "test", now);
        e.status = SendStatus::Released;
        l.record(e);
        assert_eq!(l.chain(), 0);
    }

    /// Zero is *every message through you*, a setting and not another mode.
    #[test]
    fn a_hop_budget_of_zero_holds_every_hop_and_lets_a_watchdog_through() {
        let t = team(0, 60);
        let l = Ledger::default();
        let Verdict::Hold(why) = l.judge(Some(&t), SendKind::Agent, 0) else { panic!() };
        assert!(why.contains("zero"), "{why}");
        assert_eq!(l.judge(Some(&t), SendKind::Watchdog, 0), Verdict::Deliver { hop: 0 });
    }

    /// Sixty an hour across the team, every automatic kind included, and the
    /// window slides: what landed over an hour ago no longer counts.
    #[test]
    fn the_hourly_cap_counts_every_automatic_kind_and_slides() {
        let t = team(100, 2);
        let mut l = Ledger::default();
        let now = 10_000;
        delivered(&mut l, SendKind::Watchdog, now - 100);
        delivered(&mut l, SendKind::Agent, now - 50);
        assert_eq!(l.this_hour(now), 2);
        let Verdict::Hold(why) = l.judge(Some(&t), SendKind::Agent, now) else { panic!("the third is held") };
        assert!(why.contains("cap of 2"), "{why}");
        // A human's send never counts.
        delivered(&mut l, SendKind::Human, now);
        assert_eq!(l.this_hour(now), 2);
        // An hour on, the first has aged out.
        assert_eq!(l.this_hour(now + HOUR - 60), 1);
        assert!(matches!(l.judge(Some(&t), SendKind::Agent, now + HOUR - 60), Verdict::Deliver { .. }));
    }

    /// The switch, and the switch's failure mode: a paused team holds
    /// everything automatic, and so does a team the daemon cannot read.
    #[test]
    fn paused_holds_everything_automatic_and_so_does_no_definition() {
        let mut t = team(3, 60);
        t.paused = true;
        let l = Ledger::default();
        for k in [SendKind::Agent, SendKind::Wake, SendKind::Watchdog] {
            let Verdict::Hold(why) = l.judge(Some(&t), k, 0) else { panic!("{k:?}") };
            assert_eq!(why, "the team is paused");
        }
        assert_eq!(l.judge(Some(&t), SendKind::Human, 0), Verdict::Deliver { hop: 0 }, "typing always works");
        let Verdict::Hold(why) = l.judge(None, SendKind::Agent, 0) else { panic!() };
        assert!(why.contains("treated as paused"), "{why}");
    }

    /// Held entries are a question, so they rank the seat and are never
    /// pruned; everything else is kept to the cap.
    #[test]
    fn held_entries_rank_their_seat_and_survive_the_cap() {
        let mut l = Ledger::default();
        let mut h = entry("t1", SendKind::Agent, Some("a"), "b", "please", "a said so", 5);
        h.status = SendStatus::Held;
        h.note = Some("the hop budget of 3 is spent".into());
        l.record(h);
        for i in 0..(LOG_KEEP + 10) as i64 {
            delivered(&mut l, SendKind::Watchdog, 10 + i);
        }
        assert_eq!(l.log.len(), LOG_KEEP + 1);
        assert_eq!(l.held().count(), 1);
        let held = l.held_for("b").expect("held for b");
        assert_eq!(held.count, 1);
        assert_eq!(held.why, "the hop budget of 3 is spent");
        assert!(l.held_for("a").is_none());
    }

    /// A watchdog fires once per silence, only when the seat is busy if it
    /// says so, and never before the minutes are up.
    #[test]
    fn a_watchdog_is_due_once_per_silence_and_only_when_busy_if_it_says_so() {
        let d = Watchdog { seat: None, silent_minutes: 10, while_busy: true, text: "status?".into() };
        let mut l = Ledger::default();
        let now = 100_000;
        assert!(!l.watchdog_due("a", 0, &d, 599, Some(true), now), "not yet");
        assert!(!l.watchdog_due("a", 0, &d, 600, Some(false), now), "idle at its prompt is waiting for a person");
        assert!(!l.watchdog_due("a", 0, &d, 600, None, now), "unknown is not busy");
        assert!(l.watchdog_due("a", 0, &d, 600, Some(true), now));
        l.note_nudged("a", 0, now);
        assert!(!l.watchdog_due("a", 0, &d, 900, Some(true), now + 300), "nudged within this silence");
        assert!(l.watchdog_due("a", 0, &d, 1300, Some(true), now + 700));
        let any = Watchdog { while_busy: false, ..d.clone() };
        assert!(l.watchdog_due("b", 1, &any, 600, None, now), "while_busy off: silence is enough");
    }

    /// A plain seat's turn ends when the registry goes busy to idle, and
    /// that transition is one `Stop`, not one per tick.
    #[test]
    fn a_busy_to_idle_transition_is_one_stop() {
        let mut l = Ledger::default();
        assert_eq!(l.liveness("a", Some(LiveStatus::Busy)), None);
        assert_eq!(l.liveness("a", Some(LiveStatus::Idle)), Some(SeatEvent::Stop));
        assert_eq!(l.liveness("a", Some(LiveStatus::Idle)), None);
        assert_eq!(l.liveness("a", None), None, "going dead is not a stop");
        assert!(l.exit_once("a"));
        assert!(!l.exit_once("a"));
        l.booted("a");
        assert!(l.exit_once("a"));
    }

    #[test]
    fn the_summary_is_the_definition_against_the_log() {
        let t = team(3, 60);
        let mut e = Engine::default();
        let l = e.team_mut("t1");
        delivered(l, SendKind::Agent, 100);
        let s = e.summary(&t, 200);
        assert_eq!((s.chain, s.hop_budget, s.this_hour, s.hourly_cap, s.held), (1, 3, 1, 60, 0));
        assert_eq!(counters_text(&s), "hops 1/3 · 1/60 this hour");
        let mut paused = t.clone();
        paused.paused = true;
        assert!(counters_text(&e.summary(&paused, 200)).ends_with("· paused"));
        // A restart reads the log back and counts the same.
        let again = Engine::from_log(e.summary(&t, 200).log);
        assert_eq!(again.summary(&t, 200).chain, 1);
    }
}
