//! Finding daemons on the local network, and being found. `R-I8`.
//!
//! Both halves live here — the daemon advertises, the window browses — for the
//! reason [`crate::machine`] gives: one implementation, so the two ends cannot
//! disagree about what the service record means.
//!
//! # Advertising is opt-in, and stays that way
//!
//! A daemon that advertises is announcing *"this machine is watching Claude
//! Code sessions, here is its address"* to everything on the segment. That is a
//! disclosure even before anyone tries the port — it names the machine, the
//! software and the fact that there is something worth reaching. Guest wifi,
//! a conference network and a shared office all make it a bad trade, and none
//! of them is detectable from in here.
//!
//! So it happens only when asked (`--advertise`), and never by default.
//!
//! # Discovery is an offer, never a connection
//!
//! Browsing returns a list. The window shows it, and a human picks. Nothing
//! here dials anything, because "a daemon appeared on the network and my tool
//! connected to it" is a sentence nobody should be able to say about this.
//!
//! # What is advertised interlocks with the token rule
//!
//! Advertising a loopback bind is refused: nobody else could reach it, so the
//! record would be a lie. That leaves only non-loopback binds, and `R-I10`
//! already requires a token for those — so anything found by browsing is a
//! daemon that will demand a token, and the window says so before you connect.

use anyhow::{anyhow, Result};
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use mogeung_core::wire::DaemonIdentity;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

/// The service type. `_mogeung._tcp` under the usual mDNS domain.
pub const SERVICE: &str = "_mogeung._tcp.local.";

/// TXT keys. Kept short because a TXT record is not a place to be expressive.
const KEY_MACHINE: &str = "machine";
const KEY_VERSION: &str = "version";
const KEY_TOKEN: &str = "token";
const KEY_HOME: &str = "home";

/// A live advertisement. Dropping it withdraws the record.
///
/// Withdrawing matters: a stale record sends the next person to browse at a
/// daemon that is not there, and they will read the timeout as their own
/// mistake. Best effort — a process that is killed outright cannot retract
/// anything, which is what the TTL is for.
pub struct Advert {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Drop for Advert {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}

/// Announce this daemon on the local network.
///
/// `addr` is the bind address, and it must not be loopback — see the module
/// note. The instance name is the hostname, because that is what a person
/// scanning a list is looking for.
pub fn advertise(addr: SocketAddr, identity: &DaemonIdentity, has_token: bool) -> Result<Advert> {
    if addr.ip().is_loopback() {
        return Err(anyhow!(
            "refusing to advertise a loopback bind: nobody else can reach {addr}, \
             so the record would send anyone who found it nowhere. Use --listen \
             with a reachable address (which also requires --token)."
        ));
    }

    let host = identity
        .host
        .clone()
        .unwrap_or_else(|| "mogeung".to_string());
    // mDNS hostnames end in a dot and must not contain one otherwise; a
    // machine called `dev.local` would otherwise produce a name no resolver
    // agrees about.
    let host_name = format!("{}.local.", host.replace('.', "-"));

    let properties = [
        (KEY_MACHINE, identity.machine_id.clone().unwrap_or_default()),
        (KEY_VERSION, identity.version.clone()),
        // So the window can say "needs a token" before you try, rather than
        // after a 401 you have to interpret.
        (KEY_TOKEN, if has_token { "1".into() } else { "0".into() }),
        (KEY_HOME, identity.claude_home.clone()),
    ];

    let info = ServiceInfo::new(
        SERVICE,
        &host,
        &host_name,
        addr.ip(),
        addr.port(),
        &properties[..],
    )?;
    let fullname = info.get_fullname().to_string();

    let daemon = ServiceDaemon::new()?;
    daemon.register(info)?;
    Ok(Advert { daemon, fullname })
}

/// A daemon someone else is advertising.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The instance name — its hostname, as advertised.
    pub name: String,
    /// Where to reach it.
    pub addr: SocketAddr,
    /// Its `machine_id`, when it published one. Lets a client recognise its
    /// own daemon in the list instead of offering it back.
    pub machine_id: Option<String>,
    pub version: Option<String>,
    /// Whether it will demand a token.
    pub needs_token: bool,
    /// The `~/.claude` it watches — two daemons on one host are two worlds.
    pub claude_home: Option<String>,
}

impl Found {
    /// The websocket URL for this daemon.
    pub fn url(&self) -> String {
        format!("ws://{}/ws", self.addr)
    }
}

