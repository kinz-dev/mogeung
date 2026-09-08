//! The `PATH` a daemon started from a desktop launcher does not have. `R-J87`.
//!
//! macOS gives a bundle launched from Finder, the Dock or Spotlight the
//! launchd `PATH` — `/usr/bin:/bin:/usr/sbin:/sbin` — and nothing else. No
//! `.zshrc` is read, because nothing in the chain is a login shell, so
//! `/opt/homebrew/bin` is simply not there. The same daemon started with
//! `./scripts/start.sh` inherits your shell's `PATH` and works perfectly,
//! which is why this never shows up in development.
//!
//! What it looks like instead is a lie about tmux. [`crate::state::tmux_panes`]
//! reads a failure to run `tmux` as "no tmux here" — correct, and the ordinary
//! case on a machine without it — so an installed mogeung sees every pane list
//! as empty, every session's `tmux_target` resolves to `None`, and the Agent
//! tab says **"This session is not running under tmux"** about a session that
//! plainly is. Nothing errors, and nothing is logged. Found 2026-09-06 from
//! exactly that report, against a `yolomo` session that was in `tmux ls` the
//! whole time.
//!
//! `claude_binary` and its two siblings in [`crate::state`] are this same
//! lesson, solved one binary at a time because each installer puts its binary
//! where a profile — not launchd — adds it to `PATH`. This is the general
//! form, and it covers `tmux`, which nobody thought to write a resolver for.
//!
//! ## Why not just set the variable
//!
//! `setenv` mutates state every other thread may be reading, and the daemon is
//! hosted in-process by a window that already has plenty of threads (ADR-0009)
//! — Rust 2024 makes `std::env::set_var` `unsafe` for exactly this. So nothing
//! here writes to the environment. The repaired `PATH` is computed once and
//! used two ways, because a child needs both and they are not the same thing:
//!
//! - **Finding the program.** `Command::new("tmux")` resolves the name with
//!   *this* process's `PATH` — `posix_spawnp` searches the caller's
//!   environment, not the one you attached to the child — so a `.env("PATH",
//!   …)` alone would still fail to find it. [`which`] hands over an absolute
//!   path instead.
//! - **The child's own environment.** A headless launch creates the tmux
//!   *server*, and every agent started under it inherits the environment that
//!   server was born with. [`command`] gives it the repaired `PATH` so the
//!   agent can find `git`, `node` and the rest.

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// The directories a profile normally adds, in the order a profile adds them.
///
/// Deliberately the union of what `claude_binary`, `qwen_binary` and
/// `codex_binary` already walk, plus the Homebrew and MacPorts prefixes that
/// hold `tmux`. Those resolvers stay: they know about paths no `PATH` ever
/// contains (`~/.claude/local/claude`), and a resolver that still works when
/// this list misses something is worth keeping.
fn candidates(home: &str) -> Vec<String> {
    vec![
        format!("{home}/.local/bin"),
        format!("{home}/bin"),
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        "/opt/local/bin".to_string(),
    ]
}

/// `current` with each of `extra` appended once, in order, skipping any it
/// already holds.
///
/// Appended, never prepended: a daemon started from a shell already has the
/// `PATH` its user chose, and repairing it must not quietly reorder it.
///
/// Pure so the shape can be pinned by a test on a machine whose real `PATH`
/// says nothing useful — the same reason `parse_tmux_panes` is split out from
/// the command that feeds it.
fn merged(current: &str, extra: &[String]) -> String {
    let mut out: Vec<String> = std::env::split_paths(current)
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !p.is_empty())
        .collect();
    for dir in extra {
        if !out.iter().any(|p| p == dir) {
            out.push(dir.clone());
        }
    }
    // `join_paths`, not `join(":")`: the separator is the platform's, and
    // `split_paths` above already reads it that way — a hand-written colon
    // would be a Unix assumption sitting in a module whose whole subject is
    // that environments differ. It refuses a directory containing the
    // separator, which cannot be represented; dropping the repair is the
    // right answer there, since the original `PATH` is still usable.
    match std::env::join_paths(&out) {
        Ok(joined) => joined.to_string_lossy().into_owned(),
        Err(e) => {
            tracing::warn!("PATH left as it was — it cannot be rebuilt: {e}");
            current.to_string()
        }
    }
}

/// This process's `PATH` with the usual binary directories appended, computed
/// once.
///
/// Directories that do not exist are left out rather than added hopefully, so
/// the value stays honest about this machine — and so a Linux box does not
/// carry two Homebrew prefixes it has never had.
pub fn path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let current = std::env::var("PATH").unwrap_or_default();
        let missing: Vec<String> = candidates(&home)
            .into_iter()
            .filter(|d| Path::new(d).is_dir())
            .filter(|d| !std::env::split_paths(&current).any(|p| p.to_string_lossy() == d.as_str()))
            .collect();
        if !missing.is_empty() {
            // Said out loud once, because "which PATH did the daemon have" is
            // the first question of every report that a binary it can plainly
            // see is missing.
            tracing::info!("PATH extended with {}", missing.join(", "));
        }
        merged(&current, &missing)
    })
}

