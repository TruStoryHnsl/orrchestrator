//! ENG-003 / ENG-004 / ENG-005 — engine resolution, env injection, valve gating.
//!
//! An "engine" is the LLM/endpoint a session talks to (a `ModelEntry` from
//! `library/models/*.md`). It is ORTHOGONAL to the harness (`BackendKind`), which
//! is the CLI/transport. The resolver here picks the engine; the harness is still
//! chosen in the spawn flow. The ENG-004 binding (`engine_env`) is what marries a
//! resolved engine to a chosen harness by emitting the env vars the harness needs
//! to talk to the engine's endpoint.

use std::path::Path;

use orrch_library::{ApiFormat, EngineLocation, ModelEntry, ValveStore};

use crate::backend::BackendKind;

// ─── ENG-003: four-level precedence resolver (PURE) ─────────────────────────

/// One engine choice from each precedence layer. `None` = "this layer expressed
/// no preference". The engine-id is `ModelEntry.name` (the stable id used in
/// `library/models/*.md` and in agent/project/global config).
#[derive(Debug, Clone, Default)]
pub struct EngineLayers {
    /// Per-spawn override chosen in the TUI picker (ENG-006). Highest priority.
    pub session_pick: Option<String>,
    /// agents/<role>.md frontmatter `engine:` field.
    pub agent_role: Option<String>,
    /// <project>/.orrch/ project default.
    pub project_default: Option<String>,
    /// ~/.config/orrchestrator/config.json `default_engine`.
    pub global_default: Option<String>,
}

/// Outcome of resolution: which engine id won and from which layer (for UI/debug).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEngineId {
    pub engine_id: String,
    pub source: EngineSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineSource {
    Session,
    AgentRole,
    ProjectDefault,
    GlobalDefault,
    Builtin,
}

/// PURE. Picks the first `Some` layer; if every layer is `None`, returns the
/// builtin `fallback` id (caller passes e.g. "claude-sonnet" — the id of the
/// shipped default model file). Never reads disk, never panics.
pub fn resolve_engine_id(layers: &EngineLayers, fallback: &str) -> ResolvedEngineId {
    if let Some(id) = &layers.session_pick {
        return ResolvedEngineId {
            engine_id: id.clone(),
            source: EngineSource::Session,
        };
    }
    if let Some(id) = &layers.agent_role {
        return ResolvedEngineId {
            engine_id: id.clone(),
            source: EngineSource::AgentRole,
        };
    }
    if let Some(id) = &layers.project_default {
        return ResolvedEngineId {
            engine_id: id.clone(),
            source: EngineSource::ProjectDefault,
        };
    }
    if let Some(id) = &layers.global_default {
        return ResolvedEngineId {
            engine_id: id.clone(),
            source: EngineSource::GlobalDefault,
        };
    }
    ResolvedEngineId {
        engine_id: fallback.to_string(),
        source: EngineSource::Builtin,
    }
}

/// Convenience: resolve the id, then look the `ModelEntry` up in a loaded slice
/// (the slice comes from `orrch_library::load_models`). Returns `None` for the
/// entry if the winning id isn't a known model.
pub fn resolve_engine<'a>(
    layers: &EngineLayers,
    models: &'a [ModelEntry],
    fallback: &str,
) -> (ResolvedEngineId, Option<&'a ModelEntry>) {
    let resolved = resolve_engine_id(layers, fallback);
    let entry = models.iter().find(|m| m.name == resolved.engine_id);
    (resolved, entry)
}

// ─── LOOP-012: class → default engine mapping (PURE) ────────────────────────

/// ENG-008 reasoning provider — Support loops default here.
pub const SUPPORT_DEFAULT_ENGINE: &str = "GPT-4o";
/// ENG-008 execution provider — Dev loops default here.
pub const DEV_DEFAULT_ENGINE: &str = "Claude Sonnet 4.6";

/// LOOP-012 (PURE). The class-appropriate default engine id for a loop class.
/// Support (planning/analysis/testing/research/critique) → GPT; Dev → Claude.
/// This is the loop-level analogue of `agent_layer_engine`: it feeds the
/// `EngineLayers` so the existing 4-level precedence still runs on top
/// (session pick > class default > project default > global > builtin).
pub fn class_default_engine(class: crate::loops::LoopClass) -> &'static str {
    if class.is_support() {
        SUPPORT_DEFAULT_ENGINE
    } else {
        DEV_DEFAULT_ENGINE
    }
}

/// LOOP-012 overridable resolution. The class default occupies a layer slot
/// ABOVE project/global (a class default beats a project default) but BELOW
/// session_pick + agent_role (an explicit per-spawn or per-agent engine still
/// wins → "overriding the class default works"). PURE; reuses
/// `resolve_engine_id`. `class_override` = an explicit engine the loop schedule
/// pins (None → use `class_default_engine`).
pub fn resolve_loop_engine_id(
    class: crate::loops::LoopClass,
    session_pick: Option<&str>,
    agent_role: Option<&str>,
    class_override: Option<&str>,
    project_default: Option<&str>,
    global_default: Option<&str>,
    fallback: &str,
) -> ResolvedEngineId {
    let class_engine = class_override
        .map(str::to_string)
        .or_else(|| Some(class_default_engine(class).to_string()));
    let layers = EngineLayers {
        session_pick: session_pick.map(str::to_string),
        agent_role: agent_role.map(str::to_string),
        // Fold the class default in ABOVE project by occupying the project slot
        // ONLY when the project expressed no preference of its own.
        project_default: project_default.map(str::to_string).or(class_engine),
        global_default: global_default.map(str::to_string),
    };
    resolve_engine_id(&layers, fallback)
}

