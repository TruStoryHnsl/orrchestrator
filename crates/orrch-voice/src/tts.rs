use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use std::sync::Mutex;

use anyhow::{Context, Result};
use tracing::warn;

const PIPER_BIN: &str = "/usr/bin/piper";
const DEFAULT_VOICE: &str = "en_US-lessac-medium";
const PIPER_VOICES_BASE: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main";

pub trait SpeechSink: Send + Sync {
    fn speak(&self, text: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TtsEngine {
    Piper,
    Espeak,
}

#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub engine: TtsEngine,
    pub voice: String,
    pub voice_dir: PathBuf,
}

impl TtsConfig {
    pub fn from_env() -> Self {
        Self {
            engine: select_engine(
                std::env::var("ORRCH_VOICE_TTS_ENGINE")
                    .unwrap_or_else(|_| "piper".to_string())
                    .as_str(),
            ),
            voice: std::env::var("ORRCH_VOICE_TTS_VOICE")
                .unwrap_or_else(|_| DEFAULT_VOICE.to_string()),
            voice_dir: orrchestrator_data_dir().join("piper"),
        }
    }
}

pub struct SystemTts;

impl SpeechSink for SystemTts {
    fn speak(&self, text: &str) {
        speak(text);
    }
}

pub fn speak(text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let config = TtsConfig::from_env();
    let text = text.to_string();
    thread::Builder::new()
        .name("orrch-voice-tts".into())
        .spawn(move || {
            if let Err(err) = speak_blocking(&config, &text) {
                warn!("voice TTS skipped: {err}");
            }
        })
        .ok();
}

fn speak_blocking(config: &TtsConfig, text: &str) -> Result<()> {
    match config.engine {
        TtsEngine::Piper => {
            if try_piper(config, text).is_ok() {
                return Ok(());
            }
            try_espeak(text)
        }
        TtsEngine::Espeak => try_espeak(text),
    }
}

fn try_piper(config: &TtsConfig, text: &str) -> Result<()> {
    if !Path::new(PIPER_BIN).is_file() {
        anyhow::bail!("piper binary not found at {PIPER_BIN}");
    }
    let voice = ensure_piper_voice(config)?;
    let player = find_player().ok_or_else(|| anyhow::anyhow!("no wav player found"))?;
    let wav = std::env::temp_dir().join(format!("orrch-voice-tts-{}.wav", now_ms()));

    let mut child = Command::new(PIPER_BIN)
        .arg("-m")
        .arg(&voice)
        .arg("-f")
        .arg(&wav)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start piper")?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .context("failed to write text to piper")?;
    }
    let status = child.wait().context("failed to wait for piper")?;
    if !status.success() {
        anyhow::bail!("piper exited with {status}");
    }

    let _ = Command::new(player)
        .arg(&wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = std::fs::remove_file(wav);
    Ok(())
}

fn try_espeak(text: &str) -> Result<()> {
    let espeak = find_executable_in_path("espeak-ng")
        .or_else(|| find_executable_in_path("espeak"))
        .ok_or_else(|| anyhow::anyhow!("espeak-ng/espeak not found"))?;
    Command::new(espeak)
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start espeak")?;
    Ok(())
}

fn ensure_piper_voice(config: &TtsConfig) -> Result<PathBuf> {
    let onnx = config.voice_dir.join(format!("{}.onnx", config.voice));
    let json = config.voice_dir.join(format!("{}.onnx.json", config.voice));
    if onnx.is_file() && json.is_file() {
        return Ok(onnx);
    }
    std::fs::create_dir_all(&config.voice_dir)
        .with_context(|| format!("failed to create {}", config.voice_dir.display()))?;

    let remote = piper_voice_remote_path(&config.voice)
        .ok_or_else(|| anyhow::anyhow!("unsupported piper voice id {}", config.voice))?;
    download_if_missing(&format!("{PIPER_VOICES_BASE}/{remote}.onnx"), &onnx)?;
    download_if_missing(&format!("{PIPER_VOICES_BASE}/{remote}.onnx.json"), &json)?;
    Ok(onnx)
}

fn download_if_missing(url: &str, path: &Path) -> Result<()> {
    if path.is_file() {
        return Ok(());
    }
    let bytes = reqwest::blocking::get(url)
        .with_context(|| format!("failed to download {url}"))?
        .error_for_status()
        .with_context(|| format!("failed to download {url}"))?
        .bytes()
        .with_context(|| format!("failed to read {url}"))?;
    std::fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn select_engine(value: &str) -> TtsEngine {
    match value.trim().to_lowercase().as_str() {
        "espeak" | "espeak-ng" => TtsEngine::Espeak,
        _ => TtsEngine::Piper,
    }
}

pub fn build_piper_command(voice: &Path, wav: &Path) -> Command {
    let mut command = Command::new(PIPER_BIN);
    command.arg("-m").arg(voice).arg("-f").arg(wav);
    command
}

pub fn build_espeak_command(binary: &Path, text: &str) -> Command {
    let mut command = Command::new(binary);
    command.arg(text);
    command
}

fn find_player() -> Option<PathBuf> {
    ["pw-play", "paplay", "aplay"]
        .iter()
        .find_map(|name| find_executable_in_path(name))
}

fn find_executable_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

fn piper_voice_remote_path(voice: &str) -> Option<String> {
    let mut parts = voice.rsplitn(3, '-').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    parts.reverse();
    let locale = parts[0];
    let name = parts[1];
    let quality = parts[2];
    let language = locale.split('_').next()?;
    Some(format!("{language}/{locale}/{name}/{quality}/{voice}"))
}

fn orrchestrator_data_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("orrchestrator")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
#[derive(Default)]
pub struct MockSpeechSink {
    spoken: Mutex<Vec<String>>,
}

#[cfg(test)]
impl MockSpeechSink {
    pub fn spoken(&self) -> Vec<String> {
        self.spoken.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl SpeechSink for MockSpeechSink {
    fn speak(&self, text: &str) {
        self.spoken.lock().unwrap().push(text.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn selects_requested_engine() {
        assert_eq!(select_engine("espeak"), TtsEngine::Espeak);
        assert_eq!(select_engine("piper"), TtsEngine::Piper);
        assert_eq!(select_engine("unknown"), TtsEngine::Piper);
    }

    #[test]
    fn builds_piper_command() {
        let command = build_piper_command(Path::new("/tmp/voice.onnx"), Path::new("/tmp/out.wav"));
        let args = command
            .get_args()
            .map(|arg| arg.as_bytes().to_vec())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                b"-m".to_vec(),
                b"/tmp/voice.onnx".to_vec(),
                b"-f".to_vec(),
                b"/tmp/out.wav".to_vec()
            ]
        );
    }

    #[test]
    fn builds_espeak_command() {
        let command = build_espeak_command(Path::new("/usr/bin/espeak-ng"), "hello");
        let args = command
            .get_args()
            .map(|arg| arg.as_bytes().to_vec())
            .collect::<Vec<_>>();

        assert_eq!(command.get_program().as_bytes(), b"/usr/bin/espeak-ng");
        assert_eq!(args, vec![b"hello".to_vec()]);
    }

    #[test]
    fn maps_default_piper_voice_to_hf_path() {
        assert_eq!(
            piper_voice_remote_path("en_US-lessac-medium").unwrap(),
            "en/en_US/lessac/medium/en_US-lessac-medium"
        );
    }
}
