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

/// Syntax validation for the HTTPS endpoint offered by the setup UI; no network access.
pub fn validate_webdav_url(value: &str) -> Result<()> {
    let value = value.trim();
    let url = reqwest::Url::parse(value).map_err(|_| {
        Error("Bitte eine gültige HTTPS-URL eingeben, z. B. https://cloud.example.com".into())
    })?;
    if url.scheme() != "https" || url.host_str().is_none() {
        return Err(Error(
            "WebDAV benötigt eine HTTPS-URL mit Servernamen.".into(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error(
            "Server-URL ohne Zugangsdaten, Abfrage oder Fragment eingeben.".into(),
        ));
    }
    Ok(())
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
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|error| io::Error::other(error.to_string()))?;
        let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let temporary = rel.with_file_name(format!(".cloudsave-upload-{token}.part"));
        let url = self.url_for(&temporary);
        let body = reqwest::blocking::Body::sized(file, size);
        let resp = self
            .request(reqwest::Method::PUT, &url)
            .header(reqwest::header::IF_NONE_MATCH, "*")
            .body(body)
            .send()
            .map_err(|e| io::Error::other(e.to_string()))?;
        let st = resp.status();
        if !st.is_success() {
            // In particular, 412 means the path belongs to somebody else: never delete it.
            return Err(io::Error::other(format!(
                "Temporärer PUT -> HTTP {st}; bestehende Sicherung unverändert"
            )));
        }
        let method = reqwest::Method::from_bytes(b"MOVE").map_err(io::Error::other)?;
        let result = self
            .request(method, &url)
            .header("Destination", self.url_for(rel))
            .header("Overwrite", "T")
            .send()
            .map_err(|error| io::Error::other(error.to_string()))
            .and_then(|response| {
                if response.status().is_success() {
                    Ok(size)
                } else {
                    Err(io::Error::other(format!(
                        "MOVE -> HTTP {}; bestehende Sicherung nicht ersetzt",
                        response.status()
                    )))
                }
            });
        if result.is_err() {
            // Only clean up the random file whose exclusive PUT completed successfully.
            let _ = self.delete(&temporary);
        }
        result
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

#[cfg(test)]
mod validation_tests {
    use super::validate_webdav_url;

    #[test]
    fn setup_accepts_only_valid_https_endpoints() {
        for value in [
            "",
            "  ",
            "not a url",
            "https://",
            "http://example.com",
            "ftp://example.com",
            "file:///tmp",
            "https://user:password@example.com",
            "https://example.com/#fragment",
        ] {
            assert!(validate_webdav_url(value).is_err(), "{value:?}");
        }
        for value in [
            "https://example.com",
            " https://example.com/dav/ ",
            "https://example.com:8443/dav",
            "https://[::1]/dav",
        ] {
            assert!(validate_webdav_url(value).is_ok(), "{value:?}");
        }
    }
}
