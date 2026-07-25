//! Daemon state.
//!
//! mogeung observes; it does not orchestrate. There is no supervisor here and
//! no child processes — the sessions belong to your terminals. The daemon's job
//! is to notice them, rank them, diff them, and remember what you have read.

use crate::adapter;
use crate::git;
use crate::store::Store;
use crate::watcher::{self, Tailer};
use anyhow::{anyhow, Result};
use chrono::Utc;
use mogeung_core::attention::{rank, AttentionConfig};
use mogeung_core::session::{Session, SessionId};
use mogeung_core::{Change, EventKind, ServerMsg, TranscriptEvent};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

/// How far back to look for sessions on startup.
const HISTORY_DAYS: i64 = 14;

pub struct AppState {
    pub store: Store,
    pub sessions: RwLock<HashMap<SessionId, Session>>,
    pub changes: RwLock<HashMap<SessionId, Change>>,
    pub tx: broadcast::Sender<ServerMsg>,
    tailer: Mutex<Tailer>,
    seqs: Mutex<HashMap<SessionId, u64>>,
    /// Root of the Claude Code state directory this daemon watches.
    pub claude_home: PathBuf,
    pub attention: AttentionConfig,
}

impl AppState {
    pub fn new(store: Store) -> Result<Arc<Self>> {
        Self::with_home(store, watcher::default_home())
    }

    pub fn with_home(store: Store, claude_home: PathBuf) -> Result<Arc<Self>> {
        let loaded = store.load_sessions()?;
        let mut sessions = HashMap::new();
        let mut seqs = HashMap::new();
        for mut s in loaded {
            // Liveness is re-derived from the OS on the first scan; never trust
            // a persisted "alive".
            s.alive = false;
            s.live_status = None;
            s.pid = None;
            seqs.insert(s.id.clone(), store.max_seq(&s.id).unwrap_or(0));
            sessions.insert(s.id.clone(), s);
        }
        let (tx, _) = broadcast::channel(8192);
        Ok(Arc::new(AppState {
            store,
            sessions: RwLock::new(sessions),
            changes: RwLock::new(HashMap::new()),
            tx,
            tailer: Mutex::new(Tailer::default()),
            seqs: Mutex::new(seqs),
            claude_home,
            attention: AttentionConfig::default(),
        }))
    }

    pub fn broadcast(&self, msg: ServerMsg) {
        let _ = self.tx.send(msg);
    }

    pub async fn snapshot(&self) -> ServerMsg {
        let sessions: Vec<Session> = self.sessions.read().await.values().cloned().collect();
        let queue = rank(&sessions, Utc::now(), &self.attention);
        ServerMsg::Snapshot { sessions, queue }
    }

    pub async fn publish_queue(&self) {
        let sessions: Vec<Session> = self.sessions.read().await.values().cloned().collect();
        let queue = rank(&sessions, Utc::now(), &self.attention);
        self.broadcast(ServerMsg::Queue { queue });
    }

    pub async fn get(&self, id: &str) -> Option<Session> {
        self.sessions.read().await.get(id).cloned()
    }

    async fn put(&self, s: Session) {
        let _ = self.store.save_session(&s);
        self.sessions.write().await.insert(s.id.clone(), s.clone());
        self.broadcast(ServerMsg::SessionUpdated {
            session: Box::new(s),
        });
    }

    async fn next_seq(&self, id: &str) -> u64 {
        let mut s = self.seqs.lock().await;
        let e = s.entry(id.to_string()).or_insert(0);
        *e += 1;
        *e
    }

    async fn emit(&self, id: &str, kinds: Vec<EventKind>, ts: Option<chrono::DateTime<Utc>>) {
        if kinds.is_empty() {
            return;
        }
        let mut evs = Vec::with_capacity(kinds.len());
        for kind in kinds {
            let ev = TranscriptEvent {
                session_id: id.to_string(),
                seq: self.next_seq(id).await,
                ts: ts.unwrap_or_else(Utc::now),
                kind,
            };
            let _ = self.store.append_event(&ev);
            evs.push(ev);
        }
        self.broadcast(ServerMsg::Events { events: evs });
    }

    // -----------------------------------------------------------------------
    // The scan loop
    // -----------------------------------------------------------------------

