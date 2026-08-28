//! Starting mogeung's own llmproxy, and stopping it. `R-O10`, ADR-0033.
//!
//! The policy — which port, what the starter config says, where it forwards —
//! is in `mogeung_core::llmproxy` and testable with nothing running. What is
//! here is the part that owns a child process.
//!
//! ## Adopt, then spawn
//!
//! The obvious shape is "spawn it, remember the pid, kill it on exit", and
//! `desktop/src-tauri/src/daemon.rs` already wrote down why that is not enough:
//! *a `SIGKILL`ed process never runs its cleanup, and a crash leaves an orphan
//! holding the port that the next launch has to reason about.* That argument
//! was made about this daemon's own port and it applies here unchanged — except
//! that an orphaned llmproxy is worse than an orphaned daemon, because it holds
//! a borrowed OAuth token and will serve it to anything that can reach the port.
//!
//! So the orphan is not prevented, it is **adopted**. Start-up probes the port
//! first: an llmproxy already answering there is used as-is, because the port
//! is derived from mogeung's own and nothing else puts an llmproxy on it. That
//! turns the failure mode from *a leak that accumulates* into *the same one
//! process, reused* — and it costs no state on disk, which is what made the
//! remembered-pid version fragile in the first place.
//!
//! ## Signals are the wrong tool here
//!
//! The first cut recorded the child's pid and signalled its process group on
//! shutdown, the way `run.rs` does. It would never have worked, and the reason
//! is worth keeping: **llmproxy re-execs itself as `--foreground` and
//! detaches**, so the process we spawn exits within a second and the daemon
//! that ends up holding the port is not our child at all. A pid recorded here
//! names something already gone.
//!
//! llmproxy's own `--shutdown --listen <addr>` is addressed by **port**, which
//! is the identity that actually persists — and, being llmproxy's, it stops
//! exactly that instance and leaves any other one alone. Verified: stopping
//! ours on its derived port left the user's own instance untouched.
//!
//! `PR_SET_PDEATHSIG` was considered before any of this was known and is
//! rejected twice over anyway. It fires on the death of the parent **thread**,
//! and this daemon runs on a tokio runtime free to retire the worker that
//! happened to call `spawn`; and it does not exist on macOS. Neither would it
//! have reached a detached grandchild.

use std::process::Stdio;
use std::sync::Mutex;

use mogeung_core::llmproxy::{ProxyHealth, ProxySettings, ProxyState};

#[derive(Default)]
struct Inner {
    settings: ProxySettings,
    state: Option<ProxyState>,
    url: Option<String>,
    forwards_to: Vec<String>,
    admin_url: Option<String>,
    /// The port to stop on shutdown, when there is one to stop.
    ///
    /// Set for an adopted instance as well as a hosted one, deliberately. The
    /// port is **derived from this daemon's own**, so an llmproxy answering
    /// there is ours from a previous life rather than a stranger's — and
    /// leaving it up would make the orphan permanent instead of adopted, which
    /// is the opposite of what adoption is for.
    stop_port: Option<u16>,
}

#[derive(Default)]
pub struct Proxy {
    inner: Mutex<Inner>,
}

