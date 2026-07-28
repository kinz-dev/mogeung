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
        // Resolved once for the whole pass rather than per session. Both are
        // empty when tmux is absent, which makes the lookup below a no-op
        // instead of a special case.
        let panes = tmux_panes();
        let parents = if panes.is_empty() {
            HashMap::new()
        } else {
            process_parents()
        };
        for id in ids {
            let Some(mut s) = self.get(&id).await else {
                continue;
            };
            let before = (s.alive, s.live_status, s.tmux_target.clone());
            match live_by_id.get(&id) {
                Some(e) => {
                    s.alive = true;
                    s.pid = Some(e.pid);
                    s.live_status = Some(e.status);
                    s.tmux_target = tmux_target_in(e.pid, &panes, &parents);
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
                    // A dead session has no pane. Leaving a stale target would
                    // offer a terminal tab that attaches to nothing.
                    s.tmux_target = None;
                }
            }
            if before != (s.alive, s.live_status, s.tmux_target.clone()) {
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
            // Filled by the scan's liveness pass, which is where the pid it
            // needs becomes known.
            tmux_target: None,
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

    // -----------------------------------------------------------------------
    // The file explorer (R-B24) — read-only, permanently
    // -----------------------------------------------------------------------

    /// The directory the explorer is scoped to: the repo when the session is in
    /// one, the cwd otherwise.
    async fn session_root(&self, id: &str) -> Result<PathBuf> {
        let session = self
            .get(id)
            .await
            .ok_or_else(|| anyhow::anyhow!("no such session"))?;
        let root = session.repo_root.unwrap_or(session.cwd);
        if root.is_empty() {
            anyhow::bail!("that session has no working directory");
        }
        Ok(PathBuf::from(root))
    }

    /// One directory of the session's worktree, dirs first then files.
    ///
    /// `rel` is relative to the session root; empty means the root itself.
    pub async fn list_dir(&self, id: &str, rel: &str) -> Result<Vec<mogeung_core::wire::DirEntry>> {
        let root = self.session_root(id).await?;
        let dir = resolve_inside(&root, rel)?;
        let mut entries = Vec::new();
        for e in std::fs::read_dir(&dir)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            // The one directory nobody reviews by browsing, and the biggest.
            if name == ".git" {
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push(mogeung_core::wire::DirEntry { name, is_dir });
        }
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        Ok(entries)
    }

    /// One worktree file, capped rather than unbounded — the R-A5 rule applied
    /// to worktrees. Returns the text and whether it was cut short.
    pub async fn read_file(&self, id: &str, rel: &str) -> Result<(String, bool)> {
        /// Past this a "file viewer" is really a memory test for the renderer.
        const CAP: usize = 256 * 1024;
        let root = self.session_root(id).await?;
        let path = resolve_inside(&root, rel)?;
        let bytes = std::fs::read(&path)?;
        // A NUL this early means binary; sending it would render garbage and
        // pretend it is the file.
        if bytes.iter().take(8192).any(|b| *b == 0) {
            anyhow::bail!("{rel} is a binary file");
        }
        let truncated = bytes.len() > CAP;
        let head = &bytes[..bytes.len().min(CAP)];
        Ok((String::from_utf8_lossy(head).into_owned(), truncated))
    }

    /// Every file of the worktree in one flat list, for go-to-file. `R-B25`.
    ///
    /// Runs on the blocking pool: a monorepo walk is real work, and the event
    /// loop must keep serving the other clients while it happens.
    pub async fn list_tree(&self, id: &str) -> Result<(Vec<String>, bool)> {
        let root = self.session_root(id).await?;
        let root = root
            .canonicalize()
            .map_err(|e| anyhow!("cannot open the session's directory: {e}"))?;
        tokio::task::spawn_blocking(move || Ok(walk_tree(&root, TREE_CAP))).await?
    }

    /// Matching lines for a literal query across the worktree. `R-B25`.
    ///
    /// Same blocking-pool rule as [`Self::list_tree`], and more deserved: this
    /// one reads file contents, not just names.
    pub async fn search_content(
        &self,
        id: &str,
        query: &str,
    ) -> Result<(Vec<mogeung_core::wire::ContentMatch>, bool)> {
        let query = query.trim().to_string();
        if query.is_empty() {
            anyhow::bail!("nothing to search for");
        }
        let root = self.session_root(id).await?;
        let root = root
            .canonicalize()
            .map_err(|e| anyhow!("cannot open the session's directory: {e}"))?;
        tokio::task::spawn_blocking(move || Ok(search_tree(&root, &query, MATCH_CAP))).await?
    }

    // -----------------------------------------------------------------------
    // The Git view (R-D10) — read-only, permanently
    // -----------------------------------------------------------------------

    /// The session's repo root, refusing sessions that are not in one — the
    /// pane says "not a git repository" instead of erroring elsewhere.
    async fn git_root(&self, id: &str) -> Result<PathBuf> {
        let root = self.session_root(id).await?;
        if !crate::git::is_repo(&root) {
            anyhow::bail!("that session is not in a git repository");
        }
        Ok(root)
    }

    pub async fn git_log(
        &self,
        id: &str,
        skip: u32,
        limit: u32,
        rev: Option<String>,
    ) -> Result<(Vec<mogeung_core::wire::CommitInfo>, bool)> {
        let root = self.git_root(id).await?;
        let (mut commits, files, done) = tokio::task::spawn_blocking(move || {
            crate::git::log_page(&root, skip, limit, rev.as_deref())
        })
        .await??;
        // Attribution (`R-D11`): a commit made during this session's
        // lifetime that touches files the session edited *probably* came
        // from it. A heuristic, marked as such on the wire — the daemon
        // cannot actually know, and two sessions on one file both match
        // (A8's limit, inherited).
        if let Some(session) = self.get(id).await {
            let started = session.started_at.timestamp();
            let root = session.repo_root.unwrap_or_default();
            let root = root.trim_end_matches('/');
            if !root.is_empty() && !session.touched_files.is_empty() {
                let touched: std::collections::HashSet<&str> =
                    session.touched_files.iter().map(|s| s.as_str()).collect();
                for (c, fs) in commits.iter_mut().zip(&files) {
                    // A minute of slack absorbs clock skew between the
                    // transcript's timestamps and the committer's.
                    c.touches_session = c.epoch + 60 >= started
                        && fs
                            .iter()
                            .any(|rel| touched.contains(format!("{root}/{rel}").as_str()));
                }
            }
        }
        Ok((commits, done))
    }

    pub async fn git_refs(&self, id: &str) -> Result<mogeung_core::wire::RefsInfo> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::refs(&root)).await?
    }

    pub async fn git_stashes(&self, id: &str) -> Result<Vec<mogeung_core::wire::StashInfo>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::stashes(&root)).await?
    }

    pub async fn git_stash_show(
        &self,
        id: &str,
        index: u32,
    ) -> Result<Vec<mogeung_core::change::FileChange>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::stash_show(&root, index)).await?
    }

    pub async fn git_submodules(
        &self,
        id: &str,
    ) -> Result<Vec<mogeung_core::wire::SubmoduleInfo>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::submodules(&root)).await?
    }

    pub async fn git_diff_range(
        &self,
        id: &str,
        from: &str,
        to: &str,
    ) -> Result<Vec<mogeung_core::change::FileChange>> {
        let root = self.git_root(id).await?;
        let (from, to) = (from.to_string(), to.to_string());
        tokio::task::spawn_blocking(move || crate::git::diff_range(&root, &from, &to)).await?
    }

    pub async fn git_file_at_rev(
        &self,
        id: &str,
        sha: &str,
        rel: &str,
    ) -> Result<(String, bool)> {
        let root = self.git_root(id).await?;
        let (sha, rel) = (sha.to_string(), rel.to_string());
        tokio::task::spawn_blocking(move || crate::git::file_at_rev(&root, &sha, &rel)).await?
    }

    pub async fn git_show(
        &self,
        id: &str,
        sha: &str,
    ) -> Result<Vec<mogeung_core::change::FileChange>> {
        let root = self.git_root(id).await?;
        let sha = sha.to_string();
        tokio::task::spawn_blocking(move || crate::git::show_commit(&root, &sha)).await?
    }

    pub async fn git_status(&self, id: &str) -> Result<Vec<mogeung_core::wire::StatusEntry>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::status(&root)).await?
    }

    pub async fn git_diff_file(
        &self,
        id: &str,
        rel: &str,
    ) -> Result<Vec<mogeung_core::change::FileChange>> {
        let root = self.git_root(id).await?;
        // Containment first: paths on this command are worktree identifiers,
        // same guard as the explorer's. (A deleted file cannot canonicalise;
        // its diff still matters, so fall back to a lexical `..` check.)
        if resolve_inside(&root, rel).is_err()
            && (Path::new(rel).is_absolute() || rel.split('/').any(|p| p == ".."))
        {
            anyhow::bail!("{rel} is not a path inside the session");
        }
        let rel = rel.to_string();
        tokio::task::spawn_blocking(move || crate::git::diff_file(&root, &rel)).await?
    }

    pub async fn git_blame(
        &self,
        id: &str,
        rel: &str,
        rev: Option<String>,
    ) -> Result<(Vec<mogeung_core::wire::BlameLine>, bool)> {
        let root = self.git_root(id).await?;
        // A worktree blame names a file that exists; a historical blame
        // names one that may not, so its containment is lexical — the
        // `git_diff_file` wrinkle again.
        if rev.is_none() {
            resolve_inside(&root, rel)?;
        } else if Path::new(rel).is_absolute() || rel.split('/').any(|p| p == "..") {
            anyhow::bail!("{rel} is not a path inside the session");
        }
        let rel = rel.to_string();
        tokio::task::spawn_blocking(move || crate::git::blame(&root, &rel, rev.as_deref()))
            .await?
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

/// Every process's parent, in one call.
///
/// The scan resolves a tmux pane for every live session, and doing that with a
/// `ps` per ancestry step would be a subprocess storm on a machine running the
/// four-plus sessions mogeung is built for. One table, walked in memory.
pub fn process_parents() -> HashMap<u32, u32> {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
    else {
        return HashMap::new();
    };
    parse_process_parents(&String::from_utf8_lossy(&out.stdout))
}

fn parse_process_parents(stdout: &str) -> HashMap<u32, u32> {
    stdout
        .lines()
        .filter_map(|line| {
            let (pid, ppid) = line.trim().split_once(char::is_whitespace)?;
            Some((pid.trim().parse().ok()?, ppid.trim().parse().ok()?))
        })
        .collect()
}

/// Parse `tmux list-panes` output into `(pane_pid, target)` pairs.
///
/// Split out from the command so it can be tested without a tmux server, which
/// a test machine will not have running.
fn parse_tmux_panes(stdout: &str) -> Vec<(u32, String)> {
    stdout
        .lines()
        .filter_map(|line| {
            let (pid, target) = line.trim().split_once(char::is_whitespace)?;
            // A target with no session name is useless for attaching, and a
            // session name may itself contain spaces — so take the rest whole.
            let target = target.trim();
            if target.is_empty() {
                return None;
            }
            Some((pid.trim().parse().ok()?, target.to_string()))
        })
        .collect()
}

/// Every tmux pane, as `(pane_pid, attach target)`.
///
/// Empty when tmux is not installed or no server is running — both ordinary,
/// neither an error.
pub fn tmux_panes() -> Vec<(u32, String)> {
    let Ok(out) = std::process::Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{pane_pid} #{session_name}:#{window_index}.#{pane_index}",
        ])
        .output()
    else {
        return Vec::new();
    };
    // tmux exits non-zero with "no server running" when nothing is attached.
    if !out.status.success() {
        return Vec::new();
    }
    parse_tmux_panes(&String::from_utf8_lossy(&out.stdout))
}

