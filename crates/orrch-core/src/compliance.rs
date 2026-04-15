//! License compliance and copyright header scanning.

use std::collections::HashMap;
use std::path::Path;

// ─── License Scanning ─────────────────────────────────────────────────────────

/// A single dependency's license information.
#[derive(Debug, Clone)]
pub struct LicenseDep {
    pub name: String,
    pub version: String,
    pub spdx: String,
    pub status: LicenseStatus,
}

/// Whether the license is permissive, copyleft, or unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LicenseStatus {
    Permissive,
    Copyleft,
    Unknown,
}

impl LicenseStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Permissive => "OK",
            Self::Copyleft => "COPYLEFT",
            Self::Unknown => "?",
        }
    }
}

/// Result of a license scan.
#[derive(Debug, Default, Clone)]
pub struct LicenseReport {
    pub deps: Vec<LicenseDep>,
    pub total: usize,
    pub permissive: usize,
    pub copyleft: usize,
    pub unknown: usize,
}

/// Scan `Cargo.lock` for crate licenses using an embedded SPDX table.
/// No network access — looks up known crates from embedded map.
pub fn scan_licenses(project_dir: &Path) -> LicenseReport {
    let lock_path = project_dir.join("Cargo.lock");
    let contents = match std::fs::read_to_string(&lock_path) {
        Ok(c) => c,
        Err(_) => {
            // Try workspace root (go up until Cargo.lock found or give up)
            let mut dir = project_dir.to_path_buf();
            let mut found = None;
            for _ in 0..5 {
                dir = match dir.parent() {
                    Some(p) => p.to_path_buf(),
                    None => break,
                };
                let candidate = dir.join("Cargo.lock");
                if candidate.exists() {
                    found = std::fs::read_to_string(&candidate).ok();
                    break;
                }
            }
            match found {
                Some(c) => c,
                None => return LicenseReport::default(),
            }
        }
    };

    let table = spdx_table();
    let mut deps = Vec::new();

    // Parse Cargo.lock: look for [[package]] blocks
    let mut name = String::new();
    let mut version = String::new();
    for line in contents.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            if !name.is_empty() {
                let spdx = table.get(name.as_str()).cloned().unwrap_or_else(|| "Unknown".to_string());
                let status = classify_spdx(&spdx);
                deps.push(LicenseDep { name: std::mem::take(&mut name), version: std::mem::take(&mut version), spdx, status });
            }
        } else if let Some(rest) = line.strip_prefix("name = ") {
            name = rest.trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("version = ") {
            version = rest.trim_matches('"').to_string();
        }
    }
    // Final package
    if !name.is_empty() {
        let spdx = table.get(name.as_str()).cloned().unwrap_or_else(|| "Unknown".to_string());
        let status = classify_spdx(&spdx);
        deps.push(LicenseDep { name, version, spdx, status });
    }

    let total = deps.len();
    let permissive = deps.iter().filter(|d| d.status == LicenseStatus::Permissive).count();
    let copyleft = deps.iter().filter(|d| d.status == LicenseStatus::Copyleft).count();
    let unknown = deps.iter().filter(|d| d.status == LicenseStatus::Unknown).count();

    LicenseReport { deps, total, permissive, copyleft, unknown }
}

fn classify_spdx(spdx: &str) -> LicenseStatus {
    if spdx == "Unknown" { return LicenseStatus::Unknown; }
    let s = spdx.to_uppercase();
    if s.contains("GPL") || s.contains("LGPL") || s.contains("AGPL") || s.contains("MPL") || s.contains("EUPL") {
        LicenseStatus::Copyleft
    } else {
        LicenseStatus::Permissive
    }
}

