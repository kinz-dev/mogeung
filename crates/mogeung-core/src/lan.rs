//! The LAN beacon: what a daemon says to its segment, and how a listener keeps
//! score. `R-I17`, [ADR-0039](../../../docs/decisions/0039-the-daemon-announces-itself-by-default.md).
//!
//! **Pure on purpose: no socket in this file.** The daemon sends and the shell
//! listens, and they share this so they cannot disagree about what a datagram
//! means — the reason `discovery.rs` gives for holding both mDNS halves in one
//! file, applied across the one crate boundary the shell actually has: it
//! depends on this crate and never on `mogeungd`.
//!
//! # What a beacon says, and what it never says
//!
//! Presence, a route, two counts. Never the token, never a session title,
//! never a path beyond the Claude home. ADR-0039 is the fence, and this codec
//! is where the fence is enforced: **there is no field for a secret to go in**,
//! and a test below pins that the word does not appear on the wire.
//!
//! # The address is not in the payload
//!
//! `R-I8` shipped `0.0.0.0` in a record and found it undialable. A datagram's
//! source address is the one address known to reach the sender, so the beacon
//! carries only a **port** and the listener pairs it with `recv_from`'s
//! answer. A multi-homed sender sends once per interface, from a socket bound
//! to that interface's address, so the source is right on each.
//!
//! # Three misses, not one
//!
//! A peer is *gone* when nothing has been heard for three intervals. One lost
//! datagram on Wi-Fi is the normal case, and a row that greys at sixteen
//! seconds and returns at thirty reads as a flaky network, not a flaky
//! network being reported honestly. Gone peers are kept and marked, never
//! dropped — the wifi-picker rule `R-I8` paid for twice.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// UDP. The API's own number, one protocol over — nothing to invent, and
/// nothing else on the machine answers UDP there.
pub const BEACON_PORT: u16 = 7717;

/// Between beacons. The ask said fifteen seconds; the jitter a sender adds
/// on top is its own business and is not part of the contract.
pub const INTERVAL: Duration = Duration::from_secs(15);

/// Missed beacons before a peer is *gone*.
pub const MISSES_BEFORE_GONE: u32 = 3;

/// The largest datagram produced or accepted. Under every common MTU, so a
/// beacon is never fragmented — and a cap on what a stranger can make a
/// listener parse.
pub const MAX_BYTES: usize = 1200;

/// Bumped when a field changes meaning. A listener ignores any other.
pub const VERSION: u16 = 1;

/// How the daemon behind a beacon can be reached. **Presence is not access**:
/// a beacon says a daemon exists whatever it is bound to, and this says what
/// a hand can do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reach {
    /// Bound beyond loopback — and therefore token-gated (`R-I10`). *Connect*
    /// is one press and a typed token.
    Lan,
    /// Loopback-bound, but the daemon knows how it is reached over ssh
    /// (`ssh_target` in its config). *Connect* is an `ssh -L` you run.
    Ssh,
    /// Loopback-bound and nothing configured. It exists, and that is all the
    /// segment gets to know.
    Local,
}

/// One datagram's worth of presence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Beacon {
    /// [`VERSION`].
    pub v: u16,
    /// The daemon's machine id — the key a listener files it under, so a host
    /// that changes address is one row moving rather than two rows. Empty when
    /// the daemon has none, in which case the listener keys on the address.
    pub machine: String,
    /// For people. Never compared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The API's port. Paired with the datagram's source address, never with
    /// anything in here.
    pub port: u16,
    /// The daemon's build.
    pub version: String,
    pub reach: Reach,
    /// The ssh target, when `reach` is [`Reach::Ssh`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<String>,
    /// The Claude home it watches. Two daemons on one machine are two worlds.
    pub home: String,
    /// Sessions alive.
    pub live: u32,
    /// Sessions in the queue's *needs you* — the same predicate, so the count
    /// on a found row means what the tab means.
    pub waiting: u32,
    /// Counts up per beacon sent. Lets a listener tell a lost datagram from a
    /// late one.
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// Refused before parsing: the size alone rules it out.
    TooBig(usize),
    /// Not JSON, or not this shape.
    NotJson,
    /// JSON with a `v` this listener does not speak.
    WrongVersion(u64),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::TooBig(n) => write!(f, "{n} bytes, and a beacon is at most {MAX_BYTES}"),
            CodecError::NotJson => write!(f, "not a beacon"),
            CodecError::WrongVersion(v) => write!(f, "beacon version {v}, and this listener speaks {VERSION}"),
        }
    }
}

