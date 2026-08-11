//! The daemon owns a process. `R-N4`.
//!
//! [ADR-0025](../../../docs/decisions/0025-run-a-process-you-named-never-an-agent.md)
//! clause 3: run state lives where every other piece of state lives, so a run
//! survives the window closing, is visible to a second client, and works
//! against a remote daemon — where the debuggee runs on the machine that has
//! the files, which is the whole point of `R-I6`.
//!
//! # Nothing here accepts a command
//!
//! Clause 1 is *named, not supplied*, and this module is where it is actually
//! enforced: [`Runs::start`] takes a **configuration id**, looks it up in what
//! the repository itself produced, and refuses if it is not there. There is no
//! path from a client's bytes to an argv. That is the difference between "a
//! port that can run this repository's test suite" and "a port that is a
//! shell".
//!
//! # And nothing here starts an agent
//!
//! Clause 2, checked at the last possible moment — on the program that is about
//! to be spawned, after every lookup and substitution — because a check on the
//! *configuration* could be walked around by anything that rewrites one. The
//! limit is known: a script in the repository that goes on to call `claude`
//! walks past it, and ADR-0025 says so in terms.
//!
//! # Orphans
//!
//! A child is put in its **own process group** and stopped by signalling the
//! group, so a `cargo test` that spawned a test binary does not leave it
//! running. The daemon kills what it holds on shutdown; anything that survives
//! a crash is the operating system's to reap, which is stated rather than
//! pretended about.

use mogeung_core::run::{agent_refusal, is_agent, Run, RunConfig, RunLine, RunState};
use mogeung_core::session::SessionId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, Mutex};

/// How many output lines one run keeps.
///
/// A ring rather than everything: `cargo test` on a large workspace emits tens
/// of thousands of lines, and a client that reconnects wants the end of it. The
/// bound is stated in the payload rather than silently applied, because a log
/// that quietly loses its middle is worse than one that says it did.
pub const MAX_LINES: usize = 2_000;

/// What a running process needs beyond what a client sees.
struct Live {
    run: Run,
    lines: std::collections::VecDeque<RunLine>,
    next_seq: u64,
    /// `None` once it has exited.
    pid: Option<u32>,
}

/// Every run this daemon owns.
#[derive(Clone)]
pub struct Runs {
    inner: Arc<Mutex<HashMap<String, Live>>>,
    tx: broadcast::Sender<RunEvent>,
    /// ADR-0025 clause 4: spawning is opt-in on any bind that is not loopback.
    ///
    /// A `OnceLock` for the reason [`crate::state::AppState::writes_allowed`]
    /// is one: only the code that *binds the socket* knows the answer, and this
    /// constructor is shared with tests and with the window's own hosted
    /// daemon. Unset reads as allowed, which is the loopback posture every
    /// unconfigured caller actually has.
    allowed: Arc<std::sync::OnceLock<bool>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
}

/// What the daemon publishes about runs.
#[derive(Debug, Clone)]
pub enum RunEvent {
    Started(Run),
    Output(RunLine),
    Ended(Run),
}

impl Default for Runs {
    fn default() -> Self {
        Self::new()
    }
}

