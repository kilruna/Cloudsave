use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::backend::{backend_for, Backend};
use crate::config::{Config, Target};
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

fn mtime_secs(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn target_prefix(target: &Target) -> Result<String> {
    let s = toml::to_string(target).map_err(|e| crate::Error(e.to_string()))?;
    Ok(blake3::hash(s.as_bytes()).to_hex()[..16].to_string())
}

pub fn target_label(target: &Target) -> String {
    match target {
        Target::Local { path } => format!("Lokal: {}", path.display()),
        Target::Webdav { url, user } => format!("WebDAV: {} ({})", url, user),
    }
}

pub fn sync(config: &Config, state_path: &Path) -> Result<SyncReport> {
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

    for target in &config.targets {
        let backend = match backend_for(target) {
            Ok(b) => b,
            Err(e) => {
                report
                    .errors
                    .push(format!("{}: {}", target_label(target), e));
                continue;
            }
        };
        let prefix = target_prefix(target)?;
        for (key, (abs, size, mtime)) in &scanned {
            let skey = format!("{}::{}", prefix, key);
            let unchanged = match state.get(&skey)? {
                Some(entry) => entry.size == *size && entry.mtime == *mtime,
                None => false,
            };
            if unchanged {
                report.skipped += 1;
                continue;
            }
            match upload_one(
                &*backend,
                &mut state,
                &mut report,
                &skey,
                Path::new(key),
                &(abs.clone(), *size, *mtime),
            ) {
                Ok(()) => {}
                Err(e) => report.errors.push(format!("{}: {}", skey, e)),
            }
        }

        let p = format!("{}::", prefix);
        for key in state.all_paths()? {
            if let Some(rel) = key.strip_prefix(&p) {
                if !scanned.contains_key(rel) {
                    match backend.delete(Path::new(rel)) {
                        Ok(()) => {
                            state.remove(&key)?;
                            report.deleted += 1;
                        }
                        Err(e) => report.errors.push(format!("löschen {}: {}", key, e)),
                    }
                }
            }
        }
    }

    Ok(report)
}

fn upload_one(
    backend: &dyn Backend,
    state: &mut State,
    report: &mut SyncReport,
    state_key: &str,
    rel: &Path,
    file: &(PathBuf, i64, i64),
) -> Result<()> {
    let (abs, size, mtime) = (file.0.clone(), file.1, file.2);
    backend.mkdirs(rel)?;
    let mut hasher = blake3::Hasher::new();
    {
        let mut f = fs::File::open(&abs)?;
        std::io::copy(&mut f, &mut hasher)?;
    }
    let hash = hasher.finalize().to_hex().to_string();
    let f = fs::File::open(&abs)?;
    let written = backend.put(rel, f, size as u64)?;
    state.upsert(state_key, size, mtime, &hash)?;
    report.uploaded += 1;
    report.bytes += written;
    Ok(())
}
