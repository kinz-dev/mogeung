//! SQLite persistence.
//!
//! Runs and events are stored as JSON blobs keyed by id. That is a deliberate
//! v0.1 choice: the schema is still moving, and at this scale query planning
//! matters far less than being able to change `Run` without a migration.

use anyhow::Result;
use mogeung_core::wire::Note;
use mogeung_core::{Session, TranscriptEvent};
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

/// Bumped when a stored *value* needs repairing, not when a table is added —
/// every `CREATE TABLE` here is `IF NOT EXISTS` and needs no version at all.
///
/// 1: transcripts were re-read from byte 0 on every restart, so events and
///    counters carry one extra copy per restart. See
///    `AppState::repair_reingested_history`.
pub const SCHEMA_VERSION: u32 = 1;

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            -- Without a limit SQLite reuses the WAL file but never shrinks it,
            -- and the busy-session write pattern here (a fat session row per
            -- flush, an event row per transcript line) had it sitting at 80%
            -- of the database's own size. Truncated back at checkpoint time.
            PRAGMA journal_size_limit = 4194304;

            CREATE TABLE IF NOT EXISTS sessions (
                id         TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                json       TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                run_id TEXT NOT NULL,
                seq    INTEGER NOT NULL,
                json   TEXT NOT NULL,
                PRIMARY KEY (run_id, seq)
            );

            CREATE TABLE IF NOT EXISTS reviewed (
                run_id TEXT NOT NULL,
                anchor TEXT NOT NULL,
                PRIMARY KEY (run_id, anchor)
            );

            -- The user's own writing (R-B35, pillar L). Everything else in
            -- this database is derived and can be recomputed from ~/.claude
            -- and git; this cannot, which is why it is mirrored to disk as
            -- well. See ADR-0015.
            --
            -- `session_id` and `seq` together anchor a note to one turn of one
            -- transcript. Both are nullable and both are *tags*: a note keeps
            -- existing when the session is forgotten, which is the whole point
            -- of tagging rather than nesting.
            CREATE TABLE IF NOT EXISTS notes (
                id         TEXT PRIMARY KEY,
                body       TEXT NOT NULL,
                created    INTEGER NOT NULL,
                updated    INTEGER NOT NULL,
                session_id TEXT,
                seq        INTEGER,
                repo       TEXT
            );
            CREATE INDEX IF NOT EXISTS notes_by_session ON notes (session_id, seq);

            -- Per-repo signal command and its last run (R-E2). The command
            -- is the user's own, run only on an explicit click.
            CREATE TABLE IF NOT EXISTS signals (
                repo     TEXT PRIMARY KEY,
                command  TEXT NOT NULL,
                last_run TEXT
            );

            -- How far into each transcript we have read (R-A6). Keyed by path
            -- rather than session id because that is what the tailer keys on,
            -- and because a session's transcript path is not always known
            -- before its first line is folded.
            --
            -- This has to outlive the process. Without it a restart re-read
            -- every transcript from byte 0 and folded the whole history in
            -- again — new `seq`s, so nothing collided, and both the event log
            -- and every counted field grew by one copy per restart.
            CREATE TABLE IF NOT EXISTS tail_offsets (
                path   TEXT PRIMARY KEY,
                offset INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    /// Checkpoint the WAL and truncate it. Called from the retention pass —
    /// low-frequency by design; per-tick checkpointing would stall every
    /// reader behind the single connection for no benefit.
    pub fn checkpoint_truncate(&self) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// Session ids that a note is anchored to. The retention pass must not
    /// prune a session the user wrote something about: the note survives by
    /// design (ADR-0015), but a note pointing at a transcript that was
    /// silently deleted is a broken promise.
    pub fn noted_session_ids(&self) -> Result<HashSet<String>> {
        let c = self.conn.lock().unwrap();
        let mut stmt =
            c.prepare("SELECT DISTINCT session_id FROM notes WHERE session_id IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = HashSet::new();
        for r in rows {
            out.insert(r?);
        }
        Ok(out)
    }

    pub fn save_session(&self, run: &Session) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO sessions (id, created_at, json) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![
                run.id.clone(),
                run.started_at.to_rfc3339(),
                serde_json::to_string(run)?
            ],
        )?;
        Ok(())
    }

    pub fn load_sessions(&self) -> Result<Vec<Session>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare("SELECT json FROM sessions ORDER BY created_at ASC")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            // Tolerate rows written by an older schema rather than refusing to start.
            match serde_json::from_str::<Session>(&r?) {
                Ok(run) => out.push(run),
                Err(e) => tracing::warn!("skipping unreadable session row: {e}"),
            }
        }
        Ok(out)
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        c.execute("DELETE FROM events WHERE run_id = ?1", params![id])?;
        c.execute("DELETE FROM reviewed WHERE run_id = ?1", params![id])?;
        Ok(())
    }

    /// Every note, newest first. `R-B35`.
    ///
    /// The whole set rather than a page: this is the user's own writing, it is
    /// small by nature, and a client that has all of it can show "notes on
    /// this turn" without a round trip per turn.
    pub fn load_notes(&self) -> Result<Vec<Note>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare(
            "SELECT id, body, created, updated, session_id, seq, repo \
             FROM notes ORDER BY updated DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Note {
                id: r.get(0)?,
                body: r.get(1)?,
                created: r.get(2)?,
                updated: r.get(3)?,
                session_id: r.get(4)?,
                seq: r.get::<_, Option<i64>>(5)?.map(|v| v as u64),
                repo: r.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Insert or update one note.
    pub fn save_note(&self, n: &Note) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO notes (id, body, created, updated, session_id, seq, repo) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(id) DO UPDATE SET body = ?2, updated = ?4, \
             session_id = ?5, seq = ?6, repo = ?7",
            params![
                n.id,
                n.body,
                n.created,
                n.updated,
                n.session_id,
                n.seq.map(|v| v as i64),
                n.repo
            ],
        )?;
        Ok(())
    }

    pub fn delete_note(&self, id: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Tail offsets (R-A6)
    // -----------------------------------------------------------------------

    /// Every recorded read position, for seeding the tailer at startup.
    pub fn load_tail_offsets(&self) -> Result<Vec<(String, u64)>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare("SELECT path, offset FROM tail_offsets")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Record how far a transcript has been read.
    ///
    /// Written *after* the lines it covers have been folded in, so a crash
    /// mid-fold costs a re-read of one batch rather than losing it.
    pub fn save_tail_offset(&self, path: &str, offset: u64) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO tail_offsets (path, offset) VALUES (?1, ?2)
             ON CONFLICT(path) DO UPDATE SET offset = excluded.offset",
            params![path, offset as i64],
        )?;
        Ok(())
    }

    pub fn delete_tail_offset(&self, path: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM tail_offsets WHERE path = ?1", params![path])?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Schema version and repair
    // -----------------------------------------------------------------------

    pub fn schema_version(&self) -> u32 {
        let c = self.conn.lock().unwrap();
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .map(|v| v.max(0) as u32)
            .unwrap_or(0)
    }

    pub fn set_schema_version(&self, v: u32) -> Result<()> {
        let c = self.conn.lock().unwrap();
        // PRAGMA takes no bind parameters.
        c.execute_batch(&format!("PRAGMA user_version = {v};"))?;
        Ok(())
    }

    /// Drop one session's events. Used by the repair pass before re-folding a
    /// transcript that is still on disk.
    pub fn delete_events(&self, run_id: &str) -> Result<usize> {
        let c = self.conn.lock().unwrap();
        Ok(c.execute("DELETE FROM events WHERE run_id = ?1", params![run_id])?)
    }

    /// Collapse repeats of the same event, keeping the earliest `seq` of each.
    ///
    /// Identity is the whole stored event minus its `seq`: same session, same
    /// timestamp, same kind. That is a guess, not a fact — a transcript really
    /// can carry the same prompt twice on the same timestamp, and this would
    /// collapse the pair. It is the last resort, for sessions whose transcript
    /// is gone and which therefore cannot be rebuilt from the file; it repairs
    /// the log and can do nothing for a counter. Returns how many rows went.
    pub fn dedupe_events(&self, run_id: &str) -> Result<usize> {
        let c = self.conn.lock().unwrap();
        Ok(c.execute(
            "DELETE FROM events WHERE run_id = ?1 AND seq NOT IN (
                 SELECT MIN(seq) FROM events WHERE run_id = ?1
                 GROUP BY json_remove(json, '$.seq')
             )",
            params![run_id],
        )?)
    }

    /// Reclaim the space a repair freed. Slow and standalone by nature —
    /// callers run it once, not per pass.
    pub fn vacuum(&self) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute_batch("VACUUM;")?;
        Ok(())
    }

    pub fn append_event(&self, ev: &TranscriptEvent) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT OR REPLACE INTO events (run_id, seq, json) VALUES (?1, ?2, ?3)",
            params![ev.session_id.clone(), ev.seq, serde_json::to_string(ev)?],
        )?;
        Ok(())
    }

    pub fn load_events(&self, run_id: &str, since: u64) -> Result<Vec<TranscriptEvent>> {
        let c = self.conn.lock().unwrap();
        let mut stmt =
            c.prepare("SELECT json FROM events WHERE run_id = ?1 AND seq > ?2 ORDER BY seq ASC")?;
        let rows = stmt.query_map(params![run_id, since], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            if let Ok(ev) = serde_json::from_str(&r?) {
                out.push(ev);
            }
        }
        Ok(out)
    }

    pub fn max_seq(&self, run_id: &str) -> Result<u64> {
        let c = self.conn.lock().unwrap();
        let v: Option<i64> = c.query_row(
            "SELECT MAX(seq) FROM events WHERE run_id = ?1",
            params![run_id],
            |r| r.get(0),
        )?;
        Ok(v.unwrap_or(0) as u64)
    }

    pub fn set_reviewed(&self, run_id: &str, anchor: &str, reviewed: bool) -> Result<()> {
        let c = self.conn.lock().unwrap();
        if reviewed {
            c.execute(
                "INSERT OR IGNORE INTO reviewed (run_id, anchor) VALUES (?1, ?2)",
                params![run_id, anchor],
            )?;
        } else {
            c.execute(
                "DELETE FROM reviewed WHERE run_id = ?1 AND anchor = ?2",
                params![run_id, anchor],
            )?;
        }
        Ok(())
    }

    pub fn reviewed_anchors(&self, run_id: &str) -> Result<HashSet<String>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare("SELECT anchor FROM reviewed WHERE run_id = ?1")?;
        let rows = stmt.query_map(params![run_id], |r| r.get::<_, String>(0))?;
        let mut out = HashSet::new();
        for r in rows {
            out.insert(r?);
        }
        Ok(out)
    }

    /// The signal command configured for a repo, if any. `R-E2`.
    pub fn signal_command(&self, repo: &str) -> Option<String> {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT command FROM signals WHERE repo = ?1",
            params![repo],
            |r| r.get(0),
        )
        .ok()
    }

    /// Set (or, with an empty command, clear) a repo's signal command.
    pub fn set_signal_command(&self, repo: &str, command: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        if command.trim().is_empty() {
            c.execute("DELETE FROM signals WHERE repo = ?1", params![repo])?;
        } else {
            c.execute(
                "INSERT INTO signals (repo, command) VALUES (?1, ?2)
                 ON CONFLICT(repo) DO UPDATE SET command = excluded.command",
                params![repo, command],
            )?;
        }
        Ok(())
    }

    pub fn save_signal_run(&self, repo: &str, run_json: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute(
            "UPDATE signals SET last_run = ?2 WHERE repo = ?1",
            params![repo, run_json],
        )?;
        Ok(())
    }

    pub fn signal_last_run(&self, repo: &str) -> Option<String> {
        let c = self.conn.lock().unwrap();
        c.query_row(
            "SELECT last_run FROM signals WHERE repo = ?1",
            params![repo],
            |r| r.get::<_, Option<String>>(0),
        )
        .ok()
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("mogeung-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Store::open(&dir.join("t.db")).unwrap()
    }

    /// The retention pass keeps every session a note anchors to, so this list
    /// must contain exactly the anchored ids — a free-floating scratchpad note
    /// (no `session_id`) protects nothing. `R-J57`.
    #[test]
    fn noted_session_ids_are_the_anchored_ones_only() {
        let s = store("noted");
        let note = |id: &str, sid: Option<&str>| Note {
            id: id.into(),
            body: "b".into(),
            created: 1,
            updated: 1,
            session_id: sid.map(String::from),
            seq: None,
            repo: None,
        };
        s.save_note(&note("n1", Some("sess-a"))).unwrap();
        s.save_note(&note("n2", Some("sess-a"))).unwrap();
        s.save_note(&note("n3", None)).unwrap();
        let ids = s.noted_session_ids().unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.contains("sess-a"));
    }

    /// The WAL must actually shrink at checkpoint time — `journal_size_limit`
    /// is set at open, and this is the call the retention pass makes. On the
    /// machine this was written on the WAL had grown to 80% of the database.
    #[test]
    fn checkpointing_truncates_the_wal() {
        let s = store("wal");
        for i in 0..50 {
            s.save_tail_offset(&format!("/t/{i}.jsonl"), i).unwrap();
        }
        s.checkpoint_truncate().unwrap();
        // The path the store was opened with, next to its -wal sidecar.
        let dir = std::env::temp_dir().join(format!("mogeung-store-wal-{}", std::process::id()));
        let wal = std::fs::metadata(dir.join("t.db-wal")).map(|m| m.len()).unwrap_or(0);
        assert_eq!(wal, 0, "a truncated WAL is empty until the next write");
    }
}