/// The tmux pane running `pid`, as an attach target like `mogeung-app:0.0`.
///
/// tmux reports the pid of the process it spawned in each pane — usually a
/// shell, with `claude` a child of it — so this walks the ancestry the same way
/// [`terminal_app_of`] does rather than expecting a direct hit. When `yolomo`
/// runs `claude` as the pane command with no shell between, the first check
/// matches and the walk never runs.
///
/// `None` means "not under tmux", which is the ordinary case for a session
/// started by hand. It is not an error and must not be reported as one: it is
/// the difference between a session mogeung can host and one it can only point
/// you at.
pub fn tmux_target_in(
    pid: u32,
    panes: &[(u32, String)],
    parents: &HashMap<u32, u32>,
) -> Option<String> {
    if panes.is_empty() {
        return None;
    }
    let mut current = pid;
    // Bounded because a corrupt table could contain a cycle, and a scan that
    // spins is worse than one that misses a pane.
    for _ in 0..24 {
        if let Some((_, target)) = panes.iter().find(|(p, _)| *p == current) {
            return Some(target.clone());
        }
        let parent = *parents.get(&current)?;
        if parent <= 1 || parent == current {
            return None;
        }
        current = parent;
    }
    None
}

/// Convenience for a one-off lookup outside the scan loop.
pub fn tmux_target_of(pid: u32) -> Option<String> {
    tmux_target_in(pid, &tmux_panes(), &process_parents())
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

    /// A session name may contain spaces — `yolomo` derives one from a
    /// directory, and directories do. Splitting on whitespace and taking the
    /// second field would truncate `my project:0.0` to `my`, producing a target
    /// that attaches to the wrong session or to none at all.
    #[test]
    fn a_session_name_containing_spaces_survives_parsing() {
        let panes = parse_tmux_panes("4210 my project:0.0\n");
        assert_eq!(panes, vec![(4210, "my project:0.0".to_string())]);
    }

    #[test]
    fn tmux_pane_lines_parse_and_junk_is_dropped() {
        let panes = parse_tmux_panes(
            "1234 mogeung-app:0.0\n\
             5678 mogeung-api:1.2\n\
             \n\
             notanumber mogeung-x:0.0\n\
             9999\n",
        );
        assert_eq!(
            panes,
            vec![
                (1234, "mogeung-app:0.0".to_string()),
                (5678, "mogeung-api:1.2".to_string()),
            ],
            "a malformed line must be skipped, not panic or poison the rest"
        );
    }

    /// Not being under tmux is the ordinary case, not a failure. pid 1 is never
    /// in a pane, and the walk must terminate rather than spin.
    #[test]
    fn a_process_outside_tmux_has_no_target() {
        assert_eq!(tmux_target_of(1), None);
    }

    /// The real shape is `tmux pane → shell → claude`, and it is *claude's* pid
    /// the live registry hands us — never the pane's. A lookup that only
    /// compared against `pane_pid` would resolve nothing for every real
    /// session while passing every unit test written against a flat pid.
    ///
    /// Costs a real tmux server, so it skips where tmux is absent rather than
    /// failing. `cargo test --workspace` stays free: nothing here spawns an
    /// agent.
    #[test]
    fn a_process_nested_under_a_pane_resolves_to_that_pane() {
        if std::process::Command::new("tmux").arg("-V").output().is_err() {
            eprintln!("skipping: tmux is not installed");
            return;
        }
        let name = format!("mogeung-selftest-{}", std::process::id());
        let target = format!("={name}");

        // Two commands, so the shell cannot exec-optimise itself away — that
        // optimisation is exactly what would collapse the nesting this test
        // exists to cover.
        let created = std::process::Command::new("tmux")
            .args(["new-session", "-d", "-s", &name, "sleep 60; true"])
            .status();
        if !matches!(created, Ok(s) if s.success()) {
            eprintln!("skipping: could not start a tmux server");
            return;
        }

        // The shell needs a moment to fork the child.
        let mut found = None;
        for _ in 0..50 {
            let panes = tmux_panes();
            let Some((pane_pid, _)) = panes.iter().find(|(_, t)| t.starts_with(&name)) else {
                std::thread::sleep(std::time::Duration::from_millis(40));
                continue;
            };
            let kids = std::process::Command::new("pgrep")
                .args(["-P", &pane_pid.to_string()])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();
            if let Some(child) = kids.lines().next().and_then(|l| l.trim().parse::<u32>().ok()) {
                found = Some(tmux_target_in(child, &panes, &process_parents()));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }

        // Tear down before asserting, or a failure leaves a stray server behind.
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", &target])
            .status();

        let resolved = found.expect("the pane never produced a child process");
        let resolved = resolved.expect("a process inside a pane must resolve to that pane");
        assert!(
            resolved.starts_with(&name),
            "resolved to {resolved}, which is not the pane we created ({name})"
        );
    }
}

/// Resolve `rel` against `root` and refuse anything that escapes it.
///
/// The daemon is unauthenticated on localhost, so this guard is what keeps
/// "browse the session's worktree" from being "read any file on the machine by
/// asking politely". Canonicalising *both* sides is the load-bearing part —
/// symlinked roots (`/tmp` on macOS) would otherwise fail the prefix test for
/// honest paths, and a `..` would pass it.
fn resolve_inside(root: &Path, rel: &str) -> Result<PathBuf> {
    if Path::new(rel).is_absolute() {
        anyhow::bail!("{rel} is not a path inside the session");
    }
    let root = root
        .canonicalize()
        .map_err(|e| anyhow!("cannot open the session's directory: {e}"))?;
    let joined = if rel.is_empty() { root.clone() } else { root.join(rel) };
    let full = joined
        .canonicalize()
        .map_err(|e| anyhow!("cannot open {rel}: {e}"))?;
    if !full.starts_with(&root) {
        anyhow::bail!("{rel} is not a path inside the session");
    }
    Ok(full)
}

/// The most files `list_tree` will name. Past this, go-to-file over a prefix
/// of the tree beats an answer too big to send.
const TREE_CAP: usize = 20_000;
/// The most matches one search will return.
const MATCH_CAP: usize = 500;
/// Files bigger than this are skipped by search — worktree source, not blobs.
const SEARCH_FILE_CAP: u64 = 1024 * 1024;
/// A match line is clipped here; a minified bundle is not a search result.
const MATCH_LINE_CAP: usize = 240;

/// The walk both `list_tree` and `search_content` share: hidden files
/// included (the tree pane lists them, so search must see them), `.git`
/// excluded always, gitignore rules applied when the root is a repo — which is
/// the `ignore` crate's default behaviour, not something we reimplement.
fn worktree_walk(root: &Path) -> ignore::Walk {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build()
}

/// `path` as the wire spells it: relative to `root`, `/`-joined.
fn wire_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect();
    Some(parts.join("/"))
}