impl Proxy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start or adopt, and answer with the URL the model seam should use.
    ///
    /// `None` means *do not repoint the model seam*: either the proxy is off,
    /// or it could not be started — and in the second case the configured
    /// endpoint is still there and still works, which is the whole reason this
    /// returns an option rather than failing the daemon. A routing convenience
    /// must not be able to take the panel down with it.
    ///
    /// `upstream` is the endpoint the daemon was already pointed at. It is only
    /// used to write the starter config, and it is what makes turning the proxy
    /// on a no-op for where bytes go: day one, the only provider is the one you
    /// were already using.
    pub fn ensure(
        &self,
        settings: ProxySettings,
        daemon_port: u16,
        upstream: Option<&str>,
    ) -> Option<String> {
        let port = settings.port_for(daemon_port);
        let mut g = self.inner.lock().expect("proxy lock");
        g.settings = settings.clone();

        if !settings.enabled {
            g.state = Some(ProxyState::Off);
            return None;
        }

        let path = config_path(&settings);
        g.forwards_to = std::fs::read_to_string(&path)
            .map(|t| mogeung_core::llmproxy::forwards_to(&t))
            .unwrap_or_default();

        // Adopt first. See the module note: the orphan is the expected state
        // after a SIGKILL, not an exceptional one.
        if probe(port) {
            tracing::info!("llmproxy already serving 127.0.0.1:{port} — adopting it");
            g.state = Some(ProxyState::Adopted { port });
            g.url = Some(settings.url_for(port));
            g.admin_url = read_admin_url(port);
            g.stop_port = Some(port);
            return g.url.clone();
        }

        match spawn(&settings, &path, port, upstream) {
            Ok(()) => {
                // Re-read: the starter config is written *by* `spawn`, so on a
                // first run the read above saw no file at all and the health
                // row would have claimed this proxy forwards nowhere.
                g.forwards_to = std::fs::read_to_string(&path)
                    .map(|t| mogeung_core::llmproxy::forwards_to(&t))
                    .unwrap_or_default();
                g.state = Some(ProxyState::Hosting { port });
                g.url = Some(settings.url_for(port));
                g.admin_url = read_admin_url(port);
                g.stop_port = Some(port);
                tracing::info!(
                    "llmproxy started on 127.0.0.1:{port}, rules in {}{}",
                    path.display(),
                    if g.forwards_to.is_empty() {
                        String::new()
                    } else {
                        format!(" — forwards to {}", g.forwards_to.join(", "))
                    }
                );
                g.url.clone()
            }
            Err(reason) => {
                // Loud, because the panel will silently keep using the plain
                // endpoint and the difference is invisible from the window.
                tracing::warn!("llmproxy did not start: {reason}");
                g.state = Some(ProxyState::Failed { reason });
                g.url = None;
                None
            }
        }
    }

    /// Stop the instance on our port, by asking llmproxy to.
    ///
    /// Not a signal: see the module note. The process we spawned is long gone
    /// by now, and the daemon holding the port is addressed by that port.
    pub fn shutdown(&self) {
        let (port, bin) = {
            let mut g = self.inner.lock().expect("proxy lock");
            (g.stop_port.take(), g.settings.bin.clone())
        };
        let Some(port) = port else { return };
        match stop(&bin, port) {
            Ok(()) => tracing::info!("llmproxy on 127.0.0.1:{port} stopped"),
            // Worth a line and not worth more: the daemon is on its way out,
            // and the next start-up adopts whatever is still there.
            Err(e) => tracing::warn!("could not stop llmproxy on 127.0.0.1:{port}: {e}"),
        }
    }

    pub fn health(&self) -> ProxyHealth {
        let g = self.inner.lock().expect("proxy lock");
        ProxyHealth {
            state: g.state.clone().unwrap_or(ProxyState::Off),
            url: g.url.clone(),
            admin_url: g.admin_url.clone(),
            forwards_to: g.forwards_to.clone(),
        }
    }
}

/// Ask the llmproxy on `port` to stop.
///
/// A free function rather than only a method, because there are two callers
/// that cannot share state: this daemon on its way out, and — when the daemon
/// is a **thread inside the window** — the window's own exit handler, which has
/// no reach into `AppState` at all. Two copies of this command would be two
/// things to keep in step, and the one nobody exercises would be the one that
/// rots.
///
/// Addressed by port, which is llmproxy's own identity for an instance: it
/// stops exactly that one and leaves any other alone.
///
/// `Ok` on a port nobody is serving, because llmproxy exits 0 there — the
/// verb is idempotent, so `Err` means the invocation itself failed (no such
/// binary, no permission) rather than "there was nothing to stop". That is
/// the right way round for a caller on its way out.
pub fn stop(bin: &str, port: u16) -> Result<(), String> {
    let status = std::process::Command::new(bin)
        .arg("--listen")
        .arg(format!("127.0.0.1:{port}"))
        .arg("--shutdown")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .status()
        .map_err(|e| format!("could not run `{bin}`: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{bin} --shutdown` exited {status}"))
    }
}

