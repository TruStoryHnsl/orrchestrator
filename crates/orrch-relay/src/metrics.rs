//! Lightweight atomic counters for queue health + affinity-contiguity (the
//! cache-hit-quality proxy).
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub struct Metrics {
    pub submitted: AtomicU64,
    pub completed: AtomicU64,
    pub rejected_full: AtomicU64,
    pub cluster_continuations: AtomicU64, // consecutive same-cluster dispatches
    pub cluster_switches: AtomicU64,
}

impl Metrics {
    pub fn contiguity_ratio(&self) -> f64 {
        let c = self.cluster_continuations.load(Ordering::Relaxed) as f64;
        let s = self.cluster_switches.load(Ordering::Relaxed) as f64;
        if c + s == 0.0 { 0.0 } else { c / (c + s) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contiguity_ratio_computes() {
        let m = Metrics::default();
        m.cluster_continuations.fetch_add(3, Ordering::Relaxed);
        m.cluster_switches.fetch_add(1, Ordering::Relaxed);
        assert!((m.contiguity_ratio() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn contiguity_ratio_zero_when_empty() {
        let m = Metrics::default();
        assert_eq!(m.contiguity_ratio(), 0.0);
    }
}
