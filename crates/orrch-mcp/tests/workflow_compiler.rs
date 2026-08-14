//! Integration tests for the workflow/team compilation MCP tools.
//!
//! Asserts the workflow_call / team_call dispatch path is fully deterministic
//! (byte-identical output across consecutive calls), expands every team in
//! workforce order, ALWAYS appends the cleanup team at the end, and is
//! reachable via the MCP `dispatch()` entry point without network access.

use std::path::PathBuf;

use orrch_mcp::{server::OrrchMcpServer, tools::dispatch};
use serde_json::json;

/// Build a server pointed at the test fixtures directory.
fn fixture_server() -> OrrchMcpServer {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures = manifest_dir.join("tests/fixtures");
    OrrchMcpServer {
        agents_dir: fixtures.join("agents"),
        skills_dir: fixtures.join("skills"), // empty dir is fine
        workforces_dir: fixtures.join("workforces"),
        teams_dir: fixtures.join("teams"),
        library_dir: fixtures.join("library"),
        projects_dir: fixtures.join("projects"),
    }
}

#[tokio::test]
async fn workflow_call_is_deterministic() {
    let server = fixture_server();
    let args = json!({
        "workflow": "test_workforce",
        "goal": "test goal",
        "project_dir": "/tmp/fixture",
    });
    let r1 = dispatch(&server, "workflow_call", &args).await;
    let r2 = dispatch(&server, "workflow_call", &args).await;
    assert_eq!(
        r1, r2,
        "two consecutive workflow_call invocations must be byte-identical"
    );
    assert!(
        !r1.starts_with("Error:"),
        "workflow_call returned error: {r1}"
    );
}

#[tokio::test]
async fn workflow_call_lists_each_team_in_order() {
    let server = fixture_server();
    let args = json!({
        "workflow": "test_workforce",
        "goal": "x",
        "project_dir": "/tmp",
    });
    let body = dispatch(&server, "workflow_call", &args).await;

    // The fixture workforce declares: develop_feature, develop_feature, cleanup.
    // Each step gets a global index of the form "<team_position>.<orig>".
    // We expect at minimum 3 distinct team positions to appear.
    let has_pos_1 = body.lines().any(|l| l.contains("Step 1.1"));
    let has_pos_2 = body.lines().any(|l| l.contains("Step 2.1"));
    let has_pos_3 = body.lines().any(|l| l.contains("Step 3.1"));
    assert!(has_pos_1, "expected Step 1.1 in body:\n{body}");
    assert!(has_pos_2, "expected Step 2.1 in body:\n{body}");
    assert!(has_pos_3, "expected Step 3.1 in body:\n{body}");
}

#[tokio::test]
async fn workflow_call_ends_with_cleanup() {
    let server = fixture_server();
    let body = dispatch(
        &server,
        "workflow_call",
        &json!({"workflow": "test_workforce"}),
    )
    .await;

    // Find the last "Team scope:" line in the rendered output. It must be Cleanup.
    let last_scope = body
        .lines()
        .filter_map(|l| l.strip_prefix("- **Team scope:** "))
        .next_back()
        .unwrap_or_else(|| panic!("no Team scope: lines found:\n{body}"));
    assert_eq!(
        last_scope, "Cleanup",
        "workflow must end with the Cleanup team, got '{last_scope}':\n{body}"
    );
}

#[tokio::test]
async fn workflow_call_dispatch_preamble_keeps_sessions_open() {
    let server = fixture_server();
    let body = dispatch(
        &server,
        "workflow_call",
        &json!({"workflow": "test_workforce"}),
    )
    .await;
    assert!(
        body.contains("Do NOT close prior team sessions"),
        "preamble must instruct the harness to keep prior sessions open:\n{body}"
    );
    assert!(
        body.contains("Spawn ONE harness session per team"),
        "preamble must instruct one-session-per-team:\n{body}"
    );
}

#[tokio::test]
async fn team_call_cleanup_returns_deterministic_dispatch() {
    let server = fixture_server();
    let args = json!({"team": "cleanup", "goal": "wrap up", "project_dir": "/tmp"});
    let r1 = dispatch(&server, "team_call", &args).await;
    let r2 = dispatch(&server, "team_call", &args).await;
    assert_eq!(r1, r2, "team_call must be byte-identical across calls");
    assert!(!r1.starts_with("Error:"), "team_call returned error: {r1}");
    assert!(r1.contains("Team Dispatch — cleanup"));
    assert!(r1.contains("# Team: Cleanup"));
}

#[tokio::test]
async fn team_call_develop_feature_returns_deterministic_dispatch() {
    let server = fixture_server();
    let args = json!({"team": "develop_feature"});
    let r1 = dispatch(&server, "team_call", &args).await;
    let r2 = dispatch(&server, "team_call", &args).await;
    assert_eq!(r1, r2);
    assert!(!r1.starts_with("Error:"));
    assert!(r1.contains("# Team: Develop Feature"));
}

#[tokio::test]
async fn develop_feature_now_routes_through_team_call() {
    // Restructured `develop_feature` should invoke team_call("develop_feature")
    // internally and produce the same dispatch script.
    let server = fixture_server();
    let team_args = json!({"team": "develop_feature", "goal": "fix bug", "project_dir": "/tmp"});
    let direct = dispatch(&server, "team_call", &team_args).await;
    let via_alias = dispatch(
        &server,
        "develop_feature",
        &json!({"goal": "fix bug", "project_dir": "/tmp"}),
    )
    .await;
    assert_eq!(
        direct, via_alias,
        "develop_feature must produce identical output to team_call(develop_feature):\n\
         direct:\n{direct}\n\nvia_alias:\n{via_alias}"
    );
}

#[tokio::test]
async fn team_list_enumerates_fixture_teams() {
    let server = fixture_server();
    let body = dispatch(&server, "team_list", &json!({})).await;
    assert!(body.contains("Cleanup"));
    assert!(body.contains("Develop Feature"));
}

#[tokio::test]
async fn workflow_list_enumerates_fixture_workforces() {
    let server = fixture_server();
    let body = dispatch(&server, "workflow_list", &json!({})).await;
    assert!(body.contains("Test Workforce"));
}

#[tokio::test]
async fn workflow_call_missing_workflow_returns_error() {
    let server = fixture_server();
    let body = dispatch(
        &server,
        "workflow_call",
        &json!({"workflow": "ghost_workflow"}),
    )
    .await;
    assert!(body.starts_with("Error:"));
    assert!(body.contains("ghost_workflow"));
}
