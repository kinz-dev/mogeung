//! Making one executable enough.
//!
//! A port of `crates/mogeung-ui/src/daemon.rs`, and deliberately a faithful one
//! — the reasoning below was paid for once and both clients should behave
//! identically. See
//! [ADR-0009](../../../../docs/decisions/0009-the-window-may-host-a-daemon.md).
//!
//! ## Bind, do not ask
//!
//! The obvious design is "probe the port, start a daemon if nothing answers".
//! It has a race: two windows opened together both probe, both see nothing, and
//! both try to start one. So the test is the **bind itself** — whoever gets the
//! socket is the daemon, and whoever is refused knows someone else already is.
//! There is no window in between.
//!
//! ## In-process, not a subprocess
//!
//! The daemon we start runs on a thread in this process, not as a child.
//!
//! That was not the obvious shape either — "spawn it, remember the pid, kill it
//! on exit" is the natural description. But every part of that can fail: a pid
//! file goes stale, a `SIGKILL`ed window never runs its cleanup, and a crash
//! leaves an orphan holding the port that the next launch has to reason about.
//! A thread cannot outlive its process. The bookkeeping disappears because the
//! operating system does it.

use std::net::TcpListener;

use serde::Serialize;

/// Where the daemon this window talks to came from.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Status {
    /// Something was already listening; we are a guest and will leave it be.
    Attached {
        pid: Option<u32>,
        claude_home: Option<String>,
    },
    /// Nothing was, so this process is hosting one. It dies when we do.
    Hosting,
    /// Nothing is serving, and we did not or could not start one.
    None { reason: Option<String> },
}

impl Status {
    /// The sentence the window shows. Same words as the egui client's, because
    /// the same situation should not read as two different products.
    pub fn detail(&self, addr: &str) -> String {
        match self {
            Status::Attached { pid, claude_home } => format!(
                "Attached to a daemon already running on {addr}{}{}. Closing this \
                 window leaves it running.",
                pid.map(|p| format!(" (pid {p})")).unwrap_or_default(),
                claude_home
                    .as_deref()
                    .map(|h| format!(", watching {h}"))
                    .unwrap_or_default()
            ),
            Status::Hosting => format!(
                "This window is hosting the daemon on {addr}. Closing it stops \
                 watching — run mogeungd separately if you want notifications to \
                 continue."
            ),
            Status::None { reason } => match reason {
                Some(r) => format!("Nothing is serving {addr}: {r}. The board will stay empty."),
                None => format!("Nothing is serving {addr}. The board will stay empty."),
            },
        }
    }
}

/// Attach to a running daemon, or take the port and host one.
pub fn acquire(addr: &str) -> Status {
    match TcpListener::bind(addr) {
        // Nobody home. We are the daemon now.
        Ok(listener) => {
            // The hosted daemon refuses the same binds `mogeungd` refuses
            // (`R-I10`), and it refuses them on a background thread where the
            // message is a line nobody reads. Ask the same question here, so
            // the refusal can be returned to the window and shown.
            if let Ok(bound) = listener.local_addr() {
                if let Err(e) = mogeungd::server::admit(&bound, None) {
                    return Status::None {
                        reason: Some(e.to_string()),
                    };
                }
            }
            host(listener);
            Status::Hosting
        }
        Err(_) => {
            // Something holds the port. Confirm it is actually mogeung before
            // trusting it — otherwise an unrelated service on 7717 would leave
            // the window sitting on a websocket that never connects, with no
            // explanation.
            match probe(addr) {
                Some(p) => Status::Attached {
                    pid: p.pid,
                    claude_home: p.claude_home,
                },
                None => Status::None {
                    reason: Some("something else holds the port".into()),
                },
            }
        }
    }
}

pub struct Probe {
    pub pid: Option<u32>,
    pub claude_home: Option<String>,
}

