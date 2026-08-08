//! Narrow, stateless HTTP MCP adapter.
//!
//! The adapter authenticates a trusted principal, validates a fixed tool
//! allow-list, and calls Business API through the typed client. It has no
//! database, object-storage, or business-table dependency.

mod config;

use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use business_api_client::{BusinessApiClient, ClientConfig, ClientError};
use serde_json::{json, Value};

#[derive(Clone)]
struct AppState {
    client: BusinessApiClient,
    adapter_token: String,
}

#[derive(Debug, serde::Deserialize)]
struct RpcRequest {
    jsonrpc: String,
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, serde::Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Debug, serde::Serialize)]
struct RpcError {
    code: i32,
    message: &'static str,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::AgentAdapterConfig::load()?;
    if config.env == runtime_config::RuntimeEnvironment::Production
        && config.business_api.bearer_token.trim().is_empty()
    {
        anyhow::bail!("business API bearer token must be configured")
    }
    if config.auth.bearer_token.trim().is_empty() {
        anyhow::bail!("MCP bearer token must be configured")
    }
    let _guard = observability::init_tracing(
        "agent-adapter",
        &config.observability.log_level,
        config.observability.otlp_endpoint.as_deref(),
    )?;
    let client = BusinessApiClient::new(ClientConfig::new(
        config.business_api.base_url,
        config.business_api.bearer_token,
    )?)?;
    let state = Arc::new(AppState {
        client,
        adapter_token: config.auth.bearer_token,
    });
    let app = Router::new()
        .route("/health/live", get(health))
        .route("/mcp", post(handle_rpc))
        .with_state(state);
    let listener =
        tokio::net::TcpListener::bind((config.server.host.as_str(), config.server.port)).await?;
    tracing::info!(port = config.server.port, "MCP adapter listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok","service":"agent-adapter"}))
}

async fn handle_rpc(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<RpcRequest>,
) -> impl IntoResponse {
    if !authorized(&headers, &state.adapter_token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":{"code":"unauthorized","message":"authentication required"}})),
        );
    }
    if request.jsonrpc != "2.0" {
        return rpc_http(rpc_error(request.id, -32600, "invalid request"));
    }
    let response = match request.method.as_str() {
        "initialize" => rpc_result(
            request.id,
            json!({
                "protocolVersion": "2026-07-28",
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "business-platform-mcp", "version": "0.1.0"}
            }),
        ),
        "notifications/initialized" => rpc_result(request.id, json!({})),
        "tools/list" => rpc_result(request.id, tools_list()),
        "tools/call" => call_tool(state.client.clone(), request.id, request.params).await,
        _ => rpc_error(request.id, -32601, "method not found"),
    };
    rpc_http(response)
}

fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected)
}

fn rpc_result(id: Value, result: Value) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}
fn rpc_error(id: Value, code: i32, message: &'static str) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError { code, message }),
    }
}
fn rpc_http(response: RpcResponse) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(serde_json::to_value(response).unwrap_or_else(
            |_| json!({"error":{"code":"internal","message":"MCP response failed"}}),
        )),
    )
}

fn tools_list() -> Value {
    json!({"tools": [
        tool("document.list", "List tenant documents", &json!({"type":"object","properties":{"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":50}}})),
        tool("document.get", "Get a tenant document", &json!({"type":"object","required":["id"],"properties":{"id":{"type":"string","format":"uuid"}}})),
        tool("document.processing.list", "List processing jobs for a document", &json!({"type":"object","required":["document_id"],"properties":{"document_id":{"type":"string","format":"uuid"}}})),
        tool("document.processing.get", "Get a processing job", &json!({"type":"object","required":["job_id"],"properties":{"job_id":{"type":"string","format":"uuid"}}})),
        tool("document.candidate.get", "Get a bounded extraction candidate", &json!({"type":"object","required":["job_id"],"properties":{"job_id":{"type":"string","format":"uuid"}}})),
        tool("operations.overview", "Get fixed operational overview", &json!({"type":"object","additionalProperties":false})),
        tool("governance.findings.list", "List integrity findings", &json!({"type":"object","additionalProperties":false})),
        tool("governance.finding.get", "Get an integrity finding", &json!({"type":"object","required":["id"],"properties":{"id":{"type":"string","format":"uuid"}}})),
        tool("audit.events.list", "List runtime audit events", &json!({"type":"object","properties":{"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":50}}})),
        tool("audit.event.get", "Get a runtime audit event", &json!({"type":"object","required":["id"],"properties":{"id":{"type":"string","format":"uuid"}}}))
    ]})
}

