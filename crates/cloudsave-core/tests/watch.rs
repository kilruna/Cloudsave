use cloudsave_core::{ChangeWatcher, WatchConfig, WatchEvent};
use std::{
    fs,
    time::{Duration, Instant},
};

fn wait_for(watcher: &ChangeWatcher, check: impl Fn(&WatchEvent) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let event = watcher
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("missing filesystem event");
        if let WatchEvent::Unavailable(error) = &event {
            panic!("watcher unavailable: {error}");
        }
        if check(&event) {
            return;
        }
    }
}
#[test]
fn filesystem_changes_are_detected_and_excluded_noise_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    let watcher = ChangeWatcher::new(WatchConfig {
        folders: vec![source.clone()],
        exclude_patterns: vec!["*.tmp".into()],
        ignored_paths: vec![],
    });
    wait_for(&watcher, |event| matches!(event, WatchEvent::Ready));
    fs::write(source.join("ignored.tmp"), b"noise").unwrap();
    assert!(watcher.recv_timeout(Duration::from_millis(300)).is_err());
    for operation in 0..4 {
        while watcher.try_recv().is_ok() {}
        match operation {
            0 => fs::write(source.join("save"), b"create").unwrap(),
            1 => fs::write(source.join("save"), b"modify longer").unwrap(),
            2 => fs::rename(source.join("save"), source.join("renamed")).unwrap(),
            _ => fs::remove_file(source.join("renamed")).unwrap(),
        }
        wait_for(&watcher, |event| matches!(event, WatchEvent::Changed));
    }
}
#[test]
fn watcher_reconfiguration_drops_old_sources_and_reports_unavailable_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let first = tmp.path().join("first");
    let second = tmp.path().join("second");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let mut config = WatchConfig {
        folders: vec![first.clone()],
        exclude_patterns: vec![],
        ignored_paths: vec![],
    };
    let watcher = ChangeWatcher::new(config.clone());
    wait_for(&watcher, |event| matches!(event, WatchEvent::Ready));
    config.folders = vec![second.clone()];
    watcher.update(config.clone());
    wait_for(&watcher, |event| matches!(event, WatchEvent::Ready));
    fs::write(first.join("old"), b"noise").unwrap();
    assert!(watcher.recv_timeout(Duration::from_millis(300)).is_err());
    fs::write(second.join("new"), b"change").unwrap();
    wait_for(&watcher, |event| matches!(event, WatchEvent::Changed));
    config.folders = vec![tmp.path().join("missing")];
    watcher.update(config);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if matches!(
            watcher
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap(),
            WatchEvent::Unavailable(_)
        ) {
            break;
        }
    }
}
