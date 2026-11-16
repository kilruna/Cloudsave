use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread;

use cloudsave_core::{backend_for, sync, Config, Settings, Target};

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn handle_conn(mut stream: TcpStream, srv_root: &Path) {
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
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
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
    let fs_path = srv_root.join(rel);

    let (status, reason, extra) = match method.as_str() {
        "PROPFIND" => (
            207,
            "Multi-Status",
            Some("Content-Type: application/xml\r\n".to_string()),
        ),
        "MKCOL" => {
            let _ = fs::create_dir_all(&fs_path);
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
            let _ = fs::remove_file(&fs_path);
            (204, "No Content", None)
        }
        _ => (405, "Method Not Allowed", None),
    };
    let body_out = if method == "PROPFIND" {
        r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:"></D:multistatus>"#.to_string()
    } else {
        String::new()
    };
    let resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
        status,
        reason,
        body_out.len(),
        extra.unwrap_or_default(),
        body_out
    );
    let _ = stream.write_all(resp.as_bytes());
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
        settings: Settings::default(),
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
