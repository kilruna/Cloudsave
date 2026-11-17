//! Small, bounded session activity shared by workers and the tray. No GTK on workers.
use cloudsave_core::{Progress, RestoreReport, SyncReport, Target};
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub label: String,
    pub detail: String,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub active: Vec<Entry>,
    pub recent: Vec<Entry>,
    pub status: TrayStatus,
    pub has_error: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrayStatus {
    #[default]
    Ready,
    Busy,
    Paused,
    Error,
    Success,
}

impl TrayStatus {
    pub const ALL: [Self; 5] = [
        Self::Ready,
        Self::Busy,
        Self::Paused,
        Self::Error,
        Self::Success,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Bereit · noch kein erfolgreicher Sync",
            Self::Busy => "Datei-Auftrag läuft",
            Self::Paused => "Automatische Synchronisierung pausiert",
            Self::Error => "Fehler · Sicherung prüfen",
            Self::Success => "Synchronisierung erfolgreich",
        }
    }
}

#[derive(Default)]
struct State {
    next: u64,
    active: Vec<Entry>,
    recent: VecDeque<Entry>,
    auto_sync_paused: bool,
    last_sync_succeeded: Option<bool>,
    failed_restores: Vec<Target>,
}

impl State {
    fn has_error(&self) -> bool {
        self.last_sync_succeeded == Some(false) || !self.failed_restores.is_empty()
    }

    fn tray_status(&self) -> TrayStatus {
        if !self.active.is_empty() {
            TrayStatus::Busy
        } else if self.has_error() {
            TrayStatus::Error
        } else if self.auto_sync_paused {
            TrayStatus::Paused
        } else if self.last_sync_succeeded == Some(true) {
            TrayStatus::Success
        } else {
            TrayStatus::Ready
        }
    }
}
static OPERATIONS: Mutex<()> = Mutex::new(());
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}
pub fn snapshot() -> Snapshot {
    let state = state();
    Snapshot {
        active: state.active.clone(),
        recent: state.recent.iter().cloned().collect(),
        status: state.tray_status(),
        has_error: state.has_error(),
    }
}

pub fn set_auto_sync(enabled: bool) {
    state().auto_sync_paused = !enabled;
}

enum Outcome<'a> {
    Sync(bool),
    Restore(&'a Target, bool),
}

#[must_use]
pub struct Job {
    id: u64,
    finished: bool,
}
impl Job {
    pub fn start(label: &str) -> Self {
        let mut state = state();
        state.next += 1;
        let id = state.next;
        state.active.push(Entry {
            id,
            label: label.into(),
            detail: "Wird vorbereitet …".into(),
        });
        Self {
            id,
            finished: false,
        }
    }
    pub fn progress(&self, progress: Progress) {
        let detail = match progress {
            Progress::Scanning => "Ordner werden geprüft …".into(),
            Progress::Checking { completed, total } => {
                format!("{completed}/{total} Dateien geprüft")
            }
            Progress::Uploading { path, target } => {
                format!("Upload: {} → {target}", path.display())
            }
            Progress::Downloading { path } => format!("Download: {}", path.display()),
            Progress::Deleting { path, target } => {
                format!("Spiegelung löscht: {} · {target}", path.display())
            }
        };
        if let Some(entry) = state().active.iter_mut().find(|entry| entry.id == self.id) {
            entry.detail = detail;
        }
    }
    fn finish(mut self, detail: String, outcome: Outcome<'_>) {
        self.complete(detail, Some(outcome));
    }
    fn complete(&mut self, detail: String, outcome: Option<Outcome<'_>>) {
        let mut state = state();
        match outcome {
            Some(Outcome::Sync(succeeded)) => state.last_sync_succeeded = Some(succeeded),
            Some(Outcome::Restore(target, true)) => {
                state.failed_restores.retain(|entry| entry != target)
            }
            Some(Outcome::Restore(target, false)) if !state.failed_restores.contains(target) => {
                state.failed_restores.push(target.clone());
            }
            _ => {}
        }
        if let Some(index) = state.active.iter().position(|entry| entry.id == self.id) {
            let mut entry = state.active.remove(index);
            let time = gtk4::glib::DateTime::now_local()
                .and_then(|time| time.format("%H:%M"))
                .map(|s| s.to_string())
                .unwrap_or_default();
            entry.detail = format!("{time} · {detail}");
            state.recent.push_front(entry);
            state.recent.truncate(8);
        }
        self.finished = true;
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if !self.finished {
            self.complete("Abgebrochen".into(), None);
        }
    }
}
pub fn sync(
    config: &cloudsave_core::Config,
    state: &std::path::Path,
) -> cloudsave_core::Result<SyncReport> {
    let _operation = OPERATIONS.try_lock().map_err(|_| {
        cloudsave_core::Error("Ein anderer Datei-Auftrag läuft bereits. Bitte kurz warten.".into())
    })?;
    let job = Job::start("Synchronisierung");
    let result =
        cloudsave_core::sync_with_progress(config, state, |progress| job.progress(progress));
    let detail = match &result {
        Ok(report) => format!(
            "{} hochgeladen · {} gelöscht · {} unverändert · {} Fehler",
            report.uploaded,
            report.deleted,
            report.skipped,
            report.errors.len()
        ),
        Err(error) => format!("Fehlgeschlagen: {error}"),
    };
    job.finish(
        detail,
        Outcome::Sync(result.as_ref().is_ok_and(|report| report.errors.is_empty())),
    );
    result
}
pub fn restore(
    target: &Target,
    task: impl FnOnce(&Job) -> cloudsave_core::Result<RestoreReport>,
) -> cloudsave_core::Result<RestoreReport> {
    let _operation = OPERATIONS.try_lock().map_err(|_| {
        cloudsave_core::Error("Ein anderer Datei-Auftrag läuft bereits. Bitte kurz warten.".into())
    })?;
    let job = Job::start("Wiederherstellung");
    let result = task(&job);
    let detail = match &result {
        Ok(report) => format!(
            "{} heruntergeladen · {} vorhandene behalten · {} Fehler",
            report.restored,
            report.skipped,
            report.errors.len()
        ),
        Err(error) => format!("Fehlgeschlagen: {error}"),
    };
    job.finish(
        detail,
        Outcome::Restore(
            target,
            result.as_ref().is_ok_and(|report| report.errors.is_empty()),
        ),
    );
    result
}
