use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;

use cloudsave_core::{backend_for, sync, Config, Settings, SyncMode, Target};

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn handle_conn(stream: TcpStream, srv_root: &Path) {
    handle_conn_mode(stream, srv_root, |_, _| None);
}

#[derive(PartialEq, Eq)]
enum Fault {
    HttpError,
    TruncatedBody,
}

fn handle_conn_mode(
    mut stream: TcpStream,
    srv_root: &Path,
    fail: impl FnOnce(&str, &str) -> Option<Fault>,
) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > 1_000_000 {
            return;
        }
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut content_length = 0usize;
    let mut destination = String::new();
    let mut exclusive = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("destination") {
                destination = v.trim().to_string();
            }
            if k.trim().eq_ignore_ascii_case("if-none-match") {
                exclusive = v.trim() == "*";
            }
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let n = stream.read(&mut tmp).unwrap_or(0);
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);

    let rel = path.trim_start_matches('/');
    let rel = percent_encoding::percent_decode_str(rel)
        .decode_utf8()
        .unwrap();
    let fs_path = srv_root.join(rel.as_ref());

    let fault = fail(&method, rel.as_ref());
    let (status, reason, extra) = if fault == Some(Fault::HttpError) {
        (500, "Injected failure", None)
    } else {
        match method.as_str() {
            "GET" if fs_path.is_file() => (200, "OK", None),
            "GET" => (404, "Not Found", None),
            "PROPFIND" if !fs_path.is_dir() => (404, "Not Found", None),
            "PROPFIND" => (
                207,
                "Multi-Status",
                Some("Content-Type: application/xml\r\n".to_string()),
            ),
            "MKCOL" => {
                let _ = fs::create_dir_all(&fs_path);
                (201, "Created", None)
            }
            "PUT" if exclusive && fs_path.exists() => (412, "Precondition Failed", None),
            "MOVE" => {
                let destination = reqwest::Url::parse(&destination).unwrap();
                let decoded = percent_encoding::percent_decode_str(destination.path())
                    .decode_utf8()
                    .unwrap();
                let dest = srv_root.join(decoded.trim_start_matches('/'));
                fs::rename(&fs_path, &dest).unwrap();
                (201, "Created", None)
            }
            "PUT" => {
                if let Some(p) = fs_path.parent() {
                    let _ = fs::create_dir_all(p);
                }
                let _ = fs::write(&fs_path, &body);
                (201, "Created", None)
            }
            "DELETE" => {
                if fs_path.is_dir() {
                    fs::remove_dir_all(&fs_path).unwrap();
                } else {
                    let _ = fs::remove_file(&fs_path);
                }
                (204, "No Content", None)
            }
            _ => (405, "Method Not Allowed", None),
        }
    };
    let body_out = if method == "PROPFIND" && fs_path.is_dir() {
        let mut entries = vec![fs_path.clone()];
        entries.extend(
            fs::read_dir(&fs_path)
                .unwrap()
                .map(|entry| entry.unwrap().path()),
        );
        let mut xml = String::from(r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:">"#);
        for entry in entries {
            let relative = entry.strip_prefix(srv_root).unwrap().to_string_lossy();
            let mut url = reqwest::Url::parse("http://localhost/").unwrap();
            url.set_path(&format!(
                "/{relative}{}",
                if entry.is_dir() { "/" } else { "" }
            ));
            let href = url.path().replace('&', "&amp;");
            let resource = if entry.is_dir() {
                "<D:collection/>"
            } else {
                ""
            };
            xml.push_str(&format!("<D:response><D:href>{href}</D:href><D:propstat><D:prop><D:resourcetype>{resource}</D:resourcetype><D:getcontentlength>{}</D:getcontentlength></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>",entry.metadata().unwrap().len()));
        }
        xml.push_str("</D:multistatus>");
        xml.into_bytes()
    } else if method == "GET" && fs_path.is_file() {
        fs::read(&fs_path).unwrap()
    } else {
        Vec::new()
    };
    let resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
        status,
        reason,
        body_out.len(),
        extra.unwrap_or_default()
    );
    let _ = stream.write_all(resp.as_bytes());
    let body = if fault == Some(Fault::TruncatedBody) {
        &body_out[..body_out.len() / 2]
    } else {
        &body_out
    };
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

