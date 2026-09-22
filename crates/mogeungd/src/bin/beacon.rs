//! The phase-0 probe for `A44`: does a broadcast beacon cross the mesh? `R-I17`,
//! [feature 0046](../../../../docs/features/0046-found-on-the-lan.md).
//!
//! `A44` — *a UDP datagram to the subnet's directed broadcast address crosses
//! an Orbi mesh reliably enough for a beacon every 15 s, where mDNS multicast
//! may not* — is `UNTESTED`, and the ledger's rule is that the work is to test
//! it, not to build on it. This is the test: the beacon's own codec and table
//! (`mogeung_core::lan`), real sockets, and mDNS browsed beside it so one run
//! names both transports in its verdict.
//!
//! ```sh
//! # on every host that should be found — its own addresses, every 15 s, until Ctrl-C
//! cargo run -q -p mogeungd --bin beacon -- send
//! # on the host doing the finding — ten minutes, then a summary
//! cargo run -q -p mogeungd --bin beacon -- listen --for 600
//! # one machine, a quick turn, to see the pieces move
//! cargo run -q -p mogeungd --bin beacon -- listen --for 12 --interval 2 &
//! cargo run -q -p mogeungd --bin beacon -- send --interval 2 --count 4
//! ```
//!
//! # What the summary answers
//!
//! Per peer: **heard / expected**, where expected is the run's length over the
//! sender's interval, and the longest silence between two beacons. Per mDNS
//! host: whether it resolved at all and how long that took — mDNS answers a
//! query rather than beaconing, so it has no count to compare, only a yes and
//! a latency. A sender on the listening machine is heard too and marked
//! *this machine*: if you cannot hear yourself, the socket is wrong before the
//! network is.
//!
//! Directed broadcast (`10.0.0.255`) is the transport under test. `--limited`
//! sends to `255.255.255.255` as well, for the run where the directed address
//! fails and the question becomes whether the other spelling does better.
//!
//! # Kept, not thrown away
//!
//! `R-J28`'s rule: a finding that depends on somebody writing a script is a
//! finding you get once. This stays as the operator's diagnostic — the thing
//! to run when a Found list is empty and the question is whether the network
//! or the daemon is at fault.

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use mogeung_core::lan::{self, Beacon, CodecError, Heard, PeerTable, BEACON_PORT};
use mogeungd::lan::{beacon_of, send_to_all, Target};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(about = "the A44 probe: send a beacon to the LAN's broadcast addresses, or listen for them")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Announce this machine, once per interval, to every interface's broadcast address.
    Send {
        /// Seconds between beacons. The real beacon uses 15; a smoke test wants less.
        #[arg(long, default_value_t = 15)]
        interval: u64,
        /// Stop after this many. Default: until Ctrl-C.
        #[arg(long)]
        count: Option<u64>,
        /// The UDP port to send to.
        #[arg(long, default_value_t = BEACON_PORT)]
        port: u16,
        /// Also send to 255.255.255.255, for a run where the directed address fails.
        #[arg(long)]
        limited: bool,
        /// Do not advertise over mDNS as well. By default `send` announces on
        /// both transports, so one `listen` compares them from the same host.
        #[arg(long)]
        no_mdns: bool,
    },
    /// Listen for beacons, browse mDNS beside them, and report both at the end.
    Listen {
        /// Seconds to run. Ctrl-C ends it early; the summary prints either way.
        #[arg(long = "for", default_value_t = 600)]
        seconds: u64,
        /// The interval the senders are using, so the summary can say how many were expected.
        #[arg(long, default_value_t = 15)]
        interval: u64,
        /// The UDP port to listen on.
        #[arg(long, default_value_t = BEACON_PORT)]
        port: u16,
    },
}

fn main() -> Result<()> {
    match Args::parse().cmd {
        Cmd::Send {
            interval,
            count,
            port,
            limited,
            no_mdns,
        } => send(interval, count, port, limited, no_mdns),
        Cmd::Listen {
            seconds,
            interval,
            port,
        } => listen(seconds, interval, port),
    }
}

