use std::fs;
use std::path::Path;

use cloudsave_core::{sync, Config, Settings, SyncReport, Target};

fn make_config(src: &Path, dst: &Path) -> Config {
    Config {
        folders: vec![src.to_path_buf()],
        target: Target::Local {
            path: dst.to_path_buf(),
        },
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
}

#[test]
fn missing_folder_is_reported_not_fatal() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = make_config(Path::new("/does/not/exist"), &tmp.path().join("ziel"));
    let r = sync(&cfg, &tmp.path().join("state.db")).unwrap();
    assert_eq!(r.uploaded, 0);
    assert_eq!(r.errors.len(), 1);
}
