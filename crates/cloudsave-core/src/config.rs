use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub folders: Vec<PathBuf>,
    pub target: Target,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Target {
    Local { path: PathBuf },
    Webdav { url: String, user: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_true")]
    pub auto_sync: bool,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default = "default_true")]
    pub run_in_background: bool,
    #[serde(default = "default_interval")]
    pub sync_interval_minutes: u64,
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
