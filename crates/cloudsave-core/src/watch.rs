use crate::exclusions::Exclusions;
use crate::{Config, Target};
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchConfig {
    pub folders: Vec<PathBuf>,
    pub exclude_patterns: Vec<String>,
    pub ignored_paths: Vec<PathBuf>,
}
impl WatchConfig {
    pub fn for_config(config: &Config, state_path: &Path) -> Self {
        let mut ignored_paths: Vec<PathBuf> = config
            .targets
            .iter()
            .filter_map(|target| match target {
                Target::Local { path } => Some(path.clone()),
                _ => None,
            })
            .collect();
        if let Some(parent) = state_path.parent() {
            ignored_paths.push(parent.to_path_buf());
        }
        if let Some(parent) = Config::default_path().parent() {
            ignored_paths.push(parent.to_path_buf());
        }
        Self {
            folders: config.folders.clone(),
            exclude_patterns: config.settings.exclude_patterns.clone(),
            ignored_paths,
        }
    }
}

#[derive(Debug)]
pub enum WatchEvent {
    Ready,
    Changed,
    Unavailable(String),
}

pub struct ChangeWatcher {
    control: Sender<Option<WatchConfig>>,
    events: Receiver<WatchEvent>,
}
impl ChangeWatcher {
    pub fn new(config: WatchConfig) -> Self {
        let (control, commands) = mpsc::channel::<Option<WatchConfig>>();
        let (events_tx, events) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            let mut active = None;
            while let Ok(Some(config)) = commands.recv() {
                // Drop all previous registrations before replacing them.
                drop(active.take());
                let exclusions = match Exclusions::new(&config.exclude_patterns) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = events_tx.try_send(WatchEvent::Unavailable(error.to_string()));
                        continue;
                    }
                };
                let filter = config.clone();
                let output = events_tx.clone();
                let watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        match event {
                            Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
                                if event
                                    .paths
                                    .iter()
                                    .any(|path| relevant(path, &filter, &exclusions))
                                {
                                    // Coalesce bursts; a full channel already contains a pending change.
                                    let _ = output.try_send(WatchEvent::Changed);
                                }
                            }
                            Err(error) => {
                                let _ = output.try_send(WatchEvent::Unavailable(error.to_string()));
                            }
                            _ => {}
                        }
                    });
                match watcher {
                    Ok(mut watcher) => {
                        let mut failed = false;
                        for folder in &config.folders {
                            if let Err(error) = watcher.watch(folder, RecursiveMode::Recursive) {
                                failed = true;
                                let _ = events_tx.try_send(WatchEvent::Unavailable(format!(
                                    "{}: {error}",
                                    folder.display()
                                )));
                            }
                        }
                        active = Some(watcher);
                        if !failed {
                            let _ = events_tx.try_send(WatchEvent::Ready);
                        }
                    }
                    Err(error) => {
                        let _ = events_tx.try_send(WatchEvent::Unavailable(error.to_string()));
                    }
                }
            }
        });
        let watcher = Self { control, events };
        watcher.update(config);
        watcher
    }
    pub fn update(&self, config: WatchConfig) {
        let _ = self.control.send(Some(config));
    }
    pub fn try_recv(&self) -> Result<WatchEvent, mpsc::TryRecvError> {
        self.events.try_recv()
    }
    pub fn recv_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<WatchEvent, mpsc::RecvTimeoutError> {
        self.events.recv_timeout(timeout)
    }
}
impl Drop for ChangeWatcher {
    fn drop(&mut self) {
        let _ = self.control.send(None);
    }
}

fn relevant(path: &Path, config: &WatchConfig, exclusions: &Exclusions) -> bool {
    if config
        .ignored_paths
        .iter()
        .any(|ignored| path.starts_with(ignored))
    {
        return false;
    }
    config.folders.iter().any(|root| {
        path.strip_prefix(root)
            .is_ok_and(|relative| !exclusions.matches(relative))
    })
}
