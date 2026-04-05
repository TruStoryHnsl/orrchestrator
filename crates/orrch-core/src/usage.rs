//! Background usage tracking — records per-provider session metrics to JSONL.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::config_dir;

/// A single usage event persisted as one JSONL line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub timestamp: String,
    pub provider: String,
    pub event: UsageEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsageEvent {
    SessionStart,
    SessionEnd,
    ApiCall { tokens_in: u64, tokens_out: u64 },
}

/// Append-only tracker backed by `~/.config/orrchestrator/usage.jsonl`.
pub struct UsageTracker {
    log_path: PathBuf,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self {
            log_path: config_dir().join("usage.jsonl"),
        }
    }

    /// Append a record to the JSONL log.
    pub fn record(&self, record: &UsageRecord) -> std::io::Result<()> {
        if let Some(parent) = self.log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        let line = serde_json::to_string(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Load records from the last N hours.
    pub fn load_recent(&self, hours: u64) -> Vec<UsageRecord> {
        let cutoff = now_epoch_secs().saturating_sub(hours * 3600);
        self.load_all()
            .into_iter()
            .filter(|r| parse_epoch(&r.timestamp).unwrap_or(0) >= cutoff)
            .collect()
    }

    /// Aggregate usage by provider over the last 24 hours.
    pub fn summary(&self) -> UsageSummary {
        let period_hours = 24;
        let records = self.load_recent(period_hours);

        let mut providers: std::collections::HashMap<String, ProviderUsage> =
            std::collections::HashMap::new();
        let mut total_sessions: u64 = 0;

        for rec in &records {
            let entry = providers
                .entry(rec.provider.clone())
                .or_insert_with(|| ProviderUsage {
                    provider: rec.provider.clone(),
                    session_count: 0,
                    total_duration_secs: 0.0,
                    last_used: None,
                });

            match &rec.event {
                UsageEvent::SessionStart => {
                    entry.session_count += 1;
                    total_sessions += 1;
                    // Update last_used to most recent timestamp
                    match (&entry.last_used, &rec.timestamp) {
                        (None, ts) => entry.last_used = Some(ts.clone()),
                        (Some(prev), ts) if ts > prev => entry.last_used = Some(ts.clone()),
                        _ => {}
                    }
                }
                UsageEvent::SessionEnd => {
                    if let Some(dur) = rec.duration_secs {
                        entry.total_duration_secs += dur;
                    }
                    match (&entry.last_used, &rec.timestamp) {
                        (None, ts) => entry.last_used = Some(ts.clone()),
                        (Some(prev), ts) if ts > prev => entry.last_used = Some(ts.clone()),
                        _ => {}
                    }
                }
                UsageEvent::ApiCall { .. } => {
                    match (&entry.last_used, &rec.timestamp) {
                        (None, ts) => entry.last_used = Some(ts.clone()),
                        (Some(prev), ts) if ts > prev => entry.last_used = Some(ts.clone()),
                        _ => {}
                    }
                }
            }
        }

        let mut per_provider: Vec<ProviderUsage> = providers.into_values().collect();
        per_provider.sort_by(|a, b| b.session_count.cmp(&a.session_count));

        UsageSummary {
            per_provider,
            total_sessions,
            period_hours,
        }
    }

    /// Read all records from the JSONL file.
    fn load_all(&self) -> Vec<UsageRecord> {
        let file = match File::open(&self.log_path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        BufReader::new(file)
            .lines()
            .filter_map(|line| {
                let line = line.ok()?;
                serde_json::from_str(&line).ok()
            })
            .collect()
    }
}

/// Aggregated usage summary for display.
pub struct UsageSummary {
    pub per_provider: Vec<ProviderUsage>,
    pub total_sessions: u64,
    pub period_hours: u64,
}

/// Per-provider aggregated metrics.
pub struct ProviderUsage {
    pub provider: String,
    pub session_count: u64,
    pub total_duration_secs: f64,
    pub last_used: Option<String>,
}

// ─── Timestamp helpers (no chrono dependency) ─────────────────────────

/// Current Unix epoch seconds.
fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Generate an ISO 8601 timestamp string (UTC).
pub fn iso_now() -> String {
    let secs = now_epoch_secs();
    // Convert epoch to YYYY-MM-DDTHH:MM:SSZ using manual calendar math.
    epoch_to_iso(secs)
}

fn epoch_to_iso(epoch: u64) -> String {
    let secs = epoch;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    // Days since 1970-01-01 → date
    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Parse an ISO 8601 timestamp back to epoch seconds. Returns None on failure.
fn parse_epoch(iso: &str) -> Option<u64> {
    // Expected format: YYYY-MM-DDTHH:MM:SSZ
    if iso.len() < 19 {
        return None;
    }
    let year: u64 = iso.get(0..4)?.parse().ok()?;
    let month: u64 = iso.get(5..7)?.parse().ok()?;
    let day: u64 = iso.get(8..10)?.parse().ok()?;
    let hour: u64 = iso.get(11..13)?.parse().ok()?;
    let min: u64 = iso.get(14..16)?.parse().ok()?;
    let sec: u64 = iso.get(17..19)?.parse().ok()?;

    let days = date_to_days(year, month, day);
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

/// Convert days since epoch (1970-01-01) to (year, month, day).
fn days_to_date(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Convert (year, month, day) to days since epoch (1970-01-01).
fn date_to_days(year: u64, month: u64, day: u64) -> u64 {
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Format an ISO 8601 timestamp as a human-friendly "X ago" string.
pub fn format_ago(iso: &str) -> String {
    let Some(then) = parse_epoch(iso) else {
        return "—".into();
    };
    let now = now_epoch_secs();
    if then > now {
        return "just now".into();
    }
    let diff = now - then;
    if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Format seconds into a human-friendly duration string like "4h 23m" or "12m".
pub fn format_duration(secs: f64) -> String {
    let total = secs as u64;
    if total < 60 {
        format!("{}s", total)
    } else if total < 3600 {
        let m = total / 60;
        let s = total % 60;
        if s > 0 {
            format!("{}m {:02}s", m, s)
        } else {
            format!("{}m", m)
        }
    } else {
        let h = total / 3600;
        let m = (total % 3600) / 60;
        format!("{}h {:02}m", h, m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_epoch_iso_roundtrip() {
        // 2025-01-15T10:30:00Z
        let iso = "2025-01-15T10:30:00Z";
        let epoch = parse_epoch(iso).unwrap();
        let back = epoch_to_iso(epoch);
        assert_eq!(back, iso);
    }

    #[test]
    fn test_epoch_known_date() {
        // 2024-01-01T00:00:00Z = 1704067200
        let epoch = parse_epoch("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(epoch, 1704067200);
    }

    #[test]
    fn test_format_ago() {
        // Just test the formatting logic paths
        assert_eq!(format_ago("invalid"), "—");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(45.0), "45s");
        assert_eq!(format_duration(120.0), "2m");
        assert_eq!(format_duration(3723.0), "1h 02m");
        assert_eq!(format_duration(7380.0), "2h 03m");
    }

    #[test]
    fn test_record_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let tracker = UsageTracker {
            log_path: dir.path().join("usage.jsonl"),
        };

        let rec = UsageRecord {
            timestamp: iso_now(),
            provider: "claude".into(),
            event: UsageEvent::SessionStart,
            session_name: Some("test-session".into()),
            project: Some("myproject".into()),
            duration_secs: None,
        };
        tracker.record(&rec).unwrap();

        let all = tracker.load_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].provider, "claude");

        // Verify JSONL is valid
        let mut contents = String::new();
        File::open(&tracker.log_path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert!(contents.ends_with('\n'));
        assert_eq!(contents.lines().count(), 1);
    }

    #[test]
    fn test_summary_aggregation() {
        let dir = tempfile::tempdir().unwrap();
        let tracker = UsageTracker {
            log_path: dir.path().join("usage.jsonl"),
        };

        let now = iso_now();
        // Two claude starts
        for _ in 0..2 {
            tracker
                .record(&UsageRecord {
                    timestamp: now.clone(),
                    provider: "claude".into(),
                    event: UsageEvent::SessionStart,
                    session_name: None,
                    project: None,
                    duration_secs: None,
                })
                .unwrap();
        }
        // One claude end with duration
        tracker
            .record(&UsageRecord {
                timestamp: now.clone(),
                provider: "claude".into(),
                event: UsageEvent::SessionEnd,
                session_name: None,
                project: None,
                duration_secs: Some(300.0),
            })
            .unwrap();
        // One gemini start
        tracker
            .record(&UsageRecord {
                timestamp: now.clone(),
                provider: "gemini".into(),
                event: UsageEvent::SessionStart,
                session_name: None,
                project: None,
                duration_secs: None,
            })
            .unwrap();

        let summary = tracker.summary();
        assert_eq!(summary.total_sessions, 3);
        assert_eq!(summary.per_provider.len(), 2);

        let claude = summary
            .per_provider
            .iter()
            .find(|p| p.provider == "claude")
            .unwrap();
        assert_eq!(claude.session_count, 2);
        assert!((claude.total_duration_secs - 300.0).abs() < 0.01);

        let gemini = summary
            .per_provider
            .iter()
            .find(|p| p.provider == "gemini")
            .unwrap();
        assert_eq!(gemini.session_count, 1);
    }

    #[test]
    fn test_load_recent_filters_old() {
        let dir = tempfile::tempdir().unwrap();
        let tracker = UsageTracker {
            log_path: dir.path().join("usage.jsonl"),
        };

        // Record with a very old timestamp
        tracker
            .record(&UsageRecord {
                timestamp: "2020-01-01T00:00:00Z".into(),
                provider: "claude".into(),
                event: UsageEvent::SessionStart,
                session_name: None,
                project: None,
                duration_secs: None,
            })
            .unwrap();
        // Record with current timestamp
        tracker
            .record(&UsageRecord {
                timestamp: iso_now(),
                provider: "claude".into(),
                event: UsageEvent::SessionStart,
                session_name: None,
                project: None,
                duration_secs: None,
            })
            .unwrap();

        let recent = tracker.load_recent(24);
        assert_eq!(recent.len(), 1); // only the current one
    }

    #[test]
    fn test_serde_roundtrip() {
        let rec = UsageRecord {
            timestamp: "2025-06-01T12:00:00Z".into(),
            provider: "claude".into(),
            event: UsageEvent::ApiCall {
                tokens_in: 1000,
                tokens_out: 500,
            },
            session_name: Some("test".into()),
            project: None,
            duration_secs: None,
        };
        let json = serde_json::to_string(&rec).unwrap();
        let parsed: UsageRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, "claude");
        match parsed.event {
            UsageEvent::ApiCall {
                tokens_in,
                tokens_out,
            } => {
                assert_eq!(tokens_in, 1000);
                assert_eq!(tokens_out, 500);
            }
            _ => panic!("wrong event type"),
        }
    }
}
