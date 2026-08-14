//! Lagrangian energy-based ranking of preliminary task plans (T8).
//!
//! The PM-loop-review skill calls [`loop_review`] between Step 2 (plan_tasks)
//! and Step 3 (workflow_cluster) of `teams/develop_feature.md`. The preliminary
//! FIFO task list in `.orrch/plan_preliminary.md` is read; each TASK block is
//! scored by an [`EnergyScorer`]; the ranked output is written to
//! `.orrch/plan_ranked.md` (replacing the preliminary list for downstream
//! consumers).
//!
//! Per PM decisions:
//! - T3-q1: every ranking is logged to `.orrch/lagrangian_history.jsonl` from
//!   day one so a future learner has data.
//! - T3-q2: no mid-wave re-ranking in v1. Entry point invoked only at wave
//!   boundaries.
//!
//! Spec authority:
//! - `plans/specs/lagrangian-selector.md` (W1-B) — primary 5-term linear form.
//! - `plans/research/lagrangian-optimization.md` (W1-C) — action functional
//!   `L = T - V` for the alternative `LagrangianScorer`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::file_registry::FileRegistry;

// ---------------------------------------------------------------------------
// Public file paths
// ---------------------------------------------------------------------------

pub const DEFAULT_PRELIMINARY_PATH: &str = ".orrch/plan_preliminary.md";
pub const DEFAULT_RANKED_PATH: &str = ".orrch/plan_ranked.md";
pub const DEFAULT_HISTORY_PATH: &str = ".orrch/lagrangian_history.jsonl";
pub const DEFAULT_COEFFS_PATH: &str = ".orrch/lagrangian_coeffs.json";

// ---------------------------------------------------------------------------
// Candidate task representation
// ---------------------------------------------------------------------------

/// A parsed TASK block from the preliminary plan. Mirrors the format
/// established in `library/skills/develop-feature.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateStep {
    pub id: String,
    pub description: String,
    pub agent: String,
    pub files: Vec<PathBuf>,
    pub work: String,
    pub acceptance: String,
    pub depends: Vec<String>,
    /// Heuristic edit factor inferred from `work` text. ~0.05 trivial, ~0.25
    /// feature add, ~0.6 refactor, ~1.0 rewrite.
    pub edit_factor: f64,
    /// PM-supplied architectural risk in [0.0, 1.0]. `None` → scorer falls back
    /// to a heuristic derived from `agent` / `work`.
    pub architectural_risk: Option<f64>,
}

impl CandidateStep {
    /// File LOC sum (counts newlines). Files that don't exist contribute 0.
    pub fn predicted_loc(&self, root: &Path) -> u64 {
        self.files
            .iter()
            .map(|p| {
                let abs = if p.is_absolute() {
                    p.clone()
                } else {
                    root.join(p)
                };
                match fs::read_to_string(&abs) {
                    Ok(s) => s.lines().count() as u64,
                    Err(_) => 0,
                }
            })
            .sum()
    }

