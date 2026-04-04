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
            error: Some(JsonRpcError { code, message }),
        }
    }
}

// ─── MCP protocol constants ────────────────────────────────────────────────

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "orrch-mcp-server";
const SERVER_VERSION: &str = "0.1.0";

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

// ─── Dispatch ───────────────────────────────────────────────────────────────

async fn dispatch(server: &OrrchMcpServer, req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    match req.method.as_str() {
        // ── Lifecycle ────────────────────────────────────────────────
        "initialize" => Some(handle_initialize(req.id.clone())),

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

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    let result = serde_json::json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION
        }
    });
    JsonRpcResponse::success(id, result)
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let tools_list = tools::tool_definitions();
    let result = serde_json::json!({ "tools": tools_list });
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
        "content": [
            { "type": "text", "text": result_text }
        ]
    });
    JsonRpcResponse::success(id, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_response() {
        let resp = handle_initialize(Some(Value::Number(1.into())));
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], SERVER_NAME);
    }

    #[test]
    fn test_tools_list_response() {
        let resp = handle_tools_list(Some(Value::Number(1.into())));
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 12);
        // Verify all tools have required fields.
        for tool in tools {
            assert!(tool.get("name").is_some());
            assert!(tool.get("description").is_some());
            assert!(tool.get("inputSchema").is_some());
        }
    }

    #[test]
    fn test_error_response() {
        let resp = JsonRpcResponse::error(Some(Value::Number(1.into())), -32601, "Not found".into());
        assert!(resp.error.is_some());
        assert!(resp.result.is_none());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
