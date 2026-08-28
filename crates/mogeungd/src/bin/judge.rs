//! Is the model's reading order better than the keyword order? `R-O2`, `R-O3`.
//!
//! [A35](../../../../docs/product/assumptions.md) — *a local model's reading of
//! mogeung's own evidence is worth the screen space it takes* — carries the
//! whole of pillar `O` and has no evidence at all. [A3](../../../../docs/product/assumptions.md)
//! — *risk ordering puts the file that matters first* — has been `UNTESTED`
//! since the ledger was written, on evidence that reads, in full, *"ranked
//! `auth.rs` above a lockfile once, in a test"*.
//!
//! This is where both get an answer, and it runs **before** `R-O3` draws a
//! panel on the result. The doc rule: if an assumption is `UNTESTED`, the work
//! is to test it, not to build the feature.
//!
//! ```sh
//! cargo run -q -p mogeungd --bin judge                        # every session's repo
//! cargo run -q -p mogeungd --bin judge -- --repo .             # one, by path
//! cargo run -q -p mogeungd --bin judge -- --repo . --base HEAD~1
//! ```
//!
//! `--base` is what makes this runnable at all. Without it the harness only
//! has something to judge when a tree happens to be dirty, which is not a
//! corpus — it is whatever you left lying about. With it, every commit ever
//! made is a diff the model can be asked to order, and the answers are
//! comparable because the keyword order is computed the same way for each.
//! A session's own base is used when one is recorded.
//!
//! **It exits non-zero when the model is unreachable or answers with nothing**,
//! for `--bin sweep`'s reason: a broken setup that reads as *no findings* is
//! the failure that costs you a year. A harness that shrugs is worse than none.
//!
//! It writes no snapshot file. The corpus on this machine is the truth, and a
//! checked-in copy of a verdict would go stale on its own schedule.
//!
//! **It judges nothing itself.** The keyword order is `git::compute_change`'s
//! own, the same call the Changes view makes, so the two cannot come to
//! disagree about what the incumbent even is.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use mogeung_core::change::{Change, FileChange};
use mogeung_core::model::{ChatTurn, ModelSettings};
use mogeungd::{git, model::Model, store::Store};

