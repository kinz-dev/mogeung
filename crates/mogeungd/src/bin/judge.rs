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
//! cargo run -q -p mogeungd --bin judge -- --recall             # A38: embeddings vs grep
//! ```
//!
//! ## `--recall`: the other half, and the other assumption (`R-O6`, `A38`)
//!
//! [A38](../../../../docs/product/assumptions.md) — *embeddings find what
//! substring search missed, often enough to earn a second list* — is the bet
//! under `R-O6`, and [feature 0017](../../../../docs/features/0017-cross-session.md)
//! deferred semantic search with a sequence rather than a refusal: *honest
//! substring search first*. That condition is met, so this measures whether the
//! second list would earn its place before one is drawn.
//!
//! **The ground truth is generated, not curated.** A checked-in query set would
//! measure this machine's corpus against somebody's guesses about it. Instead
//! the harness picks real lines from the corpus, asks the model to **paraphrase
//! each into a query that shares as few of its distinctive words as possible**,
//! and then asks both engines to find the original from the paraphrase. That is
//! `A38`'s claim stated as an experiment: a paraphrase is exactly the query
//! substring search cannot serve, and if embeddings do not win here they will
//! not win anywhere.
//!
//! Both directions are reported, because the assumption's removal condition
//! names both: hits grep missed, **and** hits grep found that the index ranked
//! away. A second list that loses what the first one had is not an addition.
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

use mogeung_core::change::FileChange;
use mogeung_core::model::{ChatTurn, ModelSettings};
use mogeungd::{git, guide, model::Model, store::Store};


fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| -> Option<String> {
        args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
    };
    let only: Option<PathBuf> = flag("--repo").map(PathBuf::from);
    let base = flag("--base");
    let recall = args.iter().any(|a| a == "--recall");

    let (cfg, warning) = mogeung_core::config::Config::load();
    if let Some(w) = warning {
        eprintln!("{w}");
    }

    if recall {
        recall::run(&cfg, &args);
        return;
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
        embed_model: cfg.embed_model.clone(),
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
        let files = guide::askable(&change.files);
        if files.len() < 2 {
            continue;
        }
        asked += 1;
        println!("\n=== {} — {} file(s) ===", repo.display(), files.len());

        let keyword: Vec<&FileChange> = files.clone();
        let prompt = guide::prompt(&files);
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

        let order = guide::parse_order(&text, &files);
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

fn short(path: &str) -> String {
    let p = Path::new(path);
    let n = p.components().count();
    if n <= 3 {
        return path.to_string();
    }
    format!(".../{}", p.components().skip(n - 3).map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
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


/// `A38`'s measurement: does a semantic list find what grep could not? `R-O6`.
mod recall {
    /// `split_whitespace` with the byte offsets it throws away — what makes a
    /// literal query a real slice of the text rather than a reconstruction.
    trait WithOffsets {
        fn split_whitespace_indices(&self) -> Vec<(usize, &str)>;
    }
    impl WithOffsets for str {
        fn split_whitespace_indices(&self) -> Vec<(usize, &str)> {
            self.split_whitespace()
                .map(|w| (w.as_ptr() as usize - self.as_ptr() as usize, w))
                .collect()
        }
    }

    use mogeung_core::config::Config;
    use mogeung_core::model::{ChatTurn, ModelSettings};
    use mogeungd::{embed, model::Model};
    use mogeung_core::insight;
    use mogeungd::insight as scan;

    /// How many corpus lines are embedded. The pool the originals hide in — a
    /// recall number against ten distractors is a number about nothing.
    const POOL: usize = 400;
    /// How many of those become questions.
    const QUERIES: usize = 12;
    /// A hit inside this many results counts as found, for both engines. The
    /// number is the design: a second list nobody scrolls is a list that has to
    /// be right near the top.
    const TOP_K: usize = 5;

    pub fn run(cfg: &Config, args: &[String]) {
        let flag = |name: &str| -> Option<String> {
            args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
        };
        let pool: usize = flag("--pool").and_then(|v| v.parse().ok()).unwrap_or(POOL);
        let queries: usize = flag("--queries").and_then(|v| v.parse().ok()).unwrap_or(QUERIES);
        // `--embed-model` exists so this can be measured **before** the key is
        // in anybody's config file, which matters more than it sounds: `Config`
        // is `deny_unknown_fields`, so writing `embed_model` into
        // `~/.mogeung/config.toml` stops an older installed daemon from parsing
        // it at all. A harness that requires you to break the running product
        // before it will tell you whether the feature is worth building is not
        // a harness anybody runs.
        let embed_model = flag("--embed-model").or_else(|| cfg.embed_model.clone());
        if embed_model.is_none() {
            eprintln!(
                "no embedding model: pass `--embed-model <id>` (as the endpoint's own \
                 /models lists it), or set `embed_model` in ~/.mogeung/config.toml \
                 once this build is installed. `--bin sweep`'s rule — a harness that \
                 shrugs reads as 'no finding'."
            );
            std::process::exit(1);
        }

        // The same home the watcher reads, so the harness and the daemon are
        // looking at one corpus.
        let home = mogeungd::watcher::default_home();
        let projects = home.join("projects");
        let history = home.join("history.jsonl");


        // **Two endpoints, on purpose.** The paraphrases are chat and go where
        // every other question goes — mogeung's own llmproxy when one is
        // configured, which is also the only thing that understands
        // `model_name = "Auto"`. The embeddings go to `model_url` itself:
        // llmproxy routes *chat*, and asking a chat route for vectors reports
        // an endpoint mistake as a recall failure. Found by running it.
        let model = Model::new();
        model.configure(ModelSettings {
            url: super::proxy_url(cfg).or_else(|| cfg.model_url.clone()),
            model: cfg.model_name.clone(),
            consent: cfg.allow_remote_model.clone(),
            embed_model: None,
        });
        model.set_chat_allowed(true);
        let embed_settings = ModelSettings {
            url: cfg.model_url.clone(),
            model: None,
            consent: cfg.allow_remote_model.clone(),
            embed_model,
        };

        let corpus = scan::corpus_lines(&projects, &history, pool);
        if corpus.len() < queries * 2 {
            eprintln!(
                "only {} corpus line(s) — not enough to hide {queries} answers in. \
                 Run some sessions first.",
                corpus.len()
            );
            std::process::exit(1);
        }
        println!("{} line(s) from {}", corpus.len(), projects.display());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        let texts: Vec<String> = corpus.iter().map(|c| c.text.clone()).collect();
        let vectors = match rt.block_on(embed::embed(&embed_settings, &texts)) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("the embedding endpoint answered nothing: {e}");
                std::process::exit(1);
            }
        };

        // The questions: spread across the pool, so they are not all from one
        // session, and long enough to be worth paraphrasing.
        let step = (corpus.len() / queries.max(1)).max(1);
        let chosen: Vec<usize> = (0..corpus.len())
            .step_by(step)
            .filter(|i| corpus[*i].text.chars().count() > 60)
            .take(queries)
            .collect();

        let mut asked = 0usize;
        let mut grep_found = 0usize;
        let mut semantic_found = 0usize;
        let mut both = 0usize;
        let mut neither = 0usize;
        let mut semantic_only = 0usize;
        let mut grep_only = 0usize;
        let mut literal_asked = 0usize;
        let mut literal_grep = 0usize;
        let mut literal_semantic = 0usize;
        let mut ranked_away = 0usize;

        for &i in &chosen {
            let original = &corpus[i];
            let ask = format!(
                "Rewrite the following as a short search query someone might type when \
                 trying to find it again — one line, at most ten words. Use **different \
                 words**: no distinctive term from the text may appear in your query. \
                 Answer with the query and nothing else.\n\n{}",
                original.text.chars().take(600).collect::<String>()
            );
            let paraphrase = match rt.block_on(model.chat(&[ChatTurn::user(ask)])) {
                Ok(a) => a.text.trim().trim_matches('"').to_string(),
                Err(e) => {
                    println!("  model: {e}");
                    continue;
                }
            };
            if paraphrase.is_empty() {
                continue;
            }
            asked += 1;

            // Grep: the incumbent, called exactly as the search panel calls it.
            let hits = scan::search(&projects, &history, &paraphrase, 50);
            // Matched on the pair a hit actually carries — its session and its
            // line — rather than on a path, which `SearchHit` deliberately does
            // not expose. A history line is identified by its source instead,
            // there being one such file.
            // `insight`'s own rule, not a re-derivation: a subagent transcript
            // attributes to its **parent** session, so the file stem is the
            // wrong key for one — which showed up as grep missing substrings of
            // its own corpus.
            let session = scan::session_id_of(&original.path);
            let from_history = original.role == "history";
            let by_grep = hits.hits.iter().any(|h| {
                h.line == original.line
                    && if from_history {
                        h.source == insight::SearchSource::History
                    } else {
                        h.session_id == session
                    }
            });

            // Semantic: the challenger, over the same corpus.
            let qv = match rt.block_on(embed::embed(&embed_settings, &[paraphrase.clone()])) {
                Ok(v) => v.into_iter().next().unwrap_or_default(),
                Err(e) => {
                    println!("  embedding: {e}");
                    continue;
                }
            };
            let near = embed::nearest(&qv, &vectors, TOP_K);
            let rank = near.iter().position(|(j, _)| *j == i);
            let by_semantic = rank.is_some();

            grep_found += by_grep as usize;
            semantic_found += by_semantic as usize;
            match (by_grep, by_semantic) {
                (true, true) => both += 1,
                (false, true) => semantic_only += 1,
                (true, false) => grep_only += 1,
                (false, false) => neither += 1,
            }

            // **The other direction, and it is not optional.** A paraphrase
            // shares no distinctive words by construction, so grep must score
            // zero on it — which means the paraphrase half cannot measure what
            // a second list would *lose*. A literal query can: words lifted
            // straight out of the line, which grep is guaranteed to find. If
            // the index ranks those away, the second list is subtracting.
            let literal = words_from(&original.text);
            if !literal.is_empty() {
                literal_asked += 1;
                let lit_hits = scan::search(&projects, &history, &literal, 50);
                let lit_grep = lit_hits.hits.iter().any(|h| {
                    h.line == original.line
                        && if from_history {
                            h.source == insight::SearchSource::History
                        } else {
                            h.session_id == session
                        }
                });
                let lit_semantic = match rt.block_on(embed::embed(&embed_settings, &[literal.clone()])) {
                    Ok(v) => {
                        let qv = v.into_iter().next().unwrap_or_default();
                        embed::nearest(&qv, &vectors, TOP_K).iter().any(|(j, _)| *j == i)
                    }
                    Err(_) => false,
                };
                literal_grep += lit_grep as usize;
                literal_semantic += lit_semantic as usize;
                if !lit_grep {
                    // Grep failing to find a substring of the corpus is a
                    // harness fault, not a finding — printed so it can never be
                    // read as one.
                    println!(
                        "  literal \"{}\" — grep missed its own text ({}), which is this \
                         harness, not search",
                        clip(&literal, 50),
                        original.role
                    );
                }
                if lit_grep && !lit_semantic {
                    ranked_away += 1;
                    println!("  literal \"{}\" — grep found it, semantic did not", clip(&literal, 60));
                }
            }

            println!(
                "\n\"{}\"\n  looking for: {}",
                clip(&paraphrase, 90),
                clip(&original.text, 90)
            );
            println!(
                "  grep {:<9} semantic {}",
                if by_grep { "found" } else { "missed" },
                match rank {
                    Some(r) => format!("found at {}", r + 1),
                    None => "missed".to_string(),
                }
            );
        }

        println!("\n─────────────────────────────────────────────");
        if asked == 0 {
            eprintln!(
                "nothing was asked — the model paraphrased nothing. That is the failure\n\
                 `--bin sweep` exists to make loud rather than a finding about recall."
            );
            std::process::exit(1);
        }
        println!("{asked} paraphrased quer{} against {} embedded line(s), top {TOP_K}",
            if asked == 1 { "y" } else { "ies" }, corpus.len());
        println!("grep found     {grep_found}/{asked}");
        println!("semantic found {semantic_found}/{asked}");
        println!("  both {both} · semantic only {semantic_only} · grep only {grep_only} · neither {neither}");
        println!(
            "\nliteral queries (words lifted from the line, which grep must find):\n\
             grep {literal_grep}/{literal_asked} · semantic {literal_semantic}/{literal_asked} \
             · ranked away by the index {ranked_away}"
        );
        println!(
            "\nA38 turns on two numbers and they are in different blocks. **semantic\n\
             only**, above, is what a second list would add. **ranked away**, below, is\n\
             what it would cost — a list that loses what grep already found is not an\n\
             addition, and the paraphrase block cannot see that because grep scores zero\n\
             there by construction. The judgement belongs in the roadmap row rather than\n\
             in this output."
        );
    }

    /// A literal query: a **contiguous slice** of one line of the text.
    ///
    /// Contiguous, and sliced out of the original rather than rebuilt from
    /// words, because the first version did neither and scored grep 0/11 —
    /// which is impossible for a substring query and was therefore a bug in the
    /// harness rather than a finding. Two causes, both worth naming: short
    /// words were filtered out before joining, so the "quote" never existed in
    /// the text; and the join used single spaces, which a line with a newline
    /// or a double space in it does not have.
    ///
    /// Taken from one line and from its middle: the start of an assistant turn
    /// is often boilerplate (*"I'll start by…"*) that matches a hundred other
    /// lines, and a query that matches everything measures nothing.
    fn words_from(text: &str) -> String {
        let line = text
            .lines()
            .filter(|l| l.split_whitespace().count() >= 10)
            .max_by_key(|l| l.len())
            .unwrap_or("");
        let words: Vec<(usize, &str)> = line.split_whitespace_indices();
        if words.len() < 10 {
            return String::new();
        }
        // The slice has to survive **JSON encoding**, because `search` matches
        // the raw `.jsonl` line and the corpus text is the parsed one. A quote,
        // a backslash or a non-ASCII character is escaped on disk, so a slice
        // containing one is a query that cannot match a line that does contain
        // it — which showed up as grep scoring 7/11 on substrings of itself.
        let quotable = |w: &str| w.is_ascii() && !w.contains(['"', '\\']);
        for first in (words.len() / 3)..words.len().saturating_sub(5) {
            let last = first + 5;
            if !words[first..=last].iter().all(|(_, w)| quotable(w)) {
                continue;
            }
            let start = words[first].0;
            let end = words[last].0 + words[last].1.len();
            let slice = &line[start..end];
            // Single spaces only: any other run of whitespace is a different
            // string on disk than the one this slice reads as.
            if slice.split(' ').all(|w| !w.is_empty()) {
                return slice.to_string();
            }
        }
        String::new()
    }

    fn clip(s: &str, n: usize) -> String {
        let one = s.replace('\n', " ");
        if one.chars().count() <= n {
            return one;
        }
        format!("{}…", one.chars().take(n).collect::<String>())
    }
}
