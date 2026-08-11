//! Other people's run configurations, and what each one means to us. `R-N1`.
//!
//! [ADR-0026](../../../docs/decisions/0026-other-peoples-run-configurations.md)
//! decided where run configurations come from: **detection is the source**, and
//! the only configuration *file* read is VS Code's. This module is that reader,
//! and — more importantly for `R-N1` — it is where the judgement about each
//! `type` lives, so the sweep and the daemon cannot hold different opinions.
//!
//! # Why a third private format gets `adapter.rs`'s discipline
//!
//! `launch.json` has a specification, which makes it the best-behaved of the
//! three formats mogeung reads. It is still somebody else's: the `type` field
//! is an open namespace extended by every debug extension anyone installs, so
//! there is no list to be complete against. [`HANDLED`] and [`KNOWN_IGNORED`]
//! are therefore decisions rather than a schema, and anything in neither is
//! **named and listed as unrunnable** rather than dropped — because a
//! configuration silently missing reads as *"mogeung did not find it"*, which
//! is the complaint `R-J12` cost a fortnight to notice.
//!
//! # The key is `type/request`, not `type`
//!
//! ADR-0026 says *"a `type` we run goes in `HANDLED`"*, and on the corpus this
//! was built against that turns out to be one word short. The only
//! `launch.json` on either machine is `github/codex`'s, and it carries **two**
//! `lldb` entries — one `launch` and one `attach`. They are not the same
//! decision: launching a binary is `R-N4`'s job and works today, while
//! attaching needs a live debug session, which is Phase 2 at the earliest.
//! Classifying on `type` alone would have to call `lldb` runnable and then
//! refuse half of what it matched.
//!
//! So the key is the pair, reported the way [`crate::codex`]'s nested taxonomy
//! already is — `lldb/launch`, `lldb/attach` — and the compound is what the
//! sweep names when it finds something new.
//!
//! # What is deliberately not here
//!
//! IntelliJ's `.run/*.run.xml`, `.idea/runConfigurations/*.xml` and
//! `.idea/workspace.xml` are **measured by the sweep and parsed by nothing**
//! (`R-N12`). That is ADR-0026's cut, and the sweep keeps counting them so the
//! deferral has a measurement behind it rather than becoming permanent by
//! forgetting.

use serde_json::Value;
use std::path::Path;

/// `type/request` pairs we intend to run, and the languages are not an accident:
/// `R-N11` scopes this cut to Python, Node and Rust, in that order.
///
/// `launch` only. An `attach` entry names a process that is already running and
/// needs a debugger to be worth anything, so it is understood and declined
/// below rather than half-supported here.
pub const HANDLED: &[&str] = &[
    // Rust and C/C++ via codelldb or lldb-dap — how the one real `launch.json`
    // in this corpus launches a cargo binary.
    "lldb/launch",
    "codelldb/launch",
    // Python. `debugpy` is the current type; `python` is what the older
    // extension wrote, and files outlive extensions.
    "debugpy/launch",
    "python/launch",
    // Node. `pwa-node` is what VS Code's JavaScript debugger writes now.
    "node/launch",
    "pwa-node/launch",
];

/// Pairs we understand and decline. Each is a decision with a reason, because
/// an entry here is one the sweep will never mention again.
pub const KNOWN_IGNORED: &[&str] = &[
    // **Attach, in every language.** Not a gap in coverage — a different
    // feature. Phase 1 starts processes; nothing here can join one.
    "lldb/attach",
    "codelldb/attach",
    "debugpy/attach",
    "python/attach",
    "node/attach",
    "pwa-node/attach",
    // Java, deferred by name in ADR-0026 along with IntelliJ's files.
    "java/launch",
    "java/attach",
];

/// `tasks.json` has its own namespace, and it is a much smaller one.
///
/// A task carries a command line rather than a debug session, so this is the
/// half of VS Code's configuration that Phase 1 can actually honour. It stays a
/// separate list because the two files share no vocabulary — a `type` of
/// `process` means nothing in `launch.json`, and colliding them would let a
/// decision about one silently answer for the other.
pub const TASK_HANDLED: &[&str] = &["shell", "process", "npm"];

/// Task types that need an extension's own machinery to mean anything.
pub const TASK_KNOWN_IGNORED: &[&str] = &["typescript", "gulp", "grunt"];

/// What the lists say about one configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// We intend to run it.
    Handled,
    /// We understand it and decline. Listed, never run.
    Ignored,
    /// Neither list has an opinion. **Listed as unrunnable with its type
    /// named**, and a health alert — never hidden.
    Unclassified,
}

/// One entry read out of a VS Code file, before anything decides to run it.
///
/// `key` is what the lists are consulted with and what the sweep reports:
/// `lldb/launch` for a launch configuration, a bare `shell` for a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The human's own name for it — `launch.json`'s `name`, or a task's
    /// `label`. Empty when the file did not give one.
    pub name: String,
    /// The raw `type`, kept whole so a refusal can name what it saw.
    pub ty: String,
    /// `launch` or `attach`, and `None` for a task.
    pub request: Option<String>,
    /// `type/request`, or just `type`.
    pub key: String,
}

