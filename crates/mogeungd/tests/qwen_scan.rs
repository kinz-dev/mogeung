//! Qwen Code's sessions reaching the queue. `R-I15`.
//!
//! The end-to-end half of the adapter: a synthetic `~/.qwen` on disk, the real
//! scan loop over it, and the `Session` that comes out. `tests/qwen.rs` covers
//! the parser; this covers the wiring, which is where the two `== Codex`
//! guards that meant "not Claude" used to live.
//!
//! These tests can do something the Codex ones cannot: Qwen's liveness is a
//! real `kill(pid, 0)` against a real registry file, so writing the **test
//! process's own pid** into the registry produces a genuinely live session
//! rather than a mocked one.

use mogeung_core::session::{LiveStatus, SessionSource};
use mogeungd::state::{AgentHomes, AppState};
use mogeungd::store::Store;
use std::path::PathBuf;
use std::sync::Arc;

/// `projects/<name>` uses Qwen's `sanitizeCwd`: every byte outside
/// `[a-zA-Z0-9]` becomes `-`. Lossy and not reversible, which is why the
/// adapter reads `cwd` out of the records instead of decoding this.
fn sanitize_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn homes(tag: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("mogeung-qscan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let claude = root.join("claude");
    let qwen = root.join("qwen");
    std::fs::create_dir_all(claude.join("projects")).unwrap();
    std::fs::create_dir_all(qwen.join("sessions")).unwrap();
    (claude, qwen)
}

/// Write a transcript for `session_id` under the project directory for `cwd`.
fn write_transcript(qwen: &PathBuf, cwd: &str, session_id: &str, lines: &[String]) -> PathBuf {
    let chats = qwen
        .join("projects")
        .join(sanitize_cwd(cwd))
        .join("chats");
    std::fs::create_dir_all(&chats).unwrap();
    let path = chats.join(format!("{session_id}.jsonl"));
    let mut body = String::new();
    for l in lines {
        body.push_str(l);
        body.push('\n');
    }
    std::fs::write(&path, body).unwrap();
    path
}

/// The `<boot-id>:<starttime>` token Qwen writes into `procStart`, for a pid
/// that is really running.
///
/// The registrations below have to carry a **true** one: mogeung verifies it
/// now, precisely so a stale record naming a recycled pid cannot pass as a live
/// session. A fixture with an invented token is a fixture describing a bug.
fn proc_start_of(pid: u32) -> String {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return String::new();
    };
    let Some(cut) = stat.rfind(')') else { return String::new() };
    let fields: Vec<&str> = stat[cut + 1..].split_whitespace().collect();
    let Some(starttime) = fields.get(19) else { return String::new() };
    let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default();
    format!("{}:{}", boot.trim(), starttime)
}

fn pid_ns_of() -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self/ns/pid").map(|m| m.ino()).unwrap_or(0)
}

/// Register a live session against a pid that is genuinely running.
fn register(qwen: &PathBuf, pid: u32, session_id: &str, cwd: &str, name: &str) {
    let started = chrono::Utc::now().timestamp_millis();
    let proc_start = proc_start_of(pid);
    let pid_ns = pid_ns_of();
    std::fs::write(
        qwen.join("sessions").join(format!("{pid}.json")),
        format!(
            r#"{{"schemaVersion":1,"pid":{pid},"procStart":"{proc_start}","pidNs":{pid_ns},
                "sessionId":"{session_id}","cwd":"{cwd}","name":"{name}",
                "startedAt":{started},"qwenVersion":"0.22.0"}}"#
        ),
    )
    .unwrap();
}

fn user_line(session_id: &str, cwd: &str, text: &str) -> String {
    format!(
        r#"{{"uuid":"u1","parentUuid":null,"sessionId":"{session_id}","timestamp":"{ts}","type":"user","provenance":"real_user","cwd":"{cwd}","version":"0.22.0","message":{{"role":"user","parts":[{{"text":"{text}"}}]}}}}"#,
        ts = chrono::Utc::now().to_rfc3339()
    )
}

fn assistant_line(session_id: &str, cwd: &str, tail: &str) -> String {
    let parts = match tail {
        "tool" => r#"[{"text":"thinking","thought":true},{"functionCall":{"id":"c1","name":"read_file","args":{}}}]"#,
        _ => r#"[{"text":"All done — the tests pass."}]"#,
    };
    format!(
        r#"{{"uuid":"a1","parentUuid":"u1","sessionId":"{session_id}","timestamp":"{ts}","type":"assistant","provenance":"assistant_output","cwd":"{cwd}","version":"0.22.0","model":"qwen3.8-sglang","message":{{"role":"model","parts":{parts}}},"usageMetadata":{{"promptTokenCount":1200,"candidatesTokenCount":50,"thoughtsTokenCount":10,"cachedContentTokenCount":400,"totalTokenCount":1260}},"contextWindowSize":1000000}}"#,
        ts = chrono::Utc::now().to_rfc3339()
    )
}