/// Seconds since the run began, the way every line here is stamped: a probe
/// log is read as "what happened at +45 s", not by the clock on the wall.
fn stamp(start: Instant) -> String {
    format!("+{:.1}s", start.elapsed().as_secs_f64())
}

// ---------------------------------------------------------------------------
// send

/// The daemon's own list (`mogeungd::lan::targets`), plus the limited
/// broadcast when asked — the one address the daemon never sends to.
fn targets(limited: bool) -> Result<Vec<Target>> {
    let mut out = mogeungd::lan::targets().context("listing interfaces")?;
    if limited {
        out.push(Target {
            iface: "limited".into(),
            ip: Ipv4Addr::UNSPECIFIED,
            bcast: Ipv4Addr::BROADCAST,
        });
    }
    Ok(out)
}

/// The identity the daemon would publish, and the config it would read.
fn who() -> (mogeung_core::wire::DaemonIdentity, mogeung_core::config::Config) {
    let (cfg, warning) = mogeung_core::config::Config::load();
    if let Some(w) = warning {
        eprintln!("{w}");
    }
    let identity = mogeungd::machine::identity(&mogeungd::watcher::default_home(), cfg.ssh_target.clone());
    (identity, cfg)
}

/// What the real beacon will say, from the same identity the daemon
/// publishes and the same rule for `reach` — `mogeungd::lan::beacon_of`,
/// against the bind the config file names. The counts are zero because a
/// probe has no board.
fn beacon(identity: &mogeung_core::wire::DaemonIdentity, cfg: &mogeung_core::config::Config, seq: u64) -> Beacon {
    let bind: SocketAddr = cfg
        .listen
        .as_deref()
        .and_then(|l| l.parse().ok())
        .unwrap_or_else(|| SocketAddr::from((Ipv4Addr::LOCALHOST, 7717)));
    beacon_of(identity, &bind, 0, 0, seq)
}

fn send(interval: u64, count: Option<u64>, port: u16, limited: bool, no_mdns: bool) -> Result<()> {
    let targets = targets(limited)?;
    if targets.is_empty() {
        bail!("no interface with a broadcast address — nothing to send to");
    }
    let (identity, cfg) = who();
    let first = beacon(&identity, &cfg, 1);
    println!(
        "sending as {} (machine {}, reach {:?}, api port {}) every {interval} s to {} target(s):",
        first.host.as_deref().unwrap_or("?"),
        short(&first.machine),
        first.reach,
        first.port,
        targets.len()
    );
    for t in &targets {
        println!("  {:<14} {} -> {}:{port}", t.iface, t.ip, t.bcast);
    }
    // The other transport, from the same host, so the listener's two columns
    // describe one sender. Held for the run: dropping it withdraws the record.
    let _advert = if no_mdns {
        None
    } else {
        let at = SocketAddr::from((targets[0].ip, first.port));
        match mogeungd::discovery::advertise(at, &identity, cfg.token.is_some()) {
            Ok(a) => {
                println!("  and advertising _mogeung._tcp over mDNS at {at}");
                Some(a)
            }
            Err(e) => {
                println!("  mdns: not advertising — {e}");
                None
            }
        }
    };
    let start = Instant::now();
    let mut failed: BTreeSet<String> = BTreeSet::new();
    let mut seq = 0u64;
    loop {
        seq += 1;
        let b = if seq == 1 { first.clone() } else { beacon(&identity, &cfg, seq) };
        let bytes = lan::encode(&b)?;
        let errors = send_to_all(&targets, port, &bytes);
        let sent = targets.len() - errors.len();
        // Once per interface: a down bridge fails the same way every time,
        // and a log that says so every 15 s says nothing else.
        for (iface, e) in errors {
            if failed.insert(iface.clone()) {
                println!("{:>8}  not via {iface}: {e}", stamp(start));
            }
        }
        println!("{:>8}  sent seq={seq} on {sent} interface(s) ({} bytes)", stamp(start), bytes.len());
        if count.is_some_and(|c| seq >= c) {
            break;
        }
        std::thread::sleep(Duration::from_secs(interval));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// listen

/// One UDP port, shared: two windows on one machine both listen, and a sender
/// on the same host is heard by its own listener. `SO_REUSEADDR` is what lets
/// broadcast datagrams reach every socket bound to the port on Linux;
/// `SO_REUSEPORT` is the same permission on macOS.
fn bind_shared(port: u16) -> Result<UdpSocket> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).context("udp socket")?;
    s.set_reuse_address(true)?;
    #[cfg(unix)]
    s.set_reuse_port(true)?;
    s.set_broadcast(true)?;
    s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())
        .with_context(|| format!("binding 0.0.0.0:{port}/udp"))?;
    Ok(s.into())
}