/// llmproxy's admin URL for the instance on this port, if it has one.
///
/// Read from the metadata file llmproxy writes, because the admin interface
/// binds a **random** port by default and there is no endpoint that reports
/// it. Every failure is `None` — this drives a button that is not shown.
fn read_admin_url(port: u16) -> Option<String> {
    let path = mogeung_core::llmproxy::runtime_info_path(&format!("127.0.0.1:{port}"));
    let text = std::fs::read_to_string(path).ok()?;
    mogeung_core::llmproxy::admin_url(&text)
}

/// mogeung's own rules file. Never `~/.llmproxy/config.toml` — keeping the two
/// apart is the entire point of running a second instance.
fn config_path(settings: &ProxySettings) -> std::path::PathBuf {
    settings.config.clone().unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        std::path::PathBuf::from(home).join(".mogeung").join("llmproxy.toml")
    })
}

/// Is an llmproxy answering here?
///
/// Public so `--bin judge` can ask the same question: a harness that graded a
/// different endpoint than the panel talks to would be measuring the wrong
/// model and reporting it as a finding.
///
/// Hand-rolled HTTP/1.0 over a raw socket, the way the window's own start-up
/// probe does it: one request, once, at start-up, and not worth an HTTP client
/// for. The body must be llmproxy's own `{"status":"ok"}` — anything else
/// holding the port is *not* adopted, because attaching the model seam to an
/// unrelated service is how you get a chat panel that times out with no
/// explanation.
pub fn probe(port: u16) -> bool {
    use std::io::{Read, Write};
    use std::time::Duration;

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut sock) = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(600))
    else {
        return false;
    };
    if sock.set_read_timeout(Some(Duration::from_millis(600))).is_err() {
        return false;
    }
    if sock
        .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut body = String::new();
    if sock.take(8 * 1024).read_to_string(&mut body).is_err() {
        return false;
    }
    body.contains("\"status\"") && body.contains("\"ok\"")
}

