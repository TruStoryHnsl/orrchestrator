use orrch_voice::protocol::default_socket_path;
use orrch_voice::service::{VoiceConfig, VoiceService};

fn test_config() -> VoiceConfig {
    VoiceConfig {
        model_id: "test-model".to_string(),
        language: "en".to_string(),
        device_name: None,
        socket_path: default_socket_path().with_file_name("orrch-voice-toggle-api-test.sock"),
        max_utterance_secs: 1,
        chunk_secs: 3.0,
    }
}

#[test]
fn request_voice_toggle_requires_service_then_flips_status() {
    assert_eq!(orrch_voice::request_voice_toggle(), None);

    let _service = VoiceService::new(test_config());

    assert_eq!(orrch_voice::request_voice_toggle(), Some(true));
    let status = orrch_voice::global_voice_status().unwrap();
    assert!(status.lock().unwrap().listening);

    assert_eq!(orrch_voice::request_voice_toggle(), Some(false));
    assert!(!status.lock().unwrap().listening);

    assert_eq!(orrch_voice::set_voice_listening(true), Some(true));
    assert!(status.lock().unwrap().listening);

    assert_eq!(orrch_voice::set_voice_listening(false), Some(false));
    assert!(!status.lock().unwrap().listening);
}
