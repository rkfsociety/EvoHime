use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub const NAME: &str = "mcp.call";
pub const DESCRIPTION: &str = "Call a remote MCP JSON-RPC endpoint";
pub const PERMISSIONS: &[Permission] = &[Permission::McpCall];
pub const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
struct Input {
    url: String,
    method: String,
    #[serde(default)]
    params: Value,
    timeout_ms: Option<u64>,
}

pub async fn execute(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let _ = ctx.sandbox()?;
    let input: Input = serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;

    let url = validate_url(&input.url)?;
    let timeout = Duration::from_millis(
        input
            .timeout_ms
            .unwrap_or(TIMEOUT.as_millis() as u64)
            .min(TIMEOUT.as_millis() as u64),
    );
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|error| ToolError::Execution(format!("client setup failed: {error}")))?;
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": input.method,
        "params": input.params,
    });

    let response = client
        .post(url.clone())
        .json(&payload)
        .send()
        .await
        .map_err(|error| ToolError::Execution(format!("mcp request failed: {error}")))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| ToolError::Execution(format!("failed to read response: {error}")))?;

    if !status.is_success() {
        return Err(ToolError::Execution(format!(
            "mcp endpoint returned {}: {}",
            status,
            truncate(&text)
        )));
    }

    let structured_response = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| {
        json!({
            "raw": text,
        })
    });

    Ok(ToolResult {
        output: render_output(&structured_response),
        structured: json!({
            "url": url.as_str(),
            "request": payload,
            "status_code": status.as_u16(),
            "response": structured_response,
        }),
    })
}

fn validate_url(value: &str) -> Result<Url, ToolError> {
    let url = Url::parse(value).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err(ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: "url must use http or https".to_string(),
        }),
    }
}

fn render_output(response: &Value) -> String {
    if let Some(error) = response.get("error") {
        return format!("mcp error: {}", truncate(&error.to_string()));
    }
    let rendered = serde_json::to_string_pretty(response).unwrap_or_else(|_| response.to_string());
    truncate(&rendered)
}

fn truncate(text: &str) -> String {
    const LIMIT: usize = 8 * 1024;
    text.chars().take(LIMIT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use wiremock::{
        matchers::{body_partial_json, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn posts_json_rpc_payload_and_returns_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rpc"))
            .and(body_partial_json(json!({
                "jsonrpc": "2.0",
                "method": "tools/list",
                "params": {"scope": "workspace"}
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": "123",
                "result": {
                    "tools": ["alpha", "beta"]
                }
            })))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
        };

        let result = execute(
            &ctx,
            json!({
                "url": format!("{}/rpc", server.uri()),
                "method": "tools/list",
                "params": { "scope": "workspace" }
            }),
        )
        .await
        .expect("mcp call succeeds");

        assert_eq!(result.structured["status_code"], 200);
        assert_eq!(result.structured["response"]["result"]["tools"][0], "alpha");
        assert!(result.output.contains("alpha"));
    }

    #[tokio::test]
    async fn rejects_non_http_urls() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
        };

        let error = execute(
            &ctx,
            json!({
                "url": "ftp://example.com/rpc",
                "method": "tools/list",
                "params": {}
            }),
        )
        .await
        .expect_err("url rejected");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }
}
