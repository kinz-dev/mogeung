//! Making one executable enough.
//!
//! Running mogeung used to mean two commands in two terminals. Now the window
//! checks whether a daemon is already watching, and hosts one itself if not —
//! see [ADR-0009](../../../docs/decisions/0009-the-window-may-host-a-daemon.md).
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
use std::path::PathBuf;

/// Where the daemon this window talks to came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Something was already listening; we are a guest and will leave it be.
    Attached { pid: Option<u32>, home: Option<String> },
    /// Nothing was, so this process is hosting one. It dies when we do.
    Hosting,
    /// Asked not to start one, and none was running.
    None,
}

impl Mode {
    pub fn label(&self) -> &'static str {
        match self {
            Mode::Attached { .. } => "attached",
            Mode::Hosting => "hosting",
            Mode::None => "no daemon",
        }
    }

    pub fn detail(&self, addr: &str) -> String {
        match self {
            Mode::Attached { pid, home } => format!(
                "Attached to a daemon already running on {addr}{}{}. Closing this \
                 window leaves it running.",
                pid.map(|p| format!(" (pid {p})")).unwrap_or_default(),
                home.as_deref()
                    .map(|h| format!(", watching {h}"))
                    .unwrap_or_default()
            ),
            Mode::Hosting => format!(
                "This window is hosting the daemon on {addr}. Closing it stops \
                 watching — run mogeungd separately if you want notifications to \
                 continue."
            ),
            Mode::None => format!("Nothing is serving {addr}. The board will stay empty."),
        }
    }
}

/// Attach to a running daemon, or take the port and host one.
///
/// Returns the listener to serve on when we won the bind.
pub fn acquire(addr: &str, allow_start: bool) -> (Mode, Option<TcpListener>) {
    match TcpListener::bind(addr) {
        // Nobody home. We are the daemon now.
        Ok(listener) if allow_start => (Mode::Hosting, Some(listener)),
        Ok(_) => (Mode::None, None),
        Err(_) => {
            // Something holds the port. Confirm it is actually mogeung before
            // trusting it — otherwise an unrelated service on 7717 would leave
            // the window sitting on a websocket that never connects, with no
            // explanation.
            match probe(addr) {
                Some(info) => (
                    Mode::Attached {
                        pid: info.pid,
                        home: info.claude_home,
                    },
                    None,
                ),
                None => (Mode::None, None),
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
/// start-up, and it is not worth an HTTP client dependency in the UI.
pub fn probe(addr: &str) -> Option<Probe> {
    use std::io::{Read, Write};
    use std::time::Duration;

    let target: std::net::SocketAddr = addr.parse().ok().or_else(|| {
        use std::net::ToSocketAddrs;
        addr.to_socket_addrs().ok()?.next()
    })?;

    let mut sock = std::net::TcpStream::connect_timeout(&target, Duration::from_millis(1500)).ok()?;
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

/// Start serving on `listener`, on a thread that dies with this process.
pub fn host(listener: TcpListener, db: Option<PathBuf>, notify: mogeungd::notify::NotifyConfig) {
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
            let opts = mogeungd::server::Options {
                db: db.unwrap_or_else(mogeungd::server::default_db),
                notify,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn free_port() -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        drop(l);
        format!("127.0.0.1:{}", addr.port())
    }

    #[test]
    fn an_empty_port_means_we_host() {
        let addr = free_port();
        let (mode, listener) = acquire(&addr, true);
        assert_eq!(mode, Mode::Hosting);
        assert!(listener.is_some(), "hosting needs the listener handed back");
    }

    #[test]
    fn opting_out_never_starts_one() {
        let addr = free_port();
        let (mode, listener) = acquire(&addr, false);
        assert_eq!(mode, Mode::None);
        assert!(listener.is_none());
        // And the port is released again, so a later launch can host.
        assert!(TcpListener::bind(&addr).is_ok());
    }

    /// The important negative: a port held by something that is *not* mogeung
    /// must not be treated as a daemon. Otherwise the window sits forever on a
    /// websocket that will never connect, with nothing said.
    #[test]
    fn a_stranger_on_the_port_is_not_mistaken_for_a_daemon() {
        let addr = free_port();
        let squatter = TcpListener::bind(&addr).unwrap();

        let (mode, listener) = acquire(&addr, true);
        assert_eq!(mode, Mode::None, "a non-mogeung listener was trusted");
        assert!(listener.is_none());
        drop(squatter);
    }

    #[test]
    fn probing_nothing_is_not_an_error() {
        assert!(probe(&free_port()).is_none());
    }

    #[test]
    fn the_mode_explains_what_closing_the_window_will_do() {
        // Hosting is the surprising case, so it has to say so.
        let hosting = Mode::Hosting.detail("127.0.0.1:7717");
        assert!(hosting.contains("Closing it stops watching"), "{hosting}");

        let attached = Mode::Attached {
            pid: Some(42),
            home: Some("/home/x/.claude".into()),
        }
        .detail("127.0.0.1:7717");
        assert!(attached.contains("leaves it running"), "{attached}");
        assert!(attached.contains("pid 42"));
    }
}
