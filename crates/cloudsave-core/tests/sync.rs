use std::fs;
use std::path::Path;

use cloudsave_core::{sync, Config, Settings, State, SyncMode, SyncReport, Target};

fn make_config(src: &Path, dst: &Path) -> Config {
    Config {
        folders: vec![src.to_path_buf()],
        targets: vec![Target::Local {
            path: dst.to_path_buf(),
        }],
        settings: Settings {
            sync_mode: SyncMode::Mirror,
            ..Settings::default()
        },
    }
}

#[test]
fn config_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let cfg = make_config(Path::new("/tmp/quelle"), Path::new("/tmp/ziel"));
    cfg.save(&path).unwrap();
    let loaded = Config::load(&path).unwrap();
    assert_eq!(loaded.folders, cfg.folders);
    assert!(loaded.settings.auto_sync);
}

#[test]
fn local_backend_roundtrip_delta_and_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("quelle");
    let dst = tmp.path().join("ziel");
    let state_path = tmp.path().join("state.db");
    fs::create_dir_all(src.join("unter")).unwrap();
    fs::write(src.join("a.txt"), b"hallo").unwrap();
    fs::write(src.join("unter/b.txt"), b"welt").unwrap();

    let cfg = make_config(&src, &dst);

    let r: SyncReport = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.uploaded, 2);
    assert_eq!(r.deleted, 0);
    assert_eq!(r.errors, Vec::<String>::new());
    assert!(dst.join("quelle/a.txt").exists());
    assert_eq!(fs::read(dst.join("quelle/unter/b.txt")).unwrap(), b"welt");

    let state = State::open(&state_path).unwrap();
    assert_eq!(state.count().unwrap(), 2);
    assert_eq!(state.total_size().unwrap(), 9);
    assert!(state.last_synced().unwrap().is_some());

    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.uploaded, 0);
    assert_eq!(r.skipped, 2);

    fs::write(src.join("a.txt"), b"hallo welt, das ist neu").unwrap();
    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.uploaded, 1);
    assert_eq!(r.skipped, 1);
    assert_eq!(
        fs::read(dst.join("quelle/a.txt")).unwrap(),
        b"hallo welt, das ist neu"
    );

    fs::remove_file(src.join("unter/b.txt")).unwrap();
    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.deleted, 1);
    assert!(!dst.join("quelle/unter/b.txt").exists());
    assert_eq!(state.count().unwrap(), 1);
}

#[test]
fn missing_folder_is_reported_not_fatal() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = make_config(Path::new("/does/not/exist"), &tmp.path().join("ziel"));
    let r = sync(&cfg, &tmp.path().join("state.db")).unwrap();
    assert_eq!(r.uploaded, 0);
    assert_eq!(r.errors.len(), 1);
}

#[test]
fn multi_target_sync_reaches_all_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("quelle");
    let dst1 = tmp.path().join("ziel1");
    let dst2 = tmp.path().join("ziel2");
    let state_path = tmp.path().join("state.db");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), b"daten").unwrap();

    let cfg = Config {
        folders: vec![src.clone()],
        targets: vec![
            Target::Local { path: dst1.clone() },
            Target::Local { path: dst2.clone() },
        ],
        settings: Settings {
            sync_mode: SyncMode::Mirror,
            ..Settings::default()
        },
    };

    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.uploaded, 2, "errors: {:?}", r.errors);
    assert_eq!(fs::read(dst1.join("quelle/a.txt")).unwrap(), b"daten");
    assert_eq!(fs::read(dst2.join("quelle/a.txt")).unwrap(), b"daten");

    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.uploaded, 0);
    assert_eq!(r.skipped, 2);

    fs::remove_file(src.join("a.txt")).unwrap();
    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.deleted, 2);
    assert!(!dst1.join("quelle/a.txt").exists());
    assert!(!dst2.join("quelle/a.txt").exists());
}