    /// Rough heuristic — 1 token ≈ 4 chars of source. Files that don't exist
    /// contribute 0.
    pub fn file_token_estimate(&self, root: &Path) -> u64 {
        self.files
            .iter()
            .map(|p| {
                let abs = if p.is_absolute() {
                    p.clone()
                } else {
                    root.join(p)
                };
                match fs::metadata(&abs) {
                    Ok(m) => m.len() / 4,
                    Err(_) => 0,
                }
            })
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Workspace state
// ---------------------------------------------------------------------------

/// Workspace state snapshot passed to scorers.
#[derive(Debug, Clone)]
pub struct WorkspaceState {
    pub project_root: PathBuf,
    /// Output of `git diff --stat HEAD` (best-effort; empty if not a git repo).
    pub diff_stat: String,
    /// Full `git diff HEAD` text capped at 50KB.
    pub diff_full: String,
    /// Total bytes changed across the workspace (rough proxy for diff surface).
    pub diff_bytes: u64,
}

impl WorkspaceState {
    pub fn capture(project_root: &Path) -> Self {
        let diff_stat = run_git(project_root, &["diff", "--stat", "HEAD"]).unwrap_or_default();
        let mut diff_full = run_git(project_root, &["diff", "HEAD"]).unwrap_or_default();
        let cap = 50 * 1024;
        if diff_full.len() > cap {
            diff_full.truncate(cap);
            diff_full.push_str("\n[TRUNCATED]\n");
        }
        let diff_bytes = diff_full.len() as u64;
        Self {
            project_root: project_root.to_path_buf(),
            diff_stat,
            diff_full,
            diff_bytes,
        }
    }
}

fn run_git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

// ---------------------------------------------------------------------------
// ScoringContext + EnergyScorer trait
// ---------------------------------------------------------------------------

/// Context the scorer needs (file registry, all-task list, workspace diff).
/// `#[non_exhaustive]` so future fields don't break callers.
#[derive(Debug)]
#[non_exhaustive]
pub struct ScoringContext<'a> {
    pub registry: Option<&'a FileRegistry>,
    pub all_steps: &'a [CandidateStep],
    pub workspace: &'a WorkspaceState,
}

impl<'a> ScoringContext<'a> {
    /// Public constructor — required because the struct is `#[non_exhaustive]`
    /// so external callers (tests, downstream crates) can't use the
    /// struct-literal form.
    pub fn new(
        registry: Option<&'a FileRegistry>,
        all_steps: &'a [CandidateStep],
        workspace: &'a WorkspaceState,
    ) -> Self {
        Self {
            registry,
            all_steps,
            workspace,
        }
    }
}

/// Term-breakdown of one task's score (logged + rendered in `plan_ranked.md`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TermBreakdown {
    pub reuse: f64,
    pub diff: f64,
    pub tok: f64,
    pub risk: f64,
    pub unblk: f64,
    /// For the `LagrangianScorer`: kinetic term `T`.
    #[serde(default)]
    pub kinetic: f64,
    /// For the `LagrangianScorer`: potential term `V`.
    #[serde(default)]
    pub potential: f64,
    /// For the `LagrangianScorer`: learned cost-to-go (0.0 in v1).
    #[serde(default)]
    pub tail: f64,
}

/// One ranking row. Returned by the scorer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredStep {
    pub step_id: String,
    pub energy: f64,
    pub terms: TermBreakdown,
}

/// Public trait. Implementors compute `E(step)` — lower is better.
pub trait EnergyScorer: std::fmt::Debug {
    /// Compute the energy + term-breakdown for one task.
    fn score(&self, step: &CandidateStep, ctx: &ScoringContext<'_>) -> ScoredStep;
    /// Short identifier used in `lagrangian_history.jsonl`.
    fn name(&self) -> &'static str;
    /// JSON blob describing the scorer's config (logged with each ranking).
    fn config_blob(&self) -> serde_json::Value {
        serde_json::json!({"scorer": self.name()})
    }
}

// ---------------------------------------------------------------------------
// LinearLagrangianScorer (default, 5-term)
// ---------------------------------------------------------------------------

/// Coefficients for the linear 5-term form. Loaded from
/// `.orrch/lagrangian_coeffs.json` if present, else defaults.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LinearCoeffs {
    pub alpha: f64,   // context_reuse_bonus weight
    pub beta: f64,    // diff_surface_penalty weight
    pub gamma: f64,   // token_cost weight
    pub delta: f64,   // architectural_risk weight
    pub epsilon: f64, // blocker_unblocking weight
}

impl Default for LinearCoeffs {
    fn default() -> Self {
        // W1-B spec §3 defaults.
        Self {
            alpha: 0.30,
            beta: 0.15,
            gamma: 0.20,
            delta: 0.20,
            epsilon: 0.15,
        }
    }
}