async fn boot(claude: PathBuf, qwen: PathBuf) -> Arc<AppState> {
    let store = Store::open(&claude.join("mogeung.db")).unwrap();
    let homes = AgentHomes {
        // Point Codex at a sibling that does not exist, so this test stays
        // about Qwen and never reaches the developer's real `~/.codex`.
        codex: claude.parent().unwrap().join("codex"),
        qwen,
    };
    let state = AppState::with_homes(store, claude.clone(), homes).unwrap();
    state.scan().await;
    state
}

/// The headline: a finished Qwen turn is a session waiting on you, with its
/// source, its cwd, its tokens and its prompt intact. This is A23's test —
/// the Session model absorbing a third CLI without gaining a field.
#[tokio::test]
async fn a_finished_qwen_turn_is_a_session_waiting_on_you() {
    let (claude, qwen) = homes("waiting");
    let cwd = "/w/repo";
    let id = "4ade0baa-aa19-411b-9ddb-c86b98da7f50";
    write_transcript(
        &qwen,
        cwd,
        id,
        &[
            user_line(id, cwd, "make the tests pass"),
            assistant_line(id, cwd, "text"),
        ],
    );
    register(&qwen, std::process::id(), id, cwd, "repo-e4");

    let state = boot(claude, qwen).await;
    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("qwen session reached the queue");

    assert_eq!(s.source, SessionSource::QwenCode);
    assert_eq!(s.cwd, cwd);
    assert_eq!(s.name.as_deref(), Some("repo-e4"));
    assert_eq!(s.version.as_deref(), Some("0.22.0"));
    assert_eq!(s.last_prompt.as_deref(), Some("make the tests pass"));
    assert_eq!(s.turns, 1);
    assert_eq!(s.tokens_in, 1200, "prompt tokens already include the cache");
    assert_eq!(s.tokens_out, 60);
    assert!(s.alive, "the registry names a pid that is really running");
    assert_eq!(
        s.live_status,
        Some(LiveStatus::Idle),
        "a turn that ended with text and no tool call is waiting on the human"
    );
}

/// The other half of the heuristic: a trailing tool call is work in flight,
/// not an invitation to type.
#[tokio::test]
async fn a_trailing_tool_call_is_a_session_still_working() {
    let (claude, qwen) = homes("working");
    let cwd = "/w/other";
    let id = "8d1c14b6-b0ad-4bdc-b07e-327e0982f9d7";
    write_transcript(
        &qwen,
        cwd,
        id,
        &[
            user_line(id, cwd, "refactor the parser"),
            assistant_line(id, cwd, "tool"),
        ],
    );
    register(&qwen, std::process::id(), id, cwd, "other-1a");

    let state = boot(claude, qwen).await;
    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("session");
    assert_eq!(s.live_status, Some(LiveStatus::Busy));
    assert_eq!(s.tool_calls, 1);
    assert_eq!(s.last_activity.as_deref(), Some("read_file"));
    assert!(
        s.awaiting_permission().is_none(),
        "Qwen records nothing when a tool blocks on approval, so mogeung must \
         not claim a permission prompt it cannot see"
    );
}

/// The bug the two `== SessionSource::Codex` guards would have caused: they
/// read as "skip Codex" but *meant* "skip anything that is not Claude", so a
/// third source fell into the Claude liveness pass and was marked dead on
/// every tick — while its own scan had just marked it alive.
#[tokio::test]
async fn a_live_qwen_session_survives_the_claude_liveness_pass() {
    let (claude, qwen) = homes("liveness");
    let cwd = "/w/live";
    let id = "11111111-2222-3333-4444-555555555555";
    write_transcript(&qwen, cwd, id, &[user_line(id, cwd, "hello")]);
    register(&qwen, std::process::id(), id, cwd, "live-aa");

    let state = boot(claude, qwen).await;
    // A second and third pass: the Claude registry is empty, so a session it
    // wrongly owned would flip to dead here even though the first pass was
    // right.
    state.scan().await;
    state.scan().await;

    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("session");
    assert!(
        s.alive,
        "a Qwen session must not be reaped by Claude Code's liveness pass"
    );
    assert_eq!(s.source, SessionSource::QwenCode);
}

