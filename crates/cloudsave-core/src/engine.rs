use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::backend::{backend_for, Backend};
use crate::config::Config;
use crate::state::State;
use crate::Result;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SyncReport {
    pub uploaded: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub bytes: u64,
    pub errors: Vec<String>,
}

struct HashingReader<R: Read> {
    inner: R,
    hasher: blake3::Hasher,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

fn mtime_secs(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn sync(config: &Config, state_path: &Path) -> Result<SyncReport> {
    let backend = backend_for(&config.target)?;
    let mut state = State::open(state_path)?;
    let mut report = SyncReport::default();

    let mut scanned: HashMap<String, (PathBuf, i64, i64)> = HashMap::new();
    for folder in &config.folders {
        if !folder.is_dir() {
            report
                .errors
                .push(format!("Ordner nicht gefunden: {}", folder.display()));
            continue;
        }
        let prefix = folder
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("root"));
        for entry in walkdir::WalkDir::new(folder).follow_links(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let abs = entry.path().to_path_buf();
            let meta = match fs::metadata(&abs) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let rel = match abs.strip_prefix(folder) {
                Ok(r) => prefix.join(r),
                Err(_) => continue,
            };
            let key = rel.to_string_lossy().to_string();
            scanned.insert(key, (abs, meta.len() as i64, mtime_secs(&meta)));
        }
    }

    for (key, (abs, size, mtime)) in &scanned {
        let unchanged = match state.get(key)? {
            Some(entry) => entry.size == *size && entry.mtime == *mtime,
            None => false,
        };
        if unchanged {
            report.skipped += 1;
            continue;
        }
        match upload_one(&*backend, key, abs, *size, *mtime, &mut state, &mut report) {
            Ok(()) => {}
            Err(e) => report.errors.push(format!("{}: {}", key, e)),
        }
    }

    let known = state.all_paths()?;
    for key in known {
        if !scanned.contains_key(&key) {
            match backend.delete(Path::new(&key)) {
                Ok(()) => {
                    state.remove(&key)?;
                    report.deleted += 1;
                }
                Err(e) => report.errors.push(format!("löschen {}: {}", key, e)),
            }
        }
    }

    Ok(report)
}

fn upload_one(
    backend: &dyn Backend,
    key: &str,
    abs: &Path,
    size: i64,
    mtime: i64,
    state: &mut State,
    report: &mut SyncReport,
) -> Result<()> {
    let rel = Path::new(key);
    backend.mkdirs(rel)?;
    let file = fs::File::open(abs)?;
    let mut reader = HashingReader {
        inner: file,
        hasher: blake3::Hasher::new(),
    };
    let written = backend.put(rel, &mut reader)?;
    let hash = reader.hasher.finalize().to_hex().to_string();
    state.upsert(key, size, mtime, &hash)?;
    report.uploaded += 1;
    report.bytes += written;
    Ok(())
}
