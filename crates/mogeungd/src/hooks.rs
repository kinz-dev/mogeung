//! A seat's own voice: the hook receiver and what a seat is handed to reach
//! it. `R-P5`, feature 0051.
//!
//! This is the first thing in mogeung that an agent's **own process** talks
//! to ([ADR-0043](../../../docs/decisions/0043-a-seat-is-configured-at-launch-never-in-your-home.md),
//! clause 3 and *a new daemon surface*). Every session mogeung merely found
//! is read from files it writes; a seat may also *tell* the daemon things,
//! and three fences keep that from widening:
//!
//! 1. **Loopback only, a token per seat.** The two routes (`/api/seat/hook`,
//!    `/api/seat/status`) answer a loopback peer carrying one seat's token
//!    and nobody else — not the daemon's shared token, which is a window's
//!    key, not a process's. The token is minted at launch, rides the seat's
//!    environment, is persisted with the launch record so a re-adopted seat
//!    is still heard, and never crosses the wire to a client.
//! 2. **What arrives is data.** A body is JSON or it is refused; an event
//!    name is in [`CLAUDE_HANDLED`] / [`OPENCODE_HANDLED`] or it is counted
//!    as unknown and raised through the Health canary, the discipline the
//!    transcript parsers have had since `R-A1`. Nothing in a payload is
//!    executed, followed or shown as anything but a short line on the row.
//! 3. **A hook never blocks the seat.** The generated script exits zero
//!    whatever the daemon answered, with a four-second ceiling, so a daemon
//!    that is down costs a seat nothing but silence.
//!
//! # Who carries what
//!
//! | CLI | how the hooks travel | what the receiver hears |
//! |---|---|---|
//! | Claude Code | `--settings` with `hooks` and `statusLine` | the handled events, and a status reading |
//! | Qwen Code | `hooks` in the `settings.json` of a home of its own, and only then | the same event names |
//! | opencode | a plugin file, named by `plugin` in the inline config | a handful of bus events |
//! | Codex | nothing — its hooks live in its config with trust hashes, and a home of its own is not yet written for them | nothing; the seat is *plain* |
//!
//! The status line is a reading rather than an event: it fires on every
//! repaint, so it is never logged and never counted, only kept as the latest
//! context fill and model on the launch record.

use mogeung_core::session::SessionSource;
use mogeung_core::team::SeatLaunch;
use serde_json::Value;
use std::path::Path;

/// The environment a seat's hooks read. Set on the seat's tmux session with
/// `-e`, inherited by the CLI and by every hook it runs.
pub const URL_ENV: &str = "MOGEUNG_SEAT_URL";
pub const TOKEN_ENV: &str = "MOGEUNG_SEAT_TOKEN";

/// The two routes, under the daemon's own prefix.
pub const HOOK_PATH: &str = "/api/seat/hook";
pub const STATUS_PATH: &str = "/api/seat/status";
/// The third, `R-P6`: a seat's own words to a seat of its team. The
/// recipient is the `to` query parameter, the body is the text, plain —
/// so the generated command needs no JSON encoder, only `curl`.
pub const SEND_PATH: &str = "/api/seat/send";
/// What a seat runs to reach a teammate. On its `PATH`, under its files.
pub const SEND_COMMAND: &str = "mogeung-send";
/// Where a seat posts a note about itself — its last words on a wind-down.
/// `R-P15`. Same three fences as the send route.
pub const NOTE_PATH: &str = "/api/seat/note";
/// The command on a seat's PATH that posts there.
pub const NOTE_COMMAND: &str = "mogeung-note";

/// Claude Code hook events a seat is subscribed to, and the receiver reads.
/// Qwen Code uses the same names (checked against Qwen Code 0.23.0's bundle,
/// 2026-10-10), so a Qwen seat in a home of its own is subscribed to the
/// same list.
///
/// Chosen for what a team needs to know about a seat: that it started and
/// with what, that it stopped (the handover's and the wake's trigger,
/// `R-P6`), that it is asking for something (the one signal the transcript
/// is slow to show), that it was prompted, and that its context was
/// compacted. Verified against Claude Code 2.1.296's bundle on 2026-10-10.
pub const CLAUDE_HANDLED: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "Stop",
    "SubagentStop",
    "Notification",
    "PreCompact",
];

/// Every other hook event Claude Code 2.1.296 names. **Not subscribed**, by
/// decision: the tool-level events fire per call and would carry what the
/// transcript already does, at a cost per line; the rest are bookkeeping a
/// team has no use for yet. Listed so that one arriving anyway — from a
/// settings file a user wrote into a seat's own home, say — is classified
/// rather than raised as drift. Adding one is a change of mind: move it up.
pub const CLAUDE_KNOWN_IGNORED: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionDenied",
    "StopFailure",
    "SubagentStart",
    "PostCompact",
    "Elicitation",
    "ElicitationResult",
    "ConfigChange",
    "WorktreeCreate",
    "WorktreeRemove",
    "TeammateIdle",
    "TaskCompleted",
    "InstructionsLoaded",
    "FileChanged",
    "CwdChanged",
    "Setup",
];

