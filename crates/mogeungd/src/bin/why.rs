//! Is the reason for a change actually in the transcript, and which end of the
//! conversation is it at? `R-O4`'s gate, and [A36](../../../../docs/product/assumptions.md)'s test.
//!
//! `A36` — *the rationale for a change is in the transcript that produced it,
//! and is reachable from the line* — is `UNTESTED`, and under the doc rule the
//! first work is to test it rather than to build the panel on top of it. This
//! is `--bin judge`'s shape applied to the next row: run over whatever corpus
//! is on this machine, print numbers a human reads, and change nothing.
//!
//! ```sh
//! cargo run -q -p mogeungd --bin why                       # a spread of this machine's sessions
//! cargo run -q -p mogeungd --bin why -- --limit 20
//! cargo run -q -p mogeungd --bin why -- --session <id>
//! cargo run -q -p mogeungd --bin why -- --question "what was the reviewer worried about?"
//! ```
//!
//! **It asks the same question twice, through two retrievals**, because the
//! doubt written into `A36` is not *is the reason there* but *are we looking at
//! the wrong end of the conversation*: an assistant narrates what it did far
//! more often than why, and the why is usually in the human's prompt several
//! turns earlier. `nearest-in-time` is what `R-F9`'s machinery gives today and
//! what `R-O4` would inherit by accident; `leading-up` is the shape the doubt
//! implies. The corpus picks.
//!
//! **It exits non-zero when the model is unreachable or answers nothing**,
//! which is `--bin sweep`'s rule and `--bin judge`'s: a broken setup that reads
//! as *no finding* is the failure that costs you a year.
//!
//! Three numbers come out, and only the third settles anything:
//!
//! | | |
//! | --- | --- |
//! | **found** | the turns contained a reason at all — `A36`'s first half |
//! | **cited** | the answer named lines a reader can open — provenance, not prose |
//! | **user vs assistant** | which end of the conversation the reason was at — the doubt |

use std::path::PathBuf;

use mogeung_core::model::{ChatTurn, ModelSettings};
use mogeungd::{
    model::Model,
    store::Store,
    why::{self, Shape},
};

