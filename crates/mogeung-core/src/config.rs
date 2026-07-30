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