/// What the status line calls itself in the JSON it is handed. Accepted on
/// the status route whatever the name says, because the route is the fact;
/// the name is kept so a status body posted to the hook route is classified
/// rather than unknown.
pub const STATUS_EVENTS: &[&str] = &["Status", "StatusLine"];

/// opencode bus events the generated plugin forwards and the receiver reads
/// — the plugin filters to this list **before** posting, because
/// `message.part.updated` fires per token and a receiver is not a firehose.
/// Names checked against opencode 1.18.35's bundle, 2026-10-10.
pub const OPENCODE_HANDLED: &[&str] = &[
    "session.created",
    "session.idle",
    "session.status",
    "session.error",
    "session.compacted",
    "permission.asked",
    "permission.replied",
    "question.asked",
    "question.replied",
];

/// opencode bus events the plugin does not forward, named so one that
/// arrives anyway is a decision already taken rather than drift.
pub const OPENCODE_KNOWN_IGNORED: &[&str] = &[
    "session.updated",
    "session.deleted",
    "session.diff",
    "message.updated",
    "message.removed",
    "message.part.updated",
    "message.part.removed",
    "file.edited",
    "file.watcher.updated",
    "todo.updated",
    "installation.updated",
    "tui.prompt.append",
    "server.connected",
    "lsp.client.diagnostics",
    "storage.write",
];

/// Which route a body arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Hook,
    Status,
}

/// What the receiver made of one body.
#[derive(Debug, Clone, PartialEq)]
pub enum Received {
    /// A handled event: counted, and the last word on the row.
    Event {
        name: String,
        detail: Option<String>,
        session_id: Option<String>,
    },
    /// A status reading: the latest fill and model, nothing counted.
    Status {
        context_used: Option<u8>,
        model: Option<String>,
        session_id: Option<String>,
    },
    /// A name on the ignored list: a decision already taken, not counted.
    Ignored { name: String },
    /// A name nobody has classified: raised through Health as `hook:<name>`.
    Unknown { name: String },
    /// Not JSON, or JSON with no name in it.
    Malformed,
}

/// Classify one body from a seat of `cli`, arrived on `route`.
///
/// Degrades, never panics: a shape that is not what the CLI documents is
/// `Malformed` or `Unknown`, and a field that is not what it should be is
/// read as absent. These payloads are an agent CLI's format, which `A4`
/// says moves without warning.
pub fn classify(cli: SessionSource, route: Route, body: &Value) -> Received {
    let Some(obj) = body.as_object() else {
        return Received::Malformed;
    };
    match cli {
        SessionSource::ClaudeCode | SessionSource::QwenCode | SessionSource::Codex => {
            let session_id = str_at(obj, "session_id");
            let name = str_at(obj, "hook_event_name");
            if route == Route::Status || name.as_deref().is_some_and(|n| STATUS_EVENTS.contains(&n)) {
                return Received::Status {
                    context_used: context_used(body),
                    model: model_name(body),
                    session_id,
                };
            }
            let Some(name) = name else {
                return Received::Malformed;
            };
            if CLAUDE_HANDLED.contains(&name.as_str()) {
                Received::Event {
                    detail: claude_detail(&name, obj),
                    name,
                    session_id,
                }
            } else if CLAUDE_KNOWN_IGNORED.contains(&name.as_str()) {
                Received::Ignored { name }
            } else {
                Received::Unknown { name }
            }
        }
        SessionSource::OpenCode => {
            let Some(name) = str_at(obj, "type") else {
                return Received::Malformed;
            };
            let props = obj.get("properties").and_then(Value::as_object);
            let session_id = props.and_then(|p| str_at(p, "sessionID"));
            if route == Route::Status {
                // opencode has no status line; a body on that route is a
                // plugin's mistake, read as the event it names.
            }
            if OPENCODE_HANDLED.contains(&name.as_str()) {
                Received::Event {
                    detail: opencode_detail(&name, props),
                    name,
                    session_id,
                }
            } else if OPENCODE_KNOWN_IGNORED.contains(&name.as_str()) {
                Received::Ignored { name }
            } else {
                Received::Unknown { name }
            }
        }
    }
}

