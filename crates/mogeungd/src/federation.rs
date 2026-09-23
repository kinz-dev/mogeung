//! Following other machines' daemons. `R-I18`, `R-I21`–`R-I24`,
//! [feature 0047](../../../docs/features/0047-daemon-federation/spec.md).
//!
//! This daemon becomes a **client** of the daemons it follows: one watch
//! socket per peer, whose sessions are mirrored here under a namespaced id
//! (`mogeung_core::federation`) and ranked with this machine's own. The window
//! still holds one socket to one daemon and computes nothing — ADR-0013's
//! decision, kept by its 2026-09-23 amendment.
//!
//! # Kept apart from `AppState.sessions`, on purpose
//!
//! Nearly every pass over the local board walks *every* entry: the liveness
//! pass would mark a mirrored session dead, collisions would overwrite its
//! `collisions`, the store would persist it and reload it as local after a
//! restart, and retention would prune it. So mirrored sessions live here, in a
//! map of their own, and meet the local board only where it is *shown* — the
//! snapshot and the queue.
//!
//! # The fences
//!
//! - **Following is a loopback act**, and the follow list — tokens included —
//!   is `~/.mogeung/peers.json`, `0600`, never served (ADR-0036's amendment).
//! - **No re-export**: a socket from another machine never sees a mirrored
//!   session (`api::ws_conn`'s filter), so following is never transitive.
//! - **A peer gone quiet greys out**: after [`STALE_AFTER`] its sessions are
//!   marked stale and leave the ranking; they come back with its next
//!   snapshot.

use crate::state::AppState;
use futures_util::{SinkExt, StreamExt};
use mogeung_core::federation::{is_mirrored, mirrored_id, PROTOCOL};
use mogeung_core::session::{Session, SessionOrigin};
use mogeung_core::wire::{DaemonIdentity, PeerState, PeerStatus, ServerMsg};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

/// A peer silent this long has its sessions marked stale — four beacons'
/// worth, the same patience the Found list has (ADR-0040's amendment).
pub const STALE_AFTER: Duration = Duration::from_secs(60);
/// Between pings on a watch socket. A peer that has gone to sleep leaves a
/// TCP connection open and silent; a ping unanswered is how that is noticed.
const PING_EVERY: Duration = Duration::from_secs(15);
/// Silence on a watch socket, pings included, that counts as a lost peer.
const SILENCE: Duration = Duration::from_secs(45);
/// How long a dial may take before it counts as a failure.
const DIAL_TIMEOUT: Duration = Duration::from_secs(5);

/// One followed daemon, as it rests in `peers.json`. The token is here and
/// nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default)]
    pub allow_writes: bool,
}

/// What a relay needs to reach a peer.
#[derive(Debug, Clone)]
pub struct Route {
    pub url: String,
    pub token: Option<String>,
    pub allow_writes: bool,
    pub host: String,
    pub up: bool,
}

