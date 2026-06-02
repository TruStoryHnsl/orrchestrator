// Ported from mojovoice (MIT, Copyright (c) 2024-2026 itsdevcoffee).

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self, Config};
use hf_hub::{api::sync::Api, Repo};
use tokenizers::Tokenizer;
use tracing::{debug, info, warn};

use crate::device::{device_label, select_device};

pub const DEFAULT_MODEL_ID: &str = "openai/whisper-base.en";
pub const SAMPLE_RATE: usize = whisper::SAMPLE_RATE;
pub const CHUNK_SAMPLES: usize = whisper::N_SAMPLES;

const CHUNK_LENGTH_SECS: f32 = 30.0;
const CHUNK_OVERLAP_SECS: f32 = 5.0;
const TEMPERATURES: [f64; 6] = whisper::TEMPERATURES;
const COMPRESSION_RATIO_THRESHOLD: f64 = whisper::COMPRESSION_RATIO_THRESHOLD;
const LOGPROB_THRESHOLD: f64 = whisper::LOGPROB_THRESHOLD;
const MEL_FILTERS_80: &[u8] = include_bytes!("../assets/melfilters80.bytes");
const MEL_FILTERS_128: &[u8] = include_bytes!("../assets/melfilters128.bytes");

enum Model {
    Normal(whisper::model::Whisper),
    Quantized(whisper::quantized_model::Whisper),
}

impl Model {
    fn encoder_forward(&mut self, mel: &Tensor, flush: bool) -> Result<Tensor> {
        match self {
            Self::Normal(model) => Ok(model.encoder.forward(mel, flush)?),
            Self::Quantized(model) => Ok(model.encoder.forward(mel, flush)?),
        }
    }

    fn decoder_forward(
        &mut self,
        tokens: &Tensor,
        audio_features: &Tensor,
        flush: bool,
    ) -> Result<Tensor> {
        match self {
            Self::Normal(model) => Ok(model.decoder.forward(tokens, audio_features, flush)?),
            Self::Quantized(model) => Ok(model.decoder.forward(tokens, audio_features, flush)?),
        }
    }

    fn decoder_final_linear(&self, x: &Tensor) -> Result<Tensor> {
        match self {
            Self::Normal(model) => Ok(model.decoder.final_linear(x)?),
            Self::Quantized(model) => Ok(model.decoder.final_linear(x)?),
        }
    }
}

pub struct VoiceEngine {
    device: Device,
    model: Model,
    tokenizer: Tokenizer,
    language: String,
    initial_prompt: Option<String>,
    suppress_tokens: Tensor,
    config: Config,
    is_english_only: bool,
}

impl VoiceEngine {
    pub fn load(
        model_id: &str,
        language: &str,
        initial_prompt: Option<String>,
    ) -> Result<Self> {
        let device = select_device();
        info!("Loading Whisper model on {}", device_label(&device));

        let model_path = Path::new(model_id);
        let is_local = model_path.exists();
        let is_direct_gguf =
            is_local && (model_id.ends_with(".gguf") || model_id.ends_with(".bin"));
        let is_dir_gguf = is_local && model_path.is_dir() && model_path.join("model.gguf").exists();
        let is_quantized = is_direct_gguf || is_dir_gguf;

        let (config, tokenizer, model) = if is_local {
            load_local_model(model_path, is_direct_gguf, is_dir_gguf, is_quantized, &device)?
        } else {
            load_hf_model(model_id, &device)?
        };

        let is_english_only = is_english_only_model(model_id);
        if is_english_only {
            info!("Detected English-only Whisper model");
        }

        let suppress_tokens = build_suppress_mask(&tokenizer, &device)?;

        info!(
            "Whisper model loaded: mel_bins={}, vocab={}, device={}",
            config.num_mel_bins,
            tokenizer.get_vocab_size(true),
            device_label(&device)
        );

        Ok(Self {
            device,
            model,
            tokenizer,
            language: language.to_string(),
            initial_prompt,
            suppress_tokens,
            config,
            is_english_only,
        })
    }

