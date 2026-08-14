//! Deterministic .md → script compiler.
//!
//! Translates user-authored workforce/team markdown into a fully-resolved
//! `CompiledScript` that the MCP server can return as a dispatch instruction
//! to the harness. The transformation is **purely deterministic** — no LLM
//! calls, no network access, no time-dependent behavior. The same input
//! produces byte-identical output.
//!
//! ## Architecture
//!
//! - `compile_team(team)` produces a CompiledScript for one team — the unit
//!   the harness spawns as a single session.
//! - `compile_workflow(wf, all_teams)` expands a workforce into a sequence
//!   of team-scoped CompiledScripts plus a parent dispatch preamble that
//!   instructs the harness to spawn one session per team sequentially while
//!   keeping prior sessions open. The cleanup team is enforced as the last
//!   entry; if the workforce omits it, the compiler appends it automatically.
//!
//! ## Determinism guarantee
//!
//! The compiler:
//! - never reads the system clock,
//! - never calls into network or LLM APIs,
//! - never hashes anything that includes a wall-clock value,
//! - sorts every collection it iterates over a non-stable input (e.g.,
//!   directory listings) to ensure consistent output across hosts.
//!
//! Two `compile_workflow(...)` calls with byte-identical inputs MUST produce
//! byte-identical CompiledScript instances. This is asserted by integration
//! tests in `crates/orrch-mcp/tests/workflow_compiler.rs`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::template::{Team, TeamRef, TeamStep, Workforce};

/// The default cleanup team identifier. Any workforce that omits a cleanup
/// team in its `## Teams` table gets one appended automatically by the
/// compiler. The team file at `teams/<DEFAULT_CLEANUP_TEAM>.md` must exist.
pub const DEFAULT_CLEANUP_TEAM: &str = "cleanup";

/// Output of compiling a single team — what the harness needs to drive one
/// team's session end-to-end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledScript {
    /// Unique name (team name for team scripts, workforce name for workflow
    /// scripts).
    pub name: String,
    /// One-line description.
    pub description: String,
    /// The dispatch script the harness executes — a deterministic, ordered
    /// sequence of step descriptors with embedded agent prompts. Rendering
    /// this struct via `render()` produces the final string returned over MCP.
    pub steps: Vec<CompiledStep>,
    /// Per-agent prompt body. Indexed by agent profile name. Filled in by
    /// the compiler when an `agents/<name>.md` profile is supplied. Empty
    /// otherwise — the harness can still resolve via `agent_invoke`.
    pub agent_role_prompts: BTreeMap<String, String>,
    /// Optional preamble describing how the harness should orchestrate this
    /// script (e.g. "spawn one session per team, keep prior sessions open").
    /// Workflows always have a non-empty preamble; teams typically don't.
    pub dispatch_preamble: String,
    /// True if this script represents a workflow (multiple teams) rather than
    /// a single team.
    pub is_workflow: bool,
}

impl CompiledScript {
    /// Render the compiled script as a single string suitable for return
    /// from the MCP `workflow_call` / `team_call` tools.
    ///
    /// Output is fully deterministic and self-describing. The harness reads
    /// it top-to-bottom and executes each step; embedded agent prompts mean
    /// no follow-up MCP calls are required for prompt resolution.
    pub fn render(&self) -> String {
        let mut out = String::new();

        if self.is_workflow {
            let _ = writeln!(out, "# Workflow: {}", self.name);
        } else {
            let _ = writeln!(out, "# Team: {}", self.name);
        }
        if !self.description.is_empty() {
            let _ = writeln!(out, "_{}_", self.description);
        }
        out.push('\n');

        if !self.dispatch_preamble.is_empty() {
            out.push_str("## Dispatch instructions\n\n");
            out.push_str(&self.dispatch_preamble);
            if !self.dispatch_preamble.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }

        out.push_str("## Steps\n\n");
        for step in &self.steps {
            let _ = writeln!(out, "### Step {} — {}", step.index, step.agent);
            if let Some(tool) = &step.tool_or_skill {
                let _ = writeln!(out, "- **Tool/Skill:** `{}`", tool);
            }
            let _ = writeln!(out, "- **Operation:** {}", step.operation);
            if let Some(parallel_with) = &step.parallel_with {
                let _ = writeln!(out, "- **Parallel with:** {}", parallel_with.join(", "));
            }
            if let Some(team) = &step.team_scope {
                let _ = writeln!(out, "- **Team scope:** {}", team);
            }
            out.push('\n');
        }

        if !self.agent_role_prompts.is_empty() {
            out.push_str("## Agent role prompts (embedded)\n\n");
            // BTreeMap iterates in sorted key order — deterministic.
            for (agent, prompt) in &self.agent_role_prompts {
                let _ = writeln!(out, "### {}", agent);
                out.push_str(prompt);
                if !prompt.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
        }

        out
    }
}

/// One step in a CompiledScript's pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledStep {
    /// Pipeline order (e.g. "1", "2A", "3").
    pub index: String,
    /// Agent profile name responsible for this step.
    pub agent: String,
    /// Optional tool or skill identifier.
    pub tool_or_skill: Option<String>,
    /// Natural-language operation description.
    pub operation: String,
    /// Other agents (by profile name) running in the same parallel group.
    /// `None` means the step runs alone in its index.
    pub parallel_with: Option<Vec<String>>,
    /// For workflow-scoped scripts: the team this step belongs to. `None`
    /// for team-scoped scripts.
    pub team_scope: Option<String>,
}