#[test]
fn legacy_single_target_config_still_loads() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(
        &path,
        "folders = [\"/tmp/quelle\"]\n\n[target]\ntype = \"local\"\npath = \"/tmp/ziel\"\n",
    )
    .unwrap();
    let cfg = Config::load(&path).unwrap();
    assert_eq!(cfg.targets.len(), 1);
    assert_eq!(
        cfg.targets[0],
        Target::Local {
            path: Path::new("/tmp/ziel").to_path_buf()
        }
    );
}

#[test]
fn unavailable_or_deselected_sources_preserve_both_backups_after_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(src.join("save.txt"), b"save data").unwrap();
    fs::write(src.join("nested/ümlaut.txt"), b"unicode data").unwrap();
    let mut cfg = make_config(&src, &tmp.path().join("target1"));
    cfg.targets.push(Target::Local {
        path: tmp.path().join("target2"),
    });
    let state_path = tmp.path().join("state.db");
    assert!(sync(&cfg, &state_path).unwrap().errors.is_empty());
    // Every sync reopens the persisted state, as a fresh process would.
    fs::rename(&src, tmp.path().join("unavailable")).unwrap();
    let report = sync(&cfg, &state_path).unwrap();
    assert!(!report.errors.is_empty());
    assert_eq!(report.deleted, 0);
    cfg.folders.clear();
    let report = sync(&cfg, &state_path).unwrap();
    assert!(report.errors.is_empty());
    assert_eq!(report.deleted, 0);
    for target in &cfg.targets {
        let Target::Local { path } = target else {
            unreachable!()
        };
        assert_eq!(
            fs::read(path.join("Documents/save.txt")).unwrap(),
            b"save data"
        );
        assert_eq!(
            fs::read(path.join("Documents/nested/ümlaut.txt")).unwrap(),
            b"unicode data"
        );
    }
    assert_eq!(State::open(&state_path).unwrap().count().unwrap(), 4);
    // Re-enabling the source restores ordinary delete mirroring.
    fs::rename(tmp.path().join("unavailable"), &src).unwrap();
    cfg.folders.push(src.clone());
    fs::remove_file(src.join("save.txt")).unwrap();
    assert_eq!(sync(&cfg, &state_path).unwrap().deleted, 2);
}

#[test]
fn connection_test_preserves_existing_files_and_cleans_up() {
    use cloudsave_core::{Backend, LocalBackend};
    let tmp = tempfile::tempdir().unwrap();
    let sentinel = tmp.path().join(".cloudsave-test");
    fs::write(&sentinel, b"PREEXISTING USER DATA - KEEP ME").unwrap();
    let backend = LocalBackend::new(tmp.path()).unwrap();
    for _ in 0..3 {
        backend.test_connection().unwrap();
        assert_eq!(
            fs::read(&sentinel).unwrap(),
            b"PREEXISTING USER DATA - KEEP ME"
        );
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 1);
    }
}

#[test]
fn no_targets_is_an_error_but_an_empty_source_is_valid() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("empty");
    fs::create_dir(&src).unwrap();
    let mut cfg = make_config(&src, &tmp.path().join("target"));
    assert!(sync(&cfg, &tmp.path().join("state.db"))
        .unwrap()
        .errors
        .is_empty());
    cfg.targets.clear();
    let report = sync(&cfg, &tmp.path().join("state.db")).unwrap();
    assert!(report.errors.iter().any(|e| e.contains("Kein Endpunkt")));
}