/// What a handled event means to the policy (`R-P6`): the seat's turn
/// ended, or its process did. Claude Code's and Qwen Code's `Stop` is the
/// turn's end and `SessionEnd` the process's; opencode's `session.idle` is
/// the turn's end, and it names no exit the plugin would still be alive to
/// post, so a seat's exit is read off its pane instead. Everything else is
/// news for the row and nothing for the policy.
pub fn seat_event(cli: SessionSource, name: &str) -> Option<mogeung_core::team::SeatEvent> {
    use mogeung_core::team::SeatEvent;
    match (cli, name) {
        (SessionSource::ClaudeCode | SessionSource::QwenCode | SessionSource::Codex, "Stop") => Some(SeatEvent::Stop),
        (SessionSource::ClaudeCode | SessionSource::QwenCode | SessionSource::Codex, "SessionEnd") => {
            Some(SeatEvent::Exit)
        }
        (SessionSource::OpenCode, "session.idle") => Some(SeatEvent::Stop),
        _ => None,
    }
}

fn str_at(obj: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(str::to_string)
}

/// One short line, or nothing. Never the whole payload.
fn short(s: &str) -> Option<String> {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut out: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        out.push('…');
    }
    Some(out)
}

fn claude_detail(name: &str, obj: &serde_json::Map<String, Value>) -> Option<String> {
    match name {
        "Notification" => {
            let kind = str_at(obj, "notification_type");
            let msg = str_at(obj, "message").and_then(|m| short(&m));
            match (kind, msg) {
                (Some(k), Some(m)) => Some(format!("{k}: {m}")),
                (Some(k), None) => Some(k),
                (None, m) => m,
            }
        }
        "SessionStart" => str_at(obj, "source"),
        "SessionEnd" => str_at(obj, "reason"),
        "PreCompact" => str_at(obj, "trigger"),
        "SubagentStop" => str_at(obj, "agent_type"),
        "UserPromptSubmit" => str_at(obj, "prompt").and_then(|p| short(&p)),
        _ => None,
    }
}

fn opencode_detail(name: &str, props: Option<&serde_json::Map<String, Value>>) -> Option<String> {
    let props = props?;
    match name {
        "permission.asked" => str_at(props, "title")
            .or_else(|| str_at(props, "permission"))
            .and_then(|t| short(&t)),
        "question.asked" => props
            .get("questions")
            .and_then(Value::as_array)
            .and_then(|q| q.first())
            .and_then(Value::as_object)
            .and_then(|q| str_at(q, "question"))
            .and_then(|t| short(&t)),
        "session.status" => props
            .get("status")
            .and_then(Value::as_object)
            .and_then(|s| str_at(s, "type")),
        "session.error" => props.get("error").and_then(|e| {
            e.as_object().and_then(|e| {
                e.get("data")
                    .and_then(Value::as_object)
                    .and_then(|d| str_at(d, "message"))
                    .or_else(|| str_at(e, "name"))
            })
        })
        .and_then(|m| short(&m)),
        "permission.replied" | "question.replied" => str_at(props, "reply"),
        _ => None,
    }
}

/// The context fill from a status line body, in whole percent. Claude Code
/// gives `context_window.used_percentage`; failing that, the sum of the
/// current usage over the window size.
pub fn context_used(body: &Value) -> Option<u8> {
    let cw = body.get("context_window")?.as_object()?;
    let pct = cw
        .get("used_percentage")
        .and_then(Value::as_f64)
        .or_else(|| {
            let size = cw.get("context_window_size")?.as_f64()?;
            if size <= 0.0 {
                return None;
            }
            let u = cw.get("current_usage")?.as_object()?;
            let sum: f64 = ["input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
                .iter()
                .filter_map(|k| u.get(*k).and_then(Value::as_f64))
                .sum();
            Some(sum * 100.0 / size)
        })?;
    if !pct.is_finite() {
        return None;
    }
    Some(pct.round().clamp(0.0, 100.0) as u8)
}

/// The model a status line names: its display name, else its id.
pub fn model_name(body: &Value) -> Option<String> {
    let m = body.get("model")?;
    match m {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => str_at(o, "display_name").or_else(|| str_at(o, "id")),
        _ => None,
    }
}

/// A token for one seat: 256 bits of the same randomness the session ids
/// come from, as 64 hex characters. Compared in constant time by the
/// receiver.
pub fn mint_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Length-leaking only, like `api::constant_time_eq`: every byte compared.
pub fn token_matches(given: &str, token: &str) -> bool {
    let (a, b) = (given.as_bytes(), token.as_bytes());
    if a.is_empty() || a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The seat a bearer token names, if any. A seat with no token — a record
/// from before `R-P5`, or a plain seat — can never be matched.
pub fn seat_for<'a>(launches: &'a [SeatLaunch], bearer: &str) -> Option<&'a SeatLaunch> {
    launches.iter().find(|l| token_matches(bearer, &l.token))
}

