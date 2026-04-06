use std::path::{Path, PathBuf};
use std::time::SystemTime;
use serde::{Deserialize, Serialize};

/// Capability tier for workforce assignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    /// Claude Opus/Sonnet, GPT-4o — full autonomy workflows.
    Enterprise,
    /// Mistral Large API, Gemini Pro — structured instruction workflows.
    MidTier,
    /// Ollama local, Gemini free, Mistral small — rigid logic, scope-limited workflows.
    Local,
}

impl ModelTier {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enterprise => "Enterprise",
            Self::MidTier => "Mid-Tier",
            Self::Local => "Local/Free",
        }
    }

    pub fn badge(&self) -> &'static str {
        match self {
            Self::Enterprise => "$$",
            Self::MidTier => "$ ",
            Self::Local => "  ",
        }
    }
}

/// Pricing model for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingModel {
    /// Pay per token (input + output rates).
    PerToken {
        input_per_million: f64,
        output_per_million: f64,
    },
    /// Flat subscription / unlimited tier.
    Subscription { monthly_cost: f64 },
    /// Free tier with rate limits.
    Free { requests_per_minute: Option<u32> },
    /// Local model, no API cost.
    Local,
}

impl PricingModel {
    pub fn display(&self) -> String {
        match self {
            Self::PerToken { input_per_million, output_per_million } =>
                format!("${:.2}/${:.2} per 1M tok (in/out)", input_per_million, output_per_million),
            Self::Subscription { monthly_cost } =>
                format!("${:.2}/mo", monthly_cost),
            Self::Free { requests_per_minute } => match requests_per_minute {
                Some(rpm) => format!("Free ({rpm} req/min)"),
                None => "Free".into(),
            },
            Self::Local => "Local (no cost)".into(),
        }
    }
}

/// Manual valve state for a provider — user-controlled on/off toggle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Valve {
    /// Whether this provider is currently shut off.
    pub closed: bool,
    /// When to automatically reopen (Unix timestamp). None = manual only.
    pub reopen_at: Option<u64>,
    /// Reason the valve was closed (user note).
    pub reason: String,
}

impl Default for Valve {
    fn default() -> Self {
        Self { closed: false, reopen_at: None, reason: String::new() }
    }
}

impl Valve {
    /// Check if the valve should auto-reopen based on current time.
    pub fn check_reopen(&mut self) -> bool {
        if let Some(reopen_at) = self.reopen_at {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if now >= reopen_at {
                self.closed = false;
                self.reopen_at = None;
                self.reason.clear();
                return true; // valve was reopened
            }
        }
        false
    }

    /// Check if the valve is past its reopen time (immutable check, no mutation).
    pub fn is_past_reopen(&self) -> bool {
        if let Some(reopen_at) = self.reopen_at {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now >= reopen_at
        } else {
            false
        }
    }

    /// Human-readable reopen time showing the actual date.
    pub fn reopen_display(&self) -> String {
        match self.reopen_at {
            Some(ts) => {
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if ts <= now {
                    return "reopening...".into();
                }
                let remaining = ts - now;
                let countdown = if remaining < 3600 {
                    format!("{}m", remaining / 60)
                } else if remaining < 86400 {
                    format!("{}h{}m", remaining / 3600, (remaining % 3600) / 60)
                } else {
                    format!("{}d{}h", remaining / 86400, (remaining % 86400) / 3600)
                };
                // Also show the actual date so it's never ambiguous
                let (y, mo, d) = epoch_to_date(ts);
                let weekday = day_of_week(ts);
                format!("{} ({} {}-{:02}-{:02})", countdown, weekday, y, mo, d)
            }
            None => "manual".into(),
        }
    }
}

/// A registered AI model in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub name: String,
    pub provider: String,
    pub model_id: String,
    pub tier: ModelTier,
    pub pricing: PricingModel,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub max_context: Option<u64>,
    pub api_key_env: Option<String>,
    pub notes: String,
    #[serde(skip)]
    pub path: PathBuf,
}

impl ModelEntry {
    pub fn summary_line(&self) -> String {
        format!("{} {} — {} ({})", self.tier.badge(), self.name, self.provider, self.pricing.display())
    }
}

