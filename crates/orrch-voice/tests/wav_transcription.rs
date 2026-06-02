// Ported from mojovoice (MIT, Copyright (c) 2024-2026 itsdevcoffee).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use orrch_voice::capture::resample;
use orrch_voice::engine::{DEFAULT_MODEL_ID, SAMPLE_RATE, VoiceEngine};

#[test]
#[ignore = "downloads Whisper weights and runs real transcription"]
fn harvard_sample_transcribes_expected_words() -> Result<()> {
    let wav_path = sample_path();
    let audio = read_wav_16k_mono(&wav_path)?;
    let mut engine = VoiceEngine::load(DEFAULT_MODEL_ID, "en", None)?;
    let transcript = engine.transcribe(&audio)?;
    let lower = transcript.to_lowercase();

    assert!(!transcript.trim().is_empty());
    assert!(
        ["birch", "canoe", "planks"]
            .iter()
            .any(|word| lower.contains(word)),
        "transcript did not contain expected Harvard words: {transcript}"
    );

    Ok(())
}

fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("samples")
        .join("harvard-list01-female.wav")
}

fn read_wav_16k_mono(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);

    let interleaved = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => {
            let scale = (1_i32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i16>()
                .map(|sample| sample.map(|value| f32::from(value) / scale))
                .collect::<std::result::Result<Vec<_>, _>>()?
        }
        hound::SampleFormat::Int => {
            let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<std::result::Result<Vec<_>, _>>()?
        }
    };

    let mono = if channels <= 1 {
        interleaved
    } else {
        interleaved
            .chunks(channels)
            .filter(|chunk| chunk.len() == channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if spec.sample_rate == SAMPLE_RATE as u32 {
        Ok(mono)
    } else {
        Ok(resample(&mono, spec.sample_rate, SAMPLE_RATE as u32))
    }
}