    pub fn device_label(&self) -> &'static str {
        device_label(&self.device)
    }

    pub fn transcribe(&mut self, audio_16k_mono: &[f32]) -> Result<String> {
        if audio_16k_mono.is_empty() {
            return Ok(String::new());
        }

        let duration_secs = audio_16k_mono.len() as f32 / SAMPLE_RATE as f32;
        info!(
            "Transcribing {} samples ({duration_secs:.2}s) with lang={} prompt={}",
            audio_16k_mono.len(),
            self.language,
            self.initial_prompt.is_some()
        );

        if duration_secs <= CHUNK_LENGTH_SECS {
            return self.transcribe_chunk(audio_16k_mono);
        }

        let chunk_samples = (CHUNK_LENGTH_SECS * SAMPLE_RATE as f32) as usize;
        let overlap_samples = (CHUNK_OVERLAP_SECS * SAMPLE_RATE as f32) as usize;
        let stride = chunk_samples - overlap_samples;
        let mut offset = 0;
        let mut results = Vec::new();

        while offset < audio_16k_mono.len() {
            let end = (offset + chunk_samples).min(audio_16k_mono.len());
            let chunk = &audio_16k_mono[offset..end];
            match self.transcribe_chunk(chunk) {
                Ok(text) if !text.is_empty() => results.push(text),
                Ok(_) => {}
                Err(err) => warn!("Whisper chunk failed at sample offset {offset}: {err}"),
            }

            offset += stride;
            if offset + chunk_samples > audio_16k_mono.len() && offset < audio_16k_mono.len() {
                let remaining = &audio_16k_mono[offset..];
                if remaining.len() > overlap_samples {
                    if let Ok(text) = self.transcribe_chunk(remaining) {
                        if !text.is_empty() {
                            results.push(text);
                        }
                    }
                }
                break;
            }
        }

        Ok(results.join(" "))
    }

    fn transcribe_chunk(&mut self, audio: &[f32]) -> Result<String> {
        if audio.is_empty() {
            return Ok(String::new());
        }

        let padded_audio = pad_chunk(audio);
        let filters = mel_filters(self.config.num_mel_bins)?;
        let mel_data = whisper::audio::pcm_to_mel(&self.config, &padded_audio, &filters);
        let frames = mel_data
            .len()
            .checked_div(self.config.num_mel_bins)
            .context("invalid mel shape")?;

        if frames == 0 {
            anyhow::bail!("empty mel spectrogram");
        }

        debug!(
            "Mel spectrogram: {}x{}",
            self.config.num_mel_bins, frames
        );

        let mel = Tensor::from_vec(
            mel_data,
            (self.config.num_mel_bins, frames),
            &self.device,
        )?
        .unsqueeze(0)?;

        self.decode_with_fallback(&mel)
    }

    fn get_special_tokens(&self) -> Result<SpecialTokens> {
        let token_id = |token: &str| -> Result<u32> {
            self.tokenizer
                .token_to_id(token)
                .ok_or_else(|| anyhow::anyhow!("token not found: {token}"))
        };

        Ok(SpecialTokens {
            sot_prev_token: token_id("<|startofprev|>")?,
            sot_token: token_id(whisper::SOT_TOKEN)?,
            eot_token: token_id(whisper::EOT_TOKEN)?,
            transcribe_token: token_id(whisper::TRANSCRIBE_TOKEN)?,
            no_timestamps_token: token_id(whisper::NO_TIMESTAMPS_TOKEN)?,
            language_token: token_id(&format!("<|{}|>", self.language))?,
        })
    }

    fn encode_initial_prompt(&self) -> Result<Vec<u32>> {
        let Some(prompt) = self.initial_prompt.as_ref() else {
            return Ok(Vec::new());
        };

        let encoding = self
            .tokenizer
            .encode(prompt.as_str(), false)
            .map_err(|err| anyhow::anyhow!("failed to encode initial prompt: {err}"))?;

        let tokens = encoding.get_ids().to_vec();
        const MAX_PROMPT_TOKENS: usize = 224;
        if tokens.len() > MAX_PROMPT_TOKENS {
            warn!(
                "Initial prompt has {} tokens, truncating to {}",
                tokens.len(),
                MAX_PROMPT_TOKENS
            );
            Ok(tokens[..MAX_PROMPT_TOKENS].to_vec())
        } else {
            Ok(tokens)
        }
    }

    fn decode_at_temperature(
        &mut self,
        mel: &Tensor,
        temperature: f64,
    ) -> Result<(String, f64, f64)> {
        let special_tokens = self.get_special_tokens()?;
        let prompt_tokens = self.encode_initial_prompt()?;
        let audio_features = self.model.encoder_forward(mel, true)?;
        let mut current_tokens =
            build_decoder_prefix(&prompt_tokens, &special_tokens, self.is_english_only);
        let start_result_idx = current_tokens.len();
        let max_tokens = 448_usize.saturating_sub(start_result_idx);

        let mut result_tokens = Vec::new();
        let mut sum_logprob = 0.0f64;
        let mut logprob_count = 0usize;
        let mut last_token = None;
        let mut repeat_count = 0usize;
        const MAX_REPEATS: usize = 3;

        for iteration in 0..max_tokens {
            let input = Tensor::new(current_tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let decoder_output =
                self.model
                    .decoder_forward(&input, &audio_features, iteration == 0)?;
            let logits = self.model.decoder_final_linear(&decoder_output)?.squeeze(0)?;
            let seq_len = logits.dim(0)?;
            let mut last_logit = logits.i((seq_len - 1, ..))?;

            last_logit = last_logit.broadcast_add(&self.suppress_tokens)?;
            if temperature > 0.0 {
                last_logit = (last_logit / temperature)?;
            }

            let probs = candle_nn::ops::softmax(&last_logit, 0)?;
            let next_token = last_logit.argmax(0)?.to_scalar::<u32>()?;
            let token_prob = probs.get(next_token as usize)?.to_scalar::<f32>()? as f64;
            if token_prob > 0.0 {
                sum_logprob += token_prob.ln();
                logprob_count += 1;
            }

            if next_token == special_tokens.eot_token {
                break;
            }

            if let Some(last) = last_token {
                if last == next_token {
                    repeat_count += 1;
                    if repeat_count >= MAX_REPEATS {
                        warn!(
                            "Token {} repeated {} times; ending decode",
                            next_token, repeat_count
                        );
                        break;
                    }
                } else {
                    repeat_count = 0;
                }
            }

            last_token = Some(next_token);
            current_tokens.push(next_token);
            if current_tokens.len() > start_result_idx {
                result_tokens.push(next_token);
            }
        }

        let decoded = self
            .tokenizer
            .decode(&result_tokens, true)
            .map_err(|err| anyhow::anyhow!("token decode failed: {err}"))?;
        let text = decoded.trim().to_string();
        let avg_logprob = if logprob_count > 0 {
            sum_logprob / logprob_count as f64
        } else {
            0.0
        };
        let compression_ratio = if text.is_empty() {
            0.0
        } else {
            result_tokens.len() as f64 / text.len() as f64
        };

        Ok((text, avg_logprob, compression_ratio))
    }

    fn decode_with_fallback(&mut self, mel: &Tensor) -> Result<String> {
        for (idx, temperature) in TEMPERATURES.iter().copied().enumerate() {
            let is_last = idx == TEMPERATURES.len() - 1;
            match self.decode_at_temperature(mel, temperature) {
                Ok((text, avg_logprob, compression_ratio)) => {
                    let quality_ok = compression_ratio <= COMPRESSION_RATIO_THRESHOLD
                        && avg_logprob >= LOGPROB_THRESHOLD;
                    if is_last || quality_ok {
                        debug!(
                            "Decode temperature={} logprob={:.3} compression={:.3}",
                            temperature, avg_logprob, compression_ratio
                        );
                        return Ok(text);
                    }
                }
                Err(err) => warn!("Decoding failed at temperature {temperature}: {err}"),
            }
        }

        anyhow::bail!("all Whisper temperature fallbacks failed")
    }
}

