//! The one write into a worktree. `R-P9`, feature 0051, ADR-0019's 2026-10-09
//! amendment.
//!
//! Pinned: a save lands only in a worktree the daemon made for a seat — an
//! observed session and a seat in the team's checkout are refused before the
//! path is looked at; a `FileContent` carries the hash a save brings back, a
//! file that moved is refused with the current text and no override, and a
//! seat mid-turn in the worktree is refused until the save says *anyway*.
//! Everything through the real router, as a window would.
//!
//! Costs nothing: the "worktree" is a temp directory on a launch record, the
//! "seat" a session row the test puts on the board. No agent is launched.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::session::{LiveStatus, SessionSource};
use mogeung_core::team::{SeatLaunch, SeatSignals, SeatTag, SignalKind};
use mogeung_core::{ClientMsg, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

struct Daemon {
    state: Arc<AppState>,
    url: String,
    worktree: std::path::PathBuf,
    dir: std::path::PathBuf,
}

async fn daemon(tag: &str) -> Daemon {
    let dir = std::env::temp_dir().join(format!("mogeung-seat-write-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let worktree = dir.join("wt");
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    std::fs::write(worktree.join("src/lib.rs"), "fn a() {}\n").unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    let mk = |seat: &str, d: &std::path::Path, worktree: bool| SeatLaunch {
        team_id: "t1".into(),
        team: "port".into(),
        seat: seat.into(),
        cli: SessionSource::ClaudeCode,
        session_id: None,
        pane_id: format!("%{seat}"),
        tmux_target: format!("geung-port-{seat}:0.0"),
        tmux_session: format!("geung-port-{seat}"),
        team_session: String::new(),
        dir: d.to_string_lossy().into_owned(),
        worktree,
        files_dir: "/f".into(),
        started: 1,
        alive: false,
        token: String::new(),
        signals: SeatSignals { kind: SignalKind::Plain, ..Default::default() },
    };
    store.save_launch(&mk("builder", &worktree, true)).unwrap();
    store.save_launch(&mk("keeper", &dir, false)).unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let _ = state.teams_dir.set(dir.join("teams"));
    let _ = state.seats_dir.set(dir.join("seats"));
    // The rows: the builder's, live and idle in its worktree; the keeper's,
    // in the team's checkout; and one you started by hand.
    for (id, seat, cwd, status) in [
        ("b", Some("builder"), worktree.clone(), LiveStatus::Idle),
        ("k", Some("keeper"), dir.clone(), LiveStatus::Idle),
        ("hand", None, worktree.clone(), LiveStatus::Idle),
    ] {
        let mut s = mogeungd::state::blank_session_with_id(id);
        s.alive = true;
        s.live_status = Some(status);
        s.cwd = cwd.to_string_lossy().into_owned();
        s.seat = seat.map(|name| SeatTag { team_id: "t1".into(), team: "port".into(), seat: name.into(), signals: SignalKind::Plain });
        state.sessions.write().await.insert(id.into(), s);
    }
    let app = api::router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .unwrap();
    });
    Daemon { state, url: format!("ws://127.0.0.1:{port}/ws"), worktree, dir }
}

/// Ask, and take the first answer `pick` accepts.
async fn ask<T>(url: &str, cmd: ClientMsg, pick: impl Fn(ServerMsg) -> Option<T>) -> T {
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    ws.send(Message::Text(serde_json::to_string(&cmd).unwrap().into())).await.unwrap();
    let wait = async {
        while let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(msg) = serde_json::from_str::<ServerMsg>(&t) {
                if let Some(v) = pick(msg) {
                    return v;
                }
            }
        }
        panic!("the socket closed before answering");
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), wait).await.expect("an answer in time")
}

fn error_text(m: ServerMsg) -> Option<String> {
    match m {
        ServerMsg::Error { message } => Some(message),
        _ => None,
    }
}

#[derive(Debug)]
enum Answer {
    Written(String),
    Refused { why: String, current: Option<String>, hash: String, busy: Option<String> },
    Error(String),
}

fn write_answer(m: ServerMsg) -> Option<Answer> {
    match m {
        ServerMsg::FileWritten { hash, .. } => Some(Answer::Written(hash)),
        ServerMsg::FileWriteRefused { why, current, hash, busy, .. } => Some(Answer::Refused { why, current, hash, busy }),
        ServerMsg::Error { message } => Some(Answer::Error(message)),
        _ => None,
    }
}

fn write(session: &str, content: &str, base: &str, force: bool) -> ClientMsg {
    ClientMsg::FileWrite {
        session_id: session.into(),
        path: "src/lib.rs".into(),
        content: content.into(),
        base_hash: base.into(),
        force,
    }
}