#[test]
fn webdav_backend_roundtrip_via_mock_server() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("quelle");
    let srv = tmp.path().join("srv");
    let state_path = tmp.path().join("state.db");
    fs::create_dir_all(src.join("unter")).unwrap();
    fs::write(src.join("a.txt"), b"hallo webdav").unwrap();
    fs::write(src.join("unter/b.txt"), b"zwei").unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let srv2 = srv.clone();
    thread::spawn(move || {
        for s in listener.incoming().flatten() {
            let srv3 = srv2.clone();
            thread::spawn(move || handle_conn(s, &srv3));
        }
    });

    std::env::set_var("CLOUDSAVE_WEBDAV_PASSWORD", "pw");
    let cfg = Config {
        folders: vec![src.clone()],
        targets: vec![Target::Webdav {
            url: format!("http://127.0.0.1:{}/base", port),
            user: "u1".to_string(),
        }],
        settings: Settings {
            sync_mode: SyncMode::Mirror,
            ..Settings::default()
        },
    };

    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.uploaded, 2, "errors: {:?}", r.errors);
    assert_eq!(
        fs::read(srv.join("base/quelle/a.txt")).unwrap(),
        b"hallo webdav"
    );
    assert_eq!(
        fs::read(srv.join("base/quelle/unter/b.txt")).unwrap(),
        b"zwei"
    );

    fs::write(src.join("a.txt"), b"hallo webdav neu").unwrap();
    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.uploaded, 1);
    assert_eq!(r.skipped, 1);
    assert_eq!(
        fs::read(srv.join("base/quelle/a.txt")).unwrap(),
        b"hallo webdav neu"
    );

    fs::remove_file(src.join("unter/b.txt")).unwrap();
    let r = sync(&cfg, &state_path).unwrap();
    assert_eq!(r.deleted, 1);
    assert!(!srv.join("base/quelle/unter/b.txt").exists());

    let backend = backend_for(&cfg.targets[0]).unwrap();
    backend.test_connection().unwrap();
}

#[test]
fn authentication_failure_is_reported_with_target_and_source() {
    std::env::set_var("CLOUDSAVE_WEBDAV_PASSWORD", "pw");
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    fs::create_dir(&src).unwrap();
    fs::write(src.join("save.txt"), b"synthetic data").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/dav", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 8192];
        assert!(stream.read(&mut request).unwrap() > 0);
        stream
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    let cfg = Config {
        folders: vec![src.clone()],
        targets: vec![Target::Webdav {
            url: url.clone(),
            user: "qa-user".into(),
        }],
        settings: Settings {
            sync_mode: SyncMode::Mirror,
            ..Settings::default()
        },
    };
    let report = sync(&cfg, &tmp.path().join("state.db")).unwrap();
    server.join().unwrap();
    assert_eq!(report.uploaded, 0);
    assert!(report
        .errors
        .iter()
        .any(|e| e.contains("401") && e.contains(&url) && e.contains(&src.display().to_string())));
}

#[test]
fn failed_temporary_put_or_move_preserves_previous_webdav_backup() {
    use cloudsave_core::{Backend, WebdavBackend};
    for failed_method in ["PUT", "MOVE"] {
        let tmp = tempfile::tempdir().unwrap();
        let srv = tmp.path().join("server");
        fs::create_dir_all(srv.join("base/Documents")).unwrap();
        fs::write(
            srv.join("base/Documents/save.txt"),
            b"valid previous backup",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/base", listener.local_addr().unwrap());
        let server_root = srv.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                handle_conn_mode(stream, &server_root, |method, _| {
                    (method == failed_method).then_some(Fault::HttpError)
                });
            }
        });
        let source = tmp.path().join("source");
        fs::write(&source, b"replacement").unwrap();
        let backend = WebdavBackend::new(&url, "qa", "synthetic").unwrap();
        assert!(backend
            .put(
                Path::new("Documents/save.txt"),
                fs::File::open(&source).unwrap(),
                11
            )
            .is_err());
        assert_eq!(
            fs::read(srv.join("base/Documents/save.txt")).unwrap(),
            b"valid previous backup"
        );
        assert_eq!(fs::read_dir(srv.join("base/Documents")).unwrap().count(), 1);
    }
}

