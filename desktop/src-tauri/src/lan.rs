//! Hearing beacons. `R-I17` phase 1, step 5.
//!
//! The daemon sends (`mogeungd::lan`); **the shell listens**, for the reason
//! [ADR-0036](../../../docs/decisions/0036-the-connection-list-is-the-clients-and-its-file-is-the-shells.md)
//! gave the connection list to the shell: the Found list is about the LAN
//! *this window* is on, and you cannot ask a remote daemon who its neighbours
//! are and get your own. A browser tab has no UDP and says so.
//!
//! Listening is passive and discloses nothing, so it starts with the app and
//! runs for its life; a window that opens the Connections panel asks for the
//! table as it stands (`lan_peers`) and then follows `lan:peers`, which is
//! emitted on every beacon heard — not every datagram: a burst and a
//! multi-homed sender arrive several times per beacon and are folded by the
//! table — and on every peer that goes quiet.
//!
//! What a beacon means, and when a peer is *gone*, is `mogeung_core::lan`,
//! shared with the daemon so the two ends cannot disagree. The address a row
//! dials is the datagram's source, never anything the payload claimed.

use mogeung_core::lan::{self, Heard, Peer, PeerTable, Reach, BEACON_PORT};
use serde::Serialize;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// A found daemon, as the window sees it. The table's `Instant`s become an
/// epoch stamp so the window can say *12 s ago* on its own clock.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoundDaemon {
    pub machine: String,
    pub home: String,
    pub host: Option<String>,
    /// The address to dial: the first of `addrs`, in the table's stable order.
    pub addr: String,
    pub addrs: Vec<String>,
    pub port: u16,
    pub version: String,
    pub reach: Reach,
    pub ssh: Option<String>,
    pub live: u32,
    pub waiting: u32,
    /// Unix epoch, milliseconds.
    pub last_seen_ms: u64,
    pub beacons: u64,
    pub gone: bool,
    /// This machine's own daemon — recognised, marked, not offered back.
    pub mine: bool,
}

/// The listener's state, managed by Tauri for the life of the app.
pub struct Lan {
    table: Mutex<PeerTable>,
    /// `~/.mogeung/machine-id`, read once, so a row can say *this machine*.
    me: Option<String>,
    /// Why there is no listener, when there is none — the window shows it.
    error: Mutex<Option<String>>,
}

impl Lan {
    pub fn new() -> Self {
        Lan {
            table: Mutex::new(PeerTable::new()),
            me: read_machine_id(),
            error: Mutex::new(None),
        }
    }

    /// Every daemon heard, gone ones included and marked.
    pub fn snapshot(&self) -> Vec<FoundDaemon> {
        let table = self.table.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let now_ms = epoch_ms();
        table.peers().map(|p| view(p, self.me.as_deref(), now, now_ms)).collect()
    }
}

impl Default for Lan {
    fn default() -> Self {
        Self::new()
    }
}

fn read_machine_id() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    std::fs::read_to_string(format!("{home}/.mogeung/machine-id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// One row, from one peer.
pub fn view(p: &Peer, me: Option<&str>, now: Instant, now_ms: u64) -> FoundDaemon {
    let ago = now.saturating_duration_since(p.last_seen).as_millis() as u64;
    FoundDaemon {
        machine: p.key.machine.clone(),
        home: p.key.home.clone(),
        host: p.beacon.host.clone(),
        addr: p.addr().map(|a| a.to_string()).unwrap_or_default(),
        addrs: p.addrs.iter().map(|a| a.to_string()).collect(),
        port: p.beacon.port,
        version: p.beacon.version.clone(),
        reach: p.beacon.reach,
        ssh: p.beacon.ssh.clone(),
        live: p.beacon.live,
        waiting: p.beacon.waiting,
        last_seen_ms: now_ms.saturating_sub(ago),
        beacons: p.beacons,
        gone: p.gone,
        mine: me.is_some_and(|m| m == p.key.machine),
    }
}

/// A datagram arrived: fold it into the table, and say whether the window
/// should hear about it. A duplicate — the second of a burst, or the same
/// beacon by another of the sender's interfaces — changes nothing it shows.
pub fn hear(table: &mut PeerTable, from: IpAddr, bytes: &[u8], now: Instant) -> bool {
    match lan::decode(bytes) {
        Ok(b) => !matches!(table.heard(from, b, now), Heard::Duplicate),
        Err(_) => false,
    }
}

/// The current table, for a window that has just opened the panel.
#[tauri::command]
pub fn lan_peers(state: tauri::State<'_, Arc<Lan>>) -> Result<Vec<FoundDaemon>, String> {
    if let Some(e) = state.error.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Err(e);
    }
    Ok(state.snapshot())
}

/// One UDP port, shared: the probe, a second window and the daemon's own
/// listener-to-be all bind it, and a sender on this host is heard by its own
/// listener. `SO_REUSEADDR` lets broadcast reach every socket on the port on
/// Linux; `SO_REUSEPORT` is the same permission on macOS.
fn bind_shared(port: u16) -> std::io::Result<UdpSocket> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    #[cfg(unix)]
    s.set_reuse_port(true)?;
    s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())?;
    s.set_read_timeout(Some(Duration::from_millis(500)))?;
    Ok(s.into())
}