/// Ask `GET /api/health` whether the thing on this port is a mogeung daemon.
///
/// Hand-rolled HTTP/1.0 over a raw socket: this is one request, once, at
/// start-up, and it is not worth an HTTP client dependency for it.
pub fn probe(addr: &str) -> Option<Probe> {
    use std::io::{Read, Write};
    use std::time::Duration;

    let target: std::net::SocketAddr = addr.parse().ok().or_else(|| {
        use std::net::ToSocketAddrs;
        addr.to_socket_addrs().ok()?.next()
    })?;

    let mut sock =
        std::net::TcpStream::connect_timeout(&target, Duration::from_millis(1500)).ok()?;
    sock.set_read_timeout(Some(Duration::from_millis(1500))).ok()?;
    sock.write_all(b"GET /api/health HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .ok()?;

    let mut body = String::new();
    sock.take(64 * 1024).read_to_string(&mut body).ok()?;

    let json = body.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(&body);
    let v: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    // Any JSON server could answer; require the shape only mogeungd produces.
    if v.get("ok")?.as_bool() != Some(true) || v.get("headline").is_none() {
        return None;
    }
    Some(Probe {
        pid: v.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32),
        claude_home: v
            .get("claude_home")
            .and_then(|h| h.as_str())
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// An unrelated service on the port must not be mistaken for a daemon.
    ///
    /// Without the probe, the window would attach to whatever answered and sit
    /// on a websocket that never connects, with nothing on screen explaining
    /// why. Confirming the shape is what turns that into a sentence.
    #[test]
    fn something_that_is_not_mogeung_is_not_attached_to() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        // Serves for the life of the test, and — the part that matters — never
        // lets go of the port. Taking a fixed number of connections drops the
        // listener partway through, and `acquire` then finds the port free and
        // hosts, which looks exactly like the bug this is checking for.
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if let Ok(mut s) = stream {
                    // A perfectly good HTTP server that is not this one.
                    let _ = s.write_all(
                        b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true}",
                    );
                }
            }
        });
        // `ok: true` alone is not enough — `headline` is the shape only
        // mogeungd produces, and requiring it is the whole point.
        assert!(probe(&addr).is_none(), "a bare JSON server must not pass for a daemon");

        match acquire(&addr) {
            Status::None { reason } => assert!(reason.is_some(), "a refusal has to say why"),
            other => panic!("expected None, got {other:?}"),
        }
    }

    /// The window and the daemon must derive the **same** port, or the window
    /// asks a port nobody is on, reports success, and leaves the real proxy
    /// running. `R-O10`.
    ///
    /// Built from `ProxySettings` rather than from arithmetic here, so the two
    /// cannot drift: this asserts the wiring, and `port_for` owns the rule.
    #[test]
    fn the_window_stops_the_port_the_daemon_started() {
        use mogeung_core::config::Config;
        use mogeung_core::llmproxy::ProxySettings;

        let off = Config::default();
        assert_eq!(hosted_proxy_target(&off, 7717), None, "nothing to stop when it is off");

        let on = Config { llmproxy: Some(true), ..Config::default() };
        let settings = ProxySettings { enabled: true, ..ProxySettings::default() };
        assert_eq!(
            hosted_proxy_target(&on, 7717),
            Some(("llmproxy".to_string(), settings.port_for(7717))),
            "the same derivation server::run uses"
        );

        // An explicit port and a named binary both travel.
        let pinned = Config {
            llmproxy: Some(true),
            llmproxy_bin: Some("/opt/llmproxy".into()),
            llmproxy_port: Some(9100),
            ..Config::default()
        };
        assert_eq!(
            hosted_proxy_target(&pinned, 7717),
            Some(("/opt/llmproxy".to_string(), 9100))
        );
    }

    /// A port nobody holds means *we* are the daemon — decided by winning the
    /// bind rather than by asking first, so two windows opening together cannot
    /// both conclude the port is free.
    #[test]
    fn an_empty_port_is_ours_to_take() {
        let port = free_port();
        let addr = format!("127.0.0.1:{port}");
        assert!(
            TcpListener::bind(&addr).is_ok(),
            "the port must be free for this test to mean anything"
        );
        // `acquire` would start a real daemon here, so only the decision is
        // exercised: nothing is listening, therefore the bind is what happens.
        assert!(probe(&addr).is_none(), "nothing is serving it yet");
    }
}

/// The proxy **this window** started, if it started one: its binary and port.
///
/// Set only on the hosting path. When the window merely attached to a daemon
/// somebody else is running, that daemon owns its proxy and will stop it on its
/// own way out — stopping it from here would take down a proxy serving a
/// process that is still very much alive.
static HOSTED_PROXY: std::sync::OnceLock<(String, u16)> = std::sync::OnceLock::new();

/// What this window would have to stop, given the config and the port it bound.
///
/// Split out and tested because it has to agree with `server::run`, which is
/// what actually starts the proxy: the two derive the port independently, and
/// a disagreement would be invisible — the window would ask a port nobody is
/// on, report success, and leave the real proxy running. `None` when the proxy
/// is off, which is also when there is nothing to stop.
fn hosted_proxy_target(
    cfg: &mogeung_core::config::Config,
    bound_port: u16,
) -> Option<(String, u16)> {
    if !cfg.llmproxy.unwrap_or(false) {
        return None;
    }
    let settings = mogeung_core::llmproxy::ProxySettings {
        enabled: true,
        bin: cfg.llmproxy_bin.clone().unwrap_or_else(|| "llmproxy".into()),
        config: cfg.llmproxy_config.clone(),
        port: cfg.llmproxy_port,
    };
    Some((settings.bin.clone(), settings.port_for(bound_port)))
}

