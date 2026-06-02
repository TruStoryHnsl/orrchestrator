//! Voice capture and local Whisper transcription for orrchestrator.

use std::sync::{Arc, Mutex, OnceLock};

pub mod capture;
pub mod control_loop;
pub mod device;
pub mod engine;
pub mod intent;
pub mod protocol;
pub mod service;
pub mod toggle;
pub mod vocab;

pub type VoiceStatusHandle = Arc<Mutex<VoiceStatusSnapshot>>;

static GLOBAL_VOICE_STATUS: OnceLock<VoiceStatusHandle> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceStatusSnapshot {
    pub listening: bool,
    pub model_ready: bool,
    pub model: String,
    pub device: String,
    pub pending: Option<String>,
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

pub(crate) fn update_voice_status(update: impl FnOnce(&mut VoiceStatusSnapshot)) {
    if let Some(handle) = GLOBAL_VOICE_STATUS.get() {
        update(&mut handle.lock().unwrap());
    }
}
