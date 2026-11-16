use std::collections::{HashMap, HashSet};
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

fn mtime_stamp(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
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
    let mut report = SyncReport::default();
    if config.targets.is_empty() {
        report
            .errors
            .push("Kein Endpunkt konfiguriert. Bitte einen Endpunkt hinzufügen.".into());
        return Ok(report);
    }
    for target in &config.targets {
        if let Target::Local { path } = target {
            let destination = crate::paths::resolved(path)?;
            for folder in &config.folders {
                let source = crate::paths::resolved(folder)?;
                if destination.starts_with(&source) || source.starts_with(&destination) {
                    report.errors.push(format!("Quelle {} und Ziel {} überlappen sich. Bitte einen getrennten Sicherungsordner wählen.", folder.display(), path.display()));
                }
            }
        }
    }
    if !report.errors.is_empty() {
        return Ok(report);
    }
    let exclusions = crate::exclusions::Exclusions::new(&config.settings.exclude_patterns)?;
    let mut state = State::open(state_path)?;

    let mut scanned: HashMap<String, (PathBuf, i64, i64)> = HashMap::new();
    let mut sources = HashMap::new();
    let mut complete_sources = HashSet::new();
    let mut namespaces = HashMap::new();
    for folder in &config.folders {
        let name = folder
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("root"));
        *namespaces.entry(name).or_insert(0) += 1;
    }
    for folder in &config.folders {
        let name = folder
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("root"));
        if namespaces[&name] > 1 {
            report.errors.push(format!("Quelle {}: mehrere Quellordner haben denselben Zielnamen. Sicherungen bleiben erhalten.", folder.display()));
            continue;
        }
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
        let mut complete = true;
        for entry in walkdir::WalkDir::new(folder)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !exclusions
                        .matches(entry.path().strip_prefix(folder).unwrap_or(entry.path()))
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    report
                        .errors
                        .push(format!("Quelle {}: {}", folder.display(), e));
                    complete = false;
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let abs = entry.path().to_path_buf();
            let meta = match fs::metadata(&abs) {
                Ok(m) => m,
                Err(e) => {
                    report
                        .errors
                        .push(format!("Quelle {}: {}", abs.display(), e));
                    complete = false;
                    continue;
                }
            };
            let rel = match abs.strip_prefix(folder) {
                Ok(r) => prefix.join(r),
                Err(_) => continue,
            };
            let key = rel.to_string_lossy().to_string();
            sources.insert(key.clone(), folder.clone());
            scanned.insert(key, (abs, meta.len() as i64, mtime_stamp(&meta)));
        }
        if complete {
            complete_sources.insert(folder.clone());
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
        let mut deletable_sources = complete_sources.clone();
        for (key, (abs, size, mtime)) in &scanned {
            let skey = format!("{}::{}", prefix, key);
            let source = &sources[key];
            if state.source(&skey)?.is_some_and(|owner| owner != *source) {
                report.errors.push(format!(
                    "{}: Quelle {} kollidiert mit vorhandener Sicherung {}",
                    target_label(target),
                    source.display(),
                    key
                ));
                deletable_sources.remove(source);
                continue;
            }
            let unchanged = match state.get(&skey)? {
                Some(entry) if entry.size == *size && entry.mtime == *mtime && *mtime != 0 => true,
                Some(entry) if entry.size == *size => match file_hash(abs) {
                    Ok(hash) if hash == entry.hash => {
                        // Touches and legacy second-based stamps do not require re-uploading identical data.
                        state.upsert(&skey, *size, *mtime, &hash)?;
                        true
                    }
                    Ok(_) => false,
                    Err(error) => {
                        deletable_sources.remove(source);
                        report.errors.push(format!(
                            "{} · Quelle {}: {error}",
                            target_label(target),
                            abs.display()
                        ));
                        continue;
                    }
                },
                Some(_) => false,
                None => false,
            };
            if unchanged {
                state.set_source(&skey, source)?;
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
                Ok(()) => state.set_source(&skey, source)?,
                Err(e) => {
                    deletable_sources.remove(source);
                    report.errors.push(format!(
                        "{} · Quelle {} · Datei {}: {}",
                        target_label(target),
                        source.display(),
                        abs.display(),
                        e
                    ));
                }
            }
        }

        if config.settings.sync_mode == crate::SyncMode::Backup {
            continue;
        }
        let p = format!("{}::", prefix);
        for key in state.all_paths()? {
            if let Some(rel) = key.strip_prefix(&p) {
                let relative: PathBuf = Path::new(rel).components().skip(1).collect();
                if exclusions.matches(&relative) {
                    continue;
                }
                let source_complete = state
                    .source(&key)?
                    .is_some_and(|source| deletable_sources.contains(&source));
                if source_complete && !scanned.contains_key(rel) {
                    match backend.delete(Path::new(rel)) {
                        Ok(()) => {
                            state.remove(&key)?;
                            report.deleted += 1;
                        }
                        Err(e) => report.errors.push(format!(
                            "{} · löschen {}: {}",
                            target_label(target),
                            rel,
                            e
                        )),
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
    let hash = file_hash(&abs)?;
    let f = fs::File::open(&abs)?;
    let written = backend.put(rel, f, size as u64)?;
    state.upsert(state_key, size, mtime, &hash)?;
    report.uploaded += 1;
    report.bytes += written;
    Ok(())
}

fn file_hash(path: &Path) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut file = fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}
