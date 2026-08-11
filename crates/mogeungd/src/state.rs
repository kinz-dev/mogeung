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
use crate::usage::UsageScanner;
use crate::watcher::{self, Tailer};
use anyhow::{anyhow, Result};
use chrono::Utc;
use mogeung_core::attention::{rank, AttentionConfig, AttentionItem};
use mogeung_core::health::Health;
use mogeung_core::review::{BlastRadius, DebtFile, ReviewDebt};
use mogeung_core::session::{Collision, OpenTool, Session, SessionId, Touch};
use mogeung_core::verify::{Claim, VerifyRun};
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
    /// Who this daemon is, resolved once at start-up because it cannot change
    /// while the process lives and answering costs a file read (`R-I5`).
    pub identity: mogeung_core::wire::DaemonIdentity,
    /// Where to `ssh` to reach this machine, when configured. Held apart from
    /// `identity` because the constructor is shared with tests that have no
    /// config to read; `server::prepare` fills it in once.
    pub ssh_target: std::sync::OnceLock<String>,
    /// Whether this daemon may write to a repository at all. `R-D19`.
    ///
    /// A `OnceLock` for the same reason `ssh_target` is one: only the code
    /// that *binds the socket* knows the answer, and the constructor is shared
    /// with tests and with the window's own hosted daemon. `server::run` sets
    /// it from [`crate::server::admit`], so the guard and the start-up refusal
    /// cannot disagree about what "safe to write" means.
    ///
    /// Unset reads as **allowed**, which is the loopback posture every
    /// unconfigured caller actually has — a state built by hand in a test, or
    /// by a window hosting a daemon on `127.0.0.1`. That default is only
    /// defensible because it is not the real fence: `admit` refuses a
    /// non-loopback bind with no token *before the database opens*, so a
    /// daemon that could reach this field with the wrong answer never starts.
    /// This is the second lock on a door that is already bolted, and it exists
    /// because the write verbs are the first thing here that cannot be undone.
    pub writes_allowed: std::sync::OnceLock<bool>,
    /// Root of the Codex CLI state directory, watched the same read-only way
    /// when it exists. `R-I1`.
    pub codex_home: PathBuf,
    pub attention: AttentionConfig,
    /// What we have and have not managed to read. See `health.rs`.
    health: Mutex<HealthTracker>,
    /// Tells you a session needs attention when the window is not in front.
    notifier: Mutex<Notifier>,
    /// Incremental token-burn scanner. Pillar G; reused by pillar F analytics.
    usage: Mutex<UsageScanner>,
    /// Repos with a signal run in flight — one at a time per repo. `R-E2`.
    signal_running: Mutex<std::collections::HashSet<String>>,
    /// The queue exactly as last broadcast. The scan loop publishes every
    /// tick; without this, every tick is a message every client re-applies,
    /// changed or not.
    last_queue: Mutex<Vec<AttentionItem>>,
    /// Held for the length of one scan pass, so concurrent requests to scan
    /// collapse into the pass already running rather than stacking.
    scan_running: Mutex<()>,
    /// Incremental Codex rollout state. `Arc` + std mutex because the scan
    /// closure that advances it runs on the blocking pool.
    codex_cache: Arc<std::sync::Mutex<crate::codex::ScanCache>>,
    /// Cwds that turned out not to be git repos, with a countdown until the
    /// next probe. Probing forks `git rev-parse`; without this a session
    /// whose cwd is not a repo pays that fork every tick, forever.
    non_repo_cwds: Mutex<HashMap<String, u32>>,
}

/// How many negative repo probes to skip before asking again — `git init`
/// mid-session is possible, just not worth a fork per tick to notice sooner.
const REPO_RECHECK_TICKS: u32 = 40;


/// A touched file, relative to the repository root — **tolerating two
/// spellings of one place**. `R-J27`.
///
/// The two sides of this comparison come from different places and do not have
/// to agree textually. `repo_root` is `git rev-parse --show-toplevel`, which
/// answers with the path **resolved** through every symlink; `touched_files`
/// carry whatever the transcript wrote, which is the agent's own `cwd` prefix,
/// unresolved. On macOS those differ for every temp directory — `/var` is a
/// firmlink to `/private/var` — and for any checkout reached through a symlink,
/// which is an ordinary way to keep a repo on an external volume.
///
/// When they disagree the textual strip leaves an absolute path, nothing
/// matches the relative paths a diff carries, `retain` empties the file list,
/// and the session reports **no changes at all**. Silent, and it looks exactly
/// like a session that has not touched the worktree.
///
/// Resolution is on the **miss path only**: the fast case is a string compare,
/// and only a genuine mismatch pays a `canonicalize`.
///
/// The values themselves are deliberately left as they were found. `cwd` is
/// shown to the user (`R-J16`) and reached for over tmux, and `repo_root` keys
/// the review-debt rollup; rewriting either to satisfy this comparison would
/// trade a contained fix for a change every reader of those fields has to know
/// about.
fn relative_to_root(root: &str, path: &str) -> String {
    if let Some(rest) = path.strip_prefix(&format!("{root}/")) {
        return rest.to_string();
    }
    let resolved = resolve(Path::new(path));
    if let Ok(rest) = resolved.strip_prefix(resolve(Path::new(root))) {
        return rest.to_string_lossy().into_owned();
    }
    path.to_string()
}