impl LinearCoeffs {
    /// Try to load from `<root>/.orrch/lagrangian_coeffs.json`. Falls back to
    /// defaults if the file is missing or malformed.
    pub fn load(root: &Path) -> Self {
        let path = root.join(DEFAULT_COEFFS_PATH);
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinearLagrangianScorer {
    pub coeffs: LinearCoeffs,
}

impl LinearLagrangianScorer {
    pub fn new(coeffs: LinearCoeffs) -> Self {
        Self { coeffs }
    }

    /// Default-coefficient instance.
    pub fn defaults() -> Self {
        Self::new(LinearCoeffs::default())
    }

    /// `context_reuse_bonus` ∈ [-1.0, 0.0]. Pure reuse = -1.0; no reuse = 0.0.
    fn context_reuse_bonus(&self, step: &CandidateStep, registry: Option<&FileRegistry>) -> f64 {
        let n = step.files.len().max(1) as f64;
        let already = match registry {
            Some(r) => step.files.iter().filter(|p| r.has_been_acquired(p)).count() as f64,
            None => 0.0,
        };
        -(already / n)
    }

    /// `diff_surface_penalty` ∈ [0.0, 1.0]. Normalized to 1.0 at 500 LOC.
    fn diff_surface_penalty(&self, step: &CandidateStep, root: &Path) -> f64 {
        let loc = step.predicted_loc(root) as f64;
        let predicted = loc * step.edit_factor;
        (predicted / 500.0).min(1.0).max(0.0)
    }

    /// `token_cost` ∈ [0.0, 1.0]. Normalized to 1.0 at 200K tokens.
    fn token_cost(&self, step: &CandidateStep, root: &Path) -> f64 {
        let tok = step.file_token_estimate(root) as f64;
        // Add a baseline agent prompt cost of ~2K tokens.
        let total = tok + 2000.0;
        (total / 200_000.0).min(1.0).max(0.0)
    }

    /// `architectural_risk` ∈ [0.0, 1.0]. Uses PM-supplied value if present;
    /// else heuristic from agent role.
    fn architectural_risk(&self, step: &CandidateStep) -> f64 {
        if let Some(r) = step.architectural_risk {
            return r.clamp(0.0, 1.0);
        }
        // Cheap heuristic: file count + agent role.
        let n = step.files.len();
        let base: f64 = if n <= 1 {
            0.1
        } else if n <= 2 {
            0.3
        } else if n <= 4 {
            0.5
        } else {
            0.8
        };
        let role_bias: f64 = match step.agent.as_str() {
            "Software Engineer" | "Software_Engineer" | "SoftwareEngineer" => 0.1,
            "Researcher" => -0.05,
            _ => 0.0,
        };
        (base + role_bias).clamp(0.0, 1.0)
    }

    /// `blocker_unblocking` ∈ [-1.0, 0.0]. Counts downstream tasks that depend
    /// on this one.
    fn blocker_unblocking(&self, step: &CandidateStep, all: &[CandidateStep]) -> f64 {
        let down: usize = all
            .iter()
            .filter(|t| t.depends.iter().any(|d| d == &step.id))
            .count();
        let normalized = (down as f64 / 3.0).min(1.0);
        -normalized
    }
}

impl EnergyScorer for LinearLagrangianScorer {
    fn score(&self, step: &CandidateStep, ctx: &ScoringContext<'_>) -> ScoredStep {
        let reuse = self.context_reuse_bonus(step, ctx.registry);
        let diff = self.diff_surface_penalty(step, &ctx.workspace.project_root);
        let tok = self.token_cost(step, &ctx.workspace.project_root);
        let risk = self.architectural_risk(step);
        let unblk = self.blocker_unblocking(step, ctx.all_steps);

        let e = self.coeffs.alpha * reuse
            + self.coeffs.beta * diff
            + self.coeffs.gamma * tok
            + self.coeffs.delta * risk
            + self.coeffs.epsilon * unblk;

        ScoredStep {
            step_id: step.id.clone(),
            energy: e,
            terms: TermBreakdown {
                reuse,
                diff,
                tok,
                risk,
                unblk,
                ..Default::default()
            },
        }
    }

    fn name(&self) -> &'static str {
        "linear"
    }

    fn config_blob(&self) -> serde_json::Value {
        serde_json::json!({
            "scorer": "linear",
            "alpha": self.coeffs.alpha,
            "beta":  self.coeffs.beta,
            "gamma": self.coeffs.gamma,
            "delta": self.coeffs.delta,
            "epsilon": self.coeffs.epsilon,
        })
    }
}

// ---------------------------------------------------------------------------
// LagrangianScorer (action functional T - V + tail, per W1-C)
// ---------------------------------------------------------------------------

/// Coefficients of the action-functional decomposition `L = T - V`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ActionCoeffs {
    // Kinetic — cost-of-motion
    pub alpha_tok: f64,
    pub alpha_cpu: f64,
    pub alpha_risk: f64,
    pub alpha_ctx: f64,
    // Potential — terminal value
    pub beta_block: f64,
    pub beta_acc: f64,
    pub beta_reuse: f64,
}

