//! Running the daemon, shared by the `mogeungd` binary and the window.
//!
//! Extracted so the window can host a daemon **in its own process** when none
//! is running, rather than shelling out to a second executable. See
//! [ADR-0009](../../../docs/decisions/0009-the-window-may-host-a-daemon.md).
//!
//! `run` takes an already-bound listener rather than an address. That is what
//! makes start-up race-free: whoever wins the bind is the daemon, and a loser
//! knows to attach instead. Binding, checking and then binding again would have
//! a window in the middle where two clients both decide to start one.

use crate::notify::NotifyConfig;
use crate::state::AppState;
use crate::{api, store, watcher};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct Options {
    pub db: PathBuf,
    pub poll_ms: u64,
    pub notify: NotifyConfig,
    /// Root of the Claude Code state directory. `None` means the default.
    pub claude_home: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            db: default_db(),
            poll_ms: 1500,
            notify: NotifyConfig::default(),
            claude_home: None,
        }
    }
}

pub fn default_db() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("mogeung.db")
}

/// Build the state and take one scan, so a client connecting immediately gets a
/// populated board rather than an empty one.
pub async fn prepare(opts: &Options) -> Result<Arc<AppState>> {
    let store = store::Store::open(&opts.db)?;
    let home = opts
        .claude_home
        .clone()
        .unwrap_or_else(watcher::default_home);
    let state = AppState::with_home(store, home.clone())?;

    if opts.notify.enabled() {
        state.configure_notifications(opts.notify.clone()).await;
        tracing::info!(
            "notifications on (desktop: {}, push: {})",
            opts.notify.desktop,
            opts.notify.push_url.is_some()
        );
    }

    if !home.join("projects").exists() {
        tracing::warn!(
            "no session transcripts at {} — is Claude Code installed for this user?",
            home.join("projects").display()
        );
    }

    state.scan().await;
    tracing::info!(
        "watching {} — {} session(s) known",
        home.display(),
        state.sessions.read().await.len()
    );
    Ok(state)
}

/// Serve on `listener` until the future returned by `shutdown` resolves.
///
/// The listener arrives already bound so that the caller can use the bind
/// itself as the "am I the daemon?" test.
pub async fn run<F>(listener: std::net::TcpListener, opts: Options, shutdown: F) -> Result<()>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let addr = listener.local_addr()?;
    let state = prepare(&opts).await?;

    {
        let s = state.clone();
        let period = Duration::from_millis(opts.poll_ms.max(250));
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(period);
            loop {
                tick.tick().await;
                s.scan().await;
            }
        });
    }

    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;

    tracing::info!("mogeungd listening on http://{addr} (db {})", opts.db.display());
    tracing::info!("web client at http://{addr}/");
    if !addr.ip().is_loopback() {
        // Worth shouting about: there is no authentication, by design and by
        // omission. See docs/design/wire-protocol.md.
        tracing::warn!(
            "listening beyond localhost with NO AUTHENTICATION — anyone who can \
             reach {addr} can read your transcripts and open terminals on this machine"
        );
    }

    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}
