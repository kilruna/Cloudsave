use std::path::PathBuf;

/// Work units describe checked files, not a fabricated byte/throughput estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Scanning,
    Checking { completed: usize, total: usize },
    Uploading { path: PathBuf, target: String },
    Downloading { path: PathBuf },
    Deleting { path: PathBuf, target: String },
}