fn load_hf_model(model_id: &str, device: &Device) -> Result<(Config, Tokenizer, Model)> {
    info!("Downloading/loading Hugging Face model: {model_id}");
    let api = Api::new()?;
    let repo = api.repo(Repo::model(model_id.to_string()));
    let config_path = repo.get("config.json")?;
    let tokenizer_path = repo.get("tokenizer.json")?;
    let config: Config = serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
    let tokenizer = load_tokenizer(&tokenizer_path)?;

    let model = if let Ok(weights_path) = repo.get("model.gguf") {
        info!("Found GGUF model weights");
        load_quantized_model(&weights_path, &config, device)?
    } else {
        let weights_path = repo.get("model.safetensors")?;
        load_normal_model(vec![weights_path], &config, device)?
    };

    Ok((config, tokenizer, model))
}

fn load_local_model(
    model_path: &Path,
    is_direct_gguf: bool,
    is_dir_gguf: bool,
    is_quantized: bool,
    device: &Device,
) -> Result<(Config, Tokenizer, Model)> {
    info!("Loading local Whisper model: {}", model_path.display());
    if is_quantized {
        let gguf_path = if is_direct_gguf {
            model_path.to_path_buf()
        } else {
            model_path.join("model.gguf")
        };

        if !is_valid_gguf(&gguf_path) {
            anyhow::bail!("invalid or corrupted GGUF file: {}", gguf_path.display());
        }

        let (config, tokenizer) = if is_dir_gguf {
            load_local_config_tokenizer(model_path)?
        } else {
            load_hf_config_tokenizer("openai/whisper-large-v3-turbo")?
        };
        let model = load_quantized_model(&gguf_path, &config, device)?;
        Ok((config, tokenizer, model))
    } else {
        let (config, tokenizer) = load_local_config_tokenizer(model_path)?;
        let weights_path = model_path.join("model.safetensors");
        if !weights_path.exists() {
            anyhow::bail!("model weights not found: {}", weights_path.display());
        }
        let model = load_normal_model(vec![weights_path], &config, device)?;
        Ok((config, tokenizer, model))
    }
}

