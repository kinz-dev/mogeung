//! Token burn across every transcript. Pillar G (`R-G1`–`R-G3`), reused by
//! pillar F's analytics.
//!
//! The live scan loop folds usage per *session*; this module answers the
//! questions that need history — per day, per repo, the trailing five-hour
//! window, and where past limit hits landed. It reads the same transcripts the
//! watcher tails, but on demand and incrementally: per file it remembers the
//! byte offset it reached and the aggregates so far, so a repeat report only
//! reads what was appended. Read-only against `~/.claude`, as everywhere.
//!
//! Granularity is deliberate: hours for windows, local days for calendars.
//! An hour-bucketed "five-hour window" can be off by up to an hour at the
//! edges — fine for an estimate that is labelled one, and 300× cheaper than
//! keeping every message timestamp.

use chrono::{DateTime, Local, Utc};
use mogeung_core::usage::{DayBurn, LimitHit, RepoBurn, SessionBurn, UsageReport};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Hour buckets older than this are pruned; limit hits older than this can no
/// longer have their preceding window computed.
const HOUR_RETENTION_DAYS: i64 = 14;
/// Day buckets kept per file. 60 local days is two calendar months.
const DAY_RETENTION: usize = 60;
/// Limit hits remembered per file.
const MAX_HITS: usize = 20;
/// The window the CLI enforces. Not configurable because it is not ours.
const WINDOW_HOURS: i64 = 5;

#[derive(Debug, Default, Clone)]
struct FileUsage {
    /// Bytes of the file already folded into the aggregates.
    offset: u64,
    session_id: String,
    /// Last path segment of the last cwd seen — display attribution only.
    repo: String,
    /// Local `YYYY-MM-DD` → (tokens_in, tokens_out).
    days: BTreeMap<String, (u64, u64)>,
    /// Unix-hour (`ts / 3600`) → (tokens_in, tokens_out).
    hours: BTreeMap<i64, (u64, u64)>,
    hits: Vec<(DateTime<Utc>, Option<String>)>,
    tokens_in: u64,
    tokens_out: u64,
    /// The file existed but could not be read this pass.
    skipped: bool,
}

/// Incremental scanner. Owned by the daemon, used inside `spawn_blocking`.
#[derive(Debug, Default)]
pub struct UsageScanner {
    files: HashMap<PathBuf, FileUsage>,
}

impl UsageScanner {
    /// Fold everything new under `projects_root` and produce a report.
    pub fn report(&mut self, projects_root: &Path, now: DateTime<Utc>) -> UsageReport {
        let mut paths = Vec::new();
        collect_jsonl(projects_root, &mut paths, 0);

        for path in &paths {
            self.scan_file(path);
        }
        // A transcript deleted from disk stops contributing.
        self.files.retain(|p, _| paths.iter().any(|q| q == p));

        self.aggregate(now)
    }

    fn scan_file(&mut self, path: &Path) {
        let entry = self.files.entry(path.to_path_buf()).or_default();
        entry.skipped = false;
        if entry.session_id.is_empty() {
            entry.session_id = session_id_of(path);
        }

        let Ok(meta) = std::fs::metadata(path) else {
            entry.skipped = true;
            return;
        };
        let size = meta.len();
        if size < entry.offset {
            // Truncated or rewritten: our aggregates describe bytes that no
            // longer exist. Start the file over rather than double-count.
            *entry = FileUsage {
                session_id: std::mem::take(&mut entry.session_id),
                ..Default::default()
            };
        }
        if size == entry.offset {
            return;
        }

        let Ok(mut f) = std::fs::File::open(path) else {
            entry.skipped = true;
            return;
        };
        if f.seek(SeekFrom::Start(entry.offset)).is_err() {
            entry.skipped = true;
            return;
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            // Mid-write or non-UTF8 tail: try again next report.
            entry.skipped = true;
            return;
        }
        // Only fold complete lines; a partial trailing line stays unread so
        // the next pass re-reads it whole.
        let complete = match buf.rfind('\n') {
            Some(i) => i + 1,
            None => 0,
        };
        for line in buf[..complete].lines() {
            fold_line(entry, line);
        }
        entry.offset += complete as u64;
        prune(entry);
    }

