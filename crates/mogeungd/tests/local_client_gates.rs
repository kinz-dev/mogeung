//! The gates that judged by the bind, decided per connection. `R-J103`,
//! `R-J104`.
//!
//! Reported 2026-09-23 from the window on the laptop whose `config.toml` binds
//! the LAN: *"chat is refused on a daemon bound beyond loopback"* — in the Ask
//! Mogeung panel of that same machine. Three gates were read from the bind —
//! the chat (ADR-0030 clause 4), starting a run configuration (ADR-0025 clause
//! 4) and typing into a session (ADR-0003's amendment, clause 4) — and each
//! refused the machine's own window along with the LAN it was written against.
//! These run a real server, the way `server::run` serves it, with a daemon
//! marked as bound beyond loopback on all three.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::{ClientMsg, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use tokio_tungstenite::tungstenite::Message;

async fn lan_bound_daemon(tag: &str, with_peer_address: bool) -> String {
    let dir = std::env::temp_dir().join(format!("mogeung-chatgate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    // What `server::run` decides for a bind like 0.0.0.0:7717, with no
    // `--allow-run`.
    state.model.set_chat_allowed(false);
    state.runs.set_allowed(false);
    let _ = state.send_allowed.set(false);
    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        if with_peer_address {
            axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
                .await
                .unwrap();
        } else {
            axum::serve(listener, app).await.unwrap();
        }
    });
    format!("ws://127.0.0.1:{port}/ws")
}

/// Ask, and return the first answer of the kind wanted, skipping the
/// snapshot and anything broadcast in between.
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

fn history_refusal(msg: ServerMsg) -> Option<Option<String>> {
    match msg {
        ServerMsg::ChatHistory { refusal, .. } => Some(refusal),
        _ => None,
    }
}

fn model_chat_allowed(msg: ServerMsg) -> Option<bool> {
    match msg {
        ServerMsg::Health { health } => Some(health.model.map(|m| m.chat_allowed).unwrap_or(false)),
        _ => None,
    }
}

#[tokio::test]
async fn the_window_on_this_machine_keeps_its_chat_on_a_lan_bound_daemon() {
    let url = lan_bound_daemon("local", true).await;
    let refusal = ask(&url, ClientMsg::ChatList {}, history_refusal).await;
    assert_eq!(refusal, None, "a loopback client is on this machine and is not refused");
    let allowed = ask(&url, ClientMsg::FetchHealth, model_chat_allowed).await;
    assert!(allowed, "and the health row it is shown says the chat is open");
}

/// With no peer address to go on — a router served without connection info —
/// the bind is the only fact, and it still decides.
#[tokio::test]
async fn without_a_peer_address_the_bind_still_decides() {
    let url = lan_bound_daemon("unknown", false).await;
    let refusal = ask(&url, ClientMsg::ChatList {}, history_refusal).await;
    assert!(refusal.is_some_and(|r| r.contains("beyond loopback")));
    assert!(!ask(&url, ClientMsg::FetchHealth, model_chat_allowed).await);
}

fn runs_allowed(msg: ServerMsg) -> Option<bool> {
    match msg {
        ServerMsg::RunConfigs { allowed, .. } => Some(allowed),
        _ => None,
    }
}

fn error_text(msg: ServerMsg) -> Option<String> {
    match msg {
        ServerMsg::Error { message } => Some(message),
        _ => None,
    }
}

fn send(session_id: &str) -> ClientMsg {
    ClientMsg::SendToSession { session_id: session_id.into(), text: "hello".into() }
}

/// `R-J104`: the run panel a client on this machine is shown says it will run.
#[tokio::test]
async fn the_window_on_this_machine_is_offered_its_run_configurations() {
    let url = lan_bound_daemon("run-local", true).await;
    let allowed = ask(&url, ClientMsg::FetchRunConfigs { session_id: "any".into() }, runs_allowed).await;
    assert!(allowed, "a loopback client is on this machine, and the panel should draw its buttons");
}

/// `R-J104`: typing into a session is refused to everyone but this machine.
/// Past the gate, what stops an unknown session is the lookup — the next
/// refusal in line — so the error names the session, not the bind.
#[tokio::test]
async fn the_window_on_this_machine_is_past_the_send_gate() {
    let url = lan_bound_daemon("send-local", true).await;
    let why = ask(&url, send("no-such-session"), error_text).await;
    assert!(!why.contains("beyond loopback"), "refused by the bind when it should not have been: {why}");
    assert!(why.contains("not one this daemon knows"), "{why}");
}

#[tokio::test]
async fn without_a_peer_address_run_and_send_are_still_judged_by_the_bind() {
    let url = lan_bound_daemon("run-send-unknown", false).await;
    assert!(!ask(&url, ClientMsg::FetchRunConfigs { session_id: "any".into() }, runs_allowed).await);
    let why = ask(&url, send("no-such-session"), error_text).await;
    assert!(why.contains("beyond loopback"), "{why}");
}

