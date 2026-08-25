//! End-to-end tests over the real WebSocket API.
//!
//! Everything here is free — the observer model means no test ever spawns an
//! agent.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::{ClientMsg, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

struct Harness {
    url: String,
    _dir: PathBuf,
}

async fn boot(name: &str) -> Harness {
    let dir = std::env::temp_dir().join(format!("mogeung-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let store = Store::open(&dir.join("test.db")).unwrap();
    // An empty claude home of the test's own. `AppState::new` would watch the
    // real `~/.claude`, which made these tests a function of the developer's
    // machine — a 139 MB history turns Rescan's first scan into a timeout.
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let app = api::router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    Harness {
        url: format!("ws://127.0.0.1:{port}/ws"),
        _dir: dir,
    }
}

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(ws: &mut Ws, msg: ClientMsg) {
    ws.send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .unwrap();
}

async fn wait_for<T>(
    ws: &mut Ws,
    secs: u64,
    mut pred: impl FnMut(&ServerMsg) -> Option<T>,
) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                if let Ok(msg) = serde_json::from_str::<ServerMsg>(&t) {
                    if let Some(v) = pred(&msg) {
                        return Some(v);
                    }
                }
            }
            Ok(Some(Ok(_))) => {}
            _ => return None,
        }
    }
    None
}

#[tokio::test]
async fn snapshot_arrives_unsolicited_on_connect() {
    let h = boot("snapshot").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();

    let got = wait_for(&mut ws, 5, |m| match m {
        ServerMsg::Snapshot { sessions, queue, .. } => Some((sessions.len(), queue.len())),
        _ => None,
    })
    .await;
    assert!(got.is_some(), "no snapshot on connect");
}