#[cfg(unix)]
#[test]
fn incomplete_scan_preserves_backups_but_complete_scans_still_mirror_deletes() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    let hidden = src.join("private");
    let dst = tmp.path().join("target");
    let state_path = tmp.path().join("state.db");
    fs::create_dir_all(&hidden).unwrap();
    fs::write(hidden.join("save.txt"), b"keep me").unwrap();
    fs::write(
        src.join("deleted.txt"),
        b"mirror only after a complete scan",
    )
    .unwrap();
    let cfg = make_config(&src, &dst);
    assert_eq!(sync(&cfg, &state_path).unwrap().uploaded, 2);
    fs::remove_file(src.join("deleted.txt")).unwrap();
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o000)).unwrap();
    // A privileged test runner can bypass permissions. Run this test as a normal user.
    if fs::read_dir(&hidden).is_ok() {
        fs::set_permissions(&hidden, fs::Permissions::from_mode(0o700)).unwrap();
        eprintln!("permission regression requires an unprivileged runner");
        return;
    }
    let report = sync(&cfg, &state_path).unwrap();
    fs::set_permissions(&hidden, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(report.errors.iter().any(|e| e.contains("private")));
    assert_eq!(report.deleted, 0);
    assert_eq!(
        fs::read(dst.join("Documents/private/save.txt")).unwrap(),
        b"keep me"
    );
    assert!(dst.join("Documents/deleted.txt").exists());
    let report = sync(&cfg, &state_path).unwrap();
    assert!(report.errors.is_empty());
    assert_eq!(report.deleted, 1);
}

#[test]
fn target_write_errors_identify_source_and_destination() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    let dst = tmp.path().join("target");
    fs::create_dir(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::write(src.join("save.txt"), b"data").unwrap();
    // A file blocks creation of the target's source directory.
    fs::write(dst.join("Documents"), b"obstruction").unwrap();
    let report = sync(&make_config(&src, &dst), &tmp.path().join("state.db")).unwrap();
    assert_eq!(report.uploaded, 0);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains(&src.display().to_string())
            && e.contains(&dst.display().to_string())
            && e.contains("save.txt")));
}

#[test]
fn different_source_with_same_name_cannot_claim_old_backups() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("first/Documents");
    let replacement = tmp.path().join("second/Documents");
    let dst = tmp.path().join("target");
    let state_path = tmp.path().join("state.db");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&replacement).unwrap();
    fs::write(src.join("save.txt"), b"original").unwrap();
    let mut cfg = make_config(&src, &dst);
    sync(&cfg, &state_path).unwrap();
    cfg.folders = vec![replacement.clone()];
    assert_eq!(sync(&cfg, &state_path).unwrap().deleted, 0);
    fs::write(replacement.join("save.txt"), b"replacement").unwrap();
    assert!(!sync(&cfg, &state_path).unwrap().errors.is_empty());
    assert_eq!(
        fs::read(dst.join("Documents/save.txt")).unwrap(),
        b"original"
    );
    cfg.folders.push(src);
    assert!(!sync(&cfg, &state_path).unwrap().errors.is_empty());
    assert_eq!(
        fs::read(dst.join("Documents/save.txt")).unwrap(),
        b"original"
    );
}

#[test]
fn legacy_state_migration_keeps_unknown_backups_and_learns_observed_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    let dst = tmp.path().join("target");
    let state_path = tmp.path().join("state.db");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("save.txt"), b"original").unwrap();
    let cfg = make_config(&src, &dst);
    sync(&cfg, &state_path).unwrap();
    // Recreate the old schema: persisted backups have no recorded owner yet.
    rusqlite::Connection::open(&state_path)
        .unwrap()
        .execute("DROP TABLE file_sources", [])
        .unwrap();
    fs::rename(&src, tmp.path().join("unavailable")).unwrap();
    assert_eq!(sync(&cfg, &state_path).unwrap().deleted, 0);
    assert!(dst.join("Documents/save.txt").exists());
    fs::rename(tmp.path().join("unavailable"), &src).unwrap();
    assert_eq!(sync(&cfg, &state_path).unwrap().skipped, 1);
    fs::remove_file(src.join("save.txt")).unwrap();
    assert_eq!(sync(&cfg, &state_path).unwrap().deleted, 1);
}

