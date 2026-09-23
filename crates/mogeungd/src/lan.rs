//! The beacon the daemon sends. `R-I17` phase 1;
//! [ADR-0039](../../../docs/decisions/0039-the-daemon-announces-itself-by-default.md)
//! for the default,
//! [ADR-0040](../../../docs/decisions/0040-discovery-is-the-beacon.md) for the
//! transport.
//!
//! What a beacon says and how a listener keeps score is `mogeung_core::lan`,
//! shared with the shell that listens. This is the sending half: which
//! interfaces, how the daemon behind this bind can be reached, the two counts
//! by the queue's own rule, and the task that runs for the daemon's life.
//!
//! # A burst of two, a second apart
//!
//! The probe (`--bin beacon`) measured 60 % delivery into a laptop in Wi-Fi
//! power save and 100 % into everything else. Two datagrams a second apart
//! turn 60 % into 84 % per beacon, and the listener already counts a sequence
//! number once, so the second costs nothing there. ADR-0040.
//!
//! # Jitter without a random number
//!
//! ±2 s around the interval, so two daemons started together do not beat in
//! step for ever. Derived from the machine id and the sequence number rather
//! than drawn, because a dependency for one number is the wrong trade.
//!
//! # Presence, never access
//!
//! `reach` is read from the bind: beyond loopback means `lan`, and `R-I10`
//! already means a token there; loopback with an `ssh_target` means `ssh`;
//! loopback otherwise means `local`. The token has no field to travel in —
//! the codec's own test pins that the word never appears.

use crate::state::AppState;
use mogeung_core::lan::{self, Beacon, Reach, INTERVAL, VERSION};
use mogeung_core::wire::DaemonIdentity;
use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

/// Datagrams per beacon.
pub const BURST: u32 = 2;
/// Between the datagrams of one burst.
pub const BURST_GAP: Duration = Duration::from_secs(1);
/// Either side of [`INTERVAL`].
pub const JITTER: Duration = Duration::from_secs(2);

/// One interface worth a datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub iface: String,
    pub ip: Ipv4Addr,
    pub bcast: Ipv4Addr,
}

/// Which interfaces are worth a datagram. Pure, so it is tested without one.
///
/// Loopback is skipped for `R-I8`'s reason — a record reaching another host
/// claiming `127.0.0.1` describes *their* loopback. A link with no broadcast
/// address (point-to-point) has nothing to send to, and a `/32` — a tailscale
/// address, say — reports itself as its own broadcast, which is the same
/// thing.
pub fn target_of(name: &str, ip: Ipv4Addr, broadcast: Option<Ipv4Addr>) -> Option<Target> {
    if ip.is_loopback() {
        return None;
    }
    let bcast = broadcast?;
    if bcast == ip {
        return None;
    }
    Some(Target {
        iface: name.to_string(),
        ip,
        bcast,
    })
}

/// Every interface worth a datagram, right now. Asked again before every
/// beacon, because a laptop's Wi-Fi comes and goes and a list taken at start
/// would announce on a network it left an hour ago.
pub fn targets() -> std::io::Result<Vec<Target>> {
    let mut out = Vec::new();
    for i in if_addrs::get_if_addrs()? {
        if let if_addrs::IfAddr::V4(v4) = &i.addr {
            if let Some(t) = target_of(&i.name, v4.ip, v4.broadcast) {
                out.push(t);
            }
        }
    }
    Ok(out)
}

/// How the daemon behind `bind` can be reached. Presence is not access.
pub fn reach_of(bind: &SocketAddr, ssh_target: Option<&str>) -> Reach {
    if !bind.ip().is_loopback() {
        Reach::Lan
    } else if ssh_target.is_some_and(|t| !t.is_empty()) {
        Reach::Ssh
    } else {
        Reach::Local
    }
}

/// One beacon's worth of this daemon, at `seq`.
pub fn beacon_of(identity: &DaemonIdentity, bind: &SocketAddr, live: u32, waiting: u32, seq: u64) -> Beacon {
    let reach = reach_of(bind, identity.ssh_target.as_deref());
    Beacon {
        v: VERSION,
        machine: identity.machine_id.clone().unwrap_or_default(),
        host: identity.host.clone(),
        port: bind.port(),
        version: identity.version.clone(),
        reach,
        ssh: match reach {
            Reach::Ssh => identity.ssh_target.clone(),
            _ => None,
        },
        home: identity.claude_home.clone(),
        live,
        waiting,
        seq,
    }
}

/// [`INTERVAL`] ± [`JITTER`], from the machine id and the sequence number.
pub fn interval_for(machine: &str, seq: u64) -> Duration {
    let mut h = DefaultHasher::new();
    machine.hash(&mut h);
    seq.hash(&mut h);
    let jitter_ms = JITTER.as_millis() as i64;
    let offset_ms = (h.finish() % (2 * jitter_ms as u64 + 1)) as i64 - jitter_ms;
    Duration::from_millis((INTERVAL.as_millis() as i64 + offset_ms) as u64)
}

/// From a socket bound to the interface's own address, so the datagram's
/// source — the one address the listener pairs with the port — is the address
/// that interface actually has.
fn send_one(t: &Target, port: u16, bytes: &[u8]) -> std::io::Result<()> {
    let sock = UdpSocket::bind(SocketAddrV4::new(t.ip, 0))?;
    sock.set_broadcast(true)?;
    sock.send_to(bytes, SocketAddrV4::new(t.bcast, port))?;
    Ok(())
}

/// `bytes` to every target. What failed, by interface, for a caller to log
/// once rather than every fifteen seconds.
pub fn send_to_all(targets: &[Target], port: u16, bytes: &[u8]) -> Vec<(String, std::io::Error)> {
    let mut failed = Vec::new();
    for t in targets {
        if let Err(e) = send_one(t, port, bytes) {
            failed.push((t.iface.clone(), e));
        }
    }
    failed
}

