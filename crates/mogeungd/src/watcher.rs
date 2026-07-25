//! Discovers and follows Claude Code sessions.
//!
//! Two sources, correlated by session id:
//!
//! * `~/.claude/sessions/<pid>.json` — the live registry. Gives authoritative
//!   `busy`/`idle` status, the cwd, and a friendly name. A session is alive iff
//!   its pid is still running.
//! * `~/.claude/projects/<slug>/<session-id>.jsonl` — the transcript. Tailed
//!   incrementally by byte offset.
//!
//! Polling rather than filesystem events: a few dozen files every couple of
//! seconds is nothing, and it avoids every rename/atomic-write edge case that
//! makes inotify/FSEvents miserable.

use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use mogeung_core::session::LiveStatus;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Where Claude Code keeps its state, unless told otherwise.
///
/// Callers pass the resolved path around explicitly rather than reading the
/// environment deep in the call stack, so tests can point the watcher at a
/// synthetic home without touching process-global state.
pub fn default_home() -> PathBuf {
    if let Ok(p) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".claude")
}

/// One entry of the live-session registry.
#[derive(Debug, Clone)]
pub struct LiveEntry {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub status: LiveStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub status_since: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct RawLive {
    pid: u32,
    #[serde(rename = "sessionId")]
    session_id: String,
    cwd: String,
    name: Option<String>,
    version: Option<String>,
    status: Option<String>,
    #[serde(rename = "startedAt")]
    started_at: Option<i64>,
    #[serde(rename = "statusUpdatedAt")]
    status_updated_at: Option<i64>,
    /// "interactive" for a real terminal session.
    #[allow(dead_code)]
    kind: Option<String>,
}

fn ms_to_dt(ms: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single()
}

/// Is this process still running?
///
/// The registry files are not cleaned up on exit, so liveness has to be checked
/// against the OS or every session that ever ran would look alive.
fn pid_alive(pid: u32) -> bool {
    // Signal 0 performs error checking without actually sending a signal.
    unsafe { libc_kill(pid as i32, 0) == 0 }
}

// Avoid a `libc` dependency for one call.
extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

pub fn scan_live(home: &Path) -> Vec<LiveEntry> {
    let dir = home.join("sessions");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(raw) = serde_json::from_str::<RawLive>(&text) else {
            continue;
        };
        if !pid_alive(raw.pid) {
            continue;
        }
        out.push(LiveEntry {
            pid: raw.pid,
            session_id: raw.session_id,
            cwd: raw.cwd,
            name: raw.name,
            version: raw.version,
            status: raw
                .status
                .as_deref()
                .map(LiveStatus::parse)
                .unwrap_or(LiveStatus::Unknown),
            started_at: raw.started_at.and_then(ms_to_dt),
            status_since: raw.status_updated_at.and_then(ms_to_dt),
        });
    }
    out
}

/// A transcript file on disk.
#[derive(Debug, Clone)]
pub struct TranscriptFile {
    pub session_id: String,
    pub path: PathBuf,
    pub modified: DateTime<Utc>,
    pub size: u64,
}

/// Find every session transcript, newest first.
pub fn scan_transcripts(home: &Path, max_age_days: i64) -> Vec<TranscriptFile> {
    let root = home.join("projects");
    let mut out = Vec::new();
    let cutoff = Utc::now() - chrono::Duration::days(max_age_days);

    let Ok(projects) = std::fs::read_dir(&root) else {
        return out;
    };
    for project in projects.flatten() {
        let Ok(files) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(meta) = f.metadata() else { continue };
            let modified: DateTime<Utc> = meta
                .modified()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now());
            if modified < cutoff {
                continue;
            }
            let Some(session_id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            out.push(TranscriptFile {
                session_id: session_id.to_string(),
                path: path.clone(),
                modified,
                size: meta.len(),
            });
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Tracks how far we have read into each transcript.
#[derive(Default)]
pub struct Tailer {
    offsets: HashMap<PathBuf, u64>,
}

impl Tailer {
    /// Read whatever has been appended since last time.
    ///
    /// If the file shrank it was rewritten or rotated, so we start over rather
    /// than reading from a meaningless offset.
    pub fn read_new(&mut self, path: &Path) -> Result<Vec<String>> {
        let meta = std::fs::metadata(path)?;
        let size = meta.len();
        let offset = self.offsets.entry(path.to_path_buf()).or_insert(0);
        if size < *offset {
            *offset = 0;
        }
        if size == *offset {
            return Ok(Vec::new());
        }

        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(*offset))?;
        let reader = BufReader::new(file);

        let mut lines = Vec::new();
        let mut consumed = *offset;
        for line in reader.lines() {
            let line = line?;
            // +1 for the newline the reader stripped.
            consumed += line.len() as u64 + 1;
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
        // Never advance past the size we observed, so a line being written
        // concurrently is re-read next tick rather than half-parsed.
        *offset = consumed.min(size);
        Ok(lines)
    }

    /// Start following a file from its end, skipping existing content.
    pub fn skip_to_end(&mut self, path: &Path) {
        if let Ok(meta) = std::fs::metadata(path) {
            self.offsets.insert(path.to_path_buf(), meta.len());
        }
    }

    pub fn forget(&mut self, path: &Path) {
        self.offsets.remove(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tailer_returns_only_new_lines() {
        let dir = std::env::temp_dir().join(format!("mogeung-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        std::fs::write(&path, "a\nb\n").unwrap();

        let mut t = Tailer::default();
        assert_eq!(t.read_new(&path).unwrap(), vec!["a", "b"]);
        // Nothing new yet.
        assert!(t.read_new(&path).unwrap().is_empty());

        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(f, "c").unwrap();
        assert_eq!(t.read_new(&path).unwrap(), vec!["c"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_truncated_file_is_reread_from_the_start() {
        let dir = std::env::temp_dir().join(format!("mogeung-trunc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.jsonl");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();

        let mut t = Tailer::default();
        assert_eq!(t.read_new(&path).unwrap().len(), 3);

        std::fs::write(&path, "x\n").unwrap();
        assert_eq!(t.read_new(&path).unwrap(), vec!["x"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_current_process_counts_as_alive() {
        assert!(pid_alive(std::process::id()));
        // Very unlikely to exist, and definitely not ours.
        assert!(!pid_alive(999_999));
    }
}
