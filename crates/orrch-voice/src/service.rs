use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tracing::{error, info, warn};

use crate::capture::{TARGET_SAMPLE_RATE, capture_stream};
use crate::engine::{DEFAULT_MODEL_ID, VoiceEngine};
use crate::protocol::{Utterance, VoiceRequest, VoiceResponse, default_socket_path};
use crate::toggle::ToggleState;
use crate::vocab::VocabStore;
use crate::{VoiceStatusSnapshot, publish_voice_status, publish_voice_toggle, update_voice_status};

#[derive(Debug, Clone)]
pub struct VoiceConfig {
    pub model_id: String,
    pub language: String,
    pub device_name: Option<String>,
    pub socket_path: PathBuf,
    pub max_utterance_secs: u32,
    pub chunk_secs: f32,
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
            chunk_secs: std::env::var("ORRCH_VOICE_CHUNK_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .filter(|secs: &f32| *secs > 0.0)
                .unwrap_or(3.0),
        }
    }
}

const STREAM_TAIL_OVERLAP_SECS: f32 = 0.4;

type UtteranceQueue = Arc<(Mutex<VecDeque<Utterance>>, Condvar)>;
type UtteranceSubscribers = Arc<Mutex<Vec<mpsc::Sender<Utterance>>>>;

#[derive(Clone)]
pub struct VoiceService {
    config: VoiceConfig,
    toggle: ToggleState,
    queue: UtteranceQueue,
    subscribers: UtteranceSubscribers,
    model_ready: Arc<AtomicBool>,
    engine: Arc<Mutex<Option<VoiceEngine>>>,
}

