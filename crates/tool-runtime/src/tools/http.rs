use crate::ssrf;
use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use reqwest::{redirect::Policy, Client, Url};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

pub const NAME: &str = "http.fetch";
pub const DESCRIPTION: &str = "Fetch an HTTP resource with SSRF protection";
pub const PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const TIMEOUT: Duration = Duration::from_secs(20);
const DEFAULT_MAX_CHARS: usize = 8_000;
const MAX_MAX_CHARS: usize = 20_000;

#[derive(Debug, Deserialize)]
struct Input {
    url: String,
    #[serde(default)]
    max_chars: Option<usize>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

pub async fn fetch(ctx: &ToolContext, value: serde_json::Value) -> Result<ToolResult, ToolError> {
    let _ = ctx.sandbox()?;
    let input: Input = serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;
    let source_url = input.url.clone();
    let url = validate_url(&source_url)?;
    let timeout = Duration::from_millis(
        input
            .timeout_ms
            .unwrap_or(TIMEOUT.as_millis() as u64)
            .clamp(1, TIMEOUT.as_millis() as u64),
    );
    let max_chars = input
        .max_chars
        .unwrap_or(DEFAULT_MAX_CHARS)
        .clamp(1, MAX_MAX_CHARS);
    let client = Client::builder()
        .timeout(timeout)
        .redirect(Policy::custom(|attempt| {
            if ssrf::assert_safe_http_url(attempt.url()).is_ok() {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("EvoHime/0.1 http.fetch")
        .build()
        .map_err(|error| ToolError::Execution(format!("client setup failed: {error}")))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| ToolError::Execution(format!("http request failed: {error}")))?;
    let status = response.status();
    let final_url = response.url().clone();
    ssrf::assert_safe_http_url(&final_url).map_err(|message| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: format!("ssrf blocked final url: {message}"),
    })?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .text()
        .await
        .map_err(|error| ToolError::Execution(format!("failed to read response: {error}")))?;
    if !status.is_success() {
        return Err(ToolError::Execution(format!(
            "http endpoint returned {}: {}",
            status,
            truncate(&body, 2_000)
        )));
    }
    let text = truncate(&body, max_chars);
    Ok(ToolResult {
        output: text.clone(),
        structured: json!({
            "url": source_url,
            "final_url": final_url.as_str(),
            "status_code": status.as_u16(),
            "content_type": content_type,
            "text": text,
            "truncated": body.chars().count() > max_chars,
        }),
    })
}

fn validate_url(value: &str) -> Result<Url, ToolError> {
    let url = Url::parse(value).map_err(|error| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: error.to_string(),
    })?;
    ssrf::assert_safe_http_url(&url).map_err(|message| ToolError::InvalidInput {
        tool: NAME.to_string(),
        message,
    })?;
    Ok(url)
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use uuid::Uuid;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn ctx() -> (tempfile::TempDir, ToolContext) {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        (dir, ctx)
    }

    #[tokio::test]
    async fn fetch_returns_bounded_text_and_metadata() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/data"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello world"))
            .mount(&server)
            .await;
        let (_dir, ctx) = ctx();
        let _private = crate::ssrf::lock_private_override(Some(true));

        let result = fetch(
            &ctx,
            json!({ "url": format!("{}/data", server.uri()), "max_chars": 5 }),
        )
        .await
        .expect("fetch succeeds");

        assert_eq!(result.output, "hello");
        assert_eq!(result.structured["status_code"], 200);
        assert_eq!(
            result.structured["final_url"],
            format!("{}/data", server.uri())
        );
    }

    #[tokio::test]
    async fn fetch_rejects_loopback_without_escape_hatch() {
        let (_dir, ctx) = ctx();
        let _private = crate::ssrf::lock_private_override(Some(false));
        let error = fetch(&ctx, json!({ "url": "http://127.0.0.1:9/" }))
            .await
            .expect_err("loopback blocked");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }
}
