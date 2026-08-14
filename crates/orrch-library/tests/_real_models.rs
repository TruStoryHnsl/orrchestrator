use orrch_library::{ApiFormat, EngineLocation, load_models};
use std::path::Path;

#[test]
fn real_deepseek_files_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/models");
    let models = load_models(&dir);
    let ds = models
        .iter()
        .find(|m| m.name == "DeepSeek V4 Flash")
        .expect("deepseek cloud present");
    assert_eq!(ds.base_url.as_deref(), Some("https://api.deepseek.com"));
    assert_eq!(ds.api_format, vec![ApiFormat::Anthropic, ApiFormat::OpenAI]);
    assert_eq!(ds.location, EngineLocation::Cloud);
    assert_eq!(ds.max_context, Some(1_000_000));

    let oll = models
        .iter()
        .find(|m| m.name == "DeepSeek V4 Flash (Ollama)")
        .expect("ollama present");
    assert_eq!(oll.base_url.as_deref(), Some("http://localhost:11434/v1"));
    assert_eq!(oll.api_format, vec![ApiFormat::OpenAI]);
    assert_eq!(oll.location, EngineLocation::Gateway);
    assert_eq!(oll.api_key_env, None);

    // ENG bindability fix: claude_opus.md now declares anthropic format + a
    // base_url so the engine binding (engine_env) can target it explicitly.
    let opus = models
        .iter()
        .find(|m| m.name == "Claude Opus 4.6")
        .expect("opus present");
    assert_eq!(opus.api_format, vec![ApiFormat::Anthropic]);
    assert_eq!(opus.location, EngineLocation::Cloud);
    assert_eq!(opus.base_url.as_deref(), Some("https://api.anthropic.com"));
}
