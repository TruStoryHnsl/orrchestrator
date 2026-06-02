use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// The control points orrchestrator can exert on a harness (CTX-004). Each is
/// an independent capability bit. Default = all false (a harness we can only
/// spawn blindly). `#[serde(default)]` on every field so a `HarnessEntry`
/// deserialized from a legacy .md (no matrix keys) yields an all-false matrix
/// cleanly.
///
/// CRITICAL: the harness frontmatter parser is FLAT (`extract_field_pub` —
/// key:value lines only, no nested-map support). The matrix is therefore
/// expressed as SEVEN flat scalar bool frontmatter keys, NOT a nested
/// `flexibility:` object. Legacy harness .md files lack all seven → every
/// field defaults false.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlexibilityMatrix {
    /// Can we pass project context via environment variables at spawn?
    #[serde(default)]
    pub env_injection: bool,
    /// Can we steer mid-flight over an RPC/JSONL channel (pi --mode rpc)?
    #[serde(default)]
    pub rpc_steering: bool,
    /// Can we inject a context prompt blob at/after spawn?
    #[serde(default)]
    pub prompt_injection: bool,
    /// Does it consume an MCP server for tools/resources (orrch MCP)?
    #[serde(default)]
    pub mcp_tools: bool,
    /// Can we allow/deny specific tools (gate the tool surface)?
    #[serde(default)]
    pub tool_gating: bool,
    /// Can we replace/augment the system prompt?
    #[serde(default)]
    pub system_prompt_override: bool,
    /// Can we steer an already-running session (vs. spawn-time only)?
    #[serde(default)]
    pub mid_run_steering: bool,
}

impl FlexibilityMatrix {
    /// The set of control points that are ENABLED (true), for superset tests.
    /// Used by the CTX-004 acceptance test: pi ⊇ Claude Code.
    pub fn enabled_points(&self) -> std::collections::BTreeSet<&'static str> {
        let mut set = std::collections::BTreeSet::new();
        if self.env_injection {
            set.insert("env_injection");
        }
        if self.rpc_steering {
            set.insert("rpc_steering");
        }
        if self.prompt_injection {
            set.insert("prompt_injection");
        }
        if self.mcp_tools {
            set.insert("mcp_tools");
        }
        if self.tool_gating {
            set.insert("tool_gating");
        }
        if self.system_prompt_override {
            set.insert("system_prompt_override");
        }
        if self.mid_run_steering {
            set.insert("mid_run_steering");
        }
        set
    }

    /// True iff every control point enabled in `other` is also enabled in self
    /// (self is a superset of other's controllable points). Drives the unit
    /// test asserting `pi.is_superset_of(claude_code)`.
    pub fn is_superset_of(&self, other: &FlexibilityMatrix) -> bool {
        self.enabled_points().is_superset(&other.enabled_points())
    }

    /// Parse the seven flat bool keys from already-extracted frontmatter using
    /// the existing `store::extract_field_pub`. A missing OR non-`true` value →
    /// false. Never errors; legacy frontmatter yields `Default::default()`.
    pub fn from_frontmatter(fm: &str) -> FlexibilityMatrix {
        let flag = |key: &str| -> bool {
            crate::store::extract_field_pub(fm, key)
                .map(|v| v.trim().eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        };
        FlexibilityMatrix {
            env_injection: flag("env_injection"),
            rpc_steering: flag("rpc_steering"),
            prompt_injection: flag("prompt_injection"),
            mcp_tools: flag("mcp_tools"),
            tool_gating: flag("tool_gating"),
            system_prompt_override: flag("system_prompt_override"),
            mid_run_steering: flag("mid_run_steering"),
        }
    }
}

// ─── LOOP-013: per-loop-class tool policy (PURE) ────────────────────────────
//
// Lives next to FlexibilityMatrix because enforcement happens at the CTX-004
// `tool_gating` control point. orrch-library is a LEAF crate (orrch-core
// depends on it, not vice versa) so the policy fns take a plain
// `class_is_support: bool` rather than the orrch-core `LoopClass` enum — the
// caller (orrch-core spawn flow) passes `schedule.class.is_support()`.

