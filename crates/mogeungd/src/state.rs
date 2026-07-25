//! Daemon state.
//!
//! mogeung observes; it does not orchestrate. There is no supervisor here and
//! no child processes — the sessions belong to your terminals. The daemon's job
//! is to notice them, rank them, diff them, and remember what you have read.

use crate::adapter::{self, LineOutcome};
use crate::git;
use crate::health::HealthTracker;
use crate::notify::{NotifyConfig, Notifier};
use crate::store::Store;
use crate::watcher::{self, Tailer};
use anyhow::{anyhow, Result};
use chrono::Utc;
use mogeung_core::attention::{rank, AttentionConfig, AttentionItem};
use mogeung_core::health::Health;
use mogeung_core::review::{BlastRadius, DebtFile, ReviewDebt};
use mogeung_core::session::{Collision, OpenTool, Session, SessionId, Touch};
use mogeung_core::{Change, EventKind, ServerMsg, TranscriptEvent};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

/// How far back to look for sessions on startup.
const HISTORY_DAYS: i64 = 14;

/// Largest transcript we will read from the beginning on first sight.
///
/// Above this we follow the tail instead and say so — see roadmap `R-A5`. The
/// corpus this was sized against has a 11.2 MB transcript; reading one whole
/// parses tens of thousands of lines and emits every event, synchronously,
/// inside the scan loop, to reconstruct history nobody is going to review.
///
/// The number is a judgement, not a measurement: big enough that a normal
/// working session is never truncated, small enough that a pathological one
/// cannot stall the board.
const MAX_TRANSCRIPT_BYTES: u64 = 4 << 20; // 4 MiB

/// How much of an oversized transcript to keep. Enough to show what the session
/// was recently doing without paying for its whole history.
const TAIL_BYTES: u64 = 1 << 20; // 1 MiB

/// Two live sessions touching the same file within this window are colliding.
///
/// Long enough to catch a real overlap, short enough that "we both edited this
/// file at some point today" does not count. `R-B3`.
const COLLISION_WINDOW_SECS: i64 = 600;

/// How many recent tool calls to remember per session, and how many repeats of
/// the same `tool:target` within them counts as thrashing. `R-B7`.
const LOOP_HISTORY: usize = 12;
const LOOP_REPEATS: usize = 4;

