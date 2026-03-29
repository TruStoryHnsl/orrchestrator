use std::collections::HashMap;
use std::time::Instant;

use crate::store::ErrorStore;

/// Tracks error→resolution pairs within sessions.
///
/// When a session produces errors and then continues with clean output
/// for a sustained period, we consider the error "resolved".
pub struct SolutionTracker {
    /// Seconds of clean output before marking resolved
    pub resolution_cooldown_secs: f64,
    /// session_id → pending errors: (fingerprint, timestamp)
    pending: HashMap<String, Vec<(String, Instant)>>,
    /// session_id → time of last error
    last_error_time: HashMap<String, Instant>,
    /// session_id → output accumulated since last error
    output_since_error: HashMap<String, Vec<String>>,
}

impl SolutionTracker {
    pub fn new() -> Self {
        Self {
            resolution_cooldown_secs: 30.0,
            pending: HashMap::new(),
            last_error_time: HashMap::new(),
            output_since_error: HashMap::new(),
        }
    }

    /// Record that an error was seen in a session.
    pub fn on_error(&mut self, session_id: &str, fingerprint: &str) {
        self.pending
            .entry(session_id.to_string())
            .or_default()
            .push((fingerprint.to_string(), Instant::now()));
        self.last_error_time
            .insert(session_id.to_string(), Instant::now());
        self.output_since_error
            .insert(session_id.to_string(), Vec::new());
    }

    /// Feed non-error output. Returns fingerprints that were just resolved.
    pub fn on_output(
        &mut self,
        session_id: &str,
        text: &str,
        store: &mut ErrorStore,
    ) -> Vec<String> {
        if !self.pending.contains_key(session_id) {
            return Vec::new();
        }

        self.output_since_error
            .entry(session_id.to_string())
            .or_default()
            .push(text.to_string());

        let now = Instant::now();
        let last_err = self
            .last_error_time
            .get(session_id)
            .copied()
            .unwrap_or(now);

        if now.duration_since(last_err).as_secs_f64() < self.resolution_cooldown_secs {
            return Vec::new();
        }

        // Cooldown passed — resolve all pending errors
        let mut resolved_fps = Vec::new();
        let output_chunks = self
            .output_since_error
            .remove(session_id)
            .unwrap_or_default();
        let output_summary: String = output_chunks.join("\n");
        let truncated = if output_summary.len() > 500 {
            &output_summary[output_summary.len() - 500..]
        } else {
            &output_summary
        };

        if let Some(pending) = self.pending.remove(session_id) {
            for (fp, _) in pending {
                let resolution =
                    format!("Auto-resolved after continued output. Post-error context:\n{truncated}");
                store.mark_resolved(&fp, &resolution);
                resolved_fps.push(fp);
            }
        }

        self.last_error_time.remove(session_id);
        resolved_fps
    }

    /// Session ended — pending errors remain unresolved.
    pub fn on_session_end(&mut self, session_id: &str) {
        self.pending.remove(session_id);
        self.last_error_time.remove(session_id);
        self.output_since_error.remove(session_id);
    }
}

impl Default for SolutionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_parser::ErrorCategory;
    use crate::store::ErrorRecord;
    use tempfile::TempDir;

    #[test]
    fn test_resolution_after_cooldown() {
        let tmp = TempDir::new().unwrap();
        let mut store = ErrorStore::new(tmp.path());
        let mut tracker = SolutionTracker::new();
        tracker.resolution_cooldown_secs = 0.0; // instant for testing

        // Record error
        store.append(ErrorRecord::new(
            "fp1".into(),
            ErrorCategory::Lookup,
            "KeyError".into(),
            "s1".into(),
            tmp.path().to_string_lossy().into(),
        ));
        tracker.on_error("s1", "fp1");

        // Feed clean output — should resolve immediately with 0s cooldown
        let resolved = tracker.on_output("s1", "All good now", &mut store);
        assert!(resolved.contains(&"fp1".to_string()));
        assert!(store.get_resolution("fp1").is_some());
    }

    #[test]
    fn test_session_end_clears_pending() {
        let mut tracker = SolutionTracker::new();
        tracker.on_error("s1", "fp1");
        tracker.on_session_end("s1");
        // No panic, pending cleaned up
    }
}
