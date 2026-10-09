//! What crossing a machine means for every message. `R-I18`, `R-I21`–`R-I24`,
//! [feature 0047](../../../docs/features/0047-daemon-federation/spec.md).
//!
//! A daemon that **follows** another mirrors its sessions and forwards the
//! commands a window sends about them. Everything that decides *how* lives
//! here, pure, so the daemon that forwards and the tests that pin it read the
//! same rules:
//!
//! - **The id namespace.** A mirrored session's id is `@<machine_id>:<id>` —
//!   `pane:%3` exists on every machine, so the machine has to be in the name.
//!   Run ids are a per-daemon counter and are namespaced the same way.
//! - **[`hop`]** — what a client command may do when its session is on
//!   another machine.
//! - **[`relay_class`]** — which of a followed daemon's messages a relay may
//!   hand to the client that asked.
//! - **[`outbound`] / [`inbound`]** — the id rewriting either side of the hop.
//!
//! Both classifications are `match`es with **no wildcard arm**: a message
//! type added next month does not compile until somebody decides what
//! crossing a machine means for it — `sweep`'s `HANDLED`/`KNOWN_IGNORED`
//! discipline, applied to the wire.

use crate::wire::{ClientMsg, ServerMsg};
use serde_json::Value;

/// The federation protocol this build speaks. A followed daemon that does not
/// say at least this is refused with the version it needs (`R-I24`).
pub const PROTOCOL: u32 = 1;

/// The id a mirrored session is known by here.
pub fn mirrored_id(machine: &str, id: &str) -> String {
    format!("@{machine}:{id}")
}

/// The machine and the original id, for an id that is mirrored.
pub fn split_mirrored(id: &str) -> Option<(&str, &str)> {
    let rest = id.strip_prefix('@')?;
    let (machine, orig) = rest.split_once(':')?;
    if machine.is_empty() || orig.is_empty() {
        return None;
    }
    Some((machine, orig))
}

/// Is this id one this daemon mirrors rather than one it owns? Nothing local
/// starts with `@` — Claude Code's ids are UUIDs, Codex's and Qwen's are
/// their own, provisional panes are `pane:%N` — so the prefix is the whole
/// test, which is what lets a socket filter a broadcast without a lookup.
pub fn is_mirrored(id: &str) -> bool {
    split_mirrored(id).is_some()
}

/// What a client command may do when its session lives on another machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hop {
    /// About this daemon or this machine — answered here, never forwarded.
    Here,
    /// Reads a remote session. Forwarded.
    Read,
    /// Records a judgement the remote daemon keeps — a hunk read, a snooze.
    /// Forwarded: it changes nothing on that machine but its own bookkeeping.
    Judge,
    /// Changes the remote machine — its repository, its processes, its
    /// agents, its records. Forwarded only when the follow allows writes
    /// (`R-I23`), and the remote daemon's own gates still decide there.
    Write,
    /// Acts on the remote machine's **desktop** — focus a terminal, open a
    /// folder or an IDE. Refused across the hop whatever the consent: there
    /// is nobody at that screen to see it.
    Desktop,
}