/// A session registered a moment ago has a pid before it has a line. Waiting
/// for the first transcript line to show it is the `R-J30` mistake.
#[tokio::test]
async fn a_just_started_session_appears_before_it_has_written_anything() {
    let (claude, qwen) = homes("fresh");
    let id = "99999999-8888-7777-6666-555555555555";
    register(&qwen, std::process::id(), id, "/w/fresh", "fresh-99");

    let state = boot(claude, qwen).await;
    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("registry-only session still appears");
    assert_eq!(s.cwd, "/w/fresh");
    assert!(s.alive);
    assert_eq!(
        s.live_status,
        Some(LiveStatus::Busy),
        "starting up is working, not waiting on you"
    );
}

/// A registry record whose process is gone must not resurrect the session.
/// Qwen unlinks on a clean exit, but a crash leaves the file behind.
#[tokio::test]
async fn a_stale_registry_record_is_not_a_live_session() {
    let (claude, qwen) = homes("stale");
    let cwd = "/w/dead";
    let id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    write_transcript(
        &qwen,
        cwd,
        id,
        &[
            user_line(id, cwd, "an old question"),
            assistant_line(id, cwd, "text"),
        ],
    );
    // A pid that cannot be running: pid 0 is never a user process, and
    // `kill(0, 0)` addresses the process group rather than a process.
    register(&qwen, 0, id, cwd, "dead-ff");

    let state = boot(claude, qwen).await;
    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("the transcript still makes a session");
    assert!(!s.alive, "the process is gone, whatever the file says");
    assert_eq!(s.live_status, None);
}

/// Present, watched, empty — reported as exactly that, not as nothing, and
/// through the per-agent slot rather than the Codex-shaped one.
#[tokio::test]
async fn an_empty_qwen_install_is_reported_honestly() {
    let (claude, qwen) = homes("empty");
    let state = boot(claude, qwen).await;
    let h = state.health().await;
    let agent = h
        .agents
        .iter()
        .find(|a| a.source == "qwen")
        .expect("qwen has a health slot of its own");
    assert!(agent.present);
    assert_eq!(agent.threads, 0);
    assert!(agent.error.is_none());
    assert!(state.sessions.read().await.is_empty());
}

/// No install at all stays silent — absence is not an error state.
#[tokio::test]
async fn no_qwen_install_reports_nothing() {
    let (claude, qwen) = homes("absent");
    std::fs::remove_dir_all(&qwen).unwrap();
    let state = boot(claude, qwen).await;
    let h = state.health().await;
    assert!(!h.agents.iter().any(|a| a.source == "qwen"));
    assert!(state.sessions.read().await.is_empty());
}

/// Drift in Qwen's transcript reaches the same alert list as Claude's and
/// Codex's, prefixed so the corpora stay tellable apart. Without a slot of its
/// own, a third CLI's canary would have had nowhere to report.
#[tokio::test]
async fn an_unknown_qwen_shape_reaches_the_health_alerts() {
    let (claude, qwen) = homes("canary");
    let cwd = "/w/drift";
    let id = "cccccccc-dddd-eeee-ffff-000000000000";
    write_transcript(
        &qwen,
        cwd,
        id,
        &[
            user_line(id, cwd, "hello"),
            format!(
                r#"{{"uuid":"x","sessionId":"{id}","timestamp":"{ts}","type":"system","subtype":"telepathy","provenance":"system"}}"#,
                ts = chrono::Utc::now().to_rfc3339()
            ),
        ],
    );

    let state = boot(claude, qwen).await;
    let h = state.health().await;
    let named = h.alerts.iter().any(|a| {
        matches!(
            a,
            mogeung_core::health::Alert::UnknownEventType { event_type, .. }
                if event_type == "qwen/system/telepathy"
        )
    });
    assert!(named, "unknown subtype must be named, not swallowed: {:?}", h.alerts);
}