/// Listen for advertisements for `window`, then stop.
///
/// Blocking, and meant for a thread of its own: mDNS answers arrive whenever
/// they arrive, and a UI cannot wait on that. A fixed window rather than
/// "until we have some" because *nothing found* is a real answer, and the one
/// worth reporting quickly.
pub fn browse(window: Duration) -> Result<Vec<Found>> {
    let daemon = ServiceDaemon::new()?;
    let rx = daemon.browse(SERVICE)?;
    let deadline = std::time::Instant::now() + window;
    let mut found: Vec<Found> = Vec::new();

    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        match rx.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let Some(ip) = info.addresses.iter().map(|a| a.to_ip_addr()).find(usable) else {
                    continue;
                };
                let addr = SocketAddr::new(ip, info.get_port());
                let entry = Found {
                    name: instance_name(info.get_fullname()),
                    addr,
                    machine_id: prop(&info, KEY_MACHINE),
                    version: prop(&info, KEY_VERSION),
                    needs_token: prop(&info, KEY_TOKEN).as_deref() == Some("1"),
                    claude_home: prop(&info, KEY_HOME),
                };
                // A host with several interfaces answers several times.
                if !found.iter().any(|f| f.addr == entry.addr) {
                    found.push(entry);
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    found.sort_by(|a, b| a.name.cmp(&b.name).then(a.addr.cmp(&b.addr)));
    Ok(found)
}

/// Addresses worth offering. Loopback is dropped because a record reaching us
/// over the network claiming `127.0.0.1` describes *our* loopback, not theirs,
/// and link-local v6 needs a scope this cannot carry into a URL.
fn usable(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_unspecified(),
        IpAddr::V6(v6) => !v6.is_loopback() && !v6.is_unspecified() && !is_link_local(v6),
    }
}

fn is_link_local(v6: &std::net::Ipv6Addr) -> bool {
    (v6.segments()[0] & 0xffc0) == 0xfe80
}

fn prop(info: &ResolvedService, key: &str) -> Option<String> {
    info.txt_properties
        .get_property_val_str(key)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

/// `devbox._mogeung._tcp.local.` → `devbox`.
fn instance_name(fullname: &str) -> String {
    fullname
        .strip_suffix(SERVICE)
        .and_then(|s| s.strip_suffix('.'))
        .unwrap_or(fullname)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn identity() -> DaemonIdentity {
        DaemonIdentity {
            machine_id: Some("abc".into()),
            host: Some("devbox".into()),
            claude_home: "/home/dev/.claude".into(),
            pid: 1,
            version: "0.1.0".into(),
            ssh_target: None,
        }
    }

    /// The interlock with `R-I10`: a loopback daemon cannot be reached by
    /// anyone who finds the record, so publishing one is publishing a wrong
    /// answer. It is also the only bind that escapes the mandatory token, so
    /// refusing here keeps "found on the network" and "demands a token" the
    /// same set.
    #[test]
    fn a_loopback_daemon_refuses_to_advertise() {
        let Err(err) = advertise("127.0.0.1:7717".parse().unwrap(), &identity(), false) else {
            panic!("loopback must not advertise");
        };
        assert!(err.to_string().contains("loopback"), "got: {err}");
    }

    #[test]
    fn the_instance_name_is_just_the_host() {
        assert_eq!(instance_name("devbox._mogeung._tcp.local."), "devbox");
        // Anything that does not match the shape is passed through rather than
        // mangled — a record from something else is not ours to reinterpret.
        assert_eq!(instance_name("odd-name"), "odd-name");
    }

    /// A record arriving over the network that claims `127.0.0.1` is
    /// describing our own loopback, not the sender's. Offering it would build
    /// a URL that points back at this machine.
    #[test]
    fn loopback_and_unscoped_addresses_are_not_offered() {
        assert!(usable(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 5))));
        assert!(!usable(&IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!usable(&IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert!(!usable(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
        // fe80::/10 needs an interface scope that a URL cannot carry.
        let link_local: Ipv6Addr = "fe80::1".parse().unwrap();
        assert!(!usable(&IpAddr::V6(link_local)));
        let global: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert!(usable(&IpAddr::V6(global)));
    }

    #[test]
    fn a_found_daemon_becomes_a_websocket_url() {
        let f = Found {
            name: "devbox".into(),
            addr: "192.168.1.5:7717".parse().unwrap(),
            machine_id: None,
            version: None,
            needs_token: true,
            claude_home: None,
        };
        assert_eq!(f.url(), "ws://192.168.1.5:7717/ws");
    }

    /// Nothing on the network is an answer, not a hang. Browsing must return
    /// within its window even when the segment is silent — which is the
    /// ordinary case on a developer machine with no second daemon.
    #[test]
    fn browsing_a_silent_network_returns_empty_rather_than_waiting() {
        let started = std::time::Instant::now();
        let found = browse(Duration::from_millis(300)).unwrap_or_default();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "browse must respect its window"
        );
        // Anything found here is a real daemon on this network, which is fine;
        // the assertion is about returning, not about the contents.
        for f in &found {
            assert!(!f.addr.ip().is_loopback());
        }
    }
}
