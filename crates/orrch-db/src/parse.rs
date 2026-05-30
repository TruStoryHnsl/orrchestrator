use crate::model::{EntityType, EventKind, EventRecord};
use orrch_library::store::{extract_field_pub, parse_frontmatter_pub};

/// Parse one `events/<ts>-<id>.md` file body into an `EventRecord`.
///
/// `project` is the owning project slug (the file lives under that project's
/// `.orrch/events/`). Returns `None` if required fields are missing or the
/// `kind` is unknown — a malformed event file is skipped, never fatal.
pub fn parse_event(content: &str, project: &str) -> Option<EventRecord> {
    let (frontmatter, body) = parse_frontmatter_pub(content)?;

    let id = extract_field_pub(&frontmatter, "id")?;
    let ts = extract_field_pub(&frontmatter, "ts")?;
    let kind = EventKind::from_str(&extract_field_pub(&frontmatter, "kind")?)?;
    let entity_id = extract_field_pub(&frontmatter, "entity_id")?;
    let entity_type = match extract_field_pub(&frontmatter, "entity_type")?.as_str() {
        "bug" => EntityType::Bug,
        "feature" => EntityType::Feature,
        "audit" => EntityType::Audit,
        _ => return None,
    };
    let session_id = extract_field_pub(&frontmatter, "session_id");

    // Payload = selected scalar frontmatter fields + the human-readable body note.
    let mut payload = serde_json::Map::new();
    for key in ["title", "severity", "status", "resolution"] {
        if let Some(v) = extract_field_pub(&frontmatter, key) {
            payload.insert(key.to_string(), serde_json::Value::String(v));
        }
    }
    let note = body.trim();
    if !note.is_empty() {
        payload.insert("note".to_string(), serde_json::Value::String(note.to_string()));
    }

    Some(EventRecord {
        id,
        project: project.to_string(),
        ts,
        kind,
        entity_type,
        entity_id,
        session_id,
        payload: serde_json::Value::Object(payload),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\n\
id: a1b2c3\n\
ts: 2026-05-29T14:22:33Z\n\
kind: bug_opened\n\
entity_type: bug\n\
entity_id: parser-emoji-crash\n\
session_id: T8\n\
severity: high\n\
title: Parser crashes on 4-byte emoji\n\
---\n\
PLAN.md lines starting with a green-circle emoji panic the slicer.";

    #[test]
    fn parses_a_well_formed_bug_event() {
        let ev = parse_event(SAMPLE, "orrchestrator").unwrap();
        assert_eq!(ev.id, "a1b2c3");
        assert_eq!(ev.kind, EventKind::BugOpened);
        assert_eq!(ev.entity_type, EntityType::Bug);
        assert_eq!(ev.entity_id, "parser-emoji-crash");
        assert_eq!(ev.session_id.as_deref(), Some("T8"));
        assert_eq!(ev.payload["severity"], "high");
        assert_eq!(ev.payload["title"], "Parser crashes on 4-byte emoji");
        assert!(ev.payload["note"].as_str().unwrap().contains("green-circle"));
    }

    #[test]
    fn rejects_unknown_kind() {
        let bad = SAMPLE.replace("kind: bug_opened", "kind: teleport");
        assert!(parse_event(&bad, "orrchestrator").is_none());
    }

    #[test]
    fn rejects_missing_required_field() {
        let bad = SAMPLE.replace("entity_id: parser-emoji-crash\n", "");
        assert!(parse_event(&bad, "orrchestrator").is_none());
    }
}