impl Default for ActionCoeffs {
    fn default() -> Self {
        // W1-C "Parameter tuning" table.
        Self {
            alpha_tok: 1e-5,
            alpha_cpu: 1e-3,
            alpha_risk: 1.0,
            alpha_ctx: 1e-4,
            beta_block: 5.0,
            beta_acc: 10.0,
            beta_reuse: 0.5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LagrangianScorer {
    pub coeffs: ActionCoeffs,
}

impl LagrangianScorer {
    pub fn new(coeffs: ActionCoeffs) -> Self {
        Self { coeffs }
    }

    pub fn defaults() -> Self {
        Self::new(ActionCoeffs::default())
    }

    fn kinetic(&self, step: &CandidateStep, ctx: &ScoringContext<'_>) -> f64 {
        let tokens = step.file_token_estimate(&ctx.workspace.project_root) as f64;
        let cpu_seconds = (step.predicted_loc(&ctx.workspace.project_root) as f64 / 50.0).max(0.1);
        let arch_risk = step.architectural_risk.unwrap_or(0.3);
        // Context growth = files-not-yet-owned × estimated bytes / 4.
        let ctx_growth: f64 = match ctx.registry {
            Some(r) => step
                .files
                .iter()
                .filter(|p| !r.has_been_acquired(p))
                .filter_map(|p| {
                    let abs = if p.is_absolute() {
                        p.clone()
                    } else {
                        ctx.workspace.project_root.join(p)
                    };
                    fs::metadata(&abs).ok().map(|m| (m.len() / 4) as f64)
                })
                .sum(),
            None => 0.0,
        };
        self.coeffs.alpha_tok * tokens
            + self.coeffs.alpha_cpu * cpu_seconds
            + self.coeffs.alpha_risk * arch_risk
            + self.coeffs.alpha_ctx * ctx_growth
    }

    fn potential(&self, step: &CandidateStep, ctx: &ScoringContext<'_>) -> f64 {
        // unblocked_downstream — count of tasks depending on this step.
        let unblocked_downstream = ctx
            .all_steps
            .iter()
            .filter(|t| t.depends.iter().any(|d| d == &step.id))
            .count() as f64;
        // acceptance_progress — 1.0 per acceptance criterion the task satisfies.
        // Heuristic: 1.0 if `acceptance` non-empty, else 0.
        let acceptance_progress = if step.acceptance.trim().is_empty() {
            0.0
        } else {
            1.0
        };
        // ctx_reuse — sum of reused-file sizes (kB).
        let ctx_reuse_kb: f64 = match ctx.registry {
            Some(r) => step
                .files
                .iter()
                .filter(|p| r.has_been_acquired(p))
                .filter_map(|p| {
                    let abs = if p.is_absolute() {
                        p.clone()
                    } else {
                        ctx.workspace.project_root.join(p)
                    };
                    fs::metadata(&abs).ok().map(|m| (m.len() as f64) / 1024.0)
                })
                .sum(),
            None => 0.0,
        };
        self.coeffs.beta_block * unblocked_downstream
            + self.coeffs.beta_acc * acceptance_progress
            + self.coeffs.beta_reuse * ctx_reuse_kb
    }
}

impl EnergyScorer for LagrangianScorer {
    fn score(&self, step: &CandidateStep, ctx: &ScoringContext<'_>) -> ScoredStep {
        let t = self.kinetic(step, ctx);
        let v = self.potential(step, ctx);
        // v1: stub the cost-to-go to a constant 0.0 (small-model scorer hookup
        // out of scope per PM-deferred scorer-model selection).
        let tail = 0.0_f64;
        let energy = t - v + tail;
        ScoredStep {
            step_id: step.id.clone(),
            energy,
            terms: TermBreakdown {
                kinetic: t,
                potential: v,
                tail,
                ..Default::default()
            },
        }
    }

    fn name(&self) -> &'static str {
        "lagrangian"
    }

    fn config_blob(&self) -> serde_json::Value {
        serde_json::json!({
            "scorer": "lagrangian",
            "alpha_tok":   self.coeffs.alpha_tok,
            "alpha_cpu":   self.coeffs.alpha_cpu,
            "alpha_risk":  self.coeffs.alpha_risk,
            "alpha_ctx":   self.coeffs.alpha_ctx,
            "beta_block":  self.coeffs.beta_block,
            "beta_acc":    self.coeffs.beta_acc,
            "beta_reuse":  self.coeffs.beta_reuse,
        })
    }
}

// ---------------------------------------------------------------------------
// Scorer selection (per-project override via TOML)
// ---------------------------------------------------------------------------

/// Choose which scorer to use. Default: linear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScorerKind {
    Linear,
    Lagrangian,
}

impl ScorerKind {
    /// Resolve from `~/.config/orrchestrator/lagrangian.toml` (line `scorer =
    /// "linear"` or `"lagrangian"`). Falls back to Linear.
    pub fn resolve() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = PathBuf::from(home).join(".config/orrchestrator/lagrangian.toml");
        let text = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Self::Linear,
        };
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            // Hand-parse: scorer = "linear"
            if let Some(rest) = line.strip_prefix("scorer") {
                let rest = rest.trim_start().trim_start_matches('=').trim();
                let v = rest.trim_matches(|c| c == '"' || c == '\'').trim();
                return match v {
                    "lagrangian" | "t6_action_v1" => Self::Lagrangian,
                    _ => Self::Linear,
                };
            }
        }
        Self::Linear
    }
}

