//! The config file both binaries read. `R-J3`.
//!
//! `~/.mogeung/config.toml`, flat. Flat rather than sectioned because the two
//! binaries' options overlap — `db`, `notify`, `push_url` and `token` mean the
//! same thing to the daemon and to the window, and a sectioned file would make
//! you write each of them twice and then wonder which won.
//!
//! **A flag always beats the file.** The file is where a preference lives; the
//! command line is how you override it once, and an override you have to edit
//! a file to undo is not an override.
//!
//! Every field is optional, including the file itself. What is absent stays at
//! the built-in default, so a config naming one setting is a valid config.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Everything either binary will read out of the file.
///
/// Deliberately not a mirror of the `--flags`: `--foreground`, `--log` and
/// `--no-daemon` are about *this invocation* rather than about a preference,
/// and putting them in a file would mean a window that can never run in the
/// foreground again without an edit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Daemon: address to listen on.
    pub listen: Option<String>,
    /// Window: daemon address to attach to.
    pub addr: Option<String>,
    /// Window: an explicit websocket URL, which also means "never start a
    /// daemon" — it may be another machine.
    pub url: Option<String>,
    /// Both: the database.
    pub db: Option<PathBuf>,
    /// Daemon: scan interval.
    pub poll_ms: Option<u64>,
    /// Both: desktop notifications.
    pub notify: Option<bool>,
    /// Both: also POST notifications here.
    pub push_url: Option<String>,
    /// Both: the shared token a non-loopback daemon requires.
    pub token: Option<String>,
    /// Daemon: how to reach this machine over ssh — `user@host`, or a name from
    /// `~/.ssh/config`. Published in the daemon's identity so a client can
    /// offer to open a shell here (`R-I5`, used by `R-I6`).
    pub ssh_target: Option<String>,
    /// Daemon: announce over mDNS on the local network (`R-I8`). Absent means
    /// no, which is the only safe default for a broadcast.
    pub advertise: Option<bool>,
    /// Daemon: send the LAN beacon (`R-I17`,
    /// [ADR-0039](../../../docs/decisions/0039-the-daemon-announces-itself-by-default.md)).
    /// Absent means **yes** — the one announcement that is on by default,
    /// because it says presence and never access. `false` is the standing
    /// way to be quiet; `--no-beacon` is the one-run way.
    pub beacon: Option<bool>,
    /// Daemon: the terminal application the Agent pane's terminal button
    /// opens a session's folder in. `R-J143`. Absent means **terminator** on
    /// Linux and **iTerm2** on a Mac.
    ///
    /// On Linux it is a program name — `terminator`, `gnome-terminal`,
    /// `konsole`, `xfce4-terminal`, `alacritty`, `kitty`, `wezterm`,
    /// `ghostty`, `foot`, `tilix`, `xterm` — each started with its own
    /// *start in this directory* flag; `x-terminal-emulator` means whatever
    /// the Debian alternative points at; any other name is run with the
    /// folder as its working directory, which most terminals inherit. On a
    /// Mac it is an application name for `open -a`: `iTerm2`, `Terminal`,
    /// `Ghostty`, `Warp`, `Alacritty`, `kitty`, `WezTerm`.
    ///
    /// Here, in the daemon's file, because the button is a daemon command:
    /// the terminal opens on the machine the folder is on, so the choice
    /// belongs to that machine. Re-applied on save, no restart needed.
    pub terminal: Option<String>,
    /// Daemon: base URL of an OpenAI-compatible model API — the part before
    /// `/chat/completions`, e.g. `http://spark-7ecc:8000/v1`. `R-O1`.
    ///
    /// The `…/models` URL is accepted too and trimmed, because that is the one
    /// a human can `curl` and therefore the one that gets pasted here.
    pub model_url: Option<String>,
    /// Daemon: which model to ask for, as the endpoint's own `/models` lists
    /// it. Absent means the endpoint's default.
    pub model_name: Option<String>,
    /// Daemon: which embedding model to ask for, on the **same endpoint** as
    /// `model_url`. `R-O6`. Absent means no embeddings, and every surface that
    /// would use them says so rather than degrading quietly.
    ///
    /// Deliberately not its own URL. A second endpoint would be a second host
    /// to consent to, and [ADR-0031](../../../docs/decisions/0031-consent-to-a-named-host.md)
    /// names one — a key that could point transcripts at a different machine
    /// without asking again would be the gate bypassed by a spelling.
    pub embed_model: Option<String>,
    /// Daemon: consent to a `model_url` that is not on this machine.
    /// [ADR-0031](../../../docs/decisions/0031-consent-to-a-named-host.md).
    ///
    /// `allow_remote_model = "spark-7ecc"` consents to **that host** and no
    /// other, so changing `model_url` asks again. `true` is the blanket grant
    /// the `--allow-remote-model` flag has always given; absent is no.
    ///
    /// This is the one consent key in the file, and it is here rather than
    /// flag-only because a window hosting its own daemon
    /// ([ADR-0009](../../../docs/decisions/0009-the-window-may-host-a-daemon.md))
    /// has no argv to be given a flag through — so flag-only meant *never* on
    /// the shape mogeung is normally run in. `--allow-run` still has no twin:
    /// that one grants running processes, not reading an endpoint.
    #[serde(default)]
    pub allow_remote_model: crate::model::RemoteConsent,
    /// Daemon: keep the chat panel's conversations, so they can be found
    /// again. `R-O9`. Absent means yes.
    ///
    /// `R-O5` shipped storing nothing — *no table to forget* — and the history
    /// reverses that deliberately
    /// ([ADR-0032](../../../docs/decisions/0032-the-chat-panel-remembers.md)).
    /// This key is the way back: `chat_history = false` and the daemon answers
    /// every ask and keeps none of it, exactly as it did before. What is
    /// already kept stays kept — turning the tap off is not the same act as
    /// emptying the bucket, and a setting that deleted your history would be a
    /// nasty surprise for anyone who set it to stop *adding* to it.
    pub chat_history: Option<bool>,
    /// Daemon: answer the Ask panel with opencode, inside a bubblewrap
    /// sandbox. `R-O14`,
    /// [ADR-0046](../../../docs/decisions/0046-an-ask-may-be-answered-by-a-sandboxed-agent.md).
    /// Absent means yes, and the owner chose that default.
    ///
    /// Yes only does anything where the sandbox can be built: on Linux, with
    /// `bwrap` and `opencode` found. Anywhere else the panel answers as plain
    /// chat and says why. There is deliberately no value that runs the agent
    /// without the sandbox. `ask_agent = false` returns Ask to plain chat
    /// with no note.
    pub ask_agent: Option<bool>,
    /// Daemon: run an llmproxy of mogeung's own in front of the model, so
    /// Ask Mogeung can route per question. `R-O10`, ADR-0033. Absent means no.
    ///
    /// **Its own instance, not yours.** The rules that suit a coding agent are
    /// not the rules that suit a chat panel, and llmproxy keys its daemon
    /// metadata on the bound address, so a second one on a second port is a
    /// first-class arrangement there rather than a fight.
    pub llmproxy: Option<bool>,
    /// Daemon: the binary, looked up on `PATH` unless this is a path.
    pub llmproxy_bin: Option<String>,
    /// Daemon: mogeung's own rules file. Default `~/.mogeung/llmproxy.toml`,
    /// written once if absent and never touched again.
    pub llmproxy_config: Option<PathBuf>,
    /// Daemon: an explicit port. Absent derives one from the daemon's own,
    /// which is what lets the next start-up find an orphan without a file
    /// recording where it went.
    pub llmproxy_port: Option<u16>,
    /// Window: the global shortcut that raises it. An empty string disables
    /// it, which is the file's way of saying `--no-hotkey`.
    pub hotkey: Option<String>,
    /// Window: watch the checkout it was installed from and offer an update
    /// when `update_branch` moves. `R-J105`, feature 0049. Absent means yes,
    /// in an installed build; a build run from `target/` never offers one.
    /// `false` and nothing polls at all.
    pub update_check: Option<bool>,
    /// Window: the branch an update follows. Default `main`.
    pub update_branch: Option<String>,
    /// Window: seconds between looks at the branch. Default 60. A look is one
    /// `git rev-parse`, milliseconds of work — nothing is built until you ask.
    pub update_interval: Option<u64>,
    /// Window: the checkout to update from. Default: the one `install.sh`
    /// recorded in `~/.mogeung/install.json` when it last installed.
    pub update_source: Option<PathBuf>,
    /// Daemon: read Anthropic's pricing page once a week, and say where it
    /// disagrees with the price table built in. `R-J124`,
    /// [ADR-0042](../../../docs/decisions/0042-a-timer-may-read-a-public-page.md).
    ///
    /// **The value is the host**, `refresh_pricing = "platform.claude.com"`:
    /// ADR-0031's shape, consent that says where the request goes. Absent is
    /// no, and any other host is refused rather than reached. A reading is
    /// compare-only for now — it prices nothing (feature 0050, phase 0).
    pub refresh_pricing: Option<String>,
}

