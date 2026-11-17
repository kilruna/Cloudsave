use cloudsave_core::{list_backups, restore_files, sync, BackupFile, Config, Settings, Target};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn restore_on_a_fresh_system_needs_only_the_backup_target() {
    let tmp = tempfile::tempdir().unwrap();
    let old_home = tmp.path().join("old-home");
    let source = old_home.join("Documents");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("nested/ältere Datei.txt"), b"previous system").unwrap();
    let target = Target::Local {
        path: tmp.path().join("backup"),
    };
    let config_path = old_home.join("config.toml");
    let state_path = old_home.join("state.db");
    let config = Config {
        folders: vec![source],
        targets: vec![target.clone()],
        settings: Settings::default(),
    };
    config.save(&config_path).unwrap();
    assert_eq!(sync(&config, &state_path).unwrap().uploaded, 1);
    fs::remove_dir_all(&old_home).unwrap();
    let files = list_backups(&target).unwrap();
    assert_eq!(files.len(), 1);
    let home = tmp.path().join("new-home");
    fs::create_dir(&home).unwrap();
    let report = restore_files(&target, &files, &home).unwrap();
    assert_eq!(report.restored, 1);
    assert!(report.errors.is_empty());
    assert_eq!(
        fs::read(home.join("Documents/nested/ältere Datei.txt")).unwrap(),
        b"previous system"
    );
    assert!(!old_home.exists());
    // Listing/restoring does not adopt or modify the remote inventory.
    assert_eq!(list_backups(&target).unwrap(), files);
}

#[test]
fn restore_roundtrip_keeps_unicode_and_never_overwrites_existing_files() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("Documents");
    let backup = tmp.path().join("backup");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("save.txt"), b"saved data").unwrap();
    fs::write(source.join("nested/ümlaut.txt"), "✓ Unicode".as_bytes()).unwrap();
    let target = Target::Local { path: backup };
    let config = Config {
        folders: vec![source],
        targets: vec![target.clone()],
        settings: Settings::default(),
    };
    assert!(sync(&config, &tmp.path().join("state.db"))
        .unwrap()
        .errors
        .is_empty());
    let files = list_backups(&target).unwrap();
    assert_eq!(files.len(), 2);
    let output = tmp.path().join("restore");
    fs::create_dir(&output).unwrap();
    let report = restore_files(&target, &files, &output).unwrap();
    assert_eq!(report.restored, 2);
    assert!(report.errors.is_empty());
    assert_eq!(
        fs::read(output.join("Documents/nested/ümlaut.txt")).unwrap(),
        "✓ Unicode".as_bytes()
    );
    fs::write(output.join("Documents/save.txt"), b"new local contents").unwrap();
    let report = restore_files(&target, &files, &output).unwrap();
    assert_eq!(report.restored, 0);
    assert_eq!(report.errors.len(), 2);
    assert_eq!(
        fs::read(output.join("Documents/save.txt")).unwrap(),
        b"new local contents"
    );
}

#[test]
fn restore_rejects_traversal_absolute_paths_and_symlink_parents() {
    let tmp = tempfile::tempdir().unwrap();
    let backup = tmp.path().join("backup");
    let output = tmp.path().join("output");
    fs::create_dir(&backup).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(backup.join("save"), b"data").unwrap();
    let target = Target::Local {
        path: backup.clone(),
    };
    for path in [
        "../escape",
        "/absolute",
        "dir/../../escape",
        "dir\\..\\escape",
    ] {
        assert!(restore_files(
            &target,
            &[BackupFile {
                path: PathBuf::from(path),
                size: Some(4)
            }],
            &output
        )
        .is_err());
    }
    assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    #[cfg(unix)]
    {
        fs::create_dir_all(backup.join("nested")).unwrap();
        fs::write(backup.join("nested/save"), b"data").unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, output.join("nested")).unwrap();
        let report = restore_files(
            &target,
            &[BackupFile {
                path: Path::new("nested/save").into(),
                size: Some(4),
            }],
            &output,
        )
        .unwrap();
        assert_eq!(report.restored, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(!outside.join("save").exists());
    }
}

#[test]
fn changed_or_missing_backup_never_leaves_a_partial_destination_file() {
    let tmp = tempfile::tempdir().unwrap();
    let backup = tmp.path().join("backup");
    let output = tmp.path().join("output");
    fs::create_dir(&backup).unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(backup.join("save"), b"short").unwrap();
    let target = Target::Local { path: backup };
    let report = restore_files(
        &target,
        &[
            BackupFile {
                path: "save".into(),
                size: Some(500),
            },
            BackupFile {
                path: "missing".into(),
                size: None,
            },
        ],
        &output,
    )
    .unwrap();
    assert_eq!(report.restored, 0);
    assert_eq!(report.errors.len(), 2);
    assert_eq!(fs::read_dir(output).unwrap().count(), 0);
}
