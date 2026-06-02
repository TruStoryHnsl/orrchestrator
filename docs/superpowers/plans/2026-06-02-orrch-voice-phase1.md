# orrch-voice — Phase 1: STT daemon foundation

**Part of:** the voice-control surface (4 phases). Phase 1 = the foundation: hotkey-toggle → mic → Whisper → visible transcript, as a new in-process crate. Later phases: MCP bridge tools (2), voice-control loop session (3), Hypervise display (4).

**Goal:** A new `orrch-voice` crate that captures microphone audio and transcribes it to text with local Whisper (Candle), selecting GPU at runtime when present and CPU otherwise, with custom-vocab prompt biasing and an in-process toggle. Ported from the MIT-licensed mojovoice (`/home/user/Applications/mojovoice`); attribution preserved.

**Device rule (user):** GPU if present, CPU if not — at RUNTIME. Candle CUDA/Metal must be compiled in to be usable, so: `cuda`/`metal` are cargo features (OFF by default → workspace always builds); runtime `select_device()` tries CUDA→Metal→CPU and falls back automatically. Build tooling auto-enables `cuda` when the CUDA toolkit is detected.

**License:** mojovoice is MIT (© itsdevcoffee). Add `crates/orrch-voice/LICENSE-mojovoice.txt` with the original MIT notice + a header note in ported files.

---

## What to PORT (from mojovoice) and what to LEAVE

PORT (functional core): `src/transcribe/candle_engine.rs` (Whisper engine), the `Transcriber` trait, `src/audio/mod.rs` (cpal capture), `src/vocab/store.rs` (SQLite vocab → prompt biasing), `src/state/toggle.rs` (toggle state — but reimplement in-process, NOT PID-file/SIGUSR1 based), device selection from `candle_engine::get_device()`.

LEAVE: `mojo_ffi.rs` + `lib/libmojo_audio.so` (native blob — replace mel with Candle's `pcm_to_mel` + the melfilters assets; the WAV test below proves the mel pipeline is correct), Tauri UI, enigo output, benchmark/, history/, the Unix-socket daemon (Phase 2 will add the MCP bridge instead).

CAVEAT: mojovoice notes Candle's `pcm_to_mel` "produces incorrect frame counts." So V4's real-WAV transcription is mandatory — if the mel is wrong, the transcript is garbage even though it compiles. If Candle's `pcm_to_mel` is genuinely broken for our model, port a correct log-mel (STFT → melfilter → log) in pure Rust; the test decides.

---

## Tasks

### V1 — Crate scaffold + runtime device selection
- New crate `crates/orrch-voice/` (workspace member). Deps: `candle-core`, `candle-nn`, `candle-transformers`, `hf-hub`, `tokenizers`, `safetensors` (pin to the versions mojovoice uses: 0.9.x candle), `cpal`, `hound`, `rubato`, `rusqlite` (bundled), `anyhow`, `serde`, `tracing`. Features: `cuda = ["candle-core/cuda", "candle-nn/cuda", "candle-transformers/cuda"]`, `metal = [...]`, both OFF by default.
- `src/device.rs`: `pub fn select_device() -> Device` — `#[cfg(feature="cuda")]` try `Device::new_cuda(0)`; `#[cfg(feature="metal")]` try `Device::new_metal(0)`; else/on-error `Device::Cpu`. Log which was chosen. `pub fn device_label(&Device) -> &str`.
- Test: `select_device()` returns Cpu in the default (no-feature) build and never panics.

### V2 — Candle Whisper engine (no FFI)
- `src/engine.rs`: port `CandleEngine` — `pub fn load(model_id: &str, language: &str, initial_prompt: Option<String>) -> Result<Self>` and `pub fn transcribe(&mut self, audio_16k_mono: &[f32]) -> Result<String>`. Use `select_device()`. Model load via hf-hub (cache under `data_dir()/voice/models` — reuse orrch-core `config::data_dir()` if practical, else hf-hub default cache) OR a local dir. Use Candle's `pcm_to_mel` + bundled melfilters (copy `assets/melfilters80.bytes` and `melfilters128.bytes` from mojovoice into `crates/orrch-voice/assets/`, embed via `include_bytes!`). Greedy decode w/ temperature fallback as in mojovoice. Chunk >30s.
- Default model: a small/fast Whisper (e.g. `openai/whisper-base.en` or `base`), configurable. (large-v3-turbo is the mojovoice default and fits the 3070's 8GB, but base keeps CPU-fallback usable.)
- Unit tests (no network): suppress-token mask construction; english-only vs multilingual token sequence; chunking math (samples→480000 padding). Mark any model-download test `#[ignore]`.

### V3 — cpal capture + in-process toggle + vocab
- `src/capture.rs`: port `capture(duration, device)` and `capture_toggle(max, device, stop_flag)` producing 16 kHz mono f32 (resample via rubato if device rate differs). `list_input_devices()`. NO PipeWire-pactl mutation of the system default (read-only device selection); if the mojovoice code sets the default source via `pactl`, DROP that — just pick the cpal device by name or default.
- `src/toggle.rs`: in-process `ToggleState` = `Arc<AtomicBool>` (listening on/off) + a `should_stop()` the capture loop polls. NO PID files, NO SIGUSR1.
- `src/vocab.rs`: port `VocabStore` (SQLite at `data_dir()/voice/vocab.db`): `add_term/remove_term/list_terms/get_prompt_string(max_tokens)`. Test add→list→prompt round-trip in a tmp db.

### V4 — Phase 1 verification (REAL transcription — the LOOK proof)
- Copy `harvard-list01-female.wav` (and one short clip) from mojovoice `assets/audio/samples/` into `crates/orrch-voice/assets/samples/`.
- Add an `examples/transcribe_wav.rs`: load the default Whisper model, read the WAV (hound) → f32 16k mono → `engine.transcribe()` → print the transcript. This DOWNLOADS a model on first run (network), so it's an example, not a CI test.
- Add an `#[ignore]`d integration test `tests/wav_transcription.rs` that does the same and asserts the transcript is non-empty and contains expected Harvard-sentence words (e.g. for list01: "smell"/"ham"/"beauty" — verify the actual sentence and assert 1-2 distinctive words appear). Document how to run: `cargo test -p orrch-voice --features <cpu> -- --ignored`.
- This is the gate that proves the mel + decode pipeline actually works, not just compiles.

### V5 — Review + verify + merge
- Independent review: device-fallback correctness (CPU build has no cuda symbols; never panics on a GPU-less box), mel/transcription correctness via the WAV proof, no leftover PID-file/pactl/enigo/FFI, license attribution present.
- Build check: `cargo build` (default, NO features) compiles cleanly on a CPU-only basis (proves it builds on mac/VMs). `cargo build -p orrch-voice --features cuda` is NOT required to pass here (no CUDA toolkit assumption in the default workspace build).
- LOOK: run `examples/transcribe_wav.rs`, OBSERVE the transcript of the Harvard sample. Report the actual text.
- Merge to main via the tiered tool. Then CHECK IN with the user before Phase 2.

---

## Hard constraints
- Default `cargo build` (no features) MUST compile with no CUDA toolkit present.
- Runtime: GPU used iff a GPU is actually present AND the backend was compiled in; otherwise CPU. Never panic when no GPU.
- No global system mutation (no pactl default-source changes, no PID files, no signal handlers, no enigo typing).
- 16 kHz mono f32 is the canonical audio format end to end.
- Conventional commits; tests use tmp dirs; preserve mojovoice MIT attribution.