impl Runs {
    pub fn new() -> Self {
        Runs {
            inner: Arc::new(Mutex::new(HashMap::new())),
            tx: broadcast::channel(512).0,
            allowed: Arc::new(std::sync::OnceLock::new()),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }

    /// Settled once, by whoever bound the socket. ADR-0025 clause 4.
    pub fn set_allowed(&self, yes: bool) {
        let _ = self.allowed.set(yes);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RunEvent> {
        self.tx.subscribe()
    }

    pub fn allowed(&self) -> bool {
        *self.allowed.get().unwrap_or(&true)
    }

    /// Every run, newest first.
    pub async fn list(&self) -> Vec<Run> {
        let g = self.inner.lock().await;
        let mut out: Vec<Run> = g.values().map(|l| l.run.clone()).collect();
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        out
    }

    /// The output a client reconnecting needs.
    pub async fn lines(&self, run_id: &str) -> Vec<RunLine> {
        let g = self.inner.lock().await;
        g.get(run_id).map(|l| l.lines.iter().cloned().collect()).unwrap_or_default()
    }

    /// Start a configuration **by id**, in `repo`.
    ///
    /// The four refusals, in the order they are cheapest to make: the policy
    /// gate, the lookup (which is clause 1 — an id that is not in what the
    /// repository produced is not a command anybody can run), the agent check,
    /// and finally whatever the operating system says about spawning.
    pub async fn start(
        &self,
        repo: &Path,
        config_id: &str,
        session_id: Option<SessionId>,
        configs: &[RunConfig],
    ) -> Result<Run, String> {
        if !self.allowed() {
            return Err(
                "this daemon is bound to an address other than loopback and was started without \
                 `--allow-run`, so it will not start processes (ADR-0025 clause 4)"
                    .into(),
            );
        }
        let Some(cfg) = configs.iter().find(|c| c.id == config_id) else {
            // Clause 1, and the reason the wire carries an id.
            return Err(format!(
                "no run configuration `{config_id}` in this repository — mogeung runs what the \
                 repository already describes and never a command it was handed (ADR-0025 clause 1)"
            ));
        };
        if let Some(why) = &cfg.unrunnable {
            return Err(why.clone());
        }
        // Clause 2, on the program actually about to be spawned.
        if is_agent(&cfg.program) {
            return Err(agent_refusal(&cfg.program));
        }

        let dir = if cfg.dir.is_empty() { repo.to_path_buf() } else { repo.join(&cfg.dir) };
        let id = format!("run-{}", self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed));

        // **Values are read here and nowhere else.** `R-N6`: a `RunConfig`
        // travels to every connected client, so it carries variable *names*
        // only, and the secrets are fetched from disk at the last moment.
        let env = crate::runconfig::env_for(repo, &cfg.id);

        let mut cmd = tokio::process::Command::new(&cfg.program);
        cmd.envs(env)
            .args(&cfg.args)
            .current_dir(&dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        // Its own process group, so stopping the run stops what it spawned.
        // A `cargo test` that leaves its test binary running is a stop button
        // that does not stop anything.
        #[cfg(unix)]
        unsafe {
            cmd.pre_exec(|| {
                libc_setsid();
                Ok(())
            });
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return Err(format!("could not start `{}`: {e}", cfg.command_line()));
            }
        };

        let run = Run {
            id: id.clone(),
            config_id: cfg.id.clone(),
            name: cfg.name.clone(),
            command: cfg.command_line(),
            session_id,
            state: RunState::Running,
            exit_code: None,
            started_at: chrono::Utc::now(),
            ended_at: None,
            dropped: 0,
        };

        let pid = child.id();
        {
            let mut g = self.inner.lock().await;
            g.insert(
                id.clone(),
                Live {
                    run: run.clone(),
                    lines: Default::default(),
                    next_seq: 0,
                    pid,
                },
            );
        }
        let _ = self.tx.send(RunEvent::Started(run.clone()));

        // Reading both pipes concurrently, because a child that fills one while
        // nobody drains it blocks forever — the classic way a "hung" run is
        // actually a full stderr buffer.
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        for (pipe, is_err) in [(stdout.map(Box::new), false)] {
            if let Some(p) = pipe {
                self.pump(id.clone(), *p, is_err);
            }
        }
        if let Some(p) = stderr {
            self.pump_err(id.clone(), p);
        }

        let this = self.clone();
        let wait_id = id.clone();
        tokio::spawn(async move {
            let status = child.wait().await;
            this.finish(&wait_id, status.ok().and_then(|s| s.code())).await;
        });

        Ok(run)
    }

    fn pump(&self, run_id: String, pipe: tokio::process::ChildStdout, stderr: bool) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(text)) = lines.next_line().await {
                this.push(&run_id, text, stderr).await;
            }
        });
    }

    fn pump_err(&self, run_id: String, pipe: tokio::process::ChildStderr) {
        let this = self.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(pipe).lines();
            while let Ok(Some(text)) = lines.next_line().await {
                this.push(&run_id, text, true).await;
            }
        });
    }

    async fn push(&self, run_id: &str, text: String, stderr: bool) {
        let line = {
            let mut g = self.inner.lock().await;
            let Some(live) = g.get_mut(run_id) else { return };
            let line = RunLine { run_id: run_id.to_string(), seq: live.next_seq, text, stderr };
            live.next_seq += 1;
            live.lines.push_back(line.clone());
            while live.lines.len() > MAX_LINES {
                live.lines.pop_front();
                live.run.dropped += 1;
            }
            line
        };
        let _ = self.tx.send(RunEvent::Output(line));
    }

    async fn finish(&self, run_id: &str, code: Option<i32>) {
        let ended = {
            let mut g = self.inner.lock().await;
            let Some(live) = g.get_mut(run_id) else { return };
            // A run stopped by a person keeps that state: "you stopped it" and
            // "it failed" are different answers to *did the tests pass*.
            if live.run.state == RunState::Running {
                live.run.state = RunState::Exited;
            }
            live.run.exit_code = code;
            live.run.ended_at = Some(chrono::Utc::now());
            live.pid = None;
            live.run.clone()
        };
        let _ = self.tx.send(RunEvent::Ended(ended));
    }

    /// Stop a run, and everything it started.
    pub async fn stop(&self, run_id: &str) -> Result<(), String> {
        let pid = {
            let mut g = self.inner.lock().await;
            let Some(live) = g.get_mut(run_id) else {
                return Err(format!("no run `{run_id}`"));
            };
            if live.run.state != RunState::Running {
                return Ok(());
            }
            live.run.state = RunState::Stopped;
            live.pid
        };
        let Some(pid) = pid else { return Ok(()) };
        // The **group**, negated — see the module note about orphans.
        #[cfg(unix)]
        unsafe {
            libc_kill(-(pid as i32), 15);
        }
        Ok(())
    }

    /// Stop everything, for daemon shutdown.
    pub async fn stop_all(&self) {
        let ids: Vec<String> = {
            let g = self.inner.lock().await;
            g.values()
                .filter(|l| l.run.state == RunState::Running)
                .map(|l| l.run.id.clone())
                .collect()
        };
        for id in ids {
            let _ = self.stop(&id).await;
        }
    }
}

