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

    let info = service_info(addr, &host, &host_name, &properties)?;
    let fullname = info.get_fullname().to_string();

    let daemon = ServiceDaemon::new()?;
    daemon.register(info)?;
    Ok(Advert { daemon, fullname })
}

/// Build the record, choosing the addresses to publish.
///
/// **`--listen 0.0.0.0` is the ordinary way to run a reachable daemon, and it
/// is not an address anyone can connect to.** Publishing it verbatim produces a
/// record whose only address is unspecified; a browser then has nothing usable
/// to offer, and the daemon is simply invisible — which is exactly how this
/// failed the first time it met a real network, on 2026-07-31.
///
/// So an unspecified bind hands the address list to mdns-sd's `addr_auto`,
/// which fills it from the host's live interfaces and keeps it current as they
/// change. A specific bind publishes exactly that address, because the operator
/// chose it and picking a different one would be second-guessing them.
fn service_info(
    addr: SocketAddr,
    host: &str,
    host_name: &str,
    properties: &[(&str, String)],
) -> Result<ServiceInfo> {
    if addr.ip().is_unspecified() {
        return Ok(
            ServiceInfo::new(SERVICE, host, host_name, (), addr.port(), properties)?
                .enable_addr_auto(),
        );
    }
    Ok(ServiceInfo::new(
        SERVICE,
        host,
        host_name,
        addr.ip(),
        addr.port(),
        properties,
    )?)
}

/// A daemon someone else is advertising.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The instance name — its hostname, as advertised.
    pub name: String,
    /// Every address it can be reached on, **in a stable order**: IPv4 before
    /// IPv6, then numerically.
    ///
    /// The order is part of the contract. This was a `HashSet` read with
    /// `.find()`, so a host answering on both families produced a different
    /// address each time the list was drawn — one daemon appearing to move
    /// between scans, which reads as an unreliable network rather than an
    /// unordered container.
    pub addrs: Vec<SocketAddr>,
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
    /// The address to dial: the lowest IPv4 when there is one. IPv4 first
    /// because a URL carrying a v6 literal needs brackets, and a link-local one
    /// needs a scope that will not survive the trip.
    pub fn addr(&self) -> Option<SocketAddr> {
        self.addrs.first().copied()
    }

    /// The websocket URL for this daemon.
    pub fn url(&self) -> Option<String> {
        Some(format!("ws://{}/ws", self.addr()?))
    }

    /// Merge a later sighting into an earlier one, keeping every address seen.
    ///
    /// A host announces per interface and per family, so sightings arrive
    /// piecemeal. Replacing rather than merging is half of why a row flickered.
    pub fn absorb(&mut self, other: Found) {
        for a in other.addrs {
            if !self.addrs.contains(&a) {
                self.addrs.push(a);
            }
        }
        sort_addrs(&mut self.addrs);
        self.machine_id = other.machine_id.or_else(|| self.machine_id.take());
        self.version = other.version.or_else(|| self.version.take());
        self.claude_home = other.claude_home.or_else(|| self.claude_home.take());
        self.needs_token = other.needs_token;
    }
}

/// IPv4 before IPv6, then numerically. Deterministic is the whole point.
fn sort_addrs(addrs: &mut [SocketAddr]) {
    addrs.sort_by_key(|a| match a.ip() {
        IpAddr::V4(v4) => (0u8, u128::from(u32::from(v4)), a.port()),
        IpAddr::V6(v6) => (1u8, u128::from(v6), a.port()),
    });
}

/// A live subscription to what is advertising on this network.
///
/// mdns-sd's `browse` is **continuous**: it keeps querying and reports records
/// as they resolve. The first version of this ran one for two seconds and then
/// shut it down, which fought the model in both directions — a record that had
/// not arrived yet counted as absent, and every click started from cold. A host
/// is therefore *expected* to take several seconds and several announcements to
/// appear fully, which is exactly what a fixed window cannot express.
///
/// Holding the subscription open while the list is on screen is both cheaper
/// and more accurate. Dropping it stops the queries.
pub struct Watcher {
    daemon: ServiceDaemon,
    rx: mdns_sd::Receiver<ServiceEvent>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

impl Watcher {
    /// Begin listening. Cheap — the querying happens on mdns-sd's own thread.
    pub fn start() -> Result<Self> {
        let daemon = ServiceDaemon::new()?;
        let rx = daemon.browse(SERVICE)?;
        Ok(Watcher { daemon, rx })
    }

    /// Everything resolved since the last call. Never blocks, so this is safe
    /// to call from a frame loop.
    pub fn poll(&self) -> Vec<Found> {
        let mut out = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            if let ServiceEvent::ServiceResolved(info) = event {
                if let Some(f) = resolve(&info) {
                    out.push(f);
                }
            }
        }
        out
    }
}