// ---------------------------------------------------------------------------
// Plan parsing (TASK blocks)
// ---------------------------------------------------------------------------

/// Parse the preliminary plan markdown into CandidateSteps. Tolerant of extra
/// blank lines, comment headers, and missing fields (missing → empty).
pub fn parse_task_blocks(text: &str) -> Vec<CandidateStep> {
    let mut out = Vec::new();
    // Split into blocks by lines starting with `TASK `.
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_start();
        if let Some(rest) = line.strip_prefix("TASK ") {
            // Header: "TASK <id>: <description>"
            let (id, desc) = match rest.split_once(':') {
                Some((id, d)) => (id.trim().to_string(), d.trim().to_string()),
                None => (rest.trim().to_string(), String::new()),
            };
            let mut step = CandidateStep {
                id,
                description: desc,
                agent: String::new(),
                files: Vec::new(),
                work: String::new(),
                acceptance: String::new(),
                depends: Vec::new(),
                edit_factor: 0.25,
                architectural_risk: None,
            };
            i += 1;
            while i < lines.len() {
                let next = lines[i].trim_start();
                if next.starts_with("TASK ") {
                    break;
                }
                // Field parsing.
                if let Some(v) = next.strip_prefix("Agent:") {
                    step.agent = v.trim().to_string();
                } else if let Some(v) = next.strip_prefix("Files:") {
                    step.files = v
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty() && *s != "none")
                        .map(PathBuf::from)
                        .collect();
                } else if let Some(v) = next.strip_prefix("Work:") {
                    step.work = v.trim().to_string();
                    step.edit_factor = infer_edit_factor(v);
                } else if let Some(v) = next.strip_prefix("Acceptance:") {
                    step.acceptance = v.trim().to_string();
                } else if let Some(v) = next.strip_prefix("Depends:") {
                    step.depends = v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty() && s != "none")
                        .collect();
                } else if let Some(v) = next.strip_prefix("Risk:") {
                    if let Ok(r) = v.trim().parse::<f64>() {
                        step.architectural_risk = Some(r);
                    }
                } else if let Some(v) = next.strip_prefix("EditFactor:")
                    && let Ok(r) = v.trim().parse::<f64>()
                {
                    step.edit_factor = r;
                }
                i += 1;
            }
            out.push(step);
            continue;
        }
        i += 1;
    }
    out
}

