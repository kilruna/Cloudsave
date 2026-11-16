use std::fs;
use std::path::PathBuf;

use crate::{Error, Result};

const DESKTOP_ENTRY: &str = "[Desktop Entry]\nType=Application\nName=Cloudsave\nExec=flatpak run org.cloudsave.Cloudsave --background\nIcon=org.cloudsave.Cloudsave\nX-Flatpak=org.cloudsave.Cloudsave\n";

pub fn autostart_file() -> Result<PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .ok()
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .ok_or_else(|| Error("Kein HOME/XDG_CONFIG_HOME gefunden".to_string()))?;
    Ok(base.join("autostart/org.cloudsave.Cloudsave.desktop"))
}

pub fn autostart_enabled() -> bool {
    autostart_file().map(|p| p.exists()).unwrap_or(false)
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    let file = autostart_file()?;
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    if enabled {
        fs::write(&file, DESKTOP_ENTRY)?;
    } else if file.exists() {
        fs::remove_file(&file)?;
    }
    Ok(())
}
