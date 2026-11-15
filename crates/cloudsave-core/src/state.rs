use rusqlite::{params, Connection};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Result;

pub struct State {
    conn: Connection,
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
            .execute("DELETE FROM files WHERE path = ?1", params![path])?;
        Ok(())
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