/// Coarse classification of a tool the harness might invoke, derived at the
/// gate from the tool name/kind. Mutating vs non-mutating is the only axis
/// LOOP-013 cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Read,         // Read/cat/grep/find/ls — always safe
    Grep,
    Find,
    Research,     // WebSearch/WebFetch/MCP research tools — safe
    MdWrite,      // write/edit of a *.md artifact — the ONLY write Support allows
    CodeWrite,    // write/edit of a non-.md source file — DENIED for Support
    CodeEdit,     // in-place edit of a non-.md source file — DENIED for Support
    BashMutating, // bash that writes/moves/deletes/builds — DENIED for Support
    BashReadOnly, // bash that only reads (ls/cat/git status) — allowed
    GitCommit,    // git commit/push/tag — DENIED for Support
}

/// LOOP-013 per-class tool policy. `Full` = Dev (no restriction).
/// `SupportReadOnly` = non-destructive: read/grep/find/research/md-write only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    Full,
    SupportReadOnly,
}

impl ToolPolicy {
    /// The policy a loop class enforces. Dev → Full; any Support sub-kind →
    /// SupportReadOnly. Caller passes `class_is_support` (decoupled from the
    /// orrch-core enum to keep this crate a leaf).
    pub fn for_class(class_is_support: bool) -> ToolPolicy {
        if class_is_support {
            ToolPolicy::SupportReadOnly
        } else {
            ToolPolicy::Full
        }
    }
}

/// THE GATE (LOOP-013). PURE. Returns true iff `kind` may run under `policy`.
/// Enforced at the spawn/tool-gate layer (where orrch-library/harness control
/// points are applied), NOT by convention. Dev allows everything; Support
/// denies every mutating/commit kind and permits only
/// read/grep/find/research/md-write/bash-read-only.
pub fn is_tool_allowed(policy: ToolPolicy, kind: ToolKind) -> bool {
    match policy {
        ToolPolicy::Full => true,
        ToolPolicy::SupportReadOnly => matches!(
            kind,
            ToolKind::Read
                | ToolKind::Grep
                | ToolKind::Find
                | ToolKind::Research
                | ToolKind::MdWrite
                | ToolKind::BashReadOnly
        ),
    }
}

/// LOOP-013 spawn precondition. A Support loop can only be PROVABLY
/// non-destructive on a harness that actually supports tool gating (CTX-004).
/// Returns Err with a reason the spawn layer surfaces (refuse, don't run
/// unguarded). Dev imposes no precondition.
pub fn policy_enforceable_on(
    policy: ToolPolicy,
    matrix: &FlexibilityMatrix,
) -> Result<(), &'static str> {
    match policy {
        ToolPolicy::Full => Ok(()),
        ToolPolicy::SupportReadOnly => {
            if matrix.tool_gating {
                Ok(())
            } else {
                Err("support loop requires a tool-gating-capable harness (CTX-004 tool_gating) to be provably non-destructive")
            }
        }
    }
}

/// A registered AI coding harness in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessEntry {
    pub name: String,
    pub command: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub supported_models: Vec<String>,
    pub flags: Vec<String>,
    pub available: bool,
    pub notes: String,
    /// ISO 8601 date/time of the last Mentor-triggered refresh of this entry.
    /// Populated from the YAML frontmatter field `last_checked`; `None` means
    /// the entry has never been verified (PLAN item 58).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<String>,
    /// Per-harness control-depth metadata (CTX-004). Defaults all-false for
    /// legacy .md files and JSON blobs that omit the key.
    #[serde(default)]
    pub flexibility: FlexibilityMatrix,
    #[serde(skip)]
    pub path: PathBuf,
}

impl HarnessEntry {
    pub fn summary_line(&self) -> String {
        let status = if self.available { "●" } else { "○" };
        format!("{} {} — {}", status, self.name, self.description)
    }
}

/// Load harness entries from .md files in a directory.
pub fn load_harnesses(dir: &Path) -> Vec<HarnessEntry> {
    let mut harnesses = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "md") {
                if let Some(mut h) = parse_harness_file(&path) {
                    // Auto-detect availability
                    h.available = which_exists(&h.command);
                    harnesses.push(h);
                }
            }
        }
    }
    harnesses.sort_by(|a, b| b.available.cmp(&a.available).then(a.name.cmp(&b.name)));
    harnesses
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn parse_harness_file(path: &Path) -> Option<HarnessEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let (fm, body) = crate::store::parse_frontmatter_pub(&content)?;

    Some(HarnessEntry {
        name: extract(&fm, "name")?,
        command: extract(&fm, "command").unwrap_or_default(),
        description: extract(&fm, "description").unwrap_or_default(),
        capabilities: extract_list(&fm, "capabilities"),
        limitations: extract_list(&fm, "limitations"),
        supported_models: extract_list(&fm, "supported_models"),
        flags: extract_list(&fm, "flags"),
        available: false, // set by load_harnesses
        notes: body.trim().to_string(),
        last_checked: extract(&fm, "last_checked"),
        flexibility: FlexibilityMatrix::from_frontmatter(&fm),
        path: path.to_path_buf(),
    })
}