/// Turn a resolved record into something offerable, or nothing.
fn resolve(info: &ResolvedService) -> Option<Found> {
    let mut addrs: Vec<SocketAddr> = info
        .addresses
        .iter()
        .map(|a| a.to_ip_addr())
        .filter(usable)
        .map(|ip| SocketAddr::new(ip, info.get_port()))
        .collect();
    if addrs.is_empty() {
        return None;
    }
    sort_addrs(&mut addrs);
    Some(Found {
        name: instance_name(info.get_fullname()),
        addrs,
        machine_id: prop(info, KEY_MACHINE),
        version: prop(info, KEY_VERSION),
        needs_token: prop(info, KEY_TOKEN).as_deref() == Some("1"),
        claude_home: prop(info, KEY_HOME),
    })
}

/// One-shot browse for `window`, for scripts and the probe example.
///
/// The window uses [`Watcher`] instead: a deadline cannot tell "nothing is
/// there" from "nothing has answered yet", and in a UI that difference is the
/// whole experience.
pub fn browse(window: Duration) -> Result<Vec<Found>> {
    let watcher = Watcher::start()?;
    let deadline = std::time::Instant::now() + window;
    let mut found: Vec<Found> = Vec::new();
    while std::time::Instant::now() < deadline {
        for f in watcher.poll() {
            match found.iter_mut().find(|e| e.name == f.name) {
                Some(existing) => existing.absorb(f),
                None => found.push(f),
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
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

    /// The bug that made this row invisible on its first real network.
    ///
    /// `--listen 0.0.0.0` is how you run a daemon others can reach, and
    /// `0.0.0.0` is not somewhere anyone can connect to. Published verbatim it
    /// produced a record with one unspecified address, which the browse side
    /// then dropped as unusable — so the daemon advertised perfectly and could
    /// never be found.
    #[test]
    fn an_unspecified_bind_publishes_real_interfaces_not_the_wildcard() {
        let props = [("k", "v".to_string())];
        for wildcard in ["0.0.0.0:7717", "[::]:7717"] {
            let info = service_info(wildcard.parse().unwrap(), "devbox", "devbox.local.", &props)
                .expect("a wildcard bind must still advertise");
            assert!(
                info.is_addr_auto(),
                "{wildcard} must publish live interface addresses"
            );
            assert!(
                !info.get_addresses().iter().any(|a| a.is_unspecified()),
                "{wildcard} must never publish an address nobody can dial"
            );
        }
    }

    /// A specific bind is a choice, and republishing something else would be
    /// second-guessing it.
    #[test]
    fn a_specific_bind_advertises_exactly_that_address() {
        let props = [("k", "v".to_string())];
        let info = service_info("192.168.1.5:7717".parse().unwrap(), "d", "d.local.", &props)
            .expect("info");
        assert!(!info.is_addr_auto());
        assert!(info
            .get_addresses()
            .iter()
            .any(|a| a.to_string() == "192.168.1.5"));
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

    fn found(name: &str, addrs: &[&str]) -> Found {
        Found {
            name: name.into(),
            addrs: addrs.iter().map(|a| a.parse().unwrap()).collect(),
            machine_id: None,
            version: None,
            needs_token: true,
            claude_home: None,
        }
    }

    #[test]
    fn a_found_daemon_becomes_a_websocket_url() {
        let f = found("devbox", &["192.168.1.5:7717"]);
        assert_eq!(f.url().as_deref(), Some("ws://192.168.1.5:7717/ws"));
    }

    /// The flapping this replaced: a host answering on both families used to
    /// yield whichever address a `HashSet` happened to hand over first, so the
    /// same daemon appeared to move between scans. IPv4 wins because a v6
    /// literal in a URL needs brackets, and a link-local one needs a scope that
    /// will not survive being written down.
    #[test]
    fn a_dual_stack_daemon_always_offers_the_same_address() {
        let mut f = found("devbox", &["[2001:db8::5]:7717", "192.168.1.5:7717"]);
        sort_addrs(&mut f.addrs);
        assert_eq!(f.url().as_deref(), Some("ws://192.168.1.5:7717/ws"));

        // …and the reverse arrival order must give the same answer.
        let mut g = found("devbox", &["192.168.1.5:7717", "[2001:db8::5]:7717"]);
        sort_addrs(&mut g.addrs);
        assert_eq!(f.addrs, g.addrs);
    }

    /// Sightings arrive per interface and per family. Merging keeps them all;
    /// replacing was the other half of the flicker.
    #[test]
    fn a_second_sighting_adds_addresses_rather_than_replacing_them() {
        let mut f = found("devbox", &["192.168.1.5:7717"]);
        f.absorb(found("devbox", &["[2001:db8::5]:7717"]));
        assert_eq!(f.addrs.len(), 2);
        assert_eq!(f.url().as_deref(), Some("ws://192.168.1.5:7717/ws"), "v4 still first");

        // The same sighting twice must not grow the list.
        f.absorb(found("devbox", &["192.168.1.5:7717"]));
        assert_eq!(f.addrs.len(), 2, "addresses are a set, not a log");
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
            assert!(f.addrs.iter().all(|a| !a.ip().is_loopback()));
        }
    }
}