    fn aggregate(&self, now: DateTime<Utc>) -> UsageReport {
        let mut days: BTreeMap<String, (u64, u64, u32)> = BTreeMap::new();
        let mut repos: HashMap<String, (u64, u64, u32)> = HashMap::new();
        let mut sessions: HashMap<String, SessionBurn> = HashMap::new();
        let mut all_hours: BTreeMap<i64, (u64, u64)> = BTreeMap::new();
        let mut hits: Vec<(DateTime<Utc>, String, Option<String>)> = Vec::new();
        let mut scanned = 0u32;
        let mut skipped = 0u32;

        for fu in self.files.values() {
            scanned += 1;
            if fu.skipped {
                skipped += 1;
            }
            for (day, (i, o)) in &fu.days {
                let e = days.entry(day.clone()).or_default();
                e.0 += i;
                e.1 += o;
                e.2 += 1;
            }
            for (h, (i, o)) in &fu.hours {
                let e = all_hours.entry(*h).or_default();
                e.0 += i;
                e.1 += o;
            }
            let repo = if fu.repo.is_empty() { "?".to_string() } else { fu.repo.clone() };
            let r = repos.entry(repo.clone()).or_default();
            r.0 += fu.tokens_in;
            r.1 += fu.tokens_out;
            r.2 += 1;
            let s = sessions
                .entry(fu.session_id.clone())
                .or_insert_with(|| SessionBurn {
                    session_id: fu.session_id.clone(),
                    repo,
                    tokens_in: 0,
                    tokens_out: 0,
                });
            s.tokens_in += fu.tokens_in;
            s.tokens_out += fu.tokens_out;
            for (at, resets) in &fu.hits {
                hits.push((*at, fu.session_id.clone(), resets.clone()));
            }
        }

        let window_of = |end: DateTime<Utc>| -> (u64, u64) {
            let end_h = end.timestamp() / 3600;
            let start_h = end_h - (WINDOW_HOURS - 1);
            all_hours
                .range(start_h..=end_h)
                .fold((0, 0), |(i, o), (_, (hi, ho))| (i + hi, o + ho))
        };
        let (window_tokens_in, window_tokens_out) = window_of(now);

        hits.sort_by(|a, b| b.0.cmp(&a.0));
        let hour_floor = now - chrono::Duration::days(HOUR_RETENTION_DAYS);
        let limit_hits: Vec<LimitHit> = hits
            .into_iter()
            .take(MAX_HITS)
            .map(|(at, session_id, resets)| LimitHit {
                at,
                session_id,
                resets,
                // Only computable while the hour buckets around the hit still
                // exist; 0 means "unknown", and the estimate ignores it.
                window_tokens_out: if at >= hour_floor { window_of(at).1 } else { 0 },
            })
            .collect();
        let est_window_limit_out = limit_hits
            .iter()
            .map(|h| h.window_tokens_out)
            .filter(|w| *w > 0)
            .min();

        let mut days: Vec<DayBurn> = days
            .into_iter()
            .map(|(day, (tokens_in, tokens_out, sessions))| DayBurn {
                day,
                tokens_in,
                tokens_out,
                sessions,
            })
            .collect();
        days.reverse();
        days.truncate(DAY_RETENTION);

        let mut repos: Vec<RepoBurn> = repos
            .into_iter()
            .map(|(repo, (tokens_in, tokens_out, sessions))| RepoBurn {
                repo,
                tokens_in,
                tokens_out,
                sessions,
            })
            .collect();
        repos.sort_by(|a, b| b.tokens_out.cmp(&a.tokens_out).then(a.repo.cmp(&b.repo)));

        let mut sessions: Vec<SessionBurn> = sessions.into_values().collect();
        sessions.sort_by(|a, b| {
            b.tokens_out
                .cmp(&a.tokens_out)
                .then(a.session_id.cmp(&b.session_id))
        });
        sessions.truncate(100);

        UsageReport {
            days,
            repos,
            sessions,
            window_tokens_in,
            window_tokens_out,
            limit_hits,
            est_window_limit_out,
            generated_at: Some(now),
            files_scanned: scanned,
            files_skipped: skipped,
        }
    }
}

/// The session a transcript belongs to. Subagent transcripts live at
/// `<session>/subagents/agent-*.jsonl` and burn on their parent's behalf.
fn session_id_of(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut anc = path.ancestors().skip(1);
    if let (Some(subagents), Some(parent)) = (anc.next(), anc.next()) {
        if subagents.file_name().is_some_and(|n| n == "subagents") {
            if let Some(p) = parent.file_name() {
                return p.to_string_lossy().to_string();
            }
        }
    }
    stem
}