/// Provider-level valve state. Keyed by provider name (e.g., "Anthropic", "Mistral").
/// Stored in ~/.config/orrchestrator/valves.json
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValveStore {
    pub valves: std::collections::HashMap<String, Valve>,
}

impl ValveStore {
    pub fn load() -> Self {
        let path = valve_store_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(store) = serde_json::from_str(&content) {
                    return store;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = valve_store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Get or create a valve for a provider.
    pub fn get_mut(&mut self, provider: &str) -> &mut Valve {
        self.valves.entry(provider.to_string()).or_default()
    }

    /// Check if a provider is currently blocked.
    /// Returns false if the reopen time has already passed (pending tick).
    pub fn is_blocked(&self, provider: &str) -> bool {
        self.valves
            .get(provider)
            .is_some_and(|v| v.closed && !v.is_past_reopen())
    }

    /// Close a valve (block a provider).
    pub fn close(&mut self, provider: &str, reason: &str, reopen_at: Option<u64>) {
        let valve = self.get_mut(provider);
        valve.closed = true;
        valve.reason = reason.to_string();
        valve.reopen_at = reopen_at;
        let _ = self.save();
    }

    /// Open a valve (unblock a provider).
    pub fn open(&mut self, provider: &str) {
        let valve = self.get_mut(provider);
        valve.closed = false;
        valve.reopen_at = None;
        valve.reason.clear();
        let _ = self.save();
    }

    /// Auto-close a valve with a timed reopen. Used by IRM throttling.
    pub fn auto_close(&mut self, provider: &str, reason: &str, duration_secs: u64) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.close(provider, reason, Some(now + duration_secs));
    }

    /// Close a valve until the next occurrence of `weekday` (0=Sun..6=Sat) at
    /// `hour_utc` hours UTC.  Used for billing-cycle-aware usage limits.
    ///
    /// The reason string is auto-generated with the computed date so it
    /// always matches the actual `reopen_at` timestamp.
    pub fn close_until_next_weekday(
        &mut self,
        provider: &str,
        target_weekday: u32,
        hour_utc: u32,
    ) {
        let reopen_at = next_weekday_epoch(target_weekday, hour_utc);
        let (y, mo, d) = epoch_to_date(reopen_at);
        let wday = day_of_week(reopen_at);
        let reason = format!(
            "Usage limit reached — reopens {} {}-{:02}-{:02} {:02}:00 UTC",
            wday, y, mo, d, hour_utc,
        );
        self.close(provider, &reason, Some(reopen_at));
    }

    /// Check if a provider is currently blocked, considering auto-reopen.
    /// Returns (blocked, reason) tuple.  If the reopen time has already
    /// passed, reports the provider as unblocked (the next `tick()` will
    /// persist the change).
    pub fn check_provider(&self, provider: &str) -> (bool, String) {
        match self.valves.get(provider) {
            Some(v) if v.closed => {
                // Don't report blocked if the reopen time has already passed
                if v.is_past_reopen() {
                    (false, String::new())
                } else {
                    (true, v.reason.clone())
                }
            }
            _ => (false, String::new()),
        }
    }

    /// Check all valves for auto-reopen and return names of any that reopened.
    pub fn tick(&mut self) -> Vec<String> {
        let mut reopened = Vec::new();
        for (name, valve) in self.valves.iter_mut() {
            if valve.closed && valve.check_reopen() {
                reopened.push(name.clone());
            }
        }
        if !reopened.is_empty() {
            let _ = self.save();
        }
        reopened
    }
}

fn valve_store_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/corr".into());
    PathBuf::from(home)
        .join(".config")
        .join("orrchestrator")
        .join("valves.json")
}