/// How many beacons a sender at `interval` had time to send in `elapsed`.
/// Approximate by one — the first arrives within an interval, not at zero —
/// and never zero, since a ratio over zero says nothing.
fn expected(elapsed: Duration, interval: u64) -> u64 {
    (elapsed.as_secs() / interval.max(1)).max(1)
}

fn short(machine: &str) -> &str {
    if machine.is_empty() {
        "none"
    } else {
        &machine[..machine.len().min(8)]
    }
}

fn addrs_str(addrs: &[SocketAddr]) -> String {
    addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ")
}

struct MdnsSeen {
    first: Duration,
    addrs: BTreeSet<SocketAddr>,
    resolutions: u64,
}

fn listen(seconds: u64, interval: u64, port: u16) -> Result<()> {
    let sock = bind_shared(port)?;
    sock.set_read_timeout(Some(Duration::from_millis(500)))?;
    let me = mogeungd::machine::machine_id().filter(|m| !m.is_empty());
    let watcher = match mogeungd::discovery::Watcher::start() {
        Ok(w) => Some(w),
        Err(e) => {
            println!("mdns: not browsing — {e}");
            None
        }
    };
    println!(
        "listening on 0.0.0.0:{port}/udp (reuseaddr, reuseport){} for {seconds} s — Ctrl-C ends it early; the summary prints either way",
        if watcher.is_some() {
            ", and browsing _mogeung._tcp beside it"
        } else {
            ""
        }
    );

    // The clock and the hand: a deadline, or Ctrl-C, whichever is first.
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        std::thread::spawn(move || {
            rt.block_on(async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => println!("\nCtrl-C"),
                    _ = tokio::time::sleep(Duration::from_secs(seconds)) => {}
                }
            });
            stop.store(true, Ordering::SeqCst);
        });
    }

    let start = Instant::now();
    let mut table = PeerTable::new();
    let mut mdns: BTreeMap<String, MdnsSeen> = BTreeMap::new();
    let mut complained: BTreeSet<(IpAddr, &'static str)> = BTreeSet::new();
    let mut ignored = 0u64;
    let mut buf = [0u8; 2048];

    while !stop.load(Ordering::SeqCst) {
        match sock.recv_from(&mut buf) {
            Ok((n, src)) => {
                let now = Instant::now();
                match lan::decode(&buf[..n]) {
                    Ok(b) => {
                        let mine = me.as_deref().is_some_and(|m| m == b.machine);
                        let heard = table.heard(src.ip(), b.clone(), now);
                        let tag = match heard {
                            Heard::New => "NEW  ",
                            Heard::Back => "BACK ",
                            Heard::Again => "heard",
                            // Merged into the row; a second line would count
                            // one beacon twice in the reader's head too.
                            Heard::Duplicate => continue,
                        };
                        println!(
                            "{:>8}  udp   {tag} {}{}  from {src}  port={} reach={:?} live={} waiting={} seq={}",
                            stamp(start),
                            b.host.as_deref().unwrap_or("?"),
                            if mine { " (this machine)" } else { "" },
                            b.port,
                            b.reach,
                            b.live,
                            b.waiting,
                            b.seq
                        );
                    }
                    Err(e) => {
                        ignored += 1;
                        let kind = match e {
                            CodecError::WrongVersion(_) => "version",
                            CodecError::TooBig(_) => "size",
                            CodecError::NotJson => "garbage",
                        };
                        if complained.insert((src.ip(), kind)) {
                            println!("{:>8}  udp   ignored from {src}: {e}", stamp(start));
                        }
                    }
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(e).context("receiving"),
        }
        for k in table.sweep(Instant::now()) {
            if let Some(p) = table.get(&k) {
                println!(
                    "{:>8}  udp   GONE  {} — nothing for {} s",
                    stamp(start),
                    p.beacon.host.as_deref().unwrap_or(&k.machine),
                    p.last_seen.elapsed().as_secs()
                );
            }
        }
        if let Some(w) = &watcher {
            for f in w.poll() {
                let entry = mdns.entry(f.name.clone()).or_insert_with(|| {
                    println!(
                        "{:>8}  mdns  RESOLVED {} at {} (machine {})",
                        stamp(start),
                        f.name,
                        addrs_str(&f.addrs),
                        f.machine_id.as_deref().map(short).unwrap_or("?")
                    );
                    MdnsSeen {
                        first: start.elapsed(),
                        addrs: BTreeSet::new(),
                        resolutions: 0,
                    }
                });
                entry.resolutions += 1;
                for a in &f.addrs {
                    if entry.addrs.insert(*a) && entry.resolutions > 1 {
                        println!("{:>8}  mdns  {} also at {a}", stamp(start), f.name);
                    }
                }
            }
        }
    }

    // The summary — the numbers A44's verdict is written from.
    let elapsed = start.elapsed();
    let want = expected(elapsed, interval);
    println!(
        "\n== {} s.  udp: {} peer(s); about {} beacon(s) expected from each at {} s; {} datagram(s) ignored ==",
        elapsed.as_secs(),
        table.len(),
        want,
        interval,
        ignored
    );
    for p in table.peers() {
        // Per address, so a host on a wire and a radio at once says which
        // one delivered — the leg the verdict is about.
        let addrs = p
            .addrs
            .iter()
            .map(|a| format!("{a} ×{}", p.heard_by_addr.get(a).copied().unwrap_or(0)))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {:<18} {:>3}/{:<3} beacons ({} datagram(s))   longest gap {:>4} s   {}   machine {}   from {}   api port {}   reach {:?}   v{}",
            p.beacon.host.as_deref().unwrap_or("?"),
            p.beacons,
            want,
            p.heard,
            p.longest_gap.as_secs(),
            if p.gone { "GONE" } else { "live" },
            short(&p.key.machine),
            addrs,
            p.beacon.port,
            p.beacon.reach,
            p.beacon.version
        );
    }
    if table.is_empty() {
        println!("  nothing heard — if a sender was running on this segment, the network dropped every beacon, and A44 fails here");
    }
    println!("== mdns: {} host(s) resolved ==", mdns.len());
    for (name, m) in &mdns {
        let addrs = m.addrs.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(", ");
        println!(
            "  {:<18} resolved after {:>4} s   {} resolution(s)   at {addrs}",
            name,
            m.first.as_secs(),
            m.resolutions
        );
    }
    if mdns.is_empty() {
        println!("  nothing resolved — a daemon started with `advertise = true` on this segment would have appeared here");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_beacons_never_reads_zero() {
        assert_eq!(expected(Duration::from_secs(0), 15), 1);
        assert_eq!(expected(Duration::from_secs(600), 15), 40);
        assert_eq!(expected(Duration::from_secs(12), 2), 6);
        assert_eq!(expected(Duration::from_secs(10), 0), 10);
    }
}
