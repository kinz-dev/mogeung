//! Winding a team down. `R-P15`, feature 0051, ADR-0003's amendment of
//! 2026-10-11.
//!
//! Pinned: the policy's wind-down message is typed once into every running
//! seat, logged as your act — not a hop, not counted against the hour — and
//! never held by the pause; a seat's note, posted with its own token, is
//! kept beside its files and finishes the seat; a seat's `Stop` finishes it
//! without a note, except the one `Stop` that ends the turn it was in when
//! asked; the deadline finishes the rest; the record says how each went and
//! stays until the team is booted again; `Stop now` cuts it short; a note
//! reaches the next handover as the seat's own, and a stale token is nobody.
//!
//! Costs nothing: no agent is launched. The "panes" are the ids on launch
//! records and the typing is the recording sender; `seat::stop` kills tmux
//! sessions that do not exist.

use mogeung_core::session::{LiveStatus, SessionSource};
use mogeung_core::team::{Policy, Seat, SeatEvent, SeatLaunch, SeatSignals, SendKind, SendStatus, SignalKind, Team, WindDownStatus};
use mogeungd::{api, state::AppState, store::Store};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PLANNER: &str = "planner-token-0123456789abcdef0123456789abcdef";
const BUILDER: &str = "builder-token-0123456789abcdef0123456789abcdef";
const SHARED: &str = "window-token";

type Typed = Arc<Mutex<Vec<(String, String)>>>;

struct Daemon {
    state: Arc<AppState>,
    port: u16,
    typed: Typed,
    dir: std::path::PathBuf,
}

fn launch(dir: &std::path::Path, seat: &str, pane: &str, token: &str) -> SeatLaunch {
    SeatLaunch {
        team_id: "t1".into(),
        team: "port".into(),
        seat: seat.into(),
        cli: SessionSource::ClaudeCode,
        session_id: None,
        pane_id: pane.into(),
        tmux_target: format!("geung-port-{seat}:0.0"),
        // No tmux session of this name exists, so `is_running` says no — the
        // tests below mark the seats running through their session rows and
        // let the daemon's own liveness answer where it matters.
        tmux_session: String::new(),
        team_session: String::new(),
        dir: "/w".into(),
        worktree: false,
        files_dir: dir.join("seats").join("t1").join(seat).to_string_lossy().into_owned(),
        started: chrono::Utc::now().timestamp() - 600,
        token: token.into(),
        signals: SeatSignals { kind: SignalKind::Hooked, ..Default::default() },
    }
}

fn team(minutes: u32) -> Team {
    Team {
        id: "t1".into(),
        name: "port".into(),
        repo: "/r".into(),
        guidance: String::new(),
        permission: Default::default(),
        policy: Policy {
            wind_down: "Wrap up: leave a note with {note} and stop.".into(),
            wind_down_minutes: minutes,
            ..Policy::default()
        },
        seats: vec![Seat::new("planner", SessionSource::ClaudeCode), Seat::new("builder", SessionSource::ClaudeCode)],
        paused: false,
        created: 1,
        updated: 1,
    }
}

