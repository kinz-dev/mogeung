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

    /// Announce this daemon on the local network over mDNS (R-I8).
    ///
    /// Off unless asked for, and deliberately so: the broadcast tells every
    /// machine on the segment that this one is watching Claude Code sessions
    /// and where to reach it. Requires a non-loopback --listen, which in turn
    /// requires --token.
    #[arg(long)]
    advertise: bool,

    /// Let this daemon start processes when bound anywhere but loopback.
    ///
    /// ADR-0025 clause 4. A run verb means anyone who can reach the port can
    /// cause code to execute on this machine, and "the code was already checked
    /// in" is a mitigation rather than an answer. On loopback — the same trust
    /// boundary as the terminal panel, which can already run anything — runs
    /// are allowed without this. Anywhere else needs this **and** the token
    /// `R-I10` already demands: two deliberate acts, not one.
    #[arg(long)]
    allow_run: bool,

    /// Base URL of an OpenAI-compatible model API (`R-O1`), e.g.
    /// http://127.0.0.1:8000/v1 — or `model_url` in ~/.mogeung/config.toml.
    ///
    /// The `…/models` URL is accepted and trimmed, because that is the one you
    /// can curl and therefore the one that gets pasted.
    #[arg(long, value_name = "URL")]
    model_url: Option<String>,

    /// Which model to ask for, as the endpoint's own /models lists it.
    /// Absent means the endpoint's default.
    #[arg(long, value_name = "NAME")]
    model_name: Option<String>,

    /// Let this daemon send text to a model endpoint that is **not** on this
    /// machine. ADR-0031 clause 3.
    ///
    /// A model endpoint elsewhere is publishing: what mogeung asks it travels
    /// off this box, and ADR-0014 draws the line at publishing rather than at
    /// the network. Loopback needs none of this.
    ///
    /// The flag is the **blanket** grant — any host, for this run. The
    /// narrower and preferred form is `allow_remote_model = "spark-7ecc"` in
    /// the config file, which consents to one named host and asks again when
    /// `model_url` moves. Both exist because a window hosting its own daemon
    /// has no argv (ADR-0009) and would otherwise have no way to consent at
    /// all.
    #[arg(long)]
    allow_remote_model: bool,
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
            advertise: args.advertise || cfg.advertise.unwrap_or(false),
            allow_run: args.allow_run,
            // File only, and absent means yes. No flag: this is a standing
            // preference about what is kept on disk, not a property of one
            // invocation, and a run that quietly stopped recording would be
            // the confusing half of the pair. `R-O9`.
            chat_history: cfg.chat_history.unwrap_or(true),
            // File only, and off unless asked for. No flag: this starts a
            // long-lived child process, which is a standing arrangement rather
            // than a property of one invocation — and a daemon that spawned a
            // proxy because of a flag somebody typed once would leave one
            // behind exactly when nobody was expecting it. `R-O10`.
            proxy: mogeung_core::llmproxy::ProxySettings {
                enabled: cfg.llmproxy.unwrap_or(false),
                bin: cfg.llmproxy_bin.clone().unwrap_or_else(|| "llmproxy".into()),
                config: cfg.llmproxy_config.clone(),
                port: cfg.llmproxy_port,
            },
            model: mogeung_core::model::ModelSettings {
                url: args.model_url.or(cfg.model_url),
                model: args.model_name.or(cfg.model_name),
                // File-only: an embedding model is not something you pass on a
                // launcher's argv, and it shares `model_url`'s host and consent.
                embed_model: cfg.embed_model.clone(),
                // The flag is broader than anything the file can say, so it
                // wins by being checked first — and there is deliberately no
                // way for the file to *narrow* a flag that was passed. A flag
                // is this invocation; the file is the standing preference.
                consent: if args.allow_remote_model {
                    mogeung_core::model::RemoteConsent::Any
                } else {
                    cfg.allow_remote_model
                },
            },
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
            advertise: false,
            allow_run: false,
            model_url: None,
            model_name: None,
            allow_remote_model: false,
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

    /// The endpoint follows the same order as everything else, and so does the
    /// consent — ADR-0031, which replaced ADR-0030's flag-only clause because
    /// a window-hosted daemon has no argv to receive a flag through.
    #[test]
    fn the_model_endpoint_comes_from_the_file_and_the_flag_beats_it() {
        use mogeung_core::model::RemoteConsent;

        let cfg = Config {
            model_url: Some("http://127.0.0.1:8000/v1".into()),
            model_name: Some("from-file".into()),
            ..Config::default()
        };
        let o = resolve(args(), cfg.clone()).1;
        assert_eq!(o.model.url.as_deref(), Some("http://127.0.0.1:8000/v1"));
        assert_eq!(o.model.model.as_deref(), Some("from-file"));
        assert_eq!(o.model.consent, RemoteConsent::None, "nothing asked for, nothing granted");

        let typed = Args {
            model_url: Some("http://spark-7ecc:8000/v1".into()),
            model_name: Some("typed".into()),
            allow_remote_model: true,
            ..args()
        };
        let o = resolve(typed, cfg).1;
        assert_eq!(o.model.url.as_deref(), Some("http://spark-7ecc:8000/v1"));
        assert_eq!(o.model.model.as_deref(), Some("typed"));
        assert_eq!(o.model.consent, RemoteConsent::Any, "the flag is the blanket grant");
    }

    /// The case the flag could not reach: a daemon with no argv, consenting
    /// through the file — and the flag still winning when there is one, since
    /// `Any` is broader than any host the file could name.
    #[test]
    fn the_file_can_consent_to_a_named_host_and_the_flag_still_widens_it() {
        use mogeung_core::model::RemoteConsent;

        let cfg = Config {
            model_url: Some("http://spark-7ecc:8000/v1".into()),
            allow_remote_model: RemoteConsent::Host("spark-7ecc".into()),
            ..Config::default()
        };
        let o = resolve(args(), cfg.clone()).1;
        assert_eq!(o.model.consent, RemoteConsent::Host("spark-7ecc".into()));
        assert!(mogeung_core::model::admit(&o.model).is_ok(), "the file alone must be enough");

        let o = resolve(Args { allow_remote_model: true, ..args() }, cfg).1;
        assert_eq!(o.model.consent, RemoteConsent::Any, "a flag widens, never narrows");

        assert!(!resolve(args(), Config::default()).1.model.configured());
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
