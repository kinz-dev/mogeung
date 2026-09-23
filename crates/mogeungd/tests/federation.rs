//! Two daemons in one process, one following the other. `R-I21`, `R-I22`,
//! [feature 0047](../../../docs/features/0047-daemon-federation/spec.md).
//!
//! What must hold, each a way federation could fail quietly:
//!
//! - a follow mirrors the peer's sessions under a namespaced id, and ranks a
//!   waiting one above a local idle one — one queue, answered once;
//! - a read about a mirrored session is answered **by the peer**, back to the
//!   client that asked, with the machine in its ids;
//! - a write is refused without the follow's consent, the desktop never
//!   crosses, and with consent the write reaches the peer;
//! - a client from **another machine** never sees a mirrored session, a peer
//!   list, or a way to follow — no re-export;
//! - a peer gone quiet greys out and leaves the top of the queue;
//! - following this machine's own daemon is recognised and refused.
//!
//! Every connection here is loopback, so "another machine" is made the only
//! way a daemon can be told it: a listener served **without** connection
//! info, on a daemon whose bind says LAN, judges every client as foreign.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::wire::PeerStatus;
use mogeung_core::{ClientMsg, LiveStatus, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn home(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mogeung-fed-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A daemon with its own machine id — two daemons in one process would
/// otherwise read the same `~/.mogeung/machine-id` and each call the other
/// itself.
fn daemon(tag: &str, machine: &str) -> (Arc<AppState>, std::path::PathBuf) {
    let dir = home(tag);
    let store = Store::open(&dir.join("t.db")).unwrap();
    let mut state = AppState::with_home(store, dir.join("claude")).unwrap();
    let st = Arc::get_mut(&mut state).expect("nothing else holds a fresh state");
    st.identity.machine_id = Some(machine.into());
    st.identity.host = Some(format!("{machine}-host"));
    state.scratch_dir.set(dir.join("scratch")).unwrap();
    state.notes_dir.set(dir.join("notes")).unwrap();
    state.federation.path.set(dir.join("peers.json")).unwrap();
    // A bind beyond loopback: any client this daemon cannot see an address
    // for is judged as another machine's.
    state.model.set_chat_allowed(false);
    (state, dir)
}

/// Serve `state`; `with_peer_address` decides whether loopback clients are
/// recognised as this machine's.
async fn serve(state: &Arc<AppState>, with_peer_address: bool) -> String {
    let app = api::router(state.clone());
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

/// Serve on a separate runtime; sending on the returned channel shuts that
/// runtime down, and every connection with it.
fn serve_on_own_runtime(state: Arc<AppState>) -> (String, std::sync::mpsc::Sender<()>) {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (url_tx, url_rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        rt.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            url_tx.send(format!("ws://127.0.0.1:{}/ws", listener.local_addr().unwrap().port())).unwrap();
            tokio::spawn(async move { axum::serve(listener, api::router(state)).await.unwrap() });
        });
        let _ = stop_rx.recv();
        rt.shutdown_timeout(Duration::from_secs(1));
    });
    (url_rx.recv().unwrap(), stop_tx)
}

async fn connect(url: &str) -> Ws {
    tokio_tungstenite::connect_async(url).await.unwrap().0
}

async fn send(ws: &mut Ws, cmd: ClientMsg) {
    ws.send(Message::Text(serde_json::to_string(&cmd).unwrap().into())).await.unwrap();
}

/// Read until `pick` finds something, or fail after `secs`.
async fn until<T>(ws: &mut Ws, secs: u64, mut pick: impl FnMut(&ServerMsg) -> Option<T>) -> T {
    let wait = async {
        while let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(msg) = serde_json::from_str::<ServerMsg>(&t) {
                if let Some(v) = pick(&msg) {
                    return v;
                }
            }
        }
        panic!("the socket closed first");
    };
    tokio::time::timeout(Duration::from_secs(secs), wait).await.expect("in time")
}