/// How much of one file's diff the model is shown.
///
/// A cap and not the whole thing, and the number is the point of the harness
/// rather than a detail: the model must order the files from **less** than the
/// reader has, or it is not saving anybody the scroll it exists to save. Forty
/// lines is about a screen.
const DIFF_LINES_PER_FILE: usize = 40;
/// Files above this are not worth asking about — the reading guide is for the
/// 22-file diff where three matter, and a 400-file one is a different problem.
const MAX_FILES: usize = 60;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| -> Option<String> {
        args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
    };
    let only: Option<PathBuf> = flag("--repo").map(PathBuf::from);
    let base = flag("--base");

    let (cfg, warning) = mogeung_core::config::Config::load();
    if let Some(w) = warning {
        eprintln!("{w}");
    }

    // `(repo, base)`. A named base applies to every repo; otherwise each
    // session's own recorded base is used, which is the diff that session made
    // rather than whatever the tree looks like now.
    let repos: Vec<(PathBuf, Option<String>)> = match &only {
        Some(p) => vec![(p.clone(), base.clone())],
        None => repos_with_sessions()
            .into_iter()
            .map(|(p, b)| (p, base.clone().or(b)))
            .collect(),
    };
    if repos.is_empty() {
        eprintln!("no repositories found. Pass --repo <path>, or run a session first.");
        std::process::exit(1);
    }

    // The same endpoint the panel talks to, or the finding is about a model
    // nobody uses. When `llmproxy` is configured the daemon repoints the seam
    // at it (`R-O10`), so the harness looks for that proxy on the port the
    // daemon would have derived — and **reuses** it rather than starting one:
    // a measuring tool that leaves a daemon behind is not a measuring tool.
    let url = proxy_url(&cfg).or_else(|| cfg.model_url.clone());
    if let Some(u) = proxy_url(&cfg) {
        println!("asking mogeung's own proxy at {u}");
    }
    let model = Model::new();
    model.configure(ModelSettings {
        url,
        model: cfg.model_name.clone(),
        // The harness reads the same consent the daemon does; it does not widen
        // it. An endpoint you have not permitted is a refusal here too.
        consent: cfg.allow_remote_model.clone(),
    });
    model.set_chat_allowed(true);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let mut asked = 0usize;
    let mut answered = 0usize;
    let mut agreed_top = 0usize;
    let mut dropped = 0usize;
    let mut ranked = 0usize;
    let mut moved = Vec::new();

    for (repo, base) in &repos {
        let change = git::compute_change(repo, base.as_deref(), &HashSet::new());
        let files = readable(&change);
        if files.len() < 2 {
            continue;
        }
        asked += 1;
        println!("\n=== {} — {} file(s) ===", repo.display(), files.len());

        let keyword: Vec<&FileChange> = files.clone();
        let prompt = ask_for_order(&files);
        let reply = rt.block_on(model.chat(&[ChatTurn::user(prompt)]));

        let text = match reply {
            Ok(a) => {
                answered += 1;
                println!("({} · {} ms)", a.model, a.elapsed_ms);
                a.text
            }
            Err(e) => {
                println!("  model: {e}");
                continue;
            }
        };

        let order = parse_order(&text, &files);
        if order.is_empty() {
            println!("  the model named no file this diff contains");
            continue;
        }

        // The comparison, and the only number that matters: does the model put
        // a different file first? Agreement everywhere means `R-O3` is a second
        // ordering that reads like the first, which is screen space for nothing.
        // Files the model did not mention at all. This is not a detail: a
        // reading guide that silently drops a file hides it from the reader,
        // which is worse than ordering it badly. Measured here so `R-O3` knows
        // it must append the unranked rather than trust the list it was given.
        let omitted: Vec<&str> = files
            .iter()
            .map(|f| f.path.as_str())
            .filter(|p| !order.iter().any(|(o, _)| o == p))
            .collect();
        if !omitted.is_empty() {
            dropped += omitted.len();
            println!("  omitted {} file(s): {}", omitted.len(),
                omitted.iter().map(|p| short(p)).collect::<Vec<_>>().join(", "));
        }

        ranked += order.len();
        let kw_first = keyword[0].path.clone();
        let m_first = order[0].0.clone();
        if kw_first == m_first {
            agreed_top += 1;
        } else {
            moved.push((repo.display().to_string(), kw_first.clone(), m_first.clone()));
        }

        println!("  {:<44}   {}", "keyword order", "model order");
        for i in 0..keyword.len().max(order.len()).min(12) {
            let k = keyword.get(i).map(|f| {
                let flags: Vec<&str> = f.flags.iter().map(|fl| fl.label()).collect();
                format!("{} ({}){}", short(&f.path), f.score,
                    if flags.is_empty() { String::new() } else { format!(" {}", flags.join(",")) })
            }).unwrap_or_default();
            let m = order.get(i).map(|(p, why)| format!("{} — {why}", short(p))).unwrap_or_default();
            println!("  {k:<44}   {m}");
        }
    }

    println!("\n─────────────────────────────────────────────");
    println!("{asked} repo(s) with a diff, {answered} answered");
    if answered == 0 {
        eprintln!(
            "\nthe model answered nothing. That is the failure `--bin sweep` exists to make\n\
             loud: a harness that shrugs reads as 'no finding' and costs you a year."
        );
        std::process::exit(1);
    }
    println!("{agreed_top}/{answered} agreed on which file to read first");
    println!("{ranked} file(s) ranked, {dropped} omitted by the model");
    if dropped > 0 {
        println!(
            "  → `R-O3` must append the unranked rather than render the model's list as
                 the whole diff. A file the guide does not mention is a file it hides."
        );
    }
    for (repo, kw, m) in &moved {
        println!("  {repo}: keyword says {}, model says {}", short(kw), short(m));
    }
    println!(
        "\nA35 is answered by whether those disagreements are *better*, which is a\n\
         judgement and belongs in the roadmap row rather than in this output."
    );
}

/// The files worth asking about: the ones a reader would actually read.
///
/// `Noise` is dropped — lockfiles and generated code are already collapsed in
/// the Changes view, so including them would let the model win by re-deriving
/// a decision the keyword scorer has already made.
fn readable(change: &Change) -> Vec<&FileChange> {
    change
        .files
        .iter()
        .filter(|f| !f.truncated && !f.flags.iter().any(|fl| matches!(fl, mogeung_core::change::RiskFlag::Noise)))
        .take(MAX_FILES)
        .collect()
}