// -- Teams. `R-P3`, feature 0051: a definition is written like a run is
// started (ADR-0025's amendment, clause 4) — by this machine whatever the
// bind, by anyone else only with `--allow-run`.

fn teams_allowed(msg: ServerMsg) -> Option<Option<bool>> {
    match msg {
        ServerMsg::Teams { allowed, .. } => Some(allowed),
        _ => None,
    }
}

fn team_named(name: &str) -> ClientMsg {
    ClientMsg::TeamSave {
        team: mogeung_core::team::Team {
            id: String::new(),
            name: name.into(),
            repo: String::new(),
            guidance: String::new(),
            permission: Default::default(),
            policy: Default::default(),
            seats: vec![],
            paused: false,
            created: 0,
            updated: 0,
        },
    }
}

#[tokio::test]
async fn the_window_on_this_machine_may_write_a_team_on_a_lan_bound_daemon() {
    let url = lan_bound_daemon("team-local", true).await;
    let allowed = ask(&url, ClientMsg::TeamList, teams_allowed).await;
    assert_eq!(allowed, Some(true), "the list answers the asker with its own gate");
    // Past the gate, the next refusal in line is validation — the error names
    // the field, not the bind.
    let why = ask(&url, team_named(""), error_text).await;
    assert!(why.contains("needs a name"), "{why}");
}

#[tokio::test]
async fn without_a_peer_address_a_team_write_is_judged_by_the_bind() {
    let url = lan_bound_daemon("team-unknown", false).await;
    assert_eq!(ask(&url, ClientMsg::TeamList, teams_allowed).await, Some(false));
    let why = ask(&url, team_named("port"), error_text).await;
    assert!(why.contains("--allow-run"), "{why}");
    let why = ask(&url, ClientMsg::TeamDelete { id: "x".into() }, error_text).await;
    assert!(why.contains("--allow-run"), "{why}");
}

/// `R-P4`: starting a team is judged like starting a run. Past the gate, the
/// next refusal in line is the lookup — the error names the team, not the bind.
#[tokio::test]
async fn the_window_on_this_machine_is_past_the_boot_gate() {
    let url = lan_bound_daemon("boot-local", true).await;
    let why = ask(&url, ClientMsg::TeamBoot { id: "no-such-team".into() }, error_text).await;
    assert!(why.contains("no team with id"), "{why}");
    let why = ask(&url, ClientMsg::TeamStop { id: "no-such-team".into() }, error_text).await;
    assert!(why.contains("no seat of this team is running"), "{why}");
}

/// `R-P6`: the pause switch is judged like a save, and a release or a drop
/// like your own send — a release types into a seat. Past either gate the
/// next refusal is the lookup, in words.
#[tokio::test]
async fn pause_is_judged_like_a_save_and_a_release_like_a_send() {
    let url = lan_bound_daemon("sends-lan", false).await;
    let why = ask(&url, ClientMsg::TeamPause { id: "x".into(), paused: true }, error_text).await;
    assert!(why.contains("--allow-run"), "{why}");
    for cmd in [
        ClientMsg::SendRelease { team_id: "x".into(), id: "y".into() },
        ClientMsg::SendDrop { team_id: "x".into(), id: "y".into() },
    ] {
        let why = ask(&url, cmd, error_text).await;
        assert!(why.contains("beyond loopback"), "{why}");
    }
    let url = lan_bound_daemon("sends-local", true).await;
    let why = ask(&url, ClientMsg::TeamPause { id: "x".into(), paused: true }, error_text).await;
    assert!(why.contains("no team with id"), "{why}");
    let why = ask(&url, ClientMsg::SendRelease { team_id: "x".into(), id: "y".into() }, error_text).await;
    assert!(why.contains("no held send"), "{why}");
}

/// `R-P9`: the one write into a worktree is judged like the seat verbs and
/// sits behind the write guard; past both, the lookup refuses in words.
#[tokio::test]
async fn a_worktree_write_is_judged_like_a_seat_verb_and_the_write_guard() {
    let write = || ClientMsg::FileWrite {
        session_id: "x".into(),
        path: "a.rs".into(),
        content: String::new(),
        base_hash: String::new(),
        force: false,
    };
    let url = lan_bound_daemon("write-lan", false).await;
    let why = ask(&url, write(), error_text).await;
    assert!(why.contains("token") || why.contains("--allow-run"), "{why}");
    let url = lan_bound_daemon("write-local", true).await;
    let why = ask(&url, write(), error_text).await;
    assert!(why.contains("not one this daemon knows"), "{why}");
}

#[tokio::test]
async fn without_a_peer_address_boot_stop_and_remove_are_judged_by_the_bind() {
    let url = lan_bound_daemon("boot-unknown", false).await;
    for cmd in [
        ClientMsg::TeamBoot { id: "x".into() },
        ClientMsg::TeamStop { id: "x".into() },
        ClientMsg::SeatWorktreeRemove { team_id: "x".into(), seat: "s".into() },
    ] {
        let why = ask(&url, cmd, error_text).await;
        assert!(why.contains("--allow-run"), "{why}");
    }
}
