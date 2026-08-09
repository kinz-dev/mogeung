//! What the formats look like **today**, against what the code claims to know.
//! `R-J28`.
//!
//! [A4](../../../../docs/product/assumptions.md) — *Claude Code's on-disk
//! formats are stable enough to depend on* — is `AT RISK` and always will be:
//! these are undocumented private files that change without warning.
//! Instrumentation cannot make them stable. What it can do is make a change
//! **loud**, and mogeung already has that half: the canary classifies every
//! line and an unknown type raises a health alert (`R-A1`).
//!
//! The gap this closes is *when you find out*. The canary only speaks from a
//! running daemon, so `R-J12` — a fourteenth Claude event type, in 47 of the 60
//! newest transcripts — was found by a **hand-written sweep** on a corpus that
//! had grown by 80 files since anyone last looked. A finding that depends on
//! somebody deciding to write a script is a finding you get once.
//!
//! So: one command, reading the same constants the daemon parses with, over
//! whatever corpus is on this machine.
//!
//! ```sh
//! cargo run -q -p mogeungd --bin sweep          # both corpora
//! cargo run -q -p mogeungd --bin sweep -- --quiet   # only what is unknown
//! ```
//!
//! **It exits non-zero when anything is unclassified**, so it can be a
//! pre-flight check after a CLI upgrade rather than a thing you read. `R-A2`
//! already notices when the Claude Code version moves; this is what to run
//! when it does.
//!
//! Two things it deliberately does *not* do. It does not write a snapshot
//! file: a checked-in inventory of a private format would be a second source of
//! truth that goes stale on its own schedule, and the corpus is the truth. And
//! it does not classify anything itself — every judgement comes from
//! `adapter::HANDLED`, `adapter::KNOWN_IGNORED`, `codex::HANDLED`,
//! `codex::KNOWN_IGNORED` and `codex::KNOWN_ITEMS`, because a sweep with its
//! own copy of those lists would eventually disagree with the parser and
//! report a clean bill of health for a daemon that is dropping lines.

use mogeungd::{adapter, codex, watcher};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One shape seen in the corpus: how often, and one example to look at.
#[derive(Default)]
struct Seen {
    count: u64,
    example: String,
}

#[derive(Default)]
struct Tally {
    known: BTreeMap<String, Seen>,
    unknown: BTreeMap<String, Seen>,
    /// Shapes *below* the type level — usage keys, content block types, models.
    /// Nothing to compare against, so they are an inventory rather than a
    /// verdict: their value is that re-running this and finding a new row is
    /// how the next `cache_creation` or `bridge-session` gets noticed.
    inventory: BTreeMap<String, BTreeMap<String, u64>>,
    files: u64,
    lines: u64,
    malformed: u64,
}

impl Tally {
    fn hit(&mut self, known: bool, name: String, line: &str) {
        let bucket = if known { &mut self.known } else { &mut self.unknown };
        let e = bucket.entry(name).or_default();
        e.count += 1;
        if e.example.is_empty() {
            e.example = line.chars().take(240).collect();
        }
    }

    fn note(&mut self, group: &str, item: impl Into<String>) {
        *self
            .inventory
            .entry(group.to_string())
            .or_default()
            .entry(item.into())
            .or_default() += 1;
    }
}

fn main() {
    let quiet = std::env::args().any(|a| a == "--quiet");

    let claude_home = watcher::default_home();
    let codex_home = codex::default_home();

    let claude = sweep_claude(&claude_home.join("projects"));
    let cdx = sweep_codex(&codex_home.join("sessions"));

    report("Claude Code", &claude_home, &claude, quiet);
    report("Codex", &codex_home, &cdx, quiet);

    let unknown = claude.unknown.len() + cdx.unknown.len();
    if unknown == 0 {
        println!("\nEverything on this machine is classified. Re-run after a CLI upgrade.");
        return;
    }
    println!(
        "\n{unknown} unclassified shape(s). Each is a deliberate decision: add it to the\n\
         parser's HANDLED list if it carries something, to KNOWN_IGNORED if it does not —\n\
         and add a line to tests/fixtures/corpus.jsonl either way, so the choice is pinned."
    );
    std::process::exit(1);
}

