//! mogeungd — the mogeung daemon.
//!
//! Owns repositories, agent processes, transcripts, diffs and review state.
//! Every UI is a client of this process; the daemon keeps working with no
//! window open (CONCEPT.md §7).

use anyhow::Result;
use clap::Parser;
use mogeungd::state::AppState;
use mogeungd::{api, store};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "mogeungd", version, about = "mogeung daemon")]
struct Args {
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:7717")]
    listen: String,

    /// Database location. Defaults to ~/.mogeung/mogeung.db
    #[arg(long)]
    db: Option<PathBuf>,

    /// Repositories to register at startup (repeatable).
    #[arg(long = "repo")]
    repos: Vec<String>,
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

    for r in &args.repos {
        if let Err(e) = state.add_repo(r).await {
            tracing::warn!("could not register {r}: {e}");
        }
    }

    // Stall detection is time-based, so the queue has to be recomputed even when
    // nothing is happening. That is the whole point: silence is itself a signal.
    {
        let s = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                s.publish_queue().await;
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
