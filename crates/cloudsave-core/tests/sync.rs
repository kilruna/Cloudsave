use std::fs;
use std::path::Path;

use cloudsave_core::{sync, Config, Settings, State, SyncReport, Target};

fn make_config(src: &Path, dst: &Path) -> Config {
    Config {
        folders: vec![src.to_path_buf()],
        targets: vec![Target::Local {
            path: dst.to_path_buf(),
        }],
        settings: Settings::default(),
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
        settings: Settings::default(),
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