    /// One pass: refresh liveness from the registry, then tail every transcript
    /// that has grown.
    pub async fn scan(&self) {
        let live = watcher::scan_live(&self.claude_home);
        let live_by_id: HashMap<String, watcher::LiveEntry> = live
            .into_iter()
            .map(|e| (e.session_id.clone(), e))
            .collect();

        let files = watcher::scan_transcripts(&self.claude_home, HISTORY_DAYS);
        let mut touched_ids: Vec<SessionId> = Vec::new();

        for f in &files {
            let known = self.sessions.read().await.contains_key(&f.session_id);
            if !known {
                // A session we have never seen. Read it in full so history is
                // complete, but only if it is not enormous.
                self.ensure_session(f).await;
            }
            let lines = {
                let mut t = self.tailer.lock().await;
                t.read_new(&f.path).unwrap_or_default()
            };
            if lines.is_empty() && known {
                continue;
            }
            self.apply_lines(&f.session_id, &f.path, lines).await;
            touched_ids.push(f.session_id.clone());
        }

        // Apply liveness to every known session, not just the ones that moved:
        // a session going from busy to idle produces no transcript line, and
        // that transition is the single most important signal we have.
        let ids: Vec<SessionId> = self.sessions.read().await.keys().cloned().collect();
        for id in ids {
            let Some(mut s) = self.get(&id).await else {
                continue;
            };
            let before = (s.alive, s.live_status);
            match live_by_id.get(&id) {
                Some(e) => {
                    s.alive = true;
                    s.pid = Some(e.pid);
                    s.live_status = Some(e.status);
                    s.status_since = e.status_since.or(s.status_since);
                    if s.name.is_none() {
                        s.name = e.name.clone();
                    }
                    if s.version.is_none() {
                        s.version = e.version.clone();
                    }
                    if let Some(st) = e.started_at {
                        s.started_at = st;
                    }
                    if s.cwd.is_empty() {
                        s.cwd = e.cwd.clone();
                    }
                }
                None => {
                    s.alive = false;
                    s.pid = None;
                    s.live_status = None;
                }
            }
            if before != (s.alive, s.live_status) {
                // A session that just exited is newly reviewable.
                let just_exited = before.0 && !s.alive;
                self.put(s).await;
                if just_exited {
                    self.recompute_change(&id).await;
                }
            }
        }

        for id in touched_ids {
            // Keep diffs current for sessions that are actively changing files.
            self.recompute_change(&id).await;
        }
        self.publish_queue().await;
    }

    /// Register a transcript we have not seen before.
    async fn ensure_session(&self, f: &watcher::TranscriptFile) {
        let now = Utc::now();
        let s = Session {
            id: f.session_id.clone(),
            title: None,
            name: None,
            last_prompt: None,
            cwd: String::new(),
            repo_root: None,
            git_branch: None,
            pid: None,
            alive: false,
            live_status: None,
            version: None,
            started_at: f.modified,
            last_event_at: f.modified,
            status_since: None,
            turns: 0,
            tool_calls: 0,
            tokens_in: 0,
            tokens_out: 0,
            last_activity: None,
            touched_files: Vec::new(),
            base_sha: None,
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            error: None,
            transcript_path: f.path.to_string_lossy().to_string(),
            reviewed: false,
        };
        self.sessions.write().await.insert(s.id.clone(), s);
        if now.signed_duration_since(f.modified).num_days() > HISTORY_DAYS {
            let mut t = self.tailer.lock().await;
            t.skip_to_end(&f.path);
        }
    }

    /// Fold newly-read transcript lines into a session.
    async fn apply_lines(&self, id: &str, path: &Path, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        let Some(mut s) = self.get(id).await else {
            return;
        };
        let mut events = Vec::new();
        let mut last_ts = None;

        for line in &lines {
            let Some(p) = adapter::parse_line(line) else {
                continue;
            };
            if let Some(t) = p.ts {
                last_ts = Some(t);
                s.last_event_at = t;
                if s.turns == 0 && s.started_at > t {
                    s.started_at = t;
                }
            }
            if let Some(c) = p.cwd {
                if s.cwd.is_empty() {
                    s.cwd = c;
                }
            }
            if p.git_branch.is_some() {
                s.git_branch = p.git_branch;
            }
            if p.version.is_some() {
                s.version = p.version;
            }
            if p.title.is_some() {
                s.title = p.title;
            }
            if p.last_prompt.is_some() {
                s.last_prompt = p.last_prompt;
            }
            if p.is_turn {
                s.turns += 1;
                // A new prompt clears a previous failure: you responded to it.
                s.error = None;
            }
            if p.error.is_some() {
                s.error = p.error;
            }
            s.tool_calls += p.tool_calls;
            s.tokens_in = s.tokens_in.max(p.tokens_in);
            s.tokens_out += p.tokens_out;
            if p.last_activity.is_some() {
                s.last_activity = p.last_activity;
            }
            for f in p.touched {
                if !s.touched_files.contains(&f) {
                    s.touched_files.push(f);
                }
            }
            events.extend(p.events);
        }

        // Resolve the repo once the cwd is known, and pin a diff base the first
        // time we see the session.
        if s.repo_root.is_none() && !s.cwd.is_empty() {
            let cwd = PathBuf::from(&s.cwd);
            if git::is_repo(&cwd) {
                s.repo_root = git::repo_root(&cwd).ok().map(|p| p.to_string_lossy().to_string());
                if s.base_sha.is_none() {
                    s.base_sha = git::head_sha(&cwd).ok();
                }
            }
        }
        if s.transcript_path.is_empty() {
            s.transcript_path = path.to_string_lossy().to_string();
        }

        self.put(s).await;
        self.emit(id, events, last_ts).await;
    }