impl Config {
    /// `~/.mogeung/config.toml`, or `$MOGEUNG_CONFIG` when set — which exists
    /// so the tests can point at a file without touching a real home
    /// directory, and is useful for running two configurations side by side.
    pub fn path() -> PathBuf {
        if let Ok(p) = std::env::var("MOGEUNG_CONFIG") {
            return PathBuf::from(p);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".mogeung").join("config.toml")
    }

    /// Load, and refuse a file that cannot be read whole. `R-J145`,
    /// [ADR-0045](../../../docs/decisions/0045-a-broken-config-file-stops-the-daemon.md).
    ///
    /// Until 2026-10-21 this tolerated everything — a broken file cost you
    /// your settings, never the daemon (`R-J3`) — and the day it mattered the
    /// rule read the other way: one key spelt `Notify` on a LAN daemon, and
    /// the daemon came up on defaults, loopback only and no token, with the
    /// one warning in a log nobody was reading. From the other machine that
    /// is indistinguishable from a dead daemon. So a file that does not parse
    /// is an error with the line, the key and the fix in it, and the caller
    /// decides what not to start. A missing file is still the defaults: not
    /// having one is the ordinary state of a fresh install.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => {
                return Err(ConfigError {
                    path: Some(path.to_path_buf()),
                    line: None,
                    problem: format!("could not be read: {e}"),
                    suggestion: None,
                })
            }
        };
        toml::from_str::<Config>(&text).map_err(|e| ConfigError::describe(Some(path.to_path_buf()), &text, &e))
    }

    /// Read the file as text, for showing it. `R-J79`.
    ///
    /// A missing file is `Ok("")` rather than an error: the ordinary state of
    /// a fresh install is *no config file yet*, and an editor that reports
    /// that as a failure teaches you to distrust it before you have written a
    /// line. A file that exists and cannot be read is a real error and says so.
    pub fn read_text(path: &std::path::Path) -> Result<String, String> {
        match std::fs::read_to_string(path) {
            Ok(t) => Ok(t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(format!("{} could not be read: {e}", path.display())),
        }
    }

    /// Parse without loading, so an editor can refuse to write a file the
    /// daemon would refuse to read.
    ///
    /// This is the **strict** reader, and deliberately unlike [`Self::load_from`]:
    /// loading tolerates everything because the daemon must start, where saving
    /// tolerates nothing because the person who can fix it is looking at it.
    /// `deny_unknown_fields` does most of the work — a mistyped key is caught
    /// here rather than silently ignored until someone wonders why a setting
    /// does nothing.
    pub fn check(text: &str) -> Result<Self, String> {
        toml::from_str::<Config>(text).map_err(|e| ConfigError::describe(None, text, &e).headline())
    }

    /// Validate, keep a copy of what was there, and write.
    ///
    /// Three properties, each bought by a way this can go wrong:
    ///
    /// - **Validated first.** A file that does not parse is never written, so
    ///   the worst a bad edit costs is an error message rather than a daemon
    ///   that starts on defaults and quietly ignores every setting you have.
    /// - **A `.bak` beside it.** This overwrites a file a human wrote by hand,
    ///   possibly one with comments in it, from a text box. The previous
    ///   contents are one `mv` away for exactly as long as it takes to notice.
    /// - **Written then renamed.** A crash mid-write leaves the old file
    ///   intact rather than half of the new one, which is the single failure
    ///   this file cannot survive: it is read before anything else exists to
    ///   report that it is broken.
    pub fn write_to(path: &std::path::Path, text: &str) -> Result<(), String> {
        Self::check(text)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("{} could not be created: {e}", dir.display()))?;
        }
        if let Ok(previous) = std::fs::read_to_string(path) {
            // Best effort: failing to keep a backup is not a reason to refuse
            // a save, it is a reason not to promise one.
            let _ = std::fs::write(path.with_extension("toml.bak"), previous);
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{} could not be written: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("{} could not be replaced: {e}", path.display()))
    }

    /// Every key this version understands, from the struct rather than from a
    /// list beside it.
    ///
    /// Derived through serde so it cannot drift: a field added above appears
    /// here without anyone remembering to add it, which is the whole reason
    /// not to hand-write the list. Order is the declaration order, which is
    /// grouped by what the key is for and is therefore the order worth showing.
    pub fn known_keys() -> Vec<String> {
        match serde_json::to_value(Config::default()) {
            Ok(serde_json::Value::Object(m)) => m.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// The hotkey the window should register, distinguishing "not mentioned"
    /// from "deliberately off". `None` here means the file said nothing and
    /// the built-in default applies.
    pub fn hotkey_setting(&self) -> Option<Option<String>> {
        self.hotkey.as_ref().map(|h| {
            if h.trim().is_empty() {
                None
            } else {
                Some(h.clone())
            }
        })
    }
}

/// Why a config file could not be read whole, written for the person who has
/// to fix it. `R-J145`, ADR-0045.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The file, when a file was read; `None` for text checked before a save.
    pub path: Option<PathBuf>,
    /// 1-based, when the parser could place the fault.
    pub line: Option<usize>,
    /// The fault — mogeung's words for an unknown key, the parser's otherwise.
    pub problem: String,
    /// The known key an unknown one was probably meant to be.
    pub suggestion: Option<String>,
}

