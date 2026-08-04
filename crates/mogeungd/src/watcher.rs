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
///
/// The offsets are process state, but they must not be *only* process state:
/// see `R-A6` and `Store::load_tail_offsets`. A tailer that starts empty over
/// a database that remembers the sessions re-reads every transcript whole.
#[derive(Default)]
pub struct Tailer {
    offsets: HashMap<PathBuf, u64>,
}

impl Tailer {
    /// Restore a previously recorded read position.
    pub fn seed(&mut self, path: &Path, offset: u64) {
        self.offsets.insert(path.to_path_buf(), offset);
    }

    /// Where reading would resume, if this file is being followed at all.
    pub fn offset(&self, path: &Path) -> Option<u64> {
        self.offsets.get(path).copied()
    }

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

    /// Begin following a large file near its end, keeping roughly the last
    /// `keep_bytes`. Returns how many bytes of history were skipped.
    ///
    /// The corpus on the author's machine has an 11 MB transcript, and reading
    /// one of those whole — parsing every line, emitting every event — on first
    /// sight blocks the scan loop for no benefit: nobody reviews the first
    /// thousand turns of a fortnight-old session.
    ///
    /// Starts at the first line boundary at or after the cut, so the first line
    /// read is never a fragment. A file with no newline after the cut is
    /// followed from its end rather than re-read whole.
    pub fn start_near_end(&mut self, path: &Path, keep_bytes: u64) -> u64 {
        let Ok(meta) = std::fs::metadata(path) else {
            return 0;
        };
        let size = meta.len();
        if size <= keep_bytes {
            self.offsets.insert(path.to_path_buf(), 0);
            return 0;
        }
        let cut = size - keep_bytes;

        let start = match std::fs::File::open(path) {
            Ok(mut f) => {
                if f.seek(SeekFrom::Start(cut)).is_err() {
                    size
                } else {
                    // Consume the remainder of the partial line at `cut`.
                    let mut reader = BufReader::new(&mut f);
                    let mut discard = Vec::new();
                    match reader.read_until(b'\n', &mut discard) {
                        Ok(0) => size,
                        Ok(n) => cut + n as u64,
                        Err(_) => size,
                    }
                }
            }
            Err(_) => size,
        };

        let start = start.min(size);
        self.offsets.insert(path.to_path_buf(), start);
        start
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

    /// R-A5. A huge transcript must be followed from its tail, and the first
    /// line handed back must be whole — a fragment would parse as malformed and
    /// pollute the very health signal this feature exists to provide.
    #[test]
    fn a_large_file_is_followed_from_a_line_boundary() {
        let dir = std::env::temp_dir().join(format!("mogeung-big-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.jsonl");

        let mut body = String::new();
        for i in 0..500 {
            body.push_str(&format!(r#"{{"type":"user","n":{i}}}"#));
            body.push('\n');
        }
        std::fs::write(&path, &body).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();

        let mut t = Tailer::default();
        let skipped = t.start_near_end(&path, 400);
        assert!(skipped > 0, "nothing was skipped from a file over the cap");
        assert!(skipped < size);

        let lines = t.read_new(&path).unwrap();
        assert!(!lines.is_empty());
        for l in &lines {
            assert!(
                serde_json::from_str::<serde_json::Value>(l).is_ok(),
                "tail started mid-line: {l:?}"
            );
        }
        // The last line of the file must still be among them.
        assert!(lines.last().unwrap().contains(r#""n":499"#));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_small_file_is_read_whole_despite_the_cap() {
        let dir = std::env::temp_dir().join(format!("mogeung-small-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("small.jsonl");
        std::fs::write(&path, "a\nb\nc\n").unwrap();

        let mut t = Tailer::default();
        assert_eq!(t.start_near_end(&path, 1_000_000), 0);
        assert_eq!(t.read_new(&path).unwrap(), vec!["a", "b", "c"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_current_process_counts_as_alive() {
        assert!(pid_alive(std::process::id()));
        // Very unlikely to exist, and definitely not ours.
        assert!(!pid_alive(999_999));
    }
}
