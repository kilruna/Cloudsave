use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{Result, Target};

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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS successful_syncs (
                targets TEXT PRIMARY KEY,
                completed_at INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(State { conn })
    }

    pub fn last_successful_sync(&self, targets: &[Target]) -> Result<Option<i64>> {
        let value = self.conn.query_row(
            "SELECT MAX(completed_at) FROM successful_syncs WHERE targets = ?1",
            [Self::target_set(targets)?],
            |row| row.get(0),
        )?;
        Ok(value)
    }

    pub(crate) fn record_successful_sync(&self, targets: &[Target]) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO successful_syncs (targets, completed_at) VALUES (?1, ?2)",
            params![Self::target_set(targets)?, now_secs()],
        )?;
        Ok(())
    }

    fn target_set(targets: &[Target]) -> Result<String> {
        let mut keys = targets
            .iter()
            .map(crate::engine::target_prefix)
            .collect::<Result<Vec<_>>>()?;
        keys.sort_unstable();
        keys.dedup();
        Ok(keys.join(":"))
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

    pub(crate) fn update_mtime(&self, path: &str, mtime: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET mtime = ?2 WHERE path = ?1",
            params![path, mtime],
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

    pub(crate) fn remove_folder_records(&self, prefix: &str, source: &Path) -> Result<usize> {
        let transaction = self.conn.unchecked_transaction()?;
        let params = params![prefix, source.to_string_lossy()];
        let removed = transaction.execute(
            "DELETE FROM files WHERE path IN (
                SELECT path FROM file_sources WHERE substr(path, 1, length(?1)) = ?1 AND source = ?2
            )",
            params,
        )?;
        transaction.execute(
            "DELETE FROM file_sources WHERE substr(path, 1, length(?1)) = ?1 AND source = ?2",
            params,
        )?;
        transaction.commit()?;
        Ok(removed)
    }

    pub(crate) fn sources_for_target(&self, target_key: &str) -> Result<Vec<std::path::PathBuf>> {
        let prefix = format!("{target_key}::");
        let mut query = self.conn.prepare(
            "SELECT DISTINCT source FROM file_sources WHERE substr(path, 1, length(?1)) = ?1",
        )?;
        let rows = query.query_map([prefix], |row| row.get::<_, String>(0).map(Into::into))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn count(&self) -> Result<i64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(n)
    }

    /// Report copies at the currently configured targets without discarding tracked backups.
    pub fn stats_for_targets(&self, targets: &[Target]) -> Result<(i64, i64, Option<i64>)> {
        let mut query = self.conn.prepare(
            "SELECT COUNT(*), COALESCE(SUM(size), 0), MAX(synced_at) FROM files WHERE path GLOB ?1",
        )?;
        let mut stats = (0, 0, None);
        let mut seen = HashSet::new();
        for target in targets {
            let pattern = format!("{}::*", crate::engine::target_prefix(target)?);
            if !seen.insert(pattern.clone()) {
                continue;
            }
            let (count, size, last): (i64, i64, Option<i64>) =
                query.query_row([pattern], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            stats.0 += count;
            stats.1 += size;
            stats.2 = stats.2.max(last);
        }
        Ok(stats)
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
