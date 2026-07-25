//! End-to-end tests over the real WebSocket API.
//!
//! `protocol_round_trip` is free and runs on every `cargo test`.
//! `real_agent_run` spawns an actual Claude Code session, so it costs money and
//! is `#[ignore]`d — run it deliberately with:
//!
//! ```text
//! cargo test -p mogeungd -- --ignored --nocapture
//! ```

use futures_util::{SinkExt, StreamExt};
use mogeung_core::{ClientMsg, NewRunSpec, PermissionMode, RunStatus, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

struct Harness {
    url: String,
    _dir: PathBuf,
}

/// Boot a daemon on an ephemeral port with a scratch database.
async fn boot(name: &str) -> Harness {
    let dir = std::env::temp_dir().join(format!("mogeung-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let store = Store::open(&dir.join("test.db")).unwrap();
    let state = AppState::new(store).unwrap();
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

/// A throwaway git repo with one commit.
fn make_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t.t"]);
    git(&["config", "user.name", "t"]);
    std::fs::write(repo.join("main.rs"), "fn main() {}\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    repo
}

type Ws = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn send(ws: &mut Ws, msg: ClientMsg) {
    ws.send(Message::Text(serde_json::to_string(&msg).unwrap().into()))
        .await
        .unwrap();
}

/// Read messages until `pred` matches one, or the deadline passes.
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
async fn protocol_round_trip() {
    let h = boot("proto").await;
    let repo = make_repo(&h._dir);
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();

    // A client is useful before it sends anything: the snapshot is unsolicited.
    let empty = wait_for(&mut ws, 5, |m| match m {
        ServerMsg::Snapshot { runs, .. } => Some(runs.len()),
        _ => None,
    })
    .await
    .expect("no snapshot on connect");
    assert_eq!(empty, 0);

    send(
        &mut ws,
        ClientMsg::AddRepo {
            path: repo.to_string_lossy().to_string(),
        },
    )
    .await;
    let repos = wait_for(&mut ws, 5, |m| match m {
        ServerMsg::RepoList { repos } => Some(repos.clone()),
        _ => None,
    })
    .await
    .expect("no repo list");
    assert_eq!(repos.len(), 1);

    // A bad command must produce an error, not drop the connection.
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
    let alive = wait_for(&mut ws, 5, |m| matches!(m, ServerMsg::Snapshot { .. }).then_some(()))
        .await;
    assert!(alive.is_some(), "connection died after a bad command");
}

#[tokio::test]
#[ignore = "spawns a real Claude Code session and costs money"]
async fn real_agent_run() {
    let h = boot("agent").await;
    let repo = make_repo(&h._dir);
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    send(
        &mut ws,
        ClientMsg::StartRun(NewRunSpec {
            repo_path: repo.to_string_lossy().to_string(),
            intent: "Create a file called hello.txt containing exactly: hi. Nothing else."
                .to_string(),
            model: Some("sonnet".into()),
            permission_mode: PermissionMode::AcceptEdits,
            worktree: true,
        }),
    )
    .await;

    let run_id = wait_for(&mut ws, 20, |m| match m {
        ServerMsg::RunUpdated { run } => Some(run.id),
        _ => None,
    })
    .await
    .expect("run was never created");

    // Wait for it to reach a terminal state.
    let status = wait_for(&mut ws, 240, |m| match m {
        ServerMsg::RunUpdated { run } if run.id == run_id && run.status.is_terminal() => {
            Some(run.status)
        }
        _ => None,
    })
    .await
    .expect("run never finished");
    assert!(
        matches!(status, RunStatus::AwaitingReview | RunStatus::Reviewed),
        "run ended as {status:?}"
    );

    // The diff must include the untracked file the agent created — plain
    // `git diff` would miss it entirely.
    send(&mut ws, ClientMsg::RefreshChange { run_id }).await;
    let change = wait_for(&mut ws, 30, |m| match m {
        ServerMsg::ChangeUpdated { run_id: r, change } if *r == run_id => Some(change.clone()),
        _ => None,
    })
    .await
    .expect("no change computed");
    assert!(
        change.files.iter().any(|f| f.path.contains("hello.txt")),
        "new untracked file missing from diff: {:?}",
        change.files.iter().map(|f| &f.path).collect::<Vec<_>>()
    );

    // Marking everything read must empty the unreviewed count.
    send(&mut ws, ClientMsg::ReviewAll { run_id }).await;
    let cleared = wait_for(&mut ws, 30, |m| match m {
        ServerMsg::ChangeUpdated { run_id: r, change } if *r == run_id => {
            (change.unreviewed_hunks() == 0).then_some(())
        }
        _ => None,
    })
    .await;
    assert!(cleared.is_some(), "review-all did not clear the hunks");
}

/// Guards the promise that a client can rebuild transcript history over the
/// same socket it uses for everything else.
#[tokio::test]
async fn fetch_events_is_safe_on_unknown_runs() {
    let h = boot("events").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&h.url).await.unwrap();
    wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await
    .unwrap();

    send(
        &mut ws,
        ClientMsg::FetchEvents {
            run_id: mogeung_core::RunId::new(),
            since: 0,
        },
    )
    .await;
    send(&mut ws, ClientMsg::Subscribe).await;
    let alive = wait_for(&mut ws, 5, |m| {
        matches!(m, ServerMsg::Snapshot { .. }).then_some(())
    })
    .await;
    assert!(alive.is_some(), "unknown run id killed the connection");
}