// Avoiding a `libc` dependency for two calls, the way `watcher.rs` already does.
#[cfg(unix)]
extern "C" {
    #[link_name = "setsid"]
    fn libc_setsid() -> i32;
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

/// The bind address decides whether runs are allowed at all. ADR-0025 clause 4.
///
/// Loopback is the same trust boundary as the terminal panel, which can already
/// run anything. Anything else needs `--allow-run` **and** the token `R-I10`
/// already demands — two deliberate acts, not one.
pub fn runs_allowed(bind: std::net::SocketAddr, allow_run_flag: bool) -> bool {
    bind.ip().is_loopback() || allow_run_flag
}

/// Where a session's runs happen: its repository root, or its working
/// directory when it is not in a repository.
pub fn repo_of(session: &mogeung_core::session::Session) -> PathBuf {
    session
        .repo_root
        .clone()
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&session.cwd))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::run::Origin;

    fn cfg(program: &str, args: &[&str]) -> RunConfig {
        RunConfig::new(
            Origin::Detected,
            "t",
            program,
            args.iter().map(|s| s.to_string()).collect(),
            "",
        )
    }

    async fn until_ended(runs: &Runs, id: &str) -> Run {
        for _ in 0..200 {
            let list = runs.list().await;
            if let Some(r) = list.iter().find(|r| r.id == id) {
                if r.state != RunState::Running {
                    return r.clone();
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("run {id} never ended");
    }

    #[tokio::test]
    async fn a_run_reports_its_output_and_its_exit_code() {
        let runs = Runs::new();
        let c = cfg("/bin/echo", &["hello"]);
        let started = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .expect("starts");

        let done = until_ended(&runs, &started.id).await;
        assert_eq!(done.state, RunState::Exited);
        assert_eq!(done.exit_code, Some(0));

        let lines = runs.lines(&started.id).await;
        assert!(lines.iter().any(|l| l.text == "hello"), "{lines:?}");
    }

    /// A failing command is not a failure of mogeung, and the code is the whole
    /// point — `R-N7` binds a run to a claim, and *did it really* is answered
    /// by this number.
    #[tokio::test]
    async fn a_non_zero_exit_is_reported_as_itself() {
        let runs = Runs::new();
        let c = cfg("/bin/sh", &["-c", "exit 3"]);
        let started = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .unwrap();
        let done = until_ended(&runs, &started.id).await;
        assert_eq!(done.exit_code, Some(3));
        assert_eq!(done.state, RunState::Exited);
    }

    /// Stopping has to actually stop it, and *you stopped it* must not read as
    /// *it failed*.
    #[tokio::test]
    async fn stopping_a_run_ends_it_and_says_who_ended_it() {
        let runs = Runs::new();
        let c = cfg("/bin/sh", &["-c", "sleep 30"]);
        let started = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        runs.stop(&started.id).await.expect("stops");

        let done = until_ended(&runs, &started.id).await;
        assert_eq!(done.state, RunState::Stopped, "not Exited: a person did this");
    }

    /// stderr is kept and marked, because a build's errors are the part you
    /// wanted to see.
    #[tokio::test]
    async fn stderr_is_captured_and_marked() {
        let runs = Runs::new();
        let c = cfg("/bin/sh", &["-c", "echo oops >&2"]);
        let started = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .unwrap();
        until_ended(&runs, &started.id).await;

        let lines = runs.lines(&started.id).await;
        let err = lines.iter().find(|l| l.text == "oops").expect("stderr line");
        assert!(err.stderr);
    }

    /// The ring is bounded and **says** what it dropped. A log that quietly
    /// loses its middle is worse than one that admits it.
    #[tokio::test]
    async fn output_is_bounded_and_the_loss_is_stated() {
        let runs = Runs::new();
        let n = MAX_LINES + 200;
        let c = cfg("/bin/sh", &["-c", &format!("seq 1 {n}")]);
        let started = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .unwrap();
        let done = until_ended(&runs, &started.id).await;

        let lines = runs.lines(&started.id).await;
        assert!(lines.len() <= MAX_LINES, "kept {}", lines.len());
        assert!(done.dropped > 0, "the loss must be stated");
        // The **end** is what is kept: a reconnecting client wants the result.
        assert_eq!(lines.last().unwrap().text, n.to_string());
    }

    /// **ADR-0025 clause 1.** An id that is not in what the repository produced
    /// is not something anybody can run, and this is the test that would fail
    /// if a command string ever became reachable.
    #[tokio::test]
    async fn an_id_the_repository_did_not_produce_is_refused() {
        let runs = Runs::new();
        let err = runs
            .start(Path::new("/tmp"), "detected:rm-rf", None, &[])
            .await
            .expect_err("must refuse");
        assert!(err.contains("clause 1"), "{err}");
    }

    /// **ADR-0025 clause 2**, and the refusal names the clause because
    /// otherwise it reads as a button that does not work.
    #[tokio::test]
    async fn a_configuration_that_would_start_an_agent_is_refused_by_name() {
        let runs = Runs::new();
        let c = cfg("claude", &["--dangerously-skip-permissions"]);
        let err = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .expect_err("must refuse");
        assert!(err.contains("ADR-0025"), "{err}");
        assert!(err.contains("claude"), "{err}");
    }

    /// **ADR-0025 clause 4.** A daemon reachable from the network does not
    /// spawn without a second deliberate act.
    #[tokio::test]
    async fn a_daemon_without_allow_run_starts_nothing() {
        let runs = Runs::new();
        runs.set_allowed(false);
        let c = cfg("/bin/echo", &["hi"]);
        let err = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .expect_err("must refuse");
        assert!(err.contains("--allow-run"), "{err}");
    }

    /// The policy itself: loopback is the terminal panel's trust boundary,
    /// anything else is a decision.
    #[test]
    fn only_loopback_runs_without_being_asked_twice() {
        assert!(runs_allowed("127.0.0.1:7717".parse().unwrap(), false));
        assert!(runs_allowed("[::1]:7717".parse().unwrap(), false));
        assert!(!runs_allowed("0.0.0.0:7717".parse().unwrap(), false));
        assert!(!runs_allowed("192.168.1.5:7717".parse().unwrap(), false));
        assert!(runs_allowed("0.0.0.0:7717".parse().unwrap(), true));
    }

    /// An entry listed as unrunnable is listed, not startable — `R-N2`'s
    /// refusals have to hold at the point something would actually spawn.
    #[tokio::test]
    async fn an_unrunnable_configuration_cannot_be_started() {
        let runs = Runs::new();
        let c = cfg("/bin/echo", &["hi"]).unclassified("holodeck");
        let err = runs
            .start(Path::new("/tmp"), &c.id, None, std::slice::from_ref(&c))
            .await
            .expect_err("must refuse");
        assert!(err.contains("holodeck"), "{err}");
    }

    /// A run carries the session it came from, which is the whole of `R-N7`'s
    /// mechanism — without it a run cannot be shown beside the claim.
    #[tokio::test]
    async fn a_run_remembers_the_session_it_was_started_from() {
        let runs = Runs::new();
        let c = cfg("/bin/echo", &["hi"]);
        let started = runs
            .start(Path::new("/tmp"), &c.id, Some("s-1".into()), std::slice::from_ref(&c))
            .await
            .unwrap();
        assert_eq!(started.session_id.as_deref(), Some("s-1"));
        assert_eq!(runs.list().await[0].session_id.as_deref(), Some("s-1"));
    }
}