/// Loopback, and nothing else — not the LAN with the shared token, not a
/// request with no address at all. A request with no address is a router
/// served without connect info, which no daemon does; refusing it is the
/// safe reading of an unknown.
pub fn admit(peer: Option<std::net::SocketAddr>) -> bool {
    peer.is_some_and(|a| crate::api::is_loopback_ip(a.ip()))
}

/// What the status line shows: the seat's name, that mogeung hears it, and
/// since `R-P6` the team's counters — the chain and the hour against their
/// budgets, *paused* when it is — so a seat's own screen says what the
/// policy will do with its next message.
pub fn status_text(l: &SeatLaunch, counters: Option<&str>) -> String {
    match counters {
        Some(c) if !c.is_empty() => format!("{}/{} · a seat of mogeung's · {c}", l.team, l.seat),
        _ => format!("{}/{} · a seat of mogeung's", l.team, l.seat),
    }
}

/// The command a seat runs to tell a teammate something: `mogeung-send
/// <seat> <text…>`, or the text on stdin. `R-P6`. Posts to the receiver
/// with the seat's own token, prints the daemon's one-line answer, and
/// exits 0 when delivered, 2 when held for a human, 1 when refused — so an
/// agent reading the exit code knows which, and one reading the line knows
/// why. Never silent: a seat that is told nothing retries.
pub fn send_script() -> String {
    format!(
        "#!/usr/bin/env bash\n\
         # Generated by mogeung for one seat (R-P6, ADR-0003's amendment, fence 4).\n\
         # Usage: {SEND_COMMAND} <seat> <message…>   or   echo <message> | {SEND_COMMAND} <seat>\n\
         # Tells a seat of your own team something: the daemon judges it against the\n\
         # team's hop budget, hourly cap and pause switch, then types it into that\n\
         # seat's terminal once. Exit 0 delivered, 2 held for a human, 1 refused.\n\
         to=\"${{1:-}}\"\n\
         if [ -z \"$to\" ]; then echo \"{SEND_COMMAND}: name the seat to tell\" >&2; exit 1; fi\n\
         case \"$to\" in *[!a-z0-9-]*) echo \"{SEND_COMMAND}: \\`$to\\` is not a seat name\" >&2; exit 1;; esac\n\
         shift\n\
         if [ $# -gt 0 ]; then text=\"$*\"; else text=\"$(cat)\"; fi\n\
         if [ -z \"${{{URL_ENV}:-}}\" ] || [ -z \"${{{TOKEN_ENV}:-}}\" ]; then\n\
           echo \"{SEND_COMMAND}: this seat has no way back to mogeung\" >&2; exit 1\n\
         fi\n\
         out=$(printf '%s' \"$text\" | curl -s -m 10 -X POST \\\n\
           -H \"Authorization: Bearer ${TOKEN_ENV}\" \\\n\
           -H 'Content-Type: text/plain; charset=utf-8' \\\n\
           --data-binary @- -w '\\n%{{http_code}}' \"${URL_ENV}{SEND_PATH}?to=$to\") || {{ echo \"{SEND_COMMAND}: mogeung did not answer\" >&2; exit 1; }}\n\
         code=\"${{out##*$'\\n'}}\"\n\
         body=\"${{out%$'\\n'*}}\"\n\
         echo \"$body\"\n\
         case \"$code\" in 200) exit 0;; 202) exit 2;; *) exit 1;; esac\n"
    )
}

/// The command a seat runs to leave a note for whoever has the seat next:
/// `mogeung-note <text…>`, or the text on stdin. `R-P15`. The daemon keeps
/// it beside the seat's files and feeds it to the next handover; during a
/// wind-down the note is also the seat's answer, and the seat is stopped
/// once it is in. Exit 0 kept, 1 refused — and the line says which.
pub fn note_script() -> String {
    format!(
        "#!/usr/bin/env bash\n\
         # Generated by mogeung for one seat (R-P15, ADR-0003's amendment of 2026-10-11).\n\
         # Usage: {NOTE_COMMAND} <text…>   or   echo <text> | {NOTE_COMMAND}\n\
         # Leaves a note about this seat's own work for whoever picks the seat up next:\n\
         # what you were doing, what is done, what is unfinished, what not to redo. The\n\
         # daemon keeps it and hands it to the next session's handover. Exit 0 kept.\n\
         if [ $# -gt 0 ]; then text=\"$*\"; else text=\"$(cat)\"; fi\n\
         if [ -z \"${{{URL_ENV}:-}}\" ] || [ -z \"${{{TOKEN_ENV}:-}}\" ]; then\n\
           echo \"{NOTE_COMMAND}: this seat has no way back to mogeung\" >&2; exit 1\n\
         fi\n\
         out=$(printf '%s' \"$text\" | curl -s -m 10 -X POST \\\n\
           -H \"Authorization: Bearer ${TOKEN_ENV}\" \\\n\
           -H 'Content-Type: text/plain; charset=utf-8' \\\n\
           --data-binary @- -w '\\n%{{http_code}}' \"${URL_ENV}{NOTE_PATH}\") || {{ echo \"{NOTE_COMMAND}: mogeung did not answer\" >&2; exit 1; }}\n\
         code=\"${{out##*$'\\n'}}\"\n\
         body=\"${{out%$'\\n'*}}\"\n\
         echo \"$body\"\n\
         case \"$code\" in 200) exit 0;; *) exit 1;; esac\n"
    )
}

