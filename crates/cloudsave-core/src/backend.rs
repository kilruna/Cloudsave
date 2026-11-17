use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::Target;
use crate::webdav::WebdavBackend;
use crate::{BackupFile, TransferControl};

pub trait Backend {
    fn list(&self) -> io::Result<Vec<BackupFile>>;
    fn download(&self, rel: &Path, writer: &mut dyn Write) -> io::Result<u64>;
    fn test_connection(&self) -> io::Result<()>;
    fn mkdirs(&self, rel: &Path) -> io::Result<()>;
    fn put(&self, rel: &Path, file: File, size: u64) -> io::Result<u64>;
    fn put_controlled(
        &self,
        rel: &Path,
        file: File,
        size: u64,
        control: &TransferControl,
    ) -> io::Result<u64> {
        control.check_cancelled()?;
        self.put(rel, file, size)
    }
    fn delete(&self, rel: &Path) -> io::Result<()>;
}

pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    pub fn new(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

impl Backend for LocalBackend {
    fn list(&self) -> io::Result<Vec<BackupFile>> {
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&self.root).follow_links(false) {
            let entry = entry.map_err(io::Error::other)?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&self.root)
                .map_err(io::Error::other)?
                .to_path_buf();
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".cloudsave-upload-")
                && entry.file_name().to_string_lossy().ends_with(".part")
            {
                continue;
            }
            files.push(BackupFile {
                path: relative,
                size: Some(entry.metadata().map_err(io::Error::other)?.len()),
            });
        }
        Ok(files)
    }
    fn download(&self, rel: &Path, writer: &mut dyn Write) -> io::Result<u64> {
        let path = crate::restore::safe_path(&self.root, rel, false)?;
        io::copy(&mut File::open(path)?, writer)
    }

    fn test_connection(&self) -> io::Result<()> {
        // NamedTempFile exclusively creates a random path and cleans up on errors too.
        let mut tmp = tempfile::Builder::new()
            .prefix(".cloudsave-test-")
            .tempfile_in(&self.root)?;
        tmp.write_all(b"ok")?;
        tmp.as_file().sync_all()?;
        tmp.close()?;
        Ok(())
    }

    fn mkdirs(&self, rel: &Path) -> io::Result<()> {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    fn put(&self, rel: &Path, file: File, size: u64) -> io::Result<u64> {
        self.put_controlled(rel, file, size, &TransferControl::default())
    }

    fn put_controlled(
        &self,
        rel: &Path,
        file: File,
        size: u64,
        control: &TransferControl,
    ) -> io::Result<u64> {
        control.check_cancelled()?;
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        let parent = p
            .parent()
            .ok_or_else(|| io::Error::other("Ziel hat keinen Elternordner"))?;
        let mut out = tempfile::Builder::new()
            .prefix(".cloudsave-upload-")
            .suffix(".part")
            .tempfile_in(parent)?;
        let written = io::copy(&mut control.reader(file, true), &mut out)?;
        if written != size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Quelldatei hat sich während des Uploads verändert",
            ));
        }
        out.as_file().sync_all()?;
        control.check_cancelled()?;
        out.persist(&p).map_err(|error| error.error)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(written)
    }

    fn delete(&self, rel: &Path) -> io::Result<()> {
        let p = self.root.join(rel);
        if p.exists() {
            fs::remove_file(&p)?;
        }
        Ok(())
    }
}

pub fn backend_for(target: &Target) -> crate::Result<Box<dyn Backend>> {
    match target {
        Target::Local { path } => Ok(Box::new(LocalBackend::new(path.clone())?)),
        Target::Webdav { url, user } => {
            let password = crate::secrets::load_password(url, user)?.ok_or_else(|| crate::Error("Kein Passwort im Schlüsselbund gefunden. Zugangsdaten bitte erneut einrichten.".into()))?;
            Ok(Box::new(WebdavBackend::new(url, user, &password)?))
        }
    }
}
