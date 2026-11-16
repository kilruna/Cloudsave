use std::fs::File;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::backend::Backend;
use crate::{Error, Result};

pub struct WebdavBackend {
    client: reqwest::blocking::Client,
    base: String,
    user: String,
    password: String,
}

fn url_encode_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn url_encode_path(p: &str) -> String {
    p.split('/')
        .map(url_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

impl WebdavBackend {
    pub fn new(url: &str, user: &str, password: &str) -> Result<Self> {
        let base = url.trim_end_matches('/').to_string();
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| Error(e.to_string()))?;
        Ok(Self {
            client,
            base,
            user: user.to_string(),
            password: password.to_string(),
        })
    }

    fn url_for(&self, rel: &Path) -> String {
        let rel = rel.to_string_lossy().replace('\\', "/");
        format!("{}/{}", self.base, url_encode_path(&rel))
    }

    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::blocking::RequestBuilder {
        self.client
            .request(method, url)
            .basic_auth(&self.user, Some(&self.password))
    }

    pub fn test(&self) -> Result<()> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>"#;
        let method = reqwest::Method::from_bytes(b"PROPFIND").map_err(|e| Error(e.to_string()))?;
        let resp = self
            .request(method, &format!("{}/", self.base))
            .header("Depth", "0")
            .header("Content-Type", "application/xml")
            .body(body)
            .send()
            .map_err(|e| Error(format!("Server nicht erreichbar: {}", e)))?;
        let status = resp.status();
        if status.as_u16() == 207 || status.is_success() {
            Ok(())
        } else if status.as_u16() == 401 || status.as_u16() == 403 {
            Err(Error("Anmeldedaten falsch (HTTP 401/403)".to_string()))
        } else {
            Err(Error(format!("Server antwortet mit HTTP {}", status)))
        }
    }
}

impl Backend for WebdavBackend {
    fn test_connection(&self) -> io::Result<()> {
        self.test().map_err(|e| io::Error::other(e.to_string()))
    }

    fn mkdirs(&self, rel: &Path) -> io::Result<()> {
        let comps: Vec<String> = rel
            .to_string_lossy()
            .replace('\\', "/")
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let mut acc = String::new();
        for c in comps.iter().take(comps.len().saturating_sub(1)) {
            acc.push('/');
            acc.push_str(&url_encode_segment(c));
            let url = format!("{}{}", self.base, acc);
            let method = reqwest::Method::from_bytes(b"MKCOL")
                .map_err(|e| io::Error::other(e.to_string()))?;
            let resp = self
                .request(method, &url)
                .send()
                .map_err(|e| io::Error::other(e.to_string()))?;
            let st = resp.status().as_u16();
            if !(200..300).contains(&st) && st != 405 && st != 301 {
                return Err(io::Error::other(format!("MKCOL {} -> HTTP {}", url, st)));
            }
        }
        Ok(())
    }

    fn put(&self, rel: &Path, file: File, size: u64) -> io::Result<u64> {
        let url = self.url_for(rel);
        let body = reqwest::blocking::Body::sized(file, size);
        let resp = self
            .request(reqwest::Method::PUT, &url)
            .body(body)
            .send()
            .map_err(|e| io::Error::other(e.to_string()))?;
        let st = resp.status();
        if st.is_success() || st.as_u16() == 201 || st.as_u16() == 204 {
            Ok(size)
        } else {
            Err(io::Error::other(format!("PUT {} -> HTTP {}", url, st)))
        }
    }

    fn delete(&self, rel: &Path) -> io::Result<()> {
        let url = self.url_for(rel);
        let resp = self
            .request(reqwest::Method::DELETE, &url)
            .send()
            .map_err(|e| io::Error::other(e.to_string()))?;
        let st = resp.status().as_u16();
        if (200..300).contains(&st) || st == 404 {
            Ok(())
        } else {
            Err(io::Error::other(format!("DELETE {} -> HTTP {}", url, st)))
        }
    }
}
