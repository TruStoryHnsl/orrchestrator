// Ported from mojovoice (MIT, Copyright (c) 2024-2026 itsdevcoffee).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Default)]
pub struct ToggleState {
    listening: Arc<AtomicBool>,
}

impl ToggleState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&self) {
        self.listening.store(true, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.listening.store(false, Ordering::SeqCst);
    }

    pub fn is_listening(&self) -> bool {
        self.listening.load(Ordering::SeqCst)
    }

    pub fn listening_flag(&self) -> Arc<AtomicBool> {
        self.listening.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_state_tracks_listening_flag() {
        let state = ToggleState::new();
        assert!(!state.is_listening());
        state.start();
        assert!(state.is_listening());
        state.stop();
        assert!(!state.is_listening());
    }
}