#[test]
fn backup_mode_preserves_deleted_files_in_both_targets_and_mirror_remains_explicit() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("save.txt"), b"preserve").unwrap();
    let mut cfg = make_config(&src, &tmp.path().join("target1"));
    cfg.targets.push(Target::Local {
        path: tmp.path().join("target2"),
    });
    cfg.settings.sync_mode = SyncMode::Backup;
    let state = tmp.path().join("state.db");
    assert_eq!(sync(&cfg, &state).unwrap().uploaded, 2);
    fs::remove_file(src.join("save.txt")).unwrap();
    assert_eq!(sync(&cfg, &state).unwrap().deleted, 0);
    for name in ["target1", "target2"] {
        assert_eq!(
            fs::read(tmp.path().join(name).join("Documents/save.txt")).unwrap(),
            b"preserve"
        );
    }
    assert_eq!(State::open(&state).unwrap().count().unwrap(), 2);
    cfg.settings.sync_mode = SyncMode::Mirror;
    assert_eq!(sync(&cfg, &state).unwrap().deleted, 2);
}

#[test]
fn new_profiles_default_to_backup_and_legacy_profiles_keep_mirroring() {
    assert_eq!(Settings::default().sync_mode, SyncMode::Backup);
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("config.toml");
    fs::write(&config, "folders = []\ntargets = []\n").unwrap();
    assert_eq!(
        Config::load(&config).unwrap().settings.sync_mode,
        SyncMode::Mirror
    );
    fs::write(
        &config,
        "folders = []\ntargets = []\n[settings]\nauto_sync = false\n",
    )
    .unwrap();
    let mut loaded = Config::load(&config).unwrap();
    assert_eq!(loaded.settings.sync_mode, SyncMode::Mirror);
    loaded.settings.sync_mode = SyncMode::Backup;
    loaded.save(&config).unwrap();
    assert_eq!(
        Config::load(&config).unwrap().settings.sync_mode,
        SyncMode::Backup
    );
}

#[test]
fn incomplete_local_upload_preserves_previous_backup_and_cleans_tempfile() {
    use cloudsave_core::{Backend, LocalBackend};
    let tmp = tempfile::tempdir().unwrap();
    let dst = tmp.path().join("backup");
    let backend = LocalBackend::new(&dst).unwrap();
    let rel = Path::new("save.txt");
    fs::write(dst.join(rel), b"previous valid backup").unwrap();
    let source = tmp.path().join("source");
    fs::write(&source, b"short").unwrap();
    assert!(backend
        .put(rel, fs::File::open(&source).unwrap(), 100)
        .is_err());
    assert_eq!(fs::read(dst.join(rel)).unwrap(), b"previous valid backup");
    assert_eq!(fs::read_dir(&dst).unwrap().count(), 1);
    assert_eq!(
        backend
            .put(rel, fs::File::open(&source).unwrap(), 5)
            .unwrap(),
        5
    );
    assert_eq!(fs::read(dst.join(rel)).unwrap(), b"short");
    assert_eq!(fs::read_dir(&dst).unwrap().count(), 1);
}

#[test]
fn exclusion_patterns_skip_nested_noise_and_never_delete_existing_excluded_backups() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    fs::create_dir_all(src.join("project/node_modules")).unwrap();
    fs::create_dir_all(src.join(".cache")).unwrap();
    for file in [
        "keep.txt",
        "old.tmp",
        "project/node_modules/package.js",
        ".cache/cache.db",
    ] {
        fs::write(src.join(file), b"original").unwrap();
    }
    let dst = tmp.path().join("backup");
    let state = tmp.path().join("state.db");
    let mut cfg = make_config(&src, &dst);
    assert_eq!(sync(&cfg, &state).unwrap().uploaded, 4);
    cfg.settings.exclude_patterns = vec!["*.tmp".into(), "node_modules".into(), ".cache/**".into()];
    fs::write(src.join("new.tmp"), b"skip").unwrap();
    fs::remove_file(src.join("old.tmp")).unwrap();
    fs::remove_file(src.join("keep.txt")).unwrap();
    let report = sync(&cfg, &state).unwrap();
    assert!(report.errors.is_empty());
    assert_eq!(report.deleted, 1);
    assert!(dst.join("Documents/old.tmp").exists());
    assert!(!dst.join("Documents/new.tmp").exists());
    assert!(dst
        .join("Documents/project/node_modules/package.js")
        .exists());
    cfg.settings.exclude_patterns = vec!["[".into()];
    assert!(sync(&cfg, &state).is_err());
    assert!(dst.join("Documents/old.tmp").exists());
}

