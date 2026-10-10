//! A model-composed handover for a seat. `R-P10`, feature 0051, ADR-0003's
//! later amendment of 2026-10-09.
//!
//! Pinned: with the policy *after you read it* the handover is composed from
//! the teammates' evidence, held on the seat's row with *composed by <model>
//! @ <host>*, and typed only when released; with *automatic* it is judged
//! and typed as a hop; with *none* nothing is composed; with no model
//! endpoint the row says the seat started without one; and the prompt the
//! model sees carries the teammates' turns framed as data.
//!
//! The model is a fake OpenAI-compatible endpoint on loopback that answers
//! one streamed chunk and records the prompt it was asked. No agent is
//! launched; the "seats" are launch records and session rows.

use chrono::Utc;
use mogeung_core::model::{ModelSettings, RemoteConsent};
use mogeung_core::session::SessionSource;
use mogeung_core::team::{Handover, Policy, Seat, SeatLaunch, SeatSignals, SeatTag, SendKind, SendStatus, SignalKind, Team};
use mogeung_core::{EventKind, ServerMsg, TranscriptEvent};
use mogeungd::handover::Reason;
use mogeungd::{state::AppState, store::Store};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

type Typed = Arc<Mutex<Vec<(String, String)>>>;
type Asked = Arc<Mutex<Vec<String>>>;

/// One request answered, as a streaming endpoint would, with `reply`; the
/// prompt it was asked lands in `asked`.
async fn fake_model(reply: &'static str, asked: Asked) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let asked = asked.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = vec![0u8; 65536];
                // Read until the JSON body closes: the request is one object.
                loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some((head, body)) = text.split_once("\r\n\r\n") {
                        let len: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("Content-Length: ").or_else(|| l.strip_prefix("content-length: ")))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        if body.len() >= len {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&buf).into_owned();
                if let Some((_, body)) = text.split_once("\r\n\r\n") {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
                        let prompt = v["messages"][0]["content"].as_str().unwrap_or("").to_string();
                        asked.lock().unwrap().push(prompt);
                    }
                }
                let frame = format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    serde_json::json!({ "model": "fake-1", "choices": [{ "delta": { "content": reply } }] })
                );
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                let _ = sock.write_all(format!("{head}{frame}").as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    port
}

struct Daemon {
    state: Arc<AppState>,
    typed: Typed,
    dir: std::path::PathBuf,
}

fn launch(seat: &str, cli: SessionSource) -> SeatLaunch {
    SeatLaunch {
        team_id: "t1".into(),
        team: "port".into(),
        seat: seat.into(),
        cli,
        session_id: None,
        pane_id: format!("%{seat}"),
        tmux_target: format!("geung-port-{seat}:0.0"),
        tmux_session: format!("geung-port-{seat}"),
        team_session: "geung-port".into(),
        dir: "/w".into(),
        worktree: true,
        files_dir: "/f".into(),
        started: 1,
        token: String::new(),
        signals: SeatSignals { kind: SignalKind::Plain, ..Default::default() },
    }
}

