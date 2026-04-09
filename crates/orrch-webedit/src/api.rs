//! JSON API handlers for the web node editor.
//!
//! These functions return fully-constructed [`tiny_http::Response`] objects
//! ready to hand back to the client. Every handler is infallible from the
//! caller's perspective — errors are converted into 4xx/5xx JSON responses
//! so the server loop stays simple.

use std::fs;
use std::path::{Path, PathBuf};

use orrch_workforce::{load_workforces, parse_workforce_markdown, serialize_workforce_markdown, Workforce};
use serde::Serialize;
use tiny_http::{Header, Response};

type HttpResponse = Response<std::io::Cursor<Vec<u8>>>;

/// A compact summary of a workforce, used by the `/api/workforces` list.
#[derive(Debug, Serialize)]
pub struct WorkforceSummary {
    pub name: String,
    pub description: String,
    pub agent_count: usize,
    pub connection_count: usize,
}

impl WorkforceSummary {
    fn from(wf: &Workforce) -> Self {
        Self {
            name: wf.name.clone(),
            description: wf.description.clone(),
            agent_count: wf.agents.len(),
            connection_count: wf.connections.len(),
        }
    }
}

/// `GET /api/workforces` — return a JSON array of all workforce summaries
/// found in `dir`. Non-markdown files and files that fail to parse are
/// silently skipped (matching the behaviour of
/// `orrch_workforce::load_workforces`).
pub fn list_workforces(dir: &Path) -> HttpResponse {
    let workforces = load_workforces(dir);
    let summaries: Vec<WorkforceSummary> =
        workforces.iter().map(WorkforceSummary::from).collect();
    json_response(&summaries, 200)
}

/// `GET /api/workforce/:name` — return the full Workforce struct as JSON.
/// Returns 404 if no workforce with that name exists.
pub fn get_workforce(dir: &Path, name: &str) -> HttpResponse {
    let name = percent_decode(name);
    let workforces = load_workforces(dir);
    match workforces.into_iter().find(|w| w.name == name) {
        Some(wf) => json_response(&wf, 200),
        None => json_error(&format!("workforce not found: {name}"), 404),
    }
}

/// `POST /api/workforce/:name` — accept a JSON Workforce in the request body,
/// serialize it to markdown, and write it to disk.
///
/// The on-disk filename is derived from the workforce name by lowercasing
/// and replacing spaces with underscores. This mirrors the naming
/// convention used in `workforces/*.md` today (e.g. "General Software
/// Development" → `general_software_development.md`).
pub fn put_workforce(dir: &Path, name: &str, body: &str) -> HttpResponse {
    let _url_name = percent_decode(name); // currently advisory — the body carries the canonical name

    let wf: Workforce = match serde_json::from_str(body) {
        Ok(wf) => wf,
        Err(e) => return json_error(&format!("invalid workforce json: {e}"), 400),
    };

    let md = serialize_workforce_markdown(&wf);
    let filename = filename_for(&wf.name);
    let path: PathBuf = dir.join(filename);

    if let Err(e) = fs::create_dir_all(dir) {
        return json_error(&format!("mkdir failed: {e}"), 500);
    }
    if let Err(e) = fs::write(&path, md) {
        return json_error(&format!("write failed: {e}"), 500);
    }

    // Round-trip sanity check so we never quietly corrupt a file.
    if let Ok(written) = fs::read_to_string(&path) {
        if parse_workforce_markdown(&written).is_none() {
            return json_error("serialized workforce failed round-trip parse", 500);
        }
    }

    json_response(
        &serde_json::json!({ "ok": true, "path": path.display().to_string() }),
        200,
    )
}