#[test]
fn exclusions_apply_to_all_targets_and_survive_config_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("source");
    fs::create_dir_all(src.join("nested/.cache")).unwrap();
    fs::write(src.join("nested/.cache/data"), b"ignore").unwrap();
    fs::write(src.join("keep"), b"keep").unwrap();
    let mut cfg = make_config(&src, &tmp.path().join("one"));
    cfg.targets.push(Target::Local {
        path: tmp.path().join("two"),
    });
    cfg.settings.exclude_patterns = vec![".cache".into()];
    let config = tmp.path().join("config.toml");
    cfg.save(&config).unwrap();
    let report = sync(
        &Config::load(&config).unwrap(),
        &tmp.path().join("state.db"),
    )
    .unwrap();
    assert_eq!(report.uploaded, 2);
    assert!(report.errors.is_empty());
    for target in ["one", "two"] {
        assert!(tmp.path().join(target).join("source/keep").exists());
        assert!(!tmp
            .path()
            .join(target)
            .join("source/nested/.cache/data")
            .exists());
    }
}

#[test]
fn local_backup_target_may_not_overlap_its_source_even_through_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("save"), b"source").unwrap();
    let mut cfg = make_config(&src, &src.join("backup"));
    let state = tmp.path().join("state.db");
    assert!(!sync(&cfg, &state).unwrap().errors.is_empty());
    assert!(!src.join("backup").exists());
    cfg.targets = vec![Target::Local {
        path: tmp.path().to_path_buf(),
    }];
    assert!(!sync(&cfg, &state).unwrap().errors.is_empty());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&src, tmp.path().join("alias")).unwrap();
        cfg.targets = vec![Target::Local {
            path: tmp.path().join("alias/new-backup"),
        }];
        assert!(!sync(&cfg, &state).unwrap().errors.is_empty());
        assert!(!src.join("new-backup").exists());
    }
    assert_eq!(fs::read(src.join("save")).unwrap(), b"source");
}

#[test]
fn same_size_same_second_changes_are_uploaded_without_reuploading_legacy_unchanged_files() {
    use std::time::{Duration, UNIX_EPOCH};
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Documents");
    fs::create_dir(&source).unwrap();
    let file = source.join("save");
    fs::write(&file, b"AAAA").unwrap();
    let stamp = UNIX_EPOCH + Duration::new(1_700_000_000, 100);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(stamp))
        .unwrap();
    let cfg = make_config(&source, &tmp.path().join("backup"));
    let state = tmp.path().join("state.db");
    assert_eq!(sync(&cfg, &state).unwrap().uploaded, 1);
    fs::write(&file, b"BBBB").unwrap();
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(stamp + Duration::from_nanos(100)))
        .unwrap();
    let report = sync(&cfg, &state).unwrap();
    assert_eq!(report.uploaded, 1);
    assert_eq!(
        fs::read(tmp.path().join("backup/Documents/save")).unwrap(),
        b"BBBB"
    );
    // A legacy second-based state stamp upgrades through hash verification, without another upload.
    rusqlite::Connection::open(&state)
        .unwrap()
        .execute("UPDATE files SET mtime = 1700000000", [])
        .unwrap();
    let report = sync(&cfg, &state).unwrap();
    assert_eq!(report.uploaded, 0);
    assert_eq!(report.skipped, 1);
}

