//! Ask the sandboxed agent one question from the terminal. `R-O14`.
//!
//! ```sh
//! cargo run -q -p mogeungd --bin ask -- "summarise https://example.com"
//! cargo run -q -p mogeungd --bin ask -- --probe     # what would answer, and nothing else
//! ```
//!
//! A program rather than a test for `judge`'s reason: it needs **this
//! machine's** `bwrap`, opencode and model, and `cargo test` must never start
//! an agent. It runs the code the Ask panel runs — `Model::agent_streaming`
//! over `askagent::probe` — against the same endpoint the daemon would use,
//! llmproxy included, and exits non-zero when nothing answered.

use mogeung_core::model::{ChatTurn, ModelSettings};
use mogeungd::model::Model;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = match mogeung_core::config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}\n{}", e.headline(), e.fix());
            std::process::exit(2);
        }
    };
    let enabled = cfg.ask_agent.unwrap_or(true);
    let ready = match mogeungd::askagent::probe(enabled) {
        Ok(r) => r,
        Err(u) => {
            eprintln!("no agent here: {}", u.note().unwrap_or_else(|| "`ask_agent = false`".into()));
            std::process::exit(1);
        }
    };
    println!("{}", ready.provenance());
    println!("  bwrap    {} (tmpfs sized: {})", ready.sandbox.bwrap, ready.sandbox.sized);
    println!("  opencode {}", ready.sandbox.opencode);
    if !ready.sandbox.keep.is_empty() {
        println!("  dns      {}", ready.sandbox.keep.join(", "));
    }
    if args.first().map(String::as_str) == Some("--probe") {
        return;
    }
    let question = args.join(" ");
    if question.trim().is_empty() {
        eprintln!("usage: ask <question>   or   ask --probe");
        std::process::exit(2);
    }

    let url = proxy_url(&cfg).or_else(|| cfg.model_url.clone());
    let model = Model::new();
    model.configure(ModelSettings {
        url,
        model: cfg.model_name.clone(),
        consent: cfg.allow_remote_model.clone(),
        embed_model: cfg.embed_model.clone(),
    });
    model.set_chat_allowed(true);
    if let Some(h) = model.health().and_then(|h| h.host) {
        println!("  model    {} at {h}", cfg.model_name.as_deref().unwrap_or("default"));
    }
    println!();

    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    let answer = rt.block_on(model.agent_streaming(
        "ask",
        &[ChatTurn::user(question)],
        true,
        &ready,
        |_| {},
        |s| println!("  {} {} {}", if s.ok { "✓" } else { "✗" }, s.tool, s.title),
    ));
    match answer {
        Ok(a) => {
            println!("\n{}\n\n— {} · {} · {:.1}s", a.text, a.agent, a.model, a.elapsed_ms as f64 / 1000.0);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// mogeung's own llmproxy, if configured and answering — `judge`'s rule, so
/// the harness asks what the panel would ask.
fn proxy_url(cfg: &mogeung_core::config::Config) -> Option<String> {
    if !cfg.llmproxy.unwrap_or(false) {
        return None;
    }
    let daemon_port = cfg
        .listen
        .as_deref()
        .and_then(|l| l.rsplit_once(':').and_then(|(_, p)| p.parse().ok()))
        .unwrap_or(7717u16);
    let settings = mogeung_core::llmproxy::ProxySettings { port: cfg.llmproxy_port, ..Default::default() };
    let port = settings.port_for(daemon_port);
    mogeungd::llmproxy::probe(port).then(|| settings.url_for(port))
}
