use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::capture::capture_toggle;
use crate::engine::{DEFAULT_MODEL_ID, VoiceEngine};
use crate::protocol::{Utterance, VoiceRequest, VoiceResponse, default_socket_path};
use crate::toggle::ToggleState;
use crate::vocab::VocabStore;

#[derive(Debug, Clone)]
pub struct VoiceConfig {
    pub model_id: String,
    pub language: String,
    pub device_name: Option<String>,
    pub socket_path: PathBuf,
    pub max_utterance_secs: u32,
}

impl VoiceConfig {
    pub fn from_env() -> Self {
        Self {
            model_id: std::env::var("ORRCH_VOICE_MODEL")
                .unwrap_or_else(|_| DEFAULT_MODEL_ID.to_string()),
            language: std::env::var("ORRCH_VOICE_LANGUAGE").unwrap_or_else(|_| "en".to_string()),
            device_name: std::env::var("ORRCH_VOICE_DEVICE")
                .ok()
                .filter(|s| !s.trim().is_empty()),
            socket_path: std::env::var("ORRCH_VOICE_SOCKET")
                .map(PathBuf::from)
                .unwrap_or_else(|_| default_socket_path()),
            max_utterance_secs: std::env::var("ORRCH_VOICE_MAX_UTTERANCE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
        }
    }
}

type UtteranceQueue = Arc<(Mutex<VecDeque<Utterance>>, Condvar)>;

#[derive(Clone)]
pub struct VoiceService {
    config: VoiceConfig,
    toggle: ToggleState,
    queue: UtteranceQueue,
    model_ready: Arc<AtomicBool>,
    engine: Arc<Mutex<Option<VoiceEngine>>>,
}

impl VoiceService {
    pub fn new(config: VoiceConfig) -> Self {
        Self {
            config,
            toggle: ToggleState::new(),
            queue: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            model_ready: Arc::new(AtomicBool::new(false)),
            engine: Arc::new(Mutex::new(None)),
        }
    }

    pub fn run(config: VoiceConfig) {
        let service = Self::new(config);
        service.spawn_model_loader();
        service.spawn_capture_loop();
        if let Err(err) = service.serve_socket() {
            warn!("orrch-voice socket server exited: {err}");
        }
    }

    pub fn serve_socket(&self) -> Result<()> {
        bind_socket(&self.config.socket_path).with_context(|| {
            format!(
                "failed to bind voice socket {}",
                self.config.socket_path.display()
            )
        })?
        .incoming()
        .for_each(|stream| match stream {
            Ok(stream) => {
                let service = self.clone();
                std::thread::spawn(move || {
                    if let Err(err) = service.handle_connection(stream) {
                        warn!("voice socket connection failed: {err}");
                    }
                });
            }
            Err(err) => warn!("voice socket accept failed: {err}"),
        });
        Ok(())
    }

    pub fn push_utterance_for_test(&self, text: &str) {
        self.push_utterance(text.to_string());
    }

    fn spawn_model_loader(&self) {
        let model_id = self.config.model_id.clone();
        let language = self.config.language.clone();
        let engine = self.engine.clone();
        let model_ready = self.model_ready.clone();
        std::thread::Builder::new()
            .name("orrch-voice-model".into())
            .spawn(move || {
                let prompt = VocabStore::open()
                    .and_then(|store| store.get_prompt_string(224))
                    .unwrap_or_else(|err| {
                        warn!("failed to load voice vocabulary prompt: {err}");
                        None
                    });
                match VoiceEngine::load(&model_id, &language, prompt) {
                    Ok(loaded) => {
                        *engine.lock().unwrap() = Some(loaded);
                        model_ready.store(true, Ordering::SeqCst);
                        info!("orrch-voice model ready");
                    }
                    Err(err) => warn!("failed to load orrch-voice model '{model_id}': {err}"),
                }
            })
            .ok();
    }

    fn spawn_capture_loop(&self) {
        let service = self.clone();
        std::thread::Builder::new()
            .name("orrch-voice-capture".into())
            .spawn(move || service.capture_loop())
            .ok();
    }