/// Embedded SPDX lookup for common Rust crates (no network).
fn spdx_table() -> HashMap<&'static str, String> {
    let entries: &[(&str, &str)] = &[
        // Rust stdlib / well-known
        ("anyhow", "MIT OR Apache-2.0"),
        ("tokio", "MIT"),
        ("serde", "MIT OR Apache-2.0"),
        ("serde_json", "MIT OR Apache-2.0"),
        ("clap", "MIT OR Apache-2.0"),
        ("log", "MIT OR Apache-2.0"),
        ("env_logger", "MIT OR Apache-2.0"),
        ("regex", "MIT OR Apache-2.0"),
        ("rand", "MIT OR Apache-2.0"),
        ("chrono", "MIT OR Apache-2.0"),
        ("uuid", "MIT OR Apache-2.0"),
        ("thiserror", "MIT OR Apache-2.0"),
        ("tracing", "MIT"),
        ("tracing-subscriber", "MIT"),
        ("once_cell", "MIT OR Apache-2.0"),
        ("parking_lot", "MIT OR Apache-2.0"),
        ("crossbeam-channel", "MIT OR Apache-2.0"),
        ("rayon", "MIT OR Apache-2.0"),
        ("itertools", "MIT OR Apache-2.0"),
        ("indexmap", "MIT OR Apache-2.0"),
        ("hashbrown", "MIT OR Apache-2.0"),
        ("bytes", "MIT"),
        ("futures", "MIT OR Apache-2.0"),
        ("async-trait", "MIT OR Apache-2.0"),
        ("pin-project", "MIT OR Apache-2.0"),
        ("tower", "MIT"),
        ("hyper", "MIT"),
        ("reqwest", "MIT OR Apache-2.0"),
        ("axum", "MIT"),
        ("warp", "MIT"),
        ("actix-web", "MIT OR Apache-2.0"),
        ("ratatui", "MIT"),
        ("crossterm", "MIT"),
        ("tui", "MIT"),
        ("clap_derive", "MIT OR Apache-2.0"),
        ("tempfile", "MIT OR Apache-2.0"),
        ("dirs", "MIT OR Apache-2.0"),
        ("home", "MIT OR Apache-2.0"),
        ("which", "MIT"),
        ("walkdir", "MIT OR Unlicense"),
        ("globset", "MIT OR Unlicense"),
        ("ignore", "MIT OR Unlicense"),
        ("toml", "MIT OR Apache-2.0"),
        ("toml_edit", "MIT OR Apache-2.0"),
        ("semver", "MIT OR Apache-2.0"),
        ("cargo_metadata", "MIT OR Apache-2.0"),
        ("proc-macro2", "MIT OR Apache-2.0"),
        ("quote", "MIT OR Apache-2.0"),
        ("syn", "MIT OR Apache-2.0"),
        ("libc", "MIT OR Apache-2.0"),
        ("nix", "MIT"),
        ("signal-hook", "MIT OR Apache-2.0"),
        ("ctrlc", "MIT OR Apache-2.0"),
        ("memchr", "MIT OR Unlicense"),
        ("unicode-width", "MIT OR Apache-2.0"),
        ("unicode-segmentation", "MIT OR Apache-2.0"),
        ("bitflags", "MIT OR Apache-2.0"),
        ("num-traits", "MIT OR Apache-2.0"),
        ("base64", "MIT OR Apache-2.0"),
        ("hex", "MIT OR Apache-2.0"),
        ("url", "MIT OR Apache-2.0"),
        ("percent-encoding", "MIT OR Apache-2.0"),
        ("mime", "MIT OR Apache-2.0"),
        ("http", "MIT OR Apache-2.0"),
        ("httparse", "MIT OR Apache-2.0"),
        ("h2", "MIT"),
        ("rustls", "Apache-2.0 OR ISC OR MIT"),
        ("openssl", "Apache-2.0"),
        ("native-tls", "MIT OR Apache-2.0"),
        ("rustls-webpki", "ISC"),
        ("webpki-roots", "MPL-2.0"),
        ("ring", "ISC AND MIT AND OpenSSL"),
        ("flate2", "MIT OR Apache-2.0"),
        ("tar", "MIT OR Apache-2.0"),
        ("zip", "MIT"),
        ("zstd", "MIT"),
        ("lz4", "MIT"),
        ("brotli", "MIT"),
        ("encoding_rs", "MIT OR Apache-2.0"),
        ("nom", "MIT"),
        ("pest", "MIT OR Apache-2.0"),
        ("lalrpop", "Apache-2.0 OR MIT"),
        ("rustyline", "MIT"),
        ("indicatif", "MIT"),
        ("console", "MIT"),
        ("colored", "MPL-2.0"),
        ("termcolor", "MIT OR Unlicense"),
        ("atty", "MIT"),
        ("is-terminal", "MIT"),
        ("dialoguer", "MIT"),
        ("comfy-table", "MIT"),
        ("prettytable-rs", "BSD-3-Clause"),
        ("tabled", "MIT"),
        ("csv", "MIT OR Unlicense"),
        ("rusqlite", "MIT"),
        ("diesel", "MIT OR Apache-2.0"),
        ("sqlx", "MIT OR Apache-2.0"),
        ("redis", "MIT"),
        ("mongodb", "Apache-2.0"),
        ("surrealdb", "Apache-2.0"),
        ("rocksdb", "Apache-2.0"),
        ("lmdb", "OpenLDAP"),
        ("sled", "MIT OR Apache-2.0"),
        ("redb", "MIT OR Apache-2.0"),
        ("config", "MIT OR Apache-2.0"),
        ("figment", "MIT OR Apache-2.0"),
        ("dotenv", "MIT OR Apache-2.0"),
        ("dotenvy", "MIT OR Apache-2.0"),
        ("strum", "MIT"),
        ("derive_more", "MIT"),
        ("getset", "MIT"),
        ("bon", "MIT"),
        ("typed-builder", "MIT OR Apache-2.0"),
        ("either", "MIT OR Apache-2.0"),
        ("maplit", "MIT OR Apache-2.0"),
        ("smallvec", "MIT OR Apache-2.0"),
        ("tinyvec", "MIT OR Apache-2.0"),
        ("ahash", "MIT OR Apache-2.0"),
        ("fnv", "MIT OR Apache-2.0"),
        ("crc32fast", "MIT OR Apache-2.0"),
        ("adler", "MIT OR Apache-2.0"),
        ("miniz_oxide", "MIT OR Apache-2.0 OR Zlib"),
        ("object", "MIT OR Apache-2.0"),
        ("gimli", "MIT OR Apache-2.0"),
        ("addr2line", "MIT OR Apache-2.0"),
        ("backtrace", "MIT OR Apache-2.0"),
        ("rustc-demangle", "MIT OR Apache-2.0"),
        ("orrch-core", "proprietary"),
        ("orrch-tui", "proprietary"),
        ("orrch-agents", "proprietary"),
        ("orrch-library", "proprietary"),
        ("orrch-retrospect", "proprietary"),
        ("orrch-webedit", "proprietary"),
        ("orrch-workforce", "proprietary"),
        ("orrchestrator", "proprietary"),
    ];
    entries.iter().map(|(k, v)| (*k, v.to_string())).collect()
}

