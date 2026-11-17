use crate::{backend_for, Error, Result, Target};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupFile {
    pub path: PathBuf,
    pub size: Option<u64>,
}
#[derive(Debug, Default)]
pub struct RestoreReport {
    pub restored: usize,
    pub bytes: u64,
    pub skipped: usize,
    pub errors: Vec<String>,
    pub(crate) copies: Vec<RestoredCopy>,
}

#[derive(Debug)]
pub(crate) struct RestoredCopy {
    path: PathBuf,
    size: u64,
    mtime: i64,
    hash: String,
}

struct DigestWriter<'a> {
    file: &'a mut fs::File,
    hash: blake3::Hasher,
}
impl Write for DigestWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let written = self.file.write(bytes)?;
        self.hash.update(&bytes[..written]);
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

/// Adopt only successfully downloaded files. A following sync can skip their
/// unchanged contents using the hash and timestamp captured before publication.
pub fn adopt_restored(
    target: &Target,
    report: &RestoreReport,
    locations: &[crate::RestoreLocation],
    state_path: &Path,
) -> Result<()> {
    let state = crate::State::open(state_path)?;
    let prefix = crate::engine::target_prefix(target)?;
    for copy in &report.copies {
        validate_relative(&copy.path)?;
        let mut parts = copy.path.components();
        let namespace = parts.next().unwrap().as_os_str();
        let relative: PathBuf = parts.collect();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let Some(location) = locations
            .iter()
            .find(|location| namespace == location.namespace.as_str())
        else {
            continue;
        };
        let output = safe_path(&location.destination, &relative, false)?;
        let metadata = fs::metadata(output)?;
        // Edits after a download must still be picked up by the next sync.
        if metadata.len() != copy.size || crate::engine::mtime_stamp(&metadata) != copy.mtime {
            continue;
        }
        let key = format!("{prefix}::{}", copy.path.to_string_lossy());
        state.upsert(&key, copy.size as i64, copy.mtime, &copy.hash)?;
        state.set_source(&key, &location.destination)?;
    }
    Ok(())
}

pub(crate) fn validate_relative(path: &Path) -> std::io::Result<()> {
    if path.as_os_str().is_empty()
        || path
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .any(|byte| *byte == b'\\' || *byte == 0)
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Unsicherer relativer Sicherungspfad",
        ));
    }
    Ok(())
}

pub(crate) fn safe_path(
    root: &Path,
    relative: &Path,
    create_parents: bool,
) -> std::io::Result<PathBuf> {
    validate_relative(relative)?;
    let mut path = fs::canonicalize(root)?;
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        path.push(component.as_os_str());
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Symlinks im Wiederherstellungspfad sind nicht erlaubt",
                ))
            }
            Ok(meta) if index + 1 < components.len() && !meta.is_dir() => {
                return Err(std::io::Error::other("Elternpfad ist kein Ordner"))
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && create_parents
                    && index + 1 < components.len() =>
            {
                fs::create_dir(&path)?
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && index + 1 == components.len() => {}
            Err(error) => return Err(error),
        }
    }
    Ok(path)
}

pub(crate) fn existing_backend(target: &Target) -> Result<Box<dyn crate::Backend>> {
    if let Target::Local { path } = target {
        if !path.is_dir() {
            return Err(Error(format!(
                "Sicherungsziel nicht gefunden: {}",
                path.display()
            )));
        }
    }
    backend_for(target)
}

pub fn list_backups(target: &Target) -> Result<Vec<BackupFile>> {
    let mut files = existing_backend(target)?.list()?;
    for file in &files {
        validate_relative(&file.path)?;
    }
    files.retain(|file| file.path != Path::new(crate::recovery::CATALOG));
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files.dedup_by(|left, right| left.path == right.path);
    Ok(files)
}

pub fn restore_files(
    target: &Target,
    files: &[BackupFile],
    destination: &Path,
) -> Result<RestoreReport> {
    restore_with_progress(target, files, destination, |_| {})
}

pub fn restore_with_progress(
    target: &Target,
    files: &[BackupFile],
    destination: &Path,
    mut progress: impl FnMut(crate::Progress),
) -> Result<RestoreReport> {
    // Validate every remote-controlled name before creating any destination directory.
    for file in files {
        validate_relative(&file.path)?;
    }
    if !destination.is_dir() {
        return Err(Error("Wiederherstellungsordner nicht gefunden".into()));
    }
    let backend = existing_backend(target)?;
    let entries: Vec<_> = files
        .iter()
        .map(|file| (file.clone(), destination.to_path_buf(), file.path.clone()))
        .collect();
    restore_entries(&*backend, &entries, false, &mut progress)
}

pub(crate) fn restore_entries(
    backend: &dyn crate::Backend,
    entries: &[(BackupFile, PathBuf, PathBuf)],
    keep_existing: bool,
    progress: &mut impl FnMut(crate::Progress),
) -> Result<RestoreReport> {
    let mut report = RestoreReport::default();
    for (index, (file, destination, relative)) in entries.iter().enumerate() {
        progress(crate::Progress::Checking {
            completed: index,
            total: entries.len(),
        });
        let result = (|| -> std::io::Result<Option<RestoredCopy>> {
            let output = safe_path(destination, relative, true)?;
            if output.try_exists()? {
                if keep_existing && output.is_file() {
                    return Ok(None);
                }
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "Datei existiert bereits; wird nicht überschrieben",
                ));
            }
            progress(crate::Progress::Downloading {
                path: file.path.clone(),
            });
            let parent = output
                .parent()
                .ok_or_else(|| std::io::Error::other("Kein Zielordner"))?;
            let mut temporary = tempfile::Builder::new()
                .prefix(".cloudsave-restore-")
                .tempfile_in(parent)?;
            let (written, hash) = {
                let mut writer = DigestWriter {
                    file: temporary.as_file_mut(),
                    hash: blake3::Hasher::new(),
                };
                let written = backend.download(&file.path, &mut writer)?;
                (written, writer.hash.finalize().to_hex().to_string())
            };
            if file.size.is_some_and(|size| size != written) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Sicherungsdatei hat sich während des Downloads verändert",
                ));
            }
            temporary.as_file().sync_all()?;
            let metadata = temporary.as_file().metadata()?;
            if metadata.len() != written {
                return Err(std::io::Error::other("Unvollständiger Download"));
            }
            let mtime = crate::engine::mtime_stamp(&metadata);
            temporary
                .persist_noclobber(&output)
                .map_err(|error| error.error)?;
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(Some(RestoredCopy {
                path: file.path.clone(),
                size: written,
                mtime,
                hash,
            }))
        })();
        match result {
            Ok(Some(copy)) => {
                report.restored += 1;
                report.bytes += copy.size;
                report.copies.push(copy);
            }
            Ok(None) => report.skipped += 1,
            Err(error) => report
                .errors
                .push(format!("{}: {error}", file.path.display())),
        }
    }
    progress(crate::Progress::Checking {
        completed: entries.len(),
        total: entries.len(),
    });
    Ok(report)
}