/// The script every hook runs: stdin to the receiver, the reply to stdout,
/// exit zero whatever happened. The route is its one argument.
pub fn hook_script() -> String {
    format!(
        "#!/usr/bin/env bash\n\
         # Generated by mogeung for one seat (R-P5, ADR-0043 clause 3). Posts what\n\
         # the CLI hands a hook on stdin to the daemon's loopback receiver, prints\n\
         # the reply (the status line's text), and never fails the hook: a daemon\n\
         # that is down costs the seat nothing but silence. Regenerated at every\n\
         # boot; edits do not last.\n\
         route=\"${{1:-hook}}\"\n\
         if [ -z \"${{{URL_ENV}:-}}\" ] || [ -z \"${{{TOKEN_ENV}:-}}\" ]; then cat >/dev/null; exit 0; fi\n\
         curl -s -m 4 -X POST \\\n\
           -H \"Authorization: Bearer ${TOKEN_ENV}\" \\\n\
           -H 'Content-Type: application/json' \\\n\
           --data-binary @- \"${URL_ENV}/api/seat/$route\" 2>/dev/null || true\n\
         exit 0\n"
    )
}

/// The `hooks` object for Claude Code's `--settings` and Qwen Code's
/// `settings.json`: every handled event to the script, with a short timeout
/// so a slow receiver cannot stall a seat.
pub fn hooks_object(script: &Path) -> Value {
    let cmd = format!("{} hook", shell_word(&script.to_string_lossy()));
    let mut hooks = serde_json::Map::new();
    for name in CLAUDE_HANDLED {
        hooks.insert(
            (*name).to_string(),
            serde_json::json!([{ "hooks": [{ "type": "command", "command": cmd, "timeout": 5 }] }]),
        );
    }
    Value::Object(hooks)
}

/// The whole `--settings` document for a Claude Code seat: the hooks and
/// the status line, nothing else — a key here overrides the user's for this
/// session only (`A51`).
pub fn claude_settings(script: &Path) -> Value {
    let status = format!("{} status", shell_word(&script.to_string_lossy()));
    serde_json::json!({
        "hooks": hooks_object(script),
        "statusLine": { "type": "command", "command": status }
    })
}

/// `settings.json` for a Qwen seat's own home: what was there, with `hooks`
/// replaced. The home is mogeung's (ADR-0043 clause 5), so writing it is
/// allowed; everything but `hooks` is left as found.
pub fn qwen_settings(existing: Option<&str>, script: &Path) -> Value {
    let mut doc = existing
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    doc.insert("hooks".into(), hooks_object(script));
    Value::Object(doc)
}

