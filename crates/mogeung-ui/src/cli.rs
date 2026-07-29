//! `mogeung <subcommand>` — the daemon's REST API from a terminal. `R-J4`.
//!
//! The window is still what `mogeung` with no arguments does. A leading bare
//! word means you wanted an answer on stdout instead, and nothing else about
//! the process changes: no window opens, no daemon is started, nothing is
//! written. These read a daemon that is already running, or say so.
//!
//! ## Six, out of forty
//!
//! The daemon serves about forty endpoints and this wraps six. A wrapper per
//! endpoint would be a worse tool, not a more complete one — the useful thing
//! in a terminal is the handful you would otherwise open the window to see, and
//! `--json` exists for everything else you might want to pipe.
//!
//! ## Why `curl`
//!
//! Same reasoning as the push notifier (`mogeungd/src/notify.rs`): one request
//! on a rare event does not justify an HTTP client dependency, and shelling out
//! cannot poison anything. The cost is an error message when curl is absent,
//! which is a fair trade for a subcommand nobody is required to use.

use serde_json::Value;

/// What the argument list turned out to mean.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// No subcommand: open the window, as always.
    Window,
    Run(Cmd, Opts),
    /// Print this and exit with the code.
    Say(String, i32),
}

#[derive(Debug, PartialEq, Clone)]
pub enum Cmd {
    Queue,
    Sessions,
    Health,
    Rescan,
    Diff(String),
    Search(String),
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct Opts {
    pub json: bool,
    pub addr: Option<String>,
    pub token: Option<String>,
}

const USAGE: &str = "\
mogeung <subcommand> — read a running daemon from the terminal

  queue              what needs you, ranked
  sessions           every session the daemon knows about
  health             what mogeung can and cannot see
  rescan             scan now instead of waiting for the next poll
  diff <session>     that session's changes, as patch text
  search <query>     across transcripts and prompt history

  --json             the daemon's own response, unformatted
  --addr HOST:PORT   a daemon elsewhere (default 127.0.0.1:7717)
  --token TOKEN      shared token, if the daemon requires one

With no subcommand, mogeung opens its window; run `mogeung --help` for that.";

/// Classify the argument list. Pure, so every rule below is testable without
/// a daemon, a terminal or a window.
pub fn parse<I: IntoIterator<Item = String>>(argv: I) -> Outcome {
    let argv: Vec<String> = argv.into_iter().collect();
    let Some(first) = argv.first() else {
        return Outcome::Window;
    };
    // A flag first means the window's own arguments; nothing here applies.
    if first.starts_with('-') {
        return Outcome::Window;
    }

    let mut opts = Opts::default();
    let mut rest: Vec<String> = Vec::new();
    let mut it = argv[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => opts.json = true,
            "--addr" => opts.addr = it.next().cloned(),
            "--token" => opts.token = it.next().cloned(),
            other => rest.push(other.to_string()),
        }
    }

    let needs_argument = |what: &str| {
        Outcome::Say(
            format!("mogeung {}: needs a {what}\n\n{USAGE}", first),
            2,
        )
    };
    match first.as_str() {
        "queue" => Outcome::Run(Cmd::Queue, opts),
        "sessions" => Outcome::Run(Cmd::Sessions, opts),
        "health" => Outcome::Run(Cmd::Health, opts),
        "rescan" => Outcome::Run(Cmd::Rescan, opts),
        "diff" => match rest.first() {
            Some(id) => Outcome::Run(Cmd::Diff(id.clone()), opts),
            None => needs_argument("session id"),
        },
        "search" => match rest.first() {
            Some(q) => Outcome::Run(Cmd::Search(rest.join(" ").trim().to_string()), opts)
                .tap_empty(q),
            None => needs_argument("query"),
        },
        // Deliberately not "open the window anyway": a typo that silently
        // launches a GUI is how you lose five minutes wondering what happened.
        other => Outcome::Say(format!("mogeung: no subcommand {other:?}\n\n{USAGE}"), 2),
    }
}

