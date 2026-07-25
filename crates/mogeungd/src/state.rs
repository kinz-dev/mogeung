//! Daemon state and the run supervisor.

use crate::adapter::{self, SpawnSpec};
use crate::git;
use crate::store::Store;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use mogeung_core::attention::{rank, AttentionConfig};
use mogeung_core::transcript::NoticeLevel;
use mogeung_core::{
    AgentKind, Change, EventKind, NewRunSpec, Run, RunId, RunStatus, ServerMsg, TranscriptEvent,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{broadcast, oneshot, Mutex, RwLock};

pub struct AppState {
    pub store: Store,
    pub runs: RwLock<HashMap<RunId, Run>>,
    pub changes: RwLock<HashMap<RunId, Change>>,
    pub repos: RwLock<Vec<String>>,
    pub tx: broadcast::Sender<ServerMsg>,
    /// Cancel channels for in-flight runs.
    cancels: Mutex<HashMap<RunId, oneshot::Sender<()>>>,
    seqs: Mutex<HashMap<RunId, u64>>,
    pub attention: AttentionConfig,
}

impl AppState {
    pub fn new(store: Store) -> Result<Arc<Self>> {
        let loaded = store.load_runs()?;
        let repos = store.repos()?;
        let mut runs = HashMap::new();
        let mut seqs = HashMap::new();
        for mut r in loaded {
            // Nothing survives a daemon restart mid-run: the child process died
            // with us. Mark it honestly rather than showing a phantom "running".
            if !r.status.is_terminal() {
                r.status = RunStatus::Failed;
                r.error = Some("daemon restarted while this run was in flight".into());
                r.ended_at = Some(Utc::now());
                let _ = store.save_run(&r);
            }
            seqs.insert(r.id, store.max_seq(r.id).unwrap_or(0));
            runs.insert(r.id, r);
        }
        let (tx, _) = broadcast::channel(4096);
        Ok(Arc::new(AppState {
            store,
            runs: RwLock::new(runs),
            changes: RwLock::new(HashMap::new()),
            repos: RwLock::new(repos),
            tx,
            cancels: Mutex::new(HashMap::new()),
            seqs: Mutex::new(seqs),
            attention: AttentionConfig::default(),
        }))
    }

    pub fn broadcast(&self, msg: ServerMsg) {
        // A send error only means nobody is listening, which is fine.
        let _ = self.tx.send(msg);
    }

    pub async fn snapshot(&self) -> ServerMsg {
        let runs: Vec<Run> = self.runs.read().await.values().cloned().collect();
        let queue = rank(&runs, Utc::now(), &self.attention);
        ServerMsg::Snapshot {
            runs,
            repos: self.repos.read().await.clone(),
            queue,
        }
    }

    pub async fn publish_queue(&self) {
        let runs: Vec<Run> = self.runs.read().await.values().cloned().collect();
        let queue = rank(&runs, Utc::now(), &self.attention);
        self.broadcast(ServerMsg::Queue { queue });
    }

    async fn put_run(&self, run: Run) {
        let _ = self.store.save_run(&run);
        self.runs.write().await.insert(run.id, run.clone());
        self.broadcast(ServerMsg::RunUpdated { run });
    }

    pub async fn get_run(&self, id: RunId) -> Option<Run> {
        self.runs.read().await.get(&id).cloned()
    }

    async fn next_seq(&self, id: RunId) -> u64 {
        let mut s = self.seqs.lock().await;
        let e = s.entry(id).or_insert(0);
        *e += 1;
        *e
    }

    /// Persist and broadcast a batch of transcript events.
    async fn emit(&self, run_id: RunId, kinds: Vec<EventKind>) {
        if kinds.is_empty() {
            return;
        }
        let mut evs = Vec::with_capacity(kinds.len());
        for kind in kinds {
            let ev = TranscriptEvent {
                run_id,
                seq: self.next_seq(run_id).await,
                ts: Utc::now(),
                kind,
            };
            let _ = self.store.append_event(&ev);
            evs.push(ev);
        }
        self.broadcast(ServerMsg::Events { events: evs });
    }

    // -----------------------------------------------------------------------
    // Repos
    // -----------------------------------------------------------------------

    pub async fn add_repo(&self, path: &str) -> Result<()> {
        let p = PathBuf::from(shellexpand(path));
        if !p.exists() {
            return Err(anyhow!("path does not exist: {}", p.display()));
        }
        if !git::is_repo(&p) {
            return Err(anyhow!("not a git repository: {}", p.display()));
        }
        let root = git::repo_root(&p)?.to_string_lossy().to_string();
        self.store.add_repo(&root)?;
        let mut repos = self.repos.write().await;
        if !repos.contains(&root) {
            repos.push(root);
            repos.sort();
        }
        self.broadcast(ServerMsg::RepoList {
            repos: repos.clone(),
        });
        Ok(())
    }

    pub async fn remove_repo(&self, path: &str) -> Result<()> {
        self.store.remove_repo(path)?;
        let mut repos = self.repos.write().await;
        repos.retain(|r| r != path);
        self.broadcast(ServerMsg::RepoList {
            repos: repos.clone(),
        });
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Change computation
    // -----------------------------------------------------------------------

    pub async fn recompute_change(&self, run_id: RunId) -> Option<Change> {
        let run = self.get_run(run_id).await?;
        let reviewed = self.store.reviewed_anchors(run.root).unwrap_or_default();
        let change = git::compute_change(Path::new(&run.cwd), run.base_sha.as_deref(), &reviewed);

        // Keep the run's headline stats in step with the diff we just computed.
        let mut runs = self.runs.write().await;
        if let Some(r) = runs.get_mut(&run_id) {
            let files = change.files.len() as u32;
            if r.files_changed != files
                || r.insertions != change.insertions
                || r.deletions != change.deletions
            {
                r.files_changed = files;
                r.insertions = change.insertions;
                r.deletions = change.deletions;
                let updated = r.clone();
                let _ = self.store.save_run(&updated);
                self.broadcast(ServerMsg::RunUpdated { run: updated });
            }
        }
        drop(runs);

        self.changes.write().await.insert(run_id, change.clone());
        self.broadcast(ServerMsg::ChangeUpdated {
            run_id,
            change: change.clone(),
        });
        Some(change)
    }

    /// Review marks are keyed by the session root, so they survive follow-ups.
    async fn review_key(&self, run_id: RunId) -> RunId {
        self.get_run(run_id).await.map(|r| r.root).unwrap_or(run_id)
    }

    pub async fn set_hunk_reviewed(&self, run_id: RunId, anchor: &str, reviewed: bool) {
        let key = self.review_key(run_id).await;
        let _ = self.store.set_reviewed(key, anchor, reviewed);
        self.mark_reviewed_status(run_id).await;
        self.recompute_change(run_id).await;
        self.publish_queue().await;
    }

    pub async fn review_all(&self, run_id: RunId) {
        let key = self.review_key(run_id).await;
        // Recompute rather than trusting the cache, so "review all" cannot mark
        // a stale set of hunks and silently miss new ones.
        if let Some(change) = self.recompute_change(run_id).await {
            for f in &change.files {
                for h in &f.hunks {
                    let _ = self.store.set_reviewed(key, &h.anchor, true);
                }
            }
        }
        self.mark_reviewed_status(run_id).await;
        self.recompute_change(run_id).await;
        self.publish_queue().await;
    }

    /// Promote a finished run to `Reviewed` once every hunk is marked, and demote
    /// it back to `AwaitingReview` if new unreviewed work appears.
    async fn mark_reviewed_status(&self, run_id: RunId) {
        let run = match self.get_run(run_id).await {
            Some(r) => r,
            None => return,
        };
        if !matches!(run.status, RunStatus::AwaitingReview | RunStatus::Reviewed) {
            return;
        }
        let reviewed = self.store.reviewed_anchors(run.root).unwrap_or_default();
        let change = git::compute_change(Path::new(&run.cwd), run.base_sha.as_deref(), &reviewed);
        let all_seen = change.unreviewed_hunks() == 0;
        let want = if all_seen {
            RunStatus::Reviewed
        } else {
            RunStatus::AwaitingReview
        };
        if run.status != want {
            let mut r = run;
            r.status = want;
            self.put_run(r).await;
        }
    }

    // -----------------------------------------------------------------------
    // Run lifecycle
    // -----------------------------------------------------------------------

    pub async fn start_run(self: &Arc<Self>, spec: NewRunSpec) -> Result<RunId> {
        let repo = PathBuf::from(shellexpand(&spec.repo_path));
        if !git::is_repo(&repo) {
            return Err(anyhow!("not a git repository: {}", repo.display()));
        }
        let repo = git::repo_root(&repo)?;
        let id = RunId::new();

        let (cwd, branch) = if spec.worktree {
            let branch = format!("mogeung/{}", id.short());
            let dir = git::add_worktree(&repo, &branch, &id.short())
                .context("could not create worktree")?;
            (dir, Some(branch))
        } else {
            (repo.clone(), None)
        };

        let base_sha = git::head_sha(&cwd).ok();
        let now = Utc::now();
        let run = Run {
            id,
            root: id,
            intent: spec.intent.clone(),
            agent: AgentKind::ClaudeCode,
            model: spec.model.clone(),
            permission_mode: spec.permission_mode,
            repo_path: repo.to_string_lossy().to_string(),
            cwd: cwd.to_string_lossy().to_string(),
            worktree: spec.worktree,
            branch,
            base_sha,
            session_id: Some(id.to_string()),
            status: RunStatus::Starting,
            created_at: now,
            ended_at: None,
            last_event_at: now,
            last_activity: None,
            cost_usd: 0.0,
            num_turns: 0,
            tokens_in: 0,
            tokens_out: 0,
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            permission_denials: 0,
            error: None,
            parent: None,
        };
        self.put_run(run.clone()).await;
        self.emit(
            id,
            vec![EventKind::UserPrompt {
                text: spec.intent.clone(),
            }],
        )
        .await;

        let spawn = SpawnSpec {
            cwd: run.cwd.clone(),
            prompt: spec.intent,
            model: spec.model,
            permission_mode: spec.permission_mode,
            session_id: Some(id.to_string()),
            resume: None,
        };
        self.clone().supervise(id, spawn);
        Ok(id)
    }

    /// Continue a finished run's agent session with new instructions.
    ///
    /// This is how a review note becomes work (CONCEPT.md B7). It creates a
    /// child run in the same worktree that resumes the parent's session, so the
    /// agent keeps its context and the new diff stacks on the old one.
    pub async fn follow_up(self: &Arc<Self>, parent_id: RunId, prompt: String) -> Result<RunId> {
        let parent = self
            .get_run(parent_id)
            .await
            .ok_or_else(|| anyhow!("no such run"))?;
        if !parent.status.is_terminal() {
            return Err(anyhow!("run is still active — cancel it first"));
        }
        let resume = parent
            .session_id
            .clone()
            .ok_or_else(|| anyhow!("parent run has no agent session to resume"))?;

        let id = RunId::new();
        let now = Utc::now();
        let run = Run {
            id,
            root: parent.root,
            intent: prompt.clone(),
            // Inherit the parent's base, so a follow-up's Change is the whole
            // session's cumulative diff. Combined with review marks keyed by
            // `root`, that means hunks you already read stay read and only what
            // the follow-up actually rewrote comes back unreviewed.
            base_sha: parent.base_sha.clone(),
            session_id: Some(resume.clone()),
            status: RunStatus::Starting,
            created_at: now,
            ended_at: None,
            last_event_at: now,
            last_activity: None,
            cost_usd: 0.0,
            num_turns: 0,
            tokens_in: 0,
            tokens_out: 0,
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            permission_denials: 0,
            error: None,
            parent: Some(parent_id),
            ..parent.clone()
        };
        self.put_run(run.clone()).await;
        self.emit(id, vec![EventKind::UserPrompt { text: prompt.clone() }])
            .await;

        // The child now owns this session's reviewable diff, so retire the
        // parent from the attention queue. Otherwise one session would ask for
        // review once per turn and the queue would fill with stale entries.
        if parent.status == RunStatus::AwaitingReview {
            let mut p = parent.clone();
            p.status = RunStatus::Reviewed;
            self.put_run(p).await;
        }

        let spawn = SpawnSpec {
            cwd: run.cwd.clone(),
            prompt,
            model: run.model.clone(),
            permission_mode: run.permission_mode,
            session_id: None,
            resume: Some(resume),
        };
        self.clone().supervise(id, spawn);
        Ok(id)
    }

    pub async fn cancel_run(&self, id: RunId) {
        if let Some(tx) = self.cancels.lock().await.remove(&id) {
            let _ = tx.send(());
        }
    }

    pub async fn delete_run(&self, id: RunId, remove_worktree: bool) -> Result<()> {
        self.cancel_run(id).await;
        let run = self.get_run(id).await;
        if remove_worktree {
            if let Some(r) = &run {
                if r.worktree {
                    let _ = git::remove_worktree(Path::new(&r.repo_path), Path::new(&r.cwd));
                }
            }
        }
        self.store.delete_run(id)?;
        self.runs.write().await.remove(&id);
        self.changes.write().await.remove(&id);
        self.broadcast(ServerMsg::RunDeleted { run_id: id });
        self.publish_queue().await;
        Ok(())
    }

    /// Spawn the agent process and stream its output into the run.
    fn supervise(self: Arc<Self>, id: RunId, spec: SpawnSpec) {
        tokio::spawn(async move {
            let (cancel_tx, cancel_rx) = oneshot::channel();
            self.cancels.lock().await.insert(id, cancel_tx);

            let outcome = self.drive(id, spec, cancel_rx).await;

            self.cancels.lock().await.remove(&id);

            let mut runs = self.runs.write().await;
            if let Some(r) = runs.get_mut(&id) {
                r.ended_at = Some(Utc::now());
                match outcome {
                    Ok(Outcome::Completed) => {
                        if r.status == RunStatus::Starting || r.status == RunStatus::Running {
                            r.status = RunStatus::AwaitingReview;
                        }
                    }
                    Ok(Outcome::Cancelled) => {
                        r.status = RunStatus::Cancelled;
                    }
                    Err(e) => {
                        r.status = RunStatus::Failed;
                        r.error = Some(e.to_string());
                    }
                }
                let updated = r.clone();
                let _ = self.store.save_run(&updated);
                drop(runs);
                self.broadcast(ServerMsg::RunUpdated { run: updated });
            } else {
                drop(runs);
            }

            self.recompute_change(id).await;
            // A run that ended with nothing to review is already reviewed.
            self.mark_reviewed_status(id).await;
            self.publish_queue().await;
        });
    }

    async fn drive(
        &self,
        id: RunId,
        spec: SpawnSpec,
        mut cancel: oneshot::Receiver<()>,
    ) -> Result<Outcome> {
        let args = adapter::build_args(&spec);
        let mut child = tokio::process::Command::new("claude")
            .args(&args)
            .current_dir(&spec.cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("could not start `claude` — is Claude Code on PATH?")?;

        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| anyhow!("no stderr"))?;

        // Drain stderr concurrently so a chatty child can never deadlock on a
        // full pipe. Keep only the tail; it is diagnostic, not a transcript.
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut buf: Vec<String> = Vec::new();
            while let Ok(Some(l)) = lines.next_line().await {
                buf.push(l);
                if buf.len() > 50 {
                    buf.remove(0);
                }
            }
            buf.join("\n")
        });

        let mut reader = BufReader::new(stdout).lines();
        let mut saw_result_error = None::<String>;

        loop {
            tokio::select! {
                _ = &mut cancel => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    self.emit(id, vec![adapter::notice(NoticeLevel::Warn, "cancelled by user")]).await;
                    return Ok(Outcome::Cancelled);
                }
                line = reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            if let Some(ing) = adapter::parse_line(&l) {
                                if let Some(f) = &ing.finish {
                                    if f.is_error {
                                        saw_result_error = Some(if f.text.is_empty() {
                                            format!("agent reported failure ({})", f.terminal_reason)
                                        } else {
                                            f.text.clone()
                                        });
                                    }
                                }
                                self.apply_ingest(id, ing).await;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            self.emit(id, vec![adapter::notice(
                                NoticeLevel::Error,
                                format!("error reading agent output: {e}"),
                            )]).await;
                            break;
                        }
                    }
                }
            }
        }

        let status = child.wait().await?;
        let tail = stderr_task.await.unwrap_or_default();

        if let Some(msg) = saw_result_error {
            return Err(anyhow!(msg));
        }
        if !status.success() {
            let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into());
            let detail = if tail.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", tail.trim())
            };
            return Err(anyhow!("claude exited with {code}{detail}"));
        }
        Ok(Outcome::Completed)
    }

    /// Fold one parsed stdout line into the run's state.
    async fn apply_ingest(&self, id: RunId, ing: adapter::Ingest) {
        {
            let mut runs = self.runs.write().await;
            if let Some(r) = runs.get_mut(&id) {
                r.last_event_at = Utc::now();
                if r.status == RunStatus::Starting {
                    r.status = RunStatus::Running;
                }
                if let Some(s) = ing.session_id {
                    r.session_id = Some(s);
                }
                if let Some(c) = ing.cost_usd {
                    // `result` reports the cumulative total for the turn.
                    r.cost_usd = r.cost_usd.max(c);
                }
                if let Some(t) = ing.num_turns {
                    r.num_turns = t;
                }
                if let Some(d) = ing.denials {
                    r.permission_denials = d;
                }
                if let Some(a) = ing.last_activity {
                    r.last_activity = Some(a);
                }
                r.tokens_in = r.tokens_in.max(ing.tokens_in);
                r.tokens_out = r.tokens_out.max(ing.tokens_out);
                let updated = r.clone();
                drop(runs);
                let _ = self.store.save_run(&updated);
                self.broadcast(ServerMsg::RunUpdated { run: updated });
            }
        }
        self.emit(id, ing.events).await;
    }
}

enum Outcome {
    Completed,
    Cancelled,
}

/// Minimal `~` expansion so paths typed into the UI behave as expected.
pub fn shellexpand(p: &str) -> String {
    let p = p.trim();
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}