#[test]
fn webdav_backups_can_be_browsed_and_restored_with_unicode_paths() {
    use cloudsave_core::{list_backups, restore_files};
    std::env::set_var("CLOUDSAVE_WEBDAV_PASSWORD", "pw");
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("Documents");
    let srv = tmp.path().join("server");
    fs::create_dir_all(src.join("nested")).unwrap();
    fs::write(
        src.join("nested/ümlaut & spaces.txt"),
        b"roundtrip bytes\0\xff",
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let target = Target::Webdav {
        url: format!("http://{}/base", listener.local_addr().unwrap()),
        user: "qa".into(),
    };
    let server_root = srv.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handle_conn(stream, &server_root);
        }
    });
    let config = Config {
        folders: vec![src],
        targets: vec![target.clone()],
        settings: Settings::default(),
    };
    let report = sync(&config, &tmp.path().join("state.db")).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let files = list_backups(&target).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].path,
        Path::new("Documents/nested/ümlaut & spaces.txt")
    );
    let inventory = cloudsave_core::discover_backup(&target).unwrap();
    assert!(inventory.has_catalog);
    assert_eq!(inventory.folders[0].namespace, "Documents");
    let mapped = tmp.path().join("mapped");
    fs::create_dir(&mapped).unwrap();
    let locations = vec![cloudsave_core::RestoreLocation {
        namespace: "Documents".into(),
        destination: mapped.clone(),
    }];
    let mapped_report =
        cloudsave_core::restore_mapped(&target, &inventory.files, &locations, |_| {}).unwrap();
    assert_eq!(mapped_report.restored, 1);
    assert!(mapped_report.errors.is_empty());
    let mut recovered = Config {
        folders: vec![mapped.clone()],
        targets: vec![target.clone()],
        settings: Settings::default(),
    };
    recovered
        .settings
        .folder_names
        .insert(mapped.clone(), "Documents".into());
    cloudsave_core::adopt_restored(
        &target,
        &mapped_report,
        &locations,
        &tmp.path().join("new.db"),
    )
    .unwrap();
    let first_sync = sync(&recovered, &tmp.path().join("new.db")).unwrap();
    assert_eq!(first_sync.uploaded, 0);
    assert_eq!(first_sync.skipped, 1);
    assert!(first_sync.errors.is_empty());
    fs::write(
        mapped.join("nested/ümlaut & spaces.txt"),
        b"edited after recovery",
    )
    .unwrap();
    cloudsave_core::adopt_restored(
        &target,
        &mapped_report,
        &locations,
        &tmp.path().join("new.db"),
    )
    .unwrap();
    let changed = sync(&recovered, &tmp.path().join("new.db")).unwrap();
    assert_eq!(changed.uploaded, 1);
    assert!(changed.errors.is_empty());
    assert_eq!(
        fs::read(srv.join("base/Documents/nested/ümlaut & spaces.txt")).unwrap(),
        b"edited after recovery"
    );
    // The earlier inventory becomes stale after an intentional edit; relist before
    // exercising the independent manual-restore route below.
    let files = list_backups(&target).unwrap();
    let output = tmp.path().join("restore");
    fs::create_dir(&output).unwrap();
    let report = restore_files(&target, &files, &output).unwrap();
    assert_eq!(report.restored, 1, "{:?}", report.errors);
    assert_eq!(
        fs::read(output.join(&files[0].path)).unwrap(),
        b"edited after recovery"
    );
}

