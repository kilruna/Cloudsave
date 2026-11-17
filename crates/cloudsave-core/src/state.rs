use rusqlite::{params, Connection};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::Result;

pub struct State {
    conn: Connection,
}

pub struct JobHistory {
    conn: Connection,
}

pub struct FinishedJob {
    pub finished_at: i64,
    pub label: String,
    pub detail: String,
}

impl JobHistory {
    pub const LIMIT: usize = 8;

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // Starting Cloudsave should stay responsive if another process has the history open.
        conn.busy_timeout(Duration::from_millis(150))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS jobs (
                id INTEGER PRIMARY KEY,
                finished_at INTEGER NOT NULL,
                label TEXT NOT NULL,
                detail TEXT NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn recent(&self) -> Result<Vec<FinishedJob>> {
        let mut query = self
            .conn
            .prepare("SELECT finished_at, label, detail FROM jobs ORDER BY id DESC LIMIT ?1")?;
        let rows = query.query_map([Self::LIMIT as i64], |row| {
            Ok(FinishedJob {
                finished_at: row.get(0)?,
                label: row.get(1)?,
                detail: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn record(&mut self, job: &FinishedJob) -> Result<()> {
        let transaction = self.conn.transaction()?;
        transaction.execute(
            "INSERT INTO jobs (finished_at, label, detail) VALUES (?1, ?2, ?3)",
            params![job.finished_at, job.label, job.detail],
        )?;
        transaction.execute(
            "DELETE FROM jobs WHERE id NOT IN (SELECT id FROM jobs ORDER BY id DESC LIMIT ?1)",
            [Self::LIMIT as i64],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub size: i64,
    pub mtime: i64,
    pub hash: String,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl State {
    pub fn open(path: &Path) -> Result<State> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                size INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                hash TEXT NOT NULL,
                synced_at INTEGER NOT NULL
            )",
            [],
        )?;
        // Keep source ownership across restarts and changes to the selected folders.
        // Legacy rows without ownership are retained until observed in a new scan.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS file_sources (
                path TEXT PRIMARY KEY,
                source TEXT NOT NULL
            )",
            [],
        )?;
        Ok(State { conn })
    }

    pub fn get(&self, path: &str) -> Result<Option<FileEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT size, mtime, hash FROM files WHERE path = ?1")?;
        let result = stmt.query_row(params![path], |row| {
            Ok(FileEntry {
                size: row.get(0)?,
                mtime: row.get(1)?,
                hash: row.get(2)?,
            })
        });
        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert(&self, path: &str, size: i64, mtime: i64, hash: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO files (path, size, mtime, hash, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![path, size, mtime, hash, now_secs()],
        )?;
        Ok(())
    }

    pub fn remove(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM file_sources WHERE path = ?1", params![path])?;
        self.conn
            .execute("DELETE FROM files WHERE path = ?1", params![path])?;
        Ok(())
    }

    pub(crate) fn set_source(&self, path: &str, source: &Path) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO file_sources (path, source) VALUES (?1, ?2)",
            params![path, source.to_string_lossy()],
        )?;
        Ok(())
    }

    pub(crate) fn source(&self, path: &str) -> Result<Option<std::path::PathBuf>> {
        let result = self.conn.query_row(
            "SELECT source FROM file_sources WHERE path = ?1",
            params![path],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(source) => Ok(Some(source.into())),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn count(&self) -> Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(n)
    }

    pub fn total_size(&self) -> Result<i64> {
        let n: i64 =
            self.conn
                .query_row("SELECT COALESCE(SUM(size), 0) FROM files", [], |row| {
                    row.get(0)
                })?;
        Ok(n)
    }

    pub fn last_synced(&self) -> Result<Option<i64>> {
        let mut stmt = self.conn.prepare("SELECT MAX(synced_at) FROM files")?;
        let result = stmt.query_row([], |row| row.get(0));
        match result {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn all_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM files")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut paths = Vec::new();
        for row in rows {
            paths.push(row?);
        }
        Ok(paths)
    }
}
