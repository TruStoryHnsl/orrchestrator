// Ported from mojovoice (MIT, Copyright (c) 2024-2026 itsdevcoffee).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};
use rubato::{FftFixedIn, Resampler};
use tracing::{info, warn};

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

struct AudioSetup {
    device: Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    sample_rate: u32,
    channels: u16,
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|device| device.name().ok()).collect(),
        Err(err) => {
            warn!("Failed to enumerate input devices: {err}");
            Vec::new()
        }
    }
}

pub fn capture(duration_secs: u32, device_name: Option<&str>) -> Result<Vec<f32>> {
    info!("Starting fixed audio capture: {duration_secs}s");
    let setup = setup_audio_device(device_name)?;
    let expected_samples =
        setup.sample_rate as usize * duration_secs as usize * usize::from(setup.channels);
    let buffer = Arc::new(Mutex::new(Vec::with_capacity(expected_samples)));
    let started = Arc::new(AtomicBool::new(false));

    let stream = build_capture_stream(&setup, buffer.clone(), started)?;
    std::thread::sleep(Duration::from_secs(duration_secs.into()));
    drop(stream);

    finish_capture(buffer, &setup)
}

pub fn capture_toggle(
    max_secs: u32,
    device_name: Option<&str>,
    stop: Arc<AtomicBool>,
) -> Result<Vec<f32>> {
    info!("Starting toggle audio capture: max {max_secs}s");
    let setup = setup_audio_device(device_name)?;
    let expected_samples =
        setup.sample_rate as usize * max_secs as usize * usize::from(setup.channels);
    let buffer = Arc::new(Mutex::new(Vec::with_capacity(expected_samples)));
    let started = Arc::new(AtomicBool::new(false));
    let stream = build_capture_stream(&setup, buffer.clone(), started)?;
    let start = Instant::now();
    let max_duration = Duration::from_secs(max_secs.into());

    while start.elapsed() < max_duration {
        if stop.load(Ordering::SeqCst) {
            info!("Toggle capture stop requested");
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    std::thread::sleep(Duration::from_secs(1));
    drop(stream);

    finish_capture(buffer, &setup)
}

fn setup_audio_device(device_name: Option<&str>) -> Result<AudioSetup> {
    let host = cpal::default_host();
    let device = match device_name {
        Some(name) => find_input_device_by_name(&host, name)?,
        None => host
            .default_input_device()
            .context("no default input device available")?,
    };

    let supported_config = device
        .default_input_config()
        .context("failed to get default input config")?;
    let setup = setup_from_supported_config(device, supported_config);

    info!(
        "Audio input: {} ({}Hz, {}ch, {})",
        setup
            .device
            .name()
            .unwrap_or_else(|_| "unknown".to_string()),
        setup.sample_rate,
        setup.channels,
        setup.sample_format
    );

    Ok(setup)
}

fn find_input_device_by_name(host: &cpal::Host, name: &str) -> Result<Device> {
    let devices: Vec<_> = host
        .input_devices()
        .context("failed to enumerate input devices")?
        .collect();

    devices
        .into_iter()
        .find(|device| device.name().ok().as_deref() == Some(name))
        .ok_or_else(|| {
            let available = list_input_devices();
            anyhow::anyhow!("audio input device '{name}' not found; available: {available:?}")
        })
}

fn setup_from_supported_config(
    device: Device,
    supported_config: SupportedStreamConfig,
) -> AudioSetup {
    AudioSetup {
        config: supported_config.config(),
        sample_format: supported_config.sample_format(),
        sample_rate: supported_config.sample_rate().0,
        channels: supported_config.channels(),
        device,
    }
}

fn build_capture_stream(
    setup: &AudioSetup,
    buffer: Arc<Mutex<Vec<f32>>>,
    started: Arc<AtomicBool>,
) -> Result<Stream> {
    match setup.sample_format {
        SampleFormat::I8 => build_typed_capture_stream::<i8>(setup, buffer, started),
        SampleFormat::I16 => build_typed_capture_stream::<i16>(setup, buffer, started),
        SampleFormat::I24 => build_typed_capture_stream::<cpal::I24>(setup, buffer, started),
        SampleFormat::I32 => build_typed_capture_stream::<i32>(setup, buffer, started),
        SampleFormat::I64 => build_typed_capture_stream::<i64>(setup, buffer, started),
        SampleFormat::U8 => build_typed_capture_stream::<u8>(setup, buffer, started),
        SampleFormat::U16 => build_typed_capture_stream::<u16>(setup, buffer, started),
        SampleFormat::U32 => build_typed_capture_stream::<u32>(setup, buffer, started),
        SampleFormat::U64 => build_typed_capture_stream::<u64>(setup, buffer, started),
        SampleFormat::F32 => build_typed_capture_stream::<f32>(setup, buffer, started),
        SampleFormat::F64 => build_typed_capture_stream::<f64>(setup, buffer, started),
        other => anyhow::bail!("unsupported input sample format: {other}"),
    }
}

fn build_typed_capture_stream<T>(
    setup: &AudioSetup,
    buffer: Arc<Mutex<Vec<f32>>>,
    started: Arc<AtomicBool>,
) -> Result<Stream>
where
    T: Sample + cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let stream = setup.device.build_input_stream(
        &setup.config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if !started.swap(true, Ordering::Relaxed) {
                info!("Audio capture started");
            }
            let mut buffer = buffer.lock().unwrap();
            buffer.extend(data.iter().map(|sample| sample.to_sample::<f32>()));
        },
        |err| warn!("Audio input stream error: {err}"),
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

fn finish_capture(buffer: Arc<Mutex<Vec<f32>>>, setup: &AudioSetup) -> Result<Vec<f32>> {
    let samples = Arc::try_unwrap(buffer)
        .map(|mutex| mutex.into_inner().unwrap())
        .unwrap_or_else(|buffer| buffer.lock().unwrap().clone());
    let duration = samples.len() as f32 / (setup.sample_rate * u32::from(setup.channels)) as f32;
    info!(
        "Captured {} interleaved samples ({duration:.2}s)",
        samples.len()
    );

    let mono = to_mono(samples, setup.channels);
    finalize_audio_samples(mono, setup.sample_rate, TARGET_SAMPLE_RATE)
}

fn to_mono(samples: Vec<f32>, channels: u16) -> Vec<f32> {
    let channels = usize::from(channels);
    if channels <= 1 {
        return samples;
    }

    samples
        .chunks(channels)
        .filter(|chunk| chunk.len() == channels)
        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn finalize_audio_samples(
    samples: Vec<f32>,
    source_rate: u32,
    target_rate: u32,
) -> Result<Vec<f32>> {
    if samples.is_empty() {
        warn!("No audio captured");
        return Ok(Vec::new());
    }

    let samples = if source_rate == target_rate {
        samples
    } else {
        info!("Resampling audio: {source_rate}Hz -> {target_rate}Hz");
        resample(&samples, source_rate, target_rate)
    };

    info!(
        "Final audio: {} samples ({:.2}s at {}Hz mono)",
        samples.len(),
        samples.len() as f32 / target_rate as f32,
        target_rate
    );
    Ok(samples)
}

pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    let samples_f64: Vec<f64> = samples.iter().map(|sample| f64::from(*sample)).collect();
    let chunk_size = 1024;
    let mut resampler =
        match FftFixedIn::<f64>::new(from_rate as usize, to_rate as usize, chunk_size, 2, 1) {
            Ok(resampler) => resampler,
            Err(err) => {
                warn!("Rubato resampler init failed: {err}; using linear fallback");
                return resample_linear(samples, from_rate as f32 / to_rate as f32);
            }
        };

    let mut output = Vec::new();
    for chunk in samples_f64.chunks(chunk_size) {
        let mut input = chunk.to_vec();
        if input.len() < chunk_size {
            input.resize(chunk_size, 0.0);
        }

        match resampler.process(&[input], None) {
            Ok(channels) => {
                if let Some(channel) = channels.first() {
                    output.extend(channel.iter().copied());
                }
            }
            Err(err) => {
                warn!("Rubato resample failed: {err}; using linear fallback");
                return resample_linear(samples, from_rate as f32 / to_rate as f32);
            }
        }
    }

    output.into_iter().map(|sample| sample as f32).collect()
}

fn resample_linear(samples: &[f32], ratio: f32) -> Vec<f32> {
    let output_len = (samples.len() as f32 / ratio) as usize;
    (0..output_len)
        .map(|idx| {
            let src_pos = idx as f32 * ratio;
            let src_idx = src_pos as usize;
            let frac = src_pos - src_idx as f32;

            if src_idx + 1 < samples.len() {
                samples[src_idx] * (1.0 - frac) + samples[src_idx + 1] * frac
            } else if src_idx < samples.len() {
                samples[src_idx]
            } else {
                0.0
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_samples_are_averaged_to_mono() {
        let mono = to_mono(vec![1.0, -1.0, 0.5, 0.25], 2);
        assert_eq!(mono, vec![0.0, 0.375]);
    }

    #[test]
    fn linear_resample_downsamples() {
        let samples: Vec<f32> = (0..100).map(|idx| idx as f32).collect();
        let out = resample_linear(&samples, 2.0);
        assert!(out.len() < samples.len());
    }

    #[test]
    fn linear_resample_upsamples() {
        let samples = vec![0.0, 1.0, 0.0, -1.0];
        let out = resample_linear(&samples, 0.5);
        assert!(out.len() > samples.len());
    }
}