fn fold_line(entry: &mut FileUsage, line: &str) {
    // Cheap pre-filter: the only lines that matter carry usage or the
    // synthetic-limit marker. Everything else is skipped without a JSON parse
    // — this loop sees tens of megabytes.
    if !line.contains("\"usage\"") && !line.contains("<synthetic>") {
        return;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
        return;
    }
    let Some(msg) = v.get("message") else {
        return;
    };
    let ts = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc));

    if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
        if let Some(name) = cwd.rsplit('/').next() {
            if !name.is_empty() {
                entry.repo = name.to_string();
            }
        }
    }

    if msg.get("model").and_then(|m| m.as_str()) == Some("<synthetic>") {
        let text: String = msg
            .get("content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if text.to_lowercase().contains("limit") {
            let resets = text
                .split_once("resets")
                .map(|(_, r)| r.trim_start_matches([' ', ':']).trim().to_string())
                .filter(|r| !r.is_empty());
            entry.hits.push((ts.unwrap_or_else(Utc::now), resets));
            if entry.hits.len() > MAX_HITS {
                let excess = entry.hits.len() - MAX_HITS;
                entry.hits.drain(..excess);
            }
        }
        return;
    }

    let Some(usage) = msg.get("usage") else {
        return;
    };
    let n = |k: &str| usage.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let tin = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
    let tout = n("output_tokens");
    if tin == 0 && tout == 0 {
        return;
    }
    entry.tokens_in += tin;
    entry.tokens_out += tout;
    let Some(ts) = ts else {
        return;
    };
    let day = ts.with_timezone(&Local).format("%Y-%m-%d").to_string();
    let d = entry.days.entry(day).or_default();
    d.0 += tin;
    d.1 += tout;
    let h = entry.hours.entry(ts.timestamp() / 3600).or_default();
    h.0 += tin;
    h.1 += tout;
}

fn prune(entry: &mut FileUsage) {
    let hour_floor = (Utc::now() - chrono::Duration::days(HOUR_RETENTION_DAYS)).timestamp() / 3600;
    entry.hours.retain(|h, _| *h >= hour_floor);
    while entry.days.len() > DAY_RETENTION {
        let Some(k) = entry.days.keys().next().cloned() else {
            break;
        };
        entry.days.remove(&k);
    }
}

