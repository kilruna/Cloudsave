use crate::{Error, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::path::{Component, Path};

#[derive(Clone)]
pub(crate) struct Exclusions {
    paths: GlobSet,
    components: GlobSet,
}
impl Exclusions {
    pub(crate) fn new(patterns: &[String]) -> Result<Self> {
        let mut paths = GlobSetBuilder::new();
        let mut components = GlobSetBuilder::new();
        for pattern in patterns
            .iter()
            .map(|pattern| pattern.trim())
            .filter(|pattern| !pattern.is_empty())
        {
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
                .map_err(|error| {
                    Error(format!("Ungültiges Ausschlussmuster {pattern:?}: {error}"))
                })?;
            if pattern.contains('/') {
                paths.add(glob);
                // Prune a directory before entering it when the entire subtree is excluded.
                if let Some(directory) = pattern.strip_suffix("/**") {
                    paths.add(
                        GlobBuilder::new(directory)
                            .literal_separator(true)
                            .backslash_escape(false)
                            .build()
                            .map_err(|error| Error(error.to_string()))?,
                    );
                }
            } else {
                components.add(glob);
            }
        }
        Ok(Self {
            paths: paths.build().map_err(|error| Error(error.to_string()))?,
            components: components
                .build()
                .map_err(|error| Error(error.to_string()))?,
        })
    }
    pub(crate) fn matches(&self, relative: &Path) -> bool {
        self.paths.is_match(relative)
            || relative.components().any(|component| match component {
                Component::Normal(name) => self.components.is_match(Path::new(name)),
                _ => false,
            })
    }
}

pub fn validate_exclusion_patterns(patterns: &[String]) -> Result<()> {
    Exclusions::new(patterns).map(|_| ())
}
