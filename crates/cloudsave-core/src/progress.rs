use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

/// Work units describe checked files, not a fabricated byte/throughput estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Scanning,
    Checking { completed: usize, total: usize },
    Uploading { path: PathBuf, target: String },
    Downloading { path: PathBuf },
    Deleting { path: PathBuf, target: String },
    RemovingFolder { name: String, target: String },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransferStatus {
    #[default]
    Running,
    PauseRequested,
    Paused,
    Cancelling,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    pub path: PathBuf,
    pub bytes: u64,
    pub total: Option<u64>,
    pub elapsed: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryProgress {
    pub attempt: u8,
    pub attempts: u8,
    pub remaining: Duration,
}

#[derive(Clone, Default)]
pub struct TransferControl(Arc<Control>);

#[derive(Default)]
struct Control {
    cancelled: AtomicBool,
    state: Mutex<TransferState>,
    changed: Condvar,
}

#[derive(Default)]
struct TransferState {
    status: TransferStatus,
    // Keyed by path, so parallel transfers each measure their own file.
    files: Vec<Measurement>,
    retry: Option<(u8, Instant)>,
}

struct Measurement {
    path: PathBuf,
    total: Option<u64>,
    started: Instant,
    bytes: Arc<AtomicU64>,
}

impl TransferControl {
    pub fn status(&self) -> TransferStatus {
        self.0.state.lock().unwrap().status
    }

    pub fn progress(&self) -> Option<TransferProgress> {
        let state = self.0.state.lock().unwrap();
        let file = state.files.last()?;
        Some(TransferProgress {
            path: file.path.clone(),
            bytes: file.bytes.load(Ordering::Relaxed),
            total: file.total,
            elapsed: file.started.elapsed(),
        })
    }

    pub fn retry_progress(&self) -> Option<RetryProgress> {
        let (attempt, until) = self.0.state.lock().unwrap().retry?;
        Some(RetryProgress {
            attempt,
            attempts: crate::retry::ATTEMPTS,
            remaining: until.saturating_duration_since(Instant::now()),
        })
    }

    pub fn toggle_pause(&self) {
        let mut state = self.0.state.lock().unwrap();
        state.status = match state.status {
            TransferStatus::Running => TransferStatus::PauseRequested,
            TransferStatus::Paused | TransferStatus::PauseRequested => TransferStatus::Running,
            TransferStatus::Cancelling => return,
        };
        self.0.changed.notify_all();
    }

    pub fn cancel(&self) {
        let mut state = self.0.state.lock().unwrap();
        self.0.cancelled.store(true, Ordering::Relaxed);
        state.status = TransferStatus::Cancelling;
        self.0.changed.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Relaxed)
    }

    pub(crate) fn check_cancelled(&self) -> io::Result<()> {
        if self.is_cancelled() {
            // io::copy retries Interrupted automatically.
            return Err(io::Error::other("Auftrag abgebrochen"));
        }
        Ok(())
    }

    /// A pause takes effect between files, so it never holds an HTTP request open.
    pub(crate) fn checkpoint(&self) -> io::Result<()> {
        let mut state = self.0.state.lock().unwrap();
        while matches!(
            state.status,
            TransferStatus::Paused | TransferStatus::PauseRequested
        ) {
            state.status = TransferStatus::Paused;
            state = self.0.changed.wait(state).unwrap();
        }
        self.check_cancelled()
    }

    pub(crate) fn wait_to_retry(&self, attempt: u8, delay: Duration) -> io::Result<()> {
        let until = Instant::now() + delay;
        let mut state = self.0.state.lock().unwrap();
        state.retry = Some((attempt, until));
        while state.status != TransferStatus::Cancelling {
            if matches!(
                state.status,
                TransferStatus::PauseRequested | TransferStatus::Paused
            ) {
                state.status = TransferStatus::Paused;
                state = self.0.changed.wait(state).unwrap();
            } else {
                let remaining = until.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                state = self.0.changed.wait_timeout(state, remaining).unwrap().0;
            }
        }
        state.retry = None;
        self.check_cancelled()
    }

    pub(crate) fn begin_file(&self, path: &Path, total: Option<u64>) -> FileTransfer<'_> {
        let bytes = Arc::new(AtomicU64::new(0));
        let mut state = self.0.state.lock().unwrap();
        state.files.retain(|file| file.path != path);
        state.files.push(Measurement {
            path: path.to_path_buf(),
            total,
            started: Instant::now(),
            bytes: bytes.clone(),
        });
        FileTransfer {
            control: self,
            path: path.to_path_buf(),
            bytes,
        }
    }

    pub(crate) fn reader<R>(&self, reader: R, measure: bool) -> TransferReader<R> {
        let bytes = if measure {
            self.0
                .state
                .lock()
                .unwrap()
                .files
                .last()
                .map(|file| file.bytes.clone())
        } else {
            None
        };
        TransferReader {
            reader,
            control: self.clone(),
            bytes,
        }
    }

    /// Bind byte measurement to the transfer started for `path`, even while
    /// other transfers run in parallel.
    pub(crate) fn reader_for<R>(&self, path: &Path, reader: R, measure: bool) -> TransferReader<R> {
        let bytes = if measure {
            self.0
                .state
                .lock()
                .unwrap()
                .files
                .iter()
                .find(|file| file.path == path)
                .map(|file| file.bytes.clone())
        } else {
            None
        };
        TransferReader {
            reader,
            control: self.clone(),
            bytes,
        }
    }
}

pub(crate) struct FileTransfer<'a> {
    control: &'a TransferControl,
    path: PathBuf,
    bytes: Arc<AtomicU64>,
}

impl FileTransfer<'_> {
    pub(crate) fn wrote(&self, bytes: usize) {
        self.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    pub(crate) fn check_cancelled(&self) -> io::Result<()> {
        self.control.check_cancelled()
    }
}

impl Drop for FileTransfer<'_> {
    fn drop(&mut self) {
        self.control
            .0
            .state
            .lock()
            .unwrap()
            .files
            .retain(|file| file.path != self.path);
    }
}

pub(crate) struct TransferReader<R> {
    reader: R,
    control: TransferControl,
    bytes: Option<Arc<AtomicU64>>,
}

impl<R: Read> Read for TransferReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.control.check_cancelled()?;
        let read = self.reader.read(buffer)?;
        if let Some(bytes) = &self.bytes {
            bytes.fetch_add(read as u64, Ordering::Relaxed);
        }
        Ok(read)
    }
}