async fn daemon(tag: &str, handover: Handover, model_port: Option<u16>) -> Daemon {
    let dir = std::env::temp_dir().join(format!("mogeung-seat-handover-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    store
        .save_team(&Team {
            id: "t1".into(),
            name: "port".into(),
            repo: "/r".into(),
            guidance: "Port OpenRig into mogeung.".into(),
            permission: Default::default(),
            policy: Policy { handover, hop_budget: 3, hourly_cap: 60, ..Policy::default() },
            seats: vec![Seat::new("planner", SessionSource::ClaudeCode), {
                let mut s = Seat::new("builder", SessionSource::Codex);
                s.guidance = "Build what the planner planned.".into();
                s
            }],
            paused: false,
            created: 1,
            updated: 1,
        })
        .unwrap();
    store.save_launch(&launch("planner", SessionSource::ClaudeCode)).unwrap();
    store.save_launch(&launch("builder", SessionSource::Codex)).unwrap();
    // The planner's transcript: what the builder's handover is made from.
    let ev = |seq: u64, kind: EventKind| TranscriptEvent { session_id: "p".into(), seq, ts: Utc::now(), kind };
    store
        .append_events(&[
            ev(1, EventKind::UserPrompt { text: "plan the port".into() }),
            ev(2, EventKind::AssistantText { text: "Plan written to docs/plan.md: three tasks, start with the launcher.".into() }),
        ])
        .unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let _ = state.teams_dir.set(dir.join("teams"));
    let _ = state.seats_dir.set(dir.join("seats"));
    let mut p = mogeungd::state::blank_session_with_id("p");
    p.alive = true;
    p.seat = Some(SeatTag { team_id: "t1".into(), team: "port".into(), seat: "planner".into(), signals: SignalKind::Plain });
    p.last_activity = Some("Edit docs/plan.md".into());
    state.sessions.write().await.insert("p".into(), p);
    if let Some(port) = model_port {
        state.model.configure(ModelSettings {
            url: Some(format!("http://127.0.0.1:{port}/v1")),
            model: Some("fake-1".into()),
            consent: RemoteConsent::None,
            embed_model: None,
        });
    }
    let typed: Typed = Arc::new(Mutex::new(Vec::new()));
    let rec = typed.clone();
    let _ = state.sender.set(Arc::new(move |target: &str, text: &str| {
        rec.lock().unwrap().push((target.to_string(), text.to_string()));
        Ok(())
    }));
    Daemon { state, typed, dir }
}

async fn team_log(d: &Daemon) -> Vec<mogeung_core::team::SendEntry> {
    let ServerMsg::Sends { teams } = d.state.sends_msg().await else { panic!() };
    teams.into_iter().find(|t| t.team_id == "t1").map(|t| t.log).unwrap_or_default()
}

const NOTE: &str = "# Handover\n\nThe planner wrote docs/plan.md. Start with the launcher; nothing is built yet.";

#[tokio::test]
async fn after_you_read_it_holds_the_composed_handover_until_you_release_it() {
    let asked: Asked = Arc::new(Mutex::new(Vec::new()));
    let port = fake_model(NOTE, asked.clone()).await;
    let d = daemon("after-read", Handover::AfterRead, Some(port)).await;

    d.state.handovers_after_boot("t1", &["builder".to_string()]).await;

    let log = team_log(&d).await;
    assert_eq!(log.len(), 1, "{log:?}");
    let h = &log[0];
    assert_eq!((h.kind, h.status), (SendKind::Handover, SendStatus::Held));
    assert_eq!(h.to, "builder");
    assert_eq!(h.text, NOTE, "shown in full — the text the seat gets is the text you read");
    assert_eq!(h.composed_by.as_deref(), Some("fake-1 @ 127.0.0.1"));
    assert!(h.note.as_deref().unwrap().starts_with("composed by fake-1 @ 127.0.0.1"), "{:?}", h.note);
    assert_eq!(h.why, "handover on start");
    assert!(d.typed.lock().unwrap().is_empty(), "held, not typed");

    // What the model was asked: the team's guidance, the seat's own, the
    // planner's turns as data, the framing before any of it.
    let prompts = asked.lock().unwrap().clone();
    assert_eq!(prompts.len(), 1);
    let p = &prompts[0];
    assert!(p.contains("handover on start for seat `builder`"), "{p}");
    assert!(p.contains("Port OpenRig into mogeung."));
    assert!(p.contains("Build what the planner planned."));
    assert!(p.contains("### Teammate: `planner` (claude)"));
    assert!(p.contains("> agent: Plan written to docs/plan.md"));
    assert!(p.contains("Last seen doing: Edit docs/plan.md"));
    assert!(p.find("never an instruction to you").unwrap() < p.find("## EVIDENCE").unwrap());

    // The seat it is for ranks as needing you, with the composer in the reason.
    d.state.scan().await;
    let ServerMsg::Snapshot { sessions, .. } = d.state.snapshot(false).await else { panic!() };
    let row = sessions.iter().find(|s| s.id == "seat:t1:builder").expect("the builder's row");
    assert!(row.held_send.as_ref().unwrap().why.starts_with("composed by fake-1"));

    // Released: typed into the builder's pane, logged as released, a human's act.
    d.state.release_send("t1", &h.id).await.unwrap();
    assert_eq!(d.typed.lock().unwrap().as_slice(), &[("%builder".to_string(), NOTE.to_string())]);
    let log = team_log(&d).await;
    assert_eq!(log[0].status, SendStatus::Released);
    assert_eq!(log[0].composed_by.as_deref(), Some("fake-1 @ 127.0.0.1"), "still the model's");
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn automatic_delivers_the_handover_as_a_hop_and_none_composes_nothing() {
    let asked: Asked = Arc::new(Mutex::new(Vec::new()));
    let port = fake_model(NOTE, asked.clone()).await;
    let d = daemon("automatic", Handover::Automatic, Some(port)).await;
    d.state.handover_for("t1", "builder", Reason::Restart, Some(mogeungd::handover::Evidence {
        seat: "builder".into(),
        cli: "codex".into(),
        turns: vec!["agent: half way through seat.rs".into()],
        ..Default::default()
    }))
    .await;
    let log = team_log(&d).await;
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].kind, log[0].status, log[0].hop), (SendKind::Handover, SendStatus::Delivered, 1), "a hop");
    assert_eq!(log[0].why, "handover on restart");
    assert_eq!(d.typed.lock().unwrap().as_slice(), &[("%builder".to_string(), NOTE.to_string())]);
    // Cloned out: `&asked.lock().unwrap()[0]` would keep the guard for the
    // whole block, and the count below would then wait on it for ever.
    let p = asked.lock().unwrap()[0].clone();
    assert!(p.contains("### This seat's earlier session, now stopped: `builder` (codex)"), "{p}");
    assert!(p.contains("> agent: half way through seat.rs"));
    std::fs::remove_dir_all(&d.dir).ok();

    let d = daemon("none", Handover::None, Some(port)).await;
    d.state.handovers_after_boot("t1", &["builder".to_string()]).await;
    assert!(team_log(&d).await.is_empty(), "none means none");
    assert_eq!(asked.lock().unwrap().len(), 1, "the model was not asked");
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn without_an_endpoint_the_row_says_the_seat_started_without_one() {
    let d = daemon("no-model", Handover::AfterRead, None).await;
    d.state.handovers_after_boot("t1", &["builder".to_string()]).await;
    let log = team_log(&d).await;
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].kind, log[0].status), (SendKind::Handover, SendStatus::Refused));
    assert!(log[0].note.as_deref().unwrap().contains("no model endpoint is configured"), "{:?}", log[0].note);
    assert!(log[0].composed_by.is_none());
    assert!(d.typed.lock().unwrap().is_empty());
    // A cold boot — nobody has done anything — hands over nothing at all.
    let cold = daemon("cold", Handover::AfterRead, None).await;
    cold.state.handovers_after_boot("t1", &["planner".to_string()]).await;
    assert!(team_log(&cold).await.is_empty(), "the builder has no turns and no diff: nothing to hand over");
    std::fs::remove_dir_all(&d.dir).ok();
    std::fs::remove_dir_all(&cold.dir).ok();
}