#[test]
fn explicit_live_removal_survives_restart_and_retries_only_the_failed_target() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let state = temp.path().join("state.db");
    let config_path = temp.path().join("config.toml");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("nested/file"), b"fixture").unwrap();
    let mut config = make_config(&source, &first);
    config.settings.sync_mode = SyncMode::Backup;
    config.targets.push(Target::Local {
        path: second.clone(),
    });
    assert!(sync(&config, &state).unwrap().errors.is_empty());

    let unavailable = temp.path().join("unavailable");
    fs::rename(&second, &unavailable).unwrap();
    fs::write(&second, b"unavailable mount").unwrap();
    config.remove_folder(&source).unwrap();
    config.save(&config_path).unwrap();

    let mut detached = config.clone();
    detached.remove_target(&config.targets[0]);
    assert_eq!(
        detached.settings.pending_folder_removals[0].targets,
        [config.targets[1].clone()]
    );
    detached.remove_target(&config.targets[1]);
    assert!(detached.settings.pending_folder_removals.is_empty());

    let mut config = Config::load(&config_path).unwrap();
    let report = sync(&config, &state).unwrap();
    assert!(!report.errors.is_empty());
    assert_eq!(
        report.removed_folders,
        [(source.clone(), config.targets[0].clone())]
    );
    assert!(!first.join("source").exists());
    assert!(unavailable.join("source/nested/file").exists());
    let inventory = cloudsave_core::discover_backup(&config.targets[0]).unwrap();
    assert!(inventory.folders.is_empty());
    config.finish_folder_removals(&report.removed_folders);
    assert_eq!(
        config.settings.pending_folder_removals[0].targets,
        [config.targets[1].clone()]
    );

    fs::remove_file(&second).unwrap();
    fs::rename(&unavailable, &second).unwrap();
    let report = sync(&config, &state).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    config.finish_folder_removals(&report.removed_folders);
    assert!(config.settings.pending_folder_removals.is_empty());
    assert!(!second.join("source").exists());
    assert_eq!(fs::read(source.join("nested/file")).unwrap(), b"fixture");
    assert_eq!(State::open(&state).unwrap().count().unwrap(), 0);
}

#[test]
fn explicit_folder_removal_preserves_unknown_files_and_offline_backup() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    let state = temp.path().join("state.db");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("managed"), b"fixture").unwrap();
    let mut config = make_config(&source, &target);
    assert!(sync(&config, &state).unwrap().errors.is_empty());

    let mut paused = config.clone();
    paused.settings.auto_sync = false;
    paused.remove_folder(&source).unwrap();
    assert!(paused.settings.pending_folder_removals.is_empty());
    assert!(sync(&paused, &state).unwrap().errors.is_empty());
    assert!(target.join("source/managed").exists());

    fs::write(target.join("source/unmanaged"), b"foreign").unwrap();
    config.remove_folder(&source).unwrap();
    let report = sync(&config, &state).unwrap();
    assert!(report.errors[0].contains("nicht zugeordnet"));
    assert!(report.removed_folders.is_empty());
    assert!(target.join("source/managed").exists());
    assert_eq!(
        fs::read(target.join("source/unmanaged")).unwrap(),
        b"foreign"
    );
    assert!(source.join("managed").exists());
}

#[test]
fn update_keeps_previously_unchecked_folders_available_for_explicit_removal() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    let state = temp.path().join("state.db");
    let config_path = temp.path().join("config.toml");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("file"), b"already backed up").unwrap();
    let mut config = make_config(&source, &target);
    assert!(sync(&config, &state).unwrap().errors.is_empty());

    config.folders.clear();
    config.settings.disabled_folders = None;
    config.settings.auto_sync = false;
    config.save(&config_path).unwrap();
    let mut migrated = Config::load(&config_path).unwrap();
    assert!(migrated.initialize_disabled_folders(&state).unwrap());
    assert_eq!(
        migrated.settings.disabled_folders,
        Some(vec![source.clone()])
    );
    assert!(migrated.folders.is_empty());

    migrated.remove_folder(&source).unwrap();
    migrated.save(&config_path).unwrap();
    let mut restarted = Config::load(&config_path).unwrap();
    assert!(!restarted.initialize_disabled_folders(&state).unwrap());
    assert_eq!(restarted.settings.disabled_folders, Some(vec![]));
    assert!(target.join("source/file").exists());
}
