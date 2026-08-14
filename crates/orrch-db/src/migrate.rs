use std::fs;
use std::path::Path;

/// Relocate a root `PLAN.md` / `DEVLOG.md` into `.orrch/` if present and not
/// already moved. Returns the list of relocated filenames. Idempotent: if the
/// `.orrch/` copy already exists, the root file is left alone (caller decides).
pub fn relocate_root_docs(project_dir: &Path) -> std::io::Result<Vec<String>> {
    let orrch = project_dir.join(".orrch");
    fs::create_dir_all(&orrch)?;
    let mut moved = Vec::new();
    for name in ["PLAN.md", "DEVLOG.md"] {
        let src = project_dir.join(name);
        let dst = orrch.join(name);
        if src.exists() && !dst.exists() {
            fs::rename(&src, &dst)?;
            moved.push(name.to_string());
        }
    }
    Ok(moved)
}

/// Convert a legacy `.retrospect/errors.jsonl` into one `events/*.md` file per
/// record. Each line is a JSON object with at least `fingerprint`, `category`,
/// `raw_context`, `timestamp`, `resolved`, and optionally `resolution`.
/// Returns the number of event files written. Idempotent by filename (derived
/// from fingerprint+timestamp); existing files are skipped.
pub fn migrate_errors_jsonl(project_dir: &Path) -> anyhow::Result<usize> {
    let src = project_dir.join(".retrospect").join("errors.jsonl");
    if !src.exists() {
        return Ok(0);
    }
    let events_dir = project_dir.join(".orrch").join("events");
    fs::create_dir_all(&events_dir)?;
    let content = fs::read_to_string(&src)?;
    let mut written = 0;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let fp = v
            .get("fingerprint")
            .and_then(|x| x.as_str())
            .unwrap_or("nofp");
        let ts_secs = v.get("timestamp").and_then(|x| x.as_f64()).unwrap_or(0.0) as i64;
        let short = &fp[..fp.len().min(6)];
        let fname = format!("legacy-{ts_secs}-{short}.md");
        let dst = events_dir.join(&fname);
        if dst.exists() {
            continue;
        }
        let resolved = v.get("resolved").and_then(|x| x.as_bool()).unwrap_or(false);
        let kind = if resolved {
            "bug_resolved"
        } else {
            "bug_opened"
        };
        let category = v
            .get("category")
            .and_then(|x| x.as_str())
            .unwrap_or("Unknown");
        let raw = v.get("raw_context").and_then(|x| x.as_str()).unwrap_or("");
        let resolution = v.get("resolution").and_then(|x| x.as_str()).unwrap_or("");
        // Lexical RFC3339-ish ts is not reconstructable from epoch secs without a
        // date lib; store the epoch in a sortable zero-padded form so ordering holds.
        let ts_field = format!("1970-epoch-{ts_secs:020}");
        let mut md = String::new();
        md.push_str("---\n");
        md.push_str(&format!("id: {short}{ts_secs}\n"));
        md.push_str(&format!("ts: {ts_field}\n"));
        md.push_str(&format!("kind: {kind}\n"));
        md.push_str("entity_type: bug\n");
        md.push_str(&format!("entity_id: {fp}\n"));
        md.push_str(&format!("title: {category} error\n"));
        md.push_str("severity: unknown\n");
        if resolved && !resolution.is_empty() {
            md.push_str(&format!("resolution: {resolution}\n"));
        }
        md.push_str("---\n");
        md.push_str(raw);
        md.push('\n');
        fs::write(&dst, md)?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn relocates_plan_and_devlog() {
        let tmp = tempdir().unwrap();
        fs::write(tmp.path().join("PLAN.md"), "# Plan").unwrap();
        fs::write(tmp.path().join("DEVLOG.md"), "# Log").unwrap();
        let moved = relocate_root_docs(tmp.path()).unwrap();
        assert!(moved.contains(&"PLAN.md".to_string()));
        assert!(tmp.path().join(".orrch/PLAN.md").exists());
        assert!(!tmp.path().join("PLAN.md").exists());
        // Idempotent second run moves nothing.
        let again = relocate_root_docs(tmp.path()).unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn converts_errors_jsonl_to_event_files() {
        let tmp = tempdir().unwrap();
        let retro = tmp.path().join(".retrospect");
        fs::create_dir_all(&retro).unwrap();
        fs::write(
            retro.join("errors.jsonl"),
            "{\"fingerprint\":\"abcdef123\",\"category\":\"Type\",\"raw_context\":\"mismatched types\",\"timestamp\":1700000000.0,\"resolved\":false}\n",
        )
        .unwrap();
        let n = migrate_errors_jsonl(tmp.path()).unwrap();
        assert_eq!(n, 1);
        let dir = tmp.path().join(".orrch/events");
        let count = fs::read_dir(&dir).unwrap().count();
        assert_eq!(count, 1);
    }
}
