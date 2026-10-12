//! The team's tmux session, against a real tmux. `R-P8`, feature 0051.
//!
//! `seat.rs` pins the calls; this says whether tmux does what they claim: a
//! window of every seat at once, each pane a nested client of that seat's
//! own session, the seats' windows linked in by name, and — the part worth
//! a real server — taking the team session down leaves every seat's session
//! standing, because a window linked elsewhere survives `kill-session`.
//!
//! The "seats" are `sleep` in throwaway sessions: no agent is launched, and
//! nothing is written anywhere. Skipped, not failed, where there is no tmux,
//! as `send_tmux.rs` is.

use mogeung_core::session::SessionSource;
use mogeung_core::team::{Seat, SeatLaunch, SeatSignals, SignalKind, Team};
use std::process::Command;

fn tmux(args: &[&str]) -> Option<String> {
    let out = Command::new("tmux").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn have_tmux() -> bool {
    tmux(&["-V"]).is_some()
}

fn launch(team: &Team, seat: &str, session: &str) -> SeatLaunch {
    SeatLaunch {
        team_id: team.id.clone(),
        team: team.name.clone(),
        seat: seat.into(),
        cli: SessionSource::Codex,
        session_id: None,
        pane_id: "%0".into(),
        tmux_target: format!("{session}:0.0"),
        tmux_session: session.into(),
        team_session: String::new(),
        dir: "/tmp".into(),
        worktree: false,
        files_dir: "/tmp".into(),
        started: 0,
        alive: false,
        token: String::new(),
        signals: SeatSignals { kind: SignalKind::Plain, ..Default::default() },
    }
}

#[test]
fn the_team_session_shows_every_seat_and_leaves_them_standing_when_killed() {
    if !have_tmux() {
        eprintln!("skipped: no tmux here");
        return;
    }
    let tag = std::process::id();
    let team = Team {
        id: "t-tmux".into(),
        name: format!("tmux-test-{tag}"),
        repo: "/tmp".into(),
        guidance: String::new(),
        permission: Default::default(),
        policy: Default::default(),
        seats: vec![Seat::new("alpha", SessionSource::Codex), Seat::new("beta", SessionSource::Codex)],
        paused: false,
        created: 0,
        updated: 0,
    };
    let a = format!("geung-tmux-test-{tag}-alpha");
    let b = format!("geung-tmux-test-{tag}-beta");
    let team_session = mogeungd::seat::team_session_name(&team);
    for (s, w) in [(&a, "alpha"), (&b, "beta")] {
        let _ = tmux(&["kill-session", "-t", &format!("={s}")]);
        assert!(tmux(&["new-session", "-d", "-s", s, "-n", w, "sleep 60"]).is_some(), "a throwaway seat session");
    }
    let cleanup = || {
        let _ = tmux(&["kill-session", "-t", &format!("={team_session}")]);
        let _ = tmux(&["kill-session", "-t", &format!("={a}")]);
        let _ = tmux(&["kill-session", "-t", &format!("={b}")]);
    };

    let launches = vec![launch(&team, "alpha", &a), launch(&team, "beta", &b)];
    let name = match mogeungd::seat::layout_team(&team, &launches) {
        Ok(n) => n,
        Err(e) => {
            cleanup();
            panic!("the layout was refused: {e:#}");
        }
    };
    assert_eq!(name, team_session);

    // One window of both seats, tiled; then one window per seat, by name.
    let windows = tmux(&["list-windows", "-t", &format!("={team_session}"), "-F", "#{window_name} #{window_panes}"]).unwrap_or_default();
    let lines: Vec<&str> = windows.lines().collect();
    assert_eq!(lines.first().copied(), Some("all 2"), "{windows}");
    assert!(lines.contains(&"alpha 1"), "{windows}");
    assert!(lines.contains(&"beta 1"), "{windows}");
    // The seat's window is the same window, linked, not a copy.
    let linked = tmux(&["list-windows", "-t", &format!("={a}"), "-F", "#{window_linked}"]).unwrap_or_default();
    assert_eq!(linked.trim(), "1", "alpha's window is linked into the team session");
    let status = tmux(&["show-options", "-t", &team_session, "-v", "status"]).unwrap_or_default();
    assert_eq!(status.trim(), "on");

    // Down, and the seats stand.
    mogeungd::seat::unlayout_team(&team_session);
    assert!(tmux(&["has-session", "-t", &format!("={team_session}")]).is_none(), "the team session is gone");
    assert!(tmux(&["has-session", "-t", &format!("={a}")]).is_some(), "alpha's session survived");
    assert!(tmux(&["has-session", "-t", &format!("={b}")]).is_some(), "beta's session survived");
    let linked = tmux(&["list-windows", "-t", &format!("={a}"), "-F", "#{window_linked}"]).unwrap_or_default();
    assert_eq!(linked.trim(), "0", "and its window is linked nowhere else");
    cleanup();
}

/// `R-P14`: a launch record is not a running seat; tmux is asked. A session
/// that is there is running, one that was killed is not, and the exact-match
/// prefix keeps `-2` from passing for its base.
#[test]
fn a_seat_is_running_while_its_tmux_session_exists() {
    if !have_tmux() {
        eprintln!("skipped: no tmux here");
        return;
    }
    let tag = std::process::id();
    let name = format!("geung-alive-test-{tag}");
    let longer = format!("{name}-2");
    let _ = tmux(&["kill-session", "-t", &format!("={name}")]);
    let _ = tmux(&["kill-session", "-t", &format!("={longer}")]);
    let team = Team {
        id: "t-alive".into(),
        name: format!("alive-test-{tag}"),
        repo: "/tmp".into(),
        guidance: String::new(),
        permission: Default::default(),
        policy: Default::default(),
        seats: vec![Seat::new("alpha", SessionSource::Codex)],
        paused: false,
        created: 0,
        updated: 0,
    };
    let l = launch(&team, "alpha", &name);
    assert!(!mogeungd::seat::is_running(&l), "nothing started yet");
    assert!(tmux(&["new-session", "-d", "-s", &longer, "sleep 60"]).is_some());
    assert!(!mogeungd::seat::is_running(&l), "a longer name is not this seat's");
    assert!(tmux(&["new-session", "-d", "-s", &name, "sleep 60"]).is_some());
    assert!(mogeungd::seat::is_running(&l));
    let _ = tmux(&["kill-session", "-t", &format!("={name}")]);
    assert!(!mogeungd::seat::is_running(&l), "gone with its session");
    let _ = tmux(&["kill-session", "-t", &format!("={longer}")]);
    let blank = SeatLaunch { tmux_session: String::new(), ..l };
    assert!(!mogeungd::seat::is_running(&blank), "no session named, so not running");
}