impl std::error::Error for CodecError {}

/// One line of JSON, under [`MAX_BYTES`] or refused.
pub fn encode(b: &Beacon) -> Result<Vec<u8>, CodecError> {
    let bytes = serde_json::to_vec(b).map_err(|_| CodecError::NotJson)?;
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::TooBig(bytes.len()));
    }
    Ok(bytes)
}

/// The version is read before the shape, so a newer beacon is reported as
/// *newer* rather than as *garbage* — the difference between "upgrade" and
/// "something is wrong". Unknown fields are ignored: a beacon that has
/// learnt a new word still says the old ones.
pub fn decode(bytes: &[u8]) -> Result<Beacon, CodecError> {
    if bytes.len() > MAX_BYTES {
        return Err(CodecError::TooBig(bytes.len()));
    }
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| CodecError::NotJson)?;
    let v = value.get("v").and_then(|v| v.as_u64()).ok_or(CodecError::NotJson)?;
    if v != u64::from(VERSION) {
        return Err(CodecError::WrongVersion(v));
    }
    serde_json::from_value(value).map_err(|_| CodecError::NotJson)
}

/// What a row in the table is about. Two Claude homes on one machine are two
/// daemons, and two rows.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerKey {
    pub machine: String,
    pub home: String,
}

impl PeerKey {
    /// A beacon with no machine id is keyed on where it came from — worse
    /// than an id, since a host that moves becomes two rows, and the best
    /// there is.
    pub fn of(from: IpAddr, b: &Beacon) -> PeerKey {
        let machine = if b.machine.is_empty() {
            format!("addr:{from}")
        } else {
            b.machine.clone()
        };
        PeerKey {
            machine,
            home: b.home.clone(),
        }
    }
}

/// A daemon the table has heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub key: PeerKey,
    /// The latest beacon, whole.
    pub beacon: Beacon,
    /// Every source address it has been heard from, **in a stable order** —
    /// IPv4 before IPv6, then numerically. The order is part of the contract
    /// for the reason `discovery::Found` gives: a list that reorders between
    /// draws reads as a host that keeps moving.
    pub addrs: Vec<IpAddr>,
    pub first_seen: Instant,
    pub last_seen: Instant,
    /// Datagrams heard. A multi-homed sender is heard once per interface, so
    /// this counts arrivals, not beacons.
    pub heard: u64,
    /// **Beacons** heard — distinct sequence numbers. The number a probe
    /// compares against how many were sent.
    pub beacons: u64,
    /// The sequence number last heard, which is how a duplicate is told from
    /// the next beacon.
    pub last_seq: u64,
    /// The longest silence between two beacons, for the same reason.
    pub longest_gap: Duration,
    /// Three intervals without a word. Kept and marked, never dropped.
    pub gone: bool,
}

impl Peer {
    /// The address a client would dial: the first, by the order above.
    pub fn addr(&self) -> Option<IpAddr> {
        self.addrs.first().copied()
    }
}

/// What hearing a beacon meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    /// Never heard before.
    New,
    /// Heard and already listed.
    Again,
    /// The same beacon, from another of the sender's addresses — a
    /// multi-homed host sends once per interface. Listed, merged, not counted
    /// as a second beacon.
    Duplicate,
    /// Had been marked gone, and is not.
    Back,
}

/// Everything heard, keyed so that a host changing address stays one row.
#[derive(Debug, Default, Clone)]
pub struct PeerTable {
    peers: BTreeMap<PeerKey, Peer>,
}