/// How many turns each retrieval is allowed. Enough for a prompt and the
/// answer it provoked; not so many that both shapes converge on *the whole
/// session*, which would measure nothing.
const TURNS: usize = 6;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| -> Option<String> {
        args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
    };
    let limit: usize = flag("--limit").and_then(|v| v.parse().ok()).unwrap_or(12);
    let per_session: usize = flag("--per-session").and_then(|v| v.parse().ok()).unwrap_or(2);
    let only_session = flag("--session");
    // A harness whose retrieval you cannot see is one you have to believe.
    // `--show` prints which turns each shape actually handed the model, which
    // is how *0 of 6* is told apart from a retrieval that returned nothing.
    let show = args.iter().any(|a| a == "--show");
    let question = flag("--question")
        .unwrap_or_else(|| "Why was this file changed at this point?".to_string());

    let (cfg, warning) = mogeung_core::config::Config::load();
    if let Some(w) = warning {
        eprintln!("{w}");
    }

    let moments = corpus(only_session.as_deref(), limit, per_session);
    if moments.is_empty() {
        eprintln!(
            "no edits found in any transcript on this machine. Run a session that changes a file first."
        );
        std::process::exit(1);
    }

    // The same endpoint the panel would talk to, proxy included — a harness
    // grading a different model than the feature uses is measuring a feature
    // that does not exist. `--bin judge`'s rule, and its code.
    let url = proxy_url(&cfg).or_else(|| cfg.model_url.clone());
    if let Some(u) = proxy_url(&cfg) {
        println!("asking mogeung's own proxy at {u}");
    }
    let model = Model::new();
    model.configure(ModelSettings {
        url,
        model: cfg.model_name.clone(),
        consent: cfg.allow_remote_model.clone(),
    });
    model.set_chat_allowed(true);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let shapes = [Shape::Nearest, Shape::LeadingUp];
    let mut asked = 0usize;
    let mut answered = [0usize; 2];
    let mut found = [0usize; 2];
    let mut cited = [0usize; 2];
    let mut from_user = [0usize; 2];
    let mut from_assistant = [0usize; 2];
    // The category `A36` actually warns about: an answer that reads as a
    // reason but rests **only** on the assistant's own narration. Those are
    // the confidently-wrong ones — *the file was changed because the assistant
    // then wrote the file* is a sentence, not a rationale.
    let mut narration_only = [0usize; 2];
    // Replies with no `REASON:` label. The first run found a proxy's own
    // routing classification in the body, and counting that as a reason would
    // inflate the number this harness exists to report.
    let mut unformed = [0usize; 2];
    let mut disagreed = 0usize;

    for (label, transcript, at) in &moments {
        println!(
            "\n=== {label} — {} (line {}, {})",
            short(&at.path),
            at.line,
            at.timestamp.format("%Y-%m-%d %H:%M")
        );
        asked += 1;
        let mut verdicts = [None, None];

        for (i, shape) in shapes.iter().enumerate() {
            let turns = why::turns_for(transcript, at, *shape, TURNS);
            if turns.is_empty() {
                println!("  {:<15} no turns at all", shape.label());
                continue;
            }
            if show {
                println!(
                    "  {:<15} turns {}",
                    shape.label(),
                    turns
                        .iter()
                        .map(|t| format!("{}:{}", t.line, &t.role[..1]))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                for t in &turns {
                    println!("  {:<15}   {} {}", "", t.line, clip(&t.text, 110));
                }
            }
            let prompt = why::prompt(question.trim(), &at.path, &turns);
            let reply = rt.block_on(model.chat(&[ChatTurn::user(prompt)]));
            let text = match reply {
                Ok(a) => {
                    answered[i] += 1;
                    a.text
                }
                Err(e) => {
                    println!("  {:<15} model: {e}", shape.label());
                    continue;
                }
            };
            let a = why::parse_answer(&text, &turns);
            let (u, s) = why::cited_roles(&a, &turns);
            from_user[i] += u;
            from_assistant[i] += s;
            if !a.cites.is_empty() {
                cited[i] += 1;
                if u == 0 && !a.no_reason {
                    narration_only[i] += 1;
                }
            }
            if a.unformed {
                unformed[i] += 1;
            }
            if a.no_reason {
                println!("  {:<15} no reason in these turns", shape.label());
            } else {
                found[i] += 1;
                println!(
                    "  {:<15} {}",
                    shape.label(),
                    clip(&a.reason, 150)
                );
                if !a.cites.is_empty() {
                    println!(
                        "  {:<15}   cites {} ({} from the human, {} from the assistant)",
                        "",
                        a.cites.iter().map(u64::to_string).collect::<Vec<_>>().join(", "),
                        u,
                        s
                    );
                }
            }
            verdicts[i] = Some(!a.no_reason);
        }
        if let (Some(a), Some(b)) = (verdicts[0], verdicts[1]) {
            if a != b {
                disagreed += 1;
            }
        }
    }

    println!("\n─────────────────────────────────────────────");
    println!("{asked} edit moment(s) asked about, {} turns each", TURNS);
    if answered.iter().sum::<usize>() == 0 {
        eprintln!(
            "\nthe model answered nothing. That is the failure `--bin sweep` exists to make\n\
             loud: a harness that shrugs reads as 'no finding' and costs you a year."
        );
        std::process::exit(1);
    }
    for (i, shape) in shapes.iter().enumerate() {
        println!(
            "{:<15} {}/{} answered · {} found a reason · {} cited a line",
            shape.label(),
            answered[i],
            asked,
            found[i],
            cited[i]
        );
        println!(
            "{:<15}   citations: {} human turn(s), {} assistant turn(s)",
            "", from_user[i], from_assistant[i]
        );
        println!(
            "{:<15}   {} answer(s) rested on the assistant's narration alone",
            "", narration_only[i]
        );
        println!(
            "{:<15}   {} reply/replies carried no REASON: label at all",
            "", unformed[i]
        );
    }
    println!("{disagreed} moment(s) where the two retrievals disagreed about whether a reason exists");
    println!(
        "\nRun it twice before believing a close result: the same moment can be\n\
         answered *no reason* on one run and narrated on the next. It is the gap\n\
         between the shapes that is stable, not either number alone."
    );
    println!(
        "\nA36 is answered by the *found* row: if neither shape finds a reason, the\n\
         rationale is not in the transcript and `R-O4` is answering from the code\n\
         alone. If one shape finds it and the other does not, that is a retrieval\n\
         bug wearing an assumption's clothes — which is what this run exists to\n\
         tell apart, and the judgement belongs in the roadmap row rather than here."
    );
}

/// A spread of this machine's edit moments, newest session first.
///
/// Spread rather than *the first N*: the first edits of a session are
/// scaffolding, and a harness that only ever sees the top of a transcript is
/// measuring how sessions start.
fn corpus(
    only: Option<&str>,
    limit: usize,
    per_session: usize,
) -> Vec<(String, PathBuf, why::EditMoment)> {
    let Ok(store) = Store::open(&mogeungd::server::default_db()) else {
        return Vec::new();
    };
    let Ok(mut sessions) = store.load_sessions() else {
        return Vec::new();
    };
    sessions.sort_by(|a, b| b.last_event_at.cmp(&a.last_event_at));

    let mut out = Vec::new();
    for s in sessions {
        if out.len() >= limit {
            break;
        }
        if let Some(want) = only {
            if s.id != want {
                continue;
            }
        }
        let path = PathBuf::from(&s.transcript_path);
        if !path.exists() {
            continue;
        }
        let moments = why::edit_moments(&path, None);
        if moments.is_empty() {
            continue;
        }
        let step = (moments.len() / per_session.max(1)).max(1);
        for m in moments.into_iter().step_by(step).take(per_session) {
            if out.len() >= limit {
                break;
            }
            out.push((s.label(), path.clone(), m));
        }
    }
    out
}

fn short(path: &str) -> String {
    let p = std::path::Path::new(path);
    let n = p.components().count();
    if n <= 3 {
        return path.to_string();
    }
    format!(
        ".../{}",
        p.components()
            .skip(n - 3)
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    )
}

fn clip(s: &str, n: usize) -> String {
    let one = s.replace('\n', " ");
    if one.chars().count() <= n {
        return one;
    }
    format!("{}…", one.chars().take(n).collect::<String>())
}

/// mogeung's own llmproxy, if it is configured and actually answering.
/// `--bin judge`'s, unchanged — two harnesses that resolve the endpoint
/// differently are grading two different models.
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