impl Entry {
    fn new(name: String, ty: String, request: Option<String>) -> Self {
        let key = match &request {
            Some(r) => format!("{ty}/{r}"),
            None => ty.clone(),
        };
        Entry { name, ty, request, key }
    }
}

/// Which VS Code file an entry came from. They classify against different lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Launch,
    Tasks,
}

/// The verdict for one entry, from the lists above and nowhere else.
pub fn classify(source: Source, key: &str) -> Verdict {
    let (handled, ignored) = match source {
        Source::Launch => (HANDLED, KNOWN_IGNORED),
        Source::Tasks => (TASK_HANDLED, TASK_KNOWN_IGNORED),
    };
    if handled.contains(&key) {
        Verdict::Handled
    } else if ignored.contains(&key) {
        Verdict::Ignored
    } else {
        Verdict::Unclassified
    }
}

/// Read `.vscode/launch.json`'s `configurations`, or nothing.
///
/// Nothing is an ordinary answer — one repository in nineteen carried this file
/// when ADR-0026 measured, and it is not this one.
pub fn read_launch(repo: &Path) -> Vec<Entry> {
    let Some(v) = read_jsonc(&repo.join(".vscode/launch.json")) else {
        return Vec::new();
    };
    // `compounds` is deliberately not read: it references other configurations
    // by name rather than describing one, so it has no `type` to classify and
    // nothing of its own to run.
    entries(v.get("configurations"), |o| {
        let ty = o.get("type").and_then(Value::as_str)?.to_string();
        let request = o.get("request").and_then(Value::as_str).map(str::to_string);
        let name = o.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        Some(Entry::new(name, ty, request))
    })
}

/// Read `.vscode/tasks.json`'s `tasks`, or nothing.
pub fn read_tasks(repo: &Path) -> Vec<Entry> {
    let Some(v) = read_jsonc(&repo.join(".vscode/tasks.json")) else {
        return Vec::new();
    };
    entries(v.get("tasks"), |o| {
        let ty = o.get("type").and_then(Value::as_str)?.to_string();
        let name = o.get("label").and_then(Value::as_str).unwrap_or("").to_string();
        Some(Entry::new(name, ty, None))
    })
}

/// Map an array of objects through a reader, skipping what it cannot read.
///
/// **Skipping rather than failing** is the parser rule this repository applies
/// to every foreign format: one malformed entry must not cost the file.
fn entries(array: Option<&Value>, read: impl Fn(&Value) -> Option<Entry>) -> Vec<Entry> {
    array
        .and_then(Value::as_array)
        .map(|xs| xs.iter().filter_map(read).collect())
        .unwrap_or_default()
}

/// Parse a file VS Code would accept: JSON, plus comments and trailing commas.
///
/// `None` for absent or unreadable, and `None` for genuinely broken — a
/// configuration file that does not parse is the editor's problem to report,
/// and mogeung refusing to start over one would be worse than offering nothing.
pub fn read_jsonc(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&strip_jsonc(&raw)).ok()
}

/// Turn JSONC into JSON: comments blanked, trailing commas dropped.
///
/// Written by hand rather than pulled in as a dependency, and it is a
/// *character scanner* rather than a regex for one reason worth stating: a
/// `//` inside a string literal is not a comment, and a `"` inside a comment
/// does not open a string. Both appear in real files — a `program` path of
/// `"https://…"` is the ordinary case, not the exotic one — and every
/// line-based approach gets that wrong.
///
/// Comments become spaces rather than disappearing, so byte offsets survive and
/// a parse error still points at the right column.
fn strip_jsonc(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut chars = src.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                    out.push(' ');
                }
                out.push(' ');
            }
            '/' if chars.peek() == Some(&'*') => {
                out.push_str("  ");
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    // Newlines are kept so line numbers do not shift.
                    out.push(if n == '\n' { '\n' } else { ' ' });
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    drop_trailing_commas(&out)
}