/// Cap on remembered touches, so a long session cannot grow without bound.
const MAX_RECENT_TOUCHES: usize = 200;

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
    /// What we have and have not managed to read. See `health.rs`.
    health: Mutex<HealthTracker>,
    /// Tells you a session needs attention when the window is not in front.
    notifier: Mutex<Notifier>,
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
            health: Mutex::new(HealthTracker::new(MAX_TRANSCRIPT_BYTES)),
            notifier: Mutex::new(Notifier::default()),
        }))
    }

    /// Turn desktop/push notifications on. Off unless asked for: a tool that
    /// starts posting banners the first time you run it has overstepped.
    pub async fn configure_notifications(&self, cfg: NotifyConfig) {
        *self.notifier.lock().await = Notifier::new(cfg);
    }

    /// What mogeung can currently see, and what it cannot.
    pub async fn health(&self) -> Health {
        self.health.lock().await.snapshot()
    }

    async fn publish_health(&self) {
        let health = self.health().await;
        self.broadcast(ServerMsg::Health {
            health: Box::new(health),
        });
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
        self.notify_for(&queue, &sessions).await;
        self.broadcast(ServerMsg::Queue { queue });
    }

    /// Announce anything that has newly started needing you.
    ///
    /// Delivery is spawned rather than awaited: `osascript` and `curl` are
    /// external processes, and the scan loop must not be held up by either.
    async fn notify_for(&self, queue: &[AttentionItem], sessions: &[Session]) {
        let mut notifier = self.notifier.lock().await;
        if !notifier.config().enabled() {
            return;
        }
        let labels: HashMap<&str, String> =
            sessions.iter().map(|s| (s.id.as_str(), s.label())).collect();
        let pending = notifier.diff(queue, |id| {
            labels.get(id).cloned().unwrap_or_else(|| id.to_string())
        });
        for n in pending {
            let cfg = notifier.config().clone();
            tokio::task::spawn_blocking(move || Notifier::new(cfg).send(&n));
        }
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

        self.refresh_collisions().await;

        {
            let sessions = self.sessions.read().await;
            let live_count = sessions.values().filter(|s| s.alive).count() as u64;
            self.health
                .lock()
                .await
                .finish_scan(sessions.len() as u64, live_count, files.len() as u64);
        }
        self.publish_queue().await;
        self.publish_health().await;
    }

    /// Recompute cross-session collisions and publish any that changed.
    ///
    /// Runs every scan rather than only when files move, because a collision
    /// also *ends* — when one side exits or the window lapses — and a stale
    /// warning is worse than none.
    async fn refresh_collisions(&self) {
        let now = Utc::now();
        let all: Vec<Session> = self.sessions.read().await.values().cloned().collect();
        let found = detect_collisions(&all, now);

        let mut changed = Vec::new();
        {
            let mut sessions = self.sessions.write().await;
            for s in all {
                let next = found.get(&s.id).cloned().unwrap_or_default();
                if next != s.collisions {
                    if let Some(live) = sessions.get_mut(&s.id) {
                        live.collisions = next;
                        changed.push(live.clone());
                    }
                }
            }
        }
        for s in changed {
            let _ = self.store.save_session(&s);
            self.broadcast(ServerMsg::SessionUpdated {
                session: Box::new(s),
            });
        }
    }

    /// Silence a session for `minutes`. `R-B5`.
    pub async fn snooze(&self, id: &str, minutes: i64) {
        let Some(mut s) = self.get(id).await else {
            return;
        };
        s.snoozed_until = if minutes <= 0 {
            None
        } else {
            Some(Utc::now() + chrono::Duration::minutes(minutes))
        };
        self.put(s).await;
        self.publish_queue().await;
    }

    /// Register a transcript we have not seen before.
    async fn ensure_session(&self, f: &watcher::TranscriptFile) {
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
            open_tools: Vec::new(),
            snoozed_until: None,
            collisions: Vec::new(),
            loop_signal: None,
            recent_touches: Vec::new(),
            recent_tools: Vec::new(),
        };
        self.sessions.write().await.insert(s.id.clone(), s);

        // Cap the first read. The previous guard here compared the file's age
        // against HISTORY_DAYS, which `scan_transcripts` has already filtered
        // on — so it could never fire, and the comment promising "only if it is
        // not enormous" described a check that did not exist.
        //
        // Skipping history is a real loss, so it is recorded and surfaced
        // rather than done quietly.
        if f.size > MAX_TRANSCRIPT_BYTES {
            let started_at = {
                let mut t = self.tailer.lock().await;
                t.start_near_end(&f.path, TAIL_BYTES)
            };
            if started_at > 0 {
                tracing::warn!(
                    "transcript {} is {} — following its tail, skipping {} of history",
                    f.session_id,
                    mogeung_core::health::human_bytes(f.size),
                    mogeung_core::health::human_bytes(started_at),
                );
                self.health
                    .lock()
                    .await
                    .record_skipped(&f.session_id, started_at);
            }
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
            let outcome = adapter::parse_line(line);
            {
                // Every line is accounted for, including the ones we throw
                // away. A line we cannot classify is the only early warning we
                // get that a format moved.
                let mut h = self.health.lock().await;
                h.record_line(outcome.class());
                if let LineOutcome::Unknown { event_type } = &outcome {
                    h.record_unknown(event_type);
                }
            }
            let Some(p) = outcome.parsed() else {
                continue;
            };
            if let Some(v) = &p.version {
                // Pass the line's own timestamp: sessions are scanned
                // newest-file-first and each reports the release it ran under,
                // so read order says nothing about what is current.
                self.health.lock().await.record_version(v, p.ts);
            }
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
            let at = p.ts.unwrap_or_else(Utc::now);
            for f in p.touched {
                if !s.touched_files.contains(&f) {
                    s.touched_files.push(f.clone());
                }
                // Timestamped separately: collision detection needs to know
                // *when*, and `touched_files` is cumulative for the session.
                s.recent_touches.push(Touch { path: f, at });
            }
            if s.recent_touches.len() > MAX_RECENT_TOUCHES {
                let excess = s.recent_touches.len() - MAX_RECENT_TOUCHES;
                s.recent_touches.drain(..excess);
            }

            // Open tool calls: a `tool_use` with no matching `tool_result` is
            // what distinguishes "blocked on a permission prompt" from
            // "finished and waiting for you". A new human turn clears them —
            // you cannot be blocked on a prompt you have already answered.
            if p.is_turn {
                s.open_tools.clear();
            }
            for ev in &p.events {
                match ev {
                    EventKind::ToolUse {
                        tool_use_id,
                        name,
                        summary,
                    } if !p.sidechain => {
                        s.open_tools.push(OpenTool {
                            id: tool_use_id.clone(),
                            name: name.clone(),
                            summary: summary.clone(),
                            at,
                        });
                        s.recent_tools.push(format!("{name}\u{1}{summary}"));
                        if s.recent_tools.len() > LOOP_HISTORY {
                            let excess = s.recent_tools.len() - LOOP_HISTORY;
                            s.recent_tools.drain(..excess);
                        }
                    }
                    EventKind::ToolResult { tool_use_id, .. } => {
                        s.open_tools.retain(|t| &t.id != tool_use_id);
                    }
                    _ => {}
                }
            }
            events.extend(p.events);
        }

        s.loop_signal = detect_loop(&s.recent_tools);

        // Resolve the repo once the cwd is known, and pin a diff base the first
        // time we see the session.
        if s.repo_root.is_none() && !s.cwd.is_empty() {
            let cwd = PathBuf::from(&s.cwd);
            if git::is_repo(&cwd) {
                s.repo_root = git::repo_root(&cwd).ok().map(|p| p.to_string_lossy().to_string());
                if s.base_sha.is_none() {
                    // Not HEAD: the last commit predating the session, so work
                    // it committed before mogeung noticed it is still visible.
                    s.base_sha = git::base_for_session(&cwd, s.started_at).ok();
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

    // -----------------------------------------------------------------------
    // Review debt and blast radius
    // -----------------------------------------------------------------------

    /// How much of what agents produced in one repo nobody has read. `R-D8`.
    ///
    /// Built from the diffs already computed rather than by re-walking git, so
    /// it costs nothing and always agrees with what the review tab shows. The
    /// limitation that follows: it covers sessions mogeung knows about, not the
    /// entire history of the repository.
    pub async fn review_debt(&self, repo: &str) -> ReviewDebt {
        let sessions = self.sessions.read().await;
        let changes = self.changes.read().await;

        let mut debt = ReviewDebt {
            repo: repo.to_string(),
            ..Default::default()
        };
        let mut files = std::collections::HashSet::new();

        for s in sessions.values() {
            let in_repo = s.repo_root.as_deref() == Some(repo) || s.cwd == repo;
            if !in_repo {
                continue;
            }
            let Some(change) = changes.get(&s.id) else {
                continue;
            };
            if change.files.is_empty() {
                continue;
            }
            debt.sessions += 1;
            let mut session_unread = 0u32;

            for f in &change.files {
                files.insert(f.path.clone());
                let unread = f.hunks.iter().filter(|h| !h.reviewed).count() as u32;
                debt.hunks_total += f.hunks.len() as u32;
                debt.hunks_read += (f.hunks.len() as u32).saturating_sub(unread);
                session_unread += unread;
                debt.unread_insertions += f
                    .hunks
                    .iter()
                    .filter(|h| !h.reviewed)
                    .map(|h| h.insertions)
                    .sum::<u32>();

                if unread > 0 {
                    debt.worst_files.push(DebtFile {
                        path: f.path.clone(),
                        session_id: s.id.clone(),
                        unread_hunks: unread,
                        score: f.score,
                    });
                }
            }
            if session_unread > 0 {
                debt.sessions_unread += 1;
            }
        }

        debt.files_touched = files.len() as u32;
        // Riskiest first — the same ordering the diff view uses, so "worst"
        // means the same thing in both places.
        debt.worst_files
            .sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
        debt.worst_files.truncate(25);
        debt
    }

    /// Every repo we have seen a session in, for the debt view.
    pub async fn known_repos(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        let mut repos: Vec<String> = sessions
            .values()
            .filter_map(|s| s.repo_root.clone())
            .collect();
        repos.sort();
        repos.dedup();
        repos
    }

    /// What else mentions the symbols this file's diff changed. `R-D9`.
    ///
    /// Textual search, deliberately: it points at things worth a look and makes
    /// no claim to be a call graph.
    pub async fn blast_radius(&self, id: &str, path: &str) -> Option<BlastRadius> {
        let session = self.get(id).await?;
        let repo = session
            .repo_root
            .clone()
            .unwrap_or_else(|| session.cwd.clone());
        if repo.is_empty() {
            return None;
        }

        let change = self.changes.read().await.get(id).cloned()?;
        let file = change.files.iter().find(|f| f.path == path)?;
        let lines: Vec<String> = file.hunks.iter().flat_map(|h| h.lines.clone()).collect();
        let symbols = git::symbols_in(&lines);

        let (references, truncated) = if symbols.is_empty() {
            (Vec::new(), false)
        } else {
            git::find_references(Path::new(&repo), &symbols, path)
        };

        Some(BlastRadius {
            session_id: id.to_string(),
            path: path.to_string(),
            symbols,
            references,
            truncated,
        })
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

    /// Bring the terminal a live session is running in to the front. `R-B2`.
    ///
    /// This closes the loop that `WAITING` opens: the queue tells you which
    /// session needs you, and this puts you in front of it. It moves *your*
    /// window and types nothing — the agent is untouched.
    ///
    /// Resolves the session's pid to its controlling tty, works out which
    /// terminal application owns that process by walking its ancestry, and asks
    /// that application to focus the matching tab.
    pub async fn focus_terminal(&self, id: &str) -> Result<()> {
        let session = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("no such session"))?;
        let pid = session
            .pid
            .filter(|_| session.alive)
            .ok_or_else(|| anyhow!("session is not running, so it has no terminal"))?;

        let tty = tty_of(pid)
            .ok_or_else(|| anyhow!("pid {pid} has no controlling terminal"))?;
        // `ps` prints "ttys004"; every terminal reports "/dev/ttys004".
        let dev = format!("/dev/{}", tty.trim_start_matches("/dev/"));

        // Ask the process tree rather than guessing. Trying every terminal in
        // turn would work, but it raises applications that do not own the tab.
        let detected = terminal_app_of(pid);
        let candidates: Vec<TerminalApp> = match detected {
            Some(app) => vec![app],
            // Unknown ancestry (a multiplexer, a wrapper, an app we do not
            // know). Fall back to asking the ones we can drive; the scripts
            // only activate on a match, so a miss is silent.
            None => TerminalApp::ALL.to_vec(),
        };

        for app in &candidates {
            if app.focus(&dev)? {
                return Ok(());
            }
        }

        Err(match detected {
            Some(app) => anyhow!(
                "{} is running this session on {dev}, but has no tab reporting that tty — \
                 if it is inside tmux or screen, mogeung cannot see the individual pane",
                app.name()
            ),
            None => anyhow!(
                "could not work out which terminal owns {dev}. Supported: {}",
                TerminalApp::ALL
                    .iter()
                    .map(|a| a.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })
    }

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

/// Is this session repeating itself rather than making progress?
///
/// Deliberately crude: the same tool against the same target, several times,
/// inside a short window. It catches the common real failure — an agent
/// retrying an edit that keeps not applying, or re-reading a file it has
/// already read — without pretending to understand intent.
///
/// It cannot distinguish "stuck" from "legitimately doing the same thing to
/// many similar inputs", which is why it produces an advisory string rather
/// than a queue tier of its own. `R-B7`.
fn detect_loop(recent: &[String]) -> Option<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for k in recent {
        *counts.entry(k.as_str()).or_insert(0) += 1;
    }
    let (key, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    if n < LOOP_REPEATS {
        return None;
    }
    let (tool, target) = key.split_once('\u{1}').unwrap_or((key, ""));
    let what = if target.is_empty() {
        tool.to_string()
    } else {
        format!("{tool}: {target}")
    };
    Some(format!("repeated {n}× in the last {LOOP_HISTORY} calls — {what}"))
}

/// Which live sessions are editing the same file at the same time.
///
/// Only the observer can see this — it needs a view across sessions that no
/// individual agent has. Both sides get the warning, because either one might
/// be the one you want to stop. `R-B3`.
fn detect_collisions(sessions: &[Session], now: chrono::DateTime<Utc>) -> HashMap<SessionId, Vec<Collision>> {
    let mut out: HashMap<SessionId, Vec<Collision>> = HashMap::new();
    let live: Vec<&Session> = sessions.iter().filter(|s| s.alive).collect();

    for (i, a) in live.iter().enumerate() {
        for b in live.iter().skip(i + 1) {
            let a_files = a.files_touched_since(now, COLLISION_WINDOW_SECS);
            if a_files.is_empty() {
                continue;
            }
            let b_files = b.files_touched_since(now, COLLISION_WINDOW_SECS);
            for path in a_files.iter().filter(|p| b_files.contains(p)) {
                out.entry(a.id.clone()).or_default().push(Collision {
                    other: b.id.clone(),
                    other_label: b.label(),
                    path: path.to_string(),
                });
                out.entry(b.id.clone()).or_default().push(Collision {
                    other: a.id.clone(),
                    other_label: a.label(),
                    path: path.to_string(),
                });
            }
        }
    }
    out
}

/// A terminal application mogeung knows how to drive. `R-B2`.
///
/// Each exposes its panes' tty over AppleScript, which is the only reliable way
/// to map a process back to the tab a human is looking at. Terminals without
/// scripting support (Alacritty, Ghostty, kitty) cannot be handled at all; the
/// error says so rather than focusing the wrong window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalApp {
    AppleTerminal,
    ITerm2,
}

impl TerminalApp {
    const ALL: [TerminalApp; 2] = [TerminalApp::ITerm2, TerminalApp::AppleTerminal];

    fn name(&self) -> &'static str {
        match self {
            TerminalApp::AppleTerminal => "Terminal.app",
            TerminalApp::ITerm2 => "iTerm2",
        }
    }

    /// Addressed by bundle id rather than name: iTerm2 has answered to both
    /// "iTerm" and "iTerm2" across versions, and the bundle id has not moved.
    fn script(&self, dev: &str) -> String {
        match self {
            // Terminal.app puts the tty on the tab.
            TerminalApp::AppleTerminal => format!(
                r#"tell application id "com.apple.Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      if tty of t is "{dev}" then
        set frontmost of w to true
        set selected of t to true
        activate
        return "ok"
      end if
    end repeat
  end repeat
end tell
return "no""#
            ),
            // iTerm2 has a third level: a tab holds split-pane sessions, and
            // the tty lives on the session.
            TerminalApp::ITerm2 => format!(
                r#"tell application id "com.googlecode.iterm2"
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        if tty of s is "{dev}" then
          select w
          select t
          select s
          activate
          return "ok"
        end if
      end repeat
    end repeat
  end repeat
end tell
return "no""#
            ),
        }
    }

    /// Try to focus the tab owning `dev`. `Ok(false)` means "not mine" — the
    /// script activates the application only on a match, so asking the wrong
    /// one costs nothing visible.
    fn focus(&self, dev: &str) -> Result<bool> {
        let out = std::process::Command::new("osascript")
            .arg("-e")
            .arg(self.script(dev))
            .output()
            .map_err(|e| anyhow!("osascript failed: {e}"))?;
        // A terminal that is not installed or not running errors; that is a
        // "no", not a failure of the whole operation.
        Ok(String::from_utf8_lossy(&out.stdout).trim() == "ok")
    }
}

/// Which terminal application a process is running inside, by walking its
/// ancestry until something recognisable turns up.
///
/// `claude` → `zsh` → `login` → `iTermServer` → `iTerm2` is the real shape on
/// this machine, so a couple of levels is not enough.
fn terminal_app_of(pid: u32) -> Option<TerminalApp> {
    let mut current = pid;
    for _ in 0..12 {
        let out = std::process::Command::new("ps")
            .args(["-o", "ppid=,command=", "-p", &current.to_string()])
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&out.stdout);
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let (ppid, command) = line.split_once(char::is_whitespace)?;

        if command.contains("iTerm") {
            return Some(TerminalApp::ITerm2);
        }
        if command.contains("Terminal.app") || command.ends_with("/Terminal") {
            return Some(TerminalApp::AppleTerminal);
        }

        let parent: u32 = ppid.trim().parse().ok()?;
        if parent <= 1 {
            return None;
        }
        current = parent;
    }
    None
}

/// The controlling terminal of a process, e.g. `ttys004`.
///
/// A session's own pid is the `claude` process, whose tty is the tab it runs
/// in. Returns `None` for a daemon or anything without a controlling terminal,
/// where `ps` prints `??`.
fn tty_of(pid: u32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let tty = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if tty.is_empty() || tty == "??" || tty == "?" {
        return None;
    }
    Some(tty)
}

fn shell_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod terminal_tests {
    use super::*;

    /// Regression: the first version only spoke Terminal.app's dialect and
    /// reported "no tab is attached to /dev/ttys003" to an iTerm2 user whose
    /// tab was sitting right there. Both dialects must be present, and both
    /// must be addressed by bundle id — iTerm2 has answered to "iTerm" and
    /// "iTerm2" at different versions.
    #[test]
    fn both_terminals_are_addressed_by_bundle_id() {
        let apple = TerminalApp::AppleTerminal.script("/dev/ttys003");
        assert!(apple.contains("com.apple.Terminal"));
        assert!(apple.contains("/dev/ttys003"));

        let iterm = TerminalApp::ITerm2.script("/dev/ttys003");
        assert!(iterm.contains("com.googlecode.iterm2"));
        assert!(iterm.contains("/dev/ttys003"));
        // iTerm2 nests sessions inside tabs; looking only at tabs finds nothing.
        assert!(
            iterm.contains("sessions of t"),
            "iTerm2 keeps the tty on the session, not the tab"
        );
    }

    /// Activation must come *after* the match, or asking a terminal that does
    /// not own the tab raises it anyway — which, with the fallback that tries
    /// every terminal, would shuffle the user's windows on every miss.
    #[test]
    fn a_miss_does_not_raise_the_application() {
        for app in TerminalApp::ALL {
            let s = app.script("/dev/ttys999");
            let activate = s.find("activate").expect("script should activate on match");
            let matched = s.find("if tty").expect("script should test the tty");
            assert!(
                activate > matched,
                "{} activates before checking the tty",
                app.name()
            );
        }
    }

    #[test]
    fn launchd_has_no_terminal_and_the_walk_terminates() {
        // pid 1 is launchd: no terminal ancestry, and its parent is itself, so
        // a walk without a stop condition would spin forever.
        assert_eq!(terminal_app_of(1), None);
        assert_eq!(tty_of(1), None);
    }

    #[test]
    fn a_nonexistent_pid_is_handled() {
        assert_eq!(terminal_app_of(999_999), None);
        assert_eq!(tty_of(999_999), None);
    }
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
