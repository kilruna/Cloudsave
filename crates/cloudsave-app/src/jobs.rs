//! Bounded activity shared by the workers, dashboard and tray.
use cloudsave_core::{
    Config, FinishedJob, JobHistory, Progress, RestoreReport, SyncReport, Target, TransferControl,
    TransferProgress, TransferStatus,
};
use gtk4::{gio, glib, prelude::*};
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
    pub active: Option<ActiveEntry>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveEntry {
    pub entry: Entry,
    pub control_status: TransferStatus,
    pub fraction: Option<(u64, u64)>,
}

struct ActiveJob {
    entry: Entry,
    control: TransferControl,
}

#[derive(Default)]
struct State {
    next: u64,
    active: Option<ActiveJob>,
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
        if let Some(job) = &self.active {
            match job.control.status() {
                TransferStatus::Paused => TrayStatus::Paused,
                _ => TrayStatus::Busy,
            }
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

fn open_history() -> cloudsave_core::Result<JobHistory> {
    JobHistory::open(&Config::default_state_path().with_file_name("jobs.db"))
}

pub fn load_history() {
    let path = Config::default_state_path().with_file_name("jobs.db");
    if !path.is_file() {
        return;
    }
    match open_history().and_then(|history| history.recent()) {
        Ok(history) => {
            let mut state = state();
            for job in history.into_iter().rev() {
                state.next += 1;
                let id = state.next;
                state.recent.push_front(history_entry(id, job));
            }
        }
        Err(error) => eprintln!("Auftragsverlauf nicht verfügbar: {error}"),
    }
}

fn history_entry(id: u64, job: FinishedJob) -> Entry {
    let time = glib::DateTime::from_unix_local(job.finished_at)
        .and_then(|time| time.format("%d.%m. %H:%M"))
        .map(|time| time.to_string())
        .unwrap_or_default();
    Entry {
        id,
        label: job.label,
        detail: format!("{time} · {}", job.detail),
    }
}
pub fn snapshot() -> Snapshot {
    let state = state();
    Snapshot {
        active: state.active.as_ref().map(ActiveJob::snapshot),
        recent: state.recent.iter().cloned().collect(),
        status: state.tray_status(),
        has_error: state.has_error(),
    }
}

impl ActiveJob {
    fn snapshot(&self) -> ActiveEntry {
        let control_status = self.control.status();
        let measurement = self.control.progress();
        let retry = self.control.retry_progress();
        let mut entry = self.entry.clone();
        entry.detail = match control_status {
            TransferStatus::Running => {
                if let Some(retry) = retry {
                    format!(
                        "WebDAV vorübergehend nicht verfügbar · Versuch {}/{} in {} s · {}",
                        retry.attempt,
                        retry.attempts,
                        retry.remaining.as_secs_f64().ceil() as u64,
                        entry.detail
                    )
                } else if let Some(file) = &measurement {
                    format!("{} · {}", transfer_detail(file), entry.detail)
                } else {
                    entry.detail
                }
            }
            TransferStatus::PauseRequested => {
                format!("Pause nach der aktuellen Datei · {}", entry.detail)
            }
            TransferStatus::Paused => "Auftrag pausiert".into(),
            TransferStatus::Cancelling => "Abbruch angefordert …".into(),
        };
        ActiveEntry {
            entry,
            control_status,
            fraction: measurement.and_then(|file| {
                file.total
                    .filter(|total| *total > 0)
                    .map(|total| (file.bytes, total))
            }),
        }
    }
}

pub fn operate(id: u64, action: impl FnOnce(&TransferControl)) {
    if let Some(job) = state().active.as_ref().filter(|job| job.entry.id == id) {
        action(&job.control);
    }
}

fn transfer_detail(file: &TransferProgress) -> String {
    let bytes = format_bytes(file.bytes as f64);
    let transferred = match file.total {
        Some(total) => format!("{bytes} / {}", format_bytes(total as f64)),
        None => bytes,
    };
    let seconds = file.elapsed.as_secs_f64();
    if file.bytes == 0 || seconds < 0.25 {
        return transferred;
    }
    let speed = file.bytes as f64 / seconds;
    let remaining = file
        .total
        .map(|total| total.saturating_sub(file.bytes) as f64 / speed);
    let rate = format!("{transferred} · {}/s", format_bytes(speed));
    match remaining {
        Some(time) if time >= 60.0 => format!("{rate} · noch ca. {:.0} Min", time / 60.0),
        Some(time) => format!("{rate} · noch ca. {:.0} s", time.ceil()),
        None => rate,
    }
}

pub fn format_bytes(mut bytes: f64) -> String {
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB"] {
        if bytes < 1024.0 {
            break;
        }
        bytes /= 1024.0;
        unit = next;
    }
    if unit == "B" {
        format!("{bytes:.0} B")
    } else {
        format!("{bytes:.1} {unit}").replace('.', ",")
    }
}

pub fn pause_label(status: TransferStatus) -> &'static str {
    match status {
        TransferStatus::Paused | TransferStatus::PauseRequested => "Fortsetzen",
        _ => "Pausieren",
    }
}