impl Outcome {
    /// A search for the empty string would ask the daemon to return
    /// everything, which is never what an empty argument meant.
    fn tap_empty(self, q: &str) -> Self {
        if q.trim().is_empty() {
            Outcome::Say(format!("mogeung search: needs a query\n\n{USAGE}"), 2)
        } else {
            self
        }
    }
}

/// The path and method each subcommand needs.
pub fn request(cmd: &Cmd) -> (&'static str, String) {
    match cmd {
        Cmd::Queue => ("GET", "/api/queue".into()),
        Cmd::Sessions => ("GET", "/api/sessions".into()),
        Cmd::Health => ("GET", "/api/health".into()),
        Cmd::Rescan => ("POST", "/api/rescan".into()),
        Cmd::Diff(id) => ("GET", format!("/api/sessions/{}/change", urlencode(id))),
        Cmd::Search(q) => ("GET", format!("/api/insight/search?q={}", urlencode(q))),
    }
}

/// Percent-encode everything that is not unreserved. Session ids are uuids and
/// queries are arbitrary text, so the query one is the case that matters.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Run it. Returns the process exit code.
pub fn run(cmd: Cmd, opts: Opts) -> i32 {
    let addr = opts
        .addr
        .clone()
        .or_else(|| mogeung_core::config::Config::load().0.addr)
        .unwrap_or_else(|| "127.0.0.1:7717".to_string());
    let token = opts
        .token
        .clone()
        .or_else(|| mogeung_core::config::Config::load().0.token);

    let (method, path) = request(&cmd);
    match fetch(&addr, method, &path, token.as_deref()) {
        Err(e) => {
            eprintln!("mogeung: {e}");
            1
        }
        Ok(body) => {
            if opts.json {
                return emit(body.trim_end());
            }
            match serde_json::from_str::<Value>(&body) {
                Ok(v) => emit(&render(&cmd, &v)),
                Err(e) => {
                    eprintln!("mogeung: the daemon's answer was not JSON ({e})");
                    1
                }
            }
        }
    }
}

/// Print, treating a closed pipe as the end of the job rather than a crash.
///
/// `println!` panics on `EPIPE`, so `mogeung diff … | head` — the most obvious
/// thing anyone will type — died with a Rust backtrace instead of stopping.
/// Found by running it, not by reading it.
fn emit(s: &str) -> i32 {
    use std::io::Write;
    match writeln!(std::io::stdout(), "{s}") {
        Ok(()) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            eprintln!("mogeung: could not write to stdout ({e})");
            1
        }
    }
}

