use crate::{backend_for, Error, Result, Target};
use std::fs;
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
    pub errors: Vec<String>,
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

fn existing_backend(target: &Target) -> Result<Box<dyn crate::Backend>> {
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
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files.dedup_by(|left, right| left.path == right.path);
    Ok(files)
}

pub fn restore_files(
    target: &Target,
    files: &[BackupFile],
    destination: &Path,
) -> Result<RestoreReport> {
    // Validate every remote-controlled name before creating any destination directory.
    for file in files {
        validate_relative(&file.path)?;
    }
    if !destination.is_dir() {
        return Err(Error("Wiederherstellungsordner nicht gefunden".into()));
    }
    let backend = existing_backend(target)?;
    let mut report = RestoreReport::default();
    for file in files {
        let result = (|| -> std::io::Result<u64> {
            let output = safe_path(destination, &file.path, true)?;
            if output.try_exists()? {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "Datei existiert bereits; wird nicht überschrieben",
                ));
            }
            let parent = output
                .parent()
                .ok_or_else(|| std::io::Error::other("Kein Zielordner"))?;
            let mut temporary = tempfile::Builder::new()
                .prefix(".cloudsave-restore-")
                .tempfile_in(parent)?;
            let written = backend.download(&file.path, temporary.as_file_mut())?;
            if file.size.is_some_and(|size| size != written) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "Sicherungsdatei hat sich während des Downloads verändert",
                ));
            }
            temporary.as_file().sync_all()?;
            temporary
                .persist_noclobber(&output)
                .map_err(|error| error.error)?;
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(written)
        })();
        match result {
            Ok(bytes) => {
                report.restored += 1;
                report.bytes += bytes;
            }
            Err(error) => report
                .errors
                .push(format!("{}: {error}", file.path.display())),
        }
    }
    Ok(report)
}