fn walk_tree(root: &Path, cap: usize) -> (Vec<String>, bool) {
    let mut paths = Vec::new();
    let mut truncated = false;
    for entry in worktree_walk(root) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(rel) = wire_path(root, entry.path()) else { continue };
        if paths.len() >= cap {
            truncated = true;
            break;
        }
        paths.push(rel);
    }
    paths.sort();
    (paths, truncated)
}

/// Clip to `cap` bytes without splitting a character.
fn clip_line(line: &str, cap: usize) -> String {
    if line.len() <= cap {
        return line.to_string();
    }
    let mut end = cap;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &line[..end])
}

/// A literal substring scan, smart-cased. Binary files (NUL in the head) and
/// oversized files are skipped silently — degrade, never refuse, because a
/// search that errors on the one unreadable file in the tree answers nothing.
fn search_tree(
    root: &Path,
    query: &str,
    cap: usize,
) -> (Vec<mogeung_core::wire::ContentMatch>, bool) {
    let case_sensitive = query.chars().any(|c| c.is_uppercase());
    let needle = if case_sensitive { query.to_string() } else { query.to_lowercase() };
    let mut matches = Vec::new();
    let mut truncated = false;
    'walk: for entry in worktree_walk(root) {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if entry.metadata().map(|m| m.len() > SEARCH_FILE_CAP).unwrap_or(true) {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else { continue };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue;
        }
        let Some(rel) = wire_path(root, entry.path()) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            let hit = if case_sensitive {
                line.contains(&needle)
            } else {
                line.to_lowercase().contains(&needle)
            };
            if !hit {
                continue;
            }
            if matches.len() >= cap {
                truncated = true;
                break 'walk;
            }
            matches.push(mogeung_core::wire::ContentMatch {
                path: rel.clone(),
                line: (i + 1) as u64,
                text: clip_line(line, MATCH_LINE_CAP),
            });
        }
    }
    (matches, truncated)
}