fn extract(fm: &str, key: &str) -> Option<String> {
    crate::store::extract_field_pub(fm, key)
}

fn extract_list(fm: &str, key: &str) -> Vec<String> {
    crate::store::extract_list_pub(fm, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_summary() {
        let h = HarnessEntry {
            name: "Claude Code".into(),
            command: "claude".into(),
            description: "Full agentic coding".into(),
            capabilities: vec!["tool_use".into(), "subagents".into()],
            limitations: vec![],
            supported_models: vec!["claude-opus-4-6".into()],
            flags: vec!["--dangerously-skip-permissions".into()],
            available: true,
            notes: String::new(),
            last_checked: None,
            flexibility: FlexibilityMatrix::default(),
            path: PathBuf::new(),
        };
        assert!(h.summary_line().contains("●"));
        assert!(h.summary_line().contains("Claude Code"));
    }

    #[test]
    fn test_harness_last_checked_defaults_none() {
        // parse_harness_file should return None for last_checked when the
        // frontmatter has no such field (the real claude_code.md doesn't).
        let tmp = std::env::temp_dir().join("orrch_last_checked_test.md");
        std::fs::write(
            &tmp,
            "---\nname: LCtest\ncommand: lctest\n---\n\nBody.\n",
        ).unwrap();
        let parsed = parse_harness_file(&tmp).unwrap();
        assert_eq!(parsed.last_checked, None);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_harness_last_checked_parses_when_present() {
        let tmp = std::env::temp_dir().join("orrch_last_checked_test2.md");
        std::fs::write(
            &tmp,
            "---\nname: LCtest\ncommand: lctest\nlast_checked: 2026-04-08\n---\n\nBody.\n",
        ).unwrap();
        let parsed = parse_harness_file(&tmp).unwrap();
        assert_eq!(parsed.last_checked.as_deref(), Some("2026-04-08"));
        let _ = std::fs::remove_file(&tmp);
    }

    // ---- CTX-004 FlexibilityMatrix --------------------------------------

    fn repo_harness(name: &str) -> HarnessEntry {
        // The real library harness .md files live two dirs up from the crate.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../library/harnesses")
            .join(name);
        parse_harness_file(&path)
            .unwrap_or_else(|| panic!("failed to parse {}", path.display()))
    }

    #[test]
    fn test_pi_matrix_parses() {
        let pi = repo_harness("pi.md");
        let m = pi.flexibility;
        assert!(m.rpc_steering, "pi should have rpc_steering");
        assert!(m.mid_run_steering, "pi should have mid_run_steering");
        assert!(m.system_prompt_override);
        assert!(m.tool_gating);
        assert!(m.mcp_tools);
        assert!(m.env_injection);
        assert!(m.prompt_injection);
    }

    #[test]
    fn test_claude_code_matrix_parses() {
        let claude = repo_harness("claude_code.md");
        let m = claude.flexibility;
        assert!(m.env_injection);
        assert!(m.mcp_tools);
        assert!(m.prompt_injection);
        assert!(m.tool_gating);
        // Claude Code cannot be steered over RPC/JSONL nor mid-run.
        assert!(!m.rpc_steering);
        assert!(!m.mid_run_steering);
        assert!(!m.system_prompt_override);
    }

    #[test]
    fn test_pi_is_superset_of_claude_code() {
        let pi = repo_harness("pi.md").flexibility;
        let claude = repo_harness("claude_code.md").flexibility;
        assert!(
            pi.is_superset_of(&claude),
            "pi controllable points must be a superset of claude_code"
        );
        // And NOT vice-versa: pi has rpc_steering/mid_run_steering claude lacks.
        assert!(
            !claude.is_superset_of(&pi),
            "claude_code is a strict subset, not a superset, of pi"
        );
    }

    #[test]
    fn test_legacy_harness_defaults_all_false() {
        // A synthesized .md with ZERO matrix keys parses with the default
        // (all-false) matrix and never panics.
        let tmp = std::env::temp_dir().join("orrch_flex_legacy_test.md");
        std::fs::write(&tmp, "---\nname: X\ncommand: x\n---\nbody").unwrap();
        let parsed = parse_harness_file(&tmp).unwrap();
        assert_eq!(parsed.flexibility, FlexibilityMatrix::default());
        assert!(parsed.flexibility.enabled_points().is_empty());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_from_frontmatter_sets_exactly_named_keys() {
        let m = FlexibilityMatrix::from_frontmatter(
            "env_injection: true\nmcp_tools: true\n",
        );
        assert!(m.env_injection);
        assert!(m.mcp_tools);
        assert!(!m.rpc_steering);
        assert!(!m.prompt_injection);
        assert!(!m.tool_gating);
        assert!(!m.system_prompt_override);
        assert!(!m.mid_run_steering);
        assert_eq!(
            m.enabled_points(),
            ["env_injection", "mcp_tools"].into_iter().collect()
        );
    }

    #[test]
    fn test_harness_entry_json_roundtrip_with_flexibility() {
        let mut h = repo_harness("pi.md");
        h.path = PathBuf::new(); // path is #[serde(skip)]
        let json = serde_json::to_string(&h).unwrap();
        let back: HarnessEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.flexibility, h.flexibility);
    }

    #[test]
    fn test_harness_entry_json_omitting_flexibility_defaults_false() {
        // A JSON blob with no `flexibility` key deserializes to all-false.
        let json = r#"{
            "name":"L","command":"l","description":"",
            "capabilities":[],"limitations":[],"supported_models":[],
            "flags":[],"available":false,"notes":""
        }"#;
        let h: HarnessEntry = serde_json::from_str(json).unwrap();
        assert_eq!(h.flexibility, FlexibilityMatrix::default());
    }

    // ---- LOOP-013 ToolPolicy --------------------------------------------

    #[test]
    fn test_tool_policy_for_class() {
        assert_eq!(ToolPolicy::for_class(true), ToolPolicy::SupportReadOnly);
        assert_eq!(ToolPolicy::for_class(false), ToolPolicy::Full);
    }

    #[test]
    fn test_dev_policy_allows_everything() {
        for kind in [
            ToolKind::Read,
            ToolKind::Grep,
            ToolKind::Find,
            ToolKind::Research,
            ToolKind::MdWrite,
            ToolKind::CodeWrite,
            ToolKind::CodeEdit,
            ToolKind::BashMutating,
            ToolKind::BashReadOnly,
            ToolKind::GitCommit,
        ] {
            assert!(is_tool_allowed(ToolPolicy::Full, kind), "Full must allow {kind:?}");
        }
    }

    #[test]
    fn test_support_policy_denies_mutating_and_commit() {
        for kind in [
            ToolKind::CodeWrite,
            ToolKind::CodeEdit,
            ToolKind::BashMutating,
            ToolKind::GitCommit,
        ] {
            assert!(
                !is_tool_allowed(ToolPolicy::SupportReadOnly, kind),
                "SupportReadOnly must DENY {kind:?}"
            );
        }
    }

    #[test]
    fn test_support_policy_allows_read_research_mdwrite() {
        for kind in [
            ToolKind::Read,
            ToolKind::Grep,
            ToolKind::Find,
            ToolKind::Research,
            ToolKind::MdWrite,
            ToolKind::BashReadOnly,
        ] {
            assert!(
                is_tool_allowed(ToolPolicy::SupportReadOnly, kind),
                "SupportReadOnly must ALLOW {kind:?}"
            );
        }
    }

    #[test]
    fn test_policy_enforceable_on() {
        let gating = FlexibilityMatrix { tool_gating: true, ..Default::default() };
        let no_gating = FlexibilityMatrix { tool_gating: false, ..Default::default() };

        // Full never needs a precondition.
        assert!(policy_enforceable_on(ToolPolicy::Full, &gating).is_ok());
        assert!(policy_enforceable_on(ToolPolicy::Full, &no_gating).is_ok());

        // Support requires a tool-gating-capable harness.
        assert!(policy_enforceable_on(ToolPolicy::SupportReadOnly, &gating).is_ok());
        assert!(
            policy_enforceable_on(ToolPolicy::SupportReadOnly, &no_gating).is_err(),
            "support on non-gating harness must refuse"
        );
    }
}
