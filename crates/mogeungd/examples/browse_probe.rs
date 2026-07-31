//! Ask what mogeung daemons are advertising on this network, and print them.
//!
//! `cargo run -p mogeungd --example browse_probe`
//!
//! For the question the window's list cannot answer: when nothing appears, is
//! the daemon silent, is the network dropping multicast, or is the client at
//! fault? This runs the same `discovery` code the window does with no UI in the
//! way — so a daemon that shows up here and not in the window is a UI problem,
//! and one that shows up in neither is not.
//!
//! It prints as it goes rather than at the end, because that *is* the answer:
//! mDNS replies trickle in over seconds, per interface and per address family,
//! and a host is expected to fill in gradually rather than appear whole.

use std::time::{Duration, Instant};

fn main() {
    let run_for = Duration::from_secs(10);
    println!(
        "listening {}s for {} — ctrl-c to stop\n",
        run_for.as_secs(),
        mogeungd::discovery::SERVICE
    );

    let watcher = match mogeungd::discovery::Watcher::start() {
        Ok(w) => w,
        Err(e) => {
            println!("cannot browse: {e}");
            println!("  · this machine may have no multicast-capable interface");
            return;
        }
    };

    let started = Instant::now();
    let mut seen: Vec<mogeungd::discovery::Found> = Vec::new();

    while started.elapsed() < run_for {
        for f in watcher.poll() {
            let at = started.elapsed().as_millis();
            match seen.iter_mut().find(|e| e.name == f.name) {
                Some(existing) => {
                    let before = existing.addrs.len();
                    existing.absorb(f);
                    if existing.addrs.len() != before {
                        println!(
                            "{at:>6}ms  {:<18} + address, now {:?}",
                            existing.name, existing.addrs
                        );
                    }
                }
                None => {
                    println!(
                        "{at:>6}ms  {:<18} {:<22} token={} version={} home={}",
                        f.name,
                        f.addr().map(|a| a.to_string()).unwrap_or_default(),
                        if f.needs_token { "yes" } else { "no" },
                        f.version.as_deref().unwrap_or("?"),
                        f.claude_home.as_deref().unwrap_or("?"),
                    );
                    seen.push(f);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if seen.is_empty() {
        println!("nothing advertising.");
        println!("  · a daemon only advertises with --advertise");
        println!("  · and only from a non-loopback --listen");
        println!("  · macOS prompts for firewall access on first bind; a dismissed");
        println!("    prompt looks exactly like a broken network");
        println!("  · many networks drop multicast between hosts");
    } else {
        println!("\n{} daemon(s):", seen.len());
        for f in &seen {
            println!("  {:<18} {:?}", f.name, f.addrs);
        }
    }
}