#[cfg(test)]
mod explorer_tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        // Keyed by test name as well as pid: tests share a process and would
        // otherwise race on one directory.
        let dir = std::env::temp_dir().join(format!("mogeung-explorer-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn hi() {}\n").unwrap();
        dir
    }

    /// The guard is the security boundary of R-B24: an unauthenticated client
    /// must not be able to read outside the session root by construction.
    #[test]
    fn paths_that_escape_the_root_are_refused() {
        let dir = scratch("escape");
        assert!(resolve_inside(&dir, "src/lib.rs").is_ok());
        assert!(resolve_inside(&dir, "").is_ok(), "empty means the root");
        assert!(resolve_inside(&dir, "../").is_err(), ".. escapes");
        assert!(resolve_inside(&dir, "src/../../etc").is_err(), "buried .. escapes");
        assert!(resolve_inside(&dir, "/etc/passwd").is_err(), "absolute escapes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A symlink pointing out of the tree is followed and then caught — the
    /// canonical target is what gets the prefix test, not the link's name.
    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_tree_is_refused() {
        let dir = scratch("symlink");
        std::os::unix::fs::symlink("/etc", dir.join("escape")).unwrap();
        assert!(resolve_inside(&dir, "escape").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A scratch worktree that is a git repo, so gitignore rules apply — the
    /// `ignore` crate only honours them inside one (`require_git`).
    fn repo_scratch(name: &str) -> PathBuf {
        let dir = scratch(name);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join(".gitignore"), "/target\n").unwrap();
        std::fs::write(dir.join("target/junk.rs"), "generated\n").unwrap();
        std::fs::write(dir.join(".hidden.toml"), "dot = true\n").unwrap();
        dir
    }

    /// The go-to-file list must not drown in build output, must never name
    /// `.git`, and must include honest dotfiles — they are in the tree pane,
    /// so a search that cannot see them would disagree with it.
    #[test]
    fn the_tree_walk_honours_gitignore_and_skips_dot_git() {
        let dir = repo_scratch("walk");
        let (paths, truncated) = walk_tree(&dir, 100);
        assert!(!truncated);
        assert!(paths.contains(&"src/lib.rs".to_string()), "{paths:?}");
        assert!(paths.contains(&".hidden.toml".to_string()), "dotfiles belong: {paths:?}");
        assert!(paths.contains(&".gitignore".to_string()));
        assert!(!paths.iter().any(|p| p.starts_with("target")), "ignored tree listed: {paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with(".git/")), ".git listed: {paths:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The cap is a statement, not a silent cut: `truncated` must say so.
    #[test]
    fn a_capped_walk_says_it_was_cut() {
        let dir = scratch("cap");
        for i in 0..5 {
            std::fs::write(dir.join(format!("f{i}.txt")), "x\n").unwrap();
        }
        let (paths, truncated) = walk_tree(&dir, 3);
        assert_eq!(paths.len(), 3);
        assert!(truncated, "cut the list without admitting it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Search is smart-cased, 1-based, and skips what it cannot honestly
    /// search — binary and ignored files — without erroring the whole query.
    #[test]
    fn search_finds_lines_and_skips_binary_and_ignored() {
        let dir = repo_scratch("search");
        std::fs::write(dir.join("src/lib.rs"), "pub fn hi() {}\n// TODO: later\n").unwrap();
        std::fs::write(dir.join("target/junk.rs"), "TODO in ignored land\n").unwrap();
        std::fs::write(dir.join("blob.bin"), b"TODO\x00binary\n").unwrap();

        let (m, truncated) = search_tree(&dir, "todo", MATCH_CAP);
        assert!(!truncated);
        assert_eq!(m.len(), 1, "expected one honest match: {m:?}");
        assert_eq!(m[0].path, "src/lib.rs");
        assert_eq!(m[0].line, 2, "line numbers are 1-based");
        assert_eq!(m[0].text, "// TODO: later");

        // An uppercase letter in the query flips to case-sensitive.
        let (m, _) = search_tree(&dir, "Todo", MATCH_CAP);
        assert!(m.is_empty(), "smart case failed: {m:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The match cap trips mid-file and still reports honestly.
    #[test]
    fn a_capped_search_says_it_was_cut() {
        let dir = scratch("searchcap");
        std::fs::write(dir.join("many.txt"), "hit\n".repeat(10)).unwrap();
        let (m, truncated) = search_tree(&dir, "hit", 4);
        assert_eq!(m.len(), 4);
        assert!(truncated);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Clipping a long line must not split a multi-byte character.
    #[test]
    fn match_lines_are_clipped_on_char_boundaries() {
        let long = format!("{}é tail", "x".repeat(MATCH_LINE_CAP - 1));
        let clipped = clip_line(&long, MATCH_LINE_CAP);
        assert!(clipped.ends_with('…'));
        assert!(clipped.len() <= MATCH_LINE_CAP + '…'.len_utf8());
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
