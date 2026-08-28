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
    /// Daemon: base URL of an OpenAI-compatible model API — the part before
    /// `/chat/completions`, e.g. `http://spark-7ecc:8000/v1`. `R-O1`.
    ///
    /// The `…/models` URL is accepted too and trimmed, because that is the one
    /// a human can `curl` and therefore the one that gets pasted here.
    pub model_url: Option<String>,
    /// Daemon: which model to ask for, as the endpoint's own `/models` lists
    /// it. Absent means the endpoint's default.
    pub model_name: Option<String>,
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
    /// Window: the global shortcut that raises it. An empty string disables
    /// it, which is the file's way of saying `--no-hotkey`.
    pub hotkey: Option<String>,
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

    /// Load, tolerating everything. Returns the defaults and a complaint
    /// rather than an error: a typo in a preferences file must not stop the
    /// daemon starting, because the daemon is how you find out anything is
    /// wrong at all.
    pub fn load() -> (Self, Option<String>) {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &std::path::Path) -> (Self, Option<String>) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Self::default(), None);
        };
        match toml::from_str::<Config>(&text) {
            Ok(c) => (c, None),
            Err(e) => (
                Self::default(),
                Some(format!(
                    "{} is unreadable ({}) — using defaults",
                    path.display(),
                    e.message()
                )),
            ),
        }
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
        toml::from_str::<Config>(text).map_err(|e| e.to_string())
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
        for expected in ["listen", "model_url", "model_name", "allow_remote_model", "hotkey"] {
            assert!(keys.contains(&expected.to_string()), "{expected} missing from {keys:?}");
        }
    }

    /// One file per call. Tests run in parallel, and a shared path made three
    /// of them fail by reading each other's config.
    fn parse(text: &str) -> (Config, Option<String>) {
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
        let (c, warning) = Config::load_from(std::path::Path::new("/nonexistent/mogeung.toml"));
        assert_eq!(c, Config::default());
        assert!(warning.is_none(), "not having a config is the normal case");
    }

    #[test]
    fn one_setting_is_a_valid_config() {
        let (c, warning) = parse("poll_ms = 400\n");
        assert!(warning.is_none());
        assert_eq!(c.poll_ms, Some(400));
        assert_eq!(c.listen, None, "everything else stays at its default");
    }

    /// The rule the whole file rests on: a broken config costs you your
    /// settings, never the daemon. If this ever throws, a stray character
    /// stops mogeung starting and the only way to find out is that nothing
    /// happens.
    #[test]
    fn a_broken_file_yields_defaults_and_a_complaint() {
        let (c, warning) = parse("poll_ms = = 12\nlisten = \n");
        assert_eq!(c, Config::default());
        let w = warning.expect("a broken file must say so");
        assert!(w.contains("using defaults"), "{w}");
    }

    /// A misspelled key is the likeliest mistake, and silently ignoring it is
    /// the worst response: the setting appears to be applied and is not.
    #[test]
    fn an_unknown_key_is_reported_rather_than_ignored() {
        let (c, warning) = parse("poll_ms = 400\nnotifyy = true\n");
        assert!(warning.is_some(), "a typo must not pass silently");
        assert_eq!(c, Config::default());
    }

    /// An empty hotkey is how the file says "off", which has to be
    /// distinguishable from not mentioning it at all — otherwise disabling the
    /// global shortcut is impossible without a flag on every launch.
    #[test]
    fn an_empty_hotkey_disables_it_and_an_absent_one_does_not() {
        assert_eq!(parse("hotkey = \"\"\n").0.hotkey_setting(), Some(None));
        assert_eq!(
            parse("hotkey = \"Ctrl+Alt+M\"\n").0.hotkey_setting(),
            Some(Some("Ctrl+Alt+M".to_string()))
        );
        assert_eq!(parse("poll_ms = 1\n").0.hotkey_setting(), None);
    }
}