impl ConfigError {
    /// From the parser's complaint. An unknown key is the likeliest mistake and
    /// serde's sentence for it — *unknown field `Notify`, expected one of …* —
    /// is written for a programmer, so it is replaced: the key, whether it is
    /// a known one spelt differently, and otherwise the list.
    pub fn describe(path: Option<PathBuf>, text: &str, e: &toml::de::Error) -> Self {
        let line = e
            .span()
            .map(|s| text[..s.start.min(text.len())].matches('\n').count() + 1);
        let msg = e.message();
        if let Some(rest) = msg.strip_prefix("unknown field `") {
            let key = rest.split('`').next().unwrap_or(rest).to_string();
            let suggestion = nearest_key(&key);
            return ConfigError {
                path,
                line,
                problem: format!("`{key}` is not a key this file knows"),
                suggestion,
            };
        }
        ConfigError {
            path,
            line,
            problem: msg.to_string(),
            suggestion: None,
        }
    }

    /// One line: where, and what.
    pub fn headline(&self) -> String {
        let mut s = String::new();
        if let Some(p) = &self.path {
            s.push_str(&p.display().to_string());
        }
        if let Some(n) = self.line {
            if self.path.is_some() {
                s.push(' ');
            }
            s.push_str(&format!("line {n}: "));
        } else if self.path.is_some() {
            s.push_str(": ");
        }
        s.push_str(&self.problem);
        if let Some(k) = &self.suggestion {
            s.push_str(&format!(" — did you mean `{k}`? Keys are lower-case"));
        }
        s
    }