/// Compile a single team into a deterministic CompiledScript.
///
/// `agent_profiles` is an optional collection of (agent_name, prompt_body)
/// pairs sourced from `agents/<name>.md`. When present, matching agent
/// prompts are embedded into the script under `agent_role_prompts` so the
/// harness has everything it needs in one payload. When absent, the script
/// still compiles — just without embedded prompts.
pub fn compile_team(team: &Team, agent_profiles: &[(String, String)]) -> CompiledScript {
    let steps = team
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| compile_step(s, &team.steps, i, None))
        .collect::<Vec<_>>();

    let mut agent_role_prompts = BTreeMap::new();
    for step in &team.steps {
        if let Some((_, body)) = agent_profiles.iter().find(|(n, _)| n == &step.agent) {
            agent_role_prompts
                .entry(step.agent.clone())
                .or_insert_with(|| body.clone());
        }
    }

    CompiledScript {
        name: team.name.clone(),
        description: team.description.clone(),
        steps,
        agent_role_prompts,
        dispatch_preamble: String::new(),
        is_workflow: false,
    }
}

/// Compile a workforce into a deterministic CompiledScript that expands all
/// referenced teams into a single ordered step list.
///
/// The compiler ALWAYS ensures the cleanup team is the last entry. If the
/// workforce omits it, the compiler appends a synthetic TeamRef pointing at
/// `DEFAULT_CLEANUP_TEAM`. The cleanup team itself must be present in
/// `all_teams` for the append to materialize steps.
pub fn compile_workflow(
    workforce: &Workforce,
    all_teams: &[Team],
    agent_profiles: &[(String, String)],
) -> CompiledScript {
    // Resolve team references in workforce order, then enforce cleanup-at-end.
    let mut team_refs = workforce.teams.clone();
    team_refs.sort_by_key(|t| t.order);

    // Strip any existing cleanup occurrences and re-append exactly one at the
    // end. This both deduplicates and enforces position.
    team_refs.retain(|t| t.team != DEFAULT_CLEANUP_TEAM);
    team_refs.push(TeamRef {
        order: u32::MAX,
        team: DEFAULT_CLEANUP_TEAM.to_string(),
        description: "MANDATORY workforce-scale reconciliation (auto-appended)".into(),
    });

    let mut steps = Vec::new();
    let mut agent_role_prompts = BTreeMap::new();

    for (team_idx, tref) in team_refs.iter().enumerate() {
        let Some(team) = all_teams.iter().find(|t| {
            // Match either canonical lowercased name with underscores or the
            // bare name token.
            slugify(&t.name) == tref.team.to_lowercase().replace(' ', "_")
                || t.name.eq_ignore_ascii_case(&tref.team)
        }) else {
            // Team missing from the supplied team set — emit a placeholder
            // marker step so the dispatch is auditable rather than silently
            // dropping the team.
            steps.push(CompiledStep {
                index: format!("{}.0", team_idx + 1),
                agent: "Hypervisor".into(),
                tool_or_skill: Some("error:missing_team".into()),
                operation: format!(
                    "team `{}` referenced by workforce `{}` not found in teams/ directory",
                    tref.team, workforce.name
                ),
                parallel_with: None,
                team_scope: Some(tref.team.clone()),
            });
            continue;
        };

        // Compile each team's steps and re-index them in the workflow's
        // global order: <team_position>.<original_index>
        for (i, s) in team.steps.iter().enumerate() {
            let original = compile_step(s, &team.steps, i, Some(team.name.clone()));
            let global_index = format!("{}.{}", team_idx + 1, original.index);
            steps.push(CompiledStep {
                index: global_index,
                ..original
            });
        }

        // Merge per-team agent prompts into the workflow-scoped table.
        for step in &team.steps {
            if let Some((_, body)) = agent_profiles.iter().find(|(n, _)| n == &step.agent) {
                agent_role_prompts
                    .entry(step.agent.clone())
                    .or_insert_with(|| body.clone());
            }
        }
    }

    let dispatch_preamble = build_workflow_preamble(&team_refs);

    CompiledScript {
        name: workforce.name.clone(),
        description: workforce.description.clone(),
        steps,
        agent_role_prompts,
        dispatch_preamble,
        is_workflow: true,
    }
}