/// The reason `scripts/qwenmo` exists, pinned end to end. `R-I15`.
///
/// A pty has exactly one master, so a `qwen` started in a bare terminal can
/// only be *pointed at*; one started under tmux can be **hosted** in a pane
/// (ADR-0010). That distinction is a single field — `tmux_target` — and it was
/// resolved only inside Claude Code's liveness pass, which every other source
/// deliberately skips. So wrapping `qwen` in tmux produced a session correct in
/// every visible field that still could not be attached to.
///
/// This drives the real ancestry walk against a real tmux pane rather than a
/// synthetic table, because the walk is the part that was missing. It uses the
/// default tmux server (the daemon's `tmux list-panes -a` reads that one and no
/// other), with a session name unique to this process, and kills only that
/// name. Skipped, not failed, where tmux is absent.
#[tokio::test]
async fn a_qwen_session_under_tmux_can_be_hosted_rather_than_only_seen() {
    fn tmux(args: &[&str]) -> Option<String> {
        let out = std::process::Command::new("tmux").args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
    if std::process::Command::new("tmux").arg("-V").output().is_err() {
        eprintln!("skipping: tmux is not installed");
        return;
    }

    let name = format!("mogeung-qwentest-{}", std::process::id());
    let target = format!("={name}");
    let _ = tmux(&["kill-session", "-t", &target]);
    if tmux(&["new-session", "-d", "-s", &name, "sleep", "120"]).is_none() {
        eprintln!("skipping: could not start a tmux session");
        return;
    }

    // Everything after this point must reach the kill below, so the result is
    // captured rather than asserted inline.
    let outcome = async {
        let pane_pid: u32 = tmux(&["list-panes", "-t", &target, "-F", "#{pane_pid}"])?
            .lines()
            .next()?
            .trim()
            .parse()
            .ok()?;

        let (claude, qwen) = homes("tmux");
        let cwd = "/w/hosted";
        let id = "dddddddd-eeee-ffff-0000-111111111111";
        write_transcript(&qwen, cwd, id, &[user_line(id, cwd, "hello")]);
        register(&qwen, pane_pid, id, cwd, "hosted-dd");

        let state = boot(claude, qwen).await;
        let sessions = state.sessions.read().await;
        Some(sessions.get(id)?.tmux_target.clone())
    }
    .await;

    let _ = tmux(&["kill-session", "-t", &target]);

    let resolved = outcome.expect("the qwen session should exist");
    assert_eq!(
        resolved.as_deref(),
        Some(format!("{name}:0.0").as_str()),
        "a live Qwen session in a tmux pane must resolve its attach target, or \
         the Agent pane can never host it"
    );
}

/// A dead session must not keep a pane. Leaving a stale target would offer a
/// terminal tab that attaches to nothing — the same rule Claude's liveness
/// pass applies on the way out, and one this source has to apply for itself.
///
/// Deliberately *not* asserting that a live session outside tmux resolves to
/// `None`: the first draft did, and it failed, because the test process is
/// itself a descendant of a tmux pane on a developer machine that runs its
/// agents under tmux. The walk had found a real ancestor and was right; the
/// assertion was the thing that was wrong. There is no way to assert absence
/// here without controlling the whole process tree, so the deterministic half
/// is what gets pinned.
#[tokio::test]
async fn a_dead_qwen_session_keeps_no_pane() {
    let (claude, qwen) = homes("notmux");
    let cwd = "/w/bare";
    let id = "22222222-3333-4444-5555-666666666666";
    write_transcript(
        &qwen,
        cwd,
        id,
        &[
            user_line(id, cwd, "hello"),
            assistant_line(id, cwd, "text"),
        ],
    );
    // pid 0 is never a live process, so this session is seen and not alive.
    register(&qwen, 0, id, cwd, "bare-22");

    let state = boot(claude, qwen).await;
    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("the transcript still makes a session");
    assert!(!s.alive);
    assert_eq!(
        s.tmux_target, None,
        "a dead session offering an attach target is a tab that opens onto nothing"
    );
}

/// A closed session must not go on calling itself live. `R-I15`.
///
/// Reported: *"the qwen session even if that is closed it still appears live in
/// the ATTENTION list, showing busy but silent — STALLED"*. Two independent
/// causes, and this covers the structural one: `scan_qwen` walks what it
/// **finds** — transcripts on disk, plus the registry — where the Claude pass
/// walks every id it **knows**. A session that drops out of both was therefore
/// never revisited and kept the `alive` it was last given, so it sat in the
/// queue reporting itself busy and then stalled when it fell silent.
#[tokio::test]
async fn a_qwen_session_whose_transcript_vanishes_stops_being_alive() {
    let (claude, qwen) = homes("vanish");
    let cwd = "/w/vanish";
    let id = "77777777-8888-9999-aaaa-bbbbbbbbbbbb";
    let path = write_transcript(&qwen, cwd, id, &[user_line(id, cwd, "hello")]);
    register(&qwen, std::process::id(), id, cwd, "vanish-77");

    let store = Store::open(&claude.join("mogeung.db")).unwrap();
    let homes = AgentHomes {
        codex: claude.parent().unwrap().join("codex"),
        qwen: qwen.clone(),
    };
    let state = AppState::with_homes(store, claude.clone(), homes).unwrap();
    state.scan().await;
    assert!(
        state.sessions.read().await.get(id).expect("session").alive,
        "precondition: it is running"
    );

    // The session ends and its evidence goes with it — the registry record is
    // unlinked on a clean exit, and the transcript ages out or is archived.
    std::fs::remove_file(qwen.join("sessions").join(format!("{}.json", std::process::id()))).unwrap();
    std::fs::remove_file(&path).unwrap();
    state.scan().await;

    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("the session is still known");
    assert!(!s.alive, "nothing found it, so nothing is running it");
    assert_eq!(s.live_status, None, "and it must not still read as busy");
    assert_eq!(s.tmux_target, None);
}

/// The other cause: `kill(pid, 0)` answers *"does some process have this pid"*,
/// which is not the question. A qwen killed with its parent still around stays
/// in the process table as a zombie until reaped, and signals to it succeed —
/// so a closed session went on reporting itself live off a stale registry
/// record. Qwen guards this with `procStart`; so must we.
///
/// This makes a real zombie rather than describing one.
#[tokio::test]
async fn a_zombie_is_not_a_live_session() {
    // `sh -c 'exit 0'` from a parent that never waits: the child is reaped by
    // nobody and sits as `Z` for as long as this process holds the handle.
    let mut child = match std::process::Command::new("sh").arg("-c").arg("exit 0").spawn() {
        Ok(c) => c,
        Err(_) => {
            eprintln!("skipping: could not spawn");
            return;
        }
    };
    let zombie_pid = child.id();
    // Wait for it to become defunct without reaping it.
    for _ in 0..200 {
        let stat = std::fs::read_to_string(format!("/proc/{zombie_pid}/stat")).unwrap_or_default();
        if stat.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z') {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let stat = std::fs::read_to_string(format!("/proc/{zombie_pid}/stat")).unwrap_or_default();
    if !stat.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z') {
        let _ = child.wait();
        eprintln!("skipping: no zombie on this platform");
        return;
    }

    let (claude, qwen) = homes("zombie");
    let cwd = "/w/zombie";
    let id = "33333333-4444-5555-6666-777777777777";
    write_transcript(
        &qwen,
        cwd,
        id,
        &[user_line(id, cwd, "hello"), assistant_line(id, cwd, "tool")],
    );
    // A record left behind by a crash, naming a pid that still answers
    // `kill(pid, 0)` because nobody has buried it.
    register(&qwen, zombie_pid, id, cwd, "zombie-33");

    let state = boot(claude, qwen).await;
    let alive = state.sessions.read().await.get(id).expect("session").alive;
    let _ = child.wait();

    assert!(
        !alive,
        "a defunct process is not a running agent — it answers kill(0) and does nothing, \
         which is precisely the 'live but silent, then STALLED' report"
    );
}

/// And the third way a stale record lies: pids wrap, so the next process to
/// land on that number inherits a dead session's identity. `procStart` is what
/// tells them apart — same pid, different start time.
#[tokio::test]
async fn a_reused_pid_does_not_inherit_a_dead_session() {
    let (claude, qwen) = homes("reuse");
    let cwd = "/w/reuse";
    let id = "55555555-6666-7777-8888-999999999999";
    write_transcript(&qwen, cwd, id, &[user_line(id, cwd, "hello")]);

    // This process is certainly alive, but the record claims it started at a
    // moment it did not — which is what a recycled pid looks like.
    let pid = std::process::id();
    let started = chrono::Utc::now().timestamp_millis();
    std::fs::write(
        qwen.join("sessions").join(format!("{pid}.json")),
        format!(
            r#"{{"schemaVersion":1,"pid":{pid},"procStart":"some-other-boot:1","pidNs":4026531836,
                "sessionId":"{id}","cwd":"{cwd}","name":"reuse-55",
                "startedAt":{started},"qwenVersion":"0.22.0"}}"#
        ),
    )
    .unwrap();

    let state = boot(claude, qwen).await;
    let sessions = state.sessions.read().await;
    let s = sessions.get(id).expect("the transcript still makes a session");
    if cfg!(target_os = "linux") {
        assert!(!s.alive, "the pid is live but it is not the same process");
    }
}
