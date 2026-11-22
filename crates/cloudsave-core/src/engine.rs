use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use crate::backend::{backend_for, Backend};
use crate::config::{Config, Target};
use crate::state::State;
use crate::{retry, Error, FolderRemoval, Result, TransferControl};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SyncReport {
    pub uploaded: usize,
    pub deleted: usize,
    pub skipped: usize,
    pub bytes: u64,
    pub cancelled: bool,
    pub attempted: bool,
    pub errors: Vec<String>,
    pub targets: Vec<TargetSyncReport>,
    pub removed_folders: Vec<(PathBuf, Target)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetSyncReport {
    pub target: Target,
    pub unavailable: bool,
    pub errors: Vec<String>,
}

impl SyncReport {
    fn target_error(&mut self, index: usize, error: String) {
        self.targets[index].errors.push(error.clone());
        self.errors.push(error);
    }
}

/// Bounded parallelism for listings and uploads; WebDAV servers such as Hetzner
/// allow roughly ten concurrent connections, so stay well below that.
const TRANSFER_WORKERS: usize = 4;

struct FileTask {
    skey: String,
    key: String,
    abs: PathBuf,
    size: i64,
    mtime: i64,
    source: PathBuf,
    owner: Option<PathBuf>,
    refresh_mtime: bool,
}

pub(crate) fn mtime_stamp(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

pub(crate) fn target_prefix(target: &Target) -> Result<String> {
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
    sync_with_progress(config, state_path, |_| {})
}

pub fn sync_with_progress(
    config: &Config,
    state_path: &Path,
    progress: impl FnMut(crate::Progress),
) -> Result<SyncReport> {
    sync_controlled(config, state_path, &TransferControl::default(), progress)
}

pub fn sync_controlled(
    config: &Config,
    state_path: &Path,
    control: &TransferControl,
    progress: impl FnMut(crate::Progress),
) -> Result<SyncReport> {
    let mut report = SyncReport::default();
    let result = sync_run(config, state_path, control, progress, &mut report);
    if control.is_cancelled() {
        report.cancelled = true;
    } else {
        result?;
    }
    Ok(report)
}

fn sync_run(
    config: &Config,
    state_path: &Path,
    control: &TransferControl,
    mut progress: impl FnMut(crate::Progress),
    report: &mut SyncReport,
) -> Result<()> {
    control.checkpoint()?;
    if config.settings.pending_recovery.is_some() {
        return Err(crate::Error("Zuerst die Wiederherstellung abschließen oder bewusst überspringen. Synchronisierung bleibt pausiert.".into()));
    }
    progress(crate::Progress::Scanning);
    if config.targets.is_empty() {
        report
            .errors
            .push("Kein Endpunkt konfiguriert. Bitte einen Endpunkt hinzufügen.".into());
        return Ok(());
    }
    report.attempted = config.has_sync_work();
    if !report.attempted {
        return Ok(());
    }
    report.targets = config
        .targets
        .iter()
        .map(|target| TargetSyncReport {
            target: target.clone(),
            unavailable: false,
            errors: Vec::new(),
        })
        .collect();
    let mut blocked_targets = HashSet::new();
    for (index, target) in config.targets.iter().enumerate() {
        control.checkpoint()?;
        if let Target::Local { path } = target {
            let destination = match crate::paths::resolved(path) {
                Ok(path) => path,
                Err(error) => {
                    report.targets[index].unavailable = true;
                    report.target_error(index, format!("{}: {error}", target_label(target)));
                    blocked_targets.insert(index);
                    continue;
                }
            };
            for folder in &config.folders {
                control.checkpoint()?;
                let source = crate::paths::resolved(folder)?;
                if destination.starts_with(&source) || source.starts_with(&destination) {
                    report.target_error(index, format!("Quelle {} und Ziel {} überlappen sich. Bitte einen getrennten Sicherungsordner wählen.", folder.display(), path.display()));
                    blocked_targets.insert(index);
                }
            }
        }
    }
    let exclusions = crate::exclusions::Exclusions::new(&config.settings.exclude_patterns)?;
    let state = State::open(state_path)?;

    for removal in &config.settings.pending_folder_removals {
        for (index, target) in config.targets.iter().enumerate() {
            if blocked_targets.contains(&index) || !removal.targets.contains(target) {
                continue;
            }
            control.checkpoint()?;
            progress(crate::Progress::RemovingFolder {
                name: removal.namespace.clone(),
                target: target_label(target),
            });
            match remove_backup_folder(config, &state, removal, target, control) {
                Ok(count) => {
                    report.deleted += count;
                    report
                        .removed_folders
                        .push((removal.source.clone(), target.clone()));
                }
                Err(error) => {
                    control.check_cancelled()?;
                    report.target_error(
                        index,
                        format!(
                            "{} · Sicherungsordner {} entfernen: {error}",
                            target_label(target),
                            removal.namespace
                        ),
                    );
                }
            }
        }
    }

    let mut scanned: HashMap<String, (PathBuf, i64, i64)> = HashMap::new();
    let mut sources = HashMap::new();
    let mut complete_sources = HashSet::new();
    let mut namespaces = HashMap::new();
    for folder in &config.folders {
        control.checkpoint()?;
        let name = crate::recovery::namespace(config, folder)?;
        *namespaces.entry(name).or_insert(0) += 1;
    }
    for folder in &config.folders {
        control.checkpoint()?;
        let name = crate::recovery::namespace(config, folder)?;
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
        let prefix = PathBuf::from(name);
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
            control.checkpoint()?;
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

    'targets: for (index, target) in config.targets.iter().enumerate() {
        control.checkpoint()?;
        if blocked_targets.contains(&index) {
            continue;
        }
        let backend = match backend_for(target) {
            Ok(b) => b,
            Err(e) => {
                report.targets[index].unavailable = true;
                report.target_error(index, format!("{}: {}", target_label(target), e));
                continue;
            }
        };
        let prefix = target_prefix(target)?;
        let mut deletable_sources = complete_sources.clone();

        // Phase 1: classify every scanned file against the persisted state. The
        // destination listings and uploads are batched afterwards so they can run
        // with bounded parallelism instead of one request per file.
        let mut verify: Vec<FileTask> = Vec::new();
        let mut pending: Vec<FileTask> = Vec::new();
        let mut needed_parents: Vec<PathBuf> = Vec::new();
        let mut seen_parents = HashSet::new();
        for (checked, (key, (abs, size, mtime))) in scanned.iter().enumerate() {
            control.checkpoint()?;
            progress(crate::Progress::Checking {
                completed: checked,
                total: scanned.len(),
            });
            let skey = format!("{}::{}", prefix, key);
            let source = &sources[key];
            let owner = state.source(&skey)?;
            if owner.as_ref().is_some_and(|owner| *owner != *source) {
                report.target_error(
                    index,
                    format!(
                        "{}: Quelle {} kollidiert mit vorhandener Sicherung {}",
                        target_label(target),
                        source.display(),
                        key
                    ),
                );
                deletable_sources.remove(source);
                continue;
            }
            let (unchanged, refresh_mtime) = match state.get(&skey)? {
                Some(entry) if entry.size == *size && entry.mtime == *mtime && *mtime != 0 => {
                    (true, false)
                }
                Some(entry) if entry.size == *size => match file_hash(abs, control) {
                    Ok(hash) if hash == entry.hash => (true, true),
                    Ok(_) => (false, false),
                    Err(error) => {
                        control.check_cancelled()?;
                        deletable_sources.remove(source);
                        report.target_error(
                            index,
                            format!(
                                "{} · Quelle {}: {error}",
                                target_label(target),
                                abs.display()
                            ),
                        );
                        continue;
                    }
                },
                Some(_) | None => (false, false),
            };
            let task = FileTask {
                skey,
                key: key.clone(),
                abs: abs.clone(),
                size: *size,
                mtime: *mtime,
                source: source.clone(),
                owner,
                refresh_mtime,
            };
            if unchanged {
                let parent = Path::new(key)
                    .parent()
                    .expect("scanned files have a folder namespace")
                    .to_path_buf();
                if seen_parents.insert(parent.clone()) {
                    needed_parents.push(parent);
                }
                verify.push(task);
            } else {
                pending.push(task);
            }
        }

        // Phase 2: read the destination folders holding unchanged candidates in
        // parallel; one listing per folder, not one per file.
        let mut destination_files: HashMap<PathBuf, HashMap<PathBuf, Option<u64>>> = HashMap::new();
        let mut listing_failed = false;
        if !needed_parents.is_empty() {
            let queue = Mutex::new(VecDeque::from(needed_parents.clone()));
            let (tx, rx) = mpsc::channel();
            let workers = TRANSFER_WORKERS.min(needed_parents.len());
            std::thread::scope(|scope| -> Result<()> {
                for _ in 0..workers {
                    let queue = &queue;
                    let tx = &tx;
                    let backend = &*backend;
                    scope.spawn(move || {
                        while let Some(parent) = queue
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .pop_front()
                        {
                            if control.is_cancelled() {
                                break;
                            }
                            let listed = backend.list_files(&parent, control);
                            if tx.send((parent, listed)).is_err() {
                                break;
                            }
                        }
                    });
                }
                let mut remaining = needed_parents.len();
                while remaining > 0 {
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok((parent, Ok(files))) => {
                            destination_files.insert(
                                parent,
                                files
                                    .into_iter()
                                    .map(|file| (file.path, file.size))
                                    .collect(),
                            );
                            remaining -= 1;
                        }
                        Ok((parent, Err(error))) => {
                            control.check_cancelled()?;
                            report.target_error(
                                index,
                                format!(
                                    "{} · Sicherung {} prüfen: {error}",
                                    target_label(target),
                                    parent.display()
                                ),
                            );
                            listing_failed = true;
                            remaining -= 1;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            control.check_cancelled()?;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                Ok(())
            })?;
        }
        if listing_failed {
            continue 'targets;
        }

        // Phase 3: confirm the unchanged candidates against the listings; missing
        // copies join the upload queue.
        for task in verify {
            control.checkpoint()?;
            let relative = Path::new(&task.key);
            let parent = relative
                .parent()
                .expect("scanned files have a folder namespace");
            let Some(files) = destination_files.get(parent) else {
                continue;
            };
            if files
                .get(relative)
                .is_some_and(|length| length.is_none_or(|length| length == task.size as u64))
            {
                if task.refresh_mtime {
                    state.update_mtime(&task.skey, task.mtime)?;
                }
                if task.owner.as_ref() != Some(&task.source) {
                    state.set_source(&task.skey, &task.source)?;
                }
                report.skipped += 1;
            } else {
                pending.push(task);
            }
        }

        progress(crate::Progress::Checking {
            completed: scanned.len(),
            total: scanned.len(),
        });
        control.checkpoint()?;

        // Phase 4: upload the queue with a bounded worker pool.
        let mut ctx = TargetCtx {
            target,
            index,
            state: &state,
            control,
            progress: &mut progress,
            report,
            deletable_sources: &mut deletable_sources,
        };
        if upload_pending(&*backend, &mut ctx, &pending)? {
            continue 'targets;
        }
        if report.targets[index].errors.is_empty()
            && !config.folders.is_empty()
            && complete_sources.len() == config.folders.len()
        {
            if let Err(error) = crate::recovery::save_catalog(&*backend, config, control) {
                control.check_cancelled()?;
                report.target_error(
                    index,
                    format!(
                        "{} · Ordnerzuordnung speichern: {error}",
                        target_label(target)
                    ),
                );
            }
        }
        if config.settings.sync_mode == crate::SyncMode::Backup {
            continue;
        }
        let p = format!("{}::", prefix);
        for key in state.all_paths()? {
            control.checkpoint()?;
            if let Some(rel) = key.strip_prefix(&p) {
                let relative: PathBuf = Path::new(rel).components().skip(1).collect();
                if exclusions.matches(&relative) {
                    continue;
                }
                let source_complete = state
                    .source(&key)?
                    .is_some_and(|source| deletable_sources.contains(&source));
                if source_complete && !scanned.contains_key(rel) {
                    progress(crate::Progress::Deleting {
                        path: PathBuf::from(rel),
                        target: target_label(target),
                    });
                    match retry::run(control, || backend.delete(Path::new(rel))) {
                        Ok(()) => {
                            state.remove(&key)?;
                            report.deleted += 1;
                        }
                        Err(e) => {
                            control.check_cancelled()?;
                            report.target_error(
                                index,
                                format!("{} · löschen {rel}: {e}", target_label(target)),
                            );
                            if retry::is_temporary(&e) {
                                continue 'targets;
                            }
                        }
                    }
                }
            }
        }
    }

    control.check_cancelled()?;
    if report.errors.is_empty() && !config.folders.is_empty() {
        state.record_successful_sync(&config.targets)?;
    }
    Ok(())
}

fn remove_backup_folder(
    config: &Config,
    state: &State,
    removal: &FolderRemoval,
    target: &Target,
    control: &TransferControl,
) -> Result<usize> {
    crate::recovery::validate_namespace(&removal.namespace)?;
    for folder in &config.folders {
        if folder == &removal.source
            || crate::recovery::namespace(config, folder)? == removal.namespace
        {
            return Err(Error(
                "Sicherungsordner wird von einer aktiven Quelle verwendet".into(),
            ));
        }
    }
    let folder = Path::new(&removal.namespace);
    if let Target::Local { path } = target {
        let destination = crate::paths::resolved(&path.join(folder))?;
        let source = crate::paths::resolved(&removal.source)?;
        if destination.starts_with(&source) || source.starts_with(&destination) {
            return Err(Error(
                "Quelle und zu löschender Sicherungsordner überlappen sich".into(),
            ));
        }
    }
    let backend = crate::restore::existing_backend(target)?;
    let target_key = target_prefix(target)?;
    if let Some(files) = backend.list_folder(folder, control)? {
        for file in files {
            control.checkpoint()?;
            let key = format!("{target_key}::{}", file.path.display());
            let owned = file.path.starts_with(folder)
                && state.source(&key)?.as_ref() == Some(&removal.source)
                && state.get(&key)?.is_some_and(|record| {
                    file.size
                        .is_none_or(|size| u64::try_from(record.size) == Ok(size))
                });
            if !owned {
                return Err(Error(format!("{} ist dieser Quelle nicht zugeordnet. Der Sicherungsordner wurde beibehalten.", file.path.display())));
            }
        }
        control.checkpoint()?;
        backend.delete_folder(folder)?;
    }
    crate::recovery::forget_folder(&*backend, &removal.namespace, control)?;
    state.remove_folder_records(
        &format!("{target_key}::{}/", removal.namespace),
        &removal.source,
    )
}

enum UploadMsg {
    Started(usize),
    Skipped,
    Done(usize, std::io::Result<(u64, String)>),
}

/// Mutable per-target state shared by the upload pool's calling thread.
struct TargetCtx<'a> {
    target: &'a Target,
    index: usize,
    state: &'a State,
    control: &'a TransferControl,
    progress: &'a mut dyn FnMut(crate::Progress),
    report: &'a mut SyncReport,
    deletable_sources: &'a mut HashSet<PathBuf>,
}

/// Upload the queued files with a bounded worker pool. State updates, progress
/// and the report stay on the calling thread. Returns true when a temporary
/// error aborted the rest of the target.
fn upload_pending(
    backend: &dyn Backend,
    ctx: &mut TargetCtx,
    pending: &[FileTask],
) -> Result<bool> {
    if pending.is_empty() {
        return Ok(false);
    }
    let queue = Mutex::new(VecDeque::from_iter(0..pending.len()));
    let (tx, rx) = mpsc::channel();
    let stop = AtomicBool::new(false);
    let workers = TRANSFER_WORKERS.min(pending.len());
    let control = ctx.control;
    std::thread::scope(|scope| -> Result<bool> {
        for _ in 0..workers {
            let queue = &queue;
            let tx = &tx;
            let stop = &stop;
            scope.spawn(move || loop {
                let next = queue
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .pop_front();
                let Some(i) = next else { break };
                if stop.load(Ordering::Relaxed) || control.is_cancelled() {
                    let _ = tx.send(UploadMsg::Skipped);
                    continue;
                }
                let task = &pending[i];
                // Announce the file only once its transfer has actually begun, so
                // the progress label always names a file that is really uploading.
                let started = || {
                    let _ = tx.send(UploadMsg::Started(i));
                };
                let result = upload_one(
                    backend,
                    Path::new(&task.key),
                    &task.abs,
                    task.size,
                    task.mtime,
                    control,
                    &started,
                );
                let _ = tx.send(UploadMsg::Done(i, result));
            });
        }
        let mut outstanding = pending.len();
        let mut abort = false;
        while outstanding > 0 {
            match rx.recv() {
                Ok(UploadMsg::Skipped) => outstanding -= 1,
                Ok(UploadMsg::Started(i)) => {
                    (ctx.progress)(crate::Progress::Uploading {
                        path: PathBuf::from(&pending[i].key),
                        target: target_label(ctx.target),
                    });
                }
                Ok(UploadMsg::Done(i, result)) => {
                    outstanding -= 1;
                    let task = &pending[i];
                    match result {
                        Ok((written, hash)) => {
                            ctx.state.upsert(&task.skey, task.size, task.mtime, &hash)?;
                            if task.owner.as_ref() != Some(&task.source) {
                                ctx.state.set_source(&task.skey, &task.source)?;
                            }
                            ctx.report.uploaded += 1;
                            ctx.report.bytes += written;
                        }
                        Err(error) => {
                            control.check_cancelled()?;
                            ctx.deletable_sources.remove(&task.source);
                            ctx.report.target_error(
                                ctx.index,
                                format!(
                                    "{} · Quelle {} · Datei {}: {}",
                                    target_label(ctx.target),
                                    task.source.display(),
                                    task.abs.display(),
                                    error
                                ),
                            );
                            if retry::is_temporary(&error) {
                                stop.store(true, Ordering::Relaxed);
                                abort = true;
                            }
                        }
                    }
                }
                Err(mpsc::RecvError) => break,
            }
        }
        Ok(abort)
    })
}

struct HashingReader<R> {
    inner: R,
    hasher: Arc<Mutex<blake3::Hasher>>,
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.hasher
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .update(&buffer[..read]);
        Ok(read)
    }
}

fn upload_one(
    backend: &dyn Backend,
    rel: &Path,
    abs: &Path,
    size: i64,
    mtime: i64,
    control: &TransferControl,
    started: &dyn Fn(),
) -> std::io::Result<(u64, String)> {
    retry::run(control, || backend.mkdirs(rel))?;
    // Hash while uploading: the source file is read once, not twice.
    let hasher = Arc::new(Mutex::new(blake3::Hasher::new()));
    let hash_reader = hasher.clone();
    let written = retry::run(control, || {
        hash_reader
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .reset();
        let file = fs::File::open(abs)?;
        let metadata = file.metadata()?;
        if metadata.len() != size as u64 || (mtime != 0 && mtime_stamp(&metadata) != mtime) {
            return Err(std::io::Error::other(
                "Quelldatei wurde während der Sicherung geändert",
            ));
        }
        let _transfer = control.begin_file(rel, Some(size as u64));
        started();
        let body: Box<dyn Read + Send> = Box::new(HashingReader {
            inner: file,
            hasher: hash_reader.clone(),
        });
        backend.put_controlled(rel, body, size as u64, control)
    })?;
    let hash = hasher
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .finalize()
        .to_hex()
        .to_string();
    Ok((written, hash))
}

fn file_hash(path: &Path, control: &TransferControl) -> std::io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let file = fs::File::open(path)?;
    std::io::copy(&mut control.reader(file, false), &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}