/// Convert a team's TeamStep into a CompiledStep, detecting parallel-group
/// peers (steps sharing the same `index` value).
fn compile_step(
    step: &TeamStep,
    all_steps: &[TeamStep],
    self_pos: usize,
    team_scope: Option<String>,
) -> CompiledStep {
    let peers: Vec<String> = all_steps
        .iter()
        .enumerate()
        .filter(|(j, s)| *j != self_pos && s.index == step.index)
        .map(|(_, s)| s.agent.clone())
        .collect();
    let parallel_with = if peers.is_empty() { None } else { Some(peers) };

    CompiledStep {
        index: step.index.clone(),
        agent: step.agent.clone(),
        tool_or_skill: step.tool_or_skill.clone(),
        operation: step.operation.clone(),
        parallel_with,
        team_scope,
    }
}

/// Build the deterministic dispatch preamble for a workflow. Lists each team
/// in execution order and tells the harness how to keep sessions open.
fn build_workflow_preamble(team_refs: &[TeamRef]) -> String {
    let mut out = String::new();
    out.push_str(
        "Spawn ONE harness session per team, in the order listed below. Each \
         team session executes its embedded steps independently.\n\n\
         CRITICAL: Do NOT close prior team sessions when starting the next team. \
         All sessions stay OPEN IN PARALLEL until the workforce-scale cleanup \
         team's `cleanup_summary.md` is written. The user may go back to any \
         open session to send fix prompts before the final merge.\n\n\
         The cleanup team waits for the prior team's session to write a \
         completion marker (idle prompt return), then proceeds. The cleanup \
         team's session writes the final `DEVLOG.md` entry and `cleanup_summary.md`. \
         Once the cleanup team finishes, the dispatching agent (you) closes \
         every open team session for this workforce run, in reverse order.\n\n\
         ## Team execution order\n\n",
    );
    for (i, tref) in team_refs.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}. **{}** — {}",
            i + 1,
            tref.team,
            if tref.description.is_empty() {
                "(no description)"
            } else {
                tref.description.as_str()
            }
        );
    }
    out
}

