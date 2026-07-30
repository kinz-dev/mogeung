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
use anyhow::{anyhow, Result};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub struct Options {
    pub db: PathBuf,
    pub poll_ms: u64,
    pub notify: NotifyConfig,
    /// Root of the Claude Code state directory. `None` means the default.
    pub claude_home: Option<PathBuf>,
    /// Shared token required on every request when set — the `R-I4` remote
    /// bet (A24): token on a trusted network, no TLS until that bet fails.
    pub token: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            db: default_db(),
            poll_ms: 1500,
            notify: NotifyConfig::default(),
            claude_home: None,
            token: None,
        }
    }
}

/// What a bind address and a token add up to, decided before anything serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    /// Loopback. The operating system is the authentication — reaching the port
    /// means already being on this machine, as someone who can.
    Loopback,
    /// Beyond loopback, with a shared token required on every request.
    TokenGated,
}

/// Decide whether this bind is allowed to serve, before it serves anything.
///
/// Beyond loopback there is no operating system between the socket and the
/// network, so the token stops being an option and becomes the only thing left.
/// This used to be a warning. A warning is the wrong shape for it: it scrolls
/// past during start-up, it is indistinguishable from the other start-up lines,
/// and the failure it describes is silent and total — nobody notices an open
/// daemon, which is exactly why nobody fixes one. `R-I10`.
///
/// **There is deliberately no `--insecure` escape hatch.** An override that
/// exists is an override that becomes the documented workaround, and the safe
/// route — bind loopback, reach it over ssh — is one already in the guide and
/// strictly better than a token in clear text.
pub fn admit(addr: &SocketAddr, token: Option<&str>) -> Result<Posture> {
    if addr.ip().is_loopback() {
        return Ok(Posture::Loopback);
    }
    if token.is_some_and(|t| !t.is_empty()) {
        return Ok(Posture::TokenGated);
    }
    // `\x20` rather than a plain space: a `\` line continuation eats the
    // leading whitespace of the next source line, so indentation written as
    // spaces would silently vanish from the message the user actually reads.
    Err(anyhow!(
        "refusing to listen on {addr} with no token.\n\
         \n\
         Anyone who could reach that address would be able to read every Claude Code\n\
         transcript on this machine and open terminals on it. There is no --insecure\n\
         flag; pick one of these instead:\n\
         \n\
         \x20 require a shared secret\n\
         \x20   --token \"$(openssl rand -hex 24)\"\n\
         \n\
         \x20 or stay local, and reach it over ssh\n\
         \x20   --listen 127.0.0.1:{port}\n\
         \x20   ssh -N -L {port}:localhost:{port} <this-host>\n\
         \n\
         The ssh route needs no token and puts nothing in clear text.\n\
         See docs/guide/remote.md.",
        port = addr.port(),
    ))
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
    // Before the database is opened and before a single line is scanned: a
    // refusal that has already done work is a refusal that leaves a mess.
    let posture = admit(&addr, opts.token.as_deref())?;
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
    if posture == Posture::TokenGated {
        tracing::warn!(
            "listening beyond localhost with a shared token and NO TLS — the token \
             and everything after it travel in clear text; trusted networks only (A24)"
        );
    }

    axum::serve(listener, api::router_with_token(state, opts.token.clone()))
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> SocketAddr {
        s.parse().expect("test address")
    }

    /// The ordinary case, and the one that must never start demanding a token:
    /// almost every run of this tool is a window hosting a daemon on loopback.
    #[test]
    fn loopback_needs_no_token() {
        assert_eq!(admit(&at("127.0.0.1:7717"), None).unwrap(), Posture::Loopback);
        assert_eq!(admit(&at("[::1]:7717"), None).unwrap(), Posture::Loopback);
    }

    /// The whole point of `R-I10`: this used to log a warning and serve anyway.
    #[test]
    fn a_public_bind_without_a_token_is_refused() {
        for addr in ["0.0.0.0:7717", "192.168.1.5:7717", "[::]:7717"] {
            assert!(
                admit(&at(addr), None).is_err(),
                "{addr} must not serve unauthenticated"
            );
        }
    }

    /// `0.0.0.0` reaches loopback too, but it also reaches the network, and it
    /// is the network that decides the posture.
    #[test]
    fn binding_everything_counts_as_public_not_local() {
        assert!(admit(&at("0.0.0.0:7717"), None).is_err());
    }

    #[test]
    fn a_public_bind_with_a_token_is_gated() {
        assert_eq!(
            admit(&at("0.0.0.0:7717"), Some("s3cret")).unwrap(),
            Posture::TokenGated
        );
    }

    /// `--token ""` is how a shell hands over an unset variable, and it must not
    /// read as consent. The request gate already treats it as absent
    /// (`router_with_token` filters empties), so admission must agree — the two
    /// disagreeing would mean a daemon that starts believing it is gated and
    /// then answers everyone.
    #[test]
    fn an_empty_token_is_not_a_token() {
        assert!(admit(&at("0.0.0.0:7717"), Some("")).is_err());
    }

    /// A refusal that does not say what to do instead gets worked around by
    /// whatever the internet suggests first.
    #[test]
    fn the_refusal_names_both_ways_out() {
        let err = admit(&at("0.0.0.0:7717"), None).unwrap_err().to_string();
        assert!(err.contains("--token"), "must offer the token: {err}");
        assert!(err.contains("ssh -N -L"), "must offer the tunnel: {err}");
        assert!(err.contains("127.0.0.1:7717"), "must keep the port: {err}");
    }
}