// ─── Date arithmetic (no chrono dependency) ─────────────────────────────

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Convert epoch seconds to (year, month, day) in UTC.
fn epoch_to_date(epoch: u64) -> (u64, u64, u64) {
    let days = epoch / 86400;
    // Howard Hinnant's algorithm (same as usage.rs)
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

/// Convert (year, month, day) to days since epoch in UTC.
#[cfg(test)]
fn date_to_epoch_days(year: u64, month: u64, day: u64) -> u64 {
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Day of the week for epoch seconds: 0=Sun, 1=Mon, ..., 6=Sat.
fn weekday_from_epoch(epoch: u64) -> u32 {
    // Jan 1 1970 was a Thursday (4).
    ((epoch / 86400 + 4) % 7) as u32
}

/// Short weekday name.
fn day_of_week(epoch: u64) -> &'static str {
    match weekday_from_epoch(epoch) {
        0 => "Sun", 1 => "Mon", 2 => "Tue", 3 => "Wed",
        4 => "Thu", 5 => "Fri", 6 => "Sat", _ => "???",
    }
}

/// Compute the Unix timestamp for the next occurrence of `target_weekday`
/// (0=Sun..6=Sat) at `hour_utc` hours UTC.  If today IS that weekday but
/// the hour has already passed, returns NEXT week.
fn next_weekday_epoch(target_weekday: u32, hour_utc: u32) -> u64 {
    let now = now_epoch();
    let today_wday = weekday_from_epoch(now);
    let today_at_hour = {
        let days = now / 86400;
        days * 86400 + (hour_utc as u64) * 3600
    };

    let mut days_ahead = (target_weekday as i32 - today_wday as i32).rem_euclid(7) as u64;
    // If it's the target day but past the hour, push to next week
    if days_ahead == 0 && now >= today_at_hour {
        days_ahead = 7;
    }

    today_at_hour + days_ahead * 86400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_to_date_known() {
        // 2024-01-01 00:00:00 UTC = 1704067200
        assert_eq!(epoch_to_date(1704067200), (2024, 1, 1));
    }

    #[test]
    fn test_weekday_known() {
        // 2024-01-01 was a Monday
        assert_eq!(weekday_from_epoch(1704067200), 1);
    }

    #[test]
    fn test_next_weekday_friday() {
        // From Monday 2024-01-01 00:00 UTC, next Friday at 00:00 UTC = Jan 5
        let fri = next_weekday_epoch(5, 0); // 5 = Friday
        // From any fixed Monday, the next Friday should be 4 days later
        // (or if we're past the hour on Friday, 7 days from that Friday)
        assert_eq!(weekday_from_epoch(fri), 5); // It IS a Friday
        assert!(fri > 1704067200); // It's in the future from that Monday
    }

    #[test]
    fn test_next_weekday_same_day_past_hour() {
        // If it's already Friday at 15:00 and we ask for Friday at 00:00,
        // we should get NEXT Friday, not today
        let friday_15 = {
            // Find a known Friday: 2024-01-05
            let days = date_to_epoch_days(2024, 1, 5);
            days * 86400 + 15 * 3600
        };
        assert_eq!(weekday_from_epoch(friday_15), 5); // sanity: it's Friday
        // Mock "now" is tricky since next_weekday_epoch uses SystemTime.
        // At minimum, verify the date helper roundtrips.
        let (y, m, d) = epoch_to_date(friday_15);
        assert_eq!((y, m, d), (2024, 1, 5));
    }

    #[test]
    fn test_day_of_week_names() {
        // Thursday Jan 1 1970
        assert_eq!(day_of_week(0), "Thu");
        // Friday Jan 2 1970
        assert_eq!(day_of_week(86400), "Fri");
        // Saturday Jan 3 1970
        assert_eq!(day_of_week(86400 * 2), "Sat");
    }

    #[test]
    fn test_next_weekday_always_lands_on_target() {
        // next_weekday_epoch(5, 0) must always return a Friday timestamp
        let ts = next_weekday_epoch(5, 0);
        assert_eq!(weekday_from_epoch(ts), 5, "should be Friday");
        assert!(ts > now_epoch(), "should be in the future");
        // Hour should be 0 UTC
        assert_eq!(ts % 86400, 0, "should be midnight UTC");
    }

    #[test]
    fn test_close_until_next_weekday_reason_matches_timestamp() {
        // Test the date computation logic without hitting ValveStore::save()
        let reopen_at = next_weekday_epoch(5, 0); // next Friday at 00:00 UTC
        let (y, mo, d) = epoch_to_date(reopen_at);
        let wday = day_of_week(reopen_at);
        let reason = format!(
            "Usage limit reached — reopens {} {}-{:02}-{:02} 00:00 UTC",
            wday, y, mo, d,
        );
        // The timestamp must land on a Friday
        assert_eq!(weekday_from_epoch(reopen_at), 5);
        // The reason must contain the computed date
        let expected_date = format!("{}-{:02}-{:02}", y, mo, d);
        assert!(reason.contains(&expected_date));
        assert!(reason.contains("Fri"));
    }

    #[test]
    fn test_check_provider_respects_reopen_time() {
        // Build a store with a valve past its reopen time (no save() call)
        let past = now_epoch().saturating_sub(3600);
        let mut store = ValveStore::default();
        store.valves.insert("Test".into(), Valve {
            closed: true,
            reopen_at: Some(past),
            reason: "test".into(),
        });
        let (blocked, _) = store.check_provider("Test");
        assert!(!blocked, "valve past reopen time should not report blocked");
    }

    #[test]
    fn test_check_provider_blocks_future_reopen() {
        let future = now_epoch() + 86400;
        let mut store = ValveStore::default();
        store.valves.insert("Test".into(), Valve {
            closed: true,
            reopen_at: Some(future),
            reason: "test".into(),
        });
        let (blocked, _) = store.check_provider("Test");
        assert!(blocked, "valve with future reopen should report blocked");
    }

    #[test]
    fn test_is_blocked_respects_reopen_time() {
        let past = now_epoch().saturating_sub(60);
        let mut store = ValveStore::default();
        store.valves.insert("Test".into(), Valve {
            closed: true,
            reopen_at: Some(past),
            reason: "test".into(),
        });
        assert!(!store.is_blocked("Test"), "is_blocked should return false after reopen time");
    }

    #[test]
    fn test_model_tier_labels() {
        assert_eq!(ModelTier::Enterprise.label(), "Enterprise");
        assert_eq!(ModelTier::Local.badge(), "  ");
    }

    #[test]
    fn test_pricing_display() {
        let p = PricingModel::PerToken { input_per_million: 3.0, output_per_million: 15.0 };
        assert!(p.display().contains("$3.00"));
        assert!(PricingModel::Local.display().contains("no cost"));
    }
}

/// Load model entries from .md files in a directory.
pub fn load_models(dir: &Path) -> Vec<ModelEntry> {
    let mut models = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Some(model) = parse_model_file(&path) {
                    models.push(model);
                }
            }
        }
    }
    models.sort_by(|a, b| a.tier.label().cmp(b.tier.label()).then(a.name.cmp(&b.name)));
    models
}

