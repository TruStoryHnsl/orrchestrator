//! Voice capture and local Whisper transcription for orrchestrator.

use std::sync::{Arc, Mutex, OnceLock};

use crate::toggle::ToggleState;

pub mod capture;
pub mod control_loop;
pub mod device;
pub mod engine;
pub mod intent;
pub mod portal;
pub mod portal_local;
pub mod protocol;
pub mod service;
pub mod toggle;
pub mod tts;
pub mod vocab;

pub type VoiceStatusHandle = Arc<Mutex<VoiceStatusSnapshot>>;

static GLOBAL_VOICE_STATUS: OnceLock<VoiceStatusHandle> = OnceLock::new();
static GLOBAL_VOICE_TOGGLE: OnceLock<ToggleState> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceStatusSnapshot {
    pub listening: bool,
    pub model_ready: bool,
    pub model: String,
    pub device: String,
    pub pending: Option<String>,
    pub partial_transcript: String,
    pub queued: usize,
}

pub fn global_voice_status() -> Option<VoiceStatusHandle> {
    GLOBAL_VOICE_STATUS.get().cloned()
}

pub(crate) fn publish_voice_status(snapshot: VoiceStatusSnapshot) -> VoiceStatusHandle {
    let handle = GLOBAL_VOICE_STATUS
        .get_or_init(|| Arc::new(Mutex::new(snapshot.clone())))
        .clone();
    *handle.lock().unwrap() = snapshot;
    handle
}

pub(crate) fn publish_voice_toggle(toggle: ToggleState) {
    let _ = GLOBAL_VOICE_TOGGLE.set(toggle);
}

pub(crate) fn update_voice_status(update: impl FnOnce(&mut VoiceStatusSnapshot)) {
    if let Some(handle) = GLOBAL_VOICE_STATUS.get() {
        update(&mut handle.lock().unwrap());
    }
}

/// Toggle the voice listen state from another in-process consumer (the TUI).
/// Returns the new listening state, or None if the voice service isn't running.
pub fn request_voice_toggle() -> Option<bool> {
    let toggle = GLOBAL_VOICE_TOGGLE.get()?;
    set_voice_listening(!toggle.is_listening())
}

pub fn set_voice_listening(on: bool) -> Option<bool> {
    let toggle = GLOBAL_VOICE_TOGGLE.get()?;
    if on {
        toggle.start();
    } else {
        toggle.stop();
    }
    update_voice_status(|status| {
        status.listening = on;
        if !on {
            status.partial_transcript.clear();
        }
    });
    Some(on)
}