    /// What to do: fix the line, or move the file aside to run on defaults —
    /// and the keys, when no single one was the likely intent.
    pub fn fix(&self) -> String {
        let path = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.mogeung/config.toml".to_string());
        let mut s = format!(
            "Fix the line and try again, or move the file aside to run on defaults:\n  mv {path} {path}.broken"
        );
        if self.unknown_key() && self.suggestion.is_none() {
            s.push_str(&format!(
                "\nThe keys this build knows: {}",
                Config::known_keys().join(", ")
            ));
        }
        s
    }

    /// Whether the fault is a key this file does not know, as opposed to a
    /// value or the syntax — the one case the list of keys helps with.
    pub fn unknown_key(&self) -> bool {
        self.problem.ends_with("is not a key this file knows")
    }
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\n{}", self.headline(), self.fix())
    }
}

impl std::error::Error for ConfigError {}

/// The known key an unknown one was probably meant to be: the same letters in
/// another case or with dashes for underscores, or within two edits — enough
/// for `Notify`, `notifyy` and `push-url`, not enough to guess at `foo`.
fn nearest_key(key: &str) -> Option<String> {
    let norm = |s: &str| s.trim().to_ascii_lowercase().replace('-', "_");
    let wanted = norm(key);
    let keys = Config::known_keys();
    if let Some(k) = keys.iter().find(|k| norm(k) == wanted) {
        return Some(k.clone());
    }
    keys.iter()
        .map(|k| (edit_distance(&norm(k), &wanted), k))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k.clone())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The editor's contract, and each half is a way this can hurt. `R-J79`.
    #[test]
    fn saving_validates_first_and_keeps_what_was_there() {
        let dir = std::env::temp_dir().join(format!("mogeung-save-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.toml");
        let _ = std::fs::remove_file(&path);

        // Nothing there yet is not an error — it is a fresh install.
        assert_eq!(Config::read_text(&path).unwrap(), "");

        Config::write_to(&path, "poll_ms = 400\n").unwrap();
        assert_eq!(Config::read_text(&path).unwrap(), "poll_ms = 400\n");

        // A file that would not load is refused rather than written, and what
        // was there survives.
        let err = Config::write_to(&path, "poll_ms = \"not a number\"").unwrap_err();
        assert!(!err.is_empty(), "a refusal has to say why");
        assert_eq!(Config::read_text(&path).unwrap(), "poll_ms = 400\n", "the good file survived");

        // A mistyped key is caught here rather than ignored for ever —
        // `deny_unknown_fields` earning its place.
        assert!(Config::write_to(&path, "pol_ms = 400").is_err());

        // And a good save keeps the previous contents one `mv` away.
        Config::write_to(&path, "poll_ms = 900\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(path.with_extension("toml.bak")).unwrap(),
            "poll_ms = 400\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The key list comes from the struct, so a field added above cannot go
    /// missing from the editor by being forgotten in a second list.
    #[test]
    fn the_known_keys_are_the_struct_s_own() {
        let keys = Config::known_keys();
        for expected in ["listen", "model_url", "model_name", "allow_remote_model", "hotkey", "terminal"] {
            assert!(keys.contains(&expected.to_string()), "{expected} missing from {keys:?}");
        }
    }

    /// One file per call. Tests run in parallel, and a shared path made three
    /// of them fail by reading each other's config.
    fn parse(text: &str) -> Result<Config, ConfigError> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("mogeung-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("config-{}.toml", N.fetch_add(1, Ordering::Relaxed)));
        std::fs::write(&path, text).unwrap();
        let out = Config::load_from(&path);
        let _ = std::fs::remove_file(&path);
        out
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let c = Config::load_from(std::path::Path::new("/nonexistent/mogeung.toml"))
            .expect("not having a config is the normal case");
        assert_eq!(c, Config::default());
    }

    #[test]
    fn one_setting_is_a_valid_config() {
        let c = parse("poll_ms = 400\n").unwrap();
        assert_eq!(c.poll_ms, Some(400));
        assert_eq!(c.listen, None, "everything else stays at its default");
    }

    /// The rule the whole file rests on, since 2026-10-21 (ADR-0045): a file
    /// that cannot be read whole is refused with the line named, never read as
    /// the defaults. Until then this test asserted the opposite — a broken
    /// file cost you your settings, never the daemon — and on the day it
    /// mattered that put a LAN daemon on loopback with the one warning in a
    /// log nobody was reading.
    #[test]
    fn a_broken_file_is_refused_naming_the_line() {
        let e = parse("listen = \"0.0.0.0:7717\"\npoll_ms = = 12\n").expect_err("a broken file must be refused");
        assert_eq!(e.line, Some(2), "{e}");
        assert!(e.suggestion.is_none());
        let shown = e.to_string();
        assert!(shown.contains("line 2:"), "{shown}");
        assert!(shown.contains(".broken"), "the way out is in the message: {shown}");
        assert!(!shown.contains("The keys this build knows"), "a syntax fault is not about a key: {shown}");
    }

    /// A misspelled key is the likeliest mistake, and the one that happened:
    /// `Notify = true` on 2026-10-21. The refusal names the key and the key it
    /// was probably meant to be, in the file's own case.
    #[test]
    fn an_unknown_key_is_refused_with_the_key_it_probably_meant() {
        let e = parse("poll_ms = 400\nNotify = true\n").expect_err("a typo must not pass");
        assert_eq!(e.line, Some(2), "{e}");
        assert_eq!(e.problem, "`Notify` is not a key this file knows");
        assert_eq!(e.suggestion.as_deref(), Some("notify"));
        assert!(e.headline().contains("did you mean `notify`?"), "{}", e.headline());
        assert!(!e.to_string().contains("The keys this build knows"), "a guess replaces the list");

        // Two edits away, or dashes for underscores, still guessed; nonsense is
        // not, and nonsense gets the list.
        assert_eq!(parse("notifyy = true\n").unwrap_err().suggestion.as_deref(), Some("notify"));
        assert_eq!(parse("push-url = \"x\"\n").unwrap_err().suggestion.as_deref(), Some("push_url"));
        let banana = parse("banana = 1\n").unwrap_err();
        assert_eq!(banana.suggestion, None);
        let shown = banana.to_string();
        assert!(shown.contains("The keys this build knows") && shown.contains("notify") && shown.contains("listen"), "{shown}");
    }

    /// A known key with a value of the wrong shape is the parser's complaint,
    /// verbatim, with the line — `"true"` in quotes is a string, not a flag.
    #[test]
    fn a_wrong_type_is_refused_in_the_parsers_words() {
        let e = parse("listen = \"0.0.0.0:7717\"\nnotify = \"true\"\n").expect_err("a string is not a bool");
        assert_eq!(e.line, Some(2), "{e}");
        assert!(e.problem.contains("boolean"), "{}", e.problem);
        assert!(e.suggestion.is_none());
    }

    /// The editor's check says the same thing, so a save and a start refuse a
    /// file in one voice. `R-J79`.
    #[test]
    fn the_editors_check_names_the_line_and_the_key() {
        let why = Config::check("Notify = true\n").unwrap_err();
        assert!(why.starts_with("line 1: `Notify` is not a key this file knows"), "{why}");
        assert!(why.contains("did you mean `notify`?"), "{why}");
    }

    /// An empty hotkey is how the file says "off", which has to be
    /// distinguishable from not mentioning it at all — otherwise disabling the
    /// global shortcut is impossible without a flag on every launch.
    #[test]
    fn an_empty_hotkey_disables_it_and_an_absent_one_does_not() {
        assert_eq!(parse("hotkey = \"\"\n").unwrap().hotkey_setting(), Some(None));
        assert_eq!(
            parse("hotkey = \"Ctrl+Alt+M\"\n").unwrap().hotkey_setting(),
            Some(Some("Ctrl+Alt+M".to_string()))
        );
        assert_eq!(parse("poll_ms = 1\n").unwrap().hotkey_setting(), None);
    }
}