fn load_local_config_tokenizer(model_dir: &Path) -> Result<(Config, Tokenizer)> {
    let config_path = model_dir.join("config.json");
    let tokenizer_path = model_dir.join("tokenizer.json");
    if !config_path.exists() {
        anyhow::bail!("config not found: {}", config_path.display());
    }
    if !tokenizer_path.exists() {
        anyhow::bail!("tokenizer not found: {}", tokenizer_path.display());
    }
    let config: Config = serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
    let tokenizer = load_tokenizer(&tokenizer_path)?;
    Ok((config, tokenizer))
}

fn load_hf_config_tokenizer(model_id: &str) -> Result<(Config, Tokenizer)> {
    let api = Api::new()?;
    let repo = api.repo(Repo::model(model_id.to_string()));
    let config_path = repo.get("config.json")?;
    let tokenizer_path = repo.get("tokenizer.json")?;
    let config: Config = serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
    let tokenizer = load_tokenizer(&tokenizer_path)?;
    Ok((config, tokenizer))
}

fn load_tokenizer(path: &Path) -> Result<Tokenizer> {
    Tokenizer::from_file(path).map_err(|err| anyhow::anyhow!("failed to load tokenizer: {err}"))
}

fn load_normal_model(paths: Vec<PathBuf>, config: &Config, device: &Device) -> Result<Model> {
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&paths, whisper::DTYPE, device)? };
    Ok(Model::Normal(whisper::model::Whisper::load(
        &vb,
        config.clone(),
    )?))
}

fn load_quantized_model(path: &Path, config: &Config, device: &Device) -> Result<Model> {
    let vb = candle_transformers::quantized_var_builder::VarBuilder::from_gguf(path, device)?;
    Ok(Model::Quantized(
        whisper::quantized_model::Whisper::load(&vb, config.clone())?,
    ))
}

fn is_valid_gguf(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).is_ok() && &magic == b"GGUF"
}

fn is_english_only_model(model_id: &str) -> bool {
    model_id.contains(".en")
        || model_id.ends_with("-en")
        || model_id.contains("whisper-tiny-en")
        || model_id.contains("whisper-base-en")
        || model_id.contains("whisper-small-en")
        || model_id.contains("whisper-medium-en")
}

fn build_suppress_mask(tokenizer: &Tokenizer, device: &Device) -> Result<Tensor> {
    let mask = build_suppress_mask_values(tokenizer.get_vocab_size(true), no_ts_token(tokenizer)?);
    Ok(Tensor::new(mask.as_slice(), device)?)
}

fn build_suppress_mask_values(vocab_size: usize, no_ts_token: u32) -> Vec<f32> {
    let mut mask = vec![0f32; vocab_size];
    if 220 < vocab_size {
        mask[220] = f32::NEG_INFINITY;
    }
    for token in (no_ts_token + 1)..vocab_size as u32 {
        mask[token as usize] = f32::NEG_INFINITY;
    }
    mask
}

