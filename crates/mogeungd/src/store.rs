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

            -- The chat panel's conversations (R-O9, ADR-0032). Half the user's
            -- own writing and half the model's answer, which is why it is kept
            -- like a note and pruned like a log: `CHAT_KEEP` conversations,
            -- oldest first out, plus an explicit delete per row.
            --
            -- `turns` is the whole conversation as one JSON array rather than
            -- a row per turn. It is read whole, written whole and sent whole —
            -- the wire has never carried half a conversation — so a turns
            -- table would be normalisation nothing asks for, and it would make
            -- the common write (append one exchange) a delete-and-reinsert
            -- anyway.
            --
            -- `n_turns` is stored rather than counted from `turns` with
            -- `json_array_length`. Two reasons, and a test found the first:
            -- that function errors the **whole listing** if any single row's
            -- JSON is malformed, so one bad row would cost the history rather
            -- than one door. It also needs the JSON1 extension, which is not
            -- worth depending on for a number we already know at write time.
            CREATE TABLE IF NOT EXISTS chats (
                id      TEXT PRIMARY KEY,
                title   TEXT NOT NULL,
                turns   TEXT NOT NULL,
                n_turns INTEGER NOT NULL DEFAULT 0,
                created INTEGER NOT NULL,
                updated INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS chats_by_updated ON chats (updated DESC);

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
    // Chat conversations (R-O9, ADR-0032)
    // -----------------------------------------------------------------------

    /// How many conversations are kept before the oldest falls off.
    ///
    /// A cap and not "for ever", which is the one place this differs from a
    /// note: a note is written on purpose, one at a time, and a conversation
    /// accumulates simply by using the panel. Two hundred is far past what a
    /// fortnight of `A37` would produce and small enough that the whole list
    /// is one cheap query — the point is that the number exists, not that it
    /// is exactly this.
    pub const CHAT_KEEP: usize = 200;

    /// Every conversation, newest first, **without** its turns.
    pub fn load_chats(&self) -> Result<Vec<mogeung_core::wire::ChatSummary>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare(
            "SELECT id, title, created, updated, n_turns FROM chats ORDER BY updated DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(mogeung_core::wire::ChatSummary {
                id: r.get(0)?,
                title: r.get(1)?,
                created: r.get(2)?,
                updated: r.get(3)?,
                turns: r.get::<_, i64>(4).unwrap_or(0).max(0) as u32,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// One conversation's turns.
    ///
    /// A row whose JSON will not parse comes back as **empty rather than as an
    /// error**: this is a panel, the corpus parsers' degrade-never-panic rule
    /// applies to our own writing too, and a history that refuses to open
    /// because one row is bad is worse than one door that leads nowhere.
    pub fn load_chat(&self, id: &str) -> Result<Vec<mogeung_core::model::ChatTurn>> {
        let c = self.conn.lock().unwrap();
        let json: Option<String> = c
            .query_row("SELECT turns FROM chats WHERE id = ?1", params![id], |r| {
                r.get(0)
            })
            .ok();
        Ok(json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default())
    }

    /// Insert or replace one conversation, then prune to [`Self::CHAT_KEEP`].
    ///
    /// The title is only written on insert — `ON CONFLICT` leaves it alone —
    /// because it is the *first* thing you asked, and a conversation renaming
    /// itself to your latest question as you go is a list you cannot scan.
    pub fn save_chat(
        &self,
        id: &str,
        title: &str,
        turns: &[mogeung_core::model::ChatTurn],
        now: i64,
    ) -> Result<()> {
        let json = serde_json::to_string(turns)?;
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT INTO chats (id, title, turns, n_turns, created, updated) \
             VALUES (?1, ?2, ?3, ?5, ?4, ?4) \
             ON CONFLICT(id) DO UPDATE SET turns = ?3, n_turns = ?5, updated = ?4",
            params![id, title, json, now, turns.len() as i64],
        )?;
        // Oldest out. `updated` and not `created`, so a conversation you came
        // back to yesterday outlives one you abandoned last month.
        c.execute(
            "DELETE FROM chats WHERE id NOT IN              (SELECT id FROM chats ORDER BY updated DESC LIMIT ?1)",
            params![Self::CHAT_KEEP as i64],
        )?;
        Ok(())
    }

    pub fn delete_chat(&self, id: &str) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM chats WHERE id = ?1", params![id])?;
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
        self.append_events(std::slice::from_ref(ev))
    }

    /// Append a batch of events in **one** transaction. `R-J66`.
    ///
    /// The per-event version this wraps took the connection mutex, opened an
    /// autocommit transaction and ran its own `serde_json::to_string` for
    /// every event — and a fold emits events in runs, so a busy tick was a
    /// stream of tiny transactions. That is what the measured 230 write
    /// syscalls per tick were.
    ///
    /// The durability trade is real and small: a crash mid-pass now loses that
    /// pass's events rather than none. `R-A6` already tolerates exactly this —
    /// a transcript's tail offset is recorded only *after* the lines it covers
    /// are folded, so an interrupted pass re-reads its batch rather than
    /// skipping it, and the re-read rebuilds what was lost.
    pub fn append_events(&self, evs: &[TranscriptEvent]) -> Result<()> {
        if evs.is_empty() {
            return Ok(());
        }
        let mut c = self.conn.lock().unwrap();
        let tx = c.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO events (run_id, seq, json) VALUES (?1, ?2, ?3)",
            )?;
            for ev in evs {
                stmt.execute(params![
                    ev.session_id.as_str(),
                    ev.seq,
                    serde_json::to_string(ev)?
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The newest `limit` events past `since`, ascending. The replay path's
    /// loader: a session alive for months holds an unbounded event log, and
    /// `load_events` materialises all of it into one Vec and one giant wire
    /// frame — which the client then trims to its own cap anyway. Serving the
    /// newest window from the start keeps the daemon's memory and the frame
    /// proportional to what will actually be kept.
    pub fn load_recent_events(
        &self,
        run_id: &str,
        since: u64,
        limit: u64,
    ) -> Result<Vec<TranscriptEvent>> {
        let c = self.conn.lock().unwrap();
        let mut stmt = c.prepare(
            "SELECT json FROM events WHERE run_id = ?1 AND seq > ?2 \
             ORDER BY seq DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![run_id, since, limit as i64], |r| {
            r.get::<_, String>(0)
        })?;
        let mut out = Vec::new();
        for r in rows {
            if let Ok(ev) = serde_json::from_str(&r?) {
                out.push(ev);
            }
        }
        // Newest-first from the query; ascending is the wire order.
        out.reverse();
        Ok(out)
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

    /// The replay loader must serve the *newest* window in ascending order —
    /// a cap that kept the oldest events would replay the part of the
    /// conversation furthest from what anyone is reading.
    #[test]
    fn recent_events_are_the_newest_window_in_order() {
        let s = store("recent");
        for seq in 1..=10u64 {
            s.append_event(&TranscriptEvent {
                session_id: "a".into(),
                seq,
                ts: chrono::Utc::now(),
                kind: mogeung_core::EventKind::AssistantText {
                    text: format!("t{seq}"),
                },
            })
            .unwrap();
        }
        let got = s.load_recent_events("a", 0, 4).unwrap();
        let seqs: Vec<u64> = got.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![7, 8, 9, 10]);
        // `since` still means "past this", inside the window.
        let got = s.load_recent_events("a", 8, 4).unwrap();
        let seqs: Vec<u64> = got.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![9, 10]);
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
    /// A batch must land exactly as the per-event loop landed it — same rows,
    /// same order, same `seq`s. `R-J66` swapped N autocommit transactions for
    /// one, and the whole justification is that only the transaction boundary
    /// moved.
    #[test]
    fn a_batched_append_stores_what_the_per_event_one_did() {
        let s = store("batch");
        let ev = |sid: &str, seq: u64| TranscriptEvent {
            session_id: sid.into(),
            seq,
            ts: chrono::Utc::now(),
            kind: mogeung_core::transcript::EventKind::UserPrompt {
                text: format!("p{seq}"),
            },
        };

        // One at a time, the old way.
        for seq in 1..=3 {
            s.append_event(&ev("one", seq)).unwrap();
        }
        // The same run, batched.
        let batch: Vec<TranscriptEvent> = (1..=3).map(|seq| ev("many", seq)).collect();
        s.append_events(&batch).unwrap();

        let a = s.load_recent_events("one", 0, 100).unwrap();
        let b = s.load_recent_events("many", 0, 100).unwrap();
        assert_eq!(a.len(), 3);
        assert_eq!(
            a.iter().map(|e| e.seq).collect::<Vec<_>>(),
            b.iter().map(|e| e.seq).collect::<Vec<_>>(),
            "same seqs, ascending, either way"
        );

        // An empty batch is a no-op, not an empty transaction.
        s.append_events(&[]).unwrap();
        assert_eq!(s.load_recent_events("many", 0, 100).unwrap().len(), 3);
    }

    fn turn(role: &str, content: &str) -> mogeung_core::model::ChatTurn {
        mogeung_core::model::ChatTurn { role: role.into(), content: content.into() }
    }

    /// The three properties a conversation store has that a note store does
    /// not. `R-O9`, ADR-0032.
    #[test]
    fn a_conversation_keeps_its_first_title_and_the_oldest_falls_off() {
        let s = store("chats");

        // Growing a conversation replaces its turns and moves `updated` —
        // and leaves the **title** alone. A list that renamed itself to your
        // latest question as you went is a list you cannot scan.
        let first = vec![turn("user", "why is the queue empty"), turn("assistant", "because")];
        s.save_chat("c1", "why is the queue empty", &first, 100).unwrap();
        let grown = [first.clone(), vec![turn("user", "and now"), turn("assistant", "ok")]].concat();
        s.save_chat("c1", "and now", &grown, 200).unwrap();

        let list = s.load_chats().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "why is the queue empty", "the title is the first ask");
        assert_eq!(list[0].turns, 4, "the turn count follows the thread");
        assert_eq!(list[0].created, 100, "created does not move");
        assert_eq!(list[0].updated, 200);
        assert_eq!(s.load_chat("c1").unwrap(), grown);

        s.delete_chat("c1").unwrap();
        assert!(s.load_chats().unwrap().is_empty());
        assert!(s.load_chat("c1").unwrap().is_empty(), "gone is gone");
    }

    /// Pruning is by `updated`, not `created`: a conversation you came back to
    /// yesterday outlives one you abandoned last month. Getting this backwards
    /// would throw away exactly the threads worth keeping, and silently.
    #[test]
    fn the_cap_drops_the_least_recently_touched() {
        let s = store("chats-cap");
        let body = vec![turn("user", "q"), turn("assistant", "a")];
        // Oldest `created` of the lot, but touched most recently.
        s.save_chat("ancient", "ancient", &body, 1).unwrap();
        for n in 0..Store::CHAT_KEEP {
            s.save_chat(&format!("c{n}"), "filler", &body, 1_000 + n as i64).unwrap();
        }
        s.save_chat("ancient", "ancient", &body, 10_000).unwrap();

        let list = s.load_chats().unwrap();
        assert_eq!(list.len(), Store::CHAT_KEEP, "capped");
        assert!(list.iter().any(|c| c.id == "ancient"), "revisiting keeps it alive");
        assert!(!list.iter().any(|c| c.id == "c0"), "the least recently touched went");
    }

    /// Our own writing degrades the way the corpus parsers do (`A4`): a row
    /// whose JSON will not parse opens empty rather than failing the call, so
    /// one bad row costs one door and not the whole history.
    #[test]
    fn an_unreadable_conversation_is_empty_not_an_error() {
        let s = store("chats-bad");
        s.save_chat("c", "t", &[turn("user", "q")], 1).unwrap();
        {
            let c = s.conn.lock().unwrap();
            c.execute("UPDATE chats SET turns = 'not json' WHERE id = 'c'", []).unwrap();
        }
        assert!(s.load_chat("c").unwrap().is_empty());
        assert_eq!(s.load_chats().unwrap().len(), 1, "the row is still listed, so it can be deleted");
    }

}