/// Derive an on-disk filename from a workforce display name.
fn filename_for(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| match c {
            'A'..='Z' => c.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' => c,
            _ => '_',
        })
        .collect();
    // Collapse runs of underscores and trim leading/trailing.
    let mut out = String::with_capacity(slug.len() + 3);
    let mut prev_under = false;
    for c in slug.chars() {
        if c == '_' {
            if !prev_under {
                out.push('_');
            }
            prev_under = true;
        } else {
            out.push(c);
            prev_under = false;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    format!("{}.md", if trimmed.is_empty() { "workforce" } else { &trimmed })
}

/// Minimal URL percent-decoder — only handles the characters that show up in
/// workforce names (spaces and the occasional `+`/`%20`). A full decoder
/// would be overkill for a localhost dev tool.
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = hex_val(bytes[i + 1]);
                let lo = hex_val(bytes[i + 2]);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as char);
                    i += 3;
                    continue;
                }
                out.push('%');
                i += 1;
            }
            b'+' => {
                out.push(' ');
                i += 1;
            }
            other => {
                out.push(other as char);
                i += 1;
            }
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn json_response<T: Serialize>(value: &T, status: u16) -> HttpResponse {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let mut resp = Response::from_string(body).with_status_code(status);
    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]) {
        resp = resp.with_header(h);
    }
    resp
}

fn json_error(msg: &str, status: u16) -> HttpResponse {
    let body = serde_json::json!({ "error": msg });
    json_response(&body, status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;

    fn fixture_workforce_md() -> &'static str {
        "---\nname: Test Workforce\ndescription: A fixture\noperations:\n  - DEVELOP FEATURE\n---\n\n## Agents\n\n| ID | Agent Profile | User-Facing |\n|----|---------------|-------------|\n| pm | Project Manager | yes |\n| dev | Developer | no |\n\n## Connections\n\n| From | To | Data Type |\n|------|----|-----------|\n| pm | dev | instructions |\n"
    }

    fn read_body(resp: HttpResponse) -> (u16, String) {
        let status = resp.status_code().0;
        let mut reader = resp.into_reader();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        (status, String::from_utf8(buf).unwrap())
    }

    #[test]
    fn list_workforces_reads_fixture_directory() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test_workforce.md"), fixture_workforce_md()).unwrap();

        let resp = list_workforces(dir.path());
        let (status, body) = read_body(resp);
        assert_eq!(status, 200);
        assert!(body.contains("Test Workforce"), "body: {body}");
        assert!(body.contains("agent_count"));
    }

    #[test]
    fn get_workforce_by_name() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test_workforce.md"), fixture_workforce_md()).unwrap();

        let resp = get_workforce(dir.path(), "Test%20Workforce");
        let (status, body) = read_body(resp);
        assert_eq!(status, 200);
        assert!(body.contains("Test Workforce"));
        assert!(body.contains("Project Manager"));
    }

    #[test]
    fn get_workforce_missing_returns_404() {
        let dir = tempdir().unwrap();
        let resp = get_workforce(dir.path(), "Nonexistent");
        let (status, _) = read_body(resp);
        assert_eq!(status, 404);
    }

    #[test]
    fn put_workforce_writes_file_and_round_trips() {
        let dir = tempdir().unwrap();
        // Round-trip: parse a fixture, serialize to JSON, POST it, read the
        // resulting file, parse it back, compare agent count.
        let wf = parse_workforce_markdown(fixture_workforce_md()).unwrap();
        let json = serde_json::to_string(&wf).unwrap();
        let resp = put_workforce(dir.path(), "Test%20Workforce", &json);
        let (status, _) = read_body(resp);
        assert_eq!(status, 200);

        let written = fs::read_to_string(dir.path().join("test_workforce.md")).unwrap();
        let parsed = parse_workforce_markdown(&written).unwrap();
        assert_eq!(parsed.agents.len(), 2);
    }

    #[test]
    fn put_workforce_rejects_invalid_json() {
        let dir = tempdir().unwrap();
        let resp = put_workforce(dir.path(), "foo", "{not json");
        let (status, _) = read_body(resp);
        assert_eq!(status, 400);
    }

    #[test]
    fn filename_for_slugifies_spaces_and_case() {
        assert_eq!(filename_for("General Software Development"), "general_software_development.md");
        assert_eq!(filename_for("Mid-Tier"), "mid_tier.md");
    }
}