pub fn set_auto_sync(enabled: bool) {
    state().auto_sync_paused = !enabled;
}

pub fn target_removed(target: &Target) {
    let mut state = state();
    state.failed_restores.retain(|entry| entry != target);
    state.last_sync_succeeded = None;
}

enum Outcome<'a> {
    Sync(bool),
    Restore(&'a Target, bool),
}

#[must_use]
pub struct Job {
    id: u64,
    label: &'static str,
    control: TransferControl,
    finished: bool,
}
impl Job {
    fn start(label: &'static str, control: TransferControl) -> Self {
        let mut state = state();
        state.next += 1;
        let id = state.next;
        state.active = Some(ActiveJob {
            entry: Entry {
                id,
                label: label.into(),
                detail: "Wird vorbereitet …".into(),
            },
            control: control.clone(),
        });
        Self {
            id,
            label,
            control,
            finished: false,
        }
    }
    pub fn control(&self) -> &TransferControl {
        &self.control
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
            Progress::RemovingFolder { name, target } => {
                format!("Sicherungsordner entfernen: {name} · {target}")
            }
        };
        if let Some(job) = state()
            .active
            .as_mut()
            .filter(|job| job.entry.id == self.id)
        {
            job.entry.detail = detail;
        }
    }
    fn finish(mut self, detail: String, outcome: Option<Outcome<'_>>) {
        self.complete(detail, outcome);
    }
    fn complete(&mut self, detail: String, outcome: Option<Outcome<'_>>) {
        let finished = FinishedJob {
            finished_at: glib::DateTime::now_utc().map_or(0, |time| time.to_unix()),
            label: self.label.into(),
            detail,
        };
        if let Err(error) = open_history().and_then(|mut history| history.record(&finished)) {
            eprintln!("Auftragsverlauf konnte nicht gespeichert werden: {error}");
        }
        let mut state = state();
        let notify = match outcome {
            Some(Outcome::Sync(succeeded)) => {
                let new_failure = !succeeded && state.last_sync_succeeded != Some(false);
                state.last_sync_succeeded = Some(succeeded);
                new_failure
            }
            Some(Outcome::Restore(target, true)) => {
                state.failed_restores.retain(|entry| entry != target);
                false
            }
            Some(Outcome::Restore(target, false)) if !state.failed_restores.contains(target) => {
                state.failed_restores.push(target.clone());
                true
            }
            _ => false,
        };
        if state.active.take().is_some() {
            state.recent.push_front(history_entry(self.id, finished));
            state.recent.truncate(JobHistory::LIMIT);
        }
        drop(state);
        if notify {
            notify_failure(self.label);
        }
        self.finished = true;
    }
}

fn notify_failure(label: &'static str) {
    glib::idle_add_once(move || {
        if let Some(app) = gio::Application::default() {
            let notification = gio::Notification::new(&format!("{label} mit Fehlern"));
            notification.set_body(Some("Öffne Cloudsave, um den Auftrag zu prüfen."));
            notification.set_default_action("app.open");
            app.send_notification(None, &notification);
        }
    });
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
    control: TransferControl,
) -> cloudsave_core::Result<SyncReport> {
    let _operation = OPERATIONS.try_lock().map_err(|_| {
        cloudsave_core::Error("Ein anderer Datei-Auftrag läuft bereits. Bitte kurz warten.".into())
    })?;
    let job = Job::start("Synchronisierung", control);
    let result = cloudsave_core::sync_controlled(config, state, job.control(), |progress| {
        job.progress(progress)
    });
    let detail = match &result {
        Ok(report) if report.cancelled => format!("Abgebrochen · {} hochgeladen", report.uploaded),
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
        match &result {
            Ok(report) if !report.errors.is_empty() => Some(Outcome::Sync(false)),
            Ok(report) if report.cancelled => None,
            Ok(_) => Some(Outcome::Sync(true)),
            Err(_) => Some(Outcome::Sync(false)),
        },
    );
    result
}
pub fn restore(
    target: &Target,
    control: TransferControl,
    task: impl FnOnce(&Job) -> cloudsave_core::Result<RestoreReport>,
) -> cloudsave_core::Result<RestoreReport> {
    let _operation = OPERATIONS.try_lock().map_err(|_| {
        cloudsave_core::Error("Ein anderer Datei-Auftrag läuft bereits. Bitte kurz warten.".into())
    })?;
    let job = Job::start("Wiederherstellung", control);
    let result = task(&job);
    let detail = match &result {
        Ok(report) if report.cancelled => {
            format!("Abgebrochen · {} heruntergeladen", report.restored)
        }
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
        match &result {
            Ok(report) if !report.errors.is_empty() => Some(Outcome::Restore(target, false)),
            Ok(report) if report.cancelled => None,
            Ok(_) => Some(Outcome::Restore(target, true)),
            Err(_) => Some(Outcome::Restore(target, false)),
        },
    );
    result
}