// ─── ENG-009: deploy handoff — resolved engine + human rationale ────────────

/// ENG-009 outcome: the resolved engine id, which layer won, the importance that
/// drove the agent layer, and a one-line human rationale shown at spawn time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineDecision {
    pub engine_id: String, // "" when no layer named an engine (legacy no-engine path)
    pub source: EngineSource, // which precedence layer won
    pub importance: Importance, // the importance that drove the agent layer
    pub rationale: String, // one-line human explanation, shown at spawn
}

/// ENG-009. PURE given the collected layer strings. Builds the agent layer from
/// the agent's (standard, optimal) + importance, runs the existing 4-level
/// precedence (resolve_engine_id), and emits a rationale. Caller (TUI) collects
/// the impure layer strings (session pick, project default, global default) and
/// the agent's two engine ids, then calls this.
pub fn decide_engine(
    session_pick: Option<&str>,
    agent_standard: Option<&str>,
    agent_optimal: Option<&str>,
    project_default: Option<&str>,
    global_default: Option<&str>,
    importance: Importance,
    fallback: &str,
) -> EngineDecision {
    let agent_role = agent_layer_engine(agent_standard, agent_optimal, importance);
    let layers = EngineLayers {
        session_pick: session_pick.map(str::to_string),
        agent_role,
        project_default: project_default.map(str::to_string),
        global_default: global_default.map(str::to_string),
    };
    let r = resolve_engine_id(&layers, fallback);
    let rationale = match r.source {
        EngineSource::Session => {
            format!("user-picked '{}' (overrides computed default)", r.engine_id)
        }
        EngineSource::AgentRole => format!(
            "agent's {} engine '{}' for a {:?} task",
            if importance.wants_optimal() {
                "optimal"
            } else {
                "standard"
            },
            r.engine_id,
            importance
        ),
        EngineSource::ProjectDefault => format!("project default engine '{}'", r.engine_id),
        EngineSource::GlobalDefault => format!("global default engine '{}'", r.engine_id),
        EngineSource::Builtin => {
            "no engine declared at any layer — harness uses its own default".into()
        }
    };
    EngineDecision {
        engine_id: r.engine_id,
        source: r.source,
        importance,
        rationale,
    }
}

// ─── ENG-007: task importance dimension (PURE) ──────────────────────────────

/// ENG-007: task importance dimension. Selects optimal vs standard engine at the
/// AGENT layer, BEFORE the existing 4-level precedence runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Importance {
    #[default]
    Routine, // standard_engine
    Important, // optimal_engine
    Critical,  // optimal_engine
}
impl Importance {
    pub fn wants_optimal(self) -> bool {
        matches!(self, Importance::Important | Importance::Critical)
    }
}

/// PURE. ENG-007: given an agent's declared (standard, optimal) engine ids and a
/// task importance, return the id that should populate `EngineLayers.agent_role`.
/// `None` when the agent declared neither (legacy) — caller leaves agent_role None
/// and the existing precedence falls through to project/global/builtin.
pub fn agent_layer_engine(
    standard: Option<&str>,
    optimal: Option<&str>,
    importance: Importance,
) -> Option<String> {
    if importance.wants_optimal() {
        optimal.or(standard).map(str::to_string)
    } else {
        standard.or(optimal).map(str::to_string)
    }
}

// ─── ENG-003 layer collectors (impure, kept OUT of the pure core) ───────────

/// Read the `engine:` field from an `agents/<role>.md` frontmatter. Returns
/// `None` when the file is missing or the key is absent (legacy agents that only
/// declare `preferred_backend` stay `None` — the field is ADDITIVE).
pub fn agent_role_engine(agent_md: &Path) -> Option<String> {
    let content = std::fs::read_to_string(agent_md).ok()?;
    let (fm, _body) = orrch_library::store::parse_frontmatter_pub(&content)?;
    orrch_library::store::extract_field_pub(&fm, "engine")
}

/// ENG-007: read an agent md's `(standard_engine, optimal_engine)` ids, with the
/// legacy `engine:` field used as the standard fallback. Either side `None` when
/// absent. Used by callers that pass both ids into `decide_engine`.
pub fn agent_engine_pair(agent_md: &Path) -> (Option<String>, Option<String>) {
    let Ok(content) = std::fs::read_to_string(agent_md) else {
        return (None, None);
    };
    let Some((fm, _b)) = orrch_library::store::parse_frontmatter_pub(&content) else {
        return (None, None);
    };
    let f = |k| orrch_library::store::extract_field_pub(&fm, k);
    let standard = f("standard_engine").or_else(|| f("engine"));
    let optimal = f("optimal_engine");
    (standard, optimal)
}

