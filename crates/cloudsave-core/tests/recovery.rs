use cloudsave_core::{
    discover_backup, list_backups, prepare_restore_location, restore_mapped, sync,
    sync_with_progress, Config, Progress, RestoreLocation, Settings, SyncMode, Target,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn whole_folder_reinstall_keeps_portable_paths_empty_folders_and_remote_names() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    let documents = old.join("Dokumente");
    let pictures = old.join("Bilder");
    let empty = old.join("Leer");
    fs::create_dir_all(documents.join("nested")).unwrap();
    fs::create_dir_all(&pictures).unwrap();
    fs::create_dir_all(&empty).unwrap();
    fs::write(documents.join("nested/ümlaut.txt"), "Unicode ✓").unwrap();
    fs::write(pictures.join("photo.jpg"), b"synthetic image").unwrap();
    let target = Target::Local {
        path: temp.path().join("backup"),
    };
    let mut config = Config {
        folders: vec![documents.clone(), pictures.clone(), empty.clone()],
        targets: vec![target.clone()],
        settings: Settings::default(),
    };
    config
        .settings
        .folder_home_paths
        .insert(documents, PathBuf::from("Arbeit/Dokumente"));
    config
        .settings
        .folder_home_paths
        .insert(pictures, PathBuf::from("Bilder"));
    config
        .settings
        .folder_home_paths
        .insert(empty, PathBuf::from("Leer"));
    let mut events = Vec::new();
    let report = sync_with_progress(&config, &temp.path().join("old.db"), |event| {
        events.push(event)
    })
    .unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.uploaded, 2);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Progress::Uploading { .. }))
            .count(),
        2
    );
    let inventory = discover_backup(&target).unwrap();
    assert!(inventory.has_catalog);
    assert_eq!(inventory.files.len(), 2);
    assert_eq!(inventory.folders.len(), 3);
    assert_eq!(list_backups(&target).unwrap().len(), 2);
    let catalog = fs::read_to_string(temp.path().join("backup/.cloudsave/folders.toml")).unwrap();
    assert!(!catalog.contains(old.to_str().unwrap()));
    let new_home = temp.path().join("new-home");
    fs::create_dir(&new_home).unwrap();
    let locations: Vec<_> = inventory
        .folders
        .iter()
        .map(|folder| RestoreLocation {
            namespace: folder.namespace.clone(),
            destination: prepare_restore_location(
                &target,
                &new_home,
                folder.home_relative.as_deref().unwrap(),
            )
            .unwrap(),
        })
        .collect();
    let mut downloads = Vec::new();
    let report = restore_mapped(&target, &inventory.files, &locations, |progress| {
        downloads.push(progress)
    })
    .unwrap();
    assert_eq!(report.restored, 2);
    assert_eq!(report.skipped, 0);
    assert!(report.errors.is_empty());
    assert_eq!(
        downloads
            .iter()
            .filter(|event| matches!(event, Progress::Downloading { .. }))
            .count(),
        2
    );
    assert!(new_home.join("Leer").is_dir());
    assert_eq!(
        fs::read_to_string(new_home.join("Arbeit/Dokumente/nested/ümlaut.txt")).unwrap(),
        "Unicode ✓"
    );
    let mut fresh = Config {
        folders: locations
            .iter()
            .map(|location| location.destination.clone())
            .collect(),
        targets: vec![target.clone()],
        settings: Settings::default(),
    };
    for location in &locations {
        fresh
            .settings
            .folder_names
            .insert(location.destination.clone(), location.namespace.clone());
    }
    cloudsave_core::adopt_restored(&target, &report, &locations, &temp.path().join("new.db"))
        .unwrap();
    let report = sync(&fresh, &temp.path().join("new.db")).unwrap();
    assert_eq!(report.uploaded, 0);
    assert_eq!(report.skipped, 2);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(!temp.path().join("backup/Arbeit").exists());
    assert_eq!(list_backups(&target).unwrap().len(), 2);
    let mut events = Vec::new();
    sync_with_progress(&fresh, &temp.path().join("new.db"), |event| {
        events.push(event)
    })
    .unwrap();
    assert!(!events
        .iter()
        .any(|event| matches!(event, Progress::Uploading { .. })));
    fresh.folders.retain(|path| !path.ends_with("Leer"));
    assert!(sync(&fresh, &temp.path().join("new.db"))
        .unwrap()
        .errors
        .is_empty());
    assert!(discover_backup(&target)
        .unwrap()
        .folders
        .iter()
        .any(|folder| folder.namespace == "Leer"));
}

