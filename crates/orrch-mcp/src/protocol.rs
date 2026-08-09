use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::server::OrrchMcpServer;
use crate::tools;

// ─── JSON-RPC types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JsonRpcRequest {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: None,
            }),
        }
    }

    fn error_with_data(id: Option<Value>, code: i64, message: String, data: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data: Some(data),
            }),
        }
    }
}

// ─── MCP protocol constants ────────────────────────────────────────────────

/// Modern (stateless, per-request `_meta`) protocol revision — spec 2026-07-28.
const MODERN_PROTOCOL_VERSION: &str = "2026-07-28";
/// Legacy (initialize-handshake) revisions this dual-era server still accepts,
/// newest first. Kept through the spec's twelve-month deprecation window so
/// legacy clients keep working.
const LEGACY_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const SERVER_NAME: &str = "orrch-mcp-server";
const SERVER_VERSION: &str = "0.2.0";
/// Freshness hint on cacheable list results. The tool list is static per
/// binary, so a long TTL is safe.
const LIST_TTL_MS: u64 = 3_600_000;
/// stdio server, single local client — never cacheable by shared intermediaries.
const CACHE_SCOPE: &str = "private";

/// MCP spec error code: UnsupportedProtocolVersionError (2026-07-28).
const UNSUPPORTED_PROTOCOL_VERSION: i64 = -32022;

const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

fn supported_versions() -> Vec<&'static str> {
    let mut v = vec![MODERN_PROTOCOL_VERSION];
    v.extend_from_slice(LEGACY_PROTOCOL_VERSIONS);
    v
}

fn server_info() -> Value {
    serde_json::json!({ "name": SERVER_NAME, "version": SERVER_VERSION })
}

/// `_meta` block attached to every result (spec: servers SHOULD identify
/// themselves in each result's `_meta`).
fn result_meta() -> Value {
    serde_json::json!({ META_SERVER_INFO: server_info() })
}

// ─── Main loop ──────────────────────────────────────────────────────────────

/// Read JSON-RPC requests from stdin, dispatch, write responses to stdout.
pub async fn run_stdio(server: OrrchMcpServer) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // EOF — client closed the pipe.
            eprintln!("stdin closed, shutting down");
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(None, -32700, format!("Parse error: {e}"));
                write_response(&mut stdout, &resp).await?;
                continue;
            }
        };

        // Validate jsonrpc version if present.
        if let Some(ref v) = req.jsonrpc {
            if v != "2.0" {
                let resp =
                    JsonRpcResponse::error(req.id, -32600, "Invalid jsonrpc version".into());
                write_response(&mut stdout, &resp).await?;
                continue;
            }
        }

        let response = dispatch(&server, &req).await;

        // Notifications (no id) get no response per JSON-RPC spec.
        if let Some(resp) = response {
            write_response(&mut stdout, &resp).await?;
        }
    }

    Ok(())
}

async fn write_response(
    stdout: &mut tokio::io::Stdout,
    resp: &JsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = serde_json::to_vec(resp)?;
    stdout.write_all(&bytes).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}

// ─── Version gate ───────────────────────────────────────────────────────────

/// If the request carries a modern per-request protocol version we don't
/// support, produce the mandated UnsupportedProtocolVersionError. Requests
/// without the `_meta` key are legacy-era and pass through untouched.
fn check_protocol_version(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let requested = req
        .params
        .get("_meta")
        .and_then(|m| m.get(META_PROTOCOL_VERSION))
        .and_then(|v| v.as_str())?;
    if supported_versions().contains(&requested) {
        return None;
    }
    Some(JsonRpcResponse::error_with_data(
        req.id.clone(),
        UNSUPPORTED_PROTOCOL_VERSION,
        "Unsupported protocol version".into(),
        serde_json::json!({
            "supported": supported_versions(),
            "requested": requested,
        }),
    ))
}

// ─── Dispatch ───────────────────────────────────────────────────────────────

async fn dispatch(server: &OrrchMcpServer, req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    if let Some(rejection) = check_protocol_version(req) {
        return Some(rejection);
    }

    match req.method.as_str() {
        // ── Modern discovery (2026-07-28, MUST implement) ────────────
        "server/discover" => Some(handle_discover(req.id.clone())),

        // ── Legacy lifecycle (deprecated era, kept for the 12-month
        //    window so initialize-handshake clients keep working) ─────
        "initialize" => Some(handle_initialize(req.id.clone(), &req.params)),

        // Notification — no response.
        "notifications/initialized" => None,

        // ── Tool discovery ───────────────────────────────────────────
        "tools/list" => Some(handle_tools_list(req.id.clone())),

        // ── Tool execution ───────────────────────────────────────────
        "tools/call" => Some(handle_tools_call(server, req.id.clone(), &req.params).await),

        // ── Unknown method ───────────────────────────────────────────
        other => {
            eprintln!("unknown method: {other}");
            Some(JsonRpcResponse::error(
                req.id.clone(),
                -32601,
                format!("Method not found: {other}"),
            ))
        }
    }
}

// ─── Handlers ───────────────────────────────────────────────────────────────