/// Every command, classified. No wildcard: see the module comment.
pub fn hop(cmd: &ClientMsg) -> Hop {
    use ClientMsg::*;
    match cmd {
        Subscribe | Rescan | FetchHealth | LaunchTerminal { .. } | FetchReviewDebt { .. } => Hop::Here,
        SetHunkReviewed { .. } | ReviewAll { .. } | Snooze { .. } => Hop::Judge,
        RefreshChange { .. } | FetchEvents { .. } | FetchBlastRadius { .. } => Hop::Read,
        ForgetSession { .. } => Hop::Write,
        FocusTerminal { .. } | OpenFolder { .. } | ProbeIntellij { .. } | OpenInIntellij { .. } => Hop::Desktop,
        FetchWorkspace { .. } | ListDir { .. } | FetchFile { .. } | ListTree { .. } | SearchContent { .. } => Hop::Read,
        AddWorkspaceDir { .. } | RemoveWorkspaceDir { .. } => Hop::Write,
        GitLog { .. } | GitShow { .. } | GitStatus { .. } => Hop::Read,
        GitStage { .. }
        | GitUnstage { .. }
        | GitDiscard { .. }
        | GitCommit { .. }
        | GitBranchCreate { .. }
        | GitSwitch { .. }
        | GitStashPush { .. }
        | GitStashPop { .. }
        | GitStashDrop { .. }
        | GitFetch { .. }
        | GitResolve { .. } => Hop::Write,
        NoteList | NoteSave { .. } | NoteDelete { .. } | TaskList | TaskSet { .. } => Hop::Here,
        // A team is that daemon's own record, like a note (`R-P3`); a seat
        // runs on that machine and nowhere else, so its definition is edited
        // there. Teams across machines are `R-P13`'s deferral.
        TeamList | TeamSave { .. } | TeamDelete { .. } => Hop::Here,
        ScratchList
        | ScratchCreate { .. }
        | ScratchRead { .. }
        | ScratchWrite { .. }
        | ScratchRename { .. }
        | ScratchDelete { .. }
        | ScratchDuplicate { .. }
        | ScratchMkdir { .. }
        | ScratchRmdir { .. } => Hop::Here,
        // The chat is this machine's model; an *ask about* a remote session
        // is refused in words by the daemon rather than forwarded.
        ModelChat { .. } | ModelStop { .. } | ChatList {} | ChatLoad { .. } | ChatDelete { .. } => Hop::Here,
        // The remote daemon's model, reading the remote daemon's diff — and
        // the remote daemon's clause 4 deciding, since a relay is not local.
        ReadingGuide { .. } => Hop::Read,
        SendToSession { .. } => Hop::Write,
        SemanticSearch { .. } | ClusterFailures { .. } | BuildSemanticIndex {} | ConfigGet {} | ConfigSave { .. } => {
            Hop::Here
        }
        GitDiffFile { .. }
        | GitBlame { .. }
        | GitRefs { .. }
        | GitStashes { .. }
        | GitStashShow { .. }
        | GitSubmodules { .. }
        | GitDiffRange { .. }
        | GitCompare { .. }
        | GitReflog { .. }
        | GitWorktrees { .. }
        | GitConflictFile { .. }
        | GitFileAtRev { .. } => Hop::Read,
        FetchUsage => Hop::Here,
        FetchRunConfigs { .. } | FetchRunOutput { .. } => Hop::Read,
        // A run executes on that machine; stopping one changes it too; an
        // environment value is a secret that would cross the hop.
        RunStart { .. } | RunStop { .. } | RevealRunEnv { .. } | RunSignal { .. } => Hop::Write,
        SetSignalCommand { .. } | FetchSignal { .. } => Hop::Here,
        InsightSearch { .. } | FetchDigest { .. } | FetchRecurring | FetchPromptLibrary | FetchAnalytics => Hop::Here,
        FetchSubagents { .. } | FetchDecisions { .. } => Hop::Read,
        FetchFileSessions { .. } | FetchDocScan { .. } | FetchKit | FetchKitDoc { .. } => Hop::Here,
        Follow { .. } | Unfollow { .. } | SetPeerWrites { .. } | FetchPeers => Hop::Here,
    }
}

/// What a relay does with a message a followed daemon sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relay {
    /// The board — sessions and the queue. The follow's own watch socket
    /// carries these; a relay drops them so nothing arrives twice.
    Board,
    /// An answer, or a session's live data. Handed to the client, ids
    /// rewritten into this daemon's namespace.
    Pass,
    /// About that daemon itself — its health, notes, models, settings, its
    /// whole run list. Dropped: a client would read it as describing the
    /// daemon it is connected to.
    Theirs,
}

/// Every message, classified. No wildcard: see the module comment.
pub fn relay_class(msg: &ServerMsg) -> Relay {
    use ServerMsg::*;
    match msg {
        Snapshot { .. } | SessionUpdated { .. } | SessionRemoved { .. } | Queue { .. } => Relay::Board,
        Events { .. } | ChangeUpdated { .. } | ChangeSummary { .. } => Relay::Pass,
        Health { .. } | ReviewDebt { .. } => Relay::Theirs,
        BlastRadius { .. }
        | DirListing { .. }
        | FileContent { .. }
        | Workspace { .. }
        | TreeListing { .. }
        | ContentMatches { .. }
        | GitCommits { .. }
        | GitCommitDiff { .. }
        | GitLocalChanges { .. }
        | GitFileDiff { .. }
        | GitAnnotation { .. }
        | GitRefsInfo { .. } => Relay::Pass,
        // About that machine's desktop; never asked across a hop.
        IntellijProbe { .. } => Relay::Theirs,
        GitFetched { .. } => Relay::Pass,
        Notes { .. } | Tasks { .. } | Teams { .. } | Scratches { .. } | ScratchContent { .. } | ScratchSaved { .. } => Relay::Theirs,
        ModelChunk { .. } | ModelReply { .. } => Relay::Theirs,
        ReadingGuideReady { .. } | SentToSession { .. } => Relay::Pass,
        SemanticResults { .. } | FailureClusters { .. } | ChatHistory { .. } | ChatConversation { .. } | Config { .. } => {
            Relay::Theirs
        }
        GitStashList { .. }
        | GitStashDiff { .. }
        | GitSubmoduleList { .. }
        | GitRangeDiff { .. }
        | GitFileAtRevContent { .. }
        | GitReflogList { .. }
        | GitWorktreeList { .. }
        | GitConflictStages { .. } => Relay::Pass,
        UsageStats { .. }
        | SignalStatus { .. }
        | InsightSearchResults { .. }
        | DayDigestReport { .. }
        | RecurringFailures { .. }
        | PromptLibrary { .. }
        | AnalyticsReport { .. } => Relay::Theirs,
        SubagentTreeReport { .. } | DecisionReport { .. } => Relay::Pass,
        FileSessions { .. } | DocReport { .. } | Kit { .. } | KitDoc { .. } => Relay::Theirs,
        // An error on a relay is the answer to the command it carried: the
        // daemon sends errors to the asker only (`R-J59`).
        Error { .. } => Relay::Pass,
        RunConfigs { .. } => Relay::Pass,
        // The whole run list replaces the client's — theirs, not an answer.
        Runs { .. } => Relay::Theirs,
        RunStarted { .. } | RunOutput { .. } | RunEnded { .. } | RunOutputHistory { .. } | RunEnvValue { .. } => {
            Relay::Pass
        }
        Peers { .. } => Relay::Theirs,
    }
}

