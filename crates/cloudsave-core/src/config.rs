use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "ConfigCompat")]
pub struct Config {
    pub folders: Vec<PathBuf>,
    pub targets: Vec<Target>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Deserialize)]
struct ConfigCompat {
    folders: Vec<PathBuf>,
    #[serde(default)]
    target: Option<Target>,
    #[serde(default)]
    targets: Vec<Target>,
    #[serde(default = "legacy_settings")]
    settings: Settings,
}

fn legacy_settings() -> Settings {
    Settings {
        sync_mode: SyncMode::Mirror,
        disabled_folders: None,
        ..Settings::default()
    }
}

impl From<ConfigCompat> for Config {
    fn from(c: ConfigCompat) -> Self {
        let mut targets = c.targets;
        if let Some(t) = c.target {
            if !targets.contains(&t) {
                targets.insert(0, t);
            }
        }
        Self {
            folders: c.folders,
            targets,
            settings: c.settings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Target {
    Local { path: PathBuf },
    Webdav { url: String, user: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FolderRemoval {
    pub source: PathBuf,
    pub namespace: String,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Missing in older profiles that used the selected folders as their entire list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_folders: Option<Vec<PathBuf>>,
    /// Keep remote folder names stable when restoring onto different local paths.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folder_names: BTreeMap<PathBuf, String>,
    /// Portable paths retained when a portal grants an alternative access path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folder_home_paths: BTreeMap<PathBuf, PathBuf>,
    /// A restart must return to recovery before any automatic upload can run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_recovery: Option<Target>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_folder_removals: Vec<FolderRemoval>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default = "legacy_sync_mode")]
    pub sync_mode: SyncMode,
    #[serde(default = "default_true")]
    pub auto_sync: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_true")]
    pub run_in_background: bool,
    #[serde(default = "default_interval")]
    pub sync_interval_minutes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    Backup,
    Mirror,
}

// Existing profiles keep their documented behavior. New setups choose the safe backup default.
fn legacy_sync_mode() -> SyncMode {
    SyncMode::Mirror
}

fn default_true() -> bool {
    true
}

fn default_interval() -> u64 {
    5
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            disabled_folders: Some(Vec::new()),
            folder_names: BTreeMap::new(),
            folder_home_paths: BTreeMap::new(),
            pending_recovery: None,
            pending_folder_removals: Vec::new(),
            exclude_patterns: Vec::new(),
            sync_mode: SyncMode::Backup,
            auto_sync: default_true(),
            autostart: false,
            run_in_background: default_true(),
            sync_interval_minutes: default_interval(),
        }
    }
}

impl Config {
    pub fn has_sync_work(&self) -> bool {
        !self.folders.is_empty()
            || self.settings.pending_folder_removals.iter().any(|removal| {
                removal
                    .targets
                    .iter()
                    .any(|target| self.targets.contains(target))
            })
    }

    pub fn remove_target(&mut self, target: &Target) {
        self.targets.retain(|entry| entry != target);
        for removal in &mut self.settings.pending_folder_removals {
            removal.targets.retain(|entry| entry != target);
        }
        self.settings
            .pending_folder_removals
            .retain(|removal| !removal.targets.is_empty());
    }

    /// Expose previously unchecked folders from this profile's saved ownership records.
    pub fn initialize_disabled_folders(&mut self, state_path: &Path) -> Result<bool> {
        if self.settings.disabled_folders.is_some() {
            return Ok(false);
        }
        let mut disabled = BTreeSet::new();
        if state_path.is_file() {
            let state = crate::State::open(state_path)?;
            for target in &self.targets {
                disabled.extend(state.sources_for_target(&crate::engine::target_prefix(target)?)?);
            }
        }
        disabled.retain(|path| {
            !self.folders.contains(path)
                && !self
                    .settings
                    .pending_folder_removals
                    .iter()
                    .any(|removal| removal.source == *path)
        });
        self.settings.disabled_folders = Some(disabled.into_iter().collect());
        Ok(true)
    }

    pub fn set_folder_enabled(&mut self, source: &Path, enabled: bool) {
        let disabled = self.settings.disabled_folders.get_or_insert_default();
        if enabled {
            disabled.retain(|folder| folder != source);
            if !self.folders.iter().any(|folder| folder == source) {
                self.folders.push(source.to_path_buf());
            }
        } else {
            self.folders.retain(|folder| folder != source);
            if !disabled.iter().any(|folder| folder == source) {
                disabled.push(source.to_path_buf());
            }
        }
    }

    /// Capture the current targets so later endpoint additions cannot inherit a removal.
    pub fn remove_folder(&mut self, source: &Path) -> Result<()> {
        if self.settings.auto_sync && !self.targets.is_empty() {
            let namespace = crate::recovery::namespace(self, source)?;
            if self
                .settings
                .pending_folder_removals
                .iter()
                .all(|item| item.source != source)
            {
                self.settings.pending_folder_removals.push(FolderRemoval {
                    source: source.to_path_buf(),
                    namespace,
                    targets: self.targets.clone(),
                });
            }
        }
        self.folders.retain(|folder| folder != source);
        if let Some(disabled) = &mut self.settings.disabled_folders {
            disabled.retain(|folder| folder != source);
        }
        Ok(())
    }

    pub fn folder_removal_pending(&self, source: &Path) -> Result<bool> {
        let namespace = crate::recovery::namespace(self, source)?;
        Ok(self
            .settings
            .pending_folder_removals
            .iter()
            .any(|item| item.namespace == namespace))
    }

    pub fn finish_folder_removals(&mut self, completed: &[(PathBuf, Target)]) {
        for removal in &mut self.settings.pending_folder_removals {
            removal.targets.retain(|target| {
                !completed
                    .iter()
                    .any(|(source, done)| source == &removal.source && done == target)
            });
        }
        self.settings
            .pending_folder_removals
            .retain(|item| !item.targets.is_empty());
    }

    pub fn load(path: &Path) -> Result<Config> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| Error(format!("Config nicht lesbar ({}): {}", path.display(), e)))?;
        let config: Config = toml::from_str(&data).map_err(Error::from)?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = toml::to_string_pretty(self).map_err(Error::from)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn default_path() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("cloudsave/config.toml");
            }
        }
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config/cloudsave/config.toml"))
            .unwrap_or_else(|_| PathBuf::from("config.toml"))
    }

    pub fn default_state_path() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join("cloudsave/state.db");
            }
        }
        std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".local/state/cloudsave/state.db"))
            .unwrap_or_else(|_| PathBuf::from("state.db"))
    }
}
