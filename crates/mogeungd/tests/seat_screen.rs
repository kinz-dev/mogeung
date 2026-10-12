//! Reading a seat's pane, through the scan. `R-P11`, feature 0051, ADR-0044.
//!
//! Pinned: a capture is asked for a seat's pane and no other — the capturer
//! records what it was asked, and a hand-started session on the board is
//! never in that list; a gate read off the pane lands on the row with its
//! excerpt and ranks with *(screen)* in the detail; an unchanged screen is
//! not reclassified; an unknown screen says `unknown` and raises
//! `screen:<cli>` through Health; a pane that cannot be captured is
//! `unknown` with a note; and *press Enter* is pressed once, as a logged
//! automatic send, only when the team's policy says so.
//!
//! Costs nothing: the "pane" is a fixture text the capturer answers with.

use mogeung_core::attention::{classify, AttentionConfig, AttentionReason};
use mogeung_core::session::SessionSource;
use mogeung_core::team::{Policy, ScreenShape, Seat, SeatLaunch, SeatSignals, SendKind, SendStatus, SignalKind, Team};
use mogeung_core::ServerMsg;
use mogeungd::screen::SeatPane;
use mogeungd::{state::AppState, store::Store};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

type Screens = Arc<Mutex<HashMap<String, Option<String>>>>;
type Asked = Arc<Mutex<Vec<String>>>;
type Typed = Arc<Mutex<Vec<(String, String)>>>;

struct Daemon {
    state: Arc<AppState>,
    screens: Screens,
    asked: Asked,
    typed: Typed,
    dir: std::path::PathBuf,
}

fn fixture(cli: &str, gate: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/screens/{cli}/{gate}.txt"));
    std::fs::read_to_string(p).unwrap()
}

fn launch(seat: &str, cli: SessionSource) -> SeatLaunch {
    SeatLaunch {
        team_id: "t1".into(),
        team: "port".into(),
        seat: seat.into(),
        cli,
        session_id: None,
        pane_id: format!("%{seat}"),
        tmux_target: format!("geung-port-{seat}:0.0"),
        tmux_session: format!("geung-port-{seat}"),
        team_session: String::new(),
        dir: "/w".into(),
        worktree: true,
        files_dir: "/f".into(),
        started: 1,
        alive: false,
        token: String::new(),
        signals: SeatSignals { kind: SignalKind::Plain, ..Default::default() },
    }
}

async fn daemon(tag: &str, readiness_press: bool) -> Daemon {
    let dir = std::env::temp_dir().join(format!("mogeung-seat-screen-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    store
        .save_team(&Team {
            id: "t1".into(),
            name: "port".into(),
            repo: "/r".into(),
            guidance: String::new(),
            permission: Default::default(),
            policy: Policy { readiness_press, ..Policy::default() },
            seats: vec![Seat::new("reviewer", SessionSource::ClaudeCode), Seat::new("builder", SessionSource::Codex)],
            paused: false,
            created: 1,
            updated: 1,
        })
        .unwrap();
    store.save_launch(&launch("reviewer", SessionSource::ClaudeCode)).unwrap();
    store.save_launch(&launch("builder", SessionSource::Codex)).unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let _ = state.teams_dir.set(dir.join("teams"));
    let _ = state.seats_dir.set(dir.join("seats"));
    // A session you started by hand, alive, on the board beside the seats.
    let mut hand = mogeungd::state::blank_session_with_id("hand-started");
    hand.alive = true;
    hand.tmux_target = Some("mine:0.0".into());
    state.sessions.write().await.insert("hand-started".into(), hand);
    let screens: Screens = Arc::new(Mutex::new(HashMap::new()));
    let asked: Asked = Arc::new(Mutex::new(Vec::new()));
    {
        let screens = screens.clone();
        let asked = asked.clone();
        let _ = state.capturer.set(Arc::new(move |p: &SeatPane| {
            asked.lock().unwrap().push(p.id().to_string());
            screens.lock().unwrap().get(p.id()).cloned().flatten()
        }));
    }
    let typed: Typed = Arc::new(Mutex::new(Vec::new()));
    let rec = typed.clone();
    let _ = state.sender.set(Arc::new(move |target: &str, text: &str| {
        rec.lock().unwrap().push((target.to_string(), text.to_string()));
        Ok(())
    }));
    Daemon { state, screens, asked, typed, dir }
}

async fn board(state: &AppState) -> Vec<mogeung_core::session::Session> {
    let ServerMsg::Snapshot { sessions, .. } = state.snapshot(false).await else { panic!("not a snapshot") };
    sessions
}

fn row<'a>(rows: &'a [mogeung_core::session::Session], id: &str) -> &'a mogeung_core::session::Session {
    rows.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("no row {id}"))
}