/// Start it, and wait until it actually answers.
///
/// Waiting is not politeness: `spawn` returns as soon as the fork succeeds, so
/// without this the model seam would be repointed at a port nothing is
/// listening on yet and the first question of every session would fail. The
/// budget is generous because llmproxy reads a config and may contact a vendor
/// for a token refresh on the way up.
fn spawn(
    settings: &ProxySettings,
    config: &std::path::Path,
    port: u16,
    upstream: Option<&str>,
) -> Result<(), String> {
    let listen = format!("127.0.0.1:{port}");

    if !config.exists() {
        // With nothing already configured there is nothing to preserve, so
        // the starter names a plausible local endpoint and forwards nowhere.
        // A config that pointed at a vendor by default would be publishing
        // dressed as a first run.
        // Normalised, for the same reason `model_url` is: the URL a human can
        // `curl` ends in `/models`, and copying that into a provider's
        // `base_url` verbatim asks the upstream for
        // `…/v1/models/chat/completions` and 404s with a message nobody can
        // read. Found by writing a starter from a real config.
        let raw = upstream
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .unwrap_or("http://127.0.0.1:8000/v1");
        let upstream = mogeung_core::model::normalise_base(raw);
        if let Some(dir) = config.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        std::fs::write(
            config,
            mogeung_core::llmproxy::starter_config(&listen, &upstream),
        )
        .map_err(|e| format!("could not write {}: {e}", config.display()))?;
        tracing::info!("wrote a starter {} — it is yours now", config.display());
    }

    let mut cmd = std::process::Command::new(&settings.bin);
    cmd.arg("--config")
        .arg(config)
        .arg("--listen")
        .arg(&listen)
        // **No mode flag.** `--proxy`, `--intercept` and `--integrated` all
        // select a routing mode *for an agent launch* and clap requires one of
        // `--claude`/`--codex`/`--copilot` alongside them — the half of
        // llmproxy that starts an agent CLI, which is the half mogeung must
        // never touch (ADR-0003). Bare is the plain server, which is all that
        // is wanted; routing comes from the config's own targets.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    // No `setsid` and no recorded pid: llmproxy re-execs itself as
    // `--foreground` and detaches, so this process exits within a second and
    // is not what ends up holding the port. Waiting on it, grouping it or
    // signalling it would all be operating on the wrong thing.
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start `{}`: {e}", settings.bin))?;

    for _ in 0..60 {
        if probe(port) {
            let _ = child.wait();
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(format!("started but never answered on {listen} — is the config valid?"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Off is a state, and it must not touch the model seam. A proxy nobody
    /// asked for that repointed `model_url` would silently change where every
    /// answer came from.
    #[test]
    fn disabled_leaves_the_endpoint_alone() {
        let p = Proxy::new();
        assert_eq!(p.ensure(ProxySettings::default(), 7717, None), None);
        assert_eq!(p.health().state, ProxyState::Off);
        assert!(p.health().url.is_none());
        // And stopping something never started is a no-op, not a signal to
        // whatever holds that pid now.
        p.shutdown();
    }

    /// A binary that is not there is a **degraded panel, never a dead daemon**.
    /// `R-O5` answers from the configured endpoint with no proxy at all, and
    /// that has to keep being true when the convenience fails.
    #[test]
    fn a_missing_binary_fails_the_proxy_and_not_the_daemon() {
        let dir = std::env::temp_dir().join(format!("mogeung-proxy-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let cfg = dir.join("llmproxy.toml");
        let _ = std::fs::write(&cfg, "listen_addr = \"127.0.0.1:1\"\n");

        let p = Proxy::new();
        let url = p.ensure(
            ProxySettings {
                enabled: true,
                bin: "definitely-not-a-real-binary-mogeung".into(),
                config: Some(cfg),
                port: Some(1),
            },
            7717,
            None,
        );
        assert_eq!(url, None, "the model seam is not repointed at a proxy that is not there");
        match p.health().state {
            ProxyState::Failed { reason } => {
                assert!(reason.contains("could not start"), "{reason}");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing on the port is not an llmproxy, and neither is something else.
    #[test]
    fn the_probe_requires_llmproxys_own_answer() {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut s = stream;
                // A perfectly good HTTP server that is not llmproxy.
                let _ = s.write_all(b"HTTP/1.0 200 OK\r\n\r\n{\"hello\":true}");
            }
        });
        assert!(!probe(port), "an unrelated service must not be adopted");

        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dead = free.local_addr().unwrap().port();
        drop(free);
        assert!(!probe(dead), "nothing there is not an llmproxy either");
    }

    /// The disclosure survives the proxy being down: `forwards_to` is read from
    /// the config file, so a failed start still says where it *would* have sent
    /// things. A sentence about the bytes that only appears when everything is
    /// working is the wrong way round.
    #[test]
    fn where_it_forwards_is_known_even_when_it_did_not_start() {
        let dir = std::env::temp_dir().join(format!("mogeung-proxy-fw-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let cfg = dir.join("llmproxy.toml");
        std::fs::write(
            &cfg,
            "[providers.claude_sub]\nbase_url = \"https://api.anthropic.com/v1\"\n",
        )
        .unwrap();

        let p = Proxy::new();
        p.ensure(
            ProxySettings {
                enabled: true,
                bin: "definitely-not-a-real-binary-mogeung".into(),
                config: Some(cfg),
                port: Some(1),
            },
            7717,
            None,
        );
        assert_eq!(p.health().forwards_to, vec!["api.anthropic.com".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
