//! mogeungd — the mogeung daemon.
//!
//! Watches the Claude Code sessions you run yourself, ranks them by who needs
//! you, diffs what they changed, and remembers what you have already read. It
//! never starts, steers or stops an agent.

use anyhow::Result;
use clap::Parser;
use mogeungd::state::AppState;
use mogeungd::{api, store, watcher};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "mogeungd", version, about = "mogeung daemon — watches Claude Code sessions")]
struct Args {
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:7717")]
    listen: String,

    /// Database location. Defaults to ~/.mogeung/mogeung.db
    #[arg(long)]
    db: Option<PathBuf>,

    /// How often to poll for session changes, in milliseconds.
    #[arg(long, default_value_t = 1500)]
    poll_ms: u64,
}

fn default_db() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("mogeung.db")
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mogeungd=info".into()),
        )
        .init();

    let args = Args::parse();
    let db = args.db.unwrap_or_else(default_db);
    let store = store::Store::open(&db)?;
    let state = AppState::new(store)?;

    let home = watcher::default_home();
    if !home.join("projects").exists() {
        tracing::warn!(
            "no session transcripts at {} — is Claude Code installed for this user?",
            home.join("projects").display()
        );
    }

    // First pass before serving, so a client connecting immediately gets a
    // populated snapshot rather than an empty board.
    state.scan().await;
    tracing::info!(
        "watching {} — {} session(s) known",
        home.display(),
        state.sessions.read().await.len()
    );

    {
        let s = state.clone();
        let period = Duration::from_millis(args.poll_ms.max(250));
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(period);
            loop {
                tick.tick().await;
                s.scan().await;
            }
        });
    }

    let app = api::router(state.clone());
    let listener = tokio::net::TcpListener::bind(&args.listen).await?;
    tracing::info!(
        "mogeungd listening on http://{} (db {})",
        args.listen,
        db.display()
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await?;
    Ok(())
}
