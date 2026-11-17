//! Small, bounded session activity shared by workers and the tray. No GTK on workers.
use cloudsave_core::{Progress, RestoreReport, SyncReport};
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
}
#[derive(Default)]
struct State {
    next: u64,
    active: Vec<Entry>,
    recent: VecDeque<Entry>,
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
    }
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
    pub fn finish(mut self, detail: String) {
        self.complete(detail);
    }
    fn complete(&mut self, detail: String) {
        let mut state = state();
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
            self.complete("Abgebrochen".into());
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
    job.finish(detail);
    result
}
pub fn restore(
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
    job.finish(detail);
    result
}