#[tokio::test]
async fn a_save_lands_only_in_a_seats_worktree_and_only_on_the_file_you_opened() {
    let d = daemon("save").await;
    // Read, and the hash rides with the body.
    let (content, hash) = ask(&d.url, ClientMsg::FetchFile { session_id: "b".into(), path: "src/lib.rs".into() }, |m| match m {
        ServerMsg::FileContent { content, hash, .. } => Some((content, hash)),
        _ => None,
    })
    .await;
    assert_eq!(content, "fn a() {}\n");
    assert_eq!(hash.len(), 64, "sha256 as hex");

    // The save, against the hash it was read with.
    let a = ask(&d.url, write("b", "fn a() {}\nfn b() {}\n", &hash, false), write_answer).await;
    let Answer::Written(new_hash) = a else { panic!("{a:?}") };
    assert_eq!(std::fs::read_to_string(d.worktree.join("src/lib.rs")).unwrap(), "fn a() {}\nfn b() {}\n");
    assert_ne!(new_hash, hash);

    // Stale: the base is the old hash, and an agent — here, the test — wrote
    // under you. Refused with what is there now, and `force` does not help.
    std::fs::write(d.worktree.join("src/lib.rs"), "fn a() {}\nfn b() {}\n// an agent was here\n").unwrap();
    for force in [false, true] {
        let a = ask(&d.url, write("b", "mine", &new_hash, force), write_answer).await;
        let Answer::Refused { why, current, hash: now, busy } = a else { panic!("{a:?}") };
        assert!(why.contains("changed on disk"), "{why}");
        assert_eq!(current.as_deref(), Some("fn a() {}\nfn b() {}\n// an agent was here\n"));
        assert_eq!(now.len(), 64);
        assert!(busy.is_none());
    }
    assert!(std::fs::read_to_string(d.worktree.join("src/lib.rs")).unwrap().contains("an agent was here"), "untouched");

    // A new file saves against a base of nothing, and lands in the worktree only.
    let a = ask(
        &d.url,
        ClientMsg::FileWrite { session_id: "b".into(), path: "NOTES.md".into(), content: "hi\n".into(), base_hash: String::new(), force: false },
        write_answer,
    )
    .await;
    assert!(matches!(a, Answer::Written(_)), "{a:?}");
    assert_eq!(std::fs::read_to_string(d.worktree.join("NOTES.md")).unwrap(), "hi\n");
    for bad in ["../escape.md", "/etc/passwd", ".git/config"] {
        let a = ask(
            &d.url,
            ClientMsg::FileWrite { session_id: "b".into(), path: bad.into(), content: "x".into(), base_hash: String::new(), force: false },
            write_answer,
        )
        .await;
        assert!(matches!(a, Answer::Error(_)), "{bad}: {a:?}");
    }
    assert!(!d.dir.join("escape.md").exists());
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn a_busy_seat_refuses_the_save_until_you_say_anyway() {
    let d = daemon("busy").await;
    {
        let mut sessions = d.state.sessions.write().await;
        sessions.get_mut("b").unwrap().live_status = Some(LiveStatus::Busy);
    }
    let hash = mogeungd::write::hash_bytes(b"fn a() {}\n");
    let a = ask(&d.url, write("b", "fn a() { /* mine */ }\n", &hash, false), write_answer).await;
    let Answer::Refused { why, busy, current, .. } = a else { panic!("{a:?}") };
    assert!(why.contains("mid-turn"), "{why}");
    assert_eq!(busy.as_deref(), Some("builder"));
    assert!(current.is_none(), "the file did not move; nothing to show");
    assert_eq!(std::fs::read_to_string(d.worktree.join("src/lib.rs")).unwrap(), "fn a() {}\n");
    let a = ask(&d.url, write("b", "fn a() { /* mine */ }\n", &hash, true), write_answer).await;
    assert!(matches!(a, Answer::Written(_)), "{a:?}");
    assert_eq!(std::fs::read_to_string(d.worktree.join("src/lib.rs")).unwrap(), "fn a() { /* mine */ }\n");
    std::fs::remove_dir_all(&d.dir).ok();
}

/// Fence 1: before the path is looked at. A session you started by hand has
/// no record; a seat in the team's own checkout has one that says so.
#[tokio::test]
async fn an_observed_session_and_a_checkout_seat_are_refused_in_words() {
    let d = daemon("fence1").await;
    let why = ask(&d.url, write("hand", "x", "", false), error_text).await;
    assert!(why.contains("not a seat"), "{why}");
    let why = ask(&d.url, write("k", "x", "", false), error_text).await;
    assert!(why.contains("team's own checkout"), "{why}");
    let why = ask(&d.url, write("nobody", "x", "", false), error_text).await;
    assert!(why.contains("not one this daemon knows"), "{why}");
    assert_eq!(std::fs::read_to_string(d.worktree.join("src/lib.rs")).unwrap(), "fn a() {}\n");
    std::fs::remove_dir_all(&d.dir).ok();
}
