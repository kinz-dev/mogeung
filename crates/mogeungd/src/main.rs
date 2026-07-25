//! mogeungd — the mogeung daemon.
//!
//! Watches the Claude Code sessions you run yourself, ranks them by who needs
//! you, diffs what they changed, and remembers what you have already read. It
//! never starts, steers or stops an agent.
//!
//! The serving logic lives in `mogeungd::server` so the window can host a
//! daemon in-process when none is running — see
//! [ADR-0009](../../../docs/decisions/0009-the-window-may-host-a-daemon.md).
//! This binary remains the way to run one that outlives any window.

use anyhow::{Context, Result};
use clap::Parser;
use mogeungd::notify::NotifyConfig;
use mogeungd::server::{self, Options};
use std::path::PathBuf;

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

    /// Post a macOS banner when a session starts needing you (R-C1).
    ///
    /// Off by default: a tool that starts posting notifications the first time
    /// you run it has overstepped.
    #[arg(long)]
    notify: bool,

    /// POST notifications to a URL as well — ntfy.sh, Pushover, a webhook (R-C4).
    ///
    /// The body is the message. Use with `--listen 0.0.0.0:7717` and the web
    /// client at `/` to triage away from the desk.
    #[arg(long, value_name = "URL")]
    push_url: Option<String>,
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
    let listener = std::net::TcpListener::bind(&args.listen)
        .with_context(|| format!("could not bind {} — is a daemon already running?", args.listen))?;

    server::run(
        listener,
        Options {
            db: args.db.unwrap_or_else(server::default_db),
            poll_ms: args.poll_ms,
            notify: NotifyConfig {
                desktop: args.notify,
                push_url: args.push_url,
            },
            claude_home: None,
        },
        async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        },
    )
    .await
}