fn tool(name: &str, description: &str, input_schema: &Value) -> Value {
    json!({"name":name,"description":description,"inputSchema":input_schema})
}

async fn call_tool(client: BusinessApiClient, id: Value, params: Value) -> RpcResponse {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return rpc_error(id, -32602, "invalid tool arguments");
    };
    if !is_known_tool(name) {
        return rpc_error(id, -32601, "unknown tool");
    }
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !validate_tool_arguments(name, &arguments) {
        return rpc_error(id, -32602, "invalid tool arguments");
    }
    let result: Result<Value, ClientError> = match name {
        "document.list" => client
            .documents_list(
                arguments.get("cursor").and_then(Value::as_str),
                bounded_limit(arguments.get("limit")),
            )
            .await
            .map(|value| serde_json::to_value(value).unwrap_or_default()),
        "document.get" => match uuid_argument(&arguments, "id") {
            Ok(id) => client
                .document_get(id)
                .await
                .map(|value| serde_json::to_value(value).unwrap_or_default()),
            Err(()) => return rpc_error(id, -32602, "invalid tool arguments"),
        },
        "document.processing.list" => match uuid_argument(&arguments, "document_id") {
            Ok(id) => client
                .processing_for_document(id)
                .await
                .map(|value| serde_json::to_value(value).unwrap_or_default()),
            Err(()) => return rpc_error(id, -32602, "invalid tool arguments"),
        },
        "document.processing.get" => match uuid_argument(&arguments, "job_id") {
            Ok(id) => client
                .processing_get(id)
                .await
                .map(|value| serde_json::to_value(value).unwrap_or_default()),
            Err(()) => return rpc_error(id, -32602, "invalid tool arguments"),
        },
        "document.candidate.get" => match uuid_argument(&arguments, "job_id") {
            Ok(id) => client
                .candidate_get(id)
                .await
                .map(|value| serde_json::to_value(value).unwrap_or_default()),
            Err(()) => return rpc_error(id, -32602, "invalid tool arguments"),
        },
        "operations.overview" => client
            .operations_overview()
            .await
            .map(|value| serde_json::to_value(value).unwrap_or_default()),
        "governance.findings.list" => client
            .findings_list(50)
            .await
            .map(|value| serde_json::to_value(value).unwrap_or_default()),
        "governance.finding.get" => match uuid_argument(&arguments, "id") {
            Ok(id) => client
                .finding_get(id)
                .await
                .map(|value| serde_json::to_value(value).unwrap_or_default()),
            Err(()) => return rpc_error(id, -32602, "invalid tool arguments"),
        },
        "audit.events.list" => client
            .audit_list(
                arguments.get("cursor").and_then(Value::as_str),
                u16::try_from(bounded_limit(arguments.get("limit")))
                    .unwrap_or_else(|_| unreachable!()),
            )
            .await
            .map(|value| serde_json::to_value(value).unwrap_or_default()),
        "audit.event.get" => match uuid_argument(&arguments, "id") {
            Ok(id) => client
                .audit_get(id)
                .await
                .map(|value| serde_json::to_value(value).unwrap_or_default()),
            Err(()) => return rpc_error(id, -32602, "invalid tool arguments"),
        },
        _ => return rpc_error(id, -32601, "unknown tool"),
    };
    match result {
        Ok(value) => rpc_result(
            id,
            json!({
                "content": [{"type":"text", "text": serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())}],
                "structuredContent": value
            }),
        ),
        Err(error) => client_error_response(id, &error),
    }
}

fn is_upstream_unavailable(error: &ClientError) -> bool {
    matches!(
        error,
        ClientError::Api { status, .. }
            if matches!(
                *status,
                StatusCode::BAD_GATEWAY
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::GATEWAY_TIMEOUT
            )
    )
}

fn client_error_response(id: Value, error: &ClientError) -> RpcResponse {
    match error {
        ClientError::Api { .. } => rpc_error(
            id,
            -32003,
            if is_upstream_unavailable(error) {
                "upstream unavailable"
            } else {
                "business API request denied or failed"
            },
        ),
        ClientError::Transport(_)
        | ClientError::MalformedResponse
        | ClientError::InvalidConfiguration(_) => rpc_error(id, -32001, "upstream unavailable"),
    }
}
fn bounded_limit(value: Option<&Value>) -> u32 {
    value
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(20)
        .clamp(1, 50)
}

