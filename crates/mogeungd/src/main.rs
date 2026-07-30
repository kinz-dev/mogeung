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

/// Loopback, deliberately: a daemon reachable from the network is something
/// you ask for (`--listen 0.0.0.0:…`), never something you get by default.
const DEFAULT_LISTEN: &str = "127.0.0.1:7717";
const DEFAULT_POLL_MS: u64 = 1500;

#[derive(Parser)]
#[command(name = "mogeungd", version, about = "mogeung daemon — watches Claude Code sessions")]
struct Args {
    /// Address to listen on. Default 127.0.0.1:7717, or `listen` in
    /// ~/.mogeung/config.toml.
    ///
    /// No clap default: a default is indistinguishable from a value you typed,
    /// and the config file has to lose to one and beat the other (`R-J3`).
    #[arg(long)]
    listen: Option<String>,

    /// Database location. Defaults to ~/.mogeung/mogeung.db
    #[arg(long)]
    db: Option<PathBuf>,

    /// How often to poll for session changes, in milliseconds.
    #[arg(long)]
    poll_ms: Option<u64>,

    /// Post a macOS banner when a session starts needing you (R-C1).
    ///
    /// Off by default: a tool that starts posting notifications the first time
    /// you run it has overstepped.
    #[arg(long)]
    notify: bool,

    /// POST notifications to a URL as well — ntfy.sh, Pushover, a webhook (R-C4).
    ///
    /// The body is the message — how you hear about a session while away from
    /// the desk, now that acting on it means reaching the window.
    #[arg(long, value_name = "URL")]
    push_url: Option<String>,

    /// Require this token on every request (R-I4). For non-loopback listens:
    /// clients send `Authorization: Bearer …` or `?token=…`. No TLS — the
    /// token travels in clear text, so trusted networks only.
    #[arg(long)]
    token: Option<String>,

    /// How to reach this machine over ssh — `user@host`, or a name from
    /// `~/.ssh/config` (R-I5). Published in the daemon's identity so a client
    /// watching from elsewhere knows where "here" is. The daemon never uses it.
    #[arg(long, value_name = "USER@HOST")]
    ssh_target: Option<String>,
}

/// Command line over file over default, resolved in one place so the order can
/// be tested rather than trusted. `R-J3`.
fn resolve(args: Args, cfg: mogeung_core::config::Config) -> (String, Options) {
    (
        args.listen
            .or(cfg.listen)
            .unwrap_or_else(|| DEFAULT_LISTEN.to_string()),
        Options {
            db: args.db.or(cfg.db).unwrap_or_else(server::default_db),
            poll_ms: args.poll_ms.or(cfg.poll_ms).unwrap_or(DEFAULT_POLL_MS),
            notify: NotifyConfig {
                // A flag can only turn this on: there is no --no-notify to
                // contradict, so `||` is the whole rule. Turning it off is the
                // file's job, or not passing the flag.
                desktop: args.notify || cfg.notify.unwrap_or(false),
                push_url: args.push_url.or(cfg.push_url),
            },
            claude_home: None,
            token: args.token.or(cfg.token),
            ssh_target: args.ssh_target.or(cfg.ssh_target),
        },
    )
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

    // Flags beat the file, the file beats the defaults, and a broken file
    // costs you your settings rather than the daemon (`R-J3`).
    let (cfg, cfg_warning) = mogeung_core::config::Config::load();
    if let Some(w) = cfg_warning {
        tracing::warn!("{w}");
    }
    let (listen, options) = resolve(args, cfg);

    let listener = std::net::TcpListener::bind(&listen)
        .with_context(|| format!("could not bind {listen} — is a daemon already running?"))?;

    server::run(
        listener,
        options,
        async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::config::Config;

    fn args() -> Args {
        Args {
            listen: None,
            db: None,
            poll_ms: None,
            notify: false,
            push_url: None,
            token: None,
            ssh_target: None,
        }
    }

    /// The order that makes a config file safe to keep: what you typed wins,
    /// what the file says fills the gaps, and the built-in answer is the floor.
    #[test]
    fn a_flag_beats_the_file_and_the_file_beats_the_default() {
        let cfg = Config {
            listen: Some("0.0.0.0:7000".into()),
            poll_ms: Some(400),
            token: Some("from-file".into()),
            ..Config::default()
        };

        let (listen, o) = resolve(args(), cfg.clone());
        assert_eq!(listen, "0.0.0.0:7000", "the file fills an unset flag");
        assert_eq!(o.poll_ms, 400);
        assert_eq!(o.token.as_deref(), Some("from-file"));

        let typed = Args {
            listen: Some("127.0.0.1:9999".into()),
            poll_ms: Some(50),
            token: Some("typed".into()),
            ..args()
        };
        let (listen, o) = resolve(typed, cfg);
        assert_eq!(listen, "127.0.0.1:9999", "a flag must win");
        assert_eq!(o.poll_ms, 50);
        assert_eq!(o.token.as_deref(), Some("typed"));

        // And with neither, the defaults — including loopback, which is a
        // safety property rather than a preference.
        let (listen, o) = resolve(args(), Config::default());
        assert_eq!(listen, DEFAULT_LISTEN);
        assert_eq!(o.poll_ms, DEFAULT_POLL_MS);
        assert!(o.token.is_none());
    }

    /// Notifications are opt-in from either side, and neither side can turn
    /// the other off — there is no flag that means "no", so a file that says
    /// yes is the only way this becomes true without one.
    #[test]
    fn notifications_turn_on_from_the_file_or_the_flag() {
        let on_in_file = Config { notify: Some(true), ..Config::default() };
        assert!(resolve(args(), on_in_file.clone()).1.notify.desktop);
        assert!(resolve(Args { notify: true, ..args() }, Config::default()).1.notify.desktop);
        assert!(!resolve(args(), Config::default()).1.notify.desktop);
    }
}
