use crate::{Config, Error, Result};
use secret_service::{blocking::SecretService, EncryptionType};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn secret_path(url: &str, user: &str) -> PathBuf {
    let dir = Config::default_path()
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let key = blake3::hash(format!("{}|{}", url, user).as_bytes()).to_hex();
    dir.join(format!("webdav-{}.secret", &key[..16]))
}

trait CredentialStore {
    fn get(&self, url: &str, user: &str) -> Result<Option<Vec<u8>>>;
    fn set(&self, url: &str, user: &str, password: &[u8]) -> Result<()>;
}
struct SystemKeyring;
fn attributes<'a>(url: &'a str, user: &'a str) -> HashMap<&'a str, &'a str> {
    HashMap::from([
        ("application", "org.cloudsave.Cloudsave"),
        ("url", url),
        ("user", user),
    ])
}
fn keyring_error(error: secret_service::Error) -> Error {
    Error(format!(
        "System-Schlüsselbund nicht verfügbar oder nicht entsperrt: {error}"
    ))
}
impl CredentialStore for SystemKeyring {
    fn get(&self, url: &str, user: &str) -> Result<Option<Vec<u8>>> {
        let service = SecretService::connect(EncryptionType::Dh).map_err(keyring_error)?;
        let items = service
            .search_items(attributes(url, user))
            .map_err(keyring_error)?;
        if let Some(item) = items.unlocked.first().or(items.locked.first()) {
            item.unlock().map_err(keyring_error)?;
            return item.get_secret().map(Some).map_err(keyring_error);
        }
        Ok(None)
    }
    fn set(&self, url: &str, user: &str, password: &[u8]) -> Result<()> {
        let service = SecretService::connect(EncryptionType::Dh).map_err(keyring_error)?;
        let collection = service.get_default_collection().map_err(keyring_error)?;
        collection.unlock().map_err(keyring_error)?;
        collection
            .create_item(
                "Cloudsave WebDAV",
                attributes(url, user),
                password,
                true,
                "text/plain",
            )
            .map_err(keyring_error)?;
        Ok(())
    }
}
fn remove_legacy(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error(format!(
            "Alte Passwortdatei konnte nicht entfernt werden: {error}"
        ))),
    }
}
fn load_with(
    store: &impl CredentialStore,
    path: &Path,
    url: &str,
    user: &str,
) -> Result<Option<String>> {
    if let Some(bytes) = store.get(url, user)? {
        let password = String::from_utf8(bytes)
            .map_err(|_| Error("Ungültiges Passwortformat im Schlüsselbund".into()))?;
        // Finish cleanup if an earlier process stopped after the keyring write.
        remove_legacy(path)?;
        return Ok(Some(password));
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let password = String::from_utf8(bytes)
        .map_err(|_| Error("Alte Passwortdatei enthält kein UTF-8".into()))?;
    // Keep the only copy until a successful keyring write. Password whitespace is significant.
    store.set(url, user, password.as_bytes())?;
    remove_legacy(path)?;
    Ok(Some(password))
}
pub fn save_password(url: &str, user: &str, password: &str) -> Result<()> {
    SystemKeyring.set(url, user, password.as_bytes())?;
    remove_legacy(&secret_path(url, user))
}
pub fn load_password(url: &str, user: &str) -> Result<Option<String>> {
    // Explicit process-only CLI/test override, never persisted.
    if let Ok(password) = std::env::var("CLOUDSAVE_WEBDAV_PASSWORD") {
        if !password.is_empty() {
            return Ok(Some(password));
        }
    }
    load_with(&SystemKeyring, &secret_path(url, user), url, user)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct MemoryStore {
        value: RefCell<Option<Vec<u8>>>,
        fail_write: bool,
    }
    impl CredentialStore for MemoryStore {
        fn get(&self, _: &str, _: &str) -> Result<Option<Vec<u8>>> {
            Ok(self.value.borrow().clone())
        }
        fn set(&self, _: &str, _: &str, password: &[u8]) -> Result<()> {
            if self.fail_write {
                return Err(Error("Schlüsselbund gesperrt".into()));
            }
            *self.value.borrow_mut() = Some(password.to_vec());
            Ok(())
        }
    }
    #[test]
    fn migration_keeps_legacy_file_on_failure_and_preserves_exact_password() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy.secret");
        fs::write(&path, b"  synthetic password\n").unwrap();
        let mut store = MemoryStore {
            value: RefCell::new(None),
            fail_write: true,
        };
        assert!(load_with(&store, &path, "https://example.com", "qa").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"  synthetic password\n");
        store.fail_write = false;
        assert_eq!(
            load_with(&store, &path, "https://example.com", "qa")
                .unwrap()
                .as_deref(),
            Some("  synthetic password\n")
        );
        assert!(!path.exists());
        assert_eq!(
            load_with(&store, &path, "https://example.com", "qa")
                .unwrap()
                .as_deref(),
            Some("  synthetic password\n")
        );
    }
    #[test]
    fn keyring_entry_takes_precedence_over_stale_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy.secret");
        fs::write(&path, b"stale").unwrap();
        let store = MemoryStore {
            value: RefCell::new(Some(b"current".to_vec())),
            fail_write: false,
        };
        assert_eq!(
            load_with(&store, &path, "url", "user").unwrap().as_deref(),
            Some("current")
        );
        assert!(!path.exists());
    }
    #[test]
    #[ignore = "requires an isolated unlocked Secret Service session"]
    fn system_keyring_roundtrip() {
        let url = format!("https://qa-{}.invalid", std::process::id());
        save_password(&url, "synthetic-qa", " space-sensitive ✓ ").unwrap();
        assert_eq!(
            SystemKeyring.get(&url, "synthetic-qa").unwrap().unwrap(),
            " space-sensitive ✓ ".as_bytes()
        );
        let service = SecretService::connect(EncryptionType::Dh).unwrap();
        for item in service
            .search_items(attributes(&url, "synthetic-qa"))
            .unwrap()
            .unlocked
        {
            item.delete().unwrap();
        }
    }
}
