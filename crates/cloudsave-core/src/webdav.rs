use crate::BackupFile;
use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::path::PathBuf;
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

/// Complete a scheme-less setup address with HTTPS and validate it without network access.
pub fn normalize_webdav_url(value: &str) -> Result<String> {
    let value = value.trim();
    if value.starts_with('/') || value.contains('\\') {
        return Err(Error("Bitte eine gültige Server-URL eingeben.".into()));
    }
    let url = if value.contains("://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    validate_webdav_url(&url)?;
    Ok(url)
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
        let parsed = reqwest::Url::parse(url.trim())
            .map_err(|error| Error(format!("Ungültige Server-URL: {error}")))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(Error("WebDAV benötigt HTTP(S)".into()));
        }
        let base = parsed.as_str().trim_end_matches('/').to_string();
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
    fn list(&self) -> io::Result<Vec<BackupFile>> {
        let root = reqwest::Url::parse(&format!("{}/", self.base)).map_err(io::Error::other)?;
        let mut queue = VecDeque::from([root.clone()]);
        let mut visited = HashSet::new();
        let mut files = Vec::new();
        while let Some(directory) = queue.pop_front() {
            if !visited.insert(directory.to_string()) {
                continue;
            }
            if visited.len() + files.len() > 100_000 {
                return Err(io::Error::other("Zu viele Sicherungseinträge; bitte einen kleineren WebDAV-Unterordner als Ziel wählen"));
            }
            let method = reqwest::Method::from_bytes(b"PROPFIND").map_err(io::Error::other)?;
            let response=self.request(method,directory.as_str()).header("Depth","1").header("Content-Type","application/xml")
                .body(r#"<?xml version="1.0"?><d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/><d:getcontentlength/></d:prop></d:propfind>"#)
                .send().map_err(io::Error::other)?;
            if response.status().as_u16() != 207 {
                return Err(io::Error::other(format!(
                    "Sicherungen lesen: HTTP {}",
                    response.status()
                )));
            }
            let mut xml = String::new();
            response
                .take(16 * 1024 * 1024 + 1)
                .read_to_string(&mut xml)?;
            if xml.len() > 16 * 1024 * 1024 {
                return Err(io::Error::other("WebDAV-Verzeichnisantwort zu groß"));
            }
            let (entries, collections) = parse_listing(&root, &directory, &xml)?;
            files.extend(entries);
            queue.extend(collections);
        }
        Ok(files)
    }
    fn download(&self, rel: &Path, writer: &mut dyn Write) -> io::Result<u64> {
        crate::restore::validate_relative(rel)?;
        let mut response = self
            .request(reqwest::Method::GET, &self.url_for(rel))
            .send()
            .map_err(io::Error::other)?;
        if !response.status().is_success() {
            return Err(io::Error::other(format!(
                "Sicherung herunterladen: HTTP {}",
                response.status()
            )));
        }
        io::copy(&mut response, writer)
    }

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

// Namespace prefixes are arbitrary in DAV XML; compare expanded namespace/local names.
fn parse_listing(
    root: &reqwest::Url,
    directory: &reqwest::Url,
    xml: &str,
) -> io::Result<(Vec<BackupFile>, Vec<reqwest::Url>)> {
    let document = roxmltree::Document::parse(xml).map_err(io::Error::other)?;
    if !document
        .root_element()
        .has_tag_name(("DAV:", "multistatus"))
    {
        return Err(io::Error::other("Ungültige WebDAV-Verzeichnisantwort"));
    }
    let mut files = Vec::new();
    let mut collections = Vec::new();
    let base = percent_encoding::percent_decode_str(root.path())
        .decode_utf8()
        .map_err(io::Error::other)?;
    for response in document
        .descendants()
        .filter(|node| node.has_tag_name(("DAV:", "response")))
    {
        let href = response
            .children()
            .find(|node| node.has_tag_name(("DAV:", "href")))
            .and_then(|node| node.text())
            .ok_or_else(|| io::Error::other("WebDAV-Antwort ohne Pfad"))?;
        let url = directory.join(href).map_err(io::Error::other)?;
        if url.origin() != root.origin()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(io::Error::other(
                "WebDAV liefert einen fremden oder unsicheren Sicherungspfad",
            ));
        }
        let decoded = percent_encoding::percent_decode_str(url.path())
            .decode_utf8()
            .map_err(io::Error::other)?;
        let relative = if decoded.trim_end_matches('/') == base.trim_end_matches('/') {
            ""
        } else {
            decoded.strip_prefix(base.as_ref()).ok_or_else(|| {
                io::Error::other("WebDAV-Pfad liegt außerhalb des Sicherungsziels")
            })?
        };
        let is_root = relative.trim_end_matches('/').is_empty();
        let relative = PathBuf::from(relative.trim_end_matches('/'));
        if !is_root {
            crate::restore::validate_relative(&relative)?;
        }
        let propstat = response
            .children()
            .find(|node| {
                node.has_tag_name(("DAV:", "propstat"))
                    && node
                        .children()
                        .find(|child| child.has_tag_name(("DAV:", "status")))
                        .and_then(|node| node.text())
                        .is_some_and(|status| {
                            status
                                .split_whitespace()
                                .nth(1)
                                .is_some_and(|code| code.starts_with('2'))
                        })
            })
            .ok_or_else(|| {
                io::Error::other(format!(
                    "WebDAV-Eintrag nicht lesbar: {}",
                    relative.display()
                ))
            })?;
        if is_root {
            continue;
        }
        let property = propstat
            .children()
            .find(|node| node.has_tag_name(("DAV:", "prop")))
            .ok_or_else(|| io::Error::other("WebDAV-Eigenschaften fehlen"))?;
        if property
            .descendants()
            .any(|node| node.has_tag_name(("DAV:", "collection")))
        {
            let mut url = url;
            if !url.path().ends_with('/') {
                url.set_path(&format!("{}/", url.path()));
            }
            collections.push(url);
        } else {
            let name = relative.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with(".cloudsave-upload-") && name.ends_with(".part") {
                continue;
            }
            let size = property
                .children()
                .find(|node| node.has_tag_name(("DAV:", "getcontentlength")))
                .and_then(|node| node.text())
                .map(|value| value.trim().parse::<u64>())
                .transpose()
                .map_err(io::Error::other)?;
            files.push(BackupFile {
                path: relative,
                size,
            });
        }
    }
    Ok((files, collections))
}

#[cfg(test)]
mod validation_tests {
    use super::{normalize_webdav_url, validate_webdav_url};

    #[test]
    fn setup_completes_missing_https_and_preserves_server_port_and_path() {
        for (value, expected) in [
            ("cloud.example.com", "https://cloud.example.com"),
            (
                " u123.your-storagebox.de ",
                "https://u123.your-storagebox.de",
            ),
            (
                "cloud.example.com:8443/remote.php/dav/files/user/",
                "https://cloud.example.com:8443/remote.php/dav/files/user/",
            ),
            ("[::1]:8443/dav", "https://[::1]:8443/dav"),
            (
                " https://cloud.example.com/dav/ ",
                "https://cloud.example.com/dav/",
            ),
        ] {
            assert_eq!(normalize_webdav_url(value).unwrap(), expected);
        }
        for value in [
            "",
            "  ",
            "not a url",
            "https://",
            "http://example.com",
            "ftp://example.com",
            "file:///tmp",
            "https://user:password@example.com",
            "user:password@example.com",
            "example.com?query=1",
            "example.com/#fragment",
            "/example.com",
            "//example.com",
            r"https:\example.com",
            "example.com:invalid",
        ] {
            assert!(normalize_webdav_url(value).is_err(), "{value:?}");
        }
    }

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

#[cfg(test)]
mod listing_tests {
    use super::*;
    fn xml(href: &str, status: &str) -> String {
        format!(
            r#"<x:multistatus xmlns:x="DAV:"><x:response><x:href>{href}</x:href><x:propstat><x:prop><x:resourcetype/><x:getcontentlength>4</x:getcontentlength></x:prop><x:status>HTTP/1.1 {status}</x:status></x:propstat></x:response></x:multistatus>"#
        )
    }
    #[test]
    fn listing_rejects_foreign_traversal_and_forbidden_responses() {
        let root = reqwest::Url::parse("https://example.com/backup/").unwrap();
        for href in [
            "https://other.invalid/backup/file",
            "/outside/file",
            "/backup/%2e%2e/outside",
            "/backup/sub%5c..%5coutside",
            "/backup/%2F..%2Foutside",
        ] {
            assert!(
                parse_listing(&root, &root, &xml(href, "200 OK")).is_err(),
                "{href}"
            );
        }
        assert!(parse_listing(&root, &root, &xml("/backup/", "403 Forbidden")).is_err());
        assert!(parse_listing(&root, &root, "<html>not DAV</html>").is_err());
        let (files, _) = parse_listing(&root, &root, &xml("/backup/%C3%BC.txt", "200 OK")).unwrap();
        assert_eq!(files[0].path, Path::new("ü.txt"));
    }
}