fn handle_discover(id: Option<Value>) -> JsonRpcResponse {
    let result = serde_json::json!({
        "resultType": "complete",
        "supportedVersions": supported_versions(),
        "capabilities": {
            "tools": {}
        },
        "instructions": "Orrchestrator development-pipeline tools: agents, skills, workflows, workforces, project state, and remote session management.",
        "ttlMs": LIST_TTL_MS,
        "cacheScope": CACHE_SCOPE,
        "_meta": result_meta()
    });
    JsonRpcResponse::success(id, result)
}

fn handle_initialize(id: Option<Value>, params: &Value) -> JsonRpcResponse {
    // Legacy negotiation: echo the client's requested revision when we support
    // it; otherwise answer with our newest legacy revision and let the client
    // decide (per pre-2026 lifecycle rules).
    let requested = params
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let negotiated = if LEGACY_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        LEGACY_PROTOCOL_VERSIONS[0]
    };
    let result = serde_json::json!({
        "protocolVersion": negotiated,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": server_info()
    });
    JsonRpcResponse::success(id, result)
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let tools_list = tools::tool_definitions();
    let result = serde_json::json!({
        "resultType": "complete",
        "tools": tools_list,
        "ttlMs": LIST_TTL_MS,
        "cacheScope": CACHE_SCOPE,
        "_meta": result_meta()
    });
    JsonRpcResponse::success(id, result)
}

async fn handle_tools_call(
    server: &OrrchMcpServer,
    id: Option<Value>,
    params: &Value,
) -> JsonRpcResponse {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    let result_text = tools::dispatch(server, name, &arguments).await;

    let result = serde_json::json!({
        "resultType": "complete",
        "content": [
            { "type": "text", "text": result_text }
        ],
        "_meta": result_meta()
    });
    JsonRpcResponse::success(id, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modern_request(method: &str, version: &str) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: Some("2.0".into()),
            id: Some(Value::Number(1.into())),
            method: method.into(),
            params: serde_json::json!({
                "_meta": { META_PROTOCOL_VERSION: version }
            }),
        }
    }

    #[test]
    fn test_discover_response() {
        let resp = handle_discover(Some(Value::Number(1.into())));
        let result = resp.result.unwrap();
        assert_eq!(result["resultType"], "complete");
        let versions = result["supportedVersions"].as_array().unwrap();
        assert_eq!(versions[0], MODERN_PROTOCOL_VERSION);
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["cacheScope"], CACHE_SCOPE);
        assert_eq!(result["_meta"][META_SERVER_INFO]["name"], SERVER_NAME);
    }

    #[test]
    fn test_initialize_echoes_supported_legacy_version() {
        let params = serde_json::json!({ "protocolVersion": "2025-06-18" });
        let resp = handle_initialize(Some(Value::Number(1.into())), &params);
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], "2025-06-18");
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
    }

    #[test]
    fn test_initialize_falls_back_to_newest_legacy_version() {
        let params = serde_json::json!({ "protocolVersion": "1900-01-01" });
        let resp = handle_initialize(Some(Value::Number(1.into())), &params);
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], LEGACY_PROTOCOL_VERSIONS[0]);
    }

    #[test]
    fn test_tools_list_response() {
        let resp = handle_tools_list(Some(Value::Number(1.into())));
        let result = resp.result.unwrap();
        assert_eq!(result["resultType"], "complete");
        assert_eq!(result["cacheScope"], CACHE_SCOPE);
        assert!(result["ttlMs"].as_u64().unwrap() > 0);
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 37);
        // Verify all tools have required fields.
        for tool in tools {
            assert!(tool.get("name").is_some());
            assert!(tool.get("description").is_some());
            assert!(tool.get("inputSchema").is_some());
        }
    }

    #[test]
    fn test_tools_list_deterministic_order() {
        // Spec: servers SHOULD return tools in deterministic order.
        let a = tools::tool_definitions();
        let b = tools::tool_definitions();
        let names = |v: &[Value]| {
            v.iter()
                .map(|t| t["name"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&a), names(&b));
    }

    #[test]
    fn test_unsupported_protocol_version_rejected() {
        let req = modern_request("tools/list", "1900-01-01");
        let resp = check_protocol_version(&req).expect("must reject");
        let err = resp.error.unwrap();
        assert_eq!(err.code, UNSUPPORTED_PROTOCOL_VERSION);
        let data = err.data.unwrap();
        assert_eq!(data["requested"], "1900-01-01");
        assert!(data["supported"]
            .as_array()
            .unwrap()
            .contains(&Value::String(MODERN_PROTOCOL_VERSION.into())));
    }

    #[test]
    fn test_supported_modern_version_passes() {
        let req = modern_request("tools/list", MODERN_PROTOCOL_VERSION);
        assert!(check_protocol_version(&req).is_none());
    }

    #[test]
    fn test_legacy_request_without_meta_passes() {
        let req = JsonRpcRequest {
            jsonrpc: Some("2.0".into()),
            id: Some(Value::Number(1.into())),
            method: "tools/list".into(),
            params: Value::Object(serde_json::Map::new()),
        };
        assert!(check_protocol_version(&req).is_none());
    }

    #[test]
    fn test_error_response() {
        let resp = JsonRpcResponse::error(Some(Value::Number(1.into())), -32601, "Not found".into());
        assert!(resp.error.is_some());
        assert!(resp.result.is_none());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