#[test]
fn transient_upload_failure_and_truncated_download_retry_the_whole_file() {
    use cloudsave_core::{restore_files, BackupFile};

    std::env::set_var("CLOUDSAVE_WEBDAV_PASSWORD", "pw");
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let remote = temp.path().join("remote");
    let restored = temp.path().join("restored");
    let payload = b"new complete file";
    fs::create_dir_all(remote.join("dav/source")).unwrap();
    fs::create_dir(&source).unwrap();
    fs::create_dir(&restored).unwrap();
    fs::write(source.join("game.dat"), payload).unwrap();
    fs::write(remote.join("dav/source/game.dat"), b"previous backup").unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/dav", listener.local_addr().unwrap());
    let uploads = Arc::new(AtomicUsize::new(0));
    let downloads = Arc::new(AtomicUsize::new(0));
    let server_uploads = uploads.clone();
    let server_downloads = downloads.clone();
    let server_root = remote.clone();
    thread::spawn(move || {
        for connection in listener.incoming().flatten() {
            handle_conn_mode(connection, &server_root, |method, path| {
                if method == "PUT"
                    && path.starts_with("dav/source/")
                    && server_uploads.fetch_add(1, Ordering::Relaxed) == 0
                {
                    assert_eq!(
                        fs::read(server_root.join("dav/source/game.dat")).unwrap(),
                        b"previous backup"
                    );
                    return Some(Fault::HttpError);
                }
                if method == "GET"
                    && path == "dav/source/game.dat"
                    && server_downloads.fetch_add(1, Ordering::Relaxed) == 0
                {
                    return Some(Fault::TruncatedBody);
                }
                None
            });
        }
    });
    let target = Target::Webdav {
        url,
        user: "qa-user".into(),
    };
    let config = Config {
        folders: vec![source],
        targets: vec![target.clone()],
        settings: Settings::default(),
    };

    let report = sync(&config, &temp.path().join("state.db")).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.uploaded, 1);
    assert_eq!(uploads.load(Ordering::Relaxed), 2);
    assert_eq!(
        fs::read(remote.join("dav/source/game.dat")).unwrap(),
        payload
    );

    let files = [BackupFile {
        path: "source/game.dat".into(),
        size: Some(payload.len() as u64),
    }];
    let report = restore_files(&target, &files, &restored).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.restored, 1);
    assert_eq!(downloads.load(Ordering::Relaxed), 2);
    assert_eq!(fs::read(restored.join("source/game.dat")).unwrap(), payload);
    assert_eq!(fs::read_dir(restored.join("source")).unwrap().count(), 1);
}

#[test]
fn explicit_live_removal_deletes_the_webdav_collection_only() {
    std::env::set_var("CLOUDSAVE_WEBDAV_PASSWORD", "pw");
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("Quelle");
    let remote = temp.path().join("remote");
    let state = temp.path().join("state.db");
    fs::create_dir_all(source.join("Unter")).unwrap();
    fs::write(source.join("Unter/test.txt"), b"owned backup").unwrap();
    fs::create_dir_all(remote.join("base/Andere")).unwrap();
    fs::write(remote.join("base/Andere/keep"), b"other source").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/base", listener.local_addr().unwrap());
    let server_root = remote.clone();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            handle_conn(stream, &server_root);
        }
    });
    let mut config = Config {
        folders: vec![source.clone()],
        targets: vec![Target::Webdav {
            url,
            user: "qa".into(),
        }],
        settings: Settings::default(),
    };
    assert!(sync(&config, &state).unwrap().errors.is_empty());
    config.remove_folder(&source).unwrap();
    let report = sync(&config, &state).unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.deleted, 1);
    assert_eq!(report.removed_folders.len(), 1);
    assert!(!remote.join("base/Quelle").exists());
    assert_eq!(
        fs::read(remote.join("base/Andere/keep")).unwrap(),
        b"other source"
    );
    assert!(source.join("Unter/test.txt").exists());
}
