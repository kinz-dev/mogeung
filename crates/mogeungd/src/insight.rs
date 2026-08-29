//! Cross-session intelligence engine. Pillar F (`R-F1`–`R-F9`).
//!
//! Everything mogeung otherwise knows is per-session; this module answers the
//! questions that need the whole corpus — "where did I solve this before",
//! "what happened Tuesday", "this error again?". It reads the transcripts
//! under `projects_root` (recursively — subagent files live at
//! `<session>/subagents/agent-*.jsonl`) and `history.jsonl`, both passed in
//! explicitly: nothing here knows where `~/.claude` is, and nothing writes.
//!
//! **Cost model, on purpose:** every call is a full streaming read pass over
//! what it needs. No persistent index — an index can silently lie after a
//! file changes underneath it, and honesty beats cleverness at this corpus
//! size (~67 MB on the reference machine, a few hundred ms). Memory stays
//! bounded by streaming line by line; the one transient exception is that a
//! single oversized line is read before being skipped. Callers own caching
//! and debouncing.
//!
//! Every reader degrades: a missing file or directory is an empty result,
//! a malformed line is skipped, and nothing panics — the formats are
//! undocumented and change without warning.

use chrono::{DateTime, Local, Timelike, Utc};
use mogeung_core::insight::{
    Analytics, DayCount, DayDigest, DecisionCandidate, History, HistoryEntry, NearbyTurn,
    PastedContent, ProjectCount, PromptCluster, RecurringFailure, SearchHit, SearchResults,
    SearchSource, SessionDigest, SubagentNode,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};

/// Lines longer than this are skipped, not parsed — a megabyte "line" is a
/// paste blob or corruption, and either way not something to grep sentence
/// fragments out of.
const LINE_CAP: usize = 1024 * 1024;
/// Hits allowed per file before the rest of that file goes unsearched.
const SEARCH_PER_FILE: usize = 20;
/// Preview clip length for search hits, in chars.
const PREVIEW_CHARS: usize = 200;
/// Verbatim error examples are clipped here — evidence, not a log viewer.
const EXAMPLE_CHARS: usize = 400;
/// Sentence clips for decision candidates.
const SENTENCE_CHARS: usize = 300;
/// Normalised prompts are keyed on this many leading chars, so "same prompt,
/// different tail" clusters together.
const PROMPT_KEY_CHARS: usize = 60;

// ---------------------------------------------------------------------------
// Search (R-F1)
// ---------------------------------------------------------------------------