/// Convert a team display name like "Develop Feature" into the file-stem
/// form "develop_feature" used in `teams/<name>.md` and TeamRef.team.
fn slugify(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::{AgentNode, Connection, DataFlow};

    fn sample_team(name: &str, with_pm: bool) -> Team {
        Team {
            name: name.to_string(),
            description: format!("desc for {}", name),
            agents: vec![
                AgentNode {
                    id: "pm".into(),
                    agent_profile: "Project Manager".into(),
                    user_facing: true,
                    nested_workforce: None,
                },
                AgentNode {
                    id: "dev".into(),
                    agent_profile: "Developer".into(),
                    user_facing: false,
                    nested_workforce: None,
                },
            ],
            connections: vec![Connection {
                from: "pm".into(),
                to: "dev".into(),
                data_type: DataFlow::Instructions,
            }],
            steps: vec![
                TeamStep {
                    index: "1".into(),
                    agent: if with_pm {
                        "Project Manager".into()
                    } else {
                        "Developer".into()
                    },
                    tool_or_skill: Some("skill:plan".into()),
                    operation: "plan the work".into(),
                },
                TeamStep {
                    index: "2".into(),
                    agent: "Developer".into(),
                    tool_or_skill: None,
                    operation: "do the work".into(),
                },
            ],
            summary: String::new(),
        }
    }

    #[test]
    fn compile_team_is_deterministic() {
        let team = sample_team("test", true);
        let prompts = vec![("Project Manager".to_string(), "PM prompt body".to_string())];
        let a = compile_team(&team, &prompts);
        let b = compile_team(&team, &prompts);
        assert_eq!(a, b, "two compilations of the same team must match");
        assert_eq!(a.render(), b.render(), "rendered output must match");
    }

    #[test]
    fn compile_team_embeds_matching_agent_prompts() {
        let team = sample_team("test", true);
        let prompts = vec![
            ("Project Manager".into(), "PM body".into()),
            ("Developer".into(), "Dev body".into()),
            (
                "Researcher".into(),
                "Researcher body — should NOT appear".into(),
            ),
        ];
        let compiled = compile_team(&team, &prompts);
        assert_eq!(compiled.agent_role_prompts.len(), 2);
        assert!(compiled.agent_role_prompts.contains_key("Project Manager"));
        assert!(compiled.agent_role_prompts.contains_key("Developer"));
        assert!(!compiled.agent_role_prompts.contains_key("Researcher"));
    }

    #[test]
    fn compile_workflow_appends_cleanup_when_missing() {
        let cleanup = Team {
            name: "Cleanup".into(),
            description: "cleanup desc".into(),
            agents: vec![],
            connections: vec![],
            steps: vec![TeamStep {
                index: "1".into(),
                agent: "Project Manager".into(),
                tool_or_skill: Some("tool:list_open_branches".into()),
                operation: "list branches".into(),
            }],
            summary: String::new(),
        };
        let dev = sample_team("Develop Feature", true);
        let wf = Workforce {
            name: "Test Workforce".into(),
            description: "wf desc".into(),
            agents: vec![],
            connections: vec![],
            operations: vec![],
            // NOTE: no cleanup in teams list
            teams: vec![TeamRef {
                order: 1,
                team: "develop_feature".into(),
                description: "primary cycle".into(),
            }],
        };
        let all_teams = vec![dev.clone(), cleanup.clone()];
        let compiled = compile_workflow(&wf, &all_teams, &[]);
        assert!(compiled.is_workflow);
        // Last step's team_scope must be "Cleanup".
        let last = compiled.steps.last().unwrap();
        assert_eq!(last.team_scope.as_deref(), Some("Cleanup"));
    }

    #[test]
    fn compile_workflow_dedups_user_supplied_cleanup_and_keeps_at_end() {
        let cleanup = Team {
            name: "Cleanup".into(),
            description: "cleanup desc".into(),
            agents: vec![],
            connections: vec![],
            steps: vec![TeamStep {
                index: "1".into(),
                agent: "Project Manager".into(),
                tool_or_skill: None,
                operation: "x".into(),
            }],
            summary: String::new(),
        };
        let dev = sample_team("Develop Feature", true);
        let wf = Workforce {
            name: "Test".into(),
            description: "".into(),
            agents: vec![],
            connections: vec![],
            operations: vec![],
            teams: vec![
                // user puts cleanup first by mistake
                TeamRef {
                    order: 1,
                    team: "cleanup".into(),
                    description: "".into(),
                },
                TeamRef {
                    order: 2,
                    team: "develop_feature".into(),
                    description: "".into(),
                },
            ],
        };
        let compiled = compile_workflow(&wf, &[dev, cleanup], &[]);
        // Cleanup must be the LAST team_scope, not the first.
        let scopes: Vec<&str> = compiled
            .steps
            .iter()
            .filter_map(|s| s.team_scope.as_deref())
            .collect();
        assert_eq!(scopes.last().copied(), Some("Cleanup"));
        assert_eq!(scopes.iter().filter(|s| **s == "Cleanup").count(), 1);
    }

    #[test]
    fn compile_workflow_emits_missing_team_marker() {
        let dev = sample_team("Develop Feature", true);
        let wf = Workforce {
            name: "Test".into(),
            description: "".into(),
            agents: vec![],
            connections: vec![],
            operations: vec![],
            teams: vec![TeamRef {
                order: 1,
                team: "ghost_team".into(),
                description: "does not exist".into(),
            }],
        };
        // Note: no cleanup team supplied either, so cleanup is also "missing"
        // — that's fine, both should produce error markers.
        let compiled = compile_workflow(&wf, &[dev], &[]);
        let marker_count = compiled
            .steps
            .iter()
            .filter(|s| s.tool_or_skill.as_deref() == Some("error:missing_team"))
            .count();
        assert!(
            marker_count >= 1,
            "expected at least one missing-team marker, got {}",
            marker_count
        );
    }

    #[test]
    fn render_is_byte_identical_across_calls() {
        let team = sample_team("Determinism Check", true);
        let prompts = vec![("Project Manager".into(), "body".into())];
        let compiled = compile_team(&team, &prompts);
        let r1 = compiled.render();
        let r2 = compiled.render();
        assert_eq!(r1, r2);
    }

    #[test]
    fn slugify_normalizes_team_names() {
        assert_eq!(slugify("Develop Feature"), "develop_feature");
        assert_eq!(slugify("Cleanup"), "cleanup");
        assert_eq!(slugify("Mid-Tier Cleanup"), "mid_tier_cleanup");
    }
}
