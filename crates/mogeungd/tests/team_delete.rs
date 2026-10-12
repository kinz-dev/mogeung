//! A seat's liveness on the wire, and deleting a team whole. `R-J134`,
//! feature 0051.
//!
//! Pinned: a launch record's `alive` is whether the seat's own tmux session
//! stands — recomputed as the daemon starts, read off the pane listing every
//! scan, and broadcast as `Seats` when it flips; a team cannot be deleted
//! while a seat of it runs, and the refusal names the seat; a delete forgets
//! the exited seats' records, removes the files made for its seats and the
//! mirror, and leaves a worktree standing.
//!
//! The "seats" are `sleep` in throwaway tmux sessions, as `team_tmux.rs`
//! does, so no agent is launched; the tests that need one skip, not fail,
//! without tmux. The last needs none.

use mogeung_core::session::SessionSource;
use mogeung_core::team::{Seat, SeatLaunch, SeatSignals, SignalKind, Team};
use mogeung_core::wire::ServerMsg;
use mogeungd::state::{session_in_panes, AppState, Pane};
use mogeungd::store::Store;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

fn tmux(args: &[&str]) -> Option<String> {
    let out = Command::new("tmux").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn have_tmux() -> bool {
    tmux(&["-V"]).is_some()
}

fn team() -> Team {
    Team {
        id: "t1".into(),
        name: "port".into(),
        repo: "/r".into(),
        guidance: String::new(),
        permission: Default::default(),
        policy: Default::default(),
        seats: vec![Seat::new("planner", SessionSource::ClaudeCode), Seat::new("builder", SessionSource::ClaudeCode)],
        paused: false,
        created: 1,
        updated: 1,
    }
}

fn launch(home: &Path, seat: &str, session: &str, dir: &Path, worktree: bool) -> SeatLaunch {
    SeatLaunch {
        team_id: "t1".into(),
        team: "port".into(),
        seat: seat.into(),
        cli: SessionSource::ClaudeCode,
        session_id: None,
        pane_id: "%0".into(),
        tmux_target: format!("{session}:0.0"),
        tmux_session: session.into(),
        team_session: String::new(),
        dir: dir.to_string_lossy().into_owned(),
        worktree,
        files_dir: home.join("seats").join("t1").join(seat).to_string_lossy().into_owned(),
        started: 0,
        // What the record says on disk is not believed: the daemon asks tmux.
        alive: false,
        token: String::new(),
        signals: SeatSignals { kind: SignalKind::Plain, ..Default::default() },
    }
}

/// A fresh home for one test, removed when it is dropped.
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let dir = std::env::temp_dir().join(format!("mogeung-delete-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Home(dir)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A daemon over `home` that has the team saved and mirrored, the launches
/// on record, and each seat's generated files where a boot leaves them.
fn daemon(home: &Path, launches: &[SeatLaunch]) -> Arc<AppState> {
    let store = Store::open(&home.join("t.db")).unwrap();
    for l in launches {
        store.save_launch(l).unwrap();
    }
    let state = AppState::with_home(store, home.join("claude")).unwrap();
    let _ = state.teams_dir.set(home.join("teams"));
    let _ = state.seats_dir.set(home.join("seats"));
    state.save_team(team()).unwrap();
    for l in launches {
        std::fs::create_dir_all(&l.files_dir).unwrap();
        std::fs::write(Path::new(&l.files_dir).join("hook.sh"), "#!/bin/sh\n").unwrap();
    }
    state
}

async fn seats(state: &AppState) -> Vec<SeatLaunch> {
    match state.seats_msg().await {
        ServerMsg::Seats { launches } => launches,
        other => panic!("not a Seats message: {other:?}"),
    }
}

fn mirror_files(home: &Path) -> usize {
    std::fs::read_dir(home.join("teams")).map(|d| d.count()).unwrap_or(0)
}

#[test]
fn a_session_is_in_the_listing_by_its_exact_name() {
    let pane = |target: &str| Pane { id: "%1".into(), pid: 1, target: target.into() };
    let panes = vec![pane("geung-port-planner:0.0"), pane("geung-port:1.0")];
    assert!(session_in_panes(&panes, "geung-port-planner"));
    assert!(session_in_panes(&panes, "geung-port"), "a session whose name is a prefix of another still matches on its own");
    assert!(!session_in_panes(&panes, "geung-po"), "a prefix without the colon is not a session");
    assert!(!session_in_panes(&panes, "geung-port-builder"));
    assert!(!session_in_panes(&panes, ""), "a record without a session name is not running");
}

/// Needs tmux. A re-adopted record is alive while its session stands, the
/// scan sees it fall and says so on the wire, a delete is refused while it
/// stands — naming the seat — and goes through once it is gone.
#[tokio::test]
async fn alive_follows_the_session_and_delete_waits_for_it() {
    if !have_tmux() {
        eprintln!("skipped: no tmux here");
        return;
    }
    let home = Home::new("alive");
    let name = format!("mogeung-del-alive-{}", std::process::id());
    let _ = tmux(&["kill-session", "-t", &format!("={name}")]);
    assert!(tmux(&["new-session", "-d", "-s", &name, "sleep 60"]).is_some(), "a throwaway seat session");
    let state = daemon(&home.0, &[launch(&home.0, "planner", &name, &home.0, false)]);

    // Re-adopted alive: the record on disk said no, tmux says yes.
    let on_wire = seats(&state).await;
    assert_eq!(on_wire.len(), 1);
    assert!(on_wire[0].alive, "a record whose session stands is alive as the daemon starts");

    let refused = state.delete_team("t1").await.expect_err("a running seat refuses the delete");
    assert!(refused.to_string().contains("planner"), "{refused}");
    assert_eq!(state.launches.read().await.len(), 1, "nothing was forgotten");
    assert_eq!(mirror_files(&home.0), 1, "the mirror is untouched");

    // The seat is closed by hand. The next scan reads it off the listing
    // and broadcasts the flip; the delete then goes through.
    let mut rx = state.tx.subscribe();
    assert!(tmux(&["kill-session", "-t", &format!("={name}")]).is_some());
    state.scan().await;
    assert!(!seats(&state).await[0].alive, "the record follows its session down");
    let mut heard = false;
    while let Ok(m) = rx.try_recv() {
        if let ServerMsg::Seats { launches } = m {
            heard |= launches.iter().any(|l| l.seat == "planner" && !l.alive);
        }
    }
    assert!(heard, "the flip was broadcast as a Seats message");

    let teams = state.delete_team("t1").await.expect("nothing runs, so the team may go");
    assert!(teams.is_empty());
    assert!(state.launches.read().await.is_empty(), "the exited seat's record went with the team");
    assert!(!home.0.join("seats").join("t1").exists(), "the seat's files went too");
    assert_eq!(mirror_files(&home.0), 0, "and the mirror");
    assert!(Store::open(&home.0.join("t.db")).unwrap().load_launches().unwrap().is_empty(), "the row is gone from the store");
}

/// No tmux needed: records of seats whose sessions do not exist are exited,
/// and a delete forgets them, takes the seat files, and leaves a worktree.
#[tokio::test]
async fn a_delete_forgets_exited_seats_and_leaves_the_worktree() {
    let home = Home::new("exited");
    let wt = home.0.join("worktree");
    std::fs::create_dir_all(&wt).unwrap();
    std::fs::write(wt.join("WORK.md"), "uncommitted\n").unwrap();
    let pid = std::process::id();
    let state = daemon(
        &home.0,
        &[
            launch(&home.0, "planner", &format!("mogeung-del-none-{pid}-a"), &home.0, false),
            launch(&home.0, "builder", &format!("mogeung-del-none-{pid}-b"), &wt, true),
        ],
    );
    assert!(seats(&state).await.iter().all(|l| !l.alive), "no such sessions, so nothing is alive");
    assert!(home.0.join("seats").join("t1").join("builder").join("hook.sh").is_file());

    let teams = state.delete_team("t1").await.expect("exited seats do not refuse a delete");
    assert!(teams.is_empty());
    assert!(state.launches.read().await.is_empty());
    assert!(!home.0.join("seats").join("t1").exists(), "the files made for the seats are gone");
    assert_eq!(mirror_files(&home.0), 0);
    assert!(wt.join("WORK.md").is_file(), "the worktree stays — it may hold work");
}