/// The opencode plugin: one `event` hook, filtered to the handled list,
/// posting to the receiver with the seat's token from the environment.
pub fn opencode_plugin() -> String {
    let wanted = OPENCODE_HANDLED
        .iter()
        .map(|n| format!("{n:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "// Generated by mogeung for one seat (R-P5, ADR-0043 clause 3). Forwards a\n\
         // few of opencode's bus events to the daemon's loopback receiver, for this\n\
         // seat only; the token and the address come from the seat's environment.\n\
         // Regenerated at every boot; edits do not last.\n\
         const WANTED = new Set([{wanted}]);\n\
         export default async function mogeung() {{\n\
         \x20 const url = process.env.{URL_ENV};\n\
         \x20 const token = process.env.{TOKEN_ENV};\n\
         \x20 if (!url || !token) return {{}};\n\
         \x20 return {{\n\
         \x20   event: async ({{ event }}) => {{\n\
         \x20     if (!event || !WANTED.has(event.type)) return;\n\
         \x20     try {{\n\
         \x20       await fetch(url + \"{HOOK_PATH}\", {{\n\
         \x20         method: \"POST\",\n\
         \x20         headers: {{ authorization: \"Bearer \" + token, \"content-type\": \"application/json\" }},\n\
         \x20         body: JSON.stringify(event),\n\
         \x20         signal: AbortSignal.timeout(4000),\n\
         \x20       }});\n\
         \x20     }} catch {{}}\n\
         \x20   }},\n\
         \x20 }};\n\
         }}\n"
    )
}

/// A path as one shell word, for a command string a CLI runs through a
/// shell: single-quoted, with any single quote inside closed and reopened.
fn shell_word(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+=:@%".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Keep one accepted hook body beside the seat's generated files, one JSON
/// line per call, as `A54`'s fixtures are kept: the week of captures the
/// policy engine (`R-P6`) and the next sweep read. Capped so a chatty seat
/// cannot fill a disk; the status line is never written here.
pub fn append_capture(files_dir: &Path, body: &Value) {
    const CAP: u64 = 8 * 1024 * 1024;
    let path = files_dir.join("signals.jsonl");
    if std::fs::metadata(&path).map(|m| m.len() > CAP).unwrap_or(false) {
        return;
    }
    // The directory is mogeung's own, made at launch; a re-adopted seat
    // whose files were swept away still gets its captures.
    let _ = std::fs::create_dir_all(files_dir);
    let line = serde_json::json!({ "at": chrono::Utc::now().to_rfc3339(), "body": body });
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `R-P6`: the command a seat is handed posts the text as the body and
    /// the seat as a query parameter — no JSON written in bash — refuses a
    /// name that is not a seat name before any network, and maps the
    /// daemon's answer to three exit codes an agent can read.
    #[test]
    fn the_send_command_posts_plain_text_and_speaks_in_exit_codes() {
        let s = send_script();
        assert!(s.starts_with("#!/usr/bin/env bash\n"));
        assert!(s.contains(&format!("\"${URL_ENV}{SEND_PATH}?to=$to\"")), "{s}");
        assert!(s.contains(&format!("Bearer ${TOKEN_ENV}")));
        assert!(s.contains("Content-Type: text/plain"));
        assert!(s.contains("case \"$to\" in *[!a-z0-9-]*)"), "a seat name is checked first");
        assert!(s.contains("200) exit 0;; 202) exit 2;; *) exit 1;;"));
        assert!(!s.contains("mapfile"), "bash 3.2");
        let l = SeatLaunch {
            team_id: "t".into(),
            team: "port".into(),
            seat: "builder".into(),
            cli: SessionSource::Codex,
            session_id: None,
            pane_id: "%1".into(),
            tmux_target: "x:0.0".into(),
            tmux_session: "x".into(),
            team_session: String::new(),
            dir: "/w".into(),
            worktree: false,
            files_dir: "/f".into(),
            started: 0,
            token: String::new(),
            signals: Default::default(),
        };
        assert_eq!(status_text(&l, None), "port/builder · a seat of mogeung's");
        assert_eq!(
            status_text(&l, Some("hops 1/3 · 2/60 this hour")),
            "port/builder · a seat of mogeung's · hops 1/3 · 2/60 this hour"
        );
    }

    fn claude(name: &str, extra: Value) -> Value {
        let mut v = serde_json::json!({
            "session_id": "abc", "transcript_path": "/t", "cwd": "/w",
            "hook_event_name": name, "permission_mode": "default"
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            for (k, val) in e {
                o.insert(k.clone(), val.clone());
            }
        }
        v
    }

    #[test]
    fn a_handled_claude_event_is_an_event_with_its_short_detail() {
        let r = classify(
            SessionSource::ClaudeCode,
            Route::Hook,
            &claude("Notification", serde_json::json!({ "message": "Claude needs your permission to use Bash", "notification_type": "permission_prompt" })),
        );
        assert_eq!(
            r,
            Received::Event {
                name: "Notification".into(),
                detail: Some("permission_prompt: Claude needs your permission to use Bash".into()),
                session_id: Some("abc".into()),
            }
        );
        let r = classify(SessionSource::QwenCode, Route::Hook, &claude("Stop", serde_json::json!({ "stop_hook_active": false })));
        assert!(matches!(r, Received::Event { name, detail: None, .. } if name == "Stop"));
    }

    #[test]
    fn an_unsubscribed_name_is_ignored_and_a_strange_one_is_unknown() {
        let r = classify(SessionSource::ClaudeCode, Route::Hook, &claude("PreToolUse", serde_json::json!({})));
        assert_eq!(r, Received::Ignored { name: "PreToolUse".into() });
        let r = classify(SessionSource::ClaudeCode, Route::Hook, &claude("WarpDrive", serde_json::json!({})));
        assert_eq!(r, Received::Unknown { name: "WarpDrive".into() });
        assert_eq!(classify(SessionSource::ClaudeCode, Route::Hook, &serde_json::json!([1])), Received::Malformed);
        assert_eq!(classify(SessionSource::ClaudeCode, Route::Hook, &serde_json::json!({ "x": 1 })), Received::Malformed);
    }

    /// The two lists are disjoint and together name every 2.1.296 event the
    /// bundle was checked for — a name in both would be classified twice.
    #[test]
    fn the_claude_lists_do_not_overlap() {
        for n in CLAUDE_HANDLED {
            assert!(!CLAUDE_KNOWN_IGNORED.contains(n), "{n} is in both lists");
        }
        for n in OPENCODE_HANDLED {
            assert!(!OPENCODE_KNOWN_IGNORED.contains(n), "{n} is in both lists");
        }
    }

    #[test]
    fn a_status_reading_is_the_fill_and_the_model_and_is_not_an_event() {
        let body = serde_json::json!({
            "hook_event_name": "Status", "session_id": "abc",
            "model": { "id": "claude-haiku-4-5", "display_name": "Haiku 4.5" },
            "context_window": { "context_window_size": 200000, "used_percentage": 43.4,
                                 "current_usage": { "input_tokens": 1000 } }
        });
        let r = classify(SessionSource::ClaudeCode, Route::Status, &body);
        assert_eq!(r, Received::Status { context_used: Some(43), model: Some("Haiku 4.5".into()), session_id: Some("abc".into()) });
        // The route decides even without the name; the name decides even on the other route.
        let plain = serde_json::json!({ "model": "x", "context_window": { "context_window_size": 1000, "current_usage": { "input_tokens": 100, "cache_read_input_tokens": 150 } } });
        assert_eq!(context_used(&plain), Some(25));
        assert!(matches!(classify(SessionSource::ClaudeCode, Route::Status, &plain), Received::Status { model: Some(m), .. } if m == "x"));
        assert!(matches!(classify(SessionSource::ClaudeCode, Route::Hook, &body), Received::Status { .. }));
        assert_eq!(context_used(&serde_json::json!({ "context_window": { "context_window_size": 0 } })), None);
    }

    #[test]
    fn an_opencode_event_is_read_by_its_type_and_its_properties() {
        let r = classify(
            SessionSource::OpenCode,
            Route::Hook,
            &serde_json::json!({ "type": "permission.asked", "properties": { "sessionID": "ses_1", "title": "Run `cargo test`", "permission": "bash" } }),
        );
        assert_eq!(r, Received::Event { name: "permission.asked".into(), detail: Some("Run `cargo test`".into()), session_id: Some("ses_1".into()) });
        let r = classify(SessionSource::OpenCode, Route::Hook, &serde_json::json!({ "type": "message.part.updated", "properties": {} }));
        assert_eq!(r, Received::Ignored { name: "message.part.updated".into() });
        let r = classify(SessionSource::OpenCode, Route::Hook, &serde_json::json!({ "type": "session.error", "properties": { "sessionID": "s", "error": { "name": "ProviderError", "data": { "message": "rate limited\nmore" } } } }));
        assert!(matches!(r, Received::Event { detail: Some(d), .. } if d == "rate limited"));
        assert_eq!(classify(SessionSource::OpenCode, Route::Hook, &serde_json::json!({ "type": "tui.flicker" })), Received::Unknown { name: "tui.flicker".into() });
    }

    #[test]
    fn a_detail_is_one_short_line() {
        let long = "x".repeat(300);
        let d = short(&format!("\n\n  {long}\nsecond")).unwrap();
        assert_eq!(d.chars().count(), 121);
        assert!(d.ends_with('…'));
        assert_eq!(short("   \n  "), None);
    }

    #[test]
    fn a_token_is_long_random_and_matched_in_full() {
        let t = mint_token();
        assert_eq!(t.len(), 64);
        assert_ne!(t, mint_token());
        assert!(token_matches(&t, &t));
        assert!(!token_matches(&t[..63], &t));
        assert!(!token_matches("", ""), "an empty token must never match an empty bearer");
        let l = SeatLaunch { token: t.clone(), ..launch() };
        assert!(seat_for(&[l.clone()], &t).is_some());
        assert!(seat_for(&[l], "nope").is_none());
        assert!(seat_for(&[launch()], "").is_none(), "a plain seat has no token to match");
    }

    #[test]
    fn only_a_loopback_peer_is_admitted() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
        assert!(admit(Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5))));
        assert!(admit(Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 5))));
        assert!(!admit(Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9)), 5))));
        assert!(!admit(None));
    }

    /// The settings document subscribes exactly the handled list, each to the
    /// script with a timeout, and carries the status line beside it.
    #[test]
    fn the_claude_settings_name_every_handled_event_and_the_status_line() {
        let doc = claude_settings(Path::new("/home/you/.mogeung/seats/t/s/hook.sh"));
        let hooks = doc["hooks"].as_object().unwrap();
        let mut names: Vec<&str> = hooks.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut want: Vec<&str> = CLAUDE_HANDLED.to_vec();
        want.sort_unstable();
        assert_eq!(names, want);
        let stop = &hooks["Stop"][0]["hooks"][0];
        assert_eq!(stop["type"], "command");
        assert_eq!(stop["command"], "/home/you/.mogeung/seats/t/s/hook.sh hook");
        assert_eq!(stop["timeout"], 5);
        assert_eq!(doc["statusLine"]["command"], "/home/you/.mogeung/seats/t/s/hook.sh status");
        assert!(doc.get("permissions").is_none(), "nothing but hooks and the status line");
    }

    #[test]
    fn qwen_settings_keep_what_was_there_and_replace_only_hooks() {
        let had = r#"{"theme":"dark","hooks":{"Stop":[]}}"#;
        let doc = qwen_settings(Some(had), Path::new("/s/hook.sh"));
        assert_eq!(doc["theme"], "dark");
        assert!(doc["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap().ends_with("hook.sh hook"));
        let fresh = qwen_settings(None, Path::new("/s/hook.sh"));
        assert!(fresh["hooks"]["SessionStart"].is_array());
        // A path with a space is one word to the shell.
        let doc = claude_settings(Path::new("/home/a user/hook.sh"));
        assert_eq!(doc["statusLine"]["command"], "'/home/a user/hook.sh' status");
    }

    /// The plugin forwards the handled list and nothing wider, reads the two
    /// environment names the launcher sets, and posts to the hook route.
    #[test]
    fn the_opencode_plugin_forwards_only_the_handled_events() {
        let js = opencode_plugin();
        for n in OPENCODE_HANDLED {
            assert!(js.contains(&format!("{n:?}")), "{n} missing from the plugin's list");
        }
        for n in OPENCODE_KNOWN_IGNORED {
            assert!(!js.contains(&format!("{n:?}")), "{n} must not be forwarded");
        }
        assert!(js.contains(URL_ENV) && js.contains(TOKEN_ENV));
        assert!(js.contains(HOOK_PATH));
        assert!(js.starts_with("// Generated by mogeung"));
        assert!(js.contains("export default async function"));
    }

    #[test]
    fn the_hook_script_posts_and_never_fails() {
        let sh = hook_script();
        assert!(sh.starts_with("#!/usr/bin/env bash\n"));
        assert!(sh.contains("-m 4"), "a ceiling on the wait");
        assert!(sh.contains("|| true") && sh.trim_end().ends_with("exit 0"));
        assert!(sh.contains(&format!("${URL_ENV}/api/seat/$route")));
        assert!(sh.contains("Bearer $MOGEUNG_SEAT_TOKEN"));
        // Without the environment the script reads stdin away and exits 0.
        assert!(sh.contains("cat >/dev/null; exit 0"));
    }

    #[test]
    fn a_capture_is_one_json_line_and_stops_at_the_cap() {
        let d = std::env::temp_dir().join(format!("mogeung-hooks-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        append_capture(&d, &serde_json::json!({ "hook_event_name": "Stop" }));
        append_capture(&d, &serde_json::json!({ "hook_event_name": "Stop" }));
        let text = std::fs::read_to_string(d.join("signals.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 2);
        let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(first["body"]["hook_event_name"], "Stop");
        assert!(first["at"].is_string());
        std::fs::remove_dir_all(&d).ok();
    }

    fn launch() -> SeatLaunch {
        SeatLaunch {
            team_id: "t1".into(),
            team: "port".into(),
            seat: "builder".into(),
            cli: SessionSource::ClaudeCode,
            session_id: Some("abc".into()),
            pane_id: "%1".into(),
            tmux_target: "geung-port-builder:0.0".into(),
            tmux_session: "geung-port-builder".into(),
            team_session: String::new(),
            dir: "/w".into(),
            worktree: true,
            files_dir: "/f".into(),
            started: 1,
            token: String::new(),
            signals: Default::default(),
        }
    }
    /// `R-P15`: the note command posts to the note route with the seat's own
    /// token, takes its text as arguments or on stdin, and exits 0 only on a
    /// 200 — so an agent reading the code knows the note was kept.
    #[test]
    fn the_note_command_posts_to_the_note_route_and_exits_on_the_code() {
        let s = note_script();
        assert!(s.starts_with("#!/usr/bin/env bash\n"));
        assert!(s.contains(NOTE_PATH), "{s}");
        assert!(s.contains(&format!("${URL_ENV}{NOTE_PATH}")), "the receiver's URL from the environment");
        assert!(s.contains(&format!("Bearer ${TOKEN_ENV}")), "the seat's own token");
        assert!(s.contains("text=\"$(cat)\""), "stdin when there are no arguments");
        assert!(s.contains("case \"$code\" in 200) exit 0;; *) exit 1;; esac"));
        assert!(!s.contains("?to="), "a note names no recipient");
    }

}