fn report(name: &str, home: &Path, t: &Tally, quiet: bool) {
    println!("\n=== {name} — {} ===", home.display());
    println!(
        "{} file(s), {} line(s), {} malformed",
        t.files, t.lines, t.malformed
    );

    if !t.unknown.is_empty() {
        println!("\n  UNCLASSIFIED — the parser drops these:");
        for (k, v) in &t.unknown {
            println!("    {:<44} {:>6}", k, v.count);
            println!("      e.g. {}", v.example);
        }
    }
    if quiet {
        return;
    }
    if !t.known.is_empty() {
        println!("\n  known:");
        for (k, v) in &t.known {
            println!("    {:<44} {:>6}", k, v.count);
        }
    }
    for (group, items) in &t.inventory {
        println!("\n  {group} (inventory — no list to check against):");
        for (k, n) in items {
            println!("    {:<44} {:>6}", k, n);
        }
    }
}

fn sweep_claude(projects: &Path) -> Tally {
    let mut t = Tally::default();
    for path in jsonl(projects) {
        t.files += 1;
        let Ok(body) = std::fs::read_to_string(&path) else { continue };
        for line in body.lines().filter(|l| !l.trim().is_empty()) {
            t.lines += 1;
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                t.malformed += 1;
                continue;
            };
            let Some(ty) = v.get("type").and_then(|x| x.as_str()) else {
                t.malformed += 1;
                continue;
            };
            let known =
                adapter::HANDLED.contains(&ty) || adapter::KNOWN_IGNORED.contains(&ty);
            t.hit(known, ty.to_string(), line);

            // Below the type: the shapes `R-J21` needed and nobody was
            // watching. A new usage key is how prompt caching arrived.
            if let Some(msg) = v.get("message") {
                if let Some(model) = msg.get("model").and_then(|m| m.as_str()) {
                    t.note("models", model);
                }
                if let Some(usage) = msg.get("usage").and_then(|u| u.as_object()) {
                    for k in usage.keys() {
                        t.note("usage keys", k.clone());
                    }
                }
                if let Some(blocks) = msg.get("content").and_then(|c| c.as_array()) {
                    for b in blocks {
                        if let Some(bt) = b.get("type").and_then(|x| x.as_str()) {
                            t.note("content blocks", bt);
                        }
                    }
                }
            }
        }
    }
    t
}

fn sweep_codex(sessions: &Path) -> Tally {
    let mut t = Tally::default();
    for path in jsonl(sessions) {
        t.files += 1;
        let Ok(body) = std::fs::read_to_string(&path) else { continue };
        for line in body.lines().filter(|l| !l.trim().is_empty()) {
            t.lines += 1;
            let Ok(v) = serde_json::from_str::<Value>(line) else {
                t.malformed += 1;
                continue;
            };
            let Some(ty) = v.get("type").and_then(|x| x.as_str()) else {
                t.malformed += 1;
                continue;
            };
            // Codex nests: the inner `payload.type` is where its taxonomy
            // actually lives, and drift there is exactly as consequential as
            // drift in the outer one — so it is named `outer/inner`, the same
            // compound the parser reports.
            let inner = v
                .get("payload")
                .and_then(|p| p.get("type"))
                .and_then(|x| x.as_str());
            match inner {
                Some(item) => {
                    let known = codex::HANDLED.contains(&ty) && codex::KNOWN_ITEMS.contains(&item);
                    t.hit(known, format!("{ty}/{item}"), line);
                }
                None => {
                    let known =
                        codex::HANDLED.contains(&ty) || codex::KNOWN_IGNORED.contains(&ty);
                    t.hit(known, ty.to_string(), line);
                }
            }
        }
    }
    t
}

/// Every `.jsonl` under a root, depth-capped like the usage scanner's walk —
/// a runaway symlink must not turn a sweep into a filesystem crawl.
fn jsonl(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: u8) {
        if depth > 6 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                walk(&p, out, depth + 1);
            } else if ft.is_file() && p.extension().is_some_and(|x| x == "jsonl") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out, 0);
    out
}
