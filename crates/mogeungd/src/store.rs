//! SQLite persistence.
//!
//! Runs and events are stored as JSON blobs keyed by id. That is a deliberate
//! v0.1 choice: the schema is still moving, and at this scale query planning
//! matters far less than being able to change `Run` without a migration.

use anyhow::Result;
use mogeung_core::{Session, TranscriptEvent};
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

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

            
            "#,
        )?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
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



}
