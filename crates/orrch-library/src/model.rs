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

    /// Human-readable reopen time.
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
                if remaining < 3600 {
                    format!("{}m", remaining / 60)
                } else if remaining < 86400 {
                    format!("{}h{}m", remaining / 3600, (remaining % 3600) / 60)
                } else {
                    format!("{}d{}h", remaining / 86400, (remaining % 86400) / 3600)
                }
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
    pub fn is_blocked(&self, provider: &str) -> bool {
        self.valves.get(provider).is_some_and(|v| v.closed)
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

#[cfg(test)]
mod tests {
    use super::*;

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