#[test]
fn legacy_discovery_mapped_restore_conflicts_and_restart_guard_preserve_backups() {
    let temp = tempfile::tempdir().unwrap();
    let backup = temp.path().join("backup");
    fs::create_dir_all(backup.join("Bilder")).unwrap();
    fs::write(backup.join("Bilder/a.txt"), "backup a").unwrap();
    fs::write(backup.join("Bilder/b.txt"), "backup b").unwrap();
    let target = Target::Local {
        path: backup.clone(),
    };
    let inventory = discover_backup(&target).unwrap();
    assert!(!inventory.has_catalog);
    assert_eq!(inventory.folders[0].namespace, "Bilder");
    assert!(inventory.folders[0].home_relative.is_none());
    let destination = temp.path().join("Pictures");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("a.txt"), "new local a").unwrap();
    let locations = vec![RestoreLocation {
        namespace: "Bilder".into(),
        destination: destination.clone(),
    }];
    let report = restore_mapped(&target, &inventory.files, &locations, |_| {}).unwrap();
    assert_eq!(
        (report.restored, report.skipped, report.errors.len()),
        (1, 1, 0)
    );
    assert_eq!(
        fs::read_to_string(destination.join("a.txt")).unwrap(),
        "new local a"
    );
    let mut config = Config {
        folders: vec![destination.clone()],
        targets: vec![target.clone()],
        settings: Settings::default(),
    };
    config
        .settings
        .folder_names
        .insert(destination.clone(), "Bilder".into());
    assert!(sync(&config, &temp.path().join("state.db"))
        .unwrap()
        .errors
        .is_empty());
    config.settings.sync_mode = SyncMode::Mirror;
    config.settings.pending_recovery = Some(target.clone());
    config.save(&temp.path().join("config.toml")).unwrap();
    fs::remove_file(destination.join("b.txt")).unwrap();
    let reloaded = Config::load(&temp.path().join("config.toml")).unwrap();
    assert!(sync(&reloaded, &temp.path().join("state.db")).is_err());
    assert!(backup.join("Bilder/b.txt").exists());
    assert!(!backup.join("Pictures").exists());
    assert!(restore_mapped(
        &target,
        &inventory.files,
        &[RestoreLocation {
            namespace: "Bilder".into(),
            destination: backup.join("Bilder")
        }],
        |_| {}
    )
    .is_err());
    assert!(
        prepare_restore_location(&target, &temp.path().join("backup"), Path::new("nested"))
            .is_err()
    );
    assert!(!backup.join("nested").exists());
    assert!(restore_mapped(
        &target,
        &inventory.files,
        &[
            RestoreLocation {
                namespace: "Bilder".into(),
                destination: destination.clone()
            },
            RestoreLocation {
                namespace: "Other".into(),
                destination: destination.clone()
            },
        ],
        |_| {}
    )
    .is_err());
}

#[test]
fn recovery_rejects_untrusted_catalogs_and_symlinked_output_directories() {
    let temp = tempfile::tempdir().unwrap();
    let backup = temp.path().join("backup");
    fs::create_dir_all(backup.join(".cloudsave")).unwrap();
    let target = Target::Local {
        path: backup.clone(),
    };
    for catalog in [
        "version=1\n[[folders]]\nnamespace='Bilder'\nhome_relative='../escape'\n",
        "version=1\n[[folders]]\nnamespace='Bilder'\nhome_relative='/absolute'\n",
        "version=1\n[[folders]]\nnamespace='../Bilder'\n",
        "version=1\n[[folders]]\nnamespace='Bilder/'\n",
        "version=1\n[[folders]]\nnamespace='.cloudsave'\n",
        "version=2\nfolders=[]\n",
        "version=1\n[[folders]]\nnamespace='Bilder'\n[[folders]]\nnamespace='Bilder'\n",
    ] {
        fs::write(backup.join(".cloudsave/folders.toml"), catalog).unwrap();
        assert!(discover_backup(&target).is_err(), "{catalog}");
    }
    fs::write(
        backup.join(".cloudsave/folders.toml"),
        vec![b'a'; 1024 * 1024 + 1],
    )
    .unwrap();
    assert!(discover_backup(&target).is_err());
    let home = temp.path().join("home");
    fs::create_dir(&home).unwrap();
    assert!(prepare_restore_location(&target, &home, Path::new("../escape")).is_err());
    assert!(!temp.path().join("escape").exists());
    #[cfg(unix)]
    {
        let outside = temp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, home.join("Bilder")).unwrap();
        assert!(prepare_restore_location(&target, &home, Path::new("Bilder/nested")).is_err());
        assert!(!outside.join("nested").exists());
    }
}
