//! A team out as a file and a file in as a draft, through the real router.
//! `R-P12`, feature 0051.
//!
//! Pinned: an export writes the definition whole where you say, and beside
//! the mirror when you say nothing; the file re-imports as a new draft with
//! its id dropped and nothing stored; an OpenRig `rig.yaml` beside its agents
//! comes in as a draft with the seats, the guidance and the notes; and a path
//! that is not absolute, not there or not a team is refused in words. Since
//! `R-P20`, the spec of that file comes back on `TeamSchema`, and the file an
//! export wrote has no key the spec does not name.
//!
//! Costs nothing: no agent is launched.

use futures_util::{SinkExt, StreamExt};
use mogeung_core::session::SessionSource;
use mogeung_core::team::{Seat, Team};
use mogeung_core::{ClientMsg, ServerMsg};
use mogeungd::{api, state::AppState, store::Store};
use tokio_tungstenite::tungstenite::Message;

struct Daemon {
    url: String,
    dir: std::path::PathBuf,
}

async fn daemon(tag: &str) -> Daemon {
    let dir = std::env::temp_dir().join(format!("mogeung-team-file-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = Store::open(&dir.join("t.db")).unwrap();
    let mut team = Team {
        id: "t1".into(),
        name: "Port OpenRig".into(),
        repo: "/home/you/mogeung".into(),
        guidance: "Read the spec.".into(),
        permission: Default::default(),
        policy: Default::default(),
        seats: vec![Seat::new("planner", SessionSource::ClaudeCode), Seat::new("builder", SessionSource::Codex)],
        paused: true,
        created: 10,
        updated: 20,
    };
    team.seats[1].guidance = "Build it.".into();
    store.save_team(&team).unwrap();
    let state = AppState::with_home(store, dir.join("claude")).unwrap();
    let _ = state.teams_dir.set(dir.join("teams"));
    let _ = state.seats_dir.set(dir.join("seats"));
    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
            .unwrap();
    });
    Daemon { url: format!("ws://127.0.0.1:{port}/ws"), dir }
}

async fn ask<T>(url: &str, cmd: ClientMsg, pick: impl Fn(ServerMsg) -> Option<T>) -> T {
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    ws.send(Message::Text(serde_json::to_string(&cmd).unwrap().into())).await.unwrap();
    let wait = async {
        while let Some(Ok(Message::Text(t))) = ws.next().await {
            if let Ok(msg) = serde_json::from_str::<ServerMsg>(&t) {
                if let Some(v) = pick(msg) {
                    return v;
                }
            }
        }
        panic!("the socket closed before answering");
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), wait).await.expect("an answer in time")
}

fn exported(m: ServerMsg) -> Option<String> {
    match m {
        ServerMsg::TeamExported { path, .. } => Some(path),
        ServerMsg::Error { message } => panic!("refused: {message}"),
        _ => None,
    }
}

fn draft(m: ServerMsg) -> Option<Result<(Team, Vec<String>), String>> {
    match m {
        ServerMsg::TeamDraft { team, notes } => Some(Ok((team, notes))),
        ServerMsg::Error { message } => Some(Err(message)),
        _ => None,
    }
}

#[tokio::test]
async fn an_export_is_the_definition_whole_and_imports_back_as_a_new_draft() {
    let d = daemon("json").await;
    // Where you say.
    let target = d.dir.join("out").join("port.json");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    let path = ask(&d.url, ClientMsg::TeamExport { id: "t1".into(), path: Some(target.to_string_lossy().into_owned()) }, exported).await;
    assert_eq!(std::path::Path::new(&path), target);
    let text = std::fs::read_to_string(&target).unwrap();
    let on_disk: Team = serde_json::from_str(&text).unwrap();
    assert_eq!(on_disk.id, "t1", "whole — the id is in the file");
    assert!(on_disk.paused);
    assert_eq!(on_disk.seats[1].guidance, "Build it.");

    // Beside the mirror when you say nothing.
    let path = ask(&d.url, ClientMsg::TeamExport { id: "t1".into(), path: None }, exported).await;
    assert!(path.starts_with(d.dir.join("teams").to_string_lossy().as_ref()), "{path}");
    assert!(path.ends_with("t1-port-openrig.json"), "{path}");
    assert!(std::path::Path::new(&path).is_file());

    // Back in: a new team, unpaused, unstamped — and nothing stored.
    let (team, notes) = ask(&d.url, ClientMsg::TeamImport { path: target.to_string_lossy().into_owned() }, draft).await.unwrap();
    assert_eq!(team.id, "");
    assert!(!team.paused);
    assert_eq!((team.created, team.updated), (0, 0));
    assert_eq!(team.name, "Port OpenRig");
    assert_eq!(team.seats.len(), 2);
    assert!(notes.iter().any(|n| n.contains("new team")), "{notes:?}");
    let teams = ask(&d.url, ClientMsg::TeamList, |m| match m {
        ServerMsg::Teams { teams, .. } => Some(teams),
        _ => None,
    })
    .await;
    assert_eq!(teams.len(), 1, "an import is a draft, never a save");
    std::fs::remove_dir_all(&d.dir).ok();
}