// ─── Copyright Header Scanning ─────────────────────────────────────────────

/// A file missing a copyright/SPDX header.
#[derive(Debug, Clone)]
pub struct MissingHeader {
    pub path: String,
}

/// Result of a copyright header scan.
#[derive(Debug, Default, Clone)]
pub struct CopyrightReport {
    pub scanned: usize,
    pub with_header: usize,
    pub missing: Vec<MissingHeader>,
}

impl CopyrightReport {
    pub fn coverage_pct(&self) -> f32 {
        if self.scanned == 0 { return 100.0; }
        (self.with_header as f32 / self.scanned as f32) * 100.0
    }
}

/// Scan `.rs` (and optionally `.py`, `.ts`) files under `project_dir` for SPDX headers.
/// An SPDX header is any line containing "SPDX-License-Identifier:" or "Copyright".
pub fn check_copyright(project_dir: &Path) -> CopyrightReport {
    let extensions = ["rs", "py", "ts", "tsx"];
    let mut report = CopyrightReport::default();

    let walker = match std::fs::read_dir(project_dir) {
        Ok(_) => project_dir,
        Err(_) => return report,
    };

    walk_dir(walker, &extensions, &mut report);
    report
}

fn walk_dir(dir: &Path, extensions: &[&str], report: &mut CopyrightReport) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip hidden dirs, target, node_modules
        if name_str.starts_with('.') || name_str == "target" || name_str == "node_modules" {
            continue;
        }
        if path.is_dir() {
            walk_dir(&path, extensions, report);
        } else if path.is_file() {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if extensions.contains(&ext) {
                report.scanned += 1;
                let has = file_has_header(&path);
                if has {
                    report.with_header += 1;
                } else {
                    let rel = path.to_string_lossy().to_string();
                    report.missing.push(MissingHeader { path: rel });
                }
            }
        }
    }
}

fn file_has_header(path: &Path) -> bool {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Check first 10 lines for SPDX or Copyright marker
    for line in contents.lines().take(10) {
        let l = line.to_lowercase();
        if l.contains("spdx-license-identifier") || l.contains("copyright") {
            return true;
        }
    }
    false
}