/// The running beacon. Dropping it stops the sending — `server::run` holds it
/// the way it holds the mDNS advert.
pub struct Sender {
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Start sending, for as long as the returned [`Sender`] lives.
///
/// `targets` is asked before every beacon (see [`targets`]); a test hands in
/// a loopback target to a port it is listening on.
pub fn start<F>(state: Arc<AppState>, bind: SocketAddr, port: u16, targets: F) -> Sender
where
    F: Fn() -> Vec<Target> + Send + 'static,
{
    let handle = tokio::spawn(async move {
        let mut seq = 0u64;
        // Once per interface: a bridge that is down fails the same way every
        // time, and a log that says so every 15 s says nothing else.
        let mut failed: BTreeSet<String> = BTreeSet::new();
        loop {
            seq += 1;
            let (live, waiting) = state.beacon_counts().await;
            let identity = state.daemon_identity();
            let b = beacon_of(&identity, &bind, live, waiting, seq);
            let wait = interval_for(&b.machine, seq);
            match lan::encode(&b) {
                Ok(bytes) => {
                    let now = targets();
                    for i in 0..BURST {
                        for (iface, e) in send_to_all(&now, port, &bytes) {
                            if failed.insert(iface.clone()) {
                                tracing::warn!("beacon not sent via {iface}: {e}");
                            }
                        }
                        if i + 1 < BURST {
                            tokio::time::sleep(BURST_GAP).await;
                        }
                    }
                }
                Err(e) => tracing::warn!("beacon not sent: {e}"),
            }
            tokio::time::sleep(wait.saturating_sub(BURST_GAP * (BURST - 1))).await;
        }
    });
    Sender { handle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_interface_with_a_broadcast_address_of_its_own_gets_a_datagram() {
        let ip = Ipv4Addr::new(10, 0, 0, 27);
        let bcast = Ipv4Addr::new(10, 0, 0, 255);
        assert_eq!(
            target_of("wlp0s20f3", ip, Some(bcast)),
            Some(Target {
                iface: "wlp0s20f3".into(),
                ip,
                bcast
            })
        );
        // Loopback describes our own machine, whoever hears it.
        assert_eq!(target_of("lo", Ipv4Addr::LOCALHOST, Some(Ipv4Addr::new(127, 255, 255, 255))), None);
        // A point-to-point link has nothing to broadcast to.
        assert_eq!(target_of("ppp0", ip, None), None);
        // A /32 reports itself as its own broadcast — a tailscale address.
        let ts = Ipv4Addr::new(100, 69, 22, 4);
        assert_eq!(target_of("tailscale0", ts, Some(ts)), None);
    }

    /// The bind decides. `0.0.0.0` is not loopback, so it is `lan` — and
    /// `R-I10` has already required a token for it.
    #[test]
    fn reach_follows_the_bind_and_then_the_ssh_target() {
        let lo: SocketAddr = "127.0.0.1:7717".parse().unwrap();
        let any: SocketAddr = "0.0.0.0:7717".parse().unwrap();
        let one: SocketAddr = "10.0.0.5:7717".parse().unwrap();
        assert_eq!(reach_of(&lo, None), Reach::Local);
        assert_eq!(reach_of(&lo, Some("")), Reach::Local);
        assert_eq!(reach_of(&lo, Some("kinz@devbox")), Reach::Ssh);
        assert_eq!(reach_of(&any, None), Reach::Lan);
        assert_eq!(reach_of(&one, Some("kinz@devbox")), Reach::Lan);
    }

    fn identity(ssh: Option<&str>) -> DaemonIdentity {
        DaemonIdentity {
            machine_id: Some("abc123".into()),
            host: Some("devbox".into()),
            claude_home: "/home/k/.claude".into(),
            pid: 1,
            version: "0.1.0".into(),
            ssh_target: ssh.map(String::from),
            federation: None,
        }
    }

    /// The ssh target rides only when it is the route; a `lan` daemon that
    /// also has one does not announce it, because it is not how you reach it.
    #[test]
    fn a_beacon_carries_the_ssh_target_only_when_that_is_the_route() {
        let lo: SocketAddr = "127.0.0.1:7717".parse().unwrap();
        let b = beacon_of(&identity(Some("kinz@devbox")), &lo, 3, 1, 9);
        assert_eq!((b.reach, b.ssh.as_deref(), b.port, b.live, b.waiting, b.seq), (Reach::Ssh, Some("kinz@devbox"), 7717, 3, 1, 9));
        let any: SocketAddr = "0.0.0.0:7800".parse().unwrap();
        let b = beacon_of(&identity(Some("kinz@devbox")), &any, 0, 0, 1);
        assert_eq!((b.reach, b.ssh, b.port), (Reach::Lan, None, 7800));
        assert_eq!(b.machine, "abc123");
        assert_eq!(b.host.as_deref(), Some("devbox"));
    }

    #[test]
    fn the_jitter_stays_within_two_seconds_and_differs_between_beacons() {
        let lo = INTERVAL - JITTER;
        let hi = INTERVAL + JITTER;
        let mut seen = std::collections::BTreeSet::new();
        for seq in 1..200 {
            let d = interval_for("abc123", seq);
            assert!(d >= lo && d <= hi, "{d:?}");
            seen.insert(d.as_millis());
        }
        assert!(seen.len() > 50, "a jitter that never moves is no jitter");
        // And the same input is the same answer: no clock, no randomness.
        assert_eq!(interval_for("abc123", 7), interval_for("abc123", 7));
    }
}