/// Literal substring search across every transcript and all prompt history.
///
/// Smart-cased like the worktree search in `state.rs`: an all-lowercase query
/// matches case-insensitively, any uppercase makes it exact. `history.jsonl`
/// is searched first (small, and always relevant), then transcripts in path
/// order; `cap` bounds total hits and [`SEARCH_PER_FILE`] bounds each file,
/// with `truncated` / `files_truncated` saying which cap bit.
pub fn search(projects_root: &Path, history_path: &Path, query: &str, cap: usize) -> SearchResults {
    let query = query.trim();
    let mut out = SearchResults::default();
    if query.is_empty() {
        return out;
    }
    let case_sensitive = query.chars().any(|c| c.is_uppercase());
    let needle = if case_sensitive { query.to_string() } else { query.to_lowercase() };
    let matches = |line: &str| -> bool {
        if case_sensitive {
            line.contains(&needle)
        } else {
            line.to_lowercase().contains(&needle)
        }
    };

    // Prompt history first: it is one small file and a hit there is a prompt
    // the user typed, which is what most searches are hunting.
    if stream_lines(history_path, |line_no, line| {
        if !matches(line) {
            return true;
        }
        if out.hits.len() >= cap {
            out.truncated = true;
            return false;
        }
        let v: Option<Value> = serde_json::from_str(line).ok();
        out.hits.push(SearchHit {
            source: SearchSource::History,
            session_id: v
                .as_ref()
                .and_then(|v| str_at(v, "sessionId"))
                .unwrap_or("")
                .to_string(),
            line: line_no,
            timestamp: v
                .as_ref()
                .and_then(|v| v.get("timestamp"))
                .and_then(Value::as_i64)
                .and_then(DateTime::from_timestamp_millis),
            role: "prompt".to_string(),
            preview: preview_around(line, &needle, case_sensitive),
        });
        true
    }) {
        out.files_scanned += 1;
    }
    if out.truncated {
        return out;
    }

    let mut files = Vec::new();
    collect_jsonl(projects_root, &mut files, 0);
    files.sort();

    for path in &files {
        let session = session_id_of(path);
        let subagent = is_subagent(path);
        let mut file_hits = 0usize;
        let mut file_capped = false;
        if stream_lines(path, |line_no, line| {
            if !matches(line) {
                return true;
            }
            if out.hits.len() >= cap {
                out.truncated = true;
                return false;
            }
            if file_hits >= SEARCH_PER_FILE {
                file_capped = true;
                return false;
            }
            file_hits += 1;
            let v: Option<Value> = serde_json::from_str(line).ok();
            let role = v
                .as_ref()
                .and_then(|v| str_at(v, "type"))
                .unwrap_or("")
                .to_string();
            let source = match v.as_ref() {
                // A subagent's "user" lines are its parent's tool calls, not
                // a human typing — they stay plain transcript hits.
                Some(v) if !subagent && human_prompt(v).is_some() => SearchSource::Prompt,
                _ => SearchSource::Transcript,
            };
            out.hits.push(SearchHit {
                source,
                session_id: session.clone(),
                line: line_no,
                timestamp: v.as_ref().and_then(parse_ts),
                role,
                preview: preview_around(line, &needle, case_sensitive),
            });
            true
        }) {
            out.files_scanned += 1;
        }
        if file_capped {
            out.files_truncated += 1;
        }
        if out.truncated {
            break;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// history.jsonl
// ---------------------------------------------------------------------------

/// Parse `history.jsonl`: `{display, pastedContents, timestamp (unix millis),
/// project (absolute cwd), sessionId}` per line, strictly timestamp-ordered
/// in the corpus. `pastedContents` values carry either inline `content` or
/// only a `contentHash` — both are real, so content is optional. Malformed
/// lines are skipped and counted, never fatal.
pub fn history(history_path: &Path) -> History {
    let mut out = History::default();
    stream_lines(history_path, |_, line| {
        let parsed: Option<HistoryEntry> = serde_json::from_str::<Value>(line).ok().and_then(|v| {
            Some(HistoryEntry {
                display: str_at(&v, "display")?.to_string(),
                timestamp_ms: v.get("timestamp")?.as_i64()?,
                project: str_at(&v, "project")?.to_string(),
                session_id: str_at(&v, "sessionId")?.to_string(),
                pasted: v
                    .get("pastedContents")
                    .and_then(Value::as_object)
                    .map(|o| {
                        o.values()
                            .map(|p| PastedContent {
                                content: str_at(p, "content").map(str::to_string),
                                content_hash: str_at(p, "contentHash").map(str::to_string),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        });
        match parsed {
            Some(e) => out.entries.push(e),
            None => out.malformed_lines += 1,
        }
        true
    });
    out
}

// ---------------------------------------------------------------------------
// Day digest (R-F3)
// ---------------------------------------------------------------------------

/// Measurable activity per session on one local day (`YYYY-MM-DD`).
///
/// Evidence only: prompts typed, tools called, files named by edit tools and
/// file-history deltas, error lines, token counts. Assistant prose is never
/// summarised — the spec forbids building a diary out of self-reports. Lines
/// without timestamps cannot be placed on a day and are not counted (titles
/// and cwd are still picked up from them, being attribution rather than
/// activity).
pub fn digest(projects_root: &Path, day: &str) -> DayDigest {
    #[derive(Default)]
    struct Acc {
        title: Option<String>,
        repo: String,
        turns: u32,
        tool_calls: u32,
        touched: BTreeSet<String>,
        errors: u32,
        tokens_in: u64,
        tokens_out: u64,
        active: bool,
    }

    let mut files = Vec::new();
    collect_jsonl(projects_root, &mut files, 0);
    files.sort();
    let mut sessions: HashMap<String, Acc> = HashMap::new();
    let mut files_scanned = 0u32;

    for path in &files {
        let session = session_id_of(path);
        let subagent = is_subagent(path);
        if stream_lines(path, |_, line| {
            let Ok(v) = serde_json::from_str::<Value>(line) else { return true };
            let acc = sessions.entry(session.clone()).or_default();

            // Attribution first — these are not day-bound activity.
            if str_at(&v, "type") == Some("ai-title") {
                if let Some(t) = str_at(&v, "aiTitle") {
                    acc.title = Some(t.to_string());
                }
            }
            if let Some(cwd) = str_at(&v, "cwd") {
                if let Some(name) = cwd.rsplit('/').next() {
                    if !name.is_empty() {
                        acc.repo = name.to_string();
                    }
                }
            }

            let on_day = parse_ts(&v)
                .map(|ts| ts.with_timezone(&Local).format("%Y-%m-%d").to_string() == day)
                .unwrap_or(false);
            if !on_day {
                return true;
            }
            acc.active = true;

            match str_at(&v, "type") {
                Some("file-history-delta") => {
                    if let Some(p) = str_at(&v, "trackingPath") {
                        acc.touched.insert(p.to_string());
                    }
                }
                Some("user") => {
                    if !subagent && human_prompt(&v).is_some() {
                        acc.turns += 1;
                    }
                }
                Some("assistant") => {
                    let (tin, tout) = usage_tokens(&v);
                    acc.tokens_in += tin;
                    acc.tokens_out += tout;
                    if is_error_line(&v) {
                        acc.errors += 1;
                    }
                    for block in tool_use_blocks(&v) {
                        acc.tool_calls += 1;
                        if let Some(name) = str_at(block, "name") {
                            if matches!(name, "Write" | "Edit" | "NotebookEdit") {
                                if let Some(p) =
                                    block.get("input").and_then(|i| str_at(i, "file_path"))
                                {
                                    acc.touched.insert(p.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            true
        }) {
            files_scanned += 1;
        }
    }

    let mut out: Vec<SessionDigest> = sessions
        .into_iter()
        .filter(|(_, a)| a.active)
        .map(|(session_id, a)| SessionDigest {
            session_id,
            title: a.title,
            repo: a.repo,
            turns: a.turns,
            tool_calls: a.tool_calls,
            files_touched: a.touched.into_iter().collect(),
            errors: a.errors,
            tokens_in: a.tokens_in,
            tokens_out: a.tokens_out,
        })
        .collect();
    out.sort_by(|a, b| {
        b.tokens_out
            .cmp(&a.tokens_out)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    DayDigest {
        day: day.to_string(),
        sessions: out,
        files_scanned,
    }
}

// ---------------------------------------------------------------------------
// Recurring failures (R-F4)
// ---------------------------------------------------------------------------

/// Error shapes seen in at least `min_sessions` distinct sessions.
///
/// Sources: `isApiErrorMessage` text, non-null `error` fields, and short
/// (< 400 chars) `tool_result` errors — a long error is a build log, and two
/// build logs matching each other proves nothing. Normalisation lowercases,
/// folds digit runs to `#`, replaces path-shaped tokens with `<path>` and
/// collapses whitespace; the normalised key ships with each group so the
/// merge can be audited.
pub fn recurring_failures(projects_root: &Path, min_sessions: usize) -> Vec<RecurringFailure> {
    struct Group {
        sessions: BTreeSet<String>,
        example: String,
        count: u32,
    }

    let mut files = Vec::new();
    collect_jsonl(projects_root, &mut files, 0);
    files.sort();
    let mut groups: HashMap<String, Group> = HashMap::new();

    for path in &files {
        let session = session_id_of(path);
        stream_lines(path, |_, line| {
            // Cheap pre-filter: every error source carries one of these keys.
            if !line.contains("error") && !line.contains("Error") {
                return true;
            }
            let Ok(v) = serde_json::from_str::<Value>(line) else { return true };
            let mut texts: Vec<String> = Vec::new();
            if is_api_error(&v) {
                let t = text_of(v.get("message").and_then(|m| m.get("content")).unwrap_or(&Value::Null));
                if !t.trim().is_empty() {
                    texts.push(t);
                }
            }
            if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
                let t = text_of(e);
                if !t.trim().is_empty() {
                    texts.push(t);
                }
            }
            if let Some(blocks) = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_array)
            {
                for b in blocks {
                    if str_at(b, "type") == Some("tool_result")
                        && b.get("is_error").and_then(Value::as_bool).unwrap_or(false)
                    {
                        let t = text_of(b.get("content").unwrap_or(&Value::Null));
                        if !t.trim().is_empty() && t.chars().count() < 400 {
                            texts.push(t);
                        }
                    }
                }
            }
            for t in texts {
                let key = normalise_error(&t);
                if key.is_empty() {
                    continue;
                }
                let g = groups.entry(key).or_insert_with(|| Group {
                    sessions: BTreeSet::new(),
                    example: clip(&t, EXAMPLE_CHARS),
                    count: 0,
                });
                g.sessions.insert(session.clone());
                g.count += 1;
            }
            true
        });
    }

    let mut out: Vec<RecurringFailure> = groups
        .into_iter()
        .filter(|(_, g)| g.sessions.len() >= min_sessions)
        .map(|(normalized, g)| RecurringFailure {
            normalized,
            example: g.example,
            sessions: g.sessions.into_iter().collect(),
            count: g.count,
        })
        .collect();
    out.sort_by(|a, b| {
        b.sessions
            .len()
            .cmp(&a.sessions.len())
            .then_with(|| b.count.cmp(&a.count))
            .then_with(|| a.normalized.cmp(&b.normalized))
    });
    out
}

// ---------------------------------------------------------------------------
// Prompt library (R-F6)
// ---------------------------------------------------------------------------

/// Cluster near-duplicate prompts, most-reused first.
///
/// Prompts are normalised (lowercased, whitespace collapsed) and grouped by
/// their first [`PROMPT_KEY_CHARS`] normalised chars. For short prompts the
/// key is the whole prompt, so this is exact matching; for long ones it is
/// prefix matching, which catches "same prompt with a different tail". Only
/// clusters with two or more members are reported.
pub fn prompt_library(history: &[HistoryEntry]) -> Vec<PromptCluster> {
    let mut groups: HashMap<String, Vec<&HistoryEntry>> = HashMap::new();
    for e in history {
        let norm = normalise_prompt(&e.display);
        if norm.is_empty() {
            continue;
        }
        let key: String = norm.chars().take(PROMPT_KEY_CHARS).collect();
        groups.entry(key).or_default().push(e);
    }

    let mut out: Vec<PromptCluster> = groups
        .into_values()
        .filter(|members| members.len() >= 2)
        .filter_map(|members| {
            let first = members.iter().map(|e| e.timestamp_ms).min()?;
            let latest = members.iter().max_by_key(|e| e.timestamp_ms)?;
            let mut projects: Vec<String> = members
                .iter()
                .map(|e| e.project.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            projects.dedup();
            Some(PromptCluster {
                representative: latest.display.clone(),
                count: members.len() as u32,
                first_used: DateTime::from_timestamp_millis(first)?,
                last_used: DateTime::from_timestamp_millis(latest.timestamp_ms)?,
                projects,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.last_used.cmp(&a.last_used))
            .then_with(|| a.representative.cmp(&b.representative))
    });
    out
}

// ---------------------------------------------------------------------------
// Analytics (R-F5)
// ---------------------------------------------------------------------------

/// Prompt-history analytics: prompts/day, distinct sessions/day, per-project
/// totals, and an hour-of-day histogram — all on local time, because "when
/// do I work" is a local question.
///
/// Token burn per day is deliberately **not** recomputed here: the usage
/// module (pillar G) owns burn, and a second computation of the same number
/// is a disagreement waiting to be dogfooded. `projects_root` is only used
/// to say whether each project still has a transcript directory — the
/// history→directory mapping is lossy and a client should know when a drill-
/// down will find nothing.
pub fn analytics(history: &[HistoryEntry], projects_root: &Path) -> Analytics {
    let mut prompts_per_day: BTreeMap<String, u32> = BTreeMap::new();
    let mut sessions_per_day: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut projects: BTreeMap<&str, (u32, BTreeSet<&str>)> = BTreeMap::new();
    let mut hour_histogram = [0u32; 24];

    for e in history {
        let Some(ts) = DateTime::from_timestamp_millis(e.timestamp_ms) else { continue };
        let local = ts.with_timezone(&Local);
        let day = local.format("%Y-%m-%d").to_string();
        *prompts_per_day.entry(day.clone()).or_default() += 1;
        sessions_per_day.entry(day).or_default().insert(&e.session_id);
        hour_histogram[local.hour() as usize % 24] += 1;
        let p = projects.entry(&e.project).or_default();
        p.0 += 1;
        p.1.insert(&e.session_id);
    }

    let mut projects: Vec<ProjectCount> = projects
        .into_iter()
        .map(|(project, (prompts, sessions))| ProjectCount {
            has_transcripts: projects_root.join(project_slug(project)).is_dir(),
            project: project.to_string(),
            prompts,
            sessions: sessions.len() as u32,
        })
        .collect();
    projects.sort_by(|a, b| b.prompts.cmp(&a.prompts).then_with(|| a.project.cmp(&b.project)));

    Analytics {
        prompts_per_day: prompts_per_day
            .into_iter()
            .map(|(day, count)| DayCount { day, count })
            .collect(),
        sessions_per_day: sessions_per_day
            .into_iter()
            .map(|(day, s)| DayCount { day, count: s.len() as u32 })
            .collect(),
        projects,
        hour_histogram,
    }
}

// ---------------------------------------------------------------------------
// Subagent tree (R-F8)
// ---------------------------------------------------------------------------

/// Summarise a session's `subagents/agent-*.jsonl` files, first-seen first.
///
/// The slug directory is found by scanning `projects_root`'s children for
/// one containing `<session_id>/subagents` — the session→slug mapping is not
/// stored anywhere the daemon may trust more than the filesystem itself.
pub fn subagent_tree(projects_root: &Path, session_id: &str) -> Vec<SubagentNode> {
    // A session id is a flat name; anything path-shaped is not one, and must
    // not become a traversal.
    if session_id.is_empty() || session_id.contains(['/', '\\']) || session_id.starts_with('.') {
        return Vec::new();
    }
    let Ok(rd) = std::fs::read_dir(projects_root) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path().join(session_id).join("subagents"))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "jsonl"))
            .collect();
        files.sort();
        for path in files {
            let mut node = SubagentNode {
                agent_id: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                ..Default::default()
            };
            stream_lines(&path, |_, line| {
                node.lines += 1;
                let Ok(v) = serde_json::from_str::<Value>(line) else { return true };
                if let Some(ts) = parse_ts(&v) {
                    if node.first_ts.is_none() {
                        node.first_ts = Some(ts);
                    }
                    node.last_ts = Some(ts);
                }
                if str_at(&v, "type") == Some("assistant") {
                    node.tokens_out += usage_tokens(&v).1;
                    for block in tool_use_blocks(&v) {
                        let name = str_at(block, "name").unwrap_or("tool");
                        let hint = block.get("input").map(tool_hint).unwrap_or_default();
                        node.last_activity = Some(if hint.is_empty() {
                            name.to_string()
                        } else {
                            format!("{name}: {hint}")
                        });
                    }
                    if let Some(text) = assistant_text(&v) {
                        node.last_activity = Some(clip(&text, 120));
                    }
                }
                true
            });
            out.push(node);
        }
    }
    out.sort_by(|a, b| a.first_ts.cmp(&b.first_ts).then_with(|| a.agent_id.cmp(&b.agent_id)));
    out
}

// ---------------------------------------------------------------------------
// Decision candidates (R-F7)
// ---------------------------------------------------------------------------

/// The honest patterns. Order is precedence: the first to match a sentence
/// names it. Matched on word boundaries — "undecided" is not a decision.
const DECISION_PATTERNS: &[&str] = &[
    "decided",
    "decision",
    "instead of",
    "rather than",
    "chose",
    "choosing",
    "went with",
    "we'll use",
    "switched to",
    "because",
];

/// Decision-shaped sentences from one transcript's assistant text, deduped.
///
/// These are *candidates* for a human to skim into an ADR — pattern matches,
/// nothing more, which is why each carries the `pattern` that fired.
/// Presenting them as anything stronger is forbidden (pillar K).
pub fn decision_candidates(transcript_path: &Path) -> Vec<DecisionCandidate> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    stream_lines(transcript_path, |line_no, line| {
        // Assistant text is the only source; skip everything else unparsed
        // when possible.
        if !line.contains("\"assistant\"") {
            return true;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return true };
        if str_at(&v, "type") != Some("assistant") {
            return true;
        }
        let Some(text) = assistant_text(&v) else { return true };
        let ts = parse_ts(&v);
        for sentence in text.split(['.', '!', '?', '\n']) {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                continue;
            }
            let lower = sentence.to_lowercase();
            let Some(pattern) = DECISION_PATTERNS.iter().find(|p| find_word(&lower, p)) else {
                continue;
            };
            let clipped = clip(sentence, SENTENCE_CHARS);
            if seen.insert(clipped.clone()) {
                out.push(DecisionCandidate {
                    line: line_no,
                    timestamp: ts,
                    pattern: pattern.to_string(),
                    text: clipped,
                });
            }
        }
        true
    });
    out
}

// ---------------------------------------------------------------------------
// Turns near a moment (R-F9)
// ---------------------------------------------------------------------------

/// The `k` human turns / assistant text lines nearest `epoch_secs`, nearest
/// first — the client uses the first to open a transcript "at the commit".
/// Lines without timestamps cannot be placed in time and are skipped.
pub fn turns_near(transcript_path: &Path, epoch_secs: i64, k: usize) -> Vec<NearbyTurn> {
    let mut candidates: Vec<(i64, NearbyTurn)> = Vec::new();
    stream_lines(transcript_path, |line_no, line| {
        let Ok(v) = serde_json::from_str::<Value>(line) else { return true };
        let Some(ts) = parse_ts(&v) else { return true };
        let (role, text) = if let Some(p) = human_prompt(&v) {
            ("user", p)
        } else if let Some(t) = assistant_text(&v) {
            ("assistant", t)
        } else {
            return true;
        };
        candidates.push((
            (ts.timestamp() - epoch_secs).abs(),
            NearbyTurn {
                line: line_no,
                timestamp: ts,
                role: role.to_string(),
                preview: clip(&text, PREVIEW_CHARS),
            },
        ));
        true
    });
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.line.cmp(&b.1.line)));
    candidates.truncate(k);
    candidates.into_iter().map(|(_, t)| t).collect()
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

/// Stream a file line by line: `f` gets the 1-based line number and the line
/// (lossily decoded), and returns `false` to stop early. Lines longer than
/// [`LINE_CAP`] are counted but not delivered. Returns whether the file
/// opened at all — an unreadable file delivers nothing and is no error.
pub(crate) fn stream_lines(path: &Path, mut f: impl FnMut(u64, &str) -> bool) -> bool {
    let Ok(file) = std::fs::File::open(path) else { return false };
    let mut r = std::io::BufReader::new(file);
    let mut buf: Vec<u8> = Vec::new();
    let mut line_no = 0u64;
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {
                line_no += 1;
                if buf.len() > LINE_CAP {
                    continue;
                }
                let text = String::from_utf8_lossy(&buf);
                if !f(line_no, text.trim_end_matches(['\n', '\r'])) {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    true
}

/// Recursively gather `.jsonl` files. Depth-capped like the usage scanner's
/// walk: the layout is `projects/<slug>/<id>.jsonl` plus
/// `<slug>/<id>/subagents/agent-*.jsonl`, and a runaway symlink must not turn
/// a search into a filesystem walk.
fn collect_jsonl(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
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

/// The session a transcript belongs to: its file stem, except that subagent
/// transcripts (`<session>/subagents/agent-*.jsonl`) attribute to the parent
/// session directory. Same rule as the usage scanner.
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

fn is_subagent(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == "subagents")
}

pub(crate) fn str_at<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

pub(crate) fn parse_ts(v: &Value) -> Option<DateTime<Utc>> {
    str_at(v, "timestamp")
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.with_timezone(&Utc))
}

/// The line's human prompt, if it is one: a `user` line, not a sidechain,
/// whose content is a non-empty string or carries `text` blocks. Mirrors the
/// adapter's turn rule — a `tool_result` carrier is not a human typing.
pub(crate) fn human_prompt(v: &Value) -> Option<String> {
    if str_at(v, "type") != Some("user") {
        return None;
    }
    if v.get("isSidechain").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let content = v.get("message")?.get("content")?;
    let joined = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| str_at(b, "type") == Some("text"))
            .filter_map(|b| str_at(b, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    if joined.trim().is_empty() { None } else { Some(joined) }
}

/// Joined `text` blocks of an assistant line, if any.
pub(crate) fn assistant_text(v: &Value) -> Option<String> {
    if str_at(v, "type") != Some("assistant") {
        return None;
    }
    let blocks = v.get("message")?.get("content")?.as_array()?;
    let joined = blocks
        .iter()
        .filter(|b| str_at(b, "type") == Some("text"))
        .filter_map(|b| str_at(b, "text"))
        .collect::<Vec<_>>()
        .join("\n");
    if joined.trim().is_empty() { None } else { Some(joined) }
}

pub(crate) fn tool_use_blocks(v: &Value) -> impl Iterator<Item = &Value> {
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|b| str_at(b, "type") == Some("tool_use"))
}

fn usage_tokens(v: &Value) -> (u64, u64) {
    let Some(u) = v.get("message").and_then(|m| m.get("usage")) else { return (0, 0) };
    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    (
        n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens"),
        n("output_tokens"),
    )
}

fn is_api_error(v: &Value) -> bool {
    v.get("isApiErrorMessage").and_then(Value::as_bool).unwrap_or(false)
}

fn is_error_line(v: &Value) -> bool {
    is_api_error(v) || v.get("error").is_some_and(|e| !e.is_null())
}

fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|b| str_at(b, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Clip to at most `n` chars on a char boundary, with an ellipsis when cut.
fn clip(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n).collect();
    format!("{cut}…")
}

/// A short hint for a tool call — the first recognisable string input.
fn tool_hint(input: &Value) -> String {
    for k in ["file_path", "command", "pattern", "query", "url", "description", "skill", "prompt"] {
        if let Some(s) = str_at(input, k) {
            if !s.trim().is_empty() {
                let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
                return clip(&flat, 80);
            }
        }
    }
    String::new()
}

/// Preview of a matched line: ~40 chars of context before the match, then up
/// to [`PREVIEW_CHARS`] chars, every cut on a char boundary.
fn preview_around(line: &str, needle: &str, case_sensitive: bool) -> String {
    let at = find_match(line, needle, case_sensitive).unwrap_or(0);
    // Walk back 40 chars from the match, staying on boundaries.
    let mut start = at;
    let mut walked = 0;
    while start > 0 && walked < 40 {
        start -= 1;
        while start > 0 && !line.is_char_boundary(start) {
            start -= 1;
        }
        walked += 1;
    }
    let tail = &line[start..];
    let mut p: String = tail.chars().take(PREVIEW_CHARS).collect();
    if tail.chars().nth(PREVIEW_CHARS).is_some() {
        p.push('…');
    }
    if start > 0 {
        p.insert(0, '…');
    }
    p
}

/// Byte offset of the (possibly case-insensitive) match in the original
/// string. The insensitive path compares char by char so the offset is valid
/// for `line` itself — lowercasing a string can change its byte layout.
fn find_match(line: &str, needle: &str, case_sensitive: bool) -> Option<usize> {
    if case_sensitive {
        return line.find(needle);
    }
    let n: Vec<char> = needle.chars().collect();
    for (i, _) in line.char_indices() {
        let mut hay = line[i..].chars();
        if n.iter().all(|nc| {
            hay.next()
                .map(|c| c.to_lowercase().eq(nc.to_lowercase()))
                .unwrap_or(false)
        }) {
            return Some(i);
        }
    }
    None
}

/// `pat` present in `hay` on word boundaries — the chars around the match
/// must not be alphanumeric, so "undecided" never matches "decided".
/// `hay` must already be lowercased.
fn find_word(hay: &str, pat: &str) -> bool {
    for (i, _) in hay.match_indices(pat) {
        let before_ok = hay[..i].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after_ok = hay[i + pat.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Error normalisation: lowercase, path-shaped tokens → `<path>`, digit runs
/// → `#`, whitespace collapsed. Coarse on purpose — the normalised key is
/// shown to the user so an over-merge is visible, not hidden.
fn normalise_error(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .map(|tok| {
            if tok.contains('/') && tok.len() > 1 {
                "<path>".to_string()
            } else {
                let mut out = String::with_capacity(tok.len());
                let mut in_digits = false;
                for c in tok.chars() {
                    if c.is_ascii_digit() {
                        if !in_digits {
                            out.push('#');
                            in_digits = true;
                        }
                    } else {
                        out.push(c);
                        in_digits = false;
                    }
                }
                out
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Prompt normalisation for clustering: lowercase, whitespace collapsed.
fn normalise_prompt(s: &str) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The transcript-directory slug Claude Code derives from a cwd: every
/// non-alphanumeric char becomes `-`. Deliberately generous — it only feeds
/// a "does the directory still exist" boolean.
fn project_slug(project: &str) -> String {
    project
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Unique scratch root per test, the usage.rs pattern — no tempfile dep.
    /// The tag must be unique per test: tests share a process and same-tag
    /// directories collide across parallel tests.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir()
                .join(format!("mogeung-insight-{tag}-{}", std::process::id()));
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
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
    }

    fn iso(t: DateTime<Utc>) -> String {
        t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    fn user_line(ts: &str, text: &str) -> String {
        format!(r#"{{"type":"user","timestamp":"{ts}","message":{{"role":"user","content":"{text}"}}}}"#)
    }

    fn assistant_text_line(ts: &str, text: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","message":{{"content":[{{"type":"text","text":"{text}"}}],"usage":{{"input_tokens":10,"output_tokens":5}}}}}}"#
        )
    }

    fn tool_line(ts: &str, name: &str, file_path: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","cwd":"/home/u/projects/alpha","message":{{"content":[{{"type":"tool_use","id":"t1","name":"{name}","input":{{"file_path":"{file_path}"}}}}],"usage":{{"input_tokens":7,"output_tokens":3}}}}}}"#
        )
    }

    fn api_error_line(ts: &str, text: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","isApiErrorMessage":true,"message":{{"content":[{{"type":"text","text":"{text}"}}]}}}}"#
        )
    }

    fn tool_result_error(ts: &str, text: &str) -> String {
        format!(
            r#"{{"type":"user","timestamp":"{ts}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"{text}"}}]}}}}"#
        )
    }

    fn history_line(ms: i64, display: &str, project: &str, session: &str) -> String {
        format!(
            r#"{{"display":"{display}","pastedContents":{{}},"timestamp":{ms},"project":"{project}","sessionId":"{session}"}}"#
        )
    }

    // -- search (R-F1) ------------------------------------------------------

    #[test]
    fn search_reaches_subagents_and_history_and_attributes_parents() {
        let dir = Scratch::new("search-recursive");
        let root = dir.path().join("projects");
        let now = Utc::now();
        write_lines(
            &root.join("-p/parent-1.jsonl"),
            &[assistant_text_line(&iso(now), "the xylophone theorem holds")],
        );
        write_lines(
            &root.join("-p/parent-1/subagents/agent-abc.jsonl"),
            &[assistant_text_line(&iso(now), "XYLOPHONE again, nested")],
        );
        let hist = dir.path().join("history.jsonl");
        write_lines(
            &hist,
            &[history_line(1700000000000, "fix the xylophone", "/home/u/p", "parent-1")],
        );

        let r = search(&root, &hist, "xylophone", 100);
        assert_eq!(r.hits.len(), 3, "history + top-level + subagent: {r:?}");
        assert!(!r.truncated);
        let sub = r
            .hits
            .iter()
            .find(|h| h.preview.contains("XYLOPHONE"))
            .expect("subagent hit");
        assert_eq!(sub.session_id, "parent-1", "subagent attributes to parent dir");
        assert_eq!(sub.source, SearchSource::Transcript);
        let hist_hit = r.hits.iter().find(|h| h.source == SearchSource::History).unwrap();
        assert_eq!(hist_hit.session_id, "parent-1");
        assert_eq!(hist_hit.line, 1);
        assert!(hist_hit.timestamp.is_some(), "history millis become a timestamp");
    }

    #[test]
    fn search_is_smart_cased_and_char_boundary_safe() {
        let dir = Scratch::new("search-smartcase");
        let root = dir.path().join("projects");
        let now = Utc::now();
        write_lines(
            &root.join("-p/s1.jsonl"),
            &[
                assistant_text_line(&iso(now), "Ünïcödé context 🎉 then NeedleWord appears"),
                user_line(&iso(now), "lowercase needleword here"),
            ],
        );
        let hist = dir.path().join("nonexistent-history.jsonl");

        // Lowercase query: case-insensitive, both lines hit; no panic on the
        // multibyte context, and the user hit is classified a prompt.
        let r = search(&root, &hist, "needleword", 100);
        assert_eq!(r.hits.len(), 2, "{r:?}");
        assert!(r.hits.iter().any(|h| h.source == SearchSource::Prompt && h.role == "user"));

        // Uppercase in the query: exact.
        let r = search(&root, &hist, "NeedleWord", 100);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].line, 1);
    }

    #[test]
    fn search_caps_per_file_and_in_total_and_skips_megabyte_lines() {
        let dir = Scratch::new("search-caps");
        let root = dir.path().join("projects");
        let now = Utc::now();
        let many: Vec<String> = (0..SEARCH_PER_FILE + 5)
            .map(|i| assistant_text_line(&iso(now), &format!("pelican number {i}")))
            .collect();
        write_lines(&root.join("-p/big.jsonl"), &many);
        // A >1MB line containing the needle must be skipped, not matched.
        let huge = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"pelican {}"}}]}}}}"#,
            "x".repeat(LINE_CAP + 10)
        );
        write_lines(&root.join("-p/huge.jsonl"), &[huge]);
        let hist = dir.path().join("none.jsonl");

        let r = search(&root, &hist, "pelican", 1000);
        assert_eq!(r.hits.len(), SEARCH_PER_FILE, "per-file cap holds");
        assert_eq!(r.files_truncated, 1);
        assert!(!r.truncated, "total cap not reached");

        let r = search(&root, &hist, "pelican", 5);
        assert_eq!(r.hits.len(), 5);
        assert!(r.truncated, "total cap reported");
    }

    // -- history.jsonl ------------------------------------------------------

    #[test]
    fn history_parses_both_pasted_variants_and_counts_malformed() {
        let dir = Scratch::new("history-parse");
        let path = dir.path().join("history.jsonl");
        write_lines(
            &path,
            &[
                r#"{"display":"inline paste","pastedContents":{"1":{"id":1,"type":"text","content":"the pasted text"}},"timestamp":1700000000000,"project":"/home/u/p","sessionId":"s1"}"#.to_string(),
                r#"{"display":"hash-only paste","pastedContents":{"1":{"id":1,"type":"text","contentHash":"deadbeef"}},"timestamp":1700000001000,"project":"/home/u/p","sessionId":"s2"}"#.to_string(),
                "not json at all".to_string(),
                r#"{"pastedContents":{},"timestamp":1700000002000,"project":"/p","sessionId":"s3"}"#.to_string(),
            ],
        );
        let h = history(&path);
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.malformed_lines, 2, "garbage and missing-display both count");
        assert_eq!(h.entries[0].pasted[0].content.as_deref(), Some("the pasted text"));
        assert!(h.entries[0].pasted[0].content_hash.is_none());
        assert_eq!(h.entries[1].pasted[0].content_hash.as_deref(), Some("deadbeef"));
        assert!(h.entries[1].pasted[0].content.is_none());

        // Missing file: empty, not an error.
        let h = history(&dir.path().join("absent.jsonl"));
        assert!(h.entries.is_empty());
        assert_eq!(h.malformed_lines, 0);
    }

    // -- digest (R-F3) ------------------------------------------------------

    #[test]
    fn digest_counts_only_the_named_local_day() {
        let dir = Scratch::new("digest-day");
        let root = dir.path().join("projects");
        let now = Utc::now();
        let other = now - chrono::Duration::days(3);
        let day = now.with_timezone(&Local).format("%Y-%m-%d").to_string();
        write_lines(
            &root.join("-p/s1.jsonl"),
            &[
                r#"{"type":"ai-title","aiTitle":"Fix the retry loop"}"#.to_string(),
                user_line(&iso(now), "please fix it"),
                tool_line(&iso(now), "Edit", "/w/src/a.rs"),
                format!(
                    r#"{{"type":"file-history-delta","timestamp":"{}","trackingPath":"/w/src/b.rs"}}"#,
                    iso(now)
                ),
                api_error_line(&iso(now), "overloaded_error"),
                // A different day: none of this may count.
                user_line(&iso(other), "old prompt"),
                tool_line(&iso(other), "Edit", "/w/src/old.rs"),
            ],
        );
        // Subagent activity folds into the parent as tool calls, not turns.
        write_lines(
            &root.join("-p/s1/subagents/agent-x.jsonl"),
            &[
                user_line(&iso(now), "task prompt from the parent"),
                tool_line(&iso(now), "Write", "/w/src/c.rs"),
            ],
        );

        let d = digest(&root, &day);
        assert_eq!(d.sessions.len(), 1, "{d:?}");
        let s = &d.sessions[0];
        assert_eq!(s.session_id, "s1");
        assert_eq!(s.title.as_deref(), Some("Fix the retry loop"));
        assert_eq!(s.repo, "alpha", "cwd basename");
        assert_eq!(s.turns, 1, "only the human prompt on the day; not the subagent's");
        assert_eq!(s.tool_calls, 2, "Edit on the day + subagent Write");
        assert_eq!(
            s.files_touched,
            vec!["/w/src/a.rs", "/w/src/b.rs", "/w/src/c.rs"],
            "edit input + trackingPath + subagent write, deduped and sorted; never old.rs"
        );
        assert_eq!(s.errors, 1);
        // The on-day parent tool line (out 3) + the subagent's (out 3); the
        // off-day tool line is excluded.
        assert_eq!(s.tokens_out, 6);

        let empty = digest(&root, "1999-01-01");
        assert!(empty.sessions.is_empty(), "no activity, no rows");
    }

    // -- recurring failures (R-F4) ------------------------------------------

    #[test]
    fn recurrence_needs_min_distinct_sessions() {
        let dir = Scratch::new("recur");
        let root = dir.path().join("projects");
        let now = Utc::now();
        write_lines(
            &root.join("-p/s1.jsonl"),
            &[
                tool_result_error(&iso(now), "error 404 at /a/b.rs line 12"),
                tool_result_error(&iso(now), "lonely failure only here"),
            ],
        );
        write_lines(
            &root.join("-p/s2.jsonl"),
            &[tool_result_error(&iso(now), "error 500 at /c/d.rs line 9")],
        );

        let r = recurring_failures(&root, 2);
        assert_eq!(r.len(), 1, "only the shape seen in two sessions: {r:?}");
        assert_eq!(r[0].normalized, "error # at <path> line #");
        assert_eq!(r[0].sessions, vec!["s1", "s2"]);
        assert_eq!(r[0].count, 2);
        assert!(
            r[0].example.contains("404") || r[0].example.contains("500"),
            "example stays verbatim: {}",
            r[0].example
        );
    }

    // -- prompt library (R-F6) ----------------------------------------------

    fn entry(ms: i64, display: &str, project: &str, session: &str) -> HistoryEntry {
        HistoryEntry {
            display: display.to_string(),
            timestamp_ms: ms,
            project: project.to_string(),
            session_id: session.to_string(),
            pasted: Vec::new(),
        }
    }

    #[test]
    fn prompts_cluster_by_exact_and_prefix() {
        let long_a = format!("{} tail one", "run the full verification suite and report".repeat(2));
        let long_b = format!("{} another tail", "run the full verification suite and report".repeat(2));
        let hist = vec![
            entry(1000, "Fix  the tests", "/p/a", "s1"),
            entry(2000, "fix the tests", "/p/b", "s2"),
            entry(3000, "something unrelated", "/p/a", "s1"),
            entry(4000, &long_a, "/p/a", "s3"),
            entry(5000, &long_b, "/p/a", "s3"),
        ];
        let clusters = prompt_library(&hist);
        assert_eq!(clusters.len(), 2, "{clusters:?}");
        // Both clusters have count 2; order then falls to last_used desc.
        assert_eq!(clusters[0].representative, long_b, "most recent verbatim wins");
        let exact = &clusters[1];
        assert_eq!(exact.representative, "fix the tests");
        assert_eq!(exact.count, 2);
        assert_eq!(exact.projects, vec!["/p/a", "/p/b"]);
        assert_eq!(exact.first_used.timestamp_millis(), 1000);
        assert_eq!(exact.last_used.timestamp_millis(), 2000);
    }

    // -- analytics (R-F5) ---------------------------------------------------

    #[test]
    fn analytics_buckets_days_hours_and_projects() {
        let dir = Scratch::new("analytics");
        let root = dir.path().join("projects");
        std::fs::create_dir_all(root.join("-p-a")).unwrap();
        let ms = 1_700_000_000_000i64;
        let hist = vec![
            entry(ms, "one", "/p/a", "s1"),
            entry(ms + 60_000, "two", "/p/a", "s1"),
            entry(ms + 120_000, "three", "/p/b", "s2"),
        ];
        let a = analytics(&hist, &root);

        let local = DateTime::from_timestamp_millis(ms).unwrap().with_timezone(&Local);
        let day = local.format("%Y-%m-%d").to_string();
        assert_eq!(a.prompts_per_day.len(), 1);
        assert_eq!(a.prompts_per_day[0].day, day);
        assert_eq!(a.prompts_per_day[0].count, 3);
        assert_eq!(a.sessions_per_day.len(), 1);
        assert_eq!(a.sessions_per_day[0].count, 2, "distinct sessions that day");
        assert_eq!(a.hour_histogram.iter().sum::<u32>(), 3);
        assert!(a.hour_histogram[local.hour() as usize] >= 1);
        assert_eq!(a.projects[0].project, "/p/a");
        assert_eq!(a.projects[0].prompts, 2);
        assert_eq!(a.projects[0].sessions, 1);
        assert!(a.projects[0].has_transcripts, "-p-a exists");
        assert!(!a.projects[1].has_transcripts, "/p/b has no transcript dir");
    }

    // -- subagent tree (R-F8) -----------------------------------------------

    #[test]
    fn subagent_tree_summarises_each_agent_file() {
        let dir = Scratch::new("subtree");
        let root = dir.path().join("projects");
        let t0 = Utc::now() - chrono::Duration::minutes(10);
        let t1 = Utc::now();
        write_lines(
            &root.join("-p/parent-9/subagents/agent-one.jsonl"),
            &[
                user_line(&iso(t0), "task"),
                tool_line(&iso(t0), "Edit", "/w/x.rs"),
                assistant_text_line(&iso(t1), "done with the refactor"),
            ],
        );
        write_lines(
            &root.join("-p/parent-9/subagents/agent-two.jsonl"),
            &[tool_line(&iso(t1), "Read", "/w/y.rs")],
        );

        let nodes = subagent_tree(&root, "parent-9");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].agent_id, "agent-one");
        assert_eq!(nodes[0].lines, 3);
        assert_eq!(nodes[0].tokens_out, 3 + 5);
        assert_eq!(nodes[0].last_activity.as_deref(), Some("done with the refactor"));
        assert!(nodes[0].first_ts.is_some() && nodes[0].last_ts.is_some());
        assert_eq!(nodes[1].last_activity.as_deref(), Some("Read: /w/y.rs"));

        assert!(subagent_tree(&root, "no-such-session").is_empty());
        assert!(subagent_tree(&root, "../escape").is_empty(), "path-shaped ids refused");
    }

    // -- decision candidates (R-F7) -----------------------------------------

    #[test]
    fn decision_patterns_match_on_word_boundaries() {
        let dir = Scratch::new("decisions");
        let path = dir.path().join("t.jsonl");
        let now = Utc::now();
        write_lines(
            &path,
            &[
                assistant_text_line(
                    &iso(now),
                    "We decided to use tokio here. The undecided naming question can wait. We went with axum instead of actix",
                ),
                // Duplicate sentence: deduped.
                assistant_text_line(&iso(now), "We decided to use tokio here."),
                user_line(&iso(now), "the user also decided things but user lines do not count"),
            ],
        );
        let c = decision_candidates(&path);
        assert_eq!(c.len(), 2, "{c:?}");
        assert_eq!(c[0].pattern, "decided");
        assert_eq!(c[0].text, "We decided to use tokio here");
        assert_eq!(c[0].line, 1);
        assert!(c[0].timestamp.is_some());
        // "instead of" outranks "went with" in pattern precedence.
        assert_eq!(c[1].pattern, "instead of");
        assert!(
            !c.iter().any(|d| d.text.contains("undecided")),
            "word boundary: 'undecided' is not a decision"
        );
    }

    // -- turns near a moment (R-F9) -----------------------------------------

    #[test]
    fn turns_near_returns_nearest_first() {
        let dir = Scratch::new("near");
        let path = dir.path().join("t.jsonl");
        let epoch = Utc::now();
        write_lines(
            &path,
            &[
                user_line(&iso(epoch - chrono::Duration::seconds(100)), "far before"),
                assistant_text_line(&iso(epoch - chrono::Duration::seconds(10)), "just before"),
                user_line(&iso(epoch + chrono::Duration::seconds(50)), "after"),
                // No timestamp: cannot be placed, skipped.
                r#"{"type":"user","message":{"role":"user","content":"timeless"}}"#.to_string(),
            ],
        );
        let t = turns_near(&path, epoch.timestamp(), 2);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].preview, "just before");
        assert_eq!(t[0].role, "assistant");
        assert_eq!(t[0].line, 2);
        assert_eq!(t[1].preview, "after");
        assert_eq!(t[1].role, "user");

        assert!(turns_near(&dir.path().join("absent.jsonl"), 0, 3).is_empty());
    }
}