    // -----------------------------------------------------------------------
    // Diffs and review
    // -----------------------------------------------------------------------

    pub async fn recompute_change(&self, id: &str) -> Option<Change> {
        let session = self.get(id).await?;
        if session.cwd.is_empty() {
            return None;
        }
        let reviewed = self.store.reviewed_anchors(id).unwrap_or_default();
        let mut change = git::compute_change(
            Path::new(&session.cwd),
            session.base_sha.as_deref(),
            &reviewed,
        );

        // Several sessions can share a working tree, so attribute the diff to
        // the files this session actually touched when we know them.
        if !session.touched_files.is_empty() {
            let root = session.repo_root.clone().unwrap_or_else(|| session.cwd.clone());
            let touched: Vec<String> = session
                .touched_files
                .iter()
                .map(|p| {
                    p.strip_prefix(&format!("{root}/"))
                        .unwrap_or(p)
                        .to_string()
                })
                .collect();
            change
                .files
                .retain(|f| touched.iter().any(|t| t == &f.path || f.path.ends_with(t.as_str())));
            change.insertions = change.files.iter().map(|f| f.insertions).sum();
            change.deletions = change.files.iter().map(|f| f.deletions).sum();
        }

        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.get_mut(id) {
            let files = change.files.len() as u32;
            let all_read = change.unreviewed_hunks() == 0 && change.total_hunks() > 0;
            if s.files_changed != files
                || s.insertions != change.insertions
                || s.deletions != change.deletions
                || s.reviewed != all_read
            {
                s.files_changed = files;
                s.insertions = change.insertions;
                s.deletions = change.deletions;
                s.reviewed = all_read;
                let updated = s.clone();
                let _ = self.store.save_session(&updated);
                drop(sessions);
                self.broadcast(ServerMsg::SessionUpdated {
                    session: Box::new(updated),
                });
            }
        }

        self.changes.write().await.insert(id.to_string(), change.clone());
        self.broadcast(ServerMsg::ChangeUpdated {
            session_id: id.to_string(),
            change: change.clone(),
        });
        Some(change)
    }

    pub async fn set_hunk_reviewed(&self, id: &str, anchor: &str, reviewed: bool) {
        let _ = self.store.set_reviewed(id, anchor, reviewed);
        self.recompute_change(id).await;
        self.publish_queue().await;
    }

    pub async fn review_all(&self, id: &str) {
        if let Some(change) = self.recompute_change(id).await {
            for f in &change.files {
                for h in &f.hunks {
                    let _ = self.store.set_reviewed(id, &h.anchor, true);
                }
            }
        }
        self.recompute_change(id).await;
        self.publish_queue().await;
    }

    pub async fn forget(&self, id: &str) -> Result<()> {
        if let Some(s) = self.get(id).await {
            let mut t = self.tailer.lock().await;
            t.forget(Path::new(&s.transcript_path));
        }
        self.store.delete_session(id)?;
        self.sessions.write().await.remove(id);
        self.changes.write().await.remove(id);
        self.broadcast(ServerMsg::SessionRemoved {
            session_id: id.to_string(),
        });
        self.publish_queue().await;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Launching a real session
    // -----------------------------------------------------------------------

    /// Open a terminal running interactive `claude` in `dir`.
    ///
    /// This is the one thing mogeung starts, and note what it starts: the real
    /// CLI, in your terminal, with nothing wrapped. It exists because the other
    /// half of v0.1's failure was that reaching three or four parallel sessions
    /// was awkward.
    pub async fn launch_terminal(&self, dir: &str, worktree: bool) -> Result<()> {
        let dir = PathBuf::from(shellexpand(dir));
        if !dir.exists() {
            return Err(anyhow!("path does not exist: {}", dir.display()));
        }

        let target = if worktree {
            if !git::is_repo(&dir) {
                return Err(anyhow!("not a git repository: {}", dir.display()));
            }
            let repo = git::repo_root(&dir)?;
            let stamp = Utc::now().format("%m%d-%H%M%S").to_string();
            let branch = format!("mogeung/{stamp}");
            git::add_worktree(&repo, &branch, &stamp)?
        } else {
            dir
        };

        // `open -a Terminal` cannot carry a command, so drive Terminal.app
        // directly. Failing that, fall back to just opening the directory.
        let script = format!(
            "tell application \"Terminal\"\n activate\n do script \"cd {} && claude\"\nend tell",
            shell_quote(&target.to_string_lossy())
        );
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            _ => {
                std::process::Command::new("open")
                    .arg(target.as_os_str())
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| anyhow!("could not open a terminal: {e}"))
            }
        }
    }
}

fn shell_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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
