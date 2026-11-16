use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::Result;

fn secret_path(url: &str, user: &str) -> PathBuf {
    let dir = Config::default_path()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let key = blake3::hash(format!("{}|{}", url, user).as_bytes()).to_hex();
    dir.join(format!("webdav-{}.secret", &key[..16]))
}

pub fn save_password(url: &str, user: &str, password: &str) -> Result<()> {
    let path = secret_path(url, user);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        f.write_all(password.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        fs::write(&path, password.as_bytes())?;
    }
    Ok(())
}

pub fn load_password(url: &str, user: &str) -> Result<Option<String>> {
    match fs::read(secret_path(url, user)) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).trim().to_string())),
        Err(_) => Ok(std::env::var("CLOUDSAVE_WEBDAV_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty())),
    }
}