fn no_ts_token(tokenizer: &Tokenizer) -> Result<u32> {
    tokenizer
        .token_to_id(whisper::NO_TIMESTAMPS_TOKEN)
        .ok_or_else(|| anyhow::anyhow!("{} token not found", whisper::NO_TIMESTAMPS_TOKEN))
}

fn pad_chunk(audio: &[f32]) -> Vec<f32> {
    let mut padded = audio.to_vec();
    padded.resize(CHUNK_SAMPLES, 0.0);
    padded
}

fn mel_filters(num_mel_bins: usize) -> Result<Vec<f32>> {
    let bytes = match num_mel_bins {
        80 => MEL_FILTERS_80,
        128 => MEL_FILTERS_128,
        other => anyhow::bail!("unsupported Whisper mel bin count: {other}"),
    };

    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

#[derive(Debug, Clone)]
struct SpecialTokens {
    sot_prev_token: u32,
    sot_token: u32,
    eot_token: u32,
    transcribe_token: u32,
    no_timestamps_token: u32,
    language_token: u32,
}

fn build_decoder_prefix(
    prompt_tokens: &[u32],
    special: &SpecialTokens,
    is_english_only: bool,
) -> Vec<u32> {
    if !prompt_tokens.is_empty() {
        let mut tokens = vec![special.sot_prev_token];
        tokens.extend_from_slice(prompt_tokens);
        if is_english_only {
            tokens.extend_from_slice(&[special.sot_token, special.no_timestamps_token]);
        } else {
            tokens.extend_from_slice(&[
                special.sot_token,
                special.language_token,
                special.transcribe_token,
                special.no_timestamps_token,
            ]);
        }
        tokens
    } else if is_english_only {
        vec![special.sot_token, special.no_timestamps_token]
    } else {
        vec![
            special.sot_token,
            special.language_token,
            special.transcribe_token,
            special.no_timestamps_token,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn special_tokens() -> SpecialTokens {
        SpecialTokens {
            sot_prev_token: 50360,
            sot_token: 50258,
            eot_token: 50257,
            transcribe_token: 50359,
            no_timestamps_token: 50363,
            language_token: 50259,
        }
    }

    #[test]
    fn suppress_mask_blocks_blank_and_timestamp_tokens() {
        let mask = build_suppress_mask_values(10, 6);
        assert_eq!(mask[0], 0.0);
        assert_eq!(mask[7], f32::NEG_INFINITY);
        assert_eq!(mask[8], f32::NEG_INFINITY);
        assert_eq!(mask[9], f32::NEG_INFINITY);

        let mask = build_suppress_mask_values(300, 296);
        assert_eq!(mask[220], f32::NEG_INFINITY);
        assert_eq!(mask[296], 0.0);
        assert_eq!(mask[297], f32::NEG_INFINITY);
    }

    #[test]
    fn english_only_prefix_omits_language_and_task_tokens() {
        let special = special_tokens();
        let prefix = build_decoder_prefix(&[], &special, true);
        assert_eq!(prefix, vec![special.sot_token, special.no_timestamps_token]);
        assert!(!prefix.contains(&special.language_token));
        assert!(!prefix.contains(&special.transcribe_token));
    }

    #[test]
    fn multilingual_prefix_includes_language_and_task_tokens() {
        let special = special_tokens();
        let prefix = build_decoder_prefix(&[], &special, false);
        assert_eq!(
            prefix,
            vec![
                special.sot_token,
                special.language_token,
                special.transcribe_token,
                special.no_timestamps_token
            ]
        );
    }

    #[test]
    fn prompt_prefix_uses_start_of_previous() {
        let special = special_tokens();
        let prompt = vec![10, 20, 30];
        let prefix = build_decoder_prefix(&prompt, &special, false);
        assert_eq!(prefix[0], special.sot_prev_token);
        assert_eq!(&prefix[1..=3], prompt.as_slice());
        assert_eq!(prefix[4], special.sot_token);
    }

    #[test]
    fn chunk_padding_extends_to_whisper_window() {
        let audio = vec![1.0, -1.0, 0.5];
        let padded = pad_chunk(&audio);
        assert_eq!(padded.len(), CHUNK_SAMPLES);
        assert_eq!(&padded[..3], audio.as_slice());
        assert!(padded[3..].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn mel_filters_match_expected_lengths() {
        assert_eq!(mel_filters(80).unwrap().len(), 80 * 201);
        assert_eq!(mel_filters(128).unwrap().len(), 128 * 201);
    }
}
