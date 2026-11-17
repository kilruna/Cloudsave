mod autostart;
mod backend;
mod config;
mod engine;
mod exclusions;
mod paths;
mod progress;
mod recovery;
mod restore;
mod retry;
mod secrets;
mod state;
mod watch;
mod webdav;

pub use autostart::{autostart_enabled, set_autostart};
pub use backend::{backend_for, Backend, LocalBackend};
pub use config::{Config, Settings, SyncMode, Target};
pub use engine::{sync, sync_controlled, sync_with_progress, target_label, SyncReport};
pub use exclusions::validate_exclusion_patterns;
pub use progress::{Progress, RetryProgress, TransferControl, TransferProgress, TransferStatus};
pub use recovery::{
    discover_backup, prepare_restore_location, restore_mapped, restore_mapped_controlled,
    BackupFolder, BackupInventory, RestoreLocation,
};
pub use restore::{
    adopt_restored, list_backups, restore_controlled, restore_files, restore_with_progress,
    BackupFile, RestoreReport,
};
pub use secrets::{load_password, save_password};
pub use state::{FinishedJob, JobHistory, State};
pub use watch::{ChangeWatcher, WatchConfig, WatchEvent};
pub use webdav::{normalize_webdav_url, validate_webdav_url, WebdavBackend};

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