async fn daemon(tag: &str, minutes: u32) -> Daemon {
    let dir = std::env::temp_dir().join(format!("mogeung-wind-down-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    store.save_team(&team(minutes)).unwrap();
    store.save_launch(&launch(&dir, "planner", "%990001", PLANNER)).unwrap();
    store.save_launch(&launch(&dir, "builder", "%990002", BUILDER)).unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let _ = state.teams_dir.set(dir.join("teams"));
    let _ = state.seats_dir.set(dir.join("seats"));
    let typed: Typed = Arc::new(Mutex::new(Vec::new()));
    let rec = typed.clone();
    let _ = state.sender.set(Arc::new(move |target: &str, text: &str| {
        rec.lock().unwrap().push((target.to_string(), text.to_string()));
        Ok(())
    }));
    let app = api::router_with_token(state.clone(), Some(SHARED.into()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap();
    });
    Daemon { state, port, typed, dir }
}

async fn http(port: u16, path: &str, bearer: Option<&str>, body: &str) -> (u16, String) {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let auth = bearer.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(10), s.read_to_end(&mut raw)).await.expect("an answer in time").unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status: u16 = text.split_whitespace().nth(1).and_then(|s| s.parse().ok()).expect("a status line");
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default();
    (status, body)
}

/// A live row for a seat, as the scan would publish one.
async fn live_row(d: &Daemon, id: &str, seat: &str, status: LiveStatus) {
    let now = chrono::Utc::now();
    let mut s = mogeungd::state::blank_session_with_id(id);
    s.alive = true;
    s.live_status = Some(status);
    s.last_event_at = now;
    s.started_at = now - chrono::Duration::seconds(60);
    s.seat = Some(mogeung_core::team::SeatTag { team_id: "t1".into(), team: "port".into(), seat: seat.into(), signals: SignalKind::Hooked });
    d.state.sessions.write().await.insert(id.to_string(), s);
}

async fn summary(d: &Daemon) -> mogeung_core::team::TeamSends {
    let mogeung_core::ServerMsg::Sends { teams } = d.state.sends_msg().await else { panic!("not sends") };
    teams.into_iter().find(|t| t.team_id == "t1").expect("the team's summary")
}

fn status_of(s: &mogeung_core::team::TeamSends, seat: &str) -> WindDownStatus {
    s.wind_down.as_ref().expect("a wind-down").seats.iter().find(|x| x.seat == seat).expect("the seat").status
}

/// Which seats still have a record, by name. Sorted: the store answers in
/// its own order.
async fn running(d: &Daemon) -> Vec<String> {
    let mut v: Vec<String> = d.state.launches.read().await.iter().map(|l| l.seat.clone()).collect();
    v.sort();
    v
}

/// The seats as running: `is_running` asks tmux, and nothing is there, so
/// the wind-down would find every seat gone. `tmux_session` names a real
/// throwaway session for each when tmux is here; without tmux the test is
/// skipped, as the tmux tests are.
async fn with_tmux_seats(d: &Daemon, tag: &str) -> Option<Vec<String>> {
    let out = std::process::Command::new("tmux").arg("-V").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let pid = std::process::id();
    let mut names = Vec::new();
    let mut launches = d.state.launches.write().await;
    for l in launches.iter_mut() {
        let name = format!("mogeung-wind-{tag}-{pid}-{}", l.seat);
        let _ = std::process::Command::new("tmux").args(["kill-session", "-t", &format!("={name}")]).output();
        let ok = std::process::Command::new("tmux").args(["new-session", "-d", "-s", &name, "sleep 60"]).output().ok()?.status.success();
        assert!(ok, "a throwaway seat session");
        l.tmux_session = name.clone();
        names.push(name);
    }
    Some(names)
}

fn kill_tmux(names: &[String]) {
    for n in names {
        let _ = std::process::Command::new("tmux").args(["kill-session", "-t", &format!("={n}")]).output();
    }
}

#[tokio::test]
async fn the_question_is_typed_once_each_and_the_seats_finish_by_note_by_stop_or_by_the_clock() {
    let d = daemon("asks", 1).await;
    let Some(tmux) = with_tmux_seats(&d, "asks").await else {
        eprintln!("skipped: no tmux here");
        return;
    };
    // The planner is mid-turn when asked; the builder sits at its prompt.
    live_row(&d, "p", "planner", LiveStatus::Busy).await;
    live_row(&d, "b", "builder", LiveStatus::Idle).await;

    d.state.wind_down_team("t1").await.expect("a wind-down");
    {
        let typed = d.typed.lock().unwrap();
        assert_eq!(typed.len(), 2, "{typed:?}");
        assert!(typed.iter().all(|(_, t)| t == "Wrap up: leave a note with mogeung-note and stop."), "the command's name in place of the marker: {typed:?}");
        assert!(typed.iter().any(|(p, _)| p == "%990001") && typed.iter().any(|(p, _)| p == "%990002"));
    }
    let s = summary(&d).await;
    assert_eq!(status_of(&s, "planner"), WindDownStatus::Asked);
    assert_eq!(status_of(&s, "builder"), WindDownStatus::Asked);
    let asked: Vec<_> = s.log.iter().filter(|e| e.kind == SendKind::WindDown).collect();
    assert_eq!(asked.len(), 2);
    assert!(asked.iter().all(|e| e.status == SendStatus::Delivered && e.hop == 0 && e.why == "you wound the team down"));
    assert_eq!((s.chain, s.this_hour), (0, 0), "your act: not a hop, not against the hour");
    // Asking again while it runs changes nothing.
    d.state.wind_down_team("t1").await.unwrap();
    assert_eq!(d.typed.lock().unwrap().len(), 2);

    // The planner's first Stop ends the turn it was in; the second is its answer.
    d.state.policy_event("t1", "planner", SeatEvent::Stop).await;
    assert_eq!(status_of(&summary(&d).await, "planner"), WindDownStatus::Asked, "the turn in flight, not the answer");
    assert_eq!(running(&d).await, vec!["builder", "planner"]);
    d.state.policy_event("t1", "planner", SeatEvent::Stop).await;
    assert_eq!(status_of(&summary(&d).await, "planner"), WindDownStatus::Stopped);
    assert_eq!(running(&d).await, vec!["builder"], "finished: its record is gone");

    // The builder leaves a note with its own token: kept, and the seat finished.
    let (code, line) = http(d.port, "/api/seat/note", Some(BUILDER), "Finished the parser. Next: the tests in rig.rs.").await;
    assert_eq!(code, 200, "{line}");
    assert_eq!(line, "kept — thank you; this seat is being stopped");
    let s = summary(&d).await;
    assert_eq!(status_of(&s, "builder"), WindDownStatus::Noted);
    assert_eq!(s.wind_down.as_ref().unwrap().seats.iter().find(|x| x.seat == "builder").unwrap().note.as_deref(), Some("Finished the parser. Next: the tests in rig.rs."));
    assert!(running(&d).await.is_empty(), "every seat finished");
    let note = std::fs::read_to_string(d.dir.join("seats/t1/builder/note.md")).expect("the note beside the seat's files");
    assert!(note.starts_with("# Note from seat `builder` of team `port` — "), "{note}");
    assert!(note.ends_with("Finished the parser. Next: the tests in rig.rs.\n"));
    // The record stays, finished, for the board to show.
    assert!(summary(&d).await.wind_down.is_some());

    // A stale token is nobody; a note from a seat not winding down is kept all the same.
    assert_eq!(http(d.port, "/api/seat/note", Some(SHARED), "hi").await.0, 401);
    assert_eq!(http(d.port, "/api/seat/note", None, "hi").await.0, 401);
    kill_tmux(&tmux);
    let _ = std::fs::remove_dir_all(&d.dir);
}

#[tokio::test]
async fn the_deadline_finishes_what_is_left_and_stop_now_cuts_it_short() {
    let d = daemon("clock", 1).await;
    let Some(tmux) = with_tmux_seats(&d, "clock").await else {
        eprintln!("skipped: no tmux here");
        return;
    };
    live_row(&d, "p", "planner", LiveStatus::Idle).await;
    live_row(&d, "b", "builder", LiveStatus::Idle).await;
    d.state.wind_down_team("t1").await.unwrap();
    // Not yet: the tick before the deadline finishes nothing.
    let now = chrono::Utc::now().timestamp();
    d.state.policy_tick().await;
    assert_eq!(running(&d).await.len(), 2);
    // Move the deadline into the past and tick.
    d.state.winding.lock().await.get_mut("t1").unwrap().state.deadline = now - 1;
    d.state.policy_tick().await;
    let s = summary(&d).await;
    assert_eq!(status_of(&s, "planner"), WindDownStatus::TimedOut);
    assert_eq!(status_of(&s, "builder"), WindDownStatus::TimedOut);
    assert!(running(&d).await.is_empty());
    kill_tmux(&tmux);

    // Stop now, mid-wind-down: the seats still asked are stopped without a note.
    let d2 = daemon("cut", 5).await;
    let Some(tmux2) = with_tmux_seats(&d2, "cut").await else { return };
    live_row(&d2, "p", "planner", LiveStatus::Idle).await;
    d2.state.wind_down_team("t1").await.unwrap();
    d2.state.stop_team("t1").await.unwrap();
    let s = summary(&d2).await;
    assert_eq!(status_of(&s, "planner"), WindDownStatus::Stopped);
    assert_eq!(status_of(&s, "builder"), WindDownStatus::Stopped);
    assert!(running(&d2).await.is_empty());
    kill_tmux(&tmux2);
    let _ = std::fs::remove_dir_all(&d.dir);
    let _ = std::fs::remove_dir_all(&d2.dir);
}

#[tokio::test]
async fn a_seat_whose_pane_is_already_gone_is_finished_at_once_and_a_wake_fires_for_nobody() {
    // No tmux sessions at all: every seat is gone the moment it is asked.
    let d = daemon("gone", 1).await;
    d.state.wind_down_team("t1").await.unwrap();
    assert!(d.typed.lock().unwrap().is_empty(), "nothing is typed into a pane that is not there");
    let s = summary(&d).await;
    assert_eq!(status_of(&s, "planner"), WindDownStatus::Gone);
    assert_eq!(status_of(&s, "builder"), WindDownStatus::Gone);
    assert!(running(&d).await.is_empty());
    // Nothing running: a second wind-down is refused in words.
    let err = d.state.wind_down_team("t1").await.unwrap_err().to_string();
    assert_eq!(err, "no seat of this team is running");
    let _ = std::fs::remove_dir_all(&d.dir);
}