fn is_known_tool(name: &str) -> bool {
    matches!(
        name,
        "document.list"
            | "document.get"
            | "document.processing.list"
            | "document.processing.get"
            | "document.candidate.get"
            | "operations.overview"
            | "governance.findings.list"
            | "governance.finding.get"
            | "audit.events.list"
            | "audit.event.get"
    )
}

fn validate_tool_arguments(name: &str, arguments: &Value) -> bool {
    let Some(object) = arguments.as_object() else {
        return false;
    };
    let allowed = match name {
        "document.list" | "audit.events.list" => &["cursor", "limit"][..],
        "document.get" | "governance.finding.get" | "audit.event.get" => &["id"][..],
        "document.processing.list" => &["document_id"][..],
        "document.processing.get" | "document.candidate.get" => &["job_id"][..],
        "operations.overview" | "governance.findings.list" => &[][..],
        _ => return false,
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return false;
    }
    match name {
        "document.list" | "audit.events.list" => {
            let cursor_valid = object.get("cursor").is_none_or(Value::is_string);
            let limit_valid = object.get("limit").is_none_or(|value| {
                value
                    .as_u64()
                    .is_some_and(|limit| (1..=50).contains(&limit))
            });
            cursor_valid && limit_valid
        }
        "document.get" | "governance.finding.get" | "audit.event.get" => object
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok()),
        "document.processing.list" => object
            .get("document_id")
            .and_then(Value::as_str)
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok()),
        "document.processing.get" | "document.candidate.get" => object
            .get("job_id")
            .and_then(Value::as_str)
            .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok()),
        "operations.overview" | "governance.findings.list" => object.is_empty(),
        _ => false,
    }
}

fn uuid_argument(arguments: &Value, name: &str) -> Result<uuid::Uuid, ()> {
    let value = arguments.get(name).and_then(Value::as_str).ok_or(())?;
    uuid::Uuid::parse_str(value).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use business_api_client::{BusinessApiClient, ClientConfig};

    #[test]
    fn tools_list_is_the_read_only_allow_list() {
        let tools = tools_list()["tools"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let names = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 10);
        assert!(names.iter().all(|name| is_known_tool(name)));
        assert!(!names.iter().any(|name| name.ends_with("_sql")));
    }

    #[test]
    fn malformed_arguments_and_forged_tenant_inputs_fail_closed() {
        assert!(!validate_tool_arguments(
            "operations.overview",
            &json!({"tenant_id": "x"})
        ));
        assert!(!validate_tool_arguments(
            "document.get",
            &json!({"id": "not-a-uuid"})
        ));
        assert!(!validate_tool_arguments(
            "document.list",
            &json!({"limit": 0})
        ));
        assert!(!validate_tool_arguments(
            "document.list",
            &json!({"tenant_id": "other"})
        ));
        assert!(!validate_tool_arguments("document.list", &json!(null)));
    }

    #[test]
    fn missing_or_wrong_bearer_token_is_rejected() {
        let empty = HeaderMap::new();
        assert!(!authorized(&empty, "mcp-demo-token"));
        let mut forged = HeaderMap::new();
        forged.insert(
            header::AUTHORIZATION,
            "Bearer other".parse().unwrap_or_else(|_| unreachable!()),
        );
        assert!(!authorized(&forged, "mcp-demo-token"));
    }

    #[tokio::test]
    async fn upstream_failure_is_reported_without_fake_business_data() {
        let client = BusinessApiClient::new(
            ClientConfig::new("http://127.0.0.1:9", "business-token")
                .unwrap_or_else(|_| unreachable!()),
        )
        .unwrap_or_else(|_| unreachable!());
        let response = call_tool(
            client,
            json!("1"),
            json!({"name":"operations.overview","arguments":{}}),
        )
        .await;
        assert_eq!(response.error.map(|error| error.code), Some(-32001));
        assert!(response.result.is_none());
    }
}

#[test]
fn api_5xx_is_reported_as_upstream_unavailable() {
    let error = ClientError::Api {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "upstream_error".to_string(),
        message: "unavailable".to_string(),
        trace_id: None,
    };
    assert!(is_upstream_unavailable(&error));
    assert!(!is_upstream_unavailable(&ClientError::Api {
        status: StatusCode::FORBIDDEN,
        code: "forbidden".to_string(),
        message: "denied".to_string(),
        trace_id: None,
    }));
}
