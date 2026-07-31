//! R-I4: the shared-token gate. A wrong token is a clean 401, never a hang;
//! no token configured keeps the historical open behaviour.

use mogeungd::{api, state::AppState, store::Store};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

fn temp_home(tag: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("mogeung-auth-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("projects")).unwrap();
    home
}

async fn serve(tag: &str, token: Option<&str>) -> std::net::SocketAddr {
    let home = temp_home(tag);
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    let router = api::router_with_token(state as Arc<AppState>, token.map(String::from));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    addr
}

/// One blocking HTTP GET, returning the status line.
fn get(addr: std::net::SocketAddr, path_and_query: &str, header: Option<&str>) -> String {
    let mut s = std::net::TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
    let extra = header.map(|h| format!("{h}\r\n")).unwrap_or_default();
    write!(
        s,
        "GET {path_and_query} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n{extra}\r\n"
    )
    .unwrap();
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    buf.lines().next().unwrap_or_default().to_string()
}

#[tokio::test]
async fn a_wrong_or_missing_token_is_a_clean_401() {
    let addr = serve("gate", Some("s3cret")).await;
    let addr2 = addr;
    let (none, wrong, header_ok, query_ok) = tokio::task::spawn_blocking(move || {
        (
            get(addr2, "/api/health", None),
            get(addr2, "/api/health?token=nope", None),
            get(addr2, "/api/health", Some("Authorization: Bearer s3cret")),
            get(addr2, "/api/health?token=s3cret", None),
        )
    })
    .await
    .unwrap();
    assert!(none.contains("401"), "no token must 401, got: {none}");
    assert!(wrong.contains("401"), "wrong token must 401, got: {wrong}");
    assert!(header_ok.contains("200"), "bearer token must pass, got: {header_ok}");
    assert!(query_ok.contains("200"), "query token must pass, got: {query_ok}");
}

#[tokio::test]
async fn no_token_configured_stays_open() {
    let addr = serve("open", None).await;
    let line = tokio::task::spawn_blocking(move || get(addr, "/api/health", None))
        .await
        .unwrap();
    assert!(line.contains("200"), "got: {line}");
}

/// R-I10: a bind beyond loopback with no token does not serve at all.
///
/// It used to log a warning and carry on, which meant an open daemon looked
/// exactly like a working one. Asserted through `server::run` rather than
/// `admit` directly, because the guarantee is about the daemon refusing to
/// start — a unit test of the predicate would still pass if nobody called it.
#[tokio::test]
async fn a_public_bind_with_no_token_refuses_to_serve() {
    let home = temp_home("refuse");
    let db = home.join("never-created.db");
    let listener = std::net::TcpListener::bind("0.0.0.0:0").unwrap();

    let opts = mogeungd::server::Options {
        db: db.clone(),
        claude_home: Some(home),
        token: None,
        ..Default::default()
    };
    let err = mogeungd::server::run(listener, opts, std::future::pending::<()>())
        .await
        .expect_err("a public bind with no token must not serve");

    let msg = err.to_string();
    assert!(msg.contains("refusing to listen"), "got: {msg}");
    assert!(msg.contains("--token"), "must say how to fix it: {msg}");

    // The refusal comes before any work: no database, no scan, nothing to
    // clean up after a start that should never have begun.
    assert!(
        !db.exists(),
        "refused before serving, so the database must not have been created"
    );
}

/// The same bind, with a token, gets past admission and serves.
#[tokio::test]
async fn a_public_bind_with_a_token_is_allowed() {
    let home = temp_home("allowed");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store = Store::open(&home.join("mogeung.db")).unwrap();
    let state = AppState::with_home(store, home).unwrap();
    let router = api::router_with_token(state as Arc<AppState>, Some("s3cret".into()));
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });

    let reachable = tokio::task::spawn_blocking(move || {
        get(
            std::net::SocketAddr::from(([127, 0, 0, 1], addr.port())),
            "/api/health?token=s3cret",
            None,
        )
    })
    .await
    .unwrap();
    assert!(reachable.contains("200"), "got: {reachable}");
}
