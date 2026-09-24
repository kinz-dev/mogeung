//! The easter egg's history files. See `desktop/src/lib/arcade.ts`.
//!
//! `~/.mogeung/<game>.jsonl` — one finished session per line, appended and
//! never rewritten, one file per game: `mental-math.jsonl`, `trading-game.jsonl`.
//! Asked for 2026-09-23: *"record it to some local file, so that after a few
//! games, we can build analytics"*. A file rather than the webview's
//! `localStorage` because a record you mean to keep should survive a cleared
//! cache and be yours to read, copy or delete with ordinary tools.
//!
//! The window's files, like `connections.json` beside them (ADR-0036): the
//! daemon never reads them, and they hold nothing but games. The game name is
//! checked against a fixed list, so a caller cannot name a path.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The games that keep a history. A name is a file name, so it is never
/// taken from the caller unchecked.
pub const GAMES: &[&str] = &["mental-math", "trading-game"];

/// Same `HOME` idiom as every other path under `~/.mogeung`.
pub fn path_for(game: &str) -> Result<PathBuf, String> {
    if !GAMES.contains(&game) {
        return Err(format!("no such game: {game}"));
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Ok(PathBuf::from(home).join(".mogeung").join(format!("{game}.jsonl")))
}
/// Append one game.
///
/// A record is **one line of JSON**, checked here rather than trusted: a
/// newline inside one would split it into two unreadable halves, and a line
/// that is not JSON would be skipped by every reader after it.
pub fn append_to(path: &Path, line: &str) -> Result<(), String> {
    let line = line.trim();
    if line.is_empty() || line.contains('\n') || line.contains('\r') {
        return Err("a game record is one line".into());
    }
    if serde_json::from_str::<serde_json::Value>(line).is_err() {
        return Err("a game record is JSON".into());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("could not open {}: {e}", path.display()))?;
    // One write per line, so a crash mid-record leaves at worst a truncated
    // last line — which the reader skips — never an interleaving.
    f.write_all(format!("{line}\n").as_bytes())
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Everything recorded so far. No file yet is no games yet, not an error.
pub fn read_from(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("could not read {}: {e}", path.display())),
    }
}

#[tauri::command]
pub async fn arcade_append(game: String, line: String) -> Result<(), String> {
    append_to(&path_for(&game)?, &line)
}

#[tauri::command]
pub async fn arcade_read(game: String) -> Result<String, String> {
    read_from(&path_for(&game)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mogeung-arcade-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("mental-math.jsonl")
    }

    #[test]
    fn games_accumulate_one_line_each_and_read_back_whole() {
        let path = temp("round");
        append_to(&path, r#"{"v":1,"correct":3}"#).unwrap();
        append_to(&path, "  {\"v\":1,\"correct\":5}  ").unwrap();
        assert_eq!(read_from(&path).unwrap(), "{\"v\":1,\"correct\":3}\n{\"v\":1,\"correct\":5}\n");
        let _ = std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn a_record_that_would_break_the_file_is_refused() {
        let path = temp("refuse");
        assert!(append_to(&path, "{\"a\":1}\n{\"b\":2}").is_err(), "two lines");
        assert!(append_to(&path, "not json").is_err());
        assert!(append_to(&path, "   ").is_err());
        assert_eq!(read_from(&path).unwrap(), "", "nothing was written");
    }

    #[test]
    fn only_a_known_game_names_a_file() {
        assert!(path_for("mental-math").unwrap().ends_with(".mogeung/mental-math.jsonl"));
        assert!(path_for("trading-game").unwrap().ends_with(".mogeung/trading-game.jsonl"));
        assert!(path_for("../../.bashrc").is_err());
        assert!(path_for("connections").is_err());
    }

    #[test]
    fn no_file_yet_is_no_games_yet() {
        assert_eq!(read_from(&temp("absent")).unwrap(), "");
    }
}