    fn capture_loop(&self) {
        loop {
            if !self.toggle.is_listening() {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            let stop = Arc::new(AtomicBool::new(false));
            let stop_watcher = stop.clone();
            let toggle_watcher = self.toggle.clone();
            std::thread::spawn(move || {
                while toggle_watcher.is_listening() {
                    std::thread::sleep(Duration::from_millis(50));
                }
                stop_watcher.store(true, Ordering::SeqCst);
            });

            match capture_toggle(
                self.config.max_utterance_secs,
                self.config.device_name.as_deref(),
                stop,
            ) {
                Ok(audio) if audio.is_empty() => self.toggle.stop(),
                Ok(audio) if self.model_ready.load(Ordering::SeqCst) => {
                    let text = {
                        let mut guard = self.engine.lock().unwrap();
                        match guard.as_mut() {
                            Some(engine) => engine.transcribe(&audio),
                            None => Ok(String::new()),
                        }
                    };
                    match text {
                        Ok(text) if !text.trim().is_empty() => self.push_utterance(text),
                        Ok(_) => {}
                        Err(err) => warn!("voice transcription failed: {err}"),
                    }
                    self.toggle.stop();
                }
                Ok(_) => {
                    warn!("voice audio captured before model was ready");
                    self.toggle.stop();
                }
                Err(err) => {
                    warn!("voice capture failed: {err}");
                    self.toggle.stop();
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }

    fn handle_connection(&self, stream: UnixStream) -> Result<()> {
        let reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let response = match serde_json::from_str::<VoiceRequest>(&line) {
                Ok(request) => self.handle_request(request),
                Err(err) => VoiceResponse::Error(format!("invalid request: {err}")),
            };
            serde_json::to_writer(&mut writer, &response)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
        Ok(())
    }

    fn handle_request(&self, request: VoiceRequest) -> VoiceResponse {
        match request {
            VoiceRequest::Ping => VoiceResponse::Pong,
            VoiceRequest::Status => {
                let (queue, _) = &*self.queue;
                let queued = queue.lock().unwrap().len();
                let model_ready = self.model_ready.load(Ordering::SeqCst);
                let device = self
                    .engine
                    .lock()
                    .unwrap()
                    .as_ref()
                    .map(|engine| engine.device_label().to_string())
                    .unwrap_or_else(|| "loading".to_string());
                VoiceResponse::Status {
                    listening: self.toggle.is_listening(),
                    model_ready,
                    model: self.config.model_id.clone(),
                    device,
                    queued,
                }
            }
            VoiceRequest::Toggle => {
                if self.toggle.is_listening() {
                    self.toggle.stop();
                } else {
                    self.toggle.start();
                }
                VoiceResponse::Ok
            }
            VoiceRequest::Start => {
                self.toggle.start();
                VoiceResponse::Ok
            }
            VoiceRequest::Stop => {
                self.toggle.stop();
                VoiceResponse::Ok
            }
            VoiceRequest::NextUtterance { timeout_ms } => {
                VoiceResponse::Utterance(self.next_utterance(timeout_ms))
            }
        }
    }

    fn push_utterance(&self, text: String) {
        let utterance = Utterance {
            text,
            ts_ms: now_ms(),
        };
        let (queue, condvar) = &*self.queue;
        queue.lock().unwrap().push_back(utterance);
        condvar.notify_one();
    }

    fn next_utterance(&self, timeout_ms: u64) -> Option<Utterance> {
        let (queue, condvar) = &*self.queue;
        let mut guard = queue.lock().unwrap();
        if let Some(utterance) = guard.pop_front() {
            return Some(utterance);
        }

        let timeout = Duration::from_millis(timeout_ms);
        let (mut guard, _) = condvar
            .wait_timeout_while(guard, timeout, |queue| queue.is_empty())
            .unwrap();
        guard.pop_front()
    }
}

fn bind_socket(path: &PathBuf) -> Result<UnixListener> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create socket dir {}", parent.display()))?;
    }
    if path.exists() {
        std::fs::remove_file(path)
            .with_context(|| format!("failed to remove stale socket {}", path.display()))?;
    }
    UnixListener::bind(path).map_err(Into::into)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    use super::*;
    use crate::protocol::{VoiceRequest, VoiceResponse};

    fn test_config(socket_path: PathBuf) -> VoiceConfig {
        VoiceConfig {
            model_id: "test-model".to_string(),
            language: "en".to_string(),
            device_name: None,
            socket_path,
            max_utterance_secs: 1,
        }
    }

    fn send(path: &PathBuf, request: VoiceRequest) -> VoiceResponse {
        let mut stream = UnixStream::connect(path).unwrap();
        serde_json::to_writer(&mut stream, &request).unwrap();
        stream.write_all(b"\n").unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    #[test]
    fn socket_returns_injected_utterance_and_status() {
        let temp = tempfile::tempdir().unwrap();
        let socket_path = temp.path().join("voice.sock");
        let service = VoiceService::new(test_config(socket_path.clone()));
        let socket_service = service.clone();
        std::thread::spawn(move || socket_service.serve_socket().unwrap());

        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        service.push_utterance_for_test("hello from test");

        let response = send(&socket_path, VoiceRequest::NextUtterance { timeout_ms: 250 });
        match response {
            VoiceResponse::Utterance(Some(utterance)) => {
                assert_eq!(utterance.text, "hello from test");
            }
            other => panic!("unexpected response: {other:?}"),
        }

        let response = send(&socket_path, VoiceRequest::Status);
        match response {
            VoiceResponse::Status {
                listening,
                model_ready,
                model,
                queued,
                ..
            } => {
                assert!(!listening);
                assert!(!model_ready);
                assert_eq!(model, "test-model");
                assert_eq!(queued, 0);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn next_utterance_empty_queue_times_out() {
        let service = VoiceService::new(test_config(PathBuf::from("/tmp/not-used.sock")));
        let response = service.handle_request(VoiceRequest::NextUtterance { timeout_ms: 1 });
        assert_eq!(response, VoiceResponse::Utterance(None));
    }
}