#[tokio::test]
async fn a_gate_read_off_a_seats_pane_lands_on_its_row_and_ranks_as_a_reading() {
    let d = daemon("gate", false).await;
    d.screens.lock().unwrap().insert("%reviewer".into(), Some(fixture("claude", "permission-menu")));
    d.screens.lock().unwrap().insert("%builder".into(), Some(fixture("codex", "prompt")));
    d.state.scan().await;

    // Only the seats' panes were asked for; the hand-started session never was.
    let asked = d.asked.lock().unwrap().clone();
    assert!(asked.contains(&"%reviewer".to_string()) && asked.contains(&"%builder".to_string()), "{asked:?}");
    assert!(!asked.iter().any(|p| p.contains("mine")), "{asked:?}");

    let rows = board(&d.state).await;
    let r = row(&rows, "seat:t1:reviewer");
    let screen = r.screen.as_ref().expect("the reviewer's screen");
    assert_eq!(screen.shape, ScreenShape::PermissionMenu);
    assert!(screen.excerpt.join("\n").contains("Do you want to proceed?"), "{:?}", screen.excerpt);
    let b = row(&rows, "seat:t1:builder");
    assert_eq!(b.screen.as_ref().unwrap().shape, ScreenShape::Prompt);
    assert!(row(&rows, "hand-started").screen.is_none(), "an observed session has no screen, by construction");

    // The reading ranks, and says it is one. The seat row's pane is not on
    // this machine, so it is dead here; ranked as if alive to see the reason.
    let mut live = r.clone();
    live.alive = true;
    let item = classify(&live, chrono::Utc::now(), &AttentionConfig::default());
    assert_eq!(item.reason, AttentionReason::AwaitingPermission, "{}", item.detail);
    assert_eq!(item.detail, "permission menu (screen)");
    let mut quiet = b.clone();
    quiet.alive = true;
    let item = classify(&quiet, chrono::Utc::now(), &AttentionConfig::default());
    assert_ne!(item.detail, "at the prompt (screen)", "a quiet shape never ranks");

    // The same screen again is not reclassified: the capturer is asked, the
    // read keeps its stamp.
    let first = screen.read_at;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    d.state.scan().await;
    let rows = board(&d.state).await;
    assert_eq!(row(&rows, "seat:t1:reviewer").screen.as_ref().unwrap().read_at, first);
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn an_unknown_screen_says_so_and_is_raised_through_health() {
    let d = daemon("unknown", false).await;
    d.screens.lock().unwrap().insert("%builder".into(), Some("something nobody has seen\n".into()));
    // The reviewer's pane cannot be captured at all.
    d.screens.lock().unwrap().insert("%reviewer".into(), None);
    d.state.scan().await;
    let rows = board(&d.state).await;
    let b = row(&rows, "seat:t1:builder").screen.as_ref().unwrap().clone();
    assert_eq!(b.shape, ScreenShape::Unknown);
    assert_eq!(b.shape.label(), "unknown");
    let r = row(&rows, "seat:t1:reviewer").screen.as_ref().unwrap().clone();
    assert_eq!(r.shape, ScreenShape::Unknown);
    assert!(r.excerpt[0].contains("could not be captured"), "{:?}", r.excerpt);
    let health = d.state.health().await;
    assert!(health.unknown_types.iter().any(|(k, _)| k == "screen:codex"), "{:?}", health.unknown_types);
    assert!(!health.unknown_types.iter().any(|(k, _)| k == "screen:claude"), "a pane that is gone is not drift");
    // Neither ranks on the reading.
    let mut live = row(&rows, "seat:t1:builder").clone();
    live.alive = true;
    assert!(!classify(&live, chrono::Utc::now(), &AttentionConfig::default()).detail.contains("(screen)"));
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn press_enter_is_pressed_once_and_only_when_the_policy_says_so() {
    // Off: the shape ranks as needing you and nothing is pressed.
    let d = daemon("press-off", false).await;
    d.screens.lock().unwrap().insert("%builder".into(), Some(fixture("codex", "press-enter")));
    d.state.scan().await;
    let rows = board(&d.state).await;
    let mut live = row(&rows, "seat:t1:builder").clone();
    assert_eq!(live.screen.as_ref().unwrap().shape, ScreenShape::PressEnter);
    live.alive = true;
    let item = classify(&live, chrono::Utc::now(), &AttentionConfig::default());
    assert_eq!((item.reason, item.detail.as_str()), (AttentionReason::AwaitingInput, "press Enter (screen)"));
    assert!(d.typed.lock().unwrap().is_empty(), "nothing pressed without the flag");
    std::fs::remove_dir_all(&d.dir).ok();

    // On: Enter, once, logged as a press that is not a hop.
    let d = daemon("press-on", true).await;
    d.screens.lock().unwrap().insert("%builder".into(), Some(fixture("codex", "press-enter")));
    d.state.scan().await;
    assert_eq!(d.typed.lock().unwrap().as_slice(), &[("%builder".to_string(), "<Enter>".to_string())]);
    d.state.scan().await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    d.state.scan().await;
    assert_eq!(d.typed.lock().unwrap().len(), 1, "one screen, one press");
    let ServerMsg::Sends { teams } = d.state.sends_msg().await else { panic!() };
    let log = &teams.iter().find(|t| t.team_id == "t1").unwrap().log;
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].kind, log[0].status, log[0].hop), (SendKind::Press, SendStatus::Delivered, 0));
    assert_eq!(log[0].text, "Enter");
    assert!(log[0].why.contains("readiness press"));
    std::fs::remove_dir_all(&d.dir).ok();
}