/// Cheap classifier: text in `Work:` field → edit_factor. Defaults to 0.25
/// (feature add).
fn infer_edit_factor(work: &str) -> f64 {
    let w = work.to_ascii_lowercase();
    if w.contains("rewrite") || w.contains("redesign") {
        1.0
    } else if w.contains("refactor") || w.contains("split") || w.contains("extract") {
        0.6
    } else if w.contains("tweak")
        || w.contains("comment")
        || w.contains("typo")
        || w.contains("fix line")
        || w.contains("rename")
        || w.contains("trivial")
    {
        0.05
    } else if w.contains("add ") || w.contains("new ") || w.contains("implement ") {
        0.25
    } else {
        0.25
    }
}

// ---------------------------------------------------------------------------
// Ranked plan emission
// ---------------------------------------------------------------------------

/// Format the ranked plan.md output. Layout per W1-B §4:
/// ```text
/// RANKING (by ascending E — lower is better)
/// TASK 003 | E=-0.42 | reuse=-1.00 diff=0.10 tok=0.05 risk=0.10 unblk=-0.33
/// ...
/// ---
/// TASK 003: <one-line description>
/// Agent: Developer
/// ...
/// ```
pub fn render_ranked_plan(
    scored: &[ScoredStep],
    steps_by_id: &HashMap<String, CandidateStep>,
    scorer_name: &str,
) -> String {
    let mut s = String::new();
    s.push_str("RANKING (by ascending E — lower is better)\n");
    s.push_str(&format!("Scorer: {}\n", scorer_name));
    for r in scored {
        if r.terms.reuse != 0.0
            || r.terms.diff != 0.0
            || r.terms.tok != 0.0
            || r.terms.risk != 0.0
            || r.terms.unblk != 0.0
        {
            s.push_str(&format!(
                "TASK {} | E={:+.4} | reuse={:+.2} diff={:.2} tok={:.2} risk={:.2} unblk={:+.2}\n",
                r.step_id,
                r.energy,
                r.terms.reuse,
                r.terms.diff,
                r.terms.tok,
                r.terms.risk,
                r.terms.unblk
            ));
        } else {
            s.push_str(&format!(
                "TASK {} | E={:+.4} | T={:+.4} V={:+.4} tail={:+.4}\n",
                r.step_id, r.energy, r.terms.kinetic, r.terms.potential, r.terms.tail
            ));
        }
    }
    s.push_str("\n---\n\n");
    for r in scored {
        if let Some(step) = steps_by_id.get(&r.step_id) {
            s.push_str(&format!("TASK {}: {}\n", step.id, step.description));
            if !step.agent.is_empty() {
                s.push_str(&format!("Agent: {}\n", step.agent));
            }
            if !step.files.is_empty() {
                let files: Vec<String> = step
                    .files
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                s.push_str(&format!("Files: {}\n", files.join(", ")));
            }
            if !step.work.is_empty() {
                s.push_str(&format!("Work: {}\n", step.work));
            }
            if !step.acceptance.is_empty() {
                s.push_str(&format!("Acceptance: {}\n", step.acceptance));
            }
            let dep = if step.depends.is_empty() {
                "none".to_string()
            } else {
                step.depends.join(", ")
            };
            s.push_str(&format!("Depends: {}\n", dep));
            s.push('\n');
        }
    }
    s
}

