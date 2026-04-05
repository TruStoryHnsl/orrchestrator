use std::collections::HashMap;
use std::time::Instant;

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

/// A single usage event (request or token consumption).
#[derive(Debug, Clone)]
struct UsageEvent {
    timestamp: Instant,
    tokens: u64,
}

/// Tracks API usage per provider for rate limit detection.
#[derive(Debug, Default)]
pub struct UsageTracker {
    /// Per-provider rate limit configuration.
    rate_limits: HashMap<String, RateLimitConfig>,
    /// Per-provider usage events (rolling window).
    events: HashMap<String, Vec<UsageEvent>>,
    /// Providers currently throttled, with reason strings.
    pub throttled: HashMap<String, String>,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set rate limit config for a provider.
    pub fn set_rate_limit(&mut self, provider: &str, config: RateLimitConfig) {
        self.rate_limits.insert(provider.to_string(), config);
    }

    /// Set default rate limits for known providers.
    pub fn set_defaults(&mut self) {
        // Anthropic: 60 req/min is typical for most tiers
        self.rate_limits.insert("Anthropic".into(), RateLimitConfig {
            requests_per_min: 50,
            tokens_per_min: 80_000,
            cooldown_secs: 60,
        });
        // Google: generous free tier
        self.rate_limits.insert("Google".into(), RateLimitConfig {
            requests_per_min: 30,
            tokens_per_min: 0,
            cooldown_secs: 60,
        });
        // Mistral
        self.rate_limits.insert("Mistral".into(), RateLimitConfig {
            requests_per_min: 30,
            tokens_per_min: 0,
            cooldown_secs: 60,
        });
        // OpenAI
        self.rate_limits.insert("OpenAI".into(), RateLimitConfig {
            requests_per_min: 60,
            tokens_per_min: 100_000,
            cooldown_secs: 60,
        });
    }

    /// Record a request to a provider.
    pub fn record_request(&mut self, provider: &str, tokens: u64) {
        self.events
            .entry(provider.to_string())
            .or_default()
            .push(UsageEvent {
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
    /// Returns list of (provider, reason) that should be throttled.
    pub fn check_throttle(&mut self) -> Vec<(String, String, u64)> {
        self.prune();
        let mut results = Vec::new();

        for (provider, config) in &self.rate_limits {
            if let Some(events) = self.events.get(provider) {
                let req_count = events.len() as u32;
                let token_count: u64 = events.iter().map(|e| e.tokens).sum();

                // Check requests/min
                if req_count >= config.requests_per_min {
                    let reason = format!(
                        "rate limit: {}/{} req/min",
                        req_count, config.requests_per_min
                    );
                    self.throttled.insert(provider.clone(), reason.clone());
                    results.push((provider.clone(), reason, config.cooldown_secs));
                    continue;
                }

                // Check tokens/min (if configured)
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

    /// Get a summary of current usage for all tracked providers.
    pub fn summary(&self) -> Vec<(String, u32, u64)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_tracker_basic() {
        let mut tracker = UsageTracker::new();
        tracker.set_rate_limit("TestProvider", RateLimitConfig {
            requests_per_min: 5,
            tokens_per_min: 0,
            cooldown_secs: 30,
        });

        // Record 4 requests — should be fine
        for _ in 0..4 {
            tracker.record_request("TestProvider", 100);
        }
        assert!(tracker.check_throttle().is_empty());

        // Record 1 more to hit the limit
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