/// Stop the llmproxy this window started. `R-O10`.
///
/// Called from the shell's exit handler, and it exists because of a gap
/// ADR-0033 clause 4 did not close. The hosted daemon is handed a shutdown
/// future that **never resolves** — see `host` below, and ADR-0009 for why —
/// so `server::run`'s own cleanup, `Proxy::shutdown` included, is unreachable
/// on this path. The daemon simply dies with the process, and before this the
/// proxy did not: it was orphaned on every window close and adopted again on
/// every launch, which is one process rather than a growing pile but is not
/// what *"kill it when mogeung is stopped"* asked for. Worse, the orphan holds
/// a borrowed OAuth token and serves it to anything that can reach the port,
/// while the window has just told you that closing it stopped watching.
///
/// This does not cover `SIGKILL`, and nothing can. Adoption still does.
pub fn stop_hosted_proxy() {
    let Some((bin, port)) = HOSTED_PROXY.get() else { return };
    match mogeungd::llmproxy::stop(bin, *port) {
        Ok(()) => eprintln!("llmproxy on 127.0.0.1:{port} stopped"),
        // A line, and no more: the process is on its way out, and the next
        // launch adopts whatever is still there.
        Err(e) => eprintln!("could not stop llmproxy on 127.0.0.1:{port}: {e}"),
    }
}

/// Start serving on `listener`, on a thread that dies with this process.
fn host(listener: TcpListener) {
    std::thread::Builder::new()
        .name("mogeungd".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("could not start the daemon runtime: {e}");
                    return;
                }
            };
            // A broken file costs its settings, never the daemon — `R-J3`'s rule,
            // and the warning goes to stderr because this thread has no window
            // to complain into yet.
            let (cfg, cfg_warning) = mogeung_core::config::Config::load();
            if let Some(w) = cfg_warning {
                eprintln!("{w}");
            }
            // Recorded before the daemon runs, because after this the thread
            // never returns.
            if let Ok(bound) = listener.local_addr() {
                if let Some(target) = hosted_proxy_target(&cfg, bound.port()) {
                    let _ = HOSTED_PROXY.set(target);
                }
            }

            let opts = mogeungd::server::Options {
                db: mogeungd::server::default_db(),
                // Off unless asked for, matching `mogeungd`'s own default: the
                // window is in front of you, so a banner for the thing you are
                // looking at is noise. `--notify` is opt-in there and this has
                // no flags, so it stays off.
                notify: mogeungd::notify::NotifyConfig {
                    desktop: false,
                    push_url: None,
                },
                // Read from the config file, and only these. `R-O1`, `R-O9`,
                // `R-O10`.
                //
                // This hosted daemon reads nothing else from `config.toml` — it
                // takes the defaults for `db`, `poll_ms` and the rest — and
                // widening that is a separate question with its own
                // consequences. The model seam cannot wait for it: a window that
                // hosts its own daemon has no argv, so with nothing read here
                // the chat panel would say *no model configured* for ever and
                // there would be no way to change its mind.
                //
                // The consent is read too, which it was not when this shipped.
                // ADR-0030 clause 3 made it flag-only on `--allow-run`'s shape,
                // and the shapes are not the same: `runs_allowed` reads the
                // **bind**, so a hosted daemon is loopback and needs no flag,
                // where the model gate reads the **endpoint** — leaving consent
                // literally unreachable here, and the endpoint refused for ever
                // on the shape mogeung is normally run in.
                // [ADR-0031](../../../../docs/decisions/0031-consent-to-a-named-host.md)
                // replaced it with a key that names the host it consents to.
                // `R-O9`: the fourth key, and read for the same reason as the
                // other three — a hosted daemon has no argv, so a preference
                // it cannot be told is a preference that does not exist.
                chat_history: cfg.chat_history.unwrap_or(true),
                // `R-O10`. The hosted daemon owns the proxy the same way a
                // standalone one does — and stops it the same way, which is
                // the case ADR-0033's adopt-on-restart exists for: this
                // process can be killed without ever running its cleanup.
                proxy: mogeung_core::llmproxy::ProxySettings {
                    enabled: cfg.llmproxy.unwrap_or(false),
                    bin: cfg.llmproxy_bin.clone().unwrap_or_else(|| "llmproxy".into()),
                    config: cfg.llmproxy_config.clone(),
                    port: cfg.llmproxy_port,
                },
                model: mogeung_core::model::ModelSettings {
                    url: cfg.model_url.clone(),
                    model: cfg.model_name.clone(),
                    consent: cfg.allow_remote_model.clone(),
                },
                ..Default::default()
            };
            // Never resolves: the daemon stops when the process does, which is
            // exactly the contract — we started it, so it is ours to end.
            let forever = std::future::pending::<()>();
            if let Err(e) = rt.block_on(mogeungd::server::run(listener, opts, forever)) {
                eprintln!("daemon stopped: {e}");
            }
        })
        .expect("spawning the daemon thread");
}