fn short(path: &str) -> String {
    let p = Path::new(path);
    let n = p.components().count();
    if n <= 3 {
        return path.to_string();
    }
    format!(".../{}", p.components().skip(n - 3).map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
}

/// The question, with a bounded excerpt of each file rather than the diff.
fn ask_for_order(files: &[&FileChange]) -> String {
    let mut s = String::from(
        "You are given the changed files of one commit-in-progress. Order them by what a \
         reviewer should read FIRST to understand what this change does.\n\n\
         Answer with one line per file, most important first, exactly:\n\
         <path> | <reason in at most 12 words>\n\n\
         Rank by what carries the change. Put mechanical edits last: renames, \
         formatting, generated files, and edits that only follow from another file.\n\n",
    );
    for f in files {
        s.push_str(&format!(
            "--- {} (+{} -{})\n",
            f.path, f.insertions, f.deletions
        ));
        let mut shown = 0;
        for h in &f.hunks {
            for line in h.lines.iter() {
                if shown >= DIFF_LINES_PER_FILE {
                    break;
                }
                s.push_str(line);
                s.push('\n');
                shown += 1;
            }
            if shown >= DIFF_LINES_PER_FILE {
                break;
            }
        }
        s.push('\n');
    }
    s
}

/// Read the model's order back, keeping only files this diff actually has.
///
/// A model that invents a path has not ordered *this* change, and silently
/// accepting one would let the harness report agreement it never measured.
pub fn parse_order(text: &str, files: &[&FileChange]) -> Vec<(String, String)> {
    let known: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['-', '*', '#', ' ']);
        let (left, why) = match line.split_once('|') {
            Some((l, r)) => (l.trim(), r.trim()),
            None => (line, ""),
        };
        // Substring rather than equality: a model returns `1. src/foo.rs` and
        // backticks about as often as it returns the bare path.
        let Some(hit) = known.iter().find(|k| left.contains(*k)) else {
            continue;
        };
        if seen.insert(hit.to_string()) {
            out.push((hit.to_string(), why.to_string()));
        }
    }
    out
}

/// mogeung's own llmproxy, if it is configured and actually answering.
///
/// `None` when it is off, or when nothing is on that port — in which case the
/// configured `model_url` is used and the run says what it asked, so a reader
/// is never left guessing which model produced the ordering below.
fn proxy_url(cfg: &mogeung_core::config::Config) -> Option<String> {
    if !cfg.llmproxy.unwrap_or(false) {
        return None;
    }
    let daemon_port = cfg
        .listen
        .as_deref()
        .and_then(|l| l.rsplit_once(':').and_then(|(_, p)| p.parse().ok()))
        .unwrap_or(7717u16);
    let settings = mogeung_core::llmproxy::ProxySettings {
        port: cfg.llmproxy_port,
        ..Default::default()
    };
    let port = settings.port_for(daemon_port);
    mogeungd::llmproxy::probe(port).then(|| settings.url_for(port))
}

/// Every repository a session on this machine has worked in.
fn repos_with_sessions() -> Vec<(PathBuf, Option<String>)> {
    let Ok(store) = Store::open(&mogeungd::server::default_db()) else {
        return Vec::new();
    };
    let Ok(sessions) = store.load_sessions() else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for s in sessions {
        let base = s.base_sha.clone().filter(|b| !b.is_empty());
        let root = s.repo_root.filter(|r| !r.is_empty()).unwrap_or(s.cwd);
        if root.is_empty() || !seen.insert(root.clone()) {
            continue;
        }
        let p = PathBuf::from(&root);
        if p.join(".git").exists() {
            out.push((p, base));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mogeung_core::change::FileStatus;

    fn file(path: &str) -> FileChange {
        FileChange {
            path: path.into(),
            old_path: None,
            status: FileStatus::Modified,
            insertions: 1,
            deletions: 0,
            hunks: Vec::new(),
            flags: Vec::new(),
            score: 0,
            truncated: false,
        }
    }

    /// The model answers in prose, not in a protocol. These are the shapes it
    /// actually returns, taken from real runs. `R-O2`.
    #[test]
    fn an_order_is_read_out_of_whatever_the_model_wrote() {
        let files = [file("src/model.rs"), file("src/wire.rs"), file("README.md")];
        let refs: Vec<&FileChange> = files.iter().collect();

        let text = "1. `src/wire.rs` | adds the chunk event
- src/model.rs — streams the reply
  README.md | notes it";
        let got = parse_order(text, &refs);
        assert_eq!(
            got.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["src/wire.rs", "src/model.rs", "README.md"],
            "numbering, bullets and backticks are all noise around the path"
        );
        assert_eq!(got[0].1, "adds the chunk event");
        // An em dash is not the separator; that line still ranks, with no reason.
        assert_eq!(got[1].1, "");
    }

    /// A model that invents a path has not ordered *this* change, and taking
    /// it would let the harness report agreement it never measured.
    #[test]
    fn a_file_this_diff_does_not_have_is_dropped() {
        let files = [file("src/model.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        let got = parse_order("1. src/imaginary.rs | invented
2. src/model.rs | real", &refs);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "src/model.rs");
    }

    /// Naming one file twice must not give it two places in the order — the
    /// second mention would silently push everything after it down one.
    #[test]
    fn a_file_named_twice_is_ranked_once() {
        let files = [file("a.rs"), file("b.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        let got = parse_order("a.rs | first
b.rs | second
a.rs | again", &refs);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "a.rs");
    }

    /// Nothing usable is not an empty order silently treated as agreement.
    #[test]
    fn prose_with_no_paths_orders_nothing() {
        let files = [file("a.rs")];
        let refs: Vec<&FileChange> = files.iter().collect();
        assert!(parse_order("I am sorry, I cannot help with that.", &refs).is_empty());
        assert!(parse_order("", &refs).is_empty());
    }
}