/// Remove a comma that is followed only by whitespace and a closing bracket.
///
/// Runs after comments are blanked, so `[1, /* two */]` is handled by the same
/// pass rather than needing to understand comments a second time.
fn drop_trailing_commas(src: &str) -> String {
    let bytes: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut in_string = false;
    let mut escaped = false;

    for (i, &c) in bytes.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        if c == '"' {
            in_string = true;
            out.push(c);
            continue;
        }
        if c == ',' {
            let next = bytes[i + 1..].iter().find(|x| !x.is_whitespace());
            if matches!(next, Some(']') | Some('}')) {
                // Dropped. A space keeps the offset, for the same reason the
                // comment stripper pads.
                out.push(' ');
                continue;
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus this was built against: two `lldb` entries in one file, and
    /// they are **not** the same decision. Classifying on `type` alone would
    /// have to call `lldb` runnable and then refuse half of what it matched.
    #[test]
    fn launch_and_attach_of_one_type_are_different_verdicts() {
        assert_eq!(classify(Source::Launch, "lldb/launch"), Verdict::Handled);
        assert_eq!(classify(Source::Launch, "lldb/attach"), Verdict::Ignored);
    }

    /// Anything with no opinion recorded is unclassified — never quietly
    /// runnable, and never quietly dropped.
    #[test]
    fn a_type_nobody_decided_about_is_unclassified() {
        assert_eq!(classify(Source::Launch, "holodeck/launch"), Verdict::Unclassified);
        assert_eq!(classify(Source::Tasks, "holodeck"), Verdict::Unclassified);
    }

    /// The two files share no vocabulary, and a decision about one must not
    /// answer for the other.
    #[test]
    fn the_two_files_classify_against_their_own_lists() {
        assert_eq!(classify(Source::Tasks, "shell"), Verdict::Handled);
        // `shell` means nothing in launch.json, so it is not silently runnable.
        assert_eq!(classify(Source::Launch, "shell"), Verdict::Unclassified);
        // …and a launch pair means nothing as a task.
        assert_eq!(classify(Source::Tasks, "lldb/launch"), Verdict::Unclassified);
    }

    /// A `//` inside a string is not a comment. This is the case every
    /// line-based stripper gets wrong, and a URL in a `program` field is
    /// ordinary rather than exotic.
    #[test]
    fn a_url_in_a_string_survives_comment_stripping() {
        let src = r#"{ "url": "https://example.com/x", // a real comment
            "n": 1 }"#;
        let v: Value = serde_json::from_str(&strip_jsonc(src)).expect("parses");
        assert_eq!(v["url"], "https://example.com/x");
        assert_eq!(v["n"], 1);
    }

    #[test]
    fn block_comments_and_trailing_commas_go() {
        let src = r#"{
            /* leading
               block */
            "a": [1, 2, ],
            "b": { "c": 3, },
        }"#;
        let v: Value = serde_json::from_str(&strip_jsonc(src)).expect("parses");
        assert_eq!(v["a"][1], 2);
        assert_eq!(v["b"]["c"], 3);
    }

    /// A comma inside a string is not a trailing comma, even next to a bracket.
    #[test]
    fn a_comma_in_a_string_is_left_alone() {
        let src = r#"{ "args": ["a,", "b"] }"#;
        let v: Value = serde_json::from_str(&strip_jsonc(src)).expect("parses");
        assert_eq!(v["args"][0], "a,");
    }

    /// An escaped quote must not be read as the end of the string — otherwise
    /// everything after it is scanned as if it were code.
    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        let src = r#"{ "a": "say \"hi\" // not a comment", "b": 2 }"#;
        let v: Value = serde_json::from_str(&strip_jsonc(src)).expect("parses");
        assert_eq!(v["a"], r#"say "hi" // not a comment"#);
        assert_eq!(v["b"], 2);
    }

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("mogeung-rc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The real file, reproduced: two entries, one of each request.
    #[test]
    fn the_corpus_launch_json_reads_as_two_entries() {
        let d = scratch("launch");
        write(
            &d,
            ".vscode/launch.json",
            r#"{
              "version": "0.2.0",
              "configurations": [
                { "type": "lldb", "request": "launch", "name": "Cargo launch" },
                { "type": "lldb", "request": "attach", "name": "Attach to running codex CLI" }
              ]
            }"#,
        );
        let got = read_launch(&d);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].key, "lldb/launch");
        assert_eq!(got[0].name, "Cargo launch");
        assert_eq!(got[1].key, "lldb/attach");
    }

    /// A repository with no `.vscode` at all is the **ordinary** case — five
    /// checked-in configuration files across nineteen repositories — so it must
    /// be silence rather than anything that looks like a failure.
    #[test]
    fn a_repository_with_no_vscode_directory_is_simply_empty() {
        let d = scratch("bare");
        assert!(read_launch(&d).is_empty());
        assert!(read_tasks(&d).is_empty());
    }

    /// One unreadable entry must not cost the file.
    #[test]
    fn an_entry_with_no_type_is_skipped_not_fatal() {
        let d = scratch("partial");
        write(
            &d,
            ".vscode/launch.json",
            r#"{"configurations":[{"name":"no type here"},{"type":"node","request":"launch","name":"ok"}]}"#,
        );
        let got = read_launch(&d);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "ok");
    }

    /// A file that does not parse yields nothing rather than panicking — the
    /// rule every foreign-format reader here follows.
    #[test]
    fn a_broken_file_degrades_rather_than_panicking() {
        let d = scratch("broken");
        write(&d, ".vscode/launch.json", "{ this is not json at all ");
        assert!(read_launch(&d).is_empty());
    }
}
