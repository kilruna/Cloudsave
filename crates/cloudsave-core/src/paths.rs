use std::io;
use std::path::{Component, Path, PathBuf};

// Resolve existing ancestors too, so a not-yet-created target under a symlink is checked correctly.
pub(crate) fn resolved(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::CurDir => {}
            part => normalized.push(part.as_os_str()),
        }
    }
    let mut ancestor = normalized.clone();
    let mut suffix = Vec::new();
    while !ancestor.try_exists()? {
        suffix.push(
            ancestor
                .file_name()
                .ok_or_else(|| io::Error::other("Pfad kann nicht aufgelöst werden"))?
                .to_os_string(),
        );
        ancestor.pop();
    }
    let mut result = std::fs::canonicalize(ancestor)?;
    for name in suffix.into_iter().rev() {
        result.push(name);
    }
    Ok(result)
}