/// Start listening, for the life of the app. A bind that fails is recorded
/// for the window to show rather than being fatal: a Connections panel that
/// says *not listening — port in use* beats a shell that would not start.
pub fn start(app: AppHandle, lan: Arc<Lan>) {
    let sock = match bind_shared(BEACON_PORT) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("not listening for beacons on udp/{BEACON_PORT}: {e}");
            eprintln!("{msg}");
            *lan.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(msg);
            return;
        }
    };
    std::thread::Builder::new()
        .name("lan-listener".into())
        .spawn(move || {
            let mut buf = [0u8; 2048];
            let mut last_sweep = Instant::now();
            loop {
                let mut changed = false;
                match sock.recv_from(&mut buf) {
                    Ok((n, src)) => {
                        let mut table = lan.table.lock().unwrap_or_else(|e| e.into_inner());
                        changed = hear(&mut table, src.ip(), &buf[..n], Instant::now());
                    }
                    Err(e) if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                    Err(e) => {
                        eprintln!("beacon listener stopped: {e}");
                        *lan.error.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(format!("the beacon listener stopped: {e}"));
                        return;
                    }
                }
                if last_sweep.elapsed() >= Duration::from_secs(1) {
                    last_sweep = Instant::now();
                    let mut table = lan.table.lock().unwrap_or_else(|e| e.into_inner());
                    if !table.sweep(Instant::now()).is_empty() {
                        changed = true;
                    }
                }
                if changed {
                    let _ = app.emit("lan:peers", lan.snapshot());
                }
            }
        })
        .expect("spawning the beacon listener");
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::lan::{encode, Beacon, VERSION};

    fn beacon(seq: u64, live: u32) -> Vec<u8> {
        encode(&Beacon {
            v: VERSION,
            machine: "abc123".into(),
            host: Some("devbox".into()),
            port: 7717,
            version: "0.1.0".into(),
            reach: Reach::Lan,
            ssh: None,
            home: "/home/k/.claude".into(),
            live,
            waiting: 1,
            seq,
        })
        .unwrap()
    }

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, last))
    }

    /// The emit rule: a beacon is news, its second datagram is not, garbage
    /// is nothing at all.
    #[test]
    fn a_beacon_is_news_and_its_duplicate_and_garbage_are_not() {
        let mut t = PeerTable::new();
        let now = Instant::now();
        assert!(hear(&mut t, ip(30), &beacon(1, 2), now));
        assert!(!hear(&mut t, ip(30), &beacon(1, 2), now), "the burst's second datagram");
        assert!(!hear(&mut t, ip(31), &beacon(1, 2), now), "the same beacon by another interface");
        assert!(hear(&mut t, ip(30), &beacon(2, 3), now), "the next beacon");
        assert!(!hear(&mut t, ip(30), b"not a beacon", now));
        assert_eq!(t.len(), 1);
    }

    /// The row the window draws: dialled by the datagram's source, stamped on
    /// the wall clock, and marked as this machine's when it is.
    #[test]
    fn a_row_dials_the_source_and_knows_its_own_machine() {
        let mut t = PeerTable::new();
        let now = Instant::now();
        hear(&mut t, ip(30), &beacon(1, 2), now);
        let p = t.peers().next().unwrap();
        let row = view(p, Some("abc123"), now + Duration::from_secs(12), 1_000_000);
        assert_eq!(row.addr, "10.0.0.30");
        assert_eq!((row.port, row.live, row.waiting, row.beacons), (7717, 2, 1, 1));
        assert_eq!(row.last_seen_ms, 1_000_000 - 12_000);
        assert!(row.mine);
        assert!(!row.gone);
        let other = view(p, Some("someone-else"), now, 5);
        assert!(!other.mine);
    }
}