fn fetch(addr: &str, method: &str, path: &str, token: Option<&str>) -> Result<String, String> {
    let url = format!("http://{addr}{path}");
    let mut args = vec![
        "-fsS".to_string(),
        "-m".to_string(),
        "20".to_string(),
        "-X".to_string(),
        method.to_string(),
    ];
    if let Some(t) = token {
        args.push("-H".into());
        args.push(format!("Authorization: Bearer {t}"));
    }
    args.push(url.clone());

    let out = std::process::Command::new("curl")
        .args(&args)
        .output()
        .map_err(|e| format!("could not run curl ({e})"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        return Err(if err.is_empty() {
            format!("no daemon answered at {addr} — is one running?")
        } else {
            format!("{addr}: {err}")
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// JSON to text. Separate from the fetching so every format below is testable
/// against a fixture rather than a live daemon.
pub fn render(cmd: &Cmd, v: &Value) -> String {
    match cmd {
        Cmd::Queue => render_queue(v),
        Cmd::Sessions => render_sessions(v),
        Cmd::Health => render_health(v),
        Cmd::Rescan => "rescanned".to_string(),
        Cmd::Diff(_) => render_diff(v),
        Cmd::Search(_) => render_search(v),
    }
}

fn str_at<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn render_queue(v: &Value) -> String {
    let Some(items) = v.as_array() else {
        return "nothing needs you".into();
    };
    if items.is_empty() {
        return "nothing needs you".into();
    }
    let mut out = Vec::new();
    for it in items {
        let reason = str_at(it, "reason");
        let id = str_at(it, "session_id");
        let detail = str_at(it, "detail");
        out.push(format!("{reason:<13} {:<10} {detail}", short(id)));
    }
    out.join("\n")
}

fn render_sessions(v: &Value) -> String {
    let Some(items) = v.as_array() else {
        return "no sessions".into();
    };
    if items.is_empty() {
        return "no sessions — run claude somewhere and it shows up here".into();
    }
    let mut out = Vec::new();
    for s in items {
        let live = if s.get("alive").and_then(Value::as_bool).unwrap_or(false) {
            "live"
        } else {
            "done"
        };
        let name = s
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| s.get("title").and_then(Value::as_str))
            .unwrap_or("—");
        let branch = str_at(s, "git_branch");
        let cwd = str_at(s, "cwd");
        out.push(format!(
            "{live}  {:<10} {:<24} {}",
            short(str_at(s, "id")),
            truncate(name, 24),
            if branch.is_empty() {
                cwd.to_string()
            } else {
                format!("{cwd} [{branch}]")
            }
        ));
    }
    out.join("\n")
}

fn render_health(v: &Value) -> String {
    let mut out = vec![format!(
        "{} (mogeungd {}, pid {})",
        str_at(v, "headline"),
        str_at(v, "version"),
        v.get("pid").map(|p| p.to_string()).unwrap_or_default()
    )];
    out.push(format!("watching {}", str_at(v, "claude_home")));
    if let Some(alerts) = v.get("alerts").and_then(Value::as_array) {
        for a in alerts {
            if let Some(m) = a.as_str() {
                out.push(format!("  ! {m}"));
            }
        }
    }
    out.join("\n")
}

fn render_diff(v: &Value) -> String {
    if let Some(e) = v.get("error").and_then(Value::as_str) {
        return format!("no diff: {e}");
    }
    match serde_json::from_value::<mogeung_core::change::Change>(v.clone()) {
        Ok(change) if change.files.is_empty() => "no changes".to_string(),
        // The same patch text the window's "copy as patch" produces, so what
        // you read here is what you would paste there.
        Ok(change) => crate::gitview::patch_text(&change.files),
        Err(e) => format!("could not read the change ({e})"),
    }
}

fn render_search(v: &Value) -> String {
    let hits = v
        .get("hits")
        .and_then(Value::as_array)
        .or_else(|| v.as_array());
    let Some(hits) = hits else {
        return "no matches".into();
    };
    if hits.is_empty() {
        return "no matches".into();
    }
    let mut out = Vec::new();
    for h in hits {
        let line = h.get("line").and_then(Value::as_u64).unwrap_or(0);
        out.push(format!(
            "{}:{line:<6} {:<9} {}",
            short(str_at(h, "session_id")),
            str_at(h, "role"),
            truncate(str_at(h, "preview").trim(), 96)
        ));
    }
    out.join("\n")
}

/// Session ids are uuids and a terminal line is not wide enough for two of
/// them. Eight characters is what git taught everyone to read.
fn short(id: &str) -> String {
    match id.len() > 8 {
        true => format!("{}…", &id[..8]),
        false => id.to_string(),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let cut: String = s.chars().take(n.saturating_sub(1)).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    /// The rule the window depends on: nothing that looks like the old
    /// invocation may be captured by a subcommand.
    #[test]
    fn the_window_still_opens_for_no_subcommand_and_for_flags() {
        assert_eq!(parse(Vec::<String>::new()), Outcome::Window);
        assert_eq!(parse(argv("--addr 127.0.0.1:7717")), Outcome::Window);
        assert_eq!(parse(argv("--foreground")), Outcome::Window);
    }

    /// A typo must not launch a GUI. This is the one behaviour a user cannot
    /// diagnose for themselves — the window opens, the command appears to have
    /// done nothing, and there is no message anywhere.
    #[test]
    fn an_unknown_subcommand_says_so_and_fails() {
        let Outcome::Say(msg, code) = parse(argv("quue")) else {
            panic!("an unknown subcommand must not open the window");
        };
        assert_eq!(code, 2);
        assert!(msg.contains("no subcommand"), "{msg}");
        assert!(msg.contains("queue"), "the usage must show what was meant");
    }

    #[test]
    fn subcommands_and_their_options_parse() {
        assert_eq!(parse(argv("queue")), Outcome::Run(Cmd::Queue, Opts::default()));
        assert_eq!(
            parse(argv("sessions --json")),
            Outcome::Run(Cmd::Sessions, Opts { json: true, ..Opts::default() })
        );
        assert_eq!(
            parse(argv("health --addr box:7717 --token t")),
            Outcome::Run(
                Cmd::Health,
                Opts { addr: Some("box:7717".into()), token: Some("t".into()), json: false }
            )
        );
        assert_eq!(
            parse(argv("diff abc-123")),
            Outcome::Run(Cmd::Diff("abc-123".into()), Opts::default())
        );
        // A multi-word search is one query, not the first word of one.
        assert_eq!(
            parse(argv("search rate limit event")),
            Outcome::Run(Cmd::Search("rate limit event".into()), Opts::default())
        );
    }

    /// Both of these would otherwise ask the daemon for everything it has and
    /// call it a search.
    #[test]
    fn a_subcommand_missing_its_argument_is_refused() {
        for a in ["diff", "search", "search --json"] {
            match parse(argv(a)) {
                Outcome::Say(msg, 2) => assert!(msg.contains("needs a"), "{a}: {msg}"),
                other => panic!("{a} should have been refused, got {other:?}"),
            }
        }
    }

    /// A query with a space or an ampersand in it must survive the trip. This
    /// is the one piece of string-handling here that can silently corrupt what
    /// the user asked for.
    #[test]
    fn a_query_is_encoded_into_the_url() {
        let (_, path) = request(&Cmd::Search("a & b?".into()));
        assert_eq!(path, "/api/insight/search?q=a%20%26%20b%3F");
        let (method, path) = request(&Cmd::Rescan);
        assert_eq!((method, path.as_str()), ("POST", "/api/rescan"));
    }

    #[test]
    fn the_queue_renders_as_one_line_per_item() {
        let v: Value = serde_json::from_str(
            r#"[{"session_id":"0123456789abcdef","reason":"waiting","score":90,
                 "detail":"waiting for you — 4m12s"}]"#,
        )
        .unwrap();
        let out = render(&Cmd::Queue, &v);
        assert!(out.contains("waiting"), "{out}");
        assert!(out.contains("01234567…"), "the id is shortened: {out}");
        assert!(out.contains("4m12s"), "the reason is kept: {out}");
        assert_eq!(out.lines().count(), 1);

        assert_eq!(render(&Cmd::Queue, &serde_json::json!([])), "nothing needs you");
    }

    /// An empty answer is not an error, and must not read like one.
    #[test]
    fn empty_answers_say_what_they_mean() {
        assert!(render(&Cmd::Sessions, &serde_json::json!([])).contains("no sessions"));
        assert_eq!(render(&Cmd::Search("x".into()), &serde_json::json!({"hits":[]})), "no matches");
        assert_eq!(
            render(&Cmd::Diff("s".into()), &serde_json::json!({"files":[],"insertions":0,"deletions":0})),
            "no changes"
        );
    }

    /// A daemon that could not diff must say why rather than claim there was
    /// nothing to diff — the two look identical from the outside.
    #[test]
    fn a_failed_diff_is_not_reported_as_no_changes() {
        let v = serde_json::json!({"files": [], "insertions": 0, "deletions": 0,
                                   "error": "base commit missing"});
        let out = render(&Cmd::Diff("s".into()), &v);
        assert!(out.contains("base commit missing"), "{out}");
    }
}
