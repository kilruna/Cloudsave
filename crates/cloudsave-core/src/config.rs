use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Keep remote folder names stable when restoring onto different local paths.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folder_names: BTreeMap<PathBuf, String>,
    /// Portable paths retained when a portal grants an alternative access path.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub folder_home_paths: BTreeMap<PathBuf, PathBuf>,
    /// A restart must return to recovery before any automatic upload can run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_recovery: Option<Target>,
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
            folder_names: BTreeMap::new(),
            folder_home_paths: BTreeMap::new(),
            pending_recovery: None,
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