/// The session or run a command is about, if any, straight from its JSON —
/// every such command carries it as a top-level `session_id` or `run_id`.
pub fn target(cmd: &ClientMsg) -> Option<String> {
    let v = serde_json::to_value(cmd).ok()?;
    let obj = v.as_object()?;
    obj.get("session_id")
        .or_else(|| obj.get("run_id"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
}

/// A command about a mirrored session or run, rewritten for the daemon that
/// owns it: the machine it goes to, and the JSON to send. `None` when it is
/// not about anything mirrored.
pub fn outbound(cmd: &ClientMsg) -> Option<(String, Value)> {
    let mut v = serde_json::to_value(cmd).ok()?;
    let obj = v.as_object_mut()?;
    for key in ["session_id", "run_id"] {
        let Some(id) = obj.get(key).and_then(|x| x.as_str()) else { continue };
        let Some((machine, orig)) = split_mirrored(id) else { continue };
        let (machine, orig) = (machine.to_string(), orig.to_string());
        obj.insert(key.to_string(), Value::String(orig));
        return Some((machine, v));
    }
    None
}

/// A message from a followed daemon, rewritten into this daemon's namespace:
/// every session id and run id, wherever it sits, gains the machine.
pub fn inbound(mut msg: Value, machine: &str) -> Value {
    namespace(&mut msg, machine, None);
    msg
}

fn namespace(v: &mut Value, machine: &str, parent: Option<&str>) {
    match v {
        Value::Object(map) => {
            let in_run = matches!(parent, Some("run") | Some("runs"));
            for (k, child) in map.iter_mut() {
                let is_id = k == "session_id" || k == "run_id" || (in_run && k == "id");
                if is_id {
                    if let Value::String(s) = child {
                        if !is_mirrored(s) {
                            *s = mirrored_id(machine, s);
                        }
                        continue;
                    }
                }
                namespace(child, machine, Some(k.as_str()));
            }
        }
        Value::Array(items) => {
            for item in items {
                namespace(item, machine, parent);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{ClientMsg, ServerMsg};

    #[test]
    fn a_mirrored_id_round_trips_and_nothing_local_looks_like_one() {
        let id = mirrored_id("de5059de", "pane:%3");
        assert_eq!(id, "@de5059de:pane:%3");
        assert_eq!(split_mirrored(&id), Some(("de5059de", "pane:%3")));
        assert!(is_mirrored(&id));
        for local in ["8b7c2f7e-0a1b-4c3d-9e8f-001122334455", "pane:%3", "rollout-2026", ""] {
            assert!(!is_mirrored(local), "{local}");
        }
        assert_eq!(split_mirrored("@:x"), None);
        assert_eq!(split_mirrored("@m:"), None);
    }

    #[test]
    fn a_command_about_a_mirrored_session_goes_to_its_machine_with_its_own_id() {
        let cmd = ClientMsg::FetchFile { session_id: mirrored_id("mac", "abc"), path: "src/a.rs".into() };
        let (machine, v) = outbound(&cmd).unwrap();
        assert_eq!(machine, "mac");
        assert_eq!(v["session_id"], "abc");
        assert_eq!(v["path"], "src/a.rs");
        assert_eq!(v["cmd"], "fetch_file");
        // A run id is routed the same way.
        let (machine, v) = outbound(&ClientMsg::RunStop { run_id: mirrored_id("mac", "run-4") }).unwrap();
        assert_eq!((machine.as_str(), v["run_id"].as_str()), ("mac", Some("run-4")));
        // And a local one is not routed at all.
        assert!(outbound(&ClientMsg::FetchFile { session_id: "abc".into(), path: "x".into() }).is_none());
        assert!(outbound(&ClientMsg::Rescan).is_none());
    }

    #[test]
    fn every_id_in_an_answer_gains_the_machine_wherever_it_sits() {
        let msg = serde_json::json!({
            "ev": "events",
            "events": [{ "session_id": "abc", "seq": 1 }, { "session_id": "def", "seq": 2 }],
        });
        let out = inbound(msg, "mac");
        assert_eq!(out["events"][0]["session_id"], "@mac:abc");
        assert_eq!(out["events"][1]["session_id"], "@mac:def");

        let run = serde_json::json!({
            "ev": "run_started",
            "run": { "id": "run-4", "config_id": "detected:x", "session_id": "abc", "name": "t" },
        });
        let out = inbound(run, "mac");
        assert_eq!(out["run"]["id"], "@mac:run-4");
        assert_eq!(out["run"]["session_id"], "@mac:abc");
        assert_eq!(out["run"]["config_id"], "detected:x", "a configuration id is not a session id");

        let line = inbound(serde_json::json!({ "ev": "run_output", "line": { "run_id": "run-4", "seq": 1 } }), "mac");
        assert_eq!(line["line"]["run_id"], "@mac:run-4");

        // A run with no session keeps its null.
        let none = inbound(serde_json::json!({ "run": { "id": "run-1", "session_id": null } }), "mac");
        assert!(none["run"]["session_id"].is_null());
        // An id that already carries a machine is left alone.
        let twice = inbound(serde_json::json!({ "session_id": "@other:abc" }), "mac");
        assert_eq!(twice["session_id"], "@other:abc");
    }

    /// The decisions that carry the security argument, pinned by name.
    #[test]
    fn writes_need_consent_and_the_desktop_never_crosses() {
        let s = || mirrored_id("mac", "abc");
        assert_eq!(hop(&ClientMsg::GitCommit { session_id: s(), repo: None, message: "m".into(), amend: false, session_trailer: false }), Hop::Write);
        assert_eq!(hop(&ClientMsg::RunStart { session_id: s(), config_id: "c".into() }), Hop::Write);
        assert_eq!(hop(&ClientMsg::SendToSession { session_id: s(), text: "t".into() }), Hop::Write);
        assert_eq!(hop(&ClientMsg::RevealRunEnv { session_id: s(), config_id: "c".into(), key: "K".into() }), Hop::Write);
        assert_eq!(hop(&ClientMsg::ForgetSession { session_id: s() }), Hop::Write);
        assert_eq!(hop(&ClientMsg::FocusTerminal { session_id: s() }), Hop::Desktop);
        assert_eq!(hop(&ClientMsg::OpenInIntellij { session_id: s() }), Hop::Desktop);
        assert_eq!(hop(&ClientMsg::FetchFile { session_id: s(), path: "p".into() }), Hop::Read);
        assert_eq!(hop(&ClientMsg::GitLog { session_id: s(), repo: None, skip: 0, limit: 10, rev: None, grep: None, author: None, path: None, pickaxe: None, all: false, since: None, until: None }), Hop::Read);
        assert_eq!(hop(&ClientMsg::SetHunkReviewed { session_id: s(), anchor: "a".into(), reviewed: true }), Hop::Judge);
        assert_eq!(hop(&ClientMsg::ModelChat { id: "1".into(), messages: vec![], conversation: None, about: None }), Hop::Here);
    }

    #[test]
    fn a_relay_passes_answers_and_drops_the_board_and_what_is_theirs() {
        assert_eq!(relay_class(&ServerMsg::Queue { queue: vec![] }), Relay::Board);
        assert_eq!(relay_class(&ServerMsg::SessionRemoved { session_id: "a".into() }), Relay::Board);
        assert_eq!(relay_class(&ServerMsg::Error { message: "e".into() }), Relay::Pass);
        assert_eq!(relay_class(&ServerMsg::Runs { runs: vec![] }), Relay::Theirs, "their run list would replace ours");
        assert_eq!(relay_class(&ServerMsg::Peers { peers: vec![] }), Relay::Theirs);
        assert_eq!(relay_class(&ServerMsg::Notes { notes: vec![] }), Relay::Theirs);
    }

    #[test]
    fn the_target_is_read_from_the_command_itself() {
        assert_eq!(target(&ClientMsg::FetchEvents { session_id: "x".into(), since: 0 }).as_deref(), Some("x"));
        assert_eq!(target(&ClientMsg::FetchRunOutput { run_id: "r".into() }).as_deref(), Some("r"));
        assert_eq!(target(&ClientMsg::Rescan), None);
    }
}