impl VoiceService {
    pub fn new(config: VoiceConfig) -> Self {
        let service = Self {
            config,
            toggle: ToggleState::new(),
            queue: Arc::new((Mutex::new(VecDeque::new()), Condvar::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            model_ready: Arc::new(AtomicBool::new(false)),
            engine: Arc::new(Mutex::new(None)),
        };
        publish_voice_toggle(service.toggle.clone());
        publish_voice_status(service.status_snapshot());
        service
    }

    pub fn run(config: VoiceConfig) {
        let service = Self::new(config);
        if let Err(err) = service.start() {
            warn!("orrch-voice socket server exited: {err}");
        }
    }

    pub fn start(&self) -> Result<()> {
        self.spawn_model_loader();
        self.spawn_capture_loop();
        self.serve_socket()
    }

    /// Subscribe to in-process utterance delivery without consuming the socket
    /// queue used by `VoiceRequest::NextUtterance`.
    ///
    /// When the control loop is enabled it should use this fan-out as the
    /// primary utterance stream. The socket queue remains intact for manual
    /// tools and session-driven polling.
    pub fn subscribe_utterances(&self) -> mpsc::Receiver<Utterance> {
        let (tx, rx) = mpsc::channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }

    pub fn serve_socket(&self) -> Result<()> {
        bind_socket(&self.config.socket_path)
            .with_context(|| {
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

    fn status_snapshot(&self) -> VoiceStatusSnapshot {
        let (queue, _) = &*self.queue;
        let queued = queue.lock().unwrap().len();
        let model_ready = self.model_ready.load(Ordering::SeqCst);
        let device = self
            .engine
            .lock()
            .unwrap()
            .as_ref()
            .map(|engine| engine.device_label().to_string())
            .or_else(|| self.config.device_name.clone())
            .unwrap_or_else(|| "loading".to_string());
        VoiceStatusSnapshot {
            listening: self.toggle.is_listening(),
            model_ready,
            model: self.config.model_id.clone(),
            device,
            pending: None,
            partial_transcript: String::new(),
            queued,
        }
    }

    fn refresh_status(&self) {
        let snapshot = self.status_snapshot();
        update_voice_status(|status| {
            status.listening = snapshot.listening;
            status.model_ready = snapshot.model_ready;
            status.model = snapshot.model;
            status.device = snapshot.device;
            status.queued = snapshot.queued;
        });
    }

    fn start_listening(&self) {
        self.toggle.start();
        update_voice_status(|status| status.partial_transcript.clear());
        self.refresh_status();
    }

    fn stop_listening(&self) {
        self.toggle.stop();
        update_voice_status(|status| status.partial_transcript.clear());
        self.refresh_status();
    }

    fn toggle_listening(&self) {
        if self.toggle.is_listening() {
            self.stop_listening();
        } else {
            self.start_listening();
        }
    }

    fn spawn_model_loader(&self) {
        let model_id = self.config.model_id.clone();
        let language = self.config.language.clone();
        let engine = self.engine.clone();
        let model_ready = self.model_ready.clone();
        if let Err(err) = std::thread::Builder::new()
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
                        let device = loaded.device_label().to_string();
                        *engine.lock().unwrap() = Some(loaded);
                        model_ready.store(true, Ordering::SeqCst);
                        update_voice_status(|status| {
                            status.model_ready = true;
                            status.model = model_id.clone();
                            status.device = device;
                        });
                        info!("orrch-voice model ready");
                    }
                    Err(err) => {
                        update_voice_status(|status| {
                            status.model_ready = false;
                            status.model = model_id.clone();
                            status.device = "unavailable".to_string();
                        });
                        warn!("failed to load orrch-voice model '{model_id}': {err}");
                    }
                }
            })
        {
            error!("failed to spawn orrch-voice-model thread: {err}");
        }
    }

    fn spawn_capture_loop(&self) {
        let service = self.clone();
        if let Err(err) = std::thread::Builder::new()
            .name("orrch-voice-capture".into())
            .spawn(move || service.capture_loop())
        {
            error!("failed to spawn orrch-voice-capture thread: {err}");
        }
    }

    fn capture_loop(&self) {
        loop {
            if !self.toggle.is_listening() {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            if let Err(err) = self.streaming_capture_once() {
                warn!("voice capture failed: {err}");
                self.stop_listening();
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }

    fn streaming_capture_once(&self) -> Result<()> {
        if !self.model_ready.load(Ordering::SeqCst) {
            warn!("voice listening requested before model was ready");
            self.stop_listening();
            return Ok(());
        }

        let (stream, frames) = capture_stream(self.config.device_name.as_deref())?;
        let chunk_samples =
            ((self.config.chunk_secs * TARGET_SAMPLE_RATE as f32).round() as usize).max(1);
        let tail_samples = ((STREAM_TAIL_OVERLAP_SECS * TARGET_SAMPLE_RATE as f32).round()
            as usize)
            .min(chunk_samples.saturating_sub(1));
        let max_samples = self.config.max_utterance_secs as usize * TARGET_SAMPLE_RATE as usize;
        let mut segment = Vec::with_capacity(chunk_samples);
        let mut full = Vec::with_capacity(max_samples.min(chunk_samples * 2));
        let mut new_since_last_segment = 0usize;
        let mut transcript = StreamingTranscript::default();

        info!(
            "Streaming voice capture active: chunk={:.2}s tail={:.2}s max={}s",
            self.config.chunk_secs, STREAM_TAIL_OVERLAP_SECS, self.config.max_utterance_secs
        );

        while self.toggle.is_listening() {
            match frames.recv_timeout(Duration::from_millis(50)) {
                Ok(frame) => {
                    new_since_last_segment += frame.len();
                    full.extend_from_slice(&frame);
                    segment.extend(frame);

                    while segment.len() >= chunk_samples {
                        self.transcribe_stream_segment(&segment, &mut transcript);
                        self.publish_partial_transcript(&transcript);

                        if tail_samples > 0 && segment.len() > tail_samples {
                            segment = segment[segment.len() - tail_samples..].to_vec();
                        } else {
                            segment.clear();
                        }
                        new_since_last_segment = 0;
                    }

                    if full.len() >= max_samples {
                        info!("voice capture reached max utterance duration");
                        self.toggle.stop();
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        stream.stop();
        drop(stream);
        while let Ok(frame) = frames.try_recv() {
            new_since_last_segment += frame.len();
            full.extend_from_slice(&frame);
            segment.extend(frame);
        }

        if new_since_last_segment > 0 && !segment.is_empty() {
            self.transcribe_stream_segment(&segment, &mut transcript);
        }

        let final_text = transcript.finish();
        update_voice_status(|status| {
            status.listening = false;
            status.partial_transcript.clear();
        });

        if final_text.is_empty() {
            self.refresh_status();
        } else {
            self.push_utterance(final_text);
        }

        Ok(())
    }

    fn transcribe_stream_segment(&self, segment: &[f32], transcript: &mut StreamingTranscript) {
        let text = {
            let mut guard = self.engine.lock().unwrap();
            match guard.as_mut() {
                Some(engine) => engine.transcribe(segment),
                None => Ok(String::new()),
            }
        };

        match text {
            Ok(text) => transcript.append_segment(&text),
            Err(err) => warn!("voice transcription failed: {err}"),
        }
    }

    fn publish_partial_transcript(&self, transcript: &StreamingTranscript) {
        let partial = transcript.current().to_string();
        if !partial.is_empty() {
            update_voice_status(|status| status.partial_transcript = partial);
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
                self.toggle_listening();
                VoiceResponse::Ok
            }
            VoiceRequest::Start => {
                self.start_listening();
                VoiceResponse::Ok
            }
            VoiceRequest::Stop => {
                self.stop_listening();
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
        queue.lock().unwrap().push_back(utterance.clone());
        condvar.notify_one();
        self.refresh_status();

        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|tx| tx.send(utterance.clone()).is_ok());
    }

    fn next_utterance(&self, timeout_ms: u64) -> Option<Utterance> {
        let (queue, condvar) = &*self.queue;
        let mut guard = queue.lock().unwrap();
        if let Some(utterance) = guard.pop_front() {
            drop(guard);
            self.refresh_status();
            return Some(utterance);
        }

        let timeout = Duration::from_millis(timeout_ms);
        let (mut guard, _) = condvar
            .wait_timeout_while(guard, timeout, |queue| queue.is_empty())
            .unwrap();
        let utterance = guard.pop_front();
        drop(guard);
        self.refresh_status();
        utterance
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

#[derive(Default)]
struct StreamingTranscript {
    text: String,
}

impl StreamingTranscript {
    fn append_segment(&mut self, text: &str) {
        append_stitched_text(&mut self.text, text);
    }

    fn current(&self) -> &str {
        self.text.trim()
    }

    fn finish(self) -> String {
        self.text.trim().to_string()
    }
}

fn append_stitched_text(transcript: &mut String, next: &str) {
    let next = next.trim();
    if next.is_empty() {
        return;
    }

    if transcript.trim().is_empty() {
        transcript.clear();
        transcript.push_str(next);
        return;
    }

    let existing_words = transcript.split_whitespace().collect::<Vec<_>>();
    let next_words = next.split_whitespace().collect::<Vec<_>>();
    let overlap = repeated_boundary_words(&existing_words, &next_words);
    let remainder = next_words[overlap..].join(" ");
    if remainder.is_empty() {
        return;
    }

    transcript.push(' ');
    transcript.push_str(&remainder);
}

fn repeated_boundary_words(existing_words: &[&str], next_words: &[&str]) -> usize {
    let max = existing_words.len().min(next_words.len()).min(6);
    (1..=max)
        .rev()
        .find(|&len| {
            existing_words[existing_words.len() - len..]
                .iter()
                .zip(&next_words[..len])
                .all(|(left, right)| {
                    normalize_boundary_word(left) == normalize_boundary_word(right)
                })
        })
        .unwrap_or(0)
}

fn normalize_boundary_word(word: &str) -> String {
    word.trim_matches(|ch: char| !ch.is_alphanumeric())
        .to_ascii_lowercase()
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
            chunk_secs: 3.0,
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

        let response = send(
            &socket_path,
            VoiceRequest::NextUtterance { timeout_ms: 250 },
        );
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
    fn subscriber_receives_utterance_without_consuming_socket_queue() {
        let temp = tempfile::tempdir().unwrap();
        let socket_path = temp.path().join("voice.sock");
        let service = VoiceService::new(test_config(socket_path.clone()));
        let rx = service.subscribe_utterances();

        service.push_utterance_for_test("fan out");

        let delivered = rx.recv_timeout(Duration::from_millis(250)).unwrap();
        assert_eq!(delivered.text, "fan out");

        let response = service.handle_request(VoiceRequest::NextUtterance { timeout_ms: 1 });
        match response {
            VoiceResponse::Utterance(Some(utterance)) => assert_eq!(utterance.text, "fan out"),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn next_utterance_empty_queue_times_out() {
        let service = VoiceService::new(test_config(PathBuf::from("/tmp/not-used.sock")));
        let response = service.handle_request(VoiceRequest::NextUtterance { timeout_ms: 1 });
        assert_eq!(response, VoiceResponse::Utterance(None));
    }

    #[test]
    fn streaming_transcript_stitches_incremental_segments() {
        let mut transcript = StreamingTranscript::default();
        for segment in ["spawn a", "session in", "orrchestrator"] {
            transcript.append_segment(segment);
        }

        assert_eq!(transcript.finish(), "spawn a session in orrchestrator");
    }

    #[test]
    fn streaming_transcript_deduplicates_overlap_boundary() {
        let mut transcript = StreamingTranscript::default();
        transcript.append_segment("spawn a session");
        transcript.append_segment("a session in orrchestrator");

        assert_eq!(transcript.current(), "spawn a session in orrchestrator");
    }

    #[test]
    fn streaming_transcript_finalize_appends_last_partial() {
        let mut transcript = StreamingTranscript::default();
        transcript.append_segment("spawn a");
        transcript.append_segment("session in");
        transcript.append_segment("orrchestrator");

        assert_eq!(transcript.finish(), "spawn a session in orrchestrator");
    }
}
