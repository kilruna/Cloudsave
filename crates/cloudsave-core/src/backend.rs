use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::config::Target;

pub trait Backend {
    fn mkdirs(&self, rel: &Path) -> io::Result<()>;
    fn put(&self, rel: &Path, data: &mut dyn Read) -> io::Result<u64>;
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
    fn mkdirs(&self, rel: &Path) -> io::Result<()> {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    fn put(&self, rel: &Path, data: &mut dyn Read) -> io::Result<u64> {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::File::create(&p)?;
        io::copy(data, &mut file)
    }

    fn delete(&self, rel: &Path) -> io::Result<()> {
        let p = self.root.join(rel);
        if p.exists() {
            fs::remove_file(&p)?;
        }
        Ok(())
    }
}

pub fn backend_for(target: &Target) -> io::Result<Box<dyn Backend>> {
    match target {
        Target::Local { path } => Ok(Box::new(LocalBackend::new(path.clone())?)),
    }
}
