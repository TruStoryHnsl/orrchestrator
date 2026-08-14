//! Time abstraction so the scheduler is deterministically testable.
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    /// Milliseconds since an arbitrary fixed epoch (monotonic enough for waits).
    fn now_ms(&self) -> u64;
}

/// Wall-clock implementation for production.
#[derive(Debug, Default, Clone)]
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// Manually-advanced clock for tests.
#[derive(Debug, Clone, Default)]
pub struct FakeClock(Arc<AtomicU64>);
impl FakeClock {
    pub fn new() -> Self {
        FakeClock(Arc::new(AtomicU64::new(0)))
    }
    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}
impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fake_clock_advances() {
        let c = FakeClock::new();
        assert_eq!(c.now_ms(), 0);
        c.advance(500);
        assert_eq!(c.now_ms(), 500);
    }
}