struct Peer {
    entry: PeerEntry,
    status: PeerStatus,
    detail: Option<String>,
    since_ms: i64,
    identity: Option<DaemonIdentity>,
    /// Mirrored, keyed by the namespaced id.
    sessions: HashMap<String, Session>,
    stale: bool,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Peer {
    fn new(entry: PeerEntry) -> Self {
        Peer {
            entry,
            status: PeerStatus::Connecting,
            detail: None,
            since_ms: chrono::Utc::now().timestamp_millis(),
            identity: None,
            sessions: HashMap::new(),
            stale: false,
            task: None,
        }
    }

    fn view(&self) -> PeerState {
        PeerState {
            id: self.entry.id.clone(),
            name: self.entry.name.clone(),
            url: without_token(&self.entry.url),
            state: self.status,
            machine_id: self.identity.as_ref().and_then(|i| i.machine_id.clone()),
            host: self.identity.as_ref().and_then(|i| i.host.clone()),
            version: self.identity.as_ref().map(|i| i.version.clone()),
            sessions: self.sessions.len() as u32,
            allow_writes: self.entry.allow_writes,
            detail: self.detail.clone(),
            since_ms: self.since_ms,
        }
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
}

/// Every daemon this one follows. Lives on `AppState`.
#[derive(Default)]
pub struct Federation {
    /// `peers.json`, overridable so a test never touches the real one — the
    /// lesson `notes_dir` learnt the hard way.
    pub path: OnceLock<PathBuf>,
    /// How long a peer may be silent before its sessions go stale;
    /// [`STALE_AFTER`] unless a test says otherwise.
    pub stale_after: OnceLock<Duration>,
    inner: Mutex<BTreeMap<String, Peer>>,
}

/// `~/.mogeung/peers.json`.
pub fn default_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mogeung").join("peers.json")
}

/// The follow list on disk. A missing or unreadable file is an empty list —
/// a follow list that cannot be read must not stop the daemon.
pub fn load_from(path: &Path) -> Vec<PeerEntry> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Written whole, atomically, `0600` — it holds tokens (ADR-0036).
pub fn save_to(path: &Path, list: &[PeerEntry]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
    restrict(&tmp)?;
    std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
}

#[cfg(unix)]
fn restrict(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("could not restrict {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// The address with any `token=` taken out of its query, for showing.
pub fn without_token(url: &str) -> String {
    match url.split_once('?') {
        None => url.to_string(),
        Some((base, query)) => {
            let kept: Vec<&str> = query.split('&').filter(|kv| !kv.starts_with("token=")).collect();
            if kept.is_empty() {
                base.to_string()
            } else {
                format!("{base}?{}", kept.join("&"))
            }
        }
    }
}

/// A peer's session, as this daemon shows it: namespaced by its machine,
/// with every id inside it namespaced too, and its origin said.
pub fn mirror(identity: &DaemonIdentity, machine: &str, session: &Session) -> Option<Session> {
    // A peer never sends what it mirrors (no re-export) — and if one ever
    // did, following it would loop.
    if session.origin.is_some() || is_mirrored(&session.id) {
        return None;
    }
    let value = serde_json::to_value(session).ok()?;
    let value = mogeung_core::federation::inbound(value, machine);
    let mut s: Session = serde_json::from_value(value).ok()?;
    s.id = mirrored_id(machine, &session.id);
    for c in &mut s.collisions {
        if !is_mirrored(&c.other) {
            c.other = mirrored_id(machine, &c.other);
        }
    }
    s.origin = Some(SessionOrigin {
        machine_id: machine.to_string(),
        host: identity.host.clone(),
        ssh_target: identity.ssh_target.clone(),
        stale: false,
    });
    Some(s)
}

impl Federation {
    fn path(&self) -> PathBuf {
        self.path.get().cloned().unwrap_or_else(default_path)
    }

    fn save(&self, inner: &BTreeMap<String, Peer>) -> Result<(), String> {
        let list: Vec<PeerEntry> = inner.values().map(|p| p.entry.clone()).collect();
        save_to(&self.path(), &list)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Peer>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every mirrored session, from every peer.
    pub fn sessions(&self) -> Vec<Session> {
        self.lock().values().flat_map(|p| p.sessions.values().cloned()).collect()
    }

    /// One mirrored session.
    pub fn get(&self, id: &str) -> Option<Session> {
        self.lock().values().find_map(|p| p.sessions.get(id).cloned())
    }

    pub fn peers(&self) -> Vec<PeerState> {
        self.lock().values().map(Peer::view).collect()
    }

    /// How to reach the peer a mirrored id belongs to.
    pub fn route(&self, machine: &str) -> Option<Route> {
        let g = self.lock();
        let p = g
            .values()
            .find(|p| p.identity.as_ref().and_then(|i| i.machine_id.as_deref()) == Some(machine))?;
        Some(Route {
            url: p.entry.url.clone(),
            token: p.entry.token.clone(),
            allow_writes: p.entry.allow_writes,
            host: p
                .identity
                .as_ref()
                .and_then(|i| i.host.clone())
                .unwrap_or_else(|| p.entry.name.clone()),
            up: p.status == PeerStatus::Up,
        })
    }

    /// Start watching everything in `peers.json`. Called once, by the server.
    pub fn start(&self, state: &Arc<AppState>) {
        let entries = load_from(&self.path());
        let mut g = self.lock();
        for entry in entries {
            let id = entry.id.clone();
            let mut peer = Peer::new(entry);
            peer.task = Some(spawn_watch(Arc::downgrade(state), id.clone()));
            g.insert(id, peer);
        }
    }

    /// Follow, or change what is followed under this id.
    pub async fn follow(&self, state: &Arc<AppState>, entry: PeerEntry) -> Result<(), String> {
        let url = entry.url.trim();
        if !(url.starts_with("ws://") || url.starts_with("wss://")) {
            return Err(format!("{url} is not a ws:// address"));
        }
        let removed = {
            let mut g = self.lock();
            let removed: Vec<String> = g.remove(&entry.id).map(|p| p.sessions.keys().cloned().collect()).unwrap_or_default();
            let id = entry.id.clone();
            let mut peer = Peer::new(entry);
            peer.task = Some(spawn_watch(Arc::downgrade(state), id.clone()));
            g.insert(id, peer);
            self.save(&g)?;
            removed
        };
        for id in removed {
            state.broadcast(ServerMsg::SessionRemoved { session_id: id });
        }
        state.publish_queue().await;
        self.announce(state);
        Ok(())
    }

    pub async fn unfollow(&self, state: &Arc<AppState>, id: &str) -> Result<(), String> {
        let removed = {
            let mut g = self.lock();
            let Some(peer) = g.remove(id) else {
                return Err("that daemon is not followed".into());
            };
            self.save(&g)?;
            peer.sessions.keys().cloned().collect::<Vec<_>>()
        };
        for sid in removed {
            state.broadcast(ServerMsg::SessionRemoved { session_id: sid });
        }
        state.publish_queue().await;
        self.announce(state);
        Ok(())
    }

    pub fn set_writes(&self, state: &Arc<AppState>, id: &str, allow: bool) -> Result<(), String> {
        {
            let mut g = self.lock();
            let Some(peer) = g.get_mut(id) else {
                return Err("that daemon is not followed".into());
            };
            peer.entry.allow_writes = allow;
            self.save(&g)?;
        }
        self.announce(state);
        Ok(())
    }

    /// Tell this machine's clients how the follows stand. Filtered away from
    /// any other machine's socket, like everything else about federation.
    pub fn announce(&self, state: &AppState) {
        state.broadcast(ServerMsg::Peers { peers: self.peers() });
    }

    fn set_status(&self, id: &str, status: PeerStatus, detail: Option<String>) -> bool {
        let mut g = self.lock();
        let Some(p) = g.get_mut(id) else { return false };
        if p.status == status && p.detail == detail {
            return true;
        }
        p.status = status;
        p.detail = detail;
        p.since_ms = chrono::Utc::now().timestamp_millis();
        true
    }

    fn entry(&self, id: &str) -> Option<PeerEntry> {
        self.lock().get(id).map(|p| p.entry.clone())
    }

    /// Replace a peer's mirror with its snapshot. Returns what changed, as
    /// the messages that say so.
    fn fold_snapshot(&self, id: &str, identity: &DaemonIdentity, sessions: &[Session]) -> Vec<ServerMsg> {
        let machine = identity.machine_id.clone().unwrap_or_default();
        let fresh: HashMap<String, Session> = sessions
            .iter()
            .filter_map(|s| mirror(identity, &machine, s))
            .map(|s| (s.id.clone(), s))
            .collect();
        let mut g = self.lock();
        let Some(p) = g.get_mut(id) else { return vec![] };
        let mut out = Vec::new();
        for (sid, s) in &fresh {
            if p.sessions.get(sid) != Some(s) {
                out.push(ServerMsg::SessionUpdated { session: Box::new(s.clone()) });
            }
        }
        for sid in p.sessions.keys() {
            if !fresh.contains_key(sid) {
                out.push(ServerMsg::SessionRemoved { session_id: sid.clone() });
            }
        }
        p.sessions = fresh;
        p.identity = Some(identity.clone());
        p.stale = false;
        out
    }

    fn fold_update(&self, id: &str, session: &Session) -> Option<ServerMsg> {
        let mut g = self.lock();
        let p = g.get_mut(id)?;
        let identity = p.identity.clone()?;
        let machine = identity.machine_id.clone()?;
        let s = mirror(&identity, &machine, session)?;
        if p.sessions.get(&s.id) == Some(&s) {
            return None;
        }
        p.sessions.insert(s.id.clone(), s.clone());
        Some(ServerMsg::SessionUpdated { session: Box::new(s) })
    }

    fn fold_removed(&self, id: &str, orig: &str) -> Option<ServerMsg> {
        let mut g = self.lock();
        let p = g.get_mut(id)?;
        let machine = p.identity.as_ref()?.machine_id.clone()?;
        let sid = mirrored_id(&machine, orig);
        p.sessions.remove(&sid)?;
        Some(ServerMsg::SessionRemoved { session_id: sid })
    }

    /// Mark a quiet peer's sessions stale, once.
    fn go_stale(&self, id: &str) -> Vec<ServerMsg> {
        let mut g = self.lock();
        let Some(p) = g.get_mut(id) else { return vec![] };
        if p.stale {
            return vec![];
        }
        p.stale = true;
        p.sessions
            .values_mut()
            .filter_map(|s| {
                let o = s.origin.as_mut()?;
                o.stale = true;
                Some(ServerMsg::SessionUpdated { session: Box::new(s.clone()) })
            })
            .collect()
    }
}

/// Just enough of a message to know what it is before paying for the rest.
#[derive(Deserialize)]
struct Tag {
    ev: String,
}

fn spawn_watch(state: Weak<AppState>, id: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        let mut quiet_since = Instant::now();
        loop {
            let Some(st) = state.upgrade() else { return };
            let Some(entry) = st.federation.entry(&id) else { return };
            let local = st.identity.machine_id.clone();
            st.federation.set_status(&id, PeerStatus::Connecting, None);
            st.federation.announce(&st);
            drop(st);

            let outcome = watch_once(&state, &id, &entry, local.as_deref()).await;
            let Some(st) = state.upgrade() else { return };
            match outcome {
                Watch::Stop(status, detail) => {
                    // Not worth redialling soon: this machine, or a build
                    // that cannot be followed. A slow retry catches an upgrade.
                    st.federation.set_status(&id, status, Some(detail));
                    st.federation.announce(&st);
                    backoff = Duration::from_secs(60);
                }
                Watch::Lost(status, detail) => {
                    st.federation.set_status(&id, status, Some(detail));
                    st.federation.announce(&st);
                }
                Watch::Served => {
                    // It was up; the silence starts now, and the backoff over.
                    quiet_since = Instant::now();
                    backoff = Duration::from_secs(1);
                    st.federation.set_status(&id, PeerStatus::Down, Some("the connection closed".into()));
                    st.federation.announce(&st);
                }
            }
            if quiet_since.elapsed() >= *st.federation.stale_after.get().unwrap_or(&STALE_AFTER) {
                let changed = st.federation.go_stale(&id);
                if !changed.is_empty() {
                    for m in changed {
                        st.broadcast(m);
                    }
                    st.publish_queue().await;
                }
            }
            drop(st);
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    })
}

enum Watch {
    /// Connected, mirrored, and then the connection ended.
    Served,
    /// Could not connect, or was turned away. Retry.
    Lost(PeerStatus, String),
    /// Connected to something that should not be followed. Retry slowly.
    Stop(PeerStatus, String),
}

/// A request with the token as a header, so it is never in a URL or a log.
pub fn request(
    url: &str,
    token: Option<&str>,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, String> {
    let mut req = url
        .into_client_request()
        .map_err(|e| format!("{} is not an address this can dial: {e}", without_token(url)))?;
    if let Some(t) = token.filter(|t| !t.is_empty()) {
        let v = format!("Bearer {t}")
            .parse()
            .map_err(|_| "the token cannot be sent as a header".to_string())?;
        req.headers_mut().insert(tokio_tungstenite::tungstenite::http::header::AUTHORIZATION, v);
    }
    Ok(req)
}

/// Why a dial failed, in the words the panel shows.
pub fn dial_failure(e: &tokio_tungstenite::tungstenite::Error) -> (PeerStatus, String) {
    use tokio_tungstenite::tungstenite::Error;
    match e {
        Error::Http(resp) if resp.status().as_u16() == 401 => {
            (PeerStatus::Refused, "answering, and the token was turned away".into())
        }
        Error::Http(resp) => (PeerStatus::Down, format!("answered {}, which is not a mogeung daemon", resp.status())),
        Error::Io(io) => (PeerStatus::Down, format!("nothing answered — {io}")),
        Error::Url(u) => (PeerStatus::Down, format!("not an address this can dial — {u}")),
        other => (PeerStatus::Down, other.to_string()),
    }
}

async fn watch_once(state: &Weak<AppState>, id: &str, entry: &PeerEntry, local: Option<&str>) -> Watch {
    let req = match request(&entry.url, entry.token.as_deref()) {
        Ok(r) => r,
        Err(e) => return Watch::Stop(PeerStatus::Down, e),
    };
    if entry.url.starts_with("wss://") {
        return Watch::Stop(
            PeerStatus::Incompatible,
            "wss:// is not followed yet — reach it over ws:// on the LAN, or an ssh tunnel".into(),
        );
    }
    let ws = match tokio::time::timeout(DIAL_TIMEOUT, tokio_tungstenite::connect_async(req)).await {
        Err(_) => return Watch::Lost(PeerStatus::Down, "no answer in 5 s".into()),
        Ok(Err(e)) => {
            let (s, d) = dial_failure(&e);
            return Watch::Lost(s, d);
        }
        Ok(Ok((ws, _))) => ws,
    };
    let (mut sink, mut stream) = ws.split();
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.tick().await;
    let mut last_heard = Instant::now();
    let mut served = false;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if last_heard.elapsed() > SILENCE {
                    return if served { Watch::Served } else { Watch::Lost(PeerStatus::Down, "went silent".into()) };
                }
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return if served { Watch::Served } else { Watch::Lost(PeerStatus::Down, "the connection closed".into()) };
                }
            }
            msg = stream.next() => {
                let Some(Ok(msg)) = msg else {
                    return if served { Watch::Served } else { Watch::Lost(PeerStatus::Down, "the connection closed".into()) };
                };
                last_heard = Instant::now();
                let Message::Text(txt) = msg else { continue };
                let Ok(tag) = serde_json::from_str::<Tag>(&txt) else { continue };
                let Some(st) = state.upgrade() else { return Watch::Served };
                match tag.ev.as_str() {
                    "snapshot" => {
                        let Ok(ServerMsg::Snapshot { sessions, daemon, .. }) = serde_json::from_str::<ServerMsg>(&txt) else {
                            continue;
                        };
                        let Some(identity) = daemon else {
                            return Watch::Stop(PeerStatus::Incompatible, "it does not say who it is — a build from before R-I5".into());
                        };
                        if identity.machine_id.is_some() && identity.machine_id.as_deref() == local {
                            return Watch::Stop(PeerStatus::Myself, "this is this machine's own daemon".into());
                        }
                        if identity.machine_id.is_none() {
                            return Watch::Stop(PeerStatus::Incompatible, "it publishes no machine id".into());
                        }
                        if identity.federation.unwrap_or(0) < PROTOCOL {
                            return Watch::Stop(
                                PeerStatus::Incompatible,
                                format!(
                                    "version {} predates federation — it needs a build that speaks protocol {PROTOCOL}",
                                    identity.version
                                ),
                            );
                        }
                        let changed = st.federation.fold_snapshot(id, &identity, &sessions);
                        served = true;
                        st.federation.set_status(id, PeerStatus::Up, None);
                        for m in changed {
                            st.broadcast(m);
                        }
                        st.publish_queue().await;
                        st.federation.announce(&st);
                    }
                    "session_updated" => {
                        let Ok(ServerMsg::SessionUpdated { session }) = serde_json::from_str::<ServerMsg>(&txt) else {
                            continue;
                        };
                        if let Some(m) = st.federation.fold_update(id, &session) {
                            st.broadcast(m);
                            st.publish_queue().await;
                        }
                    }
                    "session_removed" => {
                        let Ok(ServerMsg::SessionRemoved { session_id }) = serde_json::from_str::<ServerMsg>(&txt) else {
                            continue;
                        };
                        if let Some(m) = st.federation.fold_removed(id, &session_id) {
                            st.broadcast(m);
                            st.publish_queue().await;
                        }
                    }
                    // Everything else a peer broadcasts — its git answers, its
                    // health, its events — is for relays, not the board.
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::session::Collision;

    fn identity(machine: &str) -> DaemonIdentity {
        DaemonIdentity {
            machine_id: Some(machine.into()),
            host: Some("mac".into()),
            ssh_target: Some("kinz@mac".into()),
            federation: Some(PROTOCOL),
            ..Default::default()
        }
    }

    #[test]
    fn a_mirrored_session_carries_its_machine_in_every_id_and_says_where_it_is() {
        let mut s = crate::state::blank_session_with_id("abc");
        s.collisions.push(Collision { other: "def".into(), other_label: "other".into(), path: "a.rs".into() });
        let m = mirror(&identity("m1"), "m1", &s).unwrap();
        assert_eq!(m.id, "@m1:abc");
        assert_eq!(m.collisions[0].other, "@m1:def");
        let o = m.origin.unwrap();
        assert_eq!((o.machine_id.as_str(), o.host.as_deref(), o.ssh_target.as_deref(), o.stale), ("m1", Some("mac"), Some("kinz@mac"), false));
    }

    #[test]
    fn what_a_peer_mirrors_itself_is_never_mirrored_again() {
        let mut s = crate::state::blank_session_with_id("abc");
        s.origin = Some(SessionOrigin { machine_id: "m2".into(), ..Default::default() });
        assert!(mirror(&identity("m1"), "m1", &s).is_none());
        let t = crate::state::blank_session_with_id("@m2:abc");
        assert!(mirror(&identity("m1"), "m1", &t).is_none());
    }

    #[test]
    fn a_token_never_leaves_in_an_address() {
        assert_eq!(without_token("ws://h:7717/ws?token=s3cret"), "ws://h:7717/ws");
        assert_eq!(without_token("ws://h:7717/ws?a=1&token=s3cret&b=2"), "ws://h:7717/ws?a=1&b=2");
        assert_eq!(without_token("ws://h:7717/ws"), "ws://h:7717/ws");
    }

    #[test]
    fn the_follow_list_is_written_whole_and_readable_by_its_owner_only() {
        let dir = std::env::temp_dir().join(format!("mogeung-peers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("peers.json");
        let list = vec![PeerEntry {
            id: "c1".into(),
            name: "mac".into(),
            url: "ws://10.0.0.39:7717/ws".into(),
            token: Some("s3cret".into()),
            allow_writes: false,
        }];
        save_to(&path, &list).unwrap();
        assert_eq!(load_from(&path), list);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // A file that cannot be read is an empty list, never a panic.
        std::fs::write(&path, "not json").unwrap();
        assert!(load_from(&path).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_token_rides_a_header() {
        let req = request("ws://h:7717/ws", Some("s3cret")).unwrap();
        assert_eq!(req.headers().get("authorization").unwrap(), "Bearer s3cret");
        assert!(!req.uri().to_string().contains("s3cret"));
        assert!(request("ws://h:7717/ws", None).unwrap().headers().get("authorization").is_none());
    }
}

/// Commands about mirrored sessions, carried to the daemons that own them.
/// `R-I22`. One per window.
///
/// **One relay socket per window and peer.** The wire has no request ids: a
/// daemon answers on the socket that asked, or broadcasts. So a forwarded
/// command is sent up a socket that belongs to this window alone, and what
/// comes back down it is this window's to receive — correlation by socket
/// rather than by table. Opened on the first command for that peer, closed
/// with the window.
pub struct Relays {
    state: Arc<AppState>,
    reply: tokio::sync::mpsc::Sender<ServerMsg>,
    open: HashMap<String, RelayConn>,
}

struct RelayConn {
    tx: tokio::sync::mpsc::UnboundedSender<String>,
    task: tokio::task::JoinHandle<()>,
    /// The follow as it stood when this relay opened. A follow sent again
    /// with a corrected token, or its consent turned on or off, reopens the
    /// relay on the next command rather than carrying on with the old terms.
    terms: (String, Option<String>, bool),
}

impl Drop for Relays {
    fn drop(&mut self) {
        for (_, c) in self.open.drain() {
            c.task.abort();
        }
    }
}

impl Relays {
    pub fn new(state: Arc<AppState>, reply: tokio::sync::mpsc::Sender<ServerMsg>) -> Self {
        Relays { state, reply, open: HashMap::new() }
    }

    fn say(&self, message: String) {
        let _ = self.reply.try_send(ServerMsg::Error { message });
    }

    /// Take a command if it is about a mirrored session: forward it, or refuse
    /// it in words. `false` means it is this daemon's to handle.
    pub fn intercept(&mut self, cmd: &mogeung_core::ClientMsg, from_this_machine: bool) -> bool {
        use mogeung_core::federation::{hop, outbound, split_mirrored, target, Hop};
        let Some(t) = target(cmd) else { return false };
        let Some((machine, _)) = split_mirrored(&t) else { return false };
        let machine = machine.to_string();
        let h = hop(cmd);
        // A note about a remote session is still this machine's note.
        if h == Hop::Here {
            return false;
        }
        if !from_this_machine {
            self.say("sessions mirrored from other machines are shown to this machine's own windows only".into());
            return true;
        }
        let Some(route) = self.state.federation.route(&machine) else {
            self.say("that session's machine is no longer followed".into());
            return true;
        };
        match h {
            Hop::Desktop => {
                self.say(format!(
                    "that session is on {} — this acts on that machine's screen, where nobody is looking. Do it there.",
                    route.host
                ));
                return true;
            }
            Hop::Write if !route.allow_writes => {
                self.say(format!(
                    "changes on {} are off for this follow — turn on “allow changes” for it in Connections (R-I23)",
                    route.host
                ));
                return true;
            }
            _ => {}
        }
        if !route.up {
            self.say(format!("{} is not answering right now", route.host));
            return true;
        }
        let Some((_, value)) = outbound(cmd) else { return true };
        let terms = (route.url.clone(), route.token.clone(), route.allow_writes);
        let alive = self.open.get(&machine).is_some_and(|c| !c.task.is_finished() && c.terms == terms);
        if !alive {
            if let Some(old) = self.open.remove(&machine) {
                old.task.abort();
            }
            let conn = open_relay(route, machine.clone(), self.reply.clone());
            self.open.insert(machine.clone(), conn);
        }
        if self.open.get(&machine).is_none_or(|c| c.tx.send(value.to_string()).is_err()) {
            self.say("the relay to that machine closed — try again".into());
        }
        true
    }
}

fn open_relay(route: Route, machine: String, reply: tokio::sync::mpsc::Sender<ServerMsg>) -> RelayConn {
    let terms = (route.url.clone(), route.token.clone(), route.allow_writes);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let task = tokio::spawn(async move {
        let say = |message: String| {
            let _ = reply.try_send(ServerMsg::Error { message });
        };
        let req = match request(&route.url, route.token.as_deref()) {
            Ok(r) => r,
            Err(e) => return say(e),
        };
        let ws = match tokio::time::timeout(DIAL_TIMEOUT, tokio_tungstenite::connect_async(req)).await {
            Err(_) => return say(format!("{} did not answer in 5 s", route.host)),
            Ok(Err(e)) => return say(format!("{}: {}", route.host, dial_failure(&e).1)),
            Ok(Ok((ws, _))) => ws,
        };
        let (mut sink, mut stream) = ws.split();
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(text) = cmd else { break };
                    if sink.send(Message::Text(text.into())).await.is_err() {
                        say(format!("the relay to {} closed", route.host));
                        break;
                    }
                }
                msg = stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(txt))) => {
                            if let Some(m) = relayed(&txt, &machine, route.allow_writes) {
                                let _ = reply.try_send(m);
                            }
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                    }
                }
            }
        }
    });
    RelayConn { tx, task, terms }
}

/// One message from a relay, as the window should receive it — or nothing.
/// A message this build cannot read (a newer peer's) is dropped rather than
/// guessed at.
///
/// A run panel's *will it run* is the one answer rewritten on the way: the
/// peer says whether **it** would run for a socket from another machine (its
/// `--allow-run`), and a run crossing this hop also needs this follow's
/// consent (`R-I23`) — so the window is shown both, as one flag.
pub fn relayed(txt: &str, machine: &str, allow_writes: bool) -> Option<ServerMsg> {
    use mogeung_core::federation::{inbound, relay_class, Relay};
    let msg: ServerMsg = serde_json::from_str(txt).ok()?;
    if relay_class(&msg) != Relay::Pass {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(txt).ok()?;
    let mut out: ServerMsg = serde_json::from_value(inbound(value, machine)).ok()?;
    if let ServerMsg::RunConfigs { allowed, .. } = &mut out {
        *allowed = *allowed && allow_writes;
    }
    Some(out)
}

#[cfg(test)]
mod relay_tests {
    use super::relayed;
    use mogeung_core::ServerMsg;

    #[test]
    fn a_relay_passes_an_answer_with_the_machine_in_its_ids() {
        let txt = r#"{"ev":"file_content","session_id":"abc","path":"a.rs","content":"x","truncated":false}"#;
        match relayed(txt, "m1", false) {
            Some(ServerMsg::FileContent { session_id, path, .. }) => assert_eq!((session_id.as_str(), path.as_str()), ("@m1:abc", "a.rs")),
            other => panic!("{other:?}"),
        }
    }

    /// Both gates, as one flag: the peer's `--allow-run` and this follow's
    /// consent. Either off, and the window's run button says so before the
    /// click rather than after it.
    #[test]
    fn a_relayed_run_panel_is_runnable_only_with_both_gates_open() {
        let flag = |peer: bool, consent: bool| {
            let txt = format!(r#"{{"ev":"run_configs","session_id":"abc","configs":[],"allowed":{peer}}}"#);
            match relayed(&txt, "m1", consent) {
                Some(ServerMsg::RunConfigs { session_id, allowed, .. }) => {
                    assert_eq!(session_id, "@m1:abc");
                    allowed
                }
                other => panic!("{other:?}"),
            }
        };
        assert!(flag(true, true));
        assert!(!flag(true, false));
        assert!(!flag(false, true));
    }

    #[test]
    fn a_relay_drops_the_board_what_is_theirs_and_what_it_cannot_read() {
        assert!(relayed(r#"{"ev":"queue","queue":[]}"#, "m1", false).is_none());
        assert!(relayed(r#"{"ev":"notes","notes":[]}"#, "m1", false).is_none());
        assert!(relayed(r#"{"ev":"runs","runs":[]}"#, "m1", false).is_none());
        assert!(relayed(r#"{"ev":"something_from_a_newer_build"}"#, "m1", false).is_none());
        assert!(relayed("not json", "m1", false).is_none());
    }
}