/// The first `program` on [`path`], as an absolute path.
fn find_in(search: &str, program: &str, is_file: impl Fn(&Path) -> bool) -> Option<String> {
    std::env::split_paths(search)
        .map(|dir| dir.join(program))
        .find(|cand| is_file(cand))
        .map(|cand| cand.to_string_lossy().into_owned())
}

/// Where `program` is, or the bare name when nothing on [`path`] answers.
///
/// A `program` that already contains a separator is a path, not a name, and is
/// handed back untouched — the rule `execvp` itself follows. Without it
/// `Path::join` would quietly absorb an absolute argument (`dir.join("/bin/zsh")`
/// is `/bin/zsh`, whatever `dir` was), so the answer would come out right for
/// the wrong reason, and a relative `./script` would be hunted for on `PATH`
/// where it was never meant to be found.
///
/// Falling back to the bare name is deliberate, and the same choice
/// `claude_binary` makes: if this cannot find it, the error should be the
/// operating system's own "no such file", which at least names what it looked
/// for.
pub fn which(program: &str) -> String {
    if program.contains(std::path::MAIN_SEPARATOR) {
        return program.to_string();
    }
    find_in(path(), program, |p| p.is_file()).unwrap_or_else(|| program.to_string())
}

/// A [`Command`] for `program` that can both be found and find things itself.
///
/// Use this instead of `Command::new` for anything the daemon spawns. See the
/// module docs for why the two halves are separate.
pub fn command(program: &str) -> Command {
    let mut c = Command::new(which(program));
    c.env("PATH", path());
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_directories_are_appended_in_order() {
        let extra = vec!["/opt/homebrew/bin".to_string(), "/usr/local/bin".to_string()];
        assert_eq!(
            merged("/usr/bin:/bin", &extra),
            "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin"
        );
    }

    /// The whole point of appending: a daemon started from a shell keeps the
    /// order its user chose, and a directory already there does not move.
    #[test]
    fn a_directory_already_present_is_not_moved_or_repeated() {
        let extra = vec!["/opt/homebrew/bin".to_string()];
        assert_eq!(
            merged("/opt/homebrew/bin:/usr/bin", &extra),
            "/opt/homebrew/bin:/usr/bin"
        );
    }

    /// An empty `PATH` is what a launchd process can have when even the
    /// default is unset. The result must be usable rather than start with a
    /// separator, which a shell reads as "the current directory".
    #[test]
    fn an_empty_path_gains_no_empty_entry() {
        let extra = vec!["/opt/homebrew/bin".to_string()];
        assert_eq!(merged("", &extra), "/opt/homebrew/bin");
    }

    /// Homebrew is the one that mattered: `tmux` lives there on every Apple
    /// silicon Mac, and the launchd `PATH` a bundle gets is exactly the four
    /// system directories.
    #[test]
    fn the_launchd_path_gains_the_homebrew_prefix() {
        let list = candidates("/Users/someone");
        assert!(list.iter().any(|d| d == "/opt/homebrew/bin"));
        assert!(list.iter().any(|d| d == "/Users/someone/.local/bin"));
        let repaired = merged("/usr/bin:/bin:/usr/sbin:/sbin", &list);
        assert!(repaired.contains("/opt/homebrew/bin"));
        assert!(repaired.starts_with("/usr/bin:/bin:/usr/sbin:/sbin:"));
    }

    /// A path is not a name. `CommandBuilder` is handed `/bin/zsh` for the
    /// terminal panel and `tmux` for an Agent pane, and only the second is a
    /// `PATH` question.
    #[test]
    fn a_program_given_as_a_path_is_handed_back_untouched() {
        assert_eq!(which("/bin/zsh"), "/bin/zsh");
        assert_eq!(which("./scripts/yolomo"), "./scripts/yolomo");
    }

    /// The bug this module exists for, as a unit: with Homebrew on the search
    /// list, `tmux` resolves; with the launchd `PATH` alone, it does not, and
    /// the caller gets the bare name so the failure says what it looked for.
    #[test]
    fn tmux_is_found_on_the_repaired_path_and_not_on_the_launchd_one() {
        let pretend = |p: &Path| p == Path::new("/opt/homebrew/bin/tmux");
        assert_eq!(
            find_in("/usr/bin:/bin:/opt/homebrew/bin", "tmux", pretend).as_deref(),
            Some("/opt/homebrew/bin/tmux")
        );
        assert_eq!(find_in("/usr/bin:/bin", "tmux", pretend), None);
    }
}