/// Recursively gather `.jsonl` files. Depth-capped: the layout is
/// `projects/<slug>/<id>.jsonl` plus `<slug>/<id>/subagents/agent-*.jsonl`,
/// and a runaway symlink must not turn a report into a filesystem walk.
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            collect_jsonl(&p, out, depth + 1);
        } else if ft.is_file() && p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Unique scratch root per test, the canary.rs pattern — no tempfile dep.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("mogeung-usage-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_lines(path: &Path, lines: &[String]) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
    }

    fn assistant_line(ts: &str, tin: u64, tout: u64, cwd: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","cwd":"{cwd}","message":{{"model":"claude-fable-5","content":[],"usage":{{"input_tokens":{tin},"output_tokens":{tout}}}}}}}"#
        )
    }

    fn limit_line(ts: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"model":"<synthetic>","content":[{{"type":"text","text":"You've hit your session limit · resets 8pm (Europe/London)"}}],"usage":{{"input_tokens":0,"output_tokens":0}}}}}}"#
        )
    }

    fn iso(t: DateTime<Utc>) -> String {
        t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    #[test]
    fn burn_lands_in_days_repos_and_sessions() {
        let dir = Scratch::new("days");
        let slug = dir.path().join("-home-u-projects-alpha");
        std::fs::create_dir_all(&slug).unwrap();
        let now = Utc::now();
        write_lines(
            &slug.join("aaaa.jsonl"),
            &[
                assistant_line(&iso(now), 100, 40, "/home/u/projects/alpha"),
                assistant_line(&iso(now), 50, 10, "/home/u/projects/alpha"),
            ],
        );

        let mut sc = UsageScanner::default();
        let r = sc.report(dir.path(), now);
        assert_eq!(r.sessions.len(), 1);
        assert_eq!(r.sessions[0].session_id, "aaaa");
        assert_eq!(r.sessions[0].tokens_out, 50);
        assert_eq!(r.repos[0].repo, "alpha");
        assert_eq!(r.repos[0].tokens_in, 150);
        assert_eq!(r.days.len(), 1);
        assert_eq!(r.window_tokens_out, 50, "burn just now is inside the window");
        assert!(r.est_window_limit_out.is_none(), "no hit, no estimate — never guess");
    }

    /// Incrementality: a second report reads only appended bytes, and the
    /// counts keep going up rather than doubling.
    #[test]
    fn appended_lines_fold_without_double_counting() {
        let dir = Scratch::new("append");
        let slug = dir.path().join("-p");
        std::fs::create_dir_all(&slug).unwrap();
        let file = slug.join("s1.jsonl");
        let now = Utc::now();
        write_lines(&file, &[assistant_line(&iso(now), 10, 5, "/p")]);

        let mut sc = UsageScanner::default();
        let r1 = sc.report(dir.path(), now);
        assert_eq!(r1.sessions[0].tokens_out, 5);

        write_lines(&file, &[assistant_line(&iso(now), 10, 7, "/p")]);
        let r2 = sc.report(dir.path(), now);
        assert_eq!(r2.sessions[0].tokens_out, 12, "5 + 7, not 5+5+7");
    }

    /// The estimate is the smallest observed pre-hit window — and it only
    /// exists once a hit has been seen.
    #[test]
    fn a_limit_hit_yields_an_estimate_from_the_preceding_window() {
        let dir = Scratch::new("estimate");
        let slug = dir.path().join("-p");
        std::fs::create_dir_all(&slug).unwrap();
        let now = Utc::now();
        let before = now - chrono::Duration::hours(2);
        write_lines(
            &slug.join("s1.jsonl"),
            &[
                assistant_line(&iso(before), 1000, 400, "/p"),
                limit_line(&iso(now)),
            ],
        );
        let mut sc = UsageScanner::default();
        let r = sc.report(dir.path(), now);
        assert_eq!(r.limit_hits.len(), 1);
        assert_eq!(r.limit_hits[0].resets.as_deref(), Some("8pm (Europe/London)"));
        assert_eq!(r.limit_hits[0].window_tokens_out, 400);
        assert_eq!(r.est_window_limit_out, Some(400));
    }

    /// Subagent transcripts burn on their parent's account.
    #[test]
    fn subagent_burn_folds_into_the_parent_session() {
        let dir = Scratch::new("subagent");
        let sub = dir.path().join("-p").join("parent-1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        let now = Utc::now();
        write_lines(
            &sub.join("agent-abc.jsonl"),
            &[assistant_line(&iso(now), 10, 9, "/p")],
        );
        let mut sc = UsageScanner::default();
        let r = sc.report(dir.path(), now);
        assert_eq!(r.sessions[0].session_id, "parent-1");
        assert_eq!(r.sessions[0].tokens_out, 9);
    }

    /// A truncated file starts over instead of double-counting or panicking.
    #[test]
    fn a_shrunk_file_is_rescanned_from_zero() {
        let dir = Scratch::new("shrink");
        let slug = dir.path().join("-p");
        std::fs::create_dir_all(&slug).unwrap();
        let file = slug.join("s1.jsonl");
        let now = Utc::now();
        write_lines(
            &file,
            &[
                assistant_line(&iso(now), 10, 5, "/p"),
                assistant_line(&iso(now), 10, 5, "/p"),
            ],
        );
        let mut sc = UsageScanner::default();
        assert_eq!(sc.report(dir.path(), now).sessions[0].tokens_out, 10);

        std::fs::write(&file, assistant_line(&iso(now), 10, 3, "/p") + "\n").unwrap();
        let r = sc.report(dir.path(), now);
        assert_eq!(r.sessions[0].tokens_out, 3);
    }

    #[test]
    fn garbage_lines_are_skipped_not_fatal() {
        let dir = Scratch::new("garbage");
        let slug = dir.path().join("-p");
        std::fs::create_dir_all(&slug).unwrap();
        let now = Utc::now();
        write_lines(
            &slug.join("s1.jsonl"),
            &[
                "not json with \"usage\" in it".to_string(),
                assistant_line(&iso(now), 1, 2, "/p"),
            ],
        );
        let mut sc = UsageScanner::default();
        let r = sc.report(dir.path(), now);
        assert_eq!(r.sessions[0].tokens_out, 2);
    }
}