/// ENG-007: read an agent md's standard_engine/optimal_engine (falling back to the
/// legacy `engine:` field for standard) and apply importance. Absent both → None.
pub fn agent_role_engine_for(agent_md: &Path, importance: Importance) -> Option<String> {
    let (standard, optimal) = agent_engine_pair(agent_md);
    agent_layer_engine(standard.as_deref(), optimal.as_deref(), importance)
}

/// Read a project default engine id from `<project_dir>/.orrch/engine`
/// (single-line id) or an `engine:` key in `.orrch/config.{json,yaml,yml,toml}`.
/// Absent → `None`.
pub fn project_default_engine(project_dir: &Path) -> Option<String> {
    let orrch_dir = project_dir.join(".orrch");

    // 1. plain single-line `.orrch/engine`
    let engine_file = orrch_dir.join("engine");
    if let Ok(s) = std::fs::read_to_string(&engine_file) {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // 2. `engine:` key inside a `.orrch/config.*`
    for cfg_name in ["config.json", "config.yaml", "config.yml", "config.toml"] {
        let cfg_path = orrch_dir.join(cfg_name);
        if let Ok(s) = std::fs::read_to_string(&cfg_path)
            && let Some(id) = extract_engine_key(&s)
        {
            return Some(id);
        }
    }
    None
}

/// Tiny tolerant `engine:`/`"engine":` extractor for the project config file.
/// Avoids pulling a yaml/toml parser into orrch-core for one optional key.
fn extract_engine_key(s: &str) -> Option<String> {
    for line in s.lines() {
        let l = line.trim();
        // JSON: "engine": "id"  /  YAML/TOML: engine: id  | engine = "id"
        let rest = l
            .strip_prefix("\"engine\"")
            .or_else(|| l.strip_prefix("engine"))?;
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('='))?;
        let val = rest.trim().trim_matches([',', '"', '\'']).trim();
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}

// ─── ENG-004: pure env-injection binding ────────────────────────────────────

// ─── ENG-009: engine↔harness routing policy (account/billing constraints) ──

/// PURE. True if `engine` is an Anthropic/Claude-family model — the class that
/// is subscription-only: it runs ONLY in the Claude Code CLI harness, never via
/// the token-billed Anthropic API, and never in any other harness. Detected by
/// provider name or a "claude" name match (Anthropic is the only wire format
/// these speak).
pub fn is_anthropic_family(engine: &ModelEntry) -> bool {
    let provider = engine.provider.to_ascii_lowercase();
    provider.contains("anthropic")
        || provider.contains("claude")
        || engine.name.to_ascii_lowercase().contains("claude")
}

/// PURE. Enforce the recorded engine↔harness routing constraints (see
/// `.orrch/architecture.md` → "Engine routing constraints"). Returns
/// `Err(reason)` for a forbidden pairing, `Ok(())` for an allowed one.
///
/// Hard rules (the user's account reality):
///  1. **Anthropic API is token-billed and NOT covered by the subscription** —
///     the `AnthropicApi` backend is forbidden for EVERY engine. Anything that
///     needs Claude routes through the Claude Code harness instead.
///  2. **Claude/Anthropic-family models are Claude-Code-harness-only** — they
///     cannot run in any non-Claude harness (Codex, Gemini, Crush, OpenCode,
///     Pi, or either HTTP-API backend).
///  3. **OpenAI/GPT (codex-subscription) models MAY run in the Claude Code
///     harness** — the intended cross-engine flexibility vector, explicitly
///     permitted here. (The transport binding that makes Claude Code actually
///     speak to a GPT engine is a separate build; this gate only governs what
///     is *allowed*, and never widens the surface for Claude models.)
pub fn engine_harness_policy(harness: BackendKind, engine: &ModelEntry) -> Result<(), String> {
    // Rule 1 — never pay Anthropic per token.
    if harness == BackendKind::AnthropicApi {
        return Err(format!(
            "Anthropic API is token-billed and not covered by the subscription; \
             route engine '{}' through the Claude Code harness instead",
            engine.name
        ));
    }
    // Rule 2 — Claude/Anthropic models run only in the Claude Code harness.
    if is_anthropic_family(engine) && harness != BackendKind::Claude {
        return Err(format!(
            "engine '{}' is an Anthropic/Claude model — it runs only in the Claude Code \
             harness (subscription-covered); it cannot run in the {} harness",
            engine.name,
            harness.label()
        ));
    }
    // Rule 3 — OpenAI-family in the Claude Code harness is allowed (no deny).
    Ok(())
}

/// PURE. Given a harness (`BackendKind`) and a resolved engine (`ModelEntry`),
/// produce the env-var pairs that must be injected into the spawned child so the
/// harness talks to the engine's endpoint/model. The API-KEY VALUE is resolved
/// here from `engine.api_key_env` via the injected `resolve_key` closure so the
/// function stays pure & testable (tests pass a stub mapping
/// `"DEEPSEEK_API_KEY" -> "sk-test"`; production passes
/// `|var| std::env::var(var).ok()`). Returns `Err` for an incompatible
/// (harness, api_format) pair.
///
/// NOTE: secrets are returned as values to inject into the child env at spawn
/// time and are NEVER written to disk (ENG-004).
pub fn engine_env(
    harness: BackendKind,
    engine: &ModelEntry,
    resolve_key: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<Vec<(String, String)>> {
    // ENG-009 gate FIRST: reject forbidden (engine, harness) pairings before
    // any binding work — billing/asymmetry constraints take precedence over
    // wire-format compatibility.
    engine_harness_policy(harness, engine).map_err(|e| anyhow::anyhow!(e))?;
    let has = |f: ApiFormat| engine.api_format.contains(&f);
    let base = engine.base_url.clone();
    let model = engine.model_id.clone();
    let key = || -> anyhow::Result<String> {
        let var = engine
            .api_key_env
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("engine '{}' has no api_key_env", engine.name))?;
        resolve_key(var).ok_or_else(|| anyhow::anyhow!("{var} is not set"))
    };
    match harness {
        // claude code speaks Anthropic format
        BackendKind::Claude => {
            if !has(ApiFormat::Anthropic) {
                anyhow::bail!(
                    "engine '{}' does not speak Anthropic format (claude harness)",
                    engine.name
                );
            }
            let mut v = vec![("ANTHROPIC_MODEL".into(), model)];
            if let Some(b) = base {
                v.push(("ANTHROPIC_BASE_URL".into(), b));
            }
            // local engines may have no key (ollama signin); only add when present
            if engine.api_key_env.is_some() {
                v.push(("ANTHROPIC_AUTH_TOKEN".into(), key()?));
            }
            Ok(v)
        }
        // opencode / crush / aider speak OpenAI format
        BackendKind::OpenCode | BackendKind::Crush => {
            if !has(ApiFormat::OpenAI) {
                anyhow::bail!(
                    "engine '{}' does not speak OpenAI format ({} harness)",
                    engine.name,
                    harness.label()
                );
            }
            let mut v = vec![("OPENAI_MODEL".into(), model)]; // harness also gets model via flag; see provider.cli_args
            if let Some(b) = base {
                v.push(("OPENAI_BASE_URL".into(), b));
            }
            if engine.api_key_env.is_some() {
                v.push(("OPENAI_API_KEY".into(), key()?));
            }
            Ok(v)
        }
        // pi multiplexes providers via its own flags, not env; accepts either
        // format, so env injection is a no-op here.
        BackendKind::Pi => Ok(vec![]),
        // Native HTTP backends: send_api_message reads from the engine directly,
        // no child env needed.
        BackendKind::AnthropicApi | BackendKind::OpenAiApi => Ok(vec![]),
        // codex/gemini: not yet bound to arbitrary engines — only compatible if
        // the engine is their native cloud default (api_format Cli). Reject
        // explicit cross-engine binding for now with a clear message.
        BackendKind::Codex | BackendKind::Gemini => {
            if engine.api_format == vec![ApiFormat::Cli] {
                Ok(vec![])
            } else {
                anyhow::bail!(
                    "{} harness has no engine binding for '{}'",
                    harness.label(),
                    engine.name
                )
            }
        }
    }
}

// ─── ENG-001 URL composition helpers (pure, offline-testable) ───────────────

/// Anthropic messages endpoint for a given base host.
pub fn anthropic_url(base_url: &str) -> String {
    format!("{}/v1/messages", base_url.trim_end_matches('/'))
}

/// OpenAI chat-completions endpoint for a given base host.
pub fn openai_url(base_url: &str) -> String {
    format!("{}/v1/chat/completions", base_url.trim_end_matches('/'))
}

// ─── ENG-005: valve gating (PURE) ───────────────────────────────────────────

/// True if this engine is selectable right now. `EngineLocation::Local` is NEVER
/// valve-gated (ENG-005); Cloud/Gateway are hidden when the provider valve is
/// closed. Valve key is `engine.provider`.
pub fn engine_selectable(engine: &ModelEntry, valves: &ValveStore) -> bool {
    if engine.location == EngineLocation::Local {
        return true;
    }
    !valves.is_blocked(&engine.provider)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orrch_library::{ModelTier, PricingModel};
    use std::path::PathBuf;

    fn mk_engine(
        name: &str,
        provider: &str,
        model_id: &str,
        base_url: Option<&str>,
        api_key_env: Option<&str>,
        api_format: Vec<ApiFormat>,
        location: EngineLocation,
    ) -> ModelEntry {
        ModelEntry {
            name: name.into(),
            provider: provider.into(),
            model_id: model_id.into(),
            tier: ModelTier::MidTier,
            pricing: PricingModel::Local,
            capabilities: vec![],
            limitations: vec![],
            max_context: None,
            api_key_env: api_key_env.map(|s| s.into()),
            notes: String::new(),
            last_checked: None,
            base_url: base_url.map(|s| s.into()),
            api_format,
            location,
            path: PathBuf::new(),
        }
    }

    // ── resolver precedence ──────────────────────────────────────────────

    #[test]
    fn test_resolver_agent_beats_project_and_global() {
        let layers = EngineLayers {
            session_pick: None,
            agent_role: Some("deepseek-v4-flash".into()),
            project_default: Some("gpt-4o".into()),
            global_default: Some("claude".into()),
        };
        let r = resolve_engine_id(&layers, "builtin-fallback");
        assert_eq!(r.engine_id, "deepseek-v4-flash");
        assert_eq!(r.source, EngineSource::AgentRole);
    }

    #[test]
    fn test_resolver_session_wins_over_everything() {
        let layers = EngineLayers {
            session_pick: Some("session-engine".into()),
            agent_role: Some("a".into()),
            project_default: Some("p".into()),
            global_default: Some("g".into()),
        };
        let r = resolve_engine_id(&layers, "fb");
        assert_eq!(r.engine_id, "session-engine");
        assert_eq!(r.source, EngineSource::Session);
    }

    #[test]
    fn test_resolver_all_none_yields_builtin() {
        let layers = EngineLayers::default();
        let r = resolve_engine_id(&layers, "claude-sonnet");
        assert_eq!(r.engine_id, "claude-sonnet");
        assert_eq!(r.source, EngineSource::Builtin);
    }

    #[test]
    fn test_resolver_project_then_global() {
        let layers = EngineLayers {
            project_default: Some("proj".into()),
            global_default: Some("glob".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_engine_id(&layers, "fb").source,
            EngineSource::ProjectDefault
        );
        let layers2 = EngineLayers {
            global_default: Some("glob".into()),
            ..Default::default()
        };
        assert_eq!(
            resolve_engine_id(&layers2, "fb").source,
            EngineSource::GlobalDefault
        );
    }

    #[test]
    fn test_resolve_engine_finds_entry() {
        let models = vec![
            mk_engine(
                "a",
                "Anthropic",
                "claude-x",
                None,
                None,
                vec![ApiFormat::Cli],
                EngineLocation::Cloud,
            ),
            mk_engine(
                "deepseek-v4-flash",
                "DeepSeek",
                "deepseek-v4-flash",
                Some("https://api.deepseek.com"),
                Some("DEEPSEEK_API_KEY"),
                vec![ApiFormat::Anthropic, ApiFormat::OpenAI],
                EngineLocation::Cloud,
            ),
        ];
        let layers = EngineLayers {
            session_pick: Some("deepseek-v4-flash".into()),
            ..Default::default()
        };
        let (resolved, entry) = resolve_engine(&layers, &models, "a");
        assert_eq!(resolved.engine_id, "deepseek-v4-flash");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().model_id, "deepseek-v4-flash");

        // unknown id → None entry but resolved id preserved
        let layers2 = EngineLayers {
            session_pick: Some("nope".into()),
            ..Default::default()
        };
        let (_r, e) = resolve_engine(&layers2, &models, "a");
        assert!(e.is_none());
    }

    // ── engine_env binding ───────────────────────────────────────────────

    #[test]
    fn test_engine_env_claude_anthropic_engine() {
        let engine = mk_engine(
            "deepseek-v4-flash",
            "DeepSeek",
            "deepseek-v4-flash",
            Some("https://api.deepseek.com"),
            Some("DEEPSEEK_API_KEY"),
            vec![ApiFormat::Anthropic, ApiFormat::OpenAI],
            EngineLocation::Cloud,
        );
        let stub = |var: &str| {
            if var == "DEEPSEEK_API_KEY" {
                Some("sk-test".to_string())
            } else {
                None
            }
        };
        let env = engine_env(BackendKind::Claude, &engine, stub).expect("compatible");
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert_eq!(
            map.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            map.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("deepseek-v4-flash")
        );
        assert_eq!(
            map.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str),
            Some("sk-test")
        );
    }

    #[test]
    fn test_engine_env_opencode_openai_engine() {
        let engine = mk_engine(
            "deepseek-v4-flash",
            "DeepSeek",
            "deepseek-v4-flash",
            Some("https://api.deepseek.com"),
            Some("DEEPSEEK_API_KEY"),
            vec![ApiFormat::OpenAI],
            EngineLocation::Cloud,
        );
        let stub = |_: &str| Some("sk-test".to_string());
        let env = engine_env(BackendKind::OpenCode, &engine, stub).expect("compatible");
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert_eq!(
            map.get("OPENAI_BASE_URL").map(String::as_str),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            map.get("OPENAI_MODEL").map(String::as_str),
            Some("deepseek-v4-flash")
        );
        assert_eq!(
            map.get("OPENAI_API_KEY").map(String::as_str),
            Some("sk-test")
        );
    }

    // ── ENG-009 engine↔harness routing policy ───────────────────────────────

    fn mk_claude_engine() -> ModelEntry {
        mk_engine(
            "Claude Sonnet 4.6",
            "Anthropic",
            "claude-sonnet-4-6",
            None,
            Some("ANTHROPIC_AUTH_TOKEN"),
            vec![ApiFormat::Anthropic],
            EngineLocation::Cloud,
        )
    }

    fn mk_gpt_engine() -> ModelEntry {
        mk_engine(
            "GPT-4o",
            "OpenAI",
            "gpt-4o",
            None,
            Some("OPENAI_API_KEY"),
            vec![ApiFormat::OpenAI],
            EngineLocation::Cloud,
        )
    }

    #[test]
    fn test_policy_detects_anthropic_family() {
        assert!(is_anthropic_family(&mk_claude_engine()));
        assert!(!is_anthropic_family(&mk_gpt_engine()));
    }

    #[test]
    fn test_policy_claude_model_ok_in_claude_harness() {
        assert!(engine_harness_policy(BackendKind::Claude, &mk_claude_engine()).is_ok());
    }

    #[test]
    fn test_policy_claude_model_denied_in_every_other_harness() {
        for h in [
            BackendKind::Codex,
            BackendKind::Gemini,
            BackendKind::Crush,
            BackendKind::OpenCode,
            BackendKind::Pi,
            BackendKind::OpenAiApi,
            BackendKind::AnthropicApi,
        ] {
            assert!(
                engine_harness_policy(h, &mk_claude_engine()).is_err(),
                "Claude model must be denied in {} harness",
                h.label()
            );
        }
    }

    #[test]
    fn test_policy_anthropic_api_backend_denied_for_all_engines() {
        // billing guard: AnthropicApi is forbidden even for a non-Claude engine.
        assert!(engine_harness_policy(BackendKind::AnthropicApi, &mk_gpt_engine()).is_err());
        assert!(engine_harness_policy(BackendKind::AnthropicApi, &mk_claude_engine()).is_err());
    }

    #[test]
    fn test_policy_gpt_allowed_in_claude_harness() {
        // the intended cross-engine vector: OpenAI/GPT in the Claude Code harness.
        assert!(engine_harness_policy(BackendKind::Claude, &mk_gpt_engine()).is_ok());
        // and still allowed in its native OpenAI-format harnesses.
        assert!(engine_harness_policy(BackendKind::OpenCode, &mk_gpt_engine()).is_ok());
    }

    #[test]
    fn test_engine_env_blocks_claude_via_anthropic_api() {
        // integration: the gate fires inside engine_env, closing the token-billing leak.
        let err = engine_env(BackendKind::AnthropicApi, &mk_claude_engine(), |_| {
            Some("k".into())
        });
        assert!(err.is_err());
        let err = engine_env(BackendKind::OpenCode, &mk_claude_engine(), |_| {
            Some("k".into())
        });
        assert!(
            err.is_err(),
            "Claude model must not bind to a non-Claude harness"
        );
    }

    #[test]
    fn test_engine_env_claude_rejects_openai_only_engine() {
        let engine = mk_engine(
            "gpt-only",
            "OpenAI",
            "gpt-4o",
            Some("https://api.openai.com"),
            Some("OPENAI_API_KEY"),
            vec![ApiFormat::OpenAI],
            EngineLocation::Cloud,
        );
        let stub = |_: &str| Some("x".to_string());
        let err = engine_env(BackendKind::Claude, &engine, stub);
        assert!(
            err.is_err(),
            "claude + OpenAI-only engine must be incompatible"
        );
    }

    #[test]
    fn test_engine_env_local_no_key_no_auth_token() {
        // Local ollama engine speaking Anthropic format with NO api_key_env.
        let engine = mk_engine(
            "ollama-deepseek",
            "Ollama",
            "deepseek-v4-flash",
            Some("http://localhost:11434"),
            None, // no key
            vec![ApiFormat::Anthropic],
            EngineLocation::Local,
        );
        let stub = |_: &str| None;
        let env =
            engine_env(BackendKind::Claude, &engine, stub).expect("local engine, no key, no err");
        let map: std::collections::HashMap<_, _> = env.into_iter().collect();
        assert!(
            map.get("ANTHROPIC_AUTH_TOKEN").is_none(),
            "no key → no AUTH_TOKEN pair"
        );
        assert_eq!(
            map.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("http://localhost:11434")
        );
        assert_eq!(
            map.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("deepseek-v4-flash")
        );
    }

    #[test]
    fn test_engine_env_pi_is_noop() {
        let engine = mk_engine(
            "any",
            "Multi",
            "x",
            Some("https://h"),
            Some("K"),
            vec![ApiFormat::OpenAI],
            EngineLocation::Cloud,
        );
        let env = engine_env(BackendKind::Pi, &engine, |_| Some("k".into())).unwrap();
        assert!(env.is_empty());
    }

    #[test]
    fn test_engine_env_codex_rejects_non_cli_engine() {
        let engine = mk_engine(
            "gpt",
            "OpenAI",
            "gpt-4o",
            Some("https://h"),
            Some("K"),
            vec![ApiFormat::OpenAI],
            EngineLocation::Cloud,
        );
        assert!(engine_env(BackendKind::Codex, &engine, |_| Some("k".into())).is_err());
        // native cli-default engine is accepted as no-op
        let cli_engine = mk_engine(
            "codex-native",
            "OpenAI",
            "gpt-5-codex",
            None,
            None,
            vec![ApiFormat::Cli],
            EngineLocation::Cloud,
        );
        assert!(
            engine_env(BackendKind::Codex, &cli_engine, |_| None)
                .unwrap()
                .is_empty()
        );
    }

    // ── URL composition ──────────────────────────────────────────────────

    #[test]
    fn test_url_composition() {
        assert_eq!(
            anthropic_url("https://api.deepseek.com"),
            "https://api.deepseek.com/v1/messages"
        );
        assert_eq!(
            anthropic_url("https://api.anthropic.com/"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            openai_url("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    // ── valve gating ─────────────────────────────────────────────────────

    #[test]
    fn test_engine_selectable_local_never_gated() {
        let mut valves = ValveStore::default();
        valves.valves.insert(
            "Ollama".into(),
            orrch_library::Valve {
                closed: true,
                reopen_at: None,
                reason: "off".into(),
            },
        );
        let local = mk_engine(
            "o",
            "Ollama",
            "x",
            None,
            None,
            vec![ApiFormat::Cli],
            EngineLocation::Local,
        );
        assert!(
            engine_selectable(&local, &valves),
            "local engine never gated even with closed valve"
        );
    }

    #[test]
    fn test_engine_selectable_cloud_gated_by_valve() {
        let mut valves = ValveStore::default();
        valves.valves.insert(
            "DeepSeek".into(),
            orrch_library::Valve {
                closed: true,
                reopen_at: None,
                reason: "off".into(),
            },
        );
        let cloud = mk_engine(
            "d",
            "DeepSeek",
            "x",
            None,
            Some("K"),
            vec![ApiFormat::OpenAI],
            EngineLocation::Cloud,
        );
        assert!(
            !engine_selectable(&cloud, &valves),
            "closed valve hides cloud engine"
        );
        // open valve → selectable (mutate map directly, avoid disk save())
        valves.valves.remove("DeepSeek");
        assert!(engine_selectable(&cloud, &valves));
    }

    // ── ENG-007: importance → agent-layer engine ─────────────────────────

    #[test]
    fn test_agent_layer_engine_routine_picks_standard() {
        assert_eq!(
            agent_layer_engine(Some("std"), Some("opt"), Importance::Routine),
            Some("std".to_string())
        );
    }

    #[test]
    fn test_agent_layer_engine_critical_picks_optimal() {
        assert_eq!(
            agent_layer_engine(Some("std"), Some("opt"), Importance::Critical),
            Some("opt".to_string())
        );
        assert_eq!(
            agent_layer_engine(Some("std"), Some("opt"), Importance::Important),
            Some("opt".to_string())
        );
    }

    #[test]
    fn test_agent_layer_engine_critical_falls_back_to_standard() {
        // optimal absent → critical falls back to standard
        assert_eq!(
            agent_layer_engine(Some("std"), None, Importance::Critical),
            Some("std".to_string())
        );
    }

    #[test]
    fn test_agent_layer_engine_none_yields_none() {
        assert_eq!(agent_layer_engine(None, None, Importance::Routine), None);
        assert_eq!(agent_layer_engine(None, None, Importance::Critical), None);
    }

    #[test]
    fn test_importance_integration_with_precedence() {
        // Critical-resolved agent id populates the agent layer and wins over
        // project/global, picking the OPTIMAL id.
        let agent_role = agent_layer_engine(Some("sonnet"), Some("opus"), Importance::Critical);
        let layers = EngineLayers {
            session_pick: None,
            agent_role: agent_role.clone(),
            project_default: Some("proj-engine".into()),
            global_default: Some("glob-engine".into()),
        };
        let r = resolve_engine_id(&layers, "fb");
        assert_eq!(r.source, EngineSource::AgentRole);
        assert_eq!(r.engine_id, "opus");

        // a session pick still overrides the importance-driven agent layer.
        let layers2 = EngineLayers {
            session_pick: Some("user-pick".into()),
            ..layers
        };
        let r2 = resolve_engine_id(&layers2, "fb");
        assert_eq!(r2.source, EngineSource::Session);
        assert_eq!(r2.engine_id, "user-pick");
    }

    // ── ENG-009: decide_engine deploy handoff ────────────────────────────

    #[test]
    fn test_decide_engine_routine_standard_agent() {
        let d = decide_engine(
            None,
            Some("sonnet"),
            None,
            None,
            None,
            Importance::Routine,
            "",
        );
        assert_eq!(d.source, EngineSource::AgentRole);
        assert_eq!(d.engine_id, "sonnet");
        assert!(
            d.rationale.contains("standard"),
            "rationale: {}",
            d.rationale
        );
    }

    #[test]
    fn test_decide_engine_critical_optimal_agent() {
        let d = decide_engine(
            None,
            Some("sonnet"),
            Some("opus"),
            None,
            None,
            Importance::Critical,
            "",
        );
        assert_eq!(d.source, EngineSource::AgentRole);
        assert_eq!(d.engine_id, "opus");
        assert!(
            d.rationale.contains("optimal"),
            "rationale: {}",
            d.rationale
        );
    }

    #[test]
    fn test_decide_engine_session_pick_overrides() {
        let d = decide_engine(
            Some("user-pick"),
            Some("sonnet"),
            Some("opus"),
            Some("proj"),
            Some("glob"),
            Importance::Critical,
            "",
        );
        assert_eq!(d.source, EngineSource::Session);
        assert_eq!(d.engine_id, "user-pick");
        assert!(
            d.rationale.contains("overrides"),
            "rationale: {}",
            d.rationale
        );
    }

    // ── LOOP-012: class → default engine ─────────────────────────────────

    #[test]
    fn test_class_default_engine_support_is_gpt() {
        use crate::loops::{LoopClass, SupportKind};
        assert_eq!(
            class_default_engine(LoopClass::Support(SupportKind::Planning)),
            "GPT-4o"
        );
        assert_eq!(
            class_default_engine(LoopClass::Support(SupportKind::Analysis)),
            "GPT-4o"
        );
        assert_eq!(
            class_default_engine(LoopClass::Support(SupportKind::Testing)),
            "GPT-4o"
        );
        assert_eq!(
            class_default_engine(LoopClass::Support(SupportKind::Research)),
            "GPT-4o"
        );
        assert_eq!(
            class_default_engine(LoopClass::Support(SupportKind::Critique)),
            "GPT-4o"
        );
        // GPT engine id
        assert!(class_default_engine(LoopClass::Support(SupportKind::Planning)).contains("GPT"));
    }

    #[test]
    fn test_class_default_engine_dev_is_claude() {
        use crate::loops::LoopClass;
        assert_eq!(class_default_engine(LoopClass::Dev), "Claude Sonnet 4.6");
        assert!(class_default_engine(LoopClass::Dev).contains("Claude"));
    }

    #[test]
    fn test_resolve_loop_engine_class_default_when_nothing_else() {
        use crate::loops::{LoopClass, SupportKind};
        // No overrides → Support loop resolves to the GPT class default, riding
        // the project slot (source ProjectDefault).
        let r = resolve_loop_engine_id(
            LoopClass::Support(SupportKind::Planning),
            None,
            None,
            None,
            None,
            None,
            "fb",
        );
        assert_eq!(r.engine_id, "GPT-4o");
        assert_eq!(r.source, EngineSource::ProjectDefault);

        let r2 = resolve_loop_engine_id(LoopClass::Dev, None, None, None, None, None, "fb");
        assert_eq!(r2.engine_id, "Claude Sonnet 4.6");
    }

    #[test]
    fn test_resolve_loop_engine_session_pick_overrides_class_default() {
        use crate::loops::{LoopClass, SupportKind};
        let r = resolve_loop_engine_id(
            LoopClass::Support(SupportKind::Analysis),
            Some("user-pick"),
            None,
            None,
            None,
            None,
            "fb",
        );
        assert_eq!(r.engine_id, "user-pick");
        assert_eq!(r.source, EngineSource::Session);
    }

    #[test]
    fn test_resolve_loop_engine_agent_role_overrides_class_default() {
        use crate::loops::LoopClass;
        let r = resolve_loop_engine_id(
            LoopClass::Dev,
            None,
            Some("agent-engine"),
            None,
            None,
            None,
            "fb",
        );
        assert_eq!(r.engine_id, "agent-engine");
        assert_eq!(r.source, EngineSource::AgentRole);
    }

    #[test]
    fn test_resolve_loop_engine_explicit_class_override_beats_class_default() {
        use crate::loops::{LoopClass, SupportKind};
        // A pinned class_override replaces the GPT default but still rides the
        // project slot (below session/agent).
        let r = resolve_loop_engine_id(
            LoopClass::Support(SupportKind::Research),
            None,
            None,
            Some("pinned-engine"),
            None,
            None,
            "fb",
        );
        assert_eq!(r.engine_id, "pinned-engine");
        assert_eq!(r.source, EngineSource::ProjectDefault);
    }

    #[test]
    fn test_resolve_loop_engine_project_default_beats_class_default() {
        use crate::loops::{LoopClass, SupportKind};
        // When the project DOES express a preference, it occupies the slot and
        // the class default is dropped (class default only fills an empty slot).
        let r = resolve_loop_engine_id(
            LoopClass::Support(SupportKind::Planning),
            None,
            None,
            None,
            Some("proj-engine"),
            None,
            "fb",
        );
        assert_eq!(r.engine_id, "proj-engine");
        assert_eq!(r.source, EngineSource::ProjectDefault);
    }

    #[test]
    fn test_decide_engine_all_none_builtin_legacy_path() {
        // legacy no-engine spawn: fallback "" → Builtin, engine_id "".
        let d = decide_engine(None, None, None, None, None, Importance::Routine, "");
        assert_eq!(d.source, EngineSource::Builtin);
        assert_eq!(d.engine_id, "");
        assert!(
            d.rationale.contains("harness uses its own default"),
            "rationale: {}",
            d.rationale
        );
    }
}
