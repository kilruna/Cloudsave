use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::config::Target;
use crate::webdav::WebdavBackend;

pub trait Backend {
    fn test_connection(&self) -> io::Result<()>;
    fn mkdirs(&self, rel: &Path) -> io::Result<()>;
    fn put(&self, rel: &Path, file: File, size: u64) -> io::Result<u64>;
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

    fn put(&self, rel: &Path, mut file: File, _size: u64) -> io::Result<u64> {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&p)?;
        io::copy(&mut file, &mut out)
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
