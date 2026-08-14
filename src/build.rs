// Build script for the orrchestrator binary crate.
//
// Injects two compile-time env vars used by the staleness-detection
// subsystem:
//   - ORRCH_BUILD_TIMESTAMP_NS  — nanoseconds since UNIX epoch at build time
//   - ORRCH_BUILD_REPO_ROOT     — absolute path to the workspace repo root
//
// At runtime, the staleness module scans this repo root for source files
// (.rs, Cargo.toml, build.rs) with mtimes newer than the build timestamp.
// Any such file means the running binary is older than the source tree,
// and the TUI / WebUI render a protest banner instructing the user to
// rebuild.

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    println!("cargo:rustc-env=ORRCH_BUILD_TIMESTAMP_NS={nanos}");

    // CARGO_MANIFEST_DIR points to the binary crate (src/). Repo root is its parent.
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo");
    let repo_root = std::path::Path::new(&manifest_dir)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| manifest_dir.clone());
    println!("cargo:rustc-env=ORRCH_BUILD_REPO_ROOT={repo_root}");

    // Rebuild this script when the user touches it, but DO NOT rerun on
    // every source change — cargo already rebuilds the crate when sources
    // change, and rerunning build.rs each time defeats incremental builds.
    println!("cargo:rerun-if-changed=build.rs");
}
