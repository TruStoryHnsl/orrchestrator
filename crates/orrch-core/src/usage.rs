//! Background usage tracking — records per-provider session metrics to JSONL,
//! plus in-memory rate limit detection for dynamic throttling.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::config::config_dir;

// ─── Persistent JSONL types ─────────────────────────────────────────

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

// ─── Rate limiting types ────────────────────────────────────────────

/// Per-provider rate limit thresholds.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Max requests per minute before throttling.
    pub requests_per_min: u32,
    /// Max tokens per minute before throttling (0 = no limit).
    pub tokens_per_min: u64,
    /// How long to throttle when limits are exceeded (seconds).
    pub cooldown_secs: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_min: 60,
            tokens_per_min: 0,
            cooldown_secs: 60,
        }
    }
}

/// A single in-memory throttle event (request or token consumption).
#[derive(Debug, Clone)]
struct ThrottleEvent {
    timestamp: Instant,
    tokens: u64,
}

// ─── Unified tracker ────────────────────────────────────────────────

/// Combined usage tracker: JSONL persistence for historical analysis,
/// plus in-memory rolling windows for real-time rate limit detection.
pub struct UsageTracker {
    // JSONL persistence
    log_path: PathBuf,
    // Rate limiting (in-memory rolling window)
    rate_limits: HashMap<String, RateLimitConfig>,
    events: HashMap<String, Vec<ThrottleEvent>>,
    /// Providers currently throttled, with reason strings.
    pub throttled: HashMap<String, String>,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self {
            log_path: config_dir().join("usage.jsonl"),
            rate_limits: HashMap::new(),
            events: HashMap::new(),
            throttled: HashMap::new(),
        }
    }

    // ─── JSONL persistence methods ──────────────────────────────

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

        let mut providers: HashMap<String, ProviderUsage> = HashMap::new();
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

    // ─── Rate limiting methods ──────────────────────────────────

    /// Set rate limit config for a provider.
    pub fn set_rate_limit(&mut self, provider: &str, config: RateLimitConfig) {
        self.rate_limits.insert(provider.to_string(), config);
    }

    /// Set default rate limits for known providers.
    pub fn set_defaults(&mut self) {
        self.rate_limits.insert("Anthropic".into(), RateLimitConfig {
            requests_per_min: 50,
            tokens_per_min: 80_000,
            cooldown_secs: 60,
        });
        self.rate_limits.insert("Google".into(), RateLimitConfig {
            requests_per_min: 30,
            tokens_per_min: 0,
            cooldown_secs: 60,
        });
        self.rate_limits.insert("Mistral".into(), RateLimitConfig {
            requests_per_min: 30,
            tokens_per_min: 0,
            cooldown_secs: 60,
        });
        self.rate_limits.insert("OpenAI".into(), RateLimitConfig {
            requests_per_min: 60,
            tokens_per_min: 100_000,
            cooldown_secs: 60,
        });
    }

    /// Record a request to a provider (for throttle tracking).
    pub fn record_request(&mut self, provider: &str, tokens: u64) {
        self.events
            .entry(provider.to_string())
            .or_default()
            .push(ThrottleEvent {
                timestamp: Instant::now(),
                tokens,
            });
    }

    /// Prune events older than the rolling window (1 minute).
    fn prune(&mut self) {
        let cutoff = Instant::now() - std::time::Duration::from_secs(60);
        for events in self.events.values_mut() {
            events.retain(|e| e.timestamp > cutoff);
        }
    }

    /// Check all providers against their rate limits.
    /// Returns list of (provider, reason, cooldown_secs) that should be throttled.
    pub fn check_throttle(&mut self) -> Vec<(String, String, u64)> {
        self.prune();
        let mut results = Vec::new();

        for (provider, config) in &self.rate_limits {
            if let Some(events) = self.events.get(provider) {
                let req_count = events.len() as u32;
                let token_count: u64 = events.iter().map(|e| e.tokens).sum();

                if req_count >= config.requests_per_min {
                    let reason = format!(
                        "rate limit: {}/{} req/min",
                        req_count, config.requests_per_min
                    );
                    self.throttled.insert(provider.clone(), reason.clone());
                    results.push((provider.clone(), reason, config.cooldown_secs));
                    continue;
                }

                if config.tokens_per_min > 0 && token_count >= config.tokens_per_min {
                    let reason = format!(
                        "token limit: {}/{} tok/min",
                        token_count, config.tokens_per_min
                    );
                    self.throttled.insert(provider.clone(), reason.clone());
                    results.push((provider.clone(), reason, config.cooldown_secs));
                    continue;
                }

                // Clear throttle if under limits
                self.throttled.remove(provider);
            }
        }

        results
    }

    /// Check if a specific provider is currently throttled.
    pub fn is_throttled(&self, provider: &str) -> bool {
        self.throttled.contains_key(provider)
    }

    /// Get throttle reason for a provider, if any.
    pub fn throttle_reason(&self, provider: &str) -> Option<&str> {
        self.throttled.get(provider).map(|s| s.as_str())
    }

    /// Get a summary of current rolling-window usage for all tracked providers.
    pub fn throttle_summary(&self) -> Vec<(String, u32, u64)> {
        let cutoff = Instant::now() - std::time::Duration::from_secs(60);
        self.events
            .iter()
            .map(|(provider, events)| {
                let recent: Vec<_> = events.iter().filter(|e| e.timestamp > cutoff).collect();
                let req_count = recent.len() as u32;
                let token_count: u64 = recent.iter().map(|e| e.tokens).sum();
                (provider.clone(), req_count, token_count)
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
    epoch_to_iso(secs)
}

fn epoch_to_iso(epoch: u64) -> String {
    let secs = epoch;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;

    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Parse an ISO 8601 timestamp back to epoch seconds. Returns None on failure.
fn parse_epoch(iso: &str) -> Option<u64> {
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

fn days_to_date(days: u64) -> (u64, u64, u64) {
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

    fn test_tracker(log_path: PathBuf) -> UsageTracker {
        UsageTracker {
            log_path,
            rate_limits: HashMap::new(),
            events: HashMap::new(),
            throttled: HashMap::new(),
        }
    }

    #[test]
    fn test_epoch_iso_roundtrip() {
        let iso = "2025-01-15T10:30:00Z";
        let epoch = parse_epoch(iso).unwrap();
        let back = epoch_to_iso(epoch);
        assert_eq!(back, iso);
    }

    #[test]
    fn test_epoch_known_date() {
        let epoch = parse_epoch("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(epoch, 1704067200);
    }

    #[test]
    fn test_format_ago() {
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
        let tracker = test_tracker(dir.path().join("usage.jsonl"));

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
        let tracker = test_tracker(dir.path().join("usage.jsonl"));

        let now = iso_now();
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
        let tracker = test_tracker(dir.path().join("usage.jsonl"));

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
        assert_eq!(recent.len(), 1);
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

    #[test]
    fn test_usage_tracker_basic() {
        let mut tracker = UsageTracker::new();
        tracker.set_rate_limit("TestProvider", RateLimitConfig {
            requests_per_min: 5,
            tokens_per_min: 0,
            cooldown_secs: 30,
        });

        for _ in 0..4 {
            tracker.record_request("TestProvider", 100);
        }
        assert!(tracker.check_throttle().is_empty());

        tracker.record_request("TestProvider", 100);
        let throttled = tracker.check_throttle();
        assert_eq!(throttled.len(), 1);
        assert_eq!(throttled[0].0, "TestProvider");
        assert!(tracker.is_throttled("TestProvider"));
    }

    #[test]
    fn test_token_limit() {
        let mut tracker = UsageTracker::new();
        tracker.set_rate_limit("TokenProvider", RateLimitConfig {
            requests_per_min: 100,
            tokens_per_min: 1000,
            cooldown_secs: 60,
        });

        tracker.record_request("TokenProvider", 500);
        assert!(tracker.check_throttle().is_empty());

        tracker.record_request("TokenProvider", 600);
        let throttled = tracker.check_throttle();
        assert_eq!(throttled.len(), 1);
        assert!(throttled[0].1.contains("token limit"));
    }
}