fn parse_model_file(path: &Path) -> Option<ModelEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let (fm, body) = crate::store::parse_frontmatter_pub(&content)?;

    let tier_str = extract(&fm, "tier").unwrap_or_default();
    let tier = match tier_str.to_lowercase().as_str() {
        "enterprise" => ModelTier::Enterprise,
        "mid-tier" | "midtier" | "mid_tier" => ModelTier::MidTier,
        "local" | "free" | "local/free" => ModelTier::Local,
        _ => ModelTier::MidTier,
    };

    let pricing_str = extract(&fm, "pricing").unwrap_or_default();
    let pricing = if pricing_str.contains("local") {
        PricingModel::Local
    } else if pricing_str.contains("free") {
        PricingModel::Free { requests_per_minute: None }
    } else if pricing_str.contains("subscription") {
        PricingModel::Subscription { monthly_cost: 0.0 }
    } else {
        PricingModel::PerToken { input_per_million: 0.0, output_per_million: 0.0 }
    };

    Some(ModelEntry {
        name: extract(&fm, "name")?,
        provider: extract(&fm, "provider").unwrap_or_default(),
        model_id: extract(&fm, "model_id").unwrap_or_default(),
        tier,
        pricing,
        capabilities: extract_list(&fm, "capabilities"),
        limitations: extract_list(&fm, "limitations"),
        max_context: extract(&fm, "max_context").and_then(|s| s.replace(['k', 'K'], "000").parse().ok()),
        api_key_env: extract(&fm, "api_key_env"),
        notes: body.trim().to_string(),
        path: path.to_path_buf(),
    })
}

fn extract(fm: &str, key: &str) -> Option<String> {
    crate::store::extract_field_pub(fm, key)
}

fn extract_list(fm: &str, key: &str) -> Vec<String> {
    crate::store::extract_list_pub(fm, key)
}