/// `R-P20`. *"if there is a json spec for it?"* There is, and it is the
/// daemon's own: asked over the wire, it names every key the export wrote.
#[tokio::test]
async fn the_schema_the_daemon_answers_covers_the_file_it_exports() {
    let d = daemon("schema").await;
    let schema = ask(&d.url, ClientMsg::TeamSchema, |m| match m {
        ServerMsg::TeamSchema { schema } => Some(schema),
        ServerMsg::Error { message } => panic!("refused: {message}"),
        _ => None,
    })
    .await;
    assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert_eq!(schema["title"], "mogeung team definition");
    let path = ask(&d.url, ClientMsg::TeamExport { id: "t1".into(), path: None }, exported).await;
    let on_disk: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let declared = schema["properties"].as_object().unwrap();
    for key in on_disk.as_object().unwrap().keys() {
        assert!(declared.contains_key(key), "the export writes `{key}`, which the schema does not name");
    }
    let seat = schema["$defs"]["Seat"]["properties"].as_object().unwrap();
    for key in on_disk["seats"][0].as_object().unwrap().keys() {
        assert!(seat.contains_key(key), "a seat has `{key}`, which the schema does not name");
    }
    std::fs::remove_dir_all(&d.dir).ok();
}

#[tokio::test]
async fn an_openrig_rig_beside_its_agents_comes_in_as_a_draft_with_notes() {

    let d = daemon("rig").await;
    let rig_dir = d.dir.join("rig");
    std::fs::create_dir_all(rig_dir.join("agents/lead/guidance")).unwrap();
    std::fs::write(rig_dir.join("agents/lead/agent.yaml"), "name: lead\nresources:\n  guidance:\n    - path: guidance/role.md\n").unwrap();
    std::fs::write(rig_dir.join("agents/lead/guidance/role.md"), "# Role: Lead\n").unwrap();
    std::fs::write(
        rig_dir.join("rig.yaml"),
        "version: \"0.2\"\nname: demo\npods:\n  - id: orch\n    members:\n      - { id: lead, agent_ref: \"local:agents/lead\", runtime: claude-code, cwd: . }\n      - { id: ui, agent_ref: \"builtin:terminal\", runtime: terminal, cwd: . }\n",
    )
    .unwrap();
    let (team, notes) = ask(&d.url, ClientMsg::TeamImport { path: rig_dir.join("rig.yaml").to_string_lossy().into_owned() }, draft)
        .await
        .unwrap();
    assert_eq!(team.name, "demo");
    assert_eq!(team.seats.len(), 1);
    assert_eq!(team.seats[0].name, "lead");
    assert_eq!(team.seats[0].cli, SessionSource::ClaudeCode);
    assert_eq!(team.seats[0].guidance, "# Role: Lead");
    assert!(notes.iter().any(|n| n.contains("terminal")), "{notes:?}");
    assert!(notes.iter().any(|n| n.contains("set the repository")), "{notes:?}");

    // Refusals, in words.
    let why = ask(&d.url, ClientMsg::TeamImport { path: "rig.yaml".into() }, draft).await.unwrap_err();
    assert!(why.contains("absolute"), "{why}");
    let why = ask(&d.url, ClientMsg::TeamImport { path: d.dir.join("nope.json").to_string_lossy().into_owned() }, draft).await.unwrap_err();
    assert!(why.contains("cannot read"), "{why}");
    std::fs::write(d.dir.join("x.toml"), "a = 1").unwrap();
    let why = ask(&d.url, ClientMsg::TeamImport { path: d.dir.join("x.toml").to_string_lossy().into_owned() }, draft).await.unwrap_err();
    assert!(why.contains("neither"), "{why}");
    std::fs::remove_dir_all(&d.dir).ok();
}
