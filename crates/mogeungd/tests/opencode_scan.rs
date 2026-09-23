//! opencode's sessions reaching the queue. `R-I25`, feature 0048.
//!
//! The end-to-end half of the adapter: a synthetic `opencode.db` built with
//! **opencode's own schema** (copied from a 1.18.32 install), the real scan
//! loop over it, and the `Session` that comes out. `src/opencode.rs` tests the
//! reading of rows; this tests the wiring.
//!
//! Liveness here is real, not mocked. opencode keeps no registry, so a session
//! is live when an `opencode` process is working in its directory — and these
//! tests make one: a script named `opencode`, started in the session's
//! directory. The kernel names the process after the script, so the scan
//! finds it exactly as it finds the real thing.

use mogeung_core::session::{LiveStatus, SessionSource};
use mogeungd::state::{AgentHomes, AppState};
use mogeungd::store::Store;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// opencode 1.18.32's own DDL for the tables the reader touches.
const SCHEMA: &str = r#"
CREATE TABLE `session` (
  `id` text PRIMARY KEY, `project_id` text NOT NULL, `workspace_id` text, `parent_id` text,
  `slug` text NOT NULL, `directory` text NOT NULL, `path` text, `title` text NOT NULL,
  `version` text NOT NULL, `share_url` text, `summary_additions` integer, `summary_deletions` integer,
  `summary_files` integer, `summary_diffs` text, `metadata` text, `cost` real DEFAULT 0 NOT NULL,
  `tokens_input` integer DEFAULT 0 NOT NULL, `tokens_output` integer DEFAULT 0 NOT NULL,
  `tokens_reasoning` integer DEFAULT 0 NOT NULL, `tokens_cache_read` integer DEFAULT 0 NOT NULL,
  `tokens_cache_write` integer DEFAULT 0 NOT NULL, `revert` text, `permission` text, `agent` text,
  `model` text, `time_created` integer NOT NULL, `time_updated` integer NOT NULL,
  `time_compacting` integer, `time_archived` integer
);
CREATE TABLE `message` (
  `id` text PRIMARY KEY, `session_id` text NOT NULL, `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL, `data` text NOT NULL
);
CREATE TABLE `part` (
  `id` text PRIMARY KEY, `message_id` text NOT NULL, `session_id` text NOT NULL,
  `time_created` integer NOT NULL, `time_updated` integer NOT NULL, `data` text NOT NULL
);
CREATE TABLE `event_sequence` (`aggregate_id` text PRIMARY KEY, `seq` integer NOT NULL, `owner_id` text);
CREATE INDEX `message_session_time_created_id_idx` ON `message` (`session_id`,`time_created`,`id`);
CREATE INDEX `part_session_idx` ON `part` (`session_id`);
"#;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

struct Fixture {
    root: PathBuf,
    claude: PathBuf,
    oc: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("mogeung-ocscan-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let claude = root.join("claude");
        let oc = root.join("opencode");
        std::fs::create_dir_all(claude.join("projects")).unwrap();
        std::fs::create_dir_all(&oc).unwrap();
        Connection::open(oc.join("opencode.db")).unwrap().execute_batch(SCHEMA).unwrap();
        Fixture { root, claude, oc }
    }

    fn db(&self) -> Connection {
        Connection::open(self.oc.join("opencode.db")).unwrap()
    }

    /// A working directory for a session, which exists — the process must be
    /// able to start in it.
    fn dir(&self, name: &str) -> PathBuf {
        let d = self.root.join("work").join(name);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn session(&self, id: &str, dir: &Path, title: &str, updated: i64, parent: Option<&str>) {
        self.db()
            .execute(
                "INSERT INTO session (id, project_id, parent_id, slug, directory, title, version, tokens_input, tokens_output,
                   tokens_reasoning, tokens_cache_read, time_created, time_updated)
                 VALUES (?1, 'global', ?2, 'slug', ?3, ?4, '1.18.32', 11245, 7, 22, 100, ?5, ?5)",
                params![id, parent, dir.to_string_lossy(), title, updated],
            )
            .unwrap();
        self.bump(id);
    }

    fn message(&self, id: &str, session: &str, at: i64, data: serde_json::Value) {
        self.db()
            .execute(
                "INSERT OR REPLACE INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?3, ?4)",
                params![id, session, at, data.to_string()],
            )
            .unwrap();
        self.bump(session);
    }

    fn part(&self, id: &str, message: &str, session: &str, at: i64, data: serde_json::Value) {
        self.db()
            .execute(
                "INSERT OR REPLACE INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                params![id, message, session, at, data.to_string()],
            )
            .unwrap();
        self.bump(session);
    }

    /// What opencode does on every write: move the session's event sequence.
    fn bump(&self, session: &str) {
        self.db()
            .execute(
                "INSERT INTO event_sequence (aggregate_id, seq) VALUES (?1, 0)
                 ON CONFLICT(aggregate_id) DO UPDATE SET seq = seq + 1",
                [session],
            )
            .unwrap();
    }

    /// A finished turn: your prompt and an answer that stopped.
    fn finished_turn(&self, session: &str, prompt: &str, at: i64) {
        let user = format!("msg_{session}_1");
        let reply = format!("msg_{session}_2");
        self.message(&user, session, at, serde_json::json!({"role":"user","time":{"created":at}}));
        self.part(&format!("prt_{session}_1"), &user, session, at, serde_json::json!({"type":"text","text":prompt}));
        self.message(
            &reply,
            session,
            at + 1,
            serde_json::json!({"role":"assistant","time":{"created":at + 1,"completed":at + 2},"finish":"stop",
                               "tokens":{"input":11245,"output":7,"reasoning":22,"cache":{"read":0,"write":0}}}),
        );
        self.part(&format!("prt_{session}_2"), &reply, session, at + 2, serde_json::json!({"type":"text","text":"Done."}));
    }

    async fn boot(&self) -> Arc<AppState> {
        let store = Store::open(&self.claude.join("mogeung.db")).unwrap();
        let homes = AgentHomes {
            // The other CLIs point at nothing, so this stays about opencode
            // and never reaches the developer's real installs.
            codex: self.root.join("codex"),
            qwen: self.root.join("qwen"),
            opencode: self.oc.clone(),
        };
        let state = AppState::with_homes(store, self.claude.clone(), homes).unwrap();
        state.scan().await;
        state
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A process the kernel calls `opencode`, working in `dir`. Stopped on drop.
///
/// A `#!/bin/sh` script named `opencode`, because the kernel names a script's
/// process after the **script** — and a symlink to `sleep` does not work on a
/// machine whose `sleep` is the multi-call uutils binary, which dispatches on
/// the name it was run as and exits with *unknown program*. The script passes
/// `TERM` on to its `sleep`, so nothing outlives the test.
struct FakeOpencode(std::process::Child);

const FAKE: &str = "#!/bin/sh\ntrap 'kill $child 2>/dev/null; exit 0' TERM\nsleep \"$1\" & child=$!\nwait $child\n";

fn fake_binary(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join("opencode");
    std::fs::write(&path, FAKE).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

impl FakeOpencode {
    fn start(root: &Path, dir: &Path) -> FakeOpencode {
        let child = std::process::Command::new(fake_binary(root))
            .arg("120")
            .current_dir(dir)
            // opencode takes its directory from `$PWD`, and so does the scan.
            .env("PWD", dir)
            .spawn()
            .unwrap();
        // Give the kernel a moment to publish it under /proc.
        std::thread::sleep(std::time::Duration::from_millis(100));
        FakeOpencode(child)
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for FakeOpencode {
    fn drop(&mut self) {
        let _ = std::process::Command::new("kill").args(["-TERM", &self.pid().to_string()]).status();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn a_finished_opencode_turn_is_a_live_session_waiting_on_you() {
    let f = Fixture::new("finished");
    let dir = f.dir("api");
    let proc_ = FakeOpencode::start(&f.root, &dir);
    let t = now_ms();
    f.session("ses_fin", &dir, "Create hello.py and run it", t, None);
    f.finished_turn("ses_fin", "Create a file hello.py", t);

    let state = f.boot().await;
    let sessions = state.sessions.read().await;
    let s = sessions.get("ses_fin").expect("the opencode session reaches the queue");
    assert_eq!(s.source, SessionSource::OpenCode);
    assert!(s.alive, "an opencode process is working in its directory");
    assert_eq!(s.pid, Some(proc_.pid()));
    assert_eq!(s.live_status, Some(LiveStatus::Idle), "a stopped turn is your move");
    assert_eq!(s.cwd, dir.to_string_lossy());
    assert_eq!(s.title.as_deref(), Some("Create hello.py and run it"));
    assert_eq!(s.last_prompt.as_deref(), Some("Create a file hello.py"));
    assert_eq!(s.turns, 1);
    // From the session row: input plus the cached share, output plus reasoning.
    assert_eq!((s.tokens_in, s.tokens_out), (11345, 29));
    assert!(s.open_tools.is_empty());
}

#[tokio::test]
async fn a_prompt_opencode_wrote_down_ranks_as_approval_and_clears_when_answered() {
    let f = Fixture::new("approval");
    let dir = f.dir("api");
    let _proc = FakeOpencode::start(&f.root, &dir);
    let t = now_ms();
    f.session("ses_ask", &dir, "Echo permission-test command", t, None);
    f.message("msg_1", "ses_ask", t - 20_000, serde_json::json!({"role":"user","time":{"created":t - 20_000}}));
    f.part("prt_1", "msg_1", "ses_ask", t - 20_000, serde_json::json!({"type":"text","text":"run echo"}));
    f.message("msg_2", "ses_ask", t - 19_000, serde_json::json!({"role":"assistant","time":{"created":t - 19_000}}));
    // The row as it sat while opencode showed "Permission required".
    f.part(
        "prt_2",
        "msg_2",
        "ses_ask",
        t - 15_000,
        serde_json::json!({"type":"tool","tool":"bash","callID":"c1",
            "state":{"status":"running","input":{"command":"echo permission-test"},"time":{"start":t - 15_000}}}),
    );

    let state = f.boot().await;
    {
        let sessions = state.sessions.read().await;
        let s = sessions.get("ses_ask").unwrap();
        assert_eq!(s.live_status, Some(LiveStatus::Idle));
        let tool = s.awaiting_permission().expect("the prompt is an open tool, so it ranks APPROVE");
        assert_eq!(tool.name, "bash");
        assert_eq!(tool.summary, "echo permission-test");
    }

    // Approved: the same row, with metadata. The event sequence moves, so the
    // next scan re-reads it — no other signal is needed.
    f.part(
        "prt_2",
        "msg_2",
        "ses_ask",
        now_ms(),
        serde_json::json!({"type":"tool","tool":"bash","callID":"c1",
            "state":{"status":"running","input":{"command":"echo permission-test"},"metadata":{"output":""},"time":{"start":t - 15_000}}}),
    );
    state.scan().await;
    let sessions = state.sessions.read().await;
    let s = sessions.get("ses_ask").unwrap();
    assert_eq!(s.live_status, Some(LiveStatus::Busy), "approved and running");
    assert!(s.open_tools.is_empty(), "the prompt is gone once it is answered");
}

#[tokio::test]
async fn without_an_opencode_process_a_session_is_not_live_whatever_its_rows_say() {
    let f = Fixture::new("dead");
    let dir = f.dir("api");
    let t = now_ms();
    f.session("ses_dead", &dir, "Left open by a crash", t, None);
    // A turn a SIGKILL left open forever.
    f.message("msg_1", "ses_dead", t, serde_json::json!({"role":"user","time":{"created":t}}));
    f.message("msg_2", "ses_dead", t + 1, serde_json::json!({"role":"assistant","time":{"created":t + 1}}));

    let state = f.boot().await;
    let sessions = state.sessions.read().await;
    let s = sessions.get("ses_dead").unwrap();
    assert!(!s.alive);
    assert_eq!(s.live_status, None, "an open turn with no process is not a turn in progress");
}

#[tokio::test]
async fn yesterdays_session_in_the_same_directory_is_not_claimed_by_a_new_process() {
    let f = Fixture::new("yesterday");
    let dir = f.dir("api");
    let _proc = FakeOpencode::start(&f.root, &dir);
    let yesterday = now_ms() - 86_400_000;
    f.session("ses_old", &dir, "Yesterday's work", yesterday, None);
    f.finished_turn("ses_old", "fix the bug", yesterday);

    let state = f.boot().await;
    let sessions = state.sessions.read().await;
    let s = sessions.get("ses_old").unwrap();
    assert!(!s.alive, "a TUI opened today and not typed into holds no session yet");
}

#[tokio::test]
async fn subagent_and_archived_sessions_are_not_rows_of_yours() {
    let f = Fixture::new("children");
    let dir = f.dir("api");
    let t = now_ms();
    f.session("ses_parent", &dir, "Parent", t, None);
    f.session("ses_child", &dir, "List files (@general subagent)", t, Some("ses_parent"));
    f.session("ses_gone", &dir, "Archived", t, None);
    f.db().execute("UPDATE session SET time_archived = ?1 WHERE id = 'ses_gone'", [t]).unwrap();
    f.bump("ses_gone");

    let state = f.boot().await;
    let sessions = state.sessions.read().await;
    assert!(sessions.contains_key("ses_parent"));
    assert!(!sessions.contains_key("ses_child"), "a subagent's work shows in its parent");
    assert!(!sessions.contains_key("ses_gone"));
}

#[tokio::test]
async fn a_live_session_archived_later_stops_being_alive() {
    let f = Fixture::new("archived-live");
    let dir = f.dir("api");
    let _proc = FakeOpencode::start(&f.root, &dir);
    let t = now_ms();
    f.session("ses_arch", &dir, "Soon archived", t, None);
    f.finished_turn("ses_arch", "hello", t);
    let state = f.boot().await;
    assert!(state.sessions.read().await.get("ses_arch").unwrap().alive);

    f.db().execute("UPDATE session SET time_archived = ?1 WHERE id = 'ses_arch'", [now_ms()]).unwrap();
    f.bump("ses_arch");
    state.scan().await;
    let sessions = state.sessions.read().await;
    let s = sessions.get("ses_arch").unwrap();
    assert!(!s.alive, "a session the scan no longer visits must not keep the alive it was given");
    assert_eq!(s.live_status, None);
}

#[tokio::test]
async fn an_unknown_opencode_shape_reaches_the_health_alerts() {
    let f = Fixture::new("canary");
    let dir = f.dir("api");
    let t = now_ms();
    f.session("ses_drift", &dir, "Drift", t, None);
    f.finished_turn("ses_drift", "hello", t);
    f.part("prt_odd", "msg_ses_drift_2", "ses_drift", t + 3, serde_json::json!({"type":"telepathy"}));

    let state = f.boot().await;
    let h = state.health().await;
    let agent = h.agents.iter().find(|a| a.source == "opencode").expect("opencode has a health slot");
    assert!(agent.present);
    assert_eq!(agent.threads, 1);
    let named = h.alerts.iter().any(|a| {
        matches!(a, mogeung_core::health::Alert::UnknownEventType { event_type, .. } if event_type == "opencode/part/telepathy")
    });
    assert!(named, "an unknown part type must be named, not swallowed: {:?}", h.alerts);
}

#[tokio::test]
async fn a_database_whose_layout_moved_is_reported_rather_than_fatal() {
    let f = Fixture::new("layout");
    let conn = f.db();
    conn.execute_batch("DROP TABLE session; CREATE TABLE session (id text PRIMARY KEY, title text, time_created integer, time_updated integer);")
        .unwrap();
    conn.execute("INSERT INTO session VALUES ('ses_x', 'x', ?1, ?1)", [now_ms()]).unwrap();

    let state = f.boot().await;
    let h = state.health().await;
    let agent = h.agents.iter().find(|a| a.source == "opencode").unwrap();
    let error = agent.error.as_deref().expect("a missing column is said, not swallowed");
    assert!(error.contains("directory"), "{error}");
    assert!(state.sessions.read().await.is_empty());
}

#[tokio::test]
async fn no_opencode_install_reports_nothing() {
    let f = Fixture::new("absent");
    std::fs::remove_dir_all(&f.oc).unwrap();
    let state = f.boot().await;
    let h = state.health().await;
    assert!(!h.agents.iter().any(|a| a.source == "opencode"), "absence is not an error state");
}

/// The database is opencode's, and holds its credentials too. mogeung reads
/// it and must never so much as create a side file beside it.
#[tokio::test]
async fn the_scan_never_writes_to_opencodes_database() {
    let f = Fixture::new("readonly");
    let dir = f.dir("api");
    let t = now_ms();
    f.session("ses_ro", &dir, "Read me", t, None);
    f.finished_turn("ses_ro", "hello", t);
    let listing = |d: &Path| {
        let mut v: Vec<String> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    };
    let before_files = listing(&f.oc);
    let before_bytes = std::fs::read(f.oc.join("opencode.db")).unwrap();

    let state = f.boot().await;
    state.scan().await;
    assert!(state.sessions.read().await.contains_key("ses_ro"));
    assert_eq!(listing(&f.oc), before_files, "no -wal, -shm or journal was created");
    assert_eq!(std::fs::read(f.oc.join("opencode.db")).unwrap(), before_bytes, "not a byte changed");
}

/// Hostable, not merely seen: an opencode running in a tmux pane resolves an
/// attach target, which is what lets the Agent pane show it.
#[tokio::test]
async fn an_opencode_session_under_tmux_can_be_hosted() {
    fn tmux(args: &[&str]) -> Option<String> {
        let out = std::process::Command::new("tmux").args(args).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
    if std::process::Command::new("tmux").arg("-V").output().is_err() {
        eprintln!("skipping: tmux is not installed");
        return;
    }
    let f = Fixture::new("tmux");
    let dir = f.dir("hosted");
    let link = fake_binary(&f.root);

    let name = format!("mogeung-octest-{}", std::process::id());
    let target = format!("={name}");
    let _ = tmux(&["kill-session", "-t", &target]);
    let d = dir.to_string_lossy().into_owned();
    let pwd = format!("PWD={d}");
    // Several arguments, so tmux execs the script directly: the pane's own
    // process *is* the opencode one.
    if tmux(&["new-session", "-d", "-s", &name, "-c", &d, "-e", &pwd, &link.to_string_lossy(), "120"]).is_none() {
        eprintln!("skipping: could not start a tmux session");
        return;
    }
    std::thread::sleep(std::time::Duration::from_millis(100));

    let outcome = async {
        let t = now_ms();
        f.session("ses_tmux", &dir, "Hosted", t, None);
        f.finished_turn("ses_tmux", "hello", t);
        let state = f.boot().await;
        let sessions = state.sessions.read().await;
        let s = sessions.get("ses_tmux")?;
        Some((s.alive, s.tmux_target.clone()))
    }
    .await;
    let _ = tmux(&["kill-session", "-t", &target]);

    let (alive, resolved) = outcome.expect("the opencode session should exist");
    assert!(alive);
    assert_eq!(resolved.as_deref(), Some(format!("{name}:0.0").as_str()));
}

/// What a session wrote is its diff, and the rest of a shared worktree is not.
///
/// The first live run against a real `opencode.db` showed every finished
/// session as *ended with no changes*: no transcript path meant no diff was
/// ever computed. Now the scan computes one, scoped by the files opencode says
/// the session wrote.
#[tokio::test]
async fn what_a_session_wrote_is_its_diff_and_the_rest_of_the_worktree_is_not() {
    let f = Fixture::new("diff");
    let dir = f.dir("repo");
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(&dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(dir.join("a.txt"), "x\n").unwrap();
    std::fs::write(dir.join("b.txt"), "x\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "init"]);
    // The session edits a.txt; something else in the same worktree edits b.txt.
    std::fs::write(dir.join("a.txt"), "y\n").unwrap();
    std::fs::write(dir.join("b.txt"), "z\n").unwrap();

    let _proc = FakeOpencode::start(&f.root, &dir);
    let t = now_ms();
    f.session("ses_diff", &dir, "Replace x with y in a.txt", t, None);
    f.finished_turn("ses_diff", "change x to y in a.txt", t);
    let a = dir.join("a.txt").to_string_lossy().into_owned();
    f.part("prt_patch", "msg_ses_diff_2", "ses_diff", t + 3, serde_json::json!({"type":"patch","hash":"5dad","files":[a]}));

    let state = f.boot().await;
    {
        let sessions = state.sessions.read().await;
        let s = sessions.get("ses_diff").unwrap();
        assert!(s.touched_files.iter().any(|p| p.ends_with("a.txt")));
        assert!(!s.transcript_path.is_empty(), "an empty one tells the diff there is nothing to attribute");
    }
    let change = state.change_for_request("ses_diff", false).await.expect("a diff was computed");
    let paths: Vec<&str> = change.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["a.txt"], "b.txt is someone else's");
}

/// A headless launch from the New session window starts an opencode that has
/// written nothing — opencode records a session only when you send a prompt.
/// It must still be on the board, and ranked where the default scope shows
/// it: at its prompt, it is **waiting for you**. Reported 2026-09-23: *"the
/// window closed. But nothing appear in the ATTENTION list."* It had been
/// there all along as a pane row ranked *running*, which that scope hides.
#[tokio::test]
async fn a_fresh_opencode_in_tmux_is_waiting_for_you_before_it_has_written_anything() {
    fn tmux(args: &[&str]) -> Option<String> {
        let out = std::process::Command::new("tmux").args(args).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
    if std::process::Command::new("tmux").arg("-V").output().is_err() {
        eprintln!("skipping: tmux is not installed");
        return;
    }
    let f = Fixture::new("fresh");
    let dir = f.dir("fresh");
    let link = fake_binary(&f.root);
    let name = format!("mogeung-ocfresh-{}", std::process::id());
    let target = format!("={name}");
    let _ = tmux(&["kill-session", "-t", &target]);
    let d = dir.to_string_lossy().into_owned();
    if tmux(&["new-session", "-d", "-s", &name, "-c", &d, &link.to_string_lossy(), "120"]).is_none() {
        eprintln!("skipping: could not start a tmux session");
        return;
    }
    std::thread::sleep(std::time::Duration::from_millis(150));

    // The database exists and holds nothing: opencode has not been spoken to.
    let outcome = async {
        let state = f.boot().await;
        let mogeung_core::wire::ServerMsg::Snapshot { sessions, queue, .. } = state.snapshot(false).await else {
            return None;
        };
        let row = sessions.into_iter().find(|s| s.tmux_target.as_deref() == Some(&format!("{name}:0.0")))?;
        let item = queue.into_iter().find(|i| i.session_id == row.id)?;
        Some((row, item))
    }
    .await;
    let _ = tmux(&["kill-session", "-t", &target]);

    let (row, item) = outcome.expect("the fresh opencode's pane is on the board");
    assert_eq!(row.source, SessionSource::OpenCode);
    assert!(row.provisional, "nothing is written yet, and the row says so");
    assert_eq!(row.live_status, Some(LiveStatus::Idle));
    assert_eq!(item.reason, mogeung_core::attention::AttentionReason::AwaitingInput);
    // Its clock is the process's, not the scan's.
    assert!(row.status_since.is_some_and(|t| t <= chrono::Utc::now()));
}

/// What a window that is **already open** is told. Reported 2026-09-23, after
/// the fix above: *"I still can't see it if I start a new headless opencode."*
/// The daemon had the row, but a pane row reached a window only in the
/// snapshot it got on connecting — never in a broadcast, and not even in the
/// queue broadcasts that followed, so an open window could not see it at all.
///
/// And when the agent is spoken to, the row must hand over rather than vanish:
/// the real session first, then the pane row **dead with the same pid and
/// cwd** — the window's succession rule, which moves the selection and any
/// held pane onto the real session — then the row removed.
#[tokio::test]
async fn an_open_window_is_told_when_a_fresh_opencode_appears_and_when_it_becomes_a_session() {
    use mogeung_core::wire::ServerMsg;
    fn tmux(args: &[&str]) -> Option<String> {
        let out = std::process::Command::new("tmux").args(args).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
    if std::process::Command::new("tmux").arg("-V").output().is_err() {
        eprintln!("skipping: tmux is not installed");
        return;
    }
    fn drain(rx: &mut tokio::sync::broadcast::Receiver<ServerMsg>) -> Vec<ServerMsg> {
        let mut out = Vec::new();
        while let Ok(m) = rx.try_recv() {
            out.push(m);
        }
        out
    }

    let f = Fixture::new("told");
    let dir = f.dir("told");
    let link = fake_binary(&f.root);
    // The window is open before the agent starts.
    let state = f.boot().await;
    let mut rx = state.tx.subscribe();

    let name = format!("mogeung-octold-{}", std::process::id());
    let target = format!("={name}");
    let _ = tmux(&["kill-session", "-t", &target]);
    let d = dir.to_string_lossy().into_owned();
    if tmux(&["new-session", "-d", "-s", &name, "-c", &d, &link.to_string_lossy(), "120"]).is_none() {
        eprintln!("skipping: could not start a tmux session");
        return;
    }
    std::thread::sleep(std::time::Duration::from_millis(150));

    let outcome = async {
        state.scan().await;
        let first = drain(&mut rx);
        let row = first.iter().find_map(|m| match m {
            ServerMsg::SessionUpdated { session } if session.provisional && session.tmux_target.as_deref() == Some(&format!("{name}:0.0")) => {
                Some(session.as_ref().clone())
            }
            _ => None,
        })?;
        let queued = first.iter().any(|m| matches!(m, ServerMsg::Queue { queue } if queue.iter().any(|i| i.session_id == row.id)));

        // Unchanged on the next tick: nothing about it is said again.
        state.scan().await;
        let quiet = !drain(&mut rx).iter().any(|m| matches!(m, ServerMsg::SessionUpdated { session } if session.id == row.id));

        // Spoken to: opencode writes its session, in this directory, after the
        // process started.
        let t = now_ms();
        f.session("ses_told", &dir, "Hello", t, None);
        f.finished_turn("ses_told", "hello", t);
        state.scan().await;
        let second = drain(&mut rx);
        let at = |pred: &dyn Fn(&ServerMsg) -> bool| second.iter().position(pred);
        let real = at(&|m| matches!(m, ServerMsg::SessionUpdated { session } if session.id == "ses_told" && session.alive))?;
        let dead = at(&|m| matches!(m, ServerMsg::SessionUpdated { session } if session.id == row.id && !session.alive))?;
        let removed = at(&|m| matches!(m, ServerMsg::SessionRemoved { session_id } if *session_id == row.id))?;
        let (heir, ghost) = match (&second[real], &second[dead]) {
            (ServerMsg::SessionUpdated { session: h }, ServerMsg::SessionUpdated { session: g }) => (h.clone(), g.clone()),
            _ => return None,
        };
        Some((row, queued, quiet, real, dead, removed, heir, ghost))
    }
    .await;
    let _ = tmux(&["kill-session", "-t", &target]);

    let (row, queued, quiet, real, dead, removed, heir, ghost) = outcome.expect("the pane row and its hand-over were broadcast");
    assert_eq!(row.source, SessionSource::OpenCode);
    assert!(queued, "the queue broadcast ranks the pane row, or the window drops it");
    assert!(quiet, "an unchanged pane row is not announced every tick");
    assert!(real < dead && dead < removed, "real session, then the row dead, then removed: {real} {dead} {removed}");
    assert_eq!(ghost.pid, heir.pid, "succession needs one pid");
    assert_eq!(ghost.cwd, heir.cwd, "and one cwd");
}
