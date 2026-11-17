use crate::{Backend, BackupFile, Config, Error, Progress, RestoreReport, Result, Target};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) const CATALOG: &str = ".cloudsave/folders.toml";
const LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupFolder {
    /// Stable remote prefix. Empty denotes legacy files in the backup root.
    pub namespace: String,
    /// Portable path only; never store an old user's absolute home path.
    pub home_relative: Option<PathBuf>,
}
#[derive(Debug, Clone)]
pub struct BackupInventory {
    pub files: Vec<BackupFile>,
    pub folders: Vec<BackupFolder>,
    pub has_catalog: bool,
}
#[derive(Debug, Clone)]
pub struct RestoreLocation {
    pub namespace: String,
    pub destination: PathBuf,
}
#[derive(Debug, Serialize, Deserialize)]
struct Catalog {
    version: u32,
    folders: Vec<BackupFolder>,
}

pub(crate) fn namespace(config: &Config, folder: &Path) -> Result<String> {
    let name = config
        .settings
        .folder_names
        .get(folder)
        .cloned()
        .unwrap_or_else(|| {
            folder
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("root"))
                .to_string_lossy()
                .into_owned()
        });
    validate_namespace(&name)?;
    Ok(name)
}
fn validate_namespace(name: &str) -> Result<()> {
    crate::restore::validate_relative(Path::new(name))?;
    if Path::new(name).components().count() != 1 || name.contains('/') || name == ".cloudsave" {
        return Err(Error(
            "Ungültiger oder reservierter Sicherungsordnername".into(),
        ));
    }
    Ok(())
}
struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > LIMIT.saturating_sub(self.0.len()) {
            return Err(io::Error::other("Ordnerzuordnung ist zu groß"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn read_catalog(backend: &dyn Backend) -> Result<Option<Catalog>> {
    let mut data = Limited(Vec::new());
    match backend.download(Path::new(CATALOG), &mut data) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let text = std::str::from_utf8(&data.0).map_err(|error| Error(error.to_string()))?;
    let catalog: Catalog = toml::from_str(text)?;
    if catalog.version != 1 || catalog.folders.len() > 4096 {
        return Err(Error(
            "Unbekannte oder zu große Ordnerzuordnung in der Sicherung".into(),
        ));
    }
    let mut names = HashSet::new();
    for folder in &catalog.folders {
        validate_namespace(&folder.namespace)?;
        if !names.insert(&folder.namespace) {
            return Err(Error("Doppelte Ordnerzuordnung".into()));
        }
        if let Some(relative) = &folder.home_relative {
            crate::restore::validate_relative(relative)?;
        }
    }
    Ok(Some(catalog))
}

pub(crate) fn save_catalog(backend: &dyn Backend, config: &Config) -> Result<()> {
    let old = read_catalog(backend)?;
    let mut folders: BTreeMap<_, _> = old
        .as_ref()
        .map(|c| c.folders.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|folder| (folder.namespace.clone(), folder))
        .collect();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    for folder in &config.folders {
        let namespace = namespace(config, folder)?;
        let home_relative = config
            .settings
            .folder_home_paths
            .get(folder)
            .cloned()
            .or_else(|| {
                home.as_ref()
                    .and_then(|home| folder.strip_prefix(home).ok())
                    .filter(|relative| !relative.as_os_str().is_empty())
                    .map(Path::to_path_buf)
            });
        if let Some(relative) = &home_relative {
            crate::restore::validate_relative(relative)?;
        }
        folders.insert(
            namespace.clone(),
            BackupFolder {
                namespace,
                home_relative,
            },
        );
    }
    let catalog = Catalog {
        version: 1,
        folders: folders.into_values().collect(),
    };
    let text = toml::to_string(&catalog)?;
    if text.len() > LIMIT {
        return Err(Error("Ordnerzuordnung ist zu groß".into()));
    }
    if old
        .as_ref()
        .is_some_and(|old| old.folders == catalog.folders)
    {
        return Ok(());
    }
    let mut file = tempfile::tempfile()?;
    file.write_all(text.as_bytes())?;
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    backend.mkdirs(Path::new(CATALOG))?;
    backend.put(Path::new(CATALOG), file, text.len() as u64)?;
    Ok(())
}

pub fn discover_backup(target: &Target) -> Result<BackupInventory> {
    let files = crate::list_backups(target)?;
    let backend = crate::restore::existing_backend(target)?;
    let catalog = read_catalog(&*backend)?;
    let mut folders: BTreeMap<String, BackupFolder> = catalog
        .as_ref()
        .map(|c| c.folders.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|folder| (folder.namespace.clone(), folder))
        .collect();
    for file in &files {
        let namespace = if file.path.components().count() == 1 {
            String::new()
        } else {
            file.path
                .components()
                .next()
                .unwrap()
                .as_os_str()
                .to_string_lossy()
                .into_owned()
        };
        folders.entry(namespace.clone()).or_insert(BackupFolder {
            namespace,
            home_relative: None,
        });
    }
    Ok(BackupInventory {
        files,
        folders: folders.into_values().collect(),
        has_catalog: catalog.is_some(),
    })
}

/// Restore whole folders through explicitly approved destination mappings. Existing
/// local files are retained, counted separately, and never turn into remote deletes.
pub fn restore_mapped(
    target: &Target,
    files: &[BackupFile],
    locations: &[RestoreLocation],
    mut progress: impl FnMut(Progress),
) -> Result<RestoreReport> {
    let mut mappings = BTreeMap::new();
    let mut destinations = Vec::<PathBuf>::new();
    for location in locations {
        if !location.namespace.is_empty() {
            validate_namespace(&location.namespace)?;
        }
        if mappings
            .insert(location.namespace.clone(), location.destination.clone())
            .is_some()
        {
            return Err(Error("Sicherungsordner mehrfach zugeordnet".into()));
        }
        if !location.destination.is_dir() {
            return Err(Error("Wiederherstellungsordner nicht gefunden".into()));
        }
        let destination = crate::paths::resolved(&location.destination)?;
        if destinations
            .iter()
            .any(|other| destination.starts_with(other) || other.starts_with(&destination))
        {
            return Err(Error("Die Zielordner der Wiederherstellung dürfen sich nicht überlappen. Bitte getrennte Ordner wählen.".into()));
        }
        destinations.push(destination);
        if let Target::Local { path } = target {
            let backup = crate::paths::resolved(path)?;
            let output = crate::paths::resolved(&location.destination)?;
            if backup.starts_with(&output) || output.starts_with(&backup) {
                return Err(Error(
                    "Wiederherstellungsordner und Sicherungsziel dürfen sich nicht überlappen"
                        .into(),
                ));
            }
        }
    }
    let mut entries = Vec::new();
    let mut outputs = HashSet::new();
    for file in files {
        crate::restore::validate_relative(&file.path)?;
        let mut parts = file.path.components();
        let first = parts.next().unwrap().as_os_str().to_string_lossy();
        let relative: PathBuf = parts.collect();
        let (namespace, relative) = if relative.as_os_str().is_empty() {
            ("", file.path.clone())
        } else {
            (first.as_ref(), relative)
        };
        let Some(root) = mappings.get(namespace) else {
            continue;
        };
        let output = crate::paths::resolved(&root.join(&relative))?;
        if !outputs.insert(output) {
            return Err(Error(
                "Mehrere Sicherungsdateien würden denselben Zielpfad verwenden".into(),
            ));
        }
        entries.push((file.clone(), root.clone(), relative));
    }
    let backend = crate::restore::existing_backend(target)?;
    crate::restore::restore_entries(&*backend, &entries, true, &mut progress)
}

/// Create only a chosen relative folder below a user-approved root, refusing
/// symlink components and backup/destination overlap before creating anything.
pub fn prepare_restore_location(target: &Target, base: &Path, relative: &Path) -> Result<PathBuf> {
    if !relative.as_os_str().is_empty() {
        crate::restore::validate_relative(relative)?;
    }
    let destination = base.join(relative);
    if let Target::Local { path } = target {
        let backup = crate::paths::resolved(path)?;
        let output = crate::paths::resolved(&destination)?;
        if backup.starts_with(&output) || output.starts_with(&backup) {
            return Err(Error(
                "Wiederherstellungsordner und Sicherungsziel dürfen sich nicht überlappen".into(),
            ));
        }
    }
    crate::restore::safe_path(base, &relative.join(".cloudsave-directory-check"), true)?;
    Ok(destination)
}
