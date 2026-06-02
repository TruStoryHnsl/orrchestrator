use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceRequest {
    Ping,
    Status,
    Toggle,
    Start,
    Stop,
    NextUtterance { timeout_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Utterance {
    pub text: String,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceResponse {
    Pong,
    Status {
        listening: bool,
        model_ready: bool,
        model: String,
        device: String,
        queued: usize,
    },
    Utterance(Option<Utterance>),
    Ok,
    Error(String),
}

pub fn default_socket_path() -> PathBuf {
    orrch_core::data_dir().join("voice.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_variants_round_trip_json() {
        let requests = [
            VoiceRequest::Ping,
            VoiceRequest::Status,
            VoiceRequest::Toggle,
            VoiceRequest::Start,
            VoiceRequest::Stop,
            VoiceRequest::NextUtterance { timeout_ms: 30_000 },
        ];

        for request in requests {
            let json = serde_json::to_string(&request).unwrap();
            let decoded: VoiceRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, request);
        }
    }

    #[test]
    fn response_variants_round_trip_json() {
        let responses = [
            VoiceResponse::Pong,
            VoiceResponse::Status {
                listening: true,
                model_ready: false,
                model: "openai/whisper-base.en".to_string(),
                device: "loading".to_string(),
                queued: 2,
            },
            VoiceResponse::Utterance(Some(Utterance {
                text: "hello".to_string(),
                ts_ms: 42,
            })),
            VoiceResponse::Utterance(None),
            VoiceResponse::Ok,
            VoiceResponse::Error("nope".to_string()),
        ];

        for response in responses {
            let json = serde_json::to_string(&response).unwrap();
            let decoded: VoiceResponse = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, response);
        }
    }
}