/// A session waiting for its human, with a file in its folder.
async fn waiting_session(state: &Arc<AppState>, id: &str, dir: &std::path::Path) {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(work.join("a.txt"), "hello from the peer").unwrap();
    let mut s = mogeungd::state::blank_session_with_id(id);
    s.title = Some("the remote one".into());
    s.cwd = work.to_string_lossy().into_owned();
    s.alive = true;
    s.live_status = Some(LiveStatus::Idle);
    state.sessions.write().await.insert(s.id.clone(), s);
}

async fn idle_local(state: &Arc<AppState>, id: &str) {
    let mut s = mogeungd::state::blank_session_with_id(id);
    s.title = Some("the local one".into());
    s.reviewed = true;
    state.sessions.write().await.insert(s.id.clone(), s);
}

fn follow(url: &str, allow_writes: bool) -> ClientMsg {
    ClientMsg::Follow { id: "c1".into(), name: "the peer".into(), url: url.into(), token: None, allow_writes }
}

const MIRRORED: &str = "@peer-m:s1";

#[tokio::test]
async fn a_followed_daemon_is_mirrored_ranked_and_answered_through() {
    let (peer, peer_dir) = daemon("peer", "peer-m");
    waiting_session(&peer, "s1", &peer_dir).await;
    let peer_url = serve(&peer, false).await;

    let (home_d, _dir) = daemon("home", "home-m");
    idle_local(&home_d, "local-1").await;
    let home_url = serve(&home_d, true).await;
    let mut ws = connect(&home_url).await;

    send(&mut ws, follow(&peer_url, false)).await;

    // Mirrored, namespaced, and said to be remote.
    let origin = until(&mut ws, 10, |m| match m {
        ServerMsg::SessionUpdated { session } if session.id == MIRRORED => session.origin.clone(),
        _ => None,
    })
    .await;
    assert_eq!(origin.machine_id, "peer-m");
    assert_eq!(origin.host.as_deref(), Some("peer-m-host"));
    assert!(!origin.stale);

    // One queue: the remote waiting session above the local idle one.
    let order = until(&mut ws, 10, |m| match m {
        ServerMsg::Queue { queue } if queue.iter().any(|i| i.session_id == MIRRORED) => {
            Some(queue.iter().map(|i| i.session_id.clone()).collect::<Vec<_>>())
        }
        _ => None,
    })
    .await;
    assert_eq!(order.first().map(String::as_str), Some(MIRRORED), "{order:?}");
    assert!(order.contains(&"local-1".to_string()));

    // The follow list says it is up — and never carries a token.
    send(&mut ws, ClientMsg::FetchPeers).await;
    let peers = until(&mut ws, 10, |m| match m {
        ServerMsg::Peers { peers } if peers.iter().any(|p| p.state == PeerStatus::Up) => Some(peers.clone()),
        _ => None,
    })
    .await;
    assert_eq!((peers[0].sessions, peers[0].machine_id.as_deref()), (1, Some("peer-m")));

    // A read is answered by the peer, with the machine in its ids.
    send(&mut ws, ClientMsg::FetchFile { session_id: MIRRORED.into(), path: "a.txt".into() }).await;
    let (sid, content) = until(&mut ws, 10, |m| match m {
        ServerMsg::FileContent { session_id, content, .. } => Some((session_id.clone(), content.clone())),
        _ => None,
    })
    .await;
    assert_eq!((sid.as_str(), content.as_str()), (MIRRORED, "hello from the peer"));

    // A write is refused without consent; the desktop never crosses.
    send(&mut ws, ClientMsg::GitStage { session_id: MIRRORED.into(), repo: None, paths: vec!["a.txt".into()] }).await;
    let why = until(&mut ws, 10, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(why.contains("allow changes"), "{why}");
    send(&mut ws, ClientMsg::FocusTerminal { session_id: MIRRORED.into() }).await;
    let why = until(&mut ws, 10, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(why.contains("screen"), "{why}");

    // With consent, the write reaches the peer — whose own answer comes back.
    // The work folder is not a repository, so the peer's git says so.
    send(&mut ws, ClientMsg::SetPeerWrites { id: "c1".into(), allow: true }).await;
    send(&mut ws, ClientMsg::GitStage { session_id: MIRRORED.into(), repo: None, paths: vec!["a.txt".into()] }).await;
    let why = until(&mut ws, 10, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(!why.contains("allow changes"), "consent was given, so this is the peer's answer: {why}");

    // Unfollow takes the mirror away.
    send(&mut ws, ClientMsg::Unfollow { id: "c1".into() }).await;
    until(&mut ws, 10, |m| match m {
        ServerMsg::SessionRemoved { session_id } if session_id == MIRRORED => Some(()),
        _ => None,
    })
    .await;
    let _ = std::fs::remove_dir_all(peer_dir);
}

#[tokio::test]
async fn another_machine_never_sees_what_this_daemon_mirrors() {
    let (peer, peer_dir) = daemon("peer2", "peer-m");
    waiting_session(&peer, "s1", &peer_dir).await;
    let peer_url = serve(&peer, false).await;

    let (home_d, _dir) = daemon("home2", "home-m");
    let local_url = serve(&home_d, true).await;
    // The same daemon, on a listener that cannot see addresses: every client
    // of this one counts as another machine's.
    let foreign_url = serve(&home_d, false).await;

    let mut local = connect(&local_url).await;
    send(&mut local, follow(&peer_url, false)).await;
    until(&mut local, 10, |m| match m {
        ServerMsg::SessionUpdated { session } if session.id == MIRRORED => Some(()),
        _ => None,
    })
    .await;

    let mut foreign = connect(&foreign_url).await;
    let sessions = until(&mut foreign, 10, |m| match m {
        ServerMsg::Snapshot { sessions, .. } => Some(sessions.clone()),
        _ => None,
    })
    .await;
    assert!(sessions.iter().all(|s| !s.id.starts_with('@')), "a mirrored session was re-exported");

    // Nor may it list the follows — asked, it is told nothing, since every
    // window asks on connect — follow, or reach through.
    send(&mut foreign, ClientMsg::FetchPeers).await;
    send(&mut foreign, follow(&peer_url, false)).await;
    let why = until(&mut foreign, 10, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        ServerMsg::Peers { .. } => Some("LEAKED".into()),
        _ => None,
    })
    .await;
    assert!(why.contains("this machine only"), "{why}");
    send(&mut foreign, ClientMsg::FetchFile { session_id: MIRRORED.into(), path: "a.txt".into() }).await;
    let why = until(&mut foreign, 10, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        ServerMsg::FileContent { .. } => Some("LEAKED".into()),
        _ => None,
    })
    .await;
    assert!(why.contains("this machine's own windows"), "{why}");
    let _ = std::fs::remove_dir_all(peer_dir);
}

#[tokio::test]
async fn a_peer_gone_quiet_greys_out_and_leaves_the_top_of_the_queue() {
    let (peer, peer_dir) = daemon("peer3", "peer-m");
    waiting_session(&peer, "s1", &peer_dir).await;
    // On a runtime of its own, so stopping it closes every socket it holds —
    // aborting axum's serve task would only stop it accepting new ones.
    let (peer_url, stop) = serve_on_own_runtime(peer.clone());

    let (home_d, _dir) = daemon("home3", "home-m");
    home_d.federation.stale_after.set(Duration::ZERO).unwrap();
    idle_local(&home_d, "local-1").await;
    let home_url = serve(&home_d, true).await;
    let mut ws = connect(&home_url).await;
    send(&mut ws, follow(&peer_url, false)).await;
    until(&mut ws, 10, |m| match m {
        ServerMsg::SessionUpdated { session } if session.id == MIRRORED => Some(()),
        _ => None,
    })
    .await;

    // The peer goes away — its runtime, and every connection on it, with it.
    stop.send(()).unwrap();

    let stale = until(&mut ws, 20, |m| match m {
        ServerMsg::SessionUpdated { session } if session.id == MIRRORED && session.origin.as_ref().is_some_and(|o| o.stale) => {
            Some(session.clone())
        }
        _ => None,
    })
    .await;
    assert!(stale.origin.unwrap().stale);
    let order = until(&mut ws, 10, |m| match m {
        ServerMsg::Queue { queue } if queue.last().is_some_and(|i| i.session_id == MIRRORED) => {
            Some(queue.iter().map(|i| i.session_id.clone()).collect::<Vec<_>>())
        }
        _ => None,
    })
    .await;
    assert_eq!(order.last().map(String::as_str), Some(MIRRORED), "last known is not current: {order:?}");
    let _ = std::fs::remove_dir_all(peer_dir);
}

#[tokio::test]
async fn following_this_machine_is_recognised_and_refused() {
    let (home_d, _dir) = daemon("self", "home-m");
    let url = serve(&home_d, true).await;
    let foreign = serve(&home_d, false).await;
    let mut ws = connect(&url).await;
    send(&mut ws, follow(&foreign, false)).await;
    let detail = until(&mut ws, 10, |m| match m {
        ServerMsg::Peers { peers } => peers
            .iter()
            .find(|p| p.state == PeerStatus::Myself)
            .and_then(|p| p.detail.clone()),
        _ => None,
    })
    .await;
    assert!(detail.contains("this machine"), "{detail}");
}

/// Phase C (`R-I23`): consent opens the hop, and **the peer's own gates still
/// decide**. The follow's *allow changes* is this machine saying it may send a
/// change; whether that machine acts on it is that machine's call — here, a
/// peer bound beyond loopback without `--allow-run`, which starts nothing for
/// a socket from elsewhere, and a relay is one.
#[tokio::test]
async fn consent_opens_the_hop_and_the_peer_s_own_gates_still_decide() {
    let (peer, peer_dir) = daemon("peer5", "peer-m");
    peer.runs.set_allowed(false);
    waiting_session(&peer, "s1", &peer_dir).await;
    let peer_url = serve(&peer, false).await;

    let (home_d, _dir) = daemon("home5", "home-m");
    let home_url = serve(&home_d, true).await;
    let mut ws = connect(&home_url).await;
    send(&mut ws, follow(&peer_url, false)).await;
    until(&mut ws, 10, |m| match m {
        ServerMsg::SessionUpdated { session } if session.id == MIRRORED => Some(()),
        _ => None,
    })
    .await;

    // Without consent the run panel is not runnable here — and a window on
    // this machine is not shown *its own* authority over another machine's.
    assert!(!run_flag(&mut ws).await, "no consent, so no run button");

    // Consent given: the start crosses, and the peer refuses it in its words.
    send(&mut ws, ClientMsg::SetPeerWrites { id: "c1".into(), allow: true }).await;
    assert!(!run_flag(&mut ws).await, "the peer's own --allow-run is off, and the flag says so");
    send(&mut ws, ClientMsg::RunStart { session_id: MIRRORED.into(), config_id: "anything".into() }).await;
    let why = until(&mut ws, 10, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(why.contains("--allow-run"), "the peer's gate, in the peer's words: {why}");

    // Consent withdrawn: refused here again, before anything is sent.
    send(&mut ws, ClientMsg::SetPeerWrites { id: "c1".into(), allow: false }).await;
    send(&mut ws, ClientMsg::ForgetSession { session_id: MIRRORED.into() }).await;
    let why = until(&mut ws, 10, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(why.contains("allow changes"), "{why}");
    assert!(peer.sessions.read().await.contains_key("s1"), "nothing reached the peer");
    let _ = std::fs::remove_dir_all(peer_dir);
}

/// Whether the run panel for the mirrored session says it would run.
async fn run_flag(ws: &mut Ws) -> bool {
    send(ws, ClientMsg::FetchRunConfigs { session_id: MIRRORED.into() }).await;
    until(ws, 10, |m| match m {
        ServerMsg::RunConfigs { session_id, allowed, .. } if session_id == MIRRORED => Some(*allowed),
        _ => None,
    })
    .await
}

/// Phase D (`R-I24`): a peer on a build from before federation is refused a
/// follow, with the protocol it needs — not followed half-way and left to
/// fail on the first forwarded command.
#[tokio::test]
async fn a_peer_from_before_federation_is_refused_with_what_it_needs() {
    let (peer, peer_dir) = daemon("peer6", "peer-m");
    let peer_url = {
        // An older build: same wire, no `federation` in its identity.
        let mut old = peer;
        Arc::get_mut(&mut old).expect("nothing else holds it yet").identity.federation = None;
        serve(&old, false).await
    };
    let (home_d, _dir) = daemon("home6", "home-m");
    let mut ws = connect(&serve(&home_d, true).await).await;
    send(&mut ws, follow(&peer_url, false)).await;
    let (detail, sessions) = until(&mut ws, 10, |m| match m {
        ServerMsg::Peers { peers } => peers
            .iter()
            .find(|p| p.state == PeerStatus::Incompatible)
            .map(|p| (p.detail.clone().unwrap_or_default(), p.sessions)),
        _ => None,
    })
    .await;
    assert!(detail.contains("protocol"), "{detail}");
    assert_eq!(sessions, 0, "nothing mirrored from a peer that cannot be followed");
    let _ = std::fs::remove_dir_all(peer_dir);
}

/// A peer that goes quiet **without closing** — a laptop asleep, Wi-Fi in
/// power save — greys out a stale-window after it was *last heard*, not a
/// stale-window after the socket is finally declared dead. Measured from the
/// wrong end, the spec's *within a minute* was nearly two in practice (found
/// 2026-09-23 against a live pair: 61 s for a peer that closed cleanly, and
/// the half-open case adds the whole 45 s silence window on top).
#[tokio::test]
async fn a_peer_silent_on_an_open_socket_goes_stale_from_when_it_was_last_heard() {
    // A fake peer: one snapshot, then silence on a socket it never closes.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://127.0.0.1:{}/ws", listener.local_addr().unwrap().port());
    let (dir_peer, snapshot) = {
        let dir = home("peer7");
        let mut s = mogeungd::state::blank_session_with_id("s1");
        s.title = Some("asleep".into());
        s.alive = true;
        let identity = mogeung_core::wire::DaemonIdentity {
            machine_id: Some("peer-m".into()),
            host: Some("peer-m-host".into()),
            claude_home: "/nowhere".into(),
            pid: 1,
            version: "test".into(),
            ssh_target: None,
            federation: Some(mogeung_core::federation::PROTOCOL),
        };
        let msg = ServerMsg::Snapshot { sessions: vec![s], queue: vec![], daemon: Some(identity) };
        (dir, serde_json::to_string(&msg).unwrap())
    };
    let (held_tx, held_rx) = tokio::sync::oneshot::channel::<Ws2>();
    tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        ws.send(Message::Text(snapshot.into())).await.unwrap();
        // Held, never read: pings go unanswered, and nothing closes.
        let _ = held_tx.send(ws);
        // Later dials find nobody listening.
        drop(listener);
    });

    let (home_d, _dir) = daemon("home7", "home-m");
    home_d.federation.stale_after.set(Duration::from_secs(2)).unwrap();
    home_d.federation.silence.set(Duration::from_millis(900)).unwrap();
    let mut ws = connect(&serve(&home_d, true).await).await;
    send(&mut ws, follow(&url, false)).await;
    until(&mut ws, 10, |m| match m {
        ServerMsg::SessionUpdated { session } if session.id == MIRRORED => Some(()),
        _ => None,
    })
    .await;
    let heard = std::time::Instant::now();
    let _held = held_rx.await.unwrap();

    until(&mut ws, 10, |m| match m {
        ServerMsg::SessionUpdated { session } if session.id == MIRRORED && session.origin.as_ref().is_some_and(|o| o.stale) => {
            Some(())
        }
        _ => None,
    })
    .await;
    let after = heard.elapsed();
    // Two seconds after it was last heard, give or take a ping. Measured from
    // the end of the silence instead, and waiting out a redial backoff, this
    // came in past four.
    assert!(after < Duration::from_millis(3200), "stale {after:?} after it was last heard");
    let _ = std::fs::remove_dir_all(dir_peer);
}

type Ws2 = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;