/// A path as the filesystem spells it, resolving what exists and keeping the
/// rest.
///
/// `std::fs::canonicalize` fails outright on a path whose last component is
/// gone, and a **deleted** file is exactly the case a diff cares about — so
/// this canonicalises the longest existing ancestor and puts the tail back.
/// Falls back to the path unchanged, which is what makes it safe to call on
/// anything.
fn resolve(path: &Path) -> PathBuf {
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = path;
    loop {
        if let Ok(real) = std::fs::canonicalize(cur) {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (cur.parent(), cur.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_os_string());
                cur = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// A session with nothing observed about it yet.
///
/// Three discovery paths reach this shape — a transcript, the live registry,
/// a Codex rollout — and `Session` has forty fields. Written out three times,
/// a field added to the struct is a field two of them forget; written once,
/// the compiler asks each caller only about what it actually knows.
fn blank_session(
    id: SessionId,
    source: mogeung_core::session::SessionSource,
    started_at: chrono::DateTime<Utc>,
    last_event_at: chrono::DateTime<Utc>,
) -> Session {
    Session {
        id,
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
        started_at,
        last_event_at,
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
        transcript_path: String::new(),
        reviewed: false,
        open_tools: Vec::new(),
        snoozed_until: None,
        collisions: Vec::new(),
        loop_signal: None,
        recent_touches: Vec::new(),
        recent_tools: Vec::new(),
        // Filled by the scan's liveness pass, which is where the pid it needs
        // becomes known.
        tmux_target: None,
        limit_hit_at: None,
        limit_resets: None,
        verify_runs: Vec::new(),
        claims: Vec::new(),
        source,
    }
}

impl AppState {
    pub fn new(store: Store) -> Result<Arc<Self>> {
        Self::with_home(store, watcher::default_home())
    }

    /// **The Codex home is the sibling of the Claude one, not the machine's.**
    /// `R-J27`.
    ///
    /// It used to be `codex::default_home()`, which reads `$HOME/.codex`
    /// whatever home was injected — so every caller handing this a *fake*
    /// `.claude` still scanned the real Codex installation beside it. Harmless
    /// while `~/.codex` was empty, and not harmless once it was not: the canary
    /// tests began reporting seven unclassified Codex event types out of the
    /// developer's own sessions, which is a true finding arriving through a
    /// test that had no business seeing it.
    ///
    /// Deriving the sibling keeps production identical — `$HOME/.claude` and
    /// `$HOME/.codex` are siblings by construction — while a test that builds a
    /// fake home gets the fake `.codex` beside it, which is what
    /// [ADR-0006](../../../docs/decisions/0006-inject-the-watch-root.md) means
    /// by injecting the root. `CODEX_HOME` still wins where it is set, because
    /// an explicitly chosen root outranks an inferred one.
    pub fn with_home(store: Store, claude_home: PathBuf) -> Result<Arc<Self>> {
        let codex = match std::env::var("CODEX_HOME") {
            Ok(p) => PathBuf::from(p),
            Err(_) => match claude_home.parent() {
                Some(parent) => parent.join(".codex"),
                None => crate::codex::default_home(),
            },
        };
        Self::with_homes(store, claude_home, codex)
    }

    pub fn with_homes(
        store: Store,
        claude_home: PathBuf,
        codex_home: PathBuf,
    ) -> Result<Arc<Self>> {
        let loaded = store.load_sessions()?;
        let mut sessions = HashMap::new();
        let mut seqs = HashMap::new();
        // Resume every transcript where the last run left off. Without this the
        // first scan after a restart re-reads and re-folds whole files. `R-A6`.
        let mut tailer = Tailer::default();
        for (path, offset) in store.load_tail_offsets().unwrap_or_default() {
            tailer.seed(Path::new(&path), offset);
        }
        for mut s in loaded {
            // Liveness is re-derived from the OS on the first scan; never trust
            // a persisted "alive".
            s.alive = false;
            s.live_status = None;
            // The pid is KEPT, for the reason the death path states in place:
            // a dead session still holding its pid is the only evidence that
            // `/clear` moved that pid to a fresh session id, and the clients'
            // label migration matches on exactly that. Wiping it here made the
            // migration fail across a daemon restart — the same bug the death
            // path already had, one layer up. `alive` is false either way, and
            // consumers that need a *live* pid gate on it.
            seqs.insert(s.id.clone(), store.max_seq(&s.id).unwrap_or(0));
            sessions.insert(s.id.clone(), s);
        }
        let (tx, _) = broadcast::channel(8192);
        let identity = crate::machine::identity(&claude_home, None);
        Ok(Arc::new(AppState {
            store,
            sessions: RwLock::new(sessions),
            changes: RwLock::new(HashMap::new()),
            tx,
            tailer: Mutex::new(tailer),
            seqs: Mutex::new(seqs),
            identity,
            ssh_target: std::sync::OnceLock::new(),
            writes_allowed: std::sync::OnceLock::new(),
            claude_home,
            codex_home,
            attention: AttentionConfig::default(),
            health: Mutex::new(HealthTracker::new(MAX_TRANSCRIPT_BYTES)),
            notifier: Mutex::new(Notifier::default()),
            usage: Mutex::new(UsageScanner::default()),
            signal_running: Mutex::new(std::collections::HashSet::new()),
            last_queue: Mutex::new(Vec::new()),
            scan_running: Mutex::new(()),
            codex_cache: Arc::new(std::sync::Mutex::new(crate::codex::ScanCache::default())),
            non_repo_cwds: Mutex::new(HashMap::new()),
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

    /// Token burn per session/day/repo, the trailing window, and known limit
    /// hits. Incremental — the first call pays for the corpus, later calls
    /// read only appended bytes. `R-G1`–`R-G3`.
    pub async fn usage_report(&self) -> mogeung_core::usage::UsageReport {
        let root = self.claude_home.join("projects");
        let mut guard = self.usage.lock().await;
        let mut scanner = std::mem::take(&mut *guard);
        match tokio::task::spawn_blocking(move || {
            let report = scanner.report(&root, Utc::now());
            (scanner, report)
        })
        .await
        {
            Ok((scanner, report)) => {
                *guard = scanner;
                report
            }
            // A panicked scan loses its cache, not the daemon.
            Err(e) => {
                tracing::warn!("usage scan failed: {e}");
                Default::default()
            }
        }
    }

    // -----------------------------------------------------------------------
    // Insight (pillar F) and docs (pillar H)
    // -----------------------------------------------------------------------

    fn projects_root(&self) -> PathBuf {
        self.claude_home.join("projects")
    }

    fn history_path(&self) -> PathBuf {
        self.claude_home.join("history.jsonl")
    }

    /// `R-F1`. A full pass over the corpus per call — honest and cache-free;
    /// the client sends on Enter, not per keystroke.
    pub async fn insight_search(&self, query: String) -> mogeung_core::insight::SearchResults {
        let (root, hist) = (self.projects_root(), self.history_path());
        tokio::task::spawn_blocking(move || crate::insight::search(&root, &hist, &query, 500))
            .await
            .unwrap_or_default()
    }

    pub async fn day_digest(&self, day: String) -> mogeung_core::insight::DayDigest {
        let root = self.projects_root();
        tokio::task::spawn_blocking(move || crate::insight::digest(&root, &day))
            .await
            .unwrap_or_default()
    }

    pub async fn recurring_failures(&self) -> Vec<mogeung_core::insight::RecurringFailure> {
        let root = self.projects_root();
        tokio::task::spawn_blocking(move || crate::insight::recurring_failures(&root, 2))
            .await
            .unwrap_or_default()
    }

    pub async fn prompt_library(&self) -> Vec<mogeung_core::insight::PromptCluster> {
        let hist = self.history_path();
        tokio::task::spawn_blocking(move || {
            let h = crate::insight::history(&hist);
            crate::insight::prompt_library(&h.entries)
        })
        .await
        .unwrap_or_default()
    }

    pub async fn analytics(&self) -> mogeung_core::insight::Analytics {
        let (root, hist) = (self.projects_root(), self.history_path());
        tokio::task::spawn_blocking(move || {
            let h = crate::insight::history(&hist);
            crate::insight::analytics(&h.entries, &root)
        })
        .await
        .unwrap_or_default()
    }

    pub async fn subagent_tree(
        &self,
        session_id: &str,
    ) -> Vec<mogeung_core::insight::SubagentNode> {
        let root = self.projects_root();
        let id = session_id.to_string();
        tokio::task::spawn_blocking(move || crate::insight::subagent_tree(&root, &id))
            .await
            .unwrap_or_default()
    }

    pub async fn decision_candidates(
        &self,
        session_id: &str,
    ) -> Vec<mogeung_core::insight::DecisionCandidate> {
        let Some(s) = self.get(session_id).await else {
            return Vec::new();
        };
        let path = PathBuf::from(s.transcript_path);
        tokio::task::spawn_blocking(move || crate::insight::decision_candidates(&path))
            .await
            .unwrap_or_default()
    }

    /// `R-F2` — prompt-blame from what the daemon already knows: sessions
    /// whose edit tools named the file, each row carrying its match rule.
    pub async fn file_sessions(&self, path: &str) -> Vec<mogeung_core::insight::FileSession> {
        let needle = path.trim();
        if needle.is_empty() {
            return Vec::new();
        }
        let sessions = self.sessions.read().await;
        let mut out: Vec<mogeung_core::insight::FileSession> = sessions
            .values()
            .filter_map(|s| {
                let hit = s
                    .touched_files
                    .iter()
                    .find(|f| f.as_str() == needle || f.ends_with(needle))?;
                let at = s
                    .recent_touches
                    .iter()
                    .rev()
                    .find(|t| &t.path == hit)
                    .map(|t| t.at);
                Some(mogeung_core::insight::FileSession {
                    session_id: s.id.clone(),
                    label: s.label(),
                    prompt: s.last_prompt.clone(),
                    at: at.or(Some(s.last_event_at)),
                    matched: if hit.as_str() == needle {
                        "edit-tool path match (exact)".to_string()
                    } else {
                        "edit-tool path match (suffix)".to_string()
                    },
                })
            })
            .collect();
        out.sort_by(|a, b| b.at.cmp(&a.at));
        out
    }

    /// `R-H1`–`R-H5`. Only repos the daemon already knows (a session's) may
    /// be scanned — this is not a general filesystem endpoint.
    pub async fn doc_scan(&self, repo: &str) -> Result<mogeung_core::docs::DocInventory> {
        let known = {
            let sessions = self.sessions.read().await;
            sessions.values().any(|s| {
                s.repo_root.as_deref() == Some(repo) || s.cwd == repo
            })
        };
        if !known {
            return Err(anyhow!("not a repo of any watched session: {repo}"));
        }
        let root = PathBuf::from(repo);
        Ok(
            tokio::task::spawn_blocking(move || crate::docscan::scan(&root))
                .await
                .unwrap_or_default(),
        )
    }

    /// A repo's signal state: configured command, in-flight flag, last run.
    pub async fn signal_status(&self, repo: &str) -> ServerMsg {
        let running = self.signal_running.lock().await.contains(repo);
        let command = self.store.signal_command(repo);
        let last = self
            .store
            .signal_last_run(repo)
            .and_then(|j| serde_json::from_str(&j).ok())
            .map(Box::new);
        ServerMsg::SignalStatus {
            repo: repo.to_string(),
            command,
            running,
            last,
        }
    }

    pub async fn set_signal_command(&self, repo: &str, command: &str) {
        if let Err(e) = self.store.set_signal_command(repo, command) {
            tracing::warn!("could not store signal command: {e}");
        }
        let status = self.signal_status(repo).await;
        self.broadcast(status);
    }

    /// Run a repo's configured signal command. Reached only from an explicit
    /// client action — there is deliberately no timer or watcher that can
    /// call this. `R-E2`/`R-E5`.
    pub async fn run_signal(self: &Arc<Self>, session_id: &str) -> Result<()> {
        let s = self
            .get(session_id)
            .await
            .ok_or_else(|| anyhow!("unknown session"))?;
        let repo = s.repo_root.clone().unwrap_or_else(|| s.cwd.clone());
        if repo.is_empty() {
            return Err(anyhow!("session has no working directory yet"));
        }
        let Some(command) = self.store.signal_command(&repo) else {
            return Err(anyhow!("no signal command configured for {repo}"));
        };
        {
            let mut running = self.signal_running.lock().await;
            if !running.insert(repo.clone()) {
                return Err(anyhow!("a signal run is already in flight for {repo}"));
            }
        }
        let status = self.signal_status(&repo).await;
        self.broadcast(status);

        // The session's diff, for coverage-on-changed-lines. Cloned now: the
        // run may take minutes and the diff should be the one you clicked on.
        let changed: Vec<mogeung_core::change::FileChange> = self
            .changes
            .read()
            .await
            .get(session_id)
            .map(|c| c.files.clone())
            .unwrap_or_default();

        let state = Arc::clone(self);
        tokio::spawn(async move {
            let run = crate::runner::run(&repo, &command, &changed).await;
            match serde_json::to_string(&run) {
                Ok(json) => {
                    if let Err(e) = state.store.save_signal_run(&repo, &json) {
                        tracing::warn!("could not store signal run: {e}");
                    }
                }
                Err(e) => tracing::warn!("could not encode signal run: {e}"),
            }
            state.signal_running.lock().await.remove(&repo);
            let status = state.signal_status(&repo).await;
            state.broadcast(status);
        });
        Ok(())
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
        ServerMsg::Snapshot {
            sessions,
            queue,
            daemon: Some(self.daemon_identity()),
        }
    }

    /// The identity as published: the fixed part, plus the ssh target if one
    /// was configured after construction.
    pub fn daemon_identity(&self) -> mogeung_core::wire::DaemonIdentity {
        let mut id = self.identity.clone();
        id.ssh_target = self.ssh_target.get().cloned();
        id
    }

    pub async fn publish_queue(&self) {
        let sessions: Vec<Session> = self.sessions.read().await.values().cloned().collect();
        let queue = rank(&sessions, Utc::now(), &self.attention);
        self.notify_for(&queue, &sessions).await;
        // Notifications diff on their own state above; the broadcast is gated
        // separately, so an unchanged queue costs the clients nothing. New
        // clients are unaffected — they get the queue with their snapshot.
        let mut last = self.last_queue.lock().await;
        if *last == queue {
            return;
        }
        last.clone_from(&queue);
        drop(last);
        self.broadcast(ServerMsg::Queue { queue });
    }

    /// Announce the queue as it stands, gate bypassed. For the request paths:
    /// a client that explicitly asked to rescan is owed the answer even when
    /// it is the same answer, and only the periodic loop may stay silent.
    pub async fn republish_queue(&self) {
        let sessions: Vec<Session> = self.sessions.read().await.values().cloned().collect();
        let queue = rank(&sessions, Utc::now(), &self.attention);
        self.last_queue.lock().await.clone_from(&queue);
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
        // One pass at a time. The interval, a websocket `rescan` and the HTTP
        // rescan can all ask at once; overlapping passes double every
        // subprocess and re-read below, and order the results arbitrarily.
        let Ok(_running) = self.scan_running.try_lock() else {
            return;
        };
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
            let (lines, offset) = {
                let mut t = self.tailer.lock().await;
                let lines = t.read_new(&f.path).unwrap_or_default();
                (lines, t.offset(&f.path))
            };
            if lines.is_empty() && known {
                continue;
            }
            self.apply_lines(&f.session_id, &f.path, lines).await;
            // Recorded only once the lines it covers are folded in, so an
            // interrupted pass re-reads a batch rather than skipping it. `R-A6`.
            if let Some(offset) = offset {
                let _ = self
                    .store
                    .save_tail_offset(&f.path.to_string_lossy(), offset);
            }
            touched_ids.push(f.session_id.clone());
        }

        // A session that was started and not yet spoken to has a registry entry
        // and no transcript at all. Adopt it *after* the transcript pass, never
        // before: a resumed session is live and has a transcript already, and
        // taking it in here would sidestep `ensure_session`'s cap and read an
        // 11 MB history whole inside the scan loop (`R-A5`).
        for e in live_by_id.values() {
            if !self.sessions.read().await.contains_key(&e.session_id) {
                self.ensure_live_session(e).await;
            }
        }

        // Apply liveness to every known session, not just the ones that moved:
        // a session going from busy to idle produces no transcript line, and
        // that transition is the single most important signal we have.
        let ids: Vec<SessionId> = self.sessions.read().await.keys().cloned().collect();
        // Resolved once for the whole pass rather than per session. Both are
        // empty when tmux is absent, which makes the lookup below a no-op
        // instead of a special case. On the blocking pool because each is a
        // forked process whose whole lifetime would otherwise sit on a
        // runtime thread the API server needs.
        let (panes, parents) = tokio::task::spawn_blocking(|| {
            let panes = tmux_panes();
            let parents = if panes.is_empty() {
                HashMap::new()
            } else {
                process_parents()
            };
            (panes, parents)
        })
        .await
        .unwrap_or_default();
        for id in ids {
            let Some(mut s) = self.get(&id).await else {
                continue;
            };
            // Codex sessions have no entry in Claude Code's live registry;
            // their liveness belongs to `scan_codex`. Left here they would be
            // marked dead every pass.
            if s.source == mogeung_core::session::SessionSource::Codex {
                continue;
            }
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
                    // The pid is deliberately KEPT. The live registry is
                    // per-pid, so `/clear` moves the pid to a fresh session
                    // id — and a dead session still holding it is the only
                    // evidence of that succession. The client's label/pin
                    // migration matches on exactly this; wiping it here is
                    // what silently broke the first version of that fix.
                    // Consumers that need a *live* pid already gate on
                    // `alive` (focus_terminal does).
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
                    self.recompute_change_scan(&id).await;
                }
            }
        }

        for id in touched_ids {
            // Keep diffs current for sessions that are actively changing files.
            self.recompute_change_scan(&id).await;
        }

        self.scan_codex().await;
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

    // -----------------------------------------------------------------------
    // Repairing what the missing offsets did (R-A6)
    // -----------------------------------------------------------------------

    /// Undo the duplication caused by restarts that re-read every transcript
    /// from byte 0. Runs once, gated on the schema version.
    ///
    /// Every re-read appended the whole history again under fresh `seq`s — so
    /// nothing collided on the primary key — and folded every counter a second
    /// time on top of itself. The event log and `turns`/`tool_calls`/
    /// `tokens_out` therefore all carry one extra copy per restart.
    ///
    /// A session whose transcript is still on disk is rebuilt rather than
    /// patched: its events are dropped and the file folded in again from
    /// nothing, which is the only honest way to recover a counter. A session
    /// whose transcript is gone can only have its event log deduplicated —
    /// its counters stay as they are, because nothing remains to recount them
    /// from, and inventing a divisor would be worse than leaving the record
    /// visibly odd.
    pub async fn repair_reingested_history(&self) -> Result<()> {
        if self.store.schema_version() >= crate::store::SCHEMA_VERSION {
            return Ok(());
        }
        let sessions: Vec<Session> = self.sessions.read().await.values().cloned().collect();
        let (mut rebuilt, mut deduped, mut dropped) = (0usize, 0usize, 0usize);

        for s in sessions {
            // Codex threads are re-read whole every scan and never went
            // through this path; folding a rollout through the Claude Code
            // adapter would invent history rather than repair it.
            if s.source == mogeung_core::session::SessionSource::Codex {
                continue;
            }
            let path = PathBuf::from(&s.transcript_path);
            let size = std::fs::metadata(&path).ok().map(|m| m.len());
            let Some(size) = size.filter(|_| path.is_file()) else {
                let n = self.store.dedupe_events(&s.id).unwrap_or(0);
                if n > 0 {
                    dropped += n;
                    deduped += 1;
                }
                continue;
            };

            dropped += self.store.delete_events(&s.id).unwrap_or(0);
            self.seqs.lock().await.insert(s.id.clone(), 0);

            let mut s = s;
            s.turns = 0;
            s.tool_calls = 0;
            s.tokens_in = 0;
            s.tokens_out = 0;
            s.touched_files.clear();
            s.recent_touches.clear();
            s.recent_tools.clear();
            s.open_tools.clear();
            s.verify_runs.clear();
            s.claims.clear();
            s.loop_signal = None;
            s.error = None;
            s.limit_hit_at = None;
            s.limit_resets = None;
            let id = s.id.clone();
            self.put(s).await;

            // Same cap a first sighting gets: a pathological transcript is
            // followed from its tail here too, rather than read whole.
            let (lines, offset) = {
                let mut t = self.tailer.lock().await;
                t.forget(&path);
                if size > MAX_TRANSCRIPT_BYTES {
                    t.start_near_end(&path, TAIL_BYTES);
                }
                let lines = t.read_new(&path).unwrap_or_default();
                (lines, t.offset(&path))
            };
            self.apply_lines(&id, &path, lines).await;
            if let Some(offset) = offset {
                let _ = self
                    .store
                    .save_tail_offset(&path.to_string_lossy(), offset);
            }
            rebuilt += 1;
        }

        if dropped > 0 {
            tracing::warn!(
                "repaired re-ingested history: {dropped} duplicate event(s) removed, \
                 {rebuilt} session(s) rebuilt from their transcript, {deduped} \
                 deduplicated in place (transcript gone — their counters cannot \
                 be recovered)"
            );
            // The rows are gone but the pages are not; a database bloated by
            // this bug stays bloated until it is rewritten.
            if let Err(e) = self.store.vacuum() {
                tracing::warn!("vacuum after repair failed: {e}");
            }
        }
        self.store.set_schema_version(crate::store::SCHEMA_VERSION)
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
        let mut s = blank_session(
            f.session_id.clone(),
            mogeung_core::session::SessionSource::ClaudeCode,
            f.modified,
            f.modified,
        );
        s.transcript_path = f.path.to_string_lossy().to_string();
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

    /// Register a session that the live registry knows about and no transcript
    /// mentions yet.
    ///
    /// **Claude Code writes `~/.claude/projects/…/<id>.jsonl` on the first
    /// message, not on launch**, and it writes `~/.claude/sessions/<pid>.json`
    /// within milliseconds of the process starting. Discovering only from
    /// transcripts therefore left a session you had just opened invisible until
    /// you typed something into it — which is precisely when you have stopped
    /// needing to be told where it is. Reported 2026-08-09 as *"I have to run
    /// `/clear` first to make mogeungd detect it"*, and `/clear` worked because
    /// it starts a fresh id **and writes a line**.
    ///
    /// Everything else is left to the liveness pass, which already fills
    /// `alive`, the pid, the status and the tmux target from this same entry —
    /// so a session adopted here is broadcast once, by that pass, rather than
    /// twice. `transcript_path` stays empty until the file appears;
    /// [`Self::apply_lines`] adopts it then. Deriving the path from the cwd was
    /// the alternative and it would be a guess at the slug rule, which is
    /// exactly the kind of undocumented mapping `A4` says will move.
    async fn ensure_live_session(&self, e: &watcher::LiveEntry) {
        let at = e.started_at.unwrap_or_else(Utc::now);
        let mut s = blank_session(
            e.session_id.clone(),
            mogeung_core::session::SessionSource::ClaudeCode,
            at,
            at,
        );
        s.cwd = e.cwd.clone();
        s.name = e.name.clone();
        s.version = e.version.clone();
        // The base has to be pinned **here**, not left to the first line.
        // `recompute_change_inner` needs only a cwd to run, and a session
        // adopted from the registry has one from the moment it is born — so a
        // session opened, left alone and closed again would diff against HEAD,
        // attribute whatever was already uncommitted in that worktree to
        // itself, and exit into the REVIEW tier having done nothing. Pinning at
        // launch is also the more honest base: it is the last commit predating
        // the session, taken at the moment the session actually started rather
        // than whenever it first spoke.
        if let Some((root, base)) = self.probe_repo(&s.cwd, s.started_at).await {
            s.repo_root = Some(root);
            s.base_sha = base;
        }
        self.sessions.write().await.insert(s.id.clone(), s);
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
            // A limit hit pins the session until something moves again: a new
            // human turn (you came back after the reset) or real assistant
            // output (the CLI resumed on its own). `R-G1`.
            if p.limit_hit {
                s.limit_hit_at = Some(p.ts.unwrap_or_else(Utc::now));
                s.limit_resets = p.limit_resets.clone();
            } else if p.is_turn || p.tokens_out > 0 {
                s.limit_hit_at = None;
                s.limit_resets = None;
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

            // Verification-shaped commands become evidence rows, paired with
            // their results below by tool_use id. `R-E1`.
            for (id, kind, cmd) in &p.verify_cmds {
                s.verify_runs.push(VerifyRun {
                    kind: *kind,
                    command: cmd.clone(),
                    at,
                    ok: None,
                    tool_use_id: id.clone(),
                });
            }
            if s.verify_runs.len() > 30 {
                let excess = s.verify_runs.len() - 30;
                s.verify_runs.drain(..excess);
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
                    EventKind::ToolResult {
                        tool_use_id,
                        is_error,
                        ..
                    } => {
                        s.open_tools.retain(|t| &t.id != tool_use_id);
                        // A verify command's outcome is its tool result's
                        // error flag — the exit code, one layer up. `R-E1`.
                        if let Some(r) = s
                            .verify_runs
                            .iter_mut()
                            .rev()
                            .find(|r| r.tool_use_id == *tool_use_id && r.ok.is_none())
                        {
                            r.ok = Some(!is_error);
                        }
                    }
                    _ => {}
                }
            }

            // Claims bind to the newest completed run of their kind — the
            // binding itself is stated on the claim, never implied. `R-E3`.
            for (kind, text) in &p.claims {
                let bound = s
                    .verify_runs
                    .iter()
                    .rev()
                    .find(|r| r.kind == *kind && r.ok.is_some());
                let (evidence, contradicted) = match bound {
                    Some(r) => (
                        format!(
                            "matched: {} — exited {}",
                            r.command,
                            if r.ok == Some(true) { "ok" } else { "nonzero" }
                        ),
                        r.ok == Some(false),
                    ),
                    None => (
                        format!("no {} run seen before this claim", kind.label()),
                        false,
                    ),
                };
                if !s.claims.iter().any(|c| c.text == *text) {
                    s.claims.push(Claim {
                        text: text.clone(),
                        at,
                        kind: *kind,
                        evidence,
                        contradicted,
                    });
                }
            }
            if s.claims.len() > 20 {
                let excess = s.claims.len() - 20;
                s.claims.drain(..excess);
            }
            events.extend(p.events);
        }

        s.loop_signal = detect_loop(&s.recent_tools);

        // Resolve the repo once the cwd is known, and pin a diff base the first
        // time we see the session.
        if s.repo_root.is_none() && !s.cwd.is_empty() {
            if let Some((root, base)) = self.probe_repo(&s.cwd, s.started_at).await {
                s.repo_root = Some(root);
                if s.base_sha.is_none() {
                    // Not HEAD: the last commit predating the session, so work
                    // it committed before mogeung noticed it is still visible.
                    s.base_sha = base;
                }
            }
        }
        if s.transcript_path.is_empty() {
            s.transcript_path = path.to_string_lossy().to_string();
        }

        self.put(s).await;
        self.emit(id, events, last_ts).await;
    }

    /// The repo root and pinned diff base for a session's cwd, or `None` when
    /// the cwd is not inside a git repository.
    ///
    /// The probe forks `git rev-parse`, so a negative answer is remembered
    /// and re-asked only every [`REPO_RECHECK_TICKS`] passes. A positive
    /// answer needs no cache — the caller writes `repo_root` into the
    /// session, and the `repo_root.is_none()` guard stops the asking.
    async fn probe_repo(
        &self,
        cwd_str: &str,
        started_at: chrono::DateTime<Utc>,
    ) -> Option<(String, Option<String>)> {
        {
            let mut neg = self.non_repo_cwds.lock().await;
            if let Some(left) = neg.get_mut(cwd_str) {
                if *left > 0 {
                    *left -= 1;
                    return None;
                }
                neg.remove(cwd_str);
            }
        }
        let cwd = PathBuf::from(cwd_str);
        let found = tokio::task::spawn_blocking(move || {
            if !git::is_repo(&cwd) {
                return None;
            }
            let root = git::repo_root(&cwd).ok()?.to_string_lossy().to_string();
            let base = git::base_for_session(&cwd, started_at).ok();
            Some((root, base))
        })
        .await
        .ok()
        .flatten();
        if found.is_none() {
            self.non_repo_cwds
                .lock()
                .await
                .insert(cwd_str.to_string(), REPO_RECHECK_TICKS);
        }
        found
    }

    // -----------------------------------------------------------------------
    // Codex (R-I1)
    // -----------------------------------------------------------------------

    /// One read-only pass over the Codex install, mapping its threads into
    /// the same Session model. Absent install → absent from health, quietly;
    /// present-but-empty → reported as exactly that.
    async fn scan_codex(&self) {
        use crate::codex::{CodexInstall, CodexStatus};
        use mogeung_core::session::SessionSource;

        let Some(install) = CodexInstall::discover(&self.codex_home) else {
            return;
        };
        let cache = self.codex_cache.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut cache = cache.lock().expect("codex scan cache poisoned");
            let index = install.list_threads();
            let details: Vec<_> = index
                .threads
                .iter()
                .filter(|t| !t.archived)
                .map(|t| {
                    // Incremental: only bytes appended since the last pass
                    // are read and parsed. The whole-file re-read this
                    // replaced was a full-corpus parse at the poll rate.
                    let roll = cache.update(&install, t);
                    (t.clone(), roll)
                })
                .collect();
            let keep: std::collections::HashSet<String> =
                details.iter().map(|(t, _)| t.id.clone()).collect();
            cache.retain(&keep);
            (index.error, details)
        })
        .await;
        let Ok((index_error, details)) = result else {
            return;
        };

        // The Codex canary: unknown rollout kinds, merged across threads and
        // replaced wholesale. The per-thread counts are cumulative (the cache
        // folds lines in as they arrive), so the merge stays equal to what a
        // full re-read would have counted.
        let mut unknown: std::collections::BTreeMap<String, u64> = Default::default();
        for (_, roll) in &details {
            if let Some(r) = roll {
                for (kind, n) in &r.counts.unknown_kinds {
                    *unknown.entry(kind.clone()).or_insert(0) += n;
                }
            }
        }
        self.health.lock().await.set_codex(
            true,
            details.len() as u32,
            index_error,
            unknown.into_iter().collect(),
        );

        let now = Utc::now();
        for (t, roll) in details {
            let status = roll.as_ref().map(|r| r.status());
            let recent = t
                .recency()
                .map(|r| (now - r).num_minutes() < 10)
                .unwrap_or(false);
            // No pid registry exists for Codex; "alive" is recency plus an
            // unfinished turn — a heuristic, and labelled one in the docs.
            let alive = recent && !matches!(status, Some(CodexStatus::Done) | None);
            let live_status = match status {
                Some(CodexStatus::Working) => Some(mogeung_core::LiveStatus::Busy),
                Some(CodexStatus::Waiting) => Some(mogeung_core::LiveStatus::Idle),
                _ => None,
            };

            let existing = self.get(&t.id).await;
            let mut s = existing.unwrap_or_else(|| {
                blank_session(
                    t.id.clone(),
                    SessionSource::Codex,
                    t.created_at.unwrap_or(now),
                    t.updated_at.unwrap_or(now),
                )
            });

            let before = serde_json::to_string(&s).unwrap_or_default();
            s.source = SessionSource::Codex;
            s.title = t.title.clone().or(t.name.clone()).or(s.title);
            s.last_prompt = t
                .first_user_message
                .clone()
                .or(t.preview.clone())
                .filter(|p| !p.is_empty())
                .or(s.last_prompt);
            if let Some(cwd) = &t.cwd {
                if !cwd.is_empty() {
                    s.cwd = cwd.clone();
                }
            }
            s.git_branch = t.git_branch.clone().filter(|b| !b.is_empty()).or(s.git_branch);
            s.version = t.cli_version.clone().or(s.version);
            if let Some(u) = t.updated_at {
                s.last_event_at = u;
            }
            s.alive = alive;
            if s.live_status != live_status {
                s.status_since = Some(now);
            }
            s.live_status = live_status;
            if let Some(r) = &roll {
                s.last_activity = r.last_activity.clone().or(s.last_activity);
                s.tool_calls = r.tool_calls as u32;
                if let Some(u) = &r.last_usage {
                    // Codex reports cumulative totals; adopt, don't add.
                    s.tokens_in = u.total_in();
                    s.tokens_out = u.total_out();
                } else if let Some(total) = t.tokens_used {
                    // Index-only fallback. One combined figure exists; calling
                    // it input (the dominant share) beats inventing a split.
                    s.tokens_in = total;
                }
                if let Some(p) = &t.rollout_path {
                    s.transcript_path = p.to_string_lossy().to_string();
                }
                // A trailing approval request is a permission prompt in Codex
                // clothing — same tier, same reason (`R-B4`'s distinction).
                if matches!(status, Some(CodexStatus::Waiting)) {
                    if s.open_tools.is_empty() {
                        s.open_tools.push(OpenTool {
                            id: "codex-approval".into(),
                            name: "approval".into(),
                            summary: r.last_activity.clone().unwrap_or_default(),
                            at: s.last_event_at,
                        });
                    }
                } else {
                    s.open_tools.retain(|o| o.id != "codex-approval");
                }
            }

            // Pin the diff base exactly like a Claude session — the git
            // observer generalises for free, which is half of A23's answer.
            if s.repo_root.is_none() && !s.cwd.is_empty() {
                if let Some((root, base)) = self.probe_repo(&s.cwd, s.started_at).await {
                    s.repo_root = Some(root);
                    if s.base_sha.is_none() {
                        s.base_sha = base;
                    }
                }
            }

            if serde_json::to_string(&s).unwrap_or_default() != before {
                self.put(s).await;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Diffs and review
    // -----------------------------------------------------------------------

    /// Recompute and always announce, even when the diff is identical. The
    /// request paths use this: a client that just asked for a refresh needs
    /// the answer whether or not the diff moved since last time.
    pub async fn recompute_change(&self, id: &str) -> Option<Change> {
        self.recompute_change_inner(id, true).await
    }

    /// The scan loop's variant: recompute, but stay silent when nothing
    /// changed. A busy agent grows its transcript every tick, and before this
    /// gate every tick re-sent the full diff — all hunks, all lines — to every
    /// client, whether or not the worktree moved.
    pub async fn recompute_change_scan(&self, id: &str) -> Option<Change> {
        self.recompute_change_inner(id, false).await
    }

    async fn recompute_change_inner(&self, id: &str, always_announce: bool) -> Option<Change> {
        let session = self.get(id).await?;
        if session.cwd.is_empty() {
            return None;
        }
        // No transcript has ever been folded into this session, so it has made
        // no tool call we could attribute anything to. That matters because
        // attribution below is skipped when `touched_files` is empty — the
        // session is then credited with the **whole** worktree diff, which is
        // the right answer for a session whose edits we failed to attribute and
        // the wrong one for a session that has not said a word. Reachable since
        // `R-J30`: a session adopted from the live registry has a cwd from the
        // moment it is born, so one opened, left alone and closed again would
        // otherwise exit into the REVIEW tier holding whatever you had left
        // uncommitted in that repo.
        if session.transcript_path.is_empty() {
            return None;
        }
        let reviewed = self.store.reviewed_anchors(id).unwrap_or_default();
        // On the blocking pool: this is one `git diff` plus a read per
        // untracked file, and on a large repo it is the most expensive thing
        // the scan loop does.
        let cwd = session.cwd.clone();
        let base_sha = session.base_sha.clone();
        let mut change = match tokio::task::spawn_blocking(move || {
            git::compute_change(Path::new(&cwd), base_sha.as_deref(), &reviewed)
        })
        .await
        {
            Ok(change) => change,
            Err(_) => return None,
        };

        // Several sessions can share a working tree, so attribute the diff to
        // the files this session actually touched when we know them.
        if !session.touched_files.is_empty() {
            let root = session.repo_root.clone().unwrap_or_else(|| session.cwd.clone());
            let touched: Vec<String> = session
                .touched_files
                .iter()
                .map(|p| relative_to_root(&root, p))
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

        let mut changes = self.changes.write().await;
        let unchanged = changes.get(id) == Some(&change);
        changes.insert(id.to_string(), change.clone());
        drop(changes);
        if always_announce || !unchanged {
            self.broadcast(ServerMsg::ChangeUpdated {
                session_id: id.to_string(),
                change: change.clone(),
            });
        }
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
            // Up to a dozen `git grep`s — blocking-pool work, in the request
            // path of a click.
            let syms = symbols.clone();
            let excl = path.to_string();
            tokio::task::spawn_blocking(move || {
                git::find_references(Path::new(&repo), &syms, &excl)
            })
            .await
            .unwrap_or((Vec::new(), false))
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
        filter: crate::git::LogFilter,
    ) -> Result<(Vec<mogeung_core::wire::CommitInfo>, bool)> {
        let root = self.git_root(id).await?;
        let (mut commits, files, done) = tokio::task::spawn_blocking(move || {
            crate::git::log_page(&root, skip, limit, rev.as_deref(), &filter)
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
        opts: crate::git::DiffOpts,
    ) -> Result<Vec<mogeung_core::change::FileChange>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::stash_show(&root, index, opts)).await?
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
        opts: crate::git::DiffOpts,
    ) -> Result<Vec<mogeung_core::change::FileChange>> {
        let root = self.git_root(id).await?;
        let (from, to) = (from.to_string(), to.to_string());
        tokio::task::spawn_blocking(move || crate::git::diff_range(&root, &from, &to, opts))
            .await?
    }

    pub async fn git_compare(
        &self,
        id: &str,
        branch: &str,
    ) -> Result<(String, String, Vec<mogeung_core::change::FileChange>)> {
        let root = self.git_root(id).await?;
        let branch = branch.to_string();
        tokio::task::spawn_blocking(move || {
            crate::git::compare_with_head(&root, &branch, crate::git::DiffOpts::default())
        })
        .await?
    }

    pub async fn git_reflog(&self, id: &str) -> Result<Vec<mogeung_core::wire::ReflogEntry>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::reflog(&root)).await?
    }

    pub async fn git_worktrees(
        &self,
        id: &str,
    ) -> Result<Vec<mogeung_core::wire::WorktreeInfo>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::worktrees(&root)).await?
    }

    pub async fn git_conflict_stages(
        &self,
        id: &str,
        rel: &str,
    ) -> Result<(String, String, String, bool)> {
        let root = self.git_root(id).await?;
        let rel = rel.to_string();
        tokio::task::spawn_blocking(move || crate::git::conflict_stages(&root, &rel)).await?
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
        opts: crate::git::DiffOpts,
    ) -> Result<(
        Vec<mogeung_core::change::FileChange>,
        Option<mogeung_core::wire::CommitDetail>,
    )> {
        let root = self.git_root(id).await?;
        // R-D17: the union of every reviewed anchor across this repo's
        // sessions, so a commit's hunks arrive knowing whether a human
        // has already read that code. Anchors are content hashes, which
        // is exactly what lets a mark made in the Changes tab land on the
        // same hunk seen through a commit.
        let reviewed = {
            let repo = root.to_string_lossy().to_string();
            let sessions = self.sessions.read().await;
            let ids: Vec<String> = sessions
                .values()
                .filter(|s| s.repo_root.as_deref() == Some(repo.as_str()) || s.cwd == repo)
                .map(|s| s.id.clone())
                .collect();
            drop(sessions);
            let mut set = std::collections::HashSet::new();
            for sid in ids {
                if let Ok(anchors) = self.store.reviewed_anchors(&sid) {
                    set.extend(anchors);
                }
            }
            set
        };
        let sha = sha.to_string();
        tokio::task::spawn_blocking(move || crate::git::show_commit(&root, &sha, opts, &reviewed))
            .await?
    }

    pub async fn git_status(&self, id: &str) -> Result<Vec<mogeung_core::wire::StatusEntry>> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::status(&root)).await?
    }

    /// Stage paths in the session's repository. `R-D19`.
    pub async fn git_stage(&self, id: &str, paths: Vec<String>) -> Result<()> {
        let root = self.write_target(id, &paths).await?;
        tokio::task::spawn_blocking(move || crate::git::stage(&root, &paths)).await?
    }

    /// Unstage paths, leaving the working tree alone. `R-D19`.
    pub async fn git_unstage(&self, id: &str, paths: Vec<String>) -> Result<()> {
        let root = self.write_target(id, &paths).await?;
        tokio::task::spawn_blocking(move || crate::git::unstage(&root, &paths)).await?
    }

    /// Throw away local changes. Destroys work git has never seen. `R-D19`.
    pub async fn git_discard(&self, id: &str, paths: Vec<String>) -> Result<()> {
        let root = self.write_target(id, &paths).await?;
        tokio::task::spawn_blocking(move || crate::git::discard(&root, &paths)).await?
    }

    /// Whether repository writes are permitted on this bind. `R-D19`.
    ///
    /// See [`AppState::writes_allowed`] for why unset means yes.
    pub fn may_write(&self) -> bool {
        *self.writes_allowed.get().unwrap_or(&true)
    }

    /// Commit what is staged in the session's repository. `R-D20`.
    ///
    /// The trailer is built here rather than by the client, so the id recorded
    /// is the one the daemon knows the session by — a client that guessed it
    /// would write a value `R-F2` could never look anything up with.
    pub async fn git_commit(
        &self,
        id: &str,
        message: String,
        amend: bool,
        session_trailer: bool,
    ) -> Result<String> {
        let root = self.git_root(id).await?;
        let trailers = match session_trailer {
            true => vec![(crate::git::SESSION_TRAILER.to_string(), id.to_string())],
            false => Vec::new(),
        };
        tokio::task::spawn_blocking(move || {
            crate::git::commit(&root, &message, amend, &trailers)
        })
        .await?
    }

    /// Create a branch, and optionally move onto it. `R-D21`.
    pub async fn git_branch_create(&self, id: &str, name: String, switch_to: bool) -> Result<()> {
        let root = self.git_root(id).await?;
        let moved = root.clone();
        tokio::task::spawn_blocking(move || crate::git::branch_create(&root, &name, switch_to))
            .await??;
        if switch_to {
            self.unpin_diff_bases(&moved).await;
        }
        Ok(())
    }

    /// Move the worktree onto an existing branch. `R-D21`.
    pub async fn git_switch(&self, id: &str, name: String) -> Result<()> {
        let root = self.git_root(id).await?;
        let moved = root.clone();
        tokio::task::spawn_blocking(move || crate::git::switch(&root, &name)).await??;
        self.unpin_diff_bases(&moved).await;
        Ok(())
    }

    pub async fn git_stash_push(
        &self,
        id: &str,
        message: String,
        include_untracked: bool,
    ) -> Result<()> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || {
            crate::git::stash_push(&root, &message, include_untracked)
        })
        .await?
    }

    pub async fn git_stash_pop(&self, id: &str, index: u32) -> Result<()> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::stash_pop(&root, index)).await?
    }

    pub async fn git_stash_drop(&self, id: &str, index: u32) -> Result<()> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::stash_drop(&root, index)).await?
    }

    /// Update remote-tracking refs. `R-D25`.
    pub async fn git_fetch(&self, id: &str) -> Result<crate::git::FetchReport> {
        let root = self.git_root(id).await?;
        tokio::task::spawn_blocking(move || crate::git::fetch(&root)).await?
    }

    /// Every note. `R-B35`.
    pub async fn notes(&self) -> Result<Vec<mogeung_core::wire::Note>> {
        self.store.load_notes()
    }

    /// Create or update a note, and mirror it. `R-B35`.
    ///
    /// The store is written first and the mirror second, deliberately: the
    /// mirror is a copy, and a copy that cannot be written must not cost the
    /// user the writing it was meant to protect
    /// ([ADR-0015](../../../docs/decisions/0015-markdown-is-the-truth.md)).
    pub async fn save_note(
        &self,
        id: String,
        body: String,
        session_id: Option<String>,
        seq: Option<u64>,
        repo: Option<String>,
    ) -> Result<Vec<mogeung_core::wire::Note>> {
        let now = Utc::now().timestamp();
        let existing = self
            .store
            .load_notes()?
            .into_iter()
            .find(|n| n.id == id && !id.is_empty());
        let note = mogeung_core::wire::Note {
            created: existing.as_ref().map(|n| n.created).unwrap_or(now),
            id: if id.is_empty() {
                crate::notes::new_id()
            } else {
                id
            },
            body,
            updated: now,
            session_id,
            seq,
            repo,
        };
        self.store.save_note(&note)?;
        if let Err(e) = crate::notes::mirror(&note) {
            // Worth saying once, and worth not failing over.
            tracing::warn!("could not mirror note {} to disk: {e}", note.id);
        }
        self.store.load_notes()
    }

    /// Forget a note. The one destructive verb over content nothing can
    /// recompute, so it takes the mirror with it — a file left behind would
    /// read as a live note to everything that is not mogeung.
    pub async fn delete_note(&self, id: &str) -> Result<Vec<mogeung_core::wire::Note>> {
        self.store.delete_note(id)?;
        crate::notes::unmirror(id);
        self.store.load_notes()
    }

    /// Resolve one conflicted file. `R-D22`.
    pub async fn git_resolve(
        &self,
        id: &str,
        path: String,
        side: mogeung_core::wire::ResolveSide,
    ) -> Result<()> {
        let root = self.write_target(id, std::slice::from_ref(&path)).await?;
        let side = match side {
            mogeung_core::wire::ResolveSide::Ours => crate::git::Side::Ours,
            mogeung_core::wire::ResolveSide::Theirs => crate::git::Side::Theirs,
            mogeung_core::wire::ResolveSide::Mine => crate::git::Side::Mine,
        };
        tokio::task::spawn_blocking(move || crate::git::resolve(&root, &path, side)).await?
    }

    /// Forget the pinned diff base of every session in this repository. `R-D21`.
    ///
    /// A session's base is *the last commit before it started*, resolved once
    /// and kept ([A9](../../../docs/product/assumptions.md)) — which is right
    /// while HEAD only moves forward, and wrong the moment a branch switch
    /// puts it on another line of history entirely. Left alone, the Changes
    /// tab would go on diffing against a commit that is no longer an ancestor
    /// of anything checked out: not an error, just a diff nobody can act on.
    ///
    /// Clearing it rather than recomputing it here, because the scan loop
    /// already resolves a missing base on its next pass. One place that knows
    /// how to compute a base beats two.
    ///
    /// Every session in the worktree, not only the one that asked: they share
    /// the checkout, so they all just had the ground moved.
    async fn unpin_diff_bases(&self, root: &Path) {
        let root = root.to_string_lossy().to_string();
        let mut changed = Vec::new();
        {
            let mut sessions = self.sessions.write().await;
            for s in sessions.values_mut() {
                if s.repo_root.as_deref() == Some(root.as_str()) && s.base_sha.is_some() {
                    s.base_sha = None;
                    changed.push(s.clone());
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

    /// The repository a write verb may touch, once every path in it has been
    /// checked. `R-D19`.
    ///
    /// One door for all three verbs, so a fourth cannot be added that forgets
    /// to knock. It resolves the session's repository root — which is what
    /// bounds the write, since git itself will refuse a pathspec outside the
    /// repository — and then refuses any path that could name something
    /// outside it.
    async fn write_target(&self, id: &str, paths: &[String]) -> Result<PathBuf> {
        for p in paths {
            refuse_escape(p)?;
        }
        self.git_root(id).await
    }

    pub async fn git_diff_file(
        &self,
        id: &str,
        rel: &str,
        opts: crate::git::DiffOpts,
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
        tokio::task::spawn_blocking(move || crate::git::diff_file(&root, &rel, opts)).await?
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
            // Forgetting in memory only would leave the old offset on disk to
            // be seeded back on the next start, so a re-discovered session
            // would resume mid-file instead of being read afresh.
            let _ = self.store.delete_tail_offset(&s.transcript_path);
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
    /// On macOS: resolves the session's pid to its controlling tty, works out
    /// which terminal application owns that process by walking its ancestry,
    /// and asks that application (over AppleScript) to focus the matching tab.
    ///
    /// On Linux/X11: walks the pid's ancestry to the process that owns a
    /// window and activates it with wmctrl or xdotool, whichever exists. On
    /// Wayland it refuses honestly — no protocol lets one application raise
    /// another's window, and pretending otherwise would fail mutely. `R-I3`.
    pub async fn focus_terminal(&self, id: &str) -> Result<()> {
        let session = self
            .get(id)
            .await
            .ok_or_else(|| anyhow!("no such session"))?;
        let pid = session
            .pid
            .filter(|_| session.alive)
            .ok_or_else(|| anyhow!("session is not running, so it has no terminal"))?;

        // Runtime-selected rather than cfg-gated so both halves compile — and
        // stay testable — on every platform.
        if cfg!(target_os = "macos") {
            focus_terminal_macos(pid)
        } else {
            focus_terminal_linux(pid)
        }
    }

    /// Open a terminal running interactive `claude` in `dir`.
    ///
    /// This is the one thing mogeung starts, and note what it starts: the real
    /// CLI, in your terminal, with nothing wrapped. It exists because the other
    /// half of v0.1's failure was that reaching three or four parallel sessions
    /// was awkward.
    ///
    /// macOS drives Terminal.app over AppleScript; Linux walks the candidate
    /// table in [`linux_terminal_attempts`], preferring a session under tmux
    /// (ADR-0010) so what it starts is hostable, not merely visible. `R-I3`.
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

        // Runtime-selected, like `focus_terminal`, so both compile everywhere.
        if cfg!(target_os = "macos") {
            launch_terminal_macos(&target)
        } else {
            launch_terminal_linux(&target)
        }
    }
}

/// The macOS launch path, byte-for-byte the original: `open -a Terminal`
/// cannot carry a command, so drive Terminal.app directly. Failing that, fall
/// back to just opening the directory.
fn launch_terminal_macos(target: &Path) -> Result<()> {
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

/// The Linux launch path: first terminal emulator that spawns wins, each
/// handed the same argv — no shell ever sees an interpolated string.
fn launch_terminal_linux(target: &Path) -> Result<()> {
    let dir = target.to_string_lossy();
    // tmux preference is decided here, at runtime, and passed into the pure
    // composition so tests can pin both shapes on any machine.
    let tmux = std::process::Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let stamp = Utc::now().format("%m%d-%H%M%S").to_string();
    let cmd = in_terminal_command(&dir, tmux, &stamp);

    let mut tried: Vec<String> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    for (program, args, cwd) in linux_terminal_attempts(&dir, &cmd) {
        let mut c = std::process::Command::new(&program);
        c.args(&args);
        if let Some(d) = &cwd {
            c.current_dir(d);
        }
        // stderr captured, not inherited: a terminal that rejects its
        // arguments says why, and that sentence is the whole diagnosis.
        c.stderr(std::process::Stdio::piped());
        let Ok(mut child) = c.spawn() else {
            // Not installed is the ordinary case for most rows; move on.
            tried.push(program);
            continue;
        };

        // **Spawning is not launching.** A terminal handed flags it does not
        // understand starts, prints a usage error and exits — and `spawn`
        // has already returned `Ok`, so this used to report success while
        // nothing appeared on screen. That is exactly how `x-terminal-emulator
        // -e tmux new-session …` failed against terminator, whose `-e` takes a
        // single string: silently, with a cheerful green result.
        //
        // So: give it a moment, and ask whether it is still alive.
        std::thread::sleep(std::time::Duration::from_millis(400));
        match child.try_wait() {
            // Still running — a real terminal window. Reap off-frame, or
            // every launch leaves a zombie behind.
            Ok(None) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            // Exited cleanly and immediately. Normal for the client/server
            // terminals: `gnome-terminal` hands the window to its own daemon
            // and returns 0 at once. Success, and not to be confused with
            // the case below.
            Ok(Some(st)) if st.success() => return Ok(()),
            // Exited with an error. This is the one that used to be invisible.
            Ok(Some(st)) => {
                let mut why = String::new();
                if let Some(mut e) = child.stderr.take() {
                    use std::io::Read as _;
                    let _ = e.read_to_string(&mut why);
                }
                let first = why
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .unwrap_or("no message");
                refused.push(format!(
                    "{program} exited {} — {first}",
                    st.code().unwrap_or(-1)
                ));
            }
            Err(_) => tried.push(program),
        }
    }
    // A terminal that refused its arguments is a bug in this table, not a
    // missing program, and saying so is the difference between a report we
    // can act on and "the button does nothing".
    if !refused.is_empty() {
        return Err(anyhow!(
            "no terminal would start. {}",
            refused.join("; ")
        ));
    }
    Err(anyhow!(
        "no terminal emulator found — tried, in order: {}",
        tried.join(", ")
    ))
}

/// One way a terminal might launch: program, arguments, and an optional
/// working directory for emulators whose only "start here" is inheritance.
/// The same shape as `ui.rs::attempts`, for the same reason.
type LaunchAttempt = (String, Vec<String>, Option<String>);

/// The command a freshly launched terminal runs, as argv.
///
/// With tmux installed the session starts under it — what `yolomo` does, and
/// why (ADR-0010): tmux owns the pty, so mogeung can host the session in a
/// pane instead of only pointing at it. Without tmux it is a bare `claude`,
/// visible but not hostable. The stamp keeps names unique without probing the
/// server, and the name is sanitised the way `yolomo` does it — `:` and `.`
/// are tmux target separators, so a raw directory name could be unaddressable.
/// The tmux session name for a launch: `mogeung-<place>-<stamp>`.
///
/// `<place>` is the directory's own name, except when that *is* the stamp —
/// which is exactly what an isolated worktree looks like, since
/// [`crate::git::add_worktree`] lays them out as
/// `<root>/<repo>/<stamp>`. Naming it from the basename there produced
/// `mogeung-0802-013329-0802-013329`: the timestamp twice, and the repository
/// nowhere.
///
/// So a directory named after the stamp defers to its parent, which in that
/// layout is the repository — giving `mogeung-<repo>-<stamp>`, the name you
/// would have written yourself. Nothing else changes: an ordinary directory
/// still names itself.
fn session_name(dir: &str, stamp: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect()
    };
    let path = Path::new(dir);
    let own = clean(&path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let place = if own.is_empty() || own == stamp {
        // The parent, when it adds anything. A worktree root that is itself
        // called by the stamp would leave this empty, and a bare
        // `mogeung-<stamp>` is still unique and still readable.
        clean(
            &path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    } else {
        own
    };
    if place.is_empty() {
        format!("mogeung-{stamp}")
    } else {
        format!("mogeung-{place}-{stamp}")
    }
}

/// Where `claude` actually is, as an absolute path. `R-I3`.
///
/// The launcher spawns a terminal which runs `tmux` which execs this — none of
/// which is a login shell, and none of which reads `.zshrc`. `claude` normally
/// lives in `~/.local/bin`, which is put on `PATH` by exactly those files. So
/// the name alone resolves in the terminal you type in and **not** in the one
/// mogeung opens: tmux fails to exec it, the session dies at once, and the
/// window vanishes inside a second with nothing written anywhere.
///
/// This is the same lesson as `R-I6`'s remote tmux — a spawned shell does not
/// have your profile — arriving locally, which is the half that was not fixed
/// then.
///
/// The daemon's own `PATH` is tried first because it was usually started from
/// a shell that had one, then the places installers actually use. Falling back
/// to the bare name is deliberate: if none of this finds it, the error should
/// be git— the *terminal's* own "command not found", which at least names what
/// it looked for.
fn claude_binary() -> String {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join("claude");
            if cand.is_file() {
                return cand.to_string_lossy().into_owned();
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    for cand in [
        format!("{home}/.local/bin/claude"),
        format!("{home}/.claude/local/claude"),
        format!("{home}/bin/claude"),
        "/opt/homebrew/bin/claude".to_string(),
        "/usr/local/bin/claude".to_string(),
    ] {
        if Path::new(&cand).is_file() {
            return cand;
        }
    }
    "claude".to_string()
}

/// What `yolomo` adds, and the whole of what "yolo mode" means here:
/// `claude` stops asking before it runs a tool.
///
/// Asked for directly 2026-08-02. It is the agent's own flag and mogeung only
/// passes it — this is still not wrapping the conversation ([ADR-0003]) — but
/// it changes what a click costs, so the window says so in the dialog rather
/// than leaving it to be discovered.
const SKIP_PERMISSIONS: &str = "--dangerously-skip-permissions";

fn in_terminal_command(dir: &str, tmux_available: bool, stamp: &str) -> Vec<String> {
    let claude = claude_binary();
    if !tmux_available {
        return vec![claude, SKIP_PERMISSIONS.to_string()];
    }
    vec![
        "tmux".to_string(),
        "new-session".to_string(),
        "-s".to_string(),
        session_name(dir, stamp),
        "-c".to_string(),
        dir.to_string(),
        claude,
        SKIP_PERMISSIONS.to_string(),
    ]
}

/// What `x-terminal-emulator` actually is on this machine.
///
/// It is a Debian *alternatives* symlink, so it points at whatever terminal
/// the user picked — and the flag that takes a command differs between them.
/// The table below used to give it xterm's `-e argv…`, which is right for
/// xterm and wrong for terminator, whose `-e` takes **one command string** and
/// which answers a trailing argv with a usage error.
///
/// Resolving it means the alternatives entry gets the row belonging to the
/// program it really is, rather than a guess that happens to suit xterm.
fn resolved_alternative() -> Option<String> {
    std::fs::canonicalize("/usr/bin/x-terminal-emulator")
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
}

/// The Linux terminal candidates, in order — the first program that *starts
/// and stays up* wins. The Debian alternatives symlink goes first because it
/// is the user's own choice, resolved to whatever it points at so it gets the
/// right flags. `cmd` is appended as argv, never joined into a string.
fn linux_terminal_attempts(dir: &str, cmd: &[String]) -> Vec<LaunchAttempt> {
    let with_cmd = |mut pre: Vec<String>| -> Vec<String> {
        pre.extend(cmd.iter().cloned());
        pre
    };
    // Terminator needs two things, and the second is the one that made the
    // first look like it had not worked.
    //
    // `-x` rather than `-e`, because its `-e` takes a single command string
    // and answers a trailing argv with a usage error.
    //
    // `-u` (`--no-dbus`) because terminator is a **DBus application**: with an
    // instance already running — which there always is, since that is the
    // terminal the user lives in — a new invocation hands the request to the
    // existing process and exits 0 straight away. The running instance opens a
    // window from its own defaults and drops the `-x` command entirely, so
    // `claude` never starts. Nothing reports a failure because, as far as the
    // process tree is concerned, nothing failed: exit 0, immediately, which is
    // also exactly how `gnome-terminal` succeeds. `-u` forces a fresh instance
    // that honours its own arguments.
    let terminator = |name: &str| -> LaunchAttempt {
        (
            name.to_string(),
            with_cmd(vec![
                "-u".into(),
                format!("--working-directory={dir}"),
                "-x".into(),
            ]),
            None,
        )
    };
    let mut out: Vec<LaunchAttempt> = Vec::new();
    // The user's chosen terminal, under its real name and its real flags.
    match resolved_alternative().as_deref() {
        Some("terminator") => out.push(terminator("x-terminal-emulator")),
        Some("gnome-terminal") | Some("gnome-terminal.real") => out.push((
            "x-terminal-emulator".to_string(),
            with_cmd(vec![format!("--working-directory={dir}"), "--".into()]),
            None,
        )),
        Some("konsole") => out.push((
            "x-terminal-emulator".to_string(),
            with_cmd(vec!["--workdir".into(), dir.into(), "-e".into()]),
            None,
        )),
        Some("xfce4-terminal") => out.push((
            "x-terminal-emulator".to_string(),
            with_cmd(vec![format!("--working-directory={dir}"), "-x".into()]),
            None,
        )),
        Some("kitty") => out.push((
            "x-terminal-emulator".to_string(),
            with_cmd(vec!["--directory".into(), dir.into()]),
            None,
        )),
        // xterm, alacritty, urxvt and friends: `-e` really does take argv.
        // An unknown alternative gets the same treatment, which is the old
        // behaviour and still the commonest shape.
        _ => out.push((
            "x-terminal-emulator".to_string(),
            with_cmd(vec!["-e".into()]),
            Some(dir.to_string()),
        )),
    }
    out.extend(vec![
        terminator("terminator"),
        ("gnome-terminal".to_string(), with_cmd(vec![format!("--working-directory={dir}"), "--".into()]), None),
        ("konsole".to_string(), with_cmd(vec!["--workdir".into(), dir.into(), "-e".into()]), None),
        // `-x` takes the remainder as argv; `-e` would want one shell-parsed
        // string, which is exactly the interpolation this table exists to avoid.
        ("xfce4-terminal".to_string(), with_cmd(vec![format!("--working-directory={dir}"), "-x".into()]), None),
        ("alacritty".to_string(), with_cmd(vec!["--working-directory".into(), dir.into(), "-e".into()]), None),
        ("kitty".to_string(), with_cmd(vec!["--directory".into(), dir.into()]), None),
        ("xterm".to_string(), with_cmd(vec!["-e".into()]), Some(dir.to_string())),
    ]);
    out
}

/// The macOS focus path: pid → controlling tty → the terminal application
/// that owns it, asked over AppleScript to raise the matching tab.
fn focus_terminal_macos(pid: u32) -> Result<()> {
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

/// The Linux focus path. X11 only, and honestly so: Wayland has no protocol
/// for one application to raise another's window, so the refusal says that
/// instead of failing mutely. `R-I3`.
///
/// The window search walks the pid's ancestry — the session's pid is
/// `claude`, whose window (if any) belongs to a terminal several parents up.
/// A session under tmux dead-ends at the tmux *server*, a daemon with no
/// window; the thing actually on screen is whichever client is attached, so
/// the walk starts from that client instead — see [`linux_focus_pid`].
fn focus_terminal_linux(pid: u32) -> Result<()> {
    if let Some(msg) = wayland_refusal(std::env::var("XDG_SESSION_TYPE").ok().as_deref()) {
        return Err(anyhow!(msg));
    }

    let focus_pid = linux_focus_pid(pid)?;
    let chain = ancestor_chain(focus_pid, &process_parents());

    // wmctrl first: one call lists every window with its owning pid.
    match std::process::Command::new("wmctrl").args(["-l", "-p"]).output() {
        Ok(out) if out.status.success() => {
            let windows = parse_wmctrl_windows(&String::from_utf8_lossy(&out.stdout));
            let id = window_for(&chain, &windows).ok_or_else(|| {
                anyhow!(
                    "no window claims pid {focus_pid} or its ancestors — \
                     the terminal may not report its pid to the window manager"
                )
            })?;
            let st = std::process::Command::new("wmctrl")
                .args(["-i", "-a", &id])
                .status()
                .map_err(|e| anyhow!("wmctrl failed: {e}"))?;
            return if st.success() {
                Ok(())
            } else {
                Err(anyhow!("wmctrl could not activate window {id}"))
            };
        }
        // Not installed, or the X connection failed — try the next tool.
        _ => {}
    }

    // xdotool: no all-windows-with-pids listing, so search per ancestor,
    // closest first.
    let mut xdotool_present = false;
    for p in &chain {
        match std::process::Command::new("xdotool")
            .args(["search", "--pid", &p.to_string()])
            .output()
        {
            Ok(out) => {
                xdotool_present = true;
                let id = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .find_map(|l| (!l.trim().is_empty()).then(|| l.trim().to_string()));
                if let Some(id) = id {
                    let st = std::process::Command::new("xdotool")
                        .args(["windowactivate", &id])
                        .status()
                        .map_err(|e| anyhow!("xdotool failed: {e}"))?;
                    return if st.success() {
                        Ok(())
                    } else {
                        Err(anyhow!("xdotool could not activate window {id}"))
                    };
                }
            }
            Err(_) => break,
        }
    }

    if xdotool_present {
        Err(anyhow!(
            "no window claims pid {focus_pid} or its ancestors — \
             the terminal may not report its pid to the window manager"
        ))
    } else {
        Err(anyhow!(
            "focusing a window needs wmctrl or xdotool, and neither is \
             installed — install one and jump-to-terminal works on X11"
        ))
    }
}

/// The honest Wayland refusal, split out so the words are testable. `None`
/// means "not Wayland — carry on".
fn wayland_refusal(session_type: Option<&str>) -> Option<String> {
    (session_type == Some("wayland")).then(|| {
        "this desktop runs Wayland, which has no protocol for one application \
         to raise another's window — the X11 tools (wmctrl, xdotool) cannot \
         help here. mogeung can only point you at the session"
            .to_string()
    })
}

/// The pid whose window should be raised. A bare session is its own answer;
/// a tmux session's visible surface is the attached client's terminal, so
/// focus that — and when nothing is attached, say so usefully rather than
/// hunting for a window the tmux server does not have.
fn linux_focus_pid(pid: u32) -> Result<u32> {
    let Some(target) = tmux_target_of(pid) else {
        return Ok(pid);
    };
    let session = target.split(':').next().unwrap_or(&target).to_string();
    tmux_client_pids(&session).into_iter().next().ok_or_else(|| {
        anyhow!(
            "this session runs under tmux ({target}) with no terminal attached — \
             attach one with `tmux attach -t '{session}'`, or use mogeung's Agent tab"
        )
    })
}

/// The pids of clients attached to a tmux session. Empty when tmux is absent,
/// the server is down, or nothing is attached — all ordinary, none an error.
fn tmux_client_pids(session: &str) -> Vec<u32> {
    let Ok(out) = std::process::Command::new("tmux")
        .args(["list-clients", "-t", &format!("={session}"), "-F", "#{client_pid}"])
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_pid_lines(&String::from_utf8_lossy(&out.stdout))
}

fn parse_pid_lines(stdout: &str) -> Vec<u32> {
    stdout.lines().filter_map(|l| l.trim().parse().ok()).collect()
}

/// Parse `wmctrl -lp` into `(owning pid, window id)` pairs. Lines look like
/// `0x03c00003 -1 2233 host title…`; a window that does not report its pid
/// shows 0, which no real process chain contains, so it can never match.
fn parse_wmctrl_windows(stdout: &str) -> Vec<(u32, String)> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            let id = it.next()?.to_string();
            let _desktop = it.next()?;
            let pid: u32 = it.next()?.parse().ok()?;
            Some((pid, id))
        })
        .collect()
}

/// `pid` and its ancestors, closest first, walked in the given table.
/// Bounded and cycle-proof for the same reason [`tmux_target_in`] is: a
/// corrupt table must not spin the daemon.
fn ancestor_chain(pid: u32, parents: &HashMap<u32, u32>) -> Vec<u32> {
    let mut chain = vec![pid];
    let mut current = pid;
    for _ in 0..24 {
        let Some(&parent) = parents.get(&current) else { break };
        if parent <= 1 || parent == current || chain.contains(&parent) {
            break;
        }
        chain.push(parent);
        current = parent;
    }
    chain
}

/// The window belonging to the closest ancestor that owns one. Closest-first
/// matters: on a nested desktop the far ancestry can reach the compositor,
/// whose window is very much not the one to raise.
fn window_for(chain: &[u32], windows: &[(u32, String)]) -> Option<String> {
    chain.iter().find_map(|pid| {
        windows
            .iter()
            .find(|(p, _)| p == pid)
            .map(|(_, id)| id.clone())
    })
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

    /// Noticed 2026-08-02 in a real `tmux ls`:
    /// `mogeung-0802-013329-0802-013329` — the timestamp twice, and no clue
    /// which repository it belonged to.
    ///
    /// An isolated worktree lives at `<root>/<repo>/<stamp>`, so its own
    /// directory name *is* the stamp. Deferring to the parent puts the
    /// repository back in the name, which is the thing you actually search
    /// `tmux ls` for.
    #[test]
    fn a_worktrees_session_is_named_after_its_repo_not_the_stamp_twice() {
        let stamp = "0802-013329";
        assert_eq!(
            session_name("/home/k/.mogeung/worktrees/mogeung/0802-013329", stamp),
            "mogeung-mogeung-0802-013329"
        );
        // An ordinary directory is unaffected, including an awkward one.
        assert_eq!(
            session_name("/home/me/my proj.v2", "0729-101500"),
            "mogeung-my-proj-v2-0729-101500"
        );
        // Degenerate paths still produce something unique and typeable.
        assert_eq!(session_name("/", stamp), format!("mogeung-{stamp}"));
        assert_eq!(session_name(stamp, stamp), format!("mogeung-{stamp}"));
        // And nothing anywhere carries the stamp twice.
        for dir in [
            "/home/k/.mogeung/worktrees/mogeung/0802-013329",
            "/home/me/my proj.v2",
            "/",
            "0802-013329",
        ] {
            let name = session_name(dir, stamp);
            assert_eq!(
                name.matches(stamp).count(),
                1,
                "{dir} produced {name}"
            );
        }
    }

    /// Regression, reported 2026-08-02 once the window finally opened: it
    /// closed again inside a second.
    ///
    /// `claude` lives in `~/.local/bin`, which reaches `PATH` through `.zshrc`
    /// — and nothing in this chain is a login shell, let alone an interactive
    /// one. The terminal runs tmux, tmux execs `claude`, the exec fails, the
    /// session dies immediately and the window goes with it. Nothing is
    /// written anywhere, because from tmux's point of view the session simply
    /// ended.
    ///
    /// So the command must carry an absolute path, resolved by the daemon,
    /// which does have a usable `PATH`.
    #[test]
    fn the_launch_command_names_claude_by_absolute_path() {
        let cmd = in_terminal_command("/some/repo", true, "0102-030405");
        let last = &cmd[cmd.len() - 2];
        // On a machine that has it, this is absolute. On one that does not,
        // the bare name survives on purpose, so the terminal's own "command
        // not found" is what the user sees rather than silence.
        if last != "claude" {
            assert!(
                Path::new(last).is_absolute(),
                "must be absolute or the bare fallback: {last}"
            );
            assert!(last.ends_with("claude"), "{last}");
        }
        // And the no-tmux path agrees with the tmux path about what to run.
        let bare = in_terminal_command("/some/repo", false, "0102-030405");
        assert_eq!(bare, vec![last.clone(), SKIP_PERMISSIONS.to_string()]);
    }

    /// Regression, reported from a Linux desktop: **"+" did nothing.**
    ///
    /// `x-terminal-emulator` is a Debian *alternatives* symlink, and the table
    /// gave it xterm's `-e argv…`. On a machine where it points at terminator
    /// — whose `-e` takes a single command string — that is a usage error, and
    /// the process exits at once. `spawn` had already returned `Ok`, so the
    /// launch reported success and no window ever appeared.
    ///
    /// Two things had to change and this pins the first: the alternatives
    /// entry gets the flags of the program it actually is.
    #[test]
    fn the_alternatives_symlink_gets_its_real_programs_flags() {
        let cmd = vec!["tmux".to_string(), "new-session".to_string()];
        let list = linux_terminal_attempts("/repo", &cmd);

        // Whatever this machine resolves to, terminator's own row must exist
        // and must use `-x` rather than `-e`: `-e` would swallow only the
        // first word and reject the rest.
        let t = list
            .iter()
            .find(|(p, _, _)| p == "terminator")
            .expect("terminator is a candidate in its own right");
        assert!(t.1.contains(&"-x".to_string()), "{:?}", t.1);
        // Without `-u` the request goes over DBus to the instance the user is
        // already sitting in, which opens a window from its own defaults and
        // silently drops the command — exit 0, no `claude`, nothing to read.
        assert!(
            t.1.contains(&"-u".to_string()),
            "terminator must not delegate over DBus: {:?}",
            t.1
        );
        assert!(!t.1.contains(&"-e".to_string()), "-e takes one string: {:?}", t.1);
        assert!(
            t.1.iter().any(|a| a == "--working-directory=/repo"),
            "{:?}",
            t.1
        );
        // And the command still arrives as argv, never joined into a string.
        assert!(t.1.ends_with(&cmd[..]), "{:?}", t.1);
    }

    /// Every candidate must pass the command as separate argv words. Joining
    /// them into one string is how a directory with a space in it becomes two
    /// arguments, and how a filename becomes a shell metacharacter.
    #[test]
    fn no_candidate_joins_the_command_into_a_string() {
        let cmd = vec![
            "tmux".to_string(),
            "new-session".to_string(),
            "-c".to_string(),
            "/a dir/with spaces".to_string(),
        ];
        for (program, args, _) in linux_terminal_attempts("/a dir/with spaces", &cmd) {
            assert!(
                args.ends_with(&cmd[..]),
                "{program} must end with the command as argv: {args:?}"
            );
            assert!(
                !args.iter().any(|a| a.contains("tmux new-session")),
                "{program} joined the command: {args:?}"
            );
        }
    }

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

    /// The Linux launch table must speak Linux — no `open`, no `osascript` —
    /// and every row must both carry the directory (flag or inherited cwd)
    /// and end with the command it was given, as argv, never joined.
    #[test]
    fn linux_launch_attempts_carry_dir_and_command_as_argv() {
        let cmd = vec!["claude".to_string()];
        let list = linux_terminal_attempts("/some/repo", &cmd);
        assert!(!list.is_empty());
        // The Debian alternatives symlink is the user's own choice; it leads.
        assert_eq!(list[0].0, "x-terminal-emulator");
        // *How* it carries the directory is now machine-dependent, because the
        // symlink is resolved to whatever it points at and the row follows that
        // program's flags. Asserting `cwd == Some(dir)` here pinned xterm's
        // behaviour and failed on a desktop where the alternative is
        // terminator. The loop below asserts the property that actually
        // matters — the directory arrives, by one route or the other.
        assert!(
            list[0].2.is_some() || list[0].1.iter().any(|a| a.contains("/some/repo")),
            "the chosen terminal must be given the directory somehow: {:?}",
            list[0]
        );
        for (program, args, cwd) in &list {
            assert_ne!(*program, "open", "`open -a` is macOS-speak");
            assert_ne!(*program, "osascript", "AppleScript is macOS-speak");
            let in_args = args.iter().any(|a| a.contains("/some/repo"));
            let in_cwd = cwd.as_deref() == Some("/some/repo");
            assert!(in_args || in_cwd, "{program} would launch without the directory");
            assert_eq!(
                &args[args.len() - cmd.len()..],
                cmd.as_slice(),
                "{program} must receive the command verbatim as trailing argv"
            );
            // The command is argv all the way down; nothing may re-parse it.
            assert!(
                !args.iter().any(|a| a == "sh" || a == "bash" || a == "-c"),
                "{program} would hand the command to a shell"
            );
        }
        for name in ["gnome-terminal", "konsole", "xfce4-terminal", "alacritty", "kitty", "xterm"] {
            assert!(list.iter().any(|(p, _, _)| p == name), "{name} missing from the table");
        }
    }

    /// ADR-0010: with tmux present, what launch starts must be hostable, not
    /// merely visible — so the in-terminal command is a tmux session, named
    /// safely (`:` and `.` are tmux target separators) and started in `dir`.
    #[test]
    fn launch_prefers_tmux_when_available() {
        let cmd = in_terminal_command("/home/me/my proj.v2", true, "0729-101500");
        assert_eq!(cmd[0], "tmux");
        assert_eq!(cmd[1], "new-session");
        let name = &cmd[cmd.iter().position(|a| a == "-s").unwrap() + 1];
        assert_eq!(name, "mogeung-my-proj-v2-0729-101500");
        let dir = &cmd[cmd.iter().position(|a| a == "-c").unwrap() + 1];
        assert_eq!(dir, "/home/me/my proj.v2", "the directory itself must stay verbatim");
        // The agent is named by absolute path where one can be found — see
        // `the_launch_command_names_claude_by_absolute_path` for why — so this
        // asserts the shape rather than the literal.
        let agent = &cmd[cmd.len() - 2];
        assert!(agent.ends_with("claude"), "{agent}");
        assert_eq!(cmd.last().unwrap(), SKIP_PERMISSIONS, "yolo mode, asked for");

        // Without tmux: the agent and its flag, and the same one either way.
        assert_eq!(
            in_terminal_command("/x", false, "s"),
            vec![agent.clone(), SKIP_PERMISSIONS.to_string()]
        );
    }

    /// The spec's words: on Wayland the answer is an honest refusal that says
    /// why and names the tools that cannot help — never a mute failure, and
    /// never a claimed success.
    #[test]
    fn wayland_is_an_honest_refusal() {
        let msg = wayland_refusal(Some("wayland")).expect("wayland must refuse");
        assert!(msg.contains("Wayland"));
        assert!(msg.contains("wmctrl") && msg.contains("xdotool"), "must name the tools");
        assert_eq!(wayland_refusal(Some("x11")), None);
        assert_eq!(wayland_refusal(None), None, "no session type is not proof of Wayland");
    }

    /// `wmctrl -lp` lines are `id desktop pid host title…`; pid 0 (a window
    /// that reports none) must parse but can never match a real chain, and a
    /// malformed line is skipped, not fatal.
    #[test]
    fn wmctrl_window_list_parses_and_junk_is_dropped() {
        let windows = parse_wmctrl_windows(
            "0x03c00003 -1 2233 host konsole — ~/proj\n\
             0x04000007  0 0 host no-pid window\n\
             0x05000001  2 4455 host tilix: two  spaced  title\n\
             garbage line\n",
        );
        assert_eq!(
            windows,
            vec![
                (2233, "0x03c00003".to_string()),
                (0, "0x04000007".to_string()),
                (4455, "0x05000001".to_string()),
            ]
        );
        assert_eq!(window_for(&[9999, 4455], &windows), Some("0x05000001".to_string()));
        assert_eq!(window_for(&[1], &windows), None);
    }

    /// The ancestor walk is closest-first — on a nested desktop the far end
    /// of the chain is the compositor, the wrong window to raise — and a
    /// cyclic parent table must terminate, not spin the daemon.
    #[test]
    fn ancestor_chain_is_closest_first_and_cycle_proof() {
        let mut parents = HashMap::new();
        parents.insert(100, 50); // claude → shell
        parents.insert(50, 20); //  shell → terminal (owns a window)
        parents.insert(20, 10); //  terminal → compositor (also owns one)
        parents.insert(10, 1);
        assert_eq!(ancestor_chain(100, &parents), vec![100, 50, 20, 10]);

        let windows = vec![(10, "0xcompositor".to_string()), (20, "0xterminal".to_string())];
        assert_eq!(
            window_for(&ancestor_chain(100, &parents), &windows),
            Some("0xterminal".to_string()),
            "the closest ancestor's window wins"
        );

        let mut cyclic = HashMap::new();
        cyclic.insert(3, 2);
        cyclic.insert(2, 3);
        assert_eq!(ancestor_chain(3, &cyclic), vec![3, 2]);
    }

    /// tmux client pids arrive one per line; anything unparseable is skipped.
    #[test]
    fn tmux_client_pid_lines_parse() {
        assert_eq!(parse_pid_lines("4210\n\n5311\nnotapid\n"), vec![4210, 5311]);
        assert!(parse_pid_lines("").is_empty());
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

/// Refuse a path a write verb must never be pointed at. `R-D19`.
///
/// Deliberately **lexical**, unlike [`resolve_inside`], which canonicalises.
/// Canonicalising cannot be the whole test here because the commonest target
/// of `discard` is a file that does not exist — restoring a deletion is the
/// point — and a check that has to fall back when the path is missing is a
/// check with a hole in it exactly where the destructive verb lives.
///
/// No legitimate worktree path is absolute or contains `..`, so refusing both
/// outright costs nothing real and needs no filesystem to be sure. git would
/// refuse most of these itself; this does not rely on that, because "git
/// happens to say no" is a property of git's argument parsing rather than a
/// guarantee we are entitled to.
fn refuse_escape(rel: &str) -> Result<()> {
    if rel.trim().is_empty() {
        anyhow::bail!("an empty path is not a file");
    }
    let path = Path::new(rel);
    if path.is_absolute() || rel.starts_with('/') || rel.starts_with('\\') {
        anyhow::bail!("{rel} is not a path inside the session");
    }
    // Both separators, because the check must not depend on which platform
    // wrote the string — a client is free to be on another one.
    if rel.split(['/', '\\']).any(|part| part == "..") {
        anyhow::bail!("{rel} is not a path inside the session");
    }
    Ok(())
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
mod path_tests {
    use super::*;

    /// A directory, plus a symlink pointing at it — the two spellings this
    /// exists to reconcile. macOS makes them for free (`/var` is a firmlink to
    /// `/private/var`, so every temp directory has both), and a person makes
    /// them on purpose to keep a checkout on another volume.
    fn linked(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("mogeung-path-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("real");
        std::fs::create_dir_all(real.join("src")).unwrap();
        std::fs::write(real.join("src/lib.rs"), "fn hi() {}\n").unwrap();
        let link = base.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        // The root as git answers it: resolved.
        (std::fs::canonicalize(&real).unwrap(), link)
    }

    #[test]
    fn strips_a_root_that_already_matches() {
        assert_eq!(relative_to_root("/w/repo", "/w/repo/src/lib.rs"), "src/lib.rs");
    }

    /// **The bug.** `repo_root` comes from git and is resolved; a transcript's
    /// `file_path` carries the session's own unresolved prefix. Left
    /// unreconciled, the strip fails, nothing matches the relative paths a diff
    /// carries, and the session reports no changes at all.
    #[test]
    #[cfg(unix)]
    fn strips_a_root_reached_through_a_symlink() {
        let (root, link) = linked("symlink");
        let touched = link.join("src/lib.rs");
        assert_eq!(
            relative_to_root(&root.to_string_lossy(), &touched.to_string_lossy()),
            "src/lib.rs"
        );
    }

    /// A **deleted** file is exactly what a diff is about, and plain
    /// `canonicalize` fails outright on one — so the resolution has to walk up
    /// to what still exists and put the tail back.
    #[test]
    #[cfg(unix)]
    fn resolves_a_file_that_is_no_longer_there() {
        let (root, link) = linked("deleted");
        let touched = link.join("src/gone.rs");
        assert!(!touched.exists());
        assert_eq!(
            relative_to_root(&root.to_string_lossy(), &touched.to_string_lossy()),
            "src/gone.rs"
        );
    }

    /// A path from another repository is not this session's, and must come back
    /// untouched rather than being mangled into something that might match.
    #[test]
    fn leaves_an_unrelated_path_alone() {
        assert_eq!(relative_to_root("/w/repo", "/elsewhere/other.rs"), "/elsewhere/other.rs");
    }

    /// Relative input stays relative: some transcripts write a bare path, and
    /// resolving one against the daemon's own cwd would invent an answer.
    #[test]
    fn leaves_a_relative_path_alone() {
        assert_eq!(relative_to_root("/w/repo", "src/lib.rs"), "src/lib.rs");
    }
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

#[cfg(test)]
mod write_containment_tests {
    use super::*;

    /// `discard` deletes files git has no copy of, so the path it is handed
    /// decides whether an untracked file in the worktree goes or something
    /// else does. This is the check standing between the two, and it runs
    /// before any verb sees a path.
    #[test]
    fn a_write_path_cannot_leave_the_session() {
        for bad in [
            "../secrets.env",
            "../../etc/passwd",
            "src/../../out",
            "/etc/passwd",
            "/",
            "..",
            // Backslashes, because the client need not be on this platform and
            // a check that only knows `/` is a check with a second spelling.
            "..\\windows\\system32",
            "src\\..\\..\\out",
            // Nothing at all: an empty pathspec is a wildcard to some git
            // verbs, which is the opposite of what an empty selection means.
            "",
            "   ",
        ] {
            assert!(
                refuse_escape(bad).is_err(),
                "{bad:?} must be refused before git sees it"
            );
        }
    }

    /// And the ordinary paths, including the ones that merely look alarming.
    /// A guard that refuses real filenames gets removed by whoever hits it.
    #[test]
    fn ordinary_paths_are_allowed_including_awkward_ones() {
        for good in [
            "src/lib.rs",
            "a file with spaces.txt",
            "café.txt",
            // Not `..` — a legitimate name that merely starts with dots.
            "...hidden",
            "..config",
            "src/..a/b.rs",
            // A leading dash is a pathspec problem, not a containment one, and
            // is handled by the `--` separator in `git.rs`.
            "--hard",
            "-i",
        ] {
            assert!(refuse_escape(good).is_ok(), "{good:?} is an ordinary path");
        }
    }
}
