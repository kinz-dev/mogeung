//! The beacon leaves the daemon. `R-I17` phase 1, ADR-0039, ADR-0040.
//!
//! A real socket on loopback, because the two things worth pinning — that a
//! beacon goes out as a burst of two with one sequence number, and that it
//! says presence and never access — are properties of bytes on a wire.

use mogeung_core::lan::{decode, Reach};
use mogeungd::lan::{start, Target};
use mogeungd::state::AppState;
use mogeungd::store::Store;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

#[tokio::test]
async fn a_beacon_arrives_twice_with_one_sequence_number_and_says_presence_not_access() {
    let dir = std::env::temp_dir().join(format!("mogeung-beacon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();

    let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let port = rx.local_addr().unwrap().port();
    let bind: SocketAddr = "127.0.0.1:7717".parse().unwrap();
    let _sender = start(state, bind, port, || {
        vec![Target {
            iface: "lo".into(),
            ip: Ipv4Addr::LOCALHOST,
            bcast: Ipv4Addr::LOCALHOST,
        }]
    });

    let (first, raw, second) = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 2048];
        let (n, _) = rx.recv_from(&mut buf).unwrap();
        let raw = buf[..n].to_vec();
        let first = decode(&raw).unwrap();
        let (n, _) = rx.recv_from(&mut buf).unwrap();
        let second = decode(&buf[..n]).unwrap();
        (first, raw, second)
    })
    .await
    .unwrap();

    assert_eq!(first.seq, 1);
    assert_eq!(second, first, "the burst is one beacon twice, not two beacons");
    assert_eq!(first.reach, Reach::Local, "a loopback bind with no ssh target");
    assert_eq!(first.port, 7717);
    assert!(first.ssh.is_none());
    assert_eq!((first.live, first.waiting), (0, 0), "an empty board");
    assert!(!first.home.is_empty());
    assert!(!String::from_utf8_lossy(&raw).contains("token"), "presence, never access");
    let _ = std::fs::remove_dir_all(&dir);
}
