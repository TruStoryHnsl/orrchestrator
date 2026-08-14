use serde::{Deserialize, Serialize};

/// The entity an event is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Bug,
    Feature,
    Audit,
}

/// The kind of mutation an event records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    BugOpened,
    BugStatusChanged,
    BugResolved,
    KnownIssue,
    Audit,
}

impl EventKind {
    /// Parse from the `kind:` frontmatter value. Returns None for unknown kinds.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim() {
            "bug_opened" => Some(Self::BugOpened),
            "bug_status_changed" => Some(Self::BugStatusChanged),
            "bug_resolved" => Some(Self::BugResolved),
            "known_issue" => Some(Self::KnownIssue),
            "audit" => Some(Self::Audit),
            _ => None,
        }
    }
}

/// One immutable record parsed from an `events/<ts>-<id>.md` file.
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub id: String,
    pub project: String,
    /// RFC3339 timestamp string, lexically sortable.
    pub ts: String,
    pub kind: EventKind,
    pub entity_type: EntityType,
    pub entity_id: String,
    pub session_id: Option<String>,
    /// All remaining frontmatter fields + the markdown body, as JSON.
    pub payload: serde_json::Value,
}

/// Current-state bug row, folded from the event log.
#[derive(Debug, Clone, PartialEq)]
pub struct BugRow {
    pub project: String,
    pub bug_id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub first_seen: String,
    pub last_ts: String,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureRow {
    pub project: String,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub description: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryRow {
    pub kind: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub path: String,
    pub body_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArchFact {
    pub project: String,
    pub section: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LicenseRow {
    pub project: String,
    pub dependency: String,
    pub license: String,
    pub audit_status: String,
    pub notes: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_parses_known_and_rejects_unknown() {
        assert_eq!(
            EventKind::from_str("bug_opened"),
            Some(EventKind::BugOpened)
        );
        assert_eq!(
            EventKind::from_str("bug_resolved"),
            Some(EventKind::BugResolved)
        );
        assert_eq!(EventKind::from_str(" audit "), Some(EventKind::Audit));
        assert_eq!(EventKind::from_str("nonsense"), None);
    }
}