#[tokio::test]
async fn a_malformed_command_is_reported_without_dropping_the_socket() {
    let h = boot("badcmd").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    ws.send(Message::Text("{\"cmd\":\"nonsense\"}".to_string().into()))
        .await
        .unwrap();
    let err = wait_for(&mut ws, 5, |m| match m {
        ServerMsg::Error { message } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(err.is_some(), "malformed command should be reported");

    // ...and the socket still works afterwards.
    send(&mut ws, ClientMsg::Subscribe).await;
    let alive = wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await;
    assert!(alive.is_some(), "connection died after a bad command");
}

#[tokio::test]
async fn commands_about_unknown_sessions_are_harmless() {
    let h = boot("unknown").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    let ghost = "00000000-0000-0000-0000-000000000000".to_string();
    send(
        &mut ws,
        ClientMsg::FetchEvents {
            session_id: ghost.clone(),
            since: 0,
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::RefreshChange {
            session_id: ghost.clone(),
            force: true,
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::ReviewAll {
            session_id: ghost.clone(),
        },
    )
    .await;
    // The whole R-D11 git family: every one must answer an unknown session
    // (and a hostile argument) with an error event, never a wedged socket.
    send(
        &mut ws,
        ClientMsg::GitRefs {
            session_id: ghost.clone(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitStashes {
            session_id: ghost.clone(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitStashShow {
            session_id: ghost.clone(),
            index: 9999,
            context: Some(u32::MAX),
            ignore_ws: Some(true),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitSubmodules {
            session_id: ghost.clone(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitDiffRange {
            session_id: ghost.clone(),
            from: "--output=/tmp/pwned".into(),
            to: "also not a sha".into(),
            context: None,
            ignore_ws: None,
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitFileAtRev {
            session_id: ghost.clone(),
            sha: "-x".into(),
            path: "../escape".into(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitLog {
            session_id: ghost.clone(),
            skip: 0,
            limit: 50,
            rev: Some("--all".into()),
            grep: Some("a\x1b]0;pwned\x07".into()),
            author: Some("\n--exec=rm".into()),
            path: Some("../outside".into()),
            pickaxe: Some("\x00".into()),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitCompare {
            session_id: ghost.clone(),
            branch: "-D".into(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitReflog {
            session_id: ghost.clone(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitWorktrees {
            session_id: ghost.clone(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitConflictFile {
            session_id: ghost.clone(),
            path: "/etc/passwd".into(),
        },
    )
    .await;
    send(
        &mut ws,
        ClientMsg::GitBlame {
            session_id: ghost.clone(),
            path: "x.rs".into(),
            rev: Some("HEAD^".into()),
        },
    )
    .await;

    send(&mut ws, ClientMsg::Subscribe).await;
    let alive = wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await;
    assert!(alive.is_some(), "unknown session id killed the connection");
}

#[tokio::test]
async fn rescan_is_safe_to_request_over_the_wire() {
    let h = boot("rescan").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    send(&mut ws, ClientMsg::Rescan).await;
    let queued = wait_for(&mut ws, 10, |m| {
        matches!(m, ServerMsg::Queue { .. }).then_some(())
    })
    .await;
    assert!(queued.is_some(), "rescan produced no queue update");
}

/// Notes, end to end over the wire — `R-B35`, pillar L.
///
/// The properties ADR-0015 chose daemon ownership for: one set of notes
/// whatever is connected, and writing that outlives the process holding it.
#[tokio::test]
async fn a_note_reaches_every_client_and_lands_on_disk() {
    let h = boot("notes").await;
    let (mut a, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    let (mut b, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();

    send(
        &mut a,
        ClientMsg::NoteSave {
            id: String::new(),
            body: "the agent's claim about the cache is wrong".into(),
            session_id: Some("sess-1".into()),
            seq: Some(12),
            repo: None,
        },
    )
    .await;

    // Both clients see it: the answer is a broadcast of the whole set rather
    // than a reply to whoever asked.
    let mut id = String::new();
    for ws in [&mut a, &mut b] {
        let notes = wait_for(ws, 5, |m| match m {
            ServerMsg::Notes { notes } if !notes.is_empty() => Some(notes.clone()),
            _ => None,
        })
        .await
        .expect("every connected client is told");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].session_id.as_deref(), Some("sess-1"));
        assert_eq!(notes[0].seq, Some(12));
        assert!(notes[0].body.contains("cache is wrong"));
        assert!(!notes[0].id.is_empty(), "the daemon minted an id");
        id = notes[0].id.clone();
    }

    // Editing keeps the id rather than making a second note.
    send(
        &mut a,
        ClientMsg::NoteSave {
            id: id.clone(),
            body: "checked it — the claim is right after all".into(),
            session_id: Some("sess-1".into()),
            seq: Some(12),
            repo: None,
        },
    )
    .await;
    let notes = wait_for(&mut a, 5, |m| match m {
        ServerMsg::Notes { notes } => {
            notes.first().filter(|n| n.body.contains("after all")).map(|_| notes.clone())
        }
        _ => None,
    })
    .await
    .expect("the edit came back");
    assert_eq!(notes.len(), 1, "edited, not duplicated");
    assert_eq!(notes[0].id, id);

    // The mirror holds the writing, which is the whole mitigation for keeping
    // this in a database at all (ADR-0015).
    let mirror = mogeungd::notes::mirror_dir();
    let named = |id: &str| -> Vec<String> {
        std::fs::read_dir(mirror.clone())
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| n.starts_with(&format!("{id}-")))
                    .collect()
            })
            .unwrap_or_default()
    };
    let found = named(&id);
    assert_eq!(found.len(), 1, "exactly one mirror file: {found:?}");
    let text = std::fs::read_to_string(mirror.join(&found[0])).unwrap();
    assert!(text.contains("right after all"), "{text}");

    send(&mut a, ClientMsg::NoteDelete { id: id.clone() }).await;
    let gone = wait_for(&mut a, 5, |m| match m {
        ServerMsg::Notes { notes } if notes.is_empty() => Some(()),
        _ => None,
    })
    .await;
    assert!(gone.is_some(), "the delete came back");
    assert!(named(&id).is_empty(), "the mirror went with it");
}