// ---------------------------------------------------------------------------
// History logging (per PM T3-q1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRow {
    pub timestamp_us: u128,
    pub scorer_name: String,
    pub candidate_step_id: String,
    pub raw_term_values: TermBreakdown,
    pub weighted_e_score: f64,
    pub final_rank: usize,
    pub scorer_config: serde_json::Value,
}

fn append_history(
    root: &Path,
    scorer: &dyn EnergyScorer,
    scored: &[ScoredStep],
) -> std::io::Result<()> {
    let path = root.join(DEFAULT_HISTORY_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let now_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    let config = scorer.config_blob();
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    for (rank, r) in scored.iter().enumerate() {
        let row = HistoryRow {
            timestamp_us: now_us,
            scorer_name: scorer.name().to_string(),
            candidate_step_id: r.step_id.clone(),
            raw_term_values: r.terms.clone(),
            weighted_e_score: r.energy,
            final_rank: rank + 1,
            scorer_config: config.clone(),
        };
        let line = serde_json::to_string(&row).unwrap_or_default();
        writeln!(f, "{}", line)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Result of one `loop_review` invocation.
#[derive(Debug, Clone)]
pub struct LoopReviewOutcome {
    pub scorer_name: &'static str,
    pub ranked: Vec<ScoredStep>,
    pub ranked_path: PathBuf,
    pub history_path: PathBuf,
}

/// Score every preliminary task and write `.orrch/plan_ranked.md`. Logs to
/// `.orrch/lagrangian_history.jsonl`. Active scorer is resolved from
/// `~/.config/orrchestrator/lagrangian.toml` (default: linear with coeffs from
/// `.orrch/lagrangian_coeffs.json` if present).
pub fn loop_review(
    workspace_root: &Path,
    current_task_list: &[CandidateStep],
    file_registry: Option<&FileRegistry>,
) -> std::io::Result<LoopReviewOutcome> {
    // Pick scorer.
    let kind = ScorerKind::resolve();
    let workspace = WorkspaceState::capture(workspace_root);
    let ctx = ScoringContext {
        registry: file_registry,
        all_steps: current_task_list,
        workspace: &workspace,
    };

    let outcome = match kind {
        ScorerKind::Linear => {
            let coeffs = LinearCoeffs::load(workspace_root);
            let scorer = LinearLagrangianScorer::new(coeffs);
            rank_with(&scorer, current_task_list, &ctx, workspace_root)?
        }
        ScorerKind::Lagrangian => {
            let scorer = LagrangianScorer::defaults();
            rank_with(&scorer, current_task_list, &ctx, workspace_root)?
        }
    };
    Ok(outcome)
}

/// Score-with-a-specific-scorer entry point (exposed for tests and embedding).
pub fn loop_review_with(
    workspace_root: &Path,
    current_task_list: &[CandidateStep],
    file_registry: Option<&FileRegistry>,
    scorer: &dyn EnergyScorer,
) -> std::io::Result<LoopReviewOutcome> {
    let workspace = WorkspaceState::capture(workspace_root);
    let ctx = ScoringContext {
        registry: file_registry,
        all_steps: current_task_list,
        workspace: &workspace,
    };
    rank_with(scorer, current_task_list, &ctx, workspace_root)
}

fn rank_with(
    scorer: &dyn EnergyScorer,
    steps: &[CandidateStep],
    ctx: &ScoringContext<'_>,
    workspace_root: &Path,
) -> std::io::Result<LoopReviewOutcome> {
    let mut scored: Vec<ScoredStep> = steps.iter().map(|s| scorer.score(s, ctx)).collect();
    // Ascending E — lower is better.
    scored.sort_by(|a, b| {
        a.energy
            .partial_cmp(&b.energy)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.step_id.cmp(&b.step_id))
    });

    let steps_by_id: HashMap<String, CandidateStep> =
        steps.iter().map(|s| (s.id.clone(), s.clone())).collect();

    let text = render_ranked_plan(&scored, &steps_by_id, scorer.name());
    let ranked_path = workspace_root.join(DEFAULT_RANKED_PATH);
    if let Some(parent) = ranked_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&ranked_path, text)?;

    let history_path = workspace_root.join(DEFAULT_HISTORY_PATH);
    let _ = append_history(workspace_root, scorer, &scored);

    // Convert the scorer's name() (a &'static str) into something the outcome
    // can carry. The trait method returns &'static str so this is safe.
    Ok(LoopReviewOutcome {
        scorer_name: scorer.name(),
        ranked: scored,
        ranked_path,
        history_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(
        id: &str,
        files: &[&str],
        depends: &[&str],
        edit_factor: f64,
        risk: f64,
    ) -> CandidateStep {
        CandidateStep {
            id: id.to_string(),
            description: format!("desc {}", id),
            agent: "Developer".to_string(),
            files: files.iter().map(PathBuf::from).collect(),
            work: format!("work for {}", id),
            acceptance: format!("acc for {}", id),
            depends: depends.iter().map(|s| s.to_string()).collect(),
            edit_factor,
            architectural_risk: Some(risk),
        }
    }

    #[test]
    fn linear_coeffs_default_sum_to_one() {
        let c = LinearCoeffs::default();
        let s = c.alpha + c.beta + c.gamma + c.delta + c.epsilon;
        assert!(
            (s - 1.0).abs() < 1e-9,
            "coeffs should sum to 1.0, got {}",
            s
        );
    }

    #[test]
    fn parse_task_blocks_basic() {
        let text = r#"
TASK 1: do a thing
Agent: Developer
Files: a.rs, b.rs
Work: refactor the parser
Acceptance: tests pass
Depends: none

TASK 2: do another thing
Agent: Researcher
Files: none
Work: investigate
Acceptance: write report
Depends: 1
"#;
        let v = parse_task_blocks(text);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].id, "1");
        assert_eq!(v[0].agent, "Developer");
        assert_eq!(v[0].files.len(), 2);
        assert_eq!(v[1].depends, vec!["1".to_string()]);
        // "refactor" → edit_factor 0.6
        assert!((v[0].edit_factor - 0.6).abs() < 1e-9);
    }

    #[test]
    fn render_plan_contains_ranking_and_blocks() {
        let s = step("3", &["a.rs"], &[], 0.05, 0.1);
        let scored = vec![ScoredStep {
            step_id: "3".to_string(),
            energy: -0.287,
            terms: TermBreakdown {
                reuse: -1.0,
                diff: 0.02,
                tok: 0.05,
                risk: 0.1,
                unblk: 0.0,
                ..Default::default()
            },
        }];
        let mut m = HashMap::new();
        m.insert("3".to_string(), s);
        let text = render_ranked_plan(&scored, &m, "linear");
        assert!(text.starts_with("RANKING"));
        assert!(text.contains("TASK 3 | E=-0.2870"));
        assert!(text.contains("TASK 3: desc 3"));
    }
}