impl PeerTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// A beacon arrived from `from` at `now`.
    pub fn heard(&mut self, from: IpAddr, beacon: Beacon, now: Instant) -> Heard {
        let key = PeerKey::of(from, &beacon);
        match self.peers.get_mut(&key) {
            Some(p) => {
                let duplicate = beacon.seq == p.last_seq;
                if !duplicate {
                    let gap = now.saturating_duration_since(p.last_seen);
                    if gap > p.longest_gap {
                        p.longest_gap = gap;
                    }
                    p.beacons += 1;
                    p.last_seq = beacon.seq;
                }
                p.last_seen = now;
                p.heard += 1;
                p.beacon = beacon;
                insert_addr(&mut p.addrs, from);
                if p.gone {
                    p.gone = false;
                    Heard::Back
                } else if duplicate {
                    Heard::Duplicate
                } else {
                    Heard::Again
                }
            }
            None => {
                let beacon_seq = beacon.seq;
                self.peers.insert(
                    key.clone(),
                    Peer {
                        key,
                        beacon,
                        addrs: vec![from],
                        first_seen: now,
                        last_seen: now,
                        heard: 1,
                        beacons: 1,
                        last_seq: beacon_seq,
                        longest_gap: Duration::ZERO,
                        gone: false,
                    },
                );
                Heard::New
            }
        }
    }

    /// Mark what has fallen silent. Returns the peers that became gone on
    /// **this** sweep, so a caller can say so once rather than every second.
    pub fn sweep(&mut self, now: Instant) -> Vec<PeerKey> {
        let limit = INTERVAL * MISSES_BEFORE_GONE;
        let mut newly = Vec::new();
        for p in self.peers.values_mut() {
            if !p.gone && now.saturating_duration_since(p.last_seen) > limit {
                p.gone = true;
                newly.push(p.key.clone());
            }
        }
        newly
    }

    pub fn peers(&self) -> impl Iterator<Item = &Peer> {
        self.peers.values()
    }

    pub fn get(&self, key: &PeerKey) -> Option<&Peer> {
        self.peers.get(key)
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

fn insert_addr(addrs: &mut Vec<IpAddr>, ip: IpAddr) {
    if addrs.contains(&ip) {
        return;
    }
    addrs.push(ip);
    addrs.sort_by_key(|a| (a.is_ipv6(), *a));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn beacon(machine: &str, home: &str, seq: u64) -> Beacon {
        Beacon {
            v: VERSION,
            machine: machine.into(),
            host: Some("devbox".into()),
            port: 7717,
            version: "0.9.3".into(),
            reach: Reach::Lan,
            ssh: None,
            home: home.into(),
            live: 3,
            waiting: 1,
            seq,
        }
    }

    fn v4(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, last))
    }

    #[test]
    fn a_beacon_survives_the_wire() {
        let b = beacon("abc", "/home/k/.claude", 7);
        let bytes = encode(&b).unwrap();
        assert!(bytes.len() < MAX_BYTES);
        assert_eq!(decode(&bytes).unwrap(), b);
    }

    /// ADR-0039's fence, pinned at the codec: not only is there no token
    /// field, the word does not appear on the wire at all.
    #[test]
    fn a_beacon_has_nowhere_to_put_a_token() {
        let text = String::from_utf8(encode(&beacon("abc", "/h", 1)).unwrap()).unwrap();
        assert!(!text.contains("token"), "{text}");
    }

    #[test]
    fn an_oversized_beacon_is_refused_before_it_is_read() {
        let mut b = beacon("abc", "/h", 1);
        b.home = "x".repeat(MAX_BYTES);
        assert!(matches!(encode(&b), Err(CodecError::TooBig(_))));
        let big = vec![b'{'; MAX_BYTES + 1];
        assert_eq!(decode(&big), Err(CodecError::TooBig(MAX_BYTES + 1)));
    }

    /// A newer beacon is "newer", which tells you to upgrade; garbage is
    /// "garbage", which tells you something else is on the port.
    #[test]
    fn the_version_is_read_before_the_shape() {
        let newer = br#"{"v":2,"something":"this listener has never seen"}"#;
        assert_eq!(decode(newer), Err(CodecError::WrongVersion(2)));
        assert_eq!(decode(b"hello"), Err(CodecError::NotJson));
        assert_eq!(decode(br#"{"no":"version"}"#), Err(CodecError::NotJson));
    }

    #[test]
    fn a_field_this_build_has_not_learnt_is_ignored() {
        let mut value = serde_json::to_value(beacon("abc", "/h", 1)).unwrap();
        value["colour"] = serde_json::json!("teal");
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(decode(&bytes).unwrap().machine, "abc");
    }

    #[test]
    fn a_peer_is_listed_under_the_address_it_came_from() {
        let mut t = PeerTable::new();
        let now = Instant::now();
        assert_eq!(t.heard(v4(30), beacon("abc", "/h", 1), now), Heard::New);
        let p = t.peers().next().unwrap();
        assert_eq!(p.addr(), Some(v4(30)));
        assert_eq!(p.heard, 1);
        assert_eq!(t.heard(v4(30), beacon("abc", "/h", 2), now + INTERVAL), Heard::Again);
        assert_eq!(t.len(), 1);
        assert_eq!(t.peers().next().unwrap().beacon.seq, 2);
    }

    #[test]
    fn three_misses_is_gone_and_one_more_beacon_is_back() {
        let mut t = PeerTable::new();
        let t0 = Instant::now();
        t.heard(v4(30), beacon("abc", "/h", 1), t0);
        // Two intervals of silence is still listening.
        assert!(t.sweep(t0 + INTERVAL * 2).is_empty());
        assert!(!t.peers().next().unwrap().gone);
        // Past the third, gone — reported once, and kept.
        let late = t0 + INTERVAL * MISSES_BEFORE_GONE + Duration::from_secs(1);
        assert_eq!(t.sweep(late), vec![PeerKey { machine: "abc".into(), home: "/h".into() }]);
        assert!(t.sweep(late + Duration::from_secs(1)).is_empty());
        assert!(t.peers().next().unwrap().gone);
        assert_eq!(t.len(), 1);
        // Back.
        assert_eq!(t.heard(v4(30), beacon("abc", "/h", 2), late + INTERVAL), Heard::Back);
        assert!(!t.peers().next().unwrap().gone);
    }

    #[test]
    fn two_homes_on_one_machine_are_two_rows() {
        let mut t = PeerTable::new();
        let now = Instant::now();
        t.heard(v4(30), beacon("abc", "/home/a/.claude", 1), now);
        t.heard(v4(30), beacon("abc", "/home/b/.claude", 1), now);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn a_host_heard_on_two_addresses_is_one_row_in_a_stable_order() {
        let mut t = PeerTable::new();
        let now = Instant::now();
        let v6 = IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1));
        t.heard(v6, beacon("abc", "/h", 1), now);
        t.heard(v4(30), beacon("abc", "/h", 2), now);
        t.heard(v4(20), beacon("abc", "/h", 3), now);
        let p = t.peers().next().unwrap();
        assert_eq!(p.addrs, vec![v4(20), v4(30), v6]);
        assert_eq!(p.addr(), Some(v4(20)));
    }

    /// A host with two interfaces sends every beacon twice. Two arrivals, one
    /// beacon — or a probe would report 200% delivery on a multi-homed sender.
    #[test]
    fn the_same_beacon_from_a_second_address_is_a_duplicate_not_a_second_beacon() {
        let mut t = PeerTable::new();
        let now = Instant::now();
        assert_eq!(t.heard(v4(30), beacon("abc", "/h", 1), now), Heard::New);
        assert_eq!(t.heard(v4(31), beacon("abc", "/h", 1), now), Heard::Duplicate);
        assert_eq!(t.heard(v4(30), beacon("abc", "/h", 2), now + INTERVAL), Heard::Again);
        let p = t.peers().next().unwrap();
        assert_eq!((p.heard, p.beacons), (3, 2));
        assert_eq!(p.addrs, vec![v4(30), v4(31)]);
    }

    #[test]
    fn a_beacon_with_no_machine_id_is_keyed_on_its_address() {
        let mut t = PeerTable::new();
        let now = Instant::now();
        t.heard(v4(30), beacon("", "/h", 1), now);
        t.heard(v4(31), beacon("", "/h", 1), now);
        assert_eq!(t.len(), 2);
        assert_eq!(t.peers().next().unwrap().key.machine, "addr:10.0.0.30");
    }

    #[test]
    fn the_longest_gap_is_kept_for_the_probe() {
        let mut t = PeerTable::new();
        let t0 = Instant::now();
        t.heard(v4(30), beacon("abc", "/h", 1), t0);
        t.heard(v4(30), beacon("abc", "/h", 2), t0 + Duration::from_secs(15));
        t.heard(v4(30), beacon("abc", "/h", 3), t0 + Duration::from_secs(50));
        assert_eq!(t.peers().next().unwrap().longest_gap, Duration::from_secs(35));
    }
}
