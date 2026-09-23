//! ADR-0030 clause 4, decided per connection. `R-J103`.
//!
//! Reported 2026-09-23 from the window on the laptop whose `config.toml` binds
//! the LAN: *"chat is refused on a daemon bound beyond loopback"* — in the Ask
//! Mogeung panel of that same machine. The clause keeps a LAN-reachable daemon
//! from becoming an LLM proxy for the LAN; it was judged from the bind, so it
//! refused the machine's own window too. These run a real server, the way
//! `server::run` serves it, with a daemon marked as bound beyond loopback.

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
    // What `server::run` decides for a bind like 0.0.0.0:7717.
    state.model.set_chat_allowed(false);
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
