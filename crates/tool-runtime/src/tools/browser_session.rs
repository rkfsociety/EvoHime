//! Persistent browser session tools over CDP (Stage 7.100, wave 1).
//!
//! Unlike `browser.open`/`browser.extract` (one-shot HTTP fetch), these tools
//! share one real browser tab per task: JS runs, page state survives between
//! calls, and the agent can look → act → look again.

use crate::{cdp, ssrf, ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub const NAVIGATE_NAME: &str = "browser.session.navigate";
pub const NAVIGATE_DESCRIPTION: &str =
    "Navigate the task's persistent browser tab to a URL (CDP session reuse)";
pub const NAVIGATE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const NAVIGATE_TIMEOUT: Duration = Duration::from_secs(30);

pub const READ_NAME: &str = "browser.session.read";
pub const READ_DESCRIPTION: &str =
    "Read the current page of the task's browser tab without re-navigating";
pub const READ_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub const CLICK_NAME: &str = "browser.session.click";
pub const CLICK_DESCRIPTION: &str =
    "Click a CSS selector in the task's browser tab and report the resulting page";
pub const CLICK_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const CLICK_TIMEOUT: Duration = Duration::from_secs(30);

pub const SCREENSHOT_NAME: &str = "browser.session.screenshot";
pub const SCREENSHOT_DESCRIPTION: &str =
    "Save a PNG screenshot of the task's browser tab into the workspace";
pub const SCREENSHOT_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(30);

pub const TYPE_NAME: &str = "browser.session.type";
pub const TYPE_DESCRIPTION: &str =
    "Type text into a CSS selector in the task's browser tab (input/change dispatched)";
pub const TYPE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const TYPE_TIMEOUT: Duration = Duration::from_secs(30);

pub const CLOSE_NAME: &str = "browser.session.close";
pub const CLOSE_DESCRIPTION: &str = "Close the task's persistent browser tab";
pub const CLOSE_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(15);

const DEFAULT_TEXT_LIMIT: usize = 4_000;
const MAX_TEXT_LIMIT: usize = 12_000;
const DEFAULT_LOAD_WAIT: Duration = Duration::from_secs(10);
const MAX_LOAD_WAIT_MS: u64 = 20_000;
const DEFAULT_SETTLE_MS: u64 = 500;
const MAX_SETTLE_MS: u64 = 5_000;
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_SCREENSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_TYPE_TEXT_CHARS: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
struct NavigateInput {
    url: String,
    #[serde(default)]
    max_chars: Option<usize>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ReadInput {
    #[serde(default)]
    max_chars: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ClickInput {
    selector: String,
    #[serde(default)]
    max_chars: Option<usize>,
    #[serde(default)]
    settle_ms: Option<u64>,
}

fn require_cdp(tool: &str) -> Result<String, ToolError> {
    cdp::cdp_base_url().ok_or_else(|| ToolError::InvalidInput {
        tool: tool.to_string(),
        message: format!(
            "browser session is not configured: set {} to a DevTools endpoint",
            cdp::CDP_URL_ENV
        ),
    })
}

fn parse_input<T: serde::de::DeserializeOwned>(tool: &str, value: Value) -> Result<T, ToolError> {
    serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
        tool: tool.to_string(),
        message: error.to_string(),
    })
}

fn text_limit(max_chars: Option<usize>) -> usize {
    max_chars.unwrap_or(DEFAULT_TEXT_LIMIT).clamp(1, MAX_TEXT_LIMIT)
}

fn cdp_failure(message: String) -> ToolError {
    ToolError::Execution(format!("browser session: {message}"))
}

fn snapshot_result(snapshot: Value, extra: Value) -> ToolResult {
    let url = snapshot.get("url").and_then(Value::as_str).unwrap_or("");
    let title = snapshot.get("title").and_then(Value::as_str).unwrap_or("");
    let text = snapshot.get("text").and_then(Value::as_str).unwrap_or("");
    let mut output = String::new();
    if !title.is_empty() {
        output.push_str("Title: ");
        output.push_str(title);
        output.push('\n');
    }
    output.push_str("URL: ");
    output.push_str(url);
    output.push_str("\n\n");
    output.push_str(text);

    let mut structured = json!({
        "url": url,
        "title": title,
        "text": text,
    });
    if let (Some(target), Some(source)) = (structured.as_object_mut(), extra.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    ToolResult { output, structured }
}

pub async fn navigate(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let base = require_cdp(NAVIGATE_NAME)?;
    let input: NavigateInput = parse_input(NAVIGATE_NAME, value)?;

    let url = Url::parse(&input.url).map_err(|error| ToolError::InvalidInput {
        tool: NAVIGATE_NAME.to_string(),
        message: error.to_string(),
    })?;
    ssrf::assert_safe_http_url(&url).map_err(|message| ToolError::InvalidInput {
        tool: NAVIGATE_NAME.to_string(),
        message,
    })?;

    let load_wait = input
        .timeout_ms
        .map(|ms| Duration::from_millis(ms.min(MAX_LOAD_WAIT_MS)))
        .unwrap_or(DEFAULT_LOAD_WAIT);
    let session = cdp::session_for_task(ctx.task_id, &base, OPEN_TIMEOUT)
        .await
        .map_err(cdp_failure)?;
    let mut session = session.lock().await;
    session.navigate(url.as_str(), load_wait).await.map_err(cdp_failure)?;
    let snapshot = session
        .page_snapshot(text_limit(input.max_chars))
        .await
        .map_err(cdp_failure)?;
    session.last_used = std::time::Instant::now();

    Ok(snapshot_result(snapshot, json!({ "requested_url": input.url })))
}

pub async fn read(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let base = require_cdp(READ_NAME)?;
    let input: ReadInput = parse_input(READ_NAME, value)?;

    let session = cdp::session_for_task(ctx.task_id, &base, OPEN_TIMEOUT)
        .await
        .map_err(cdp_failure)?;
    let mut session = session.lock().await;
    let snapshot = session
        .page_snapshot(text_limit(input.max_chars))
        .await
        .map_err(cdp_failure)?;
    session.last_used = std::time::Instant::now();

    Ok(snapshot_result(snapshot, json!({})))
}

pub async fn click(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let base = require_cdp(CLICK_NAME)?;
    let input: ClickInput = parse_input(CLICK_NAME, value)?;
    if input.selector.trim().is_empty() {
        return Err(ToolError::InvalidInput {
            tool: CLICK_NAME.to_string(),
            message: "selector must not be empty".into(),
        });
    }

    let session = cdp::session_for_task(ctx.task_id, &base, OPEN_TIMEOUT)
        .await
        .map_err(cdp_failure)?;
    let mut session = session.lock().await;
    let clicked = session.click(&input.selector).await.map_err(cdp_failure)?;
    if !clicked {
        return Err(ToolError::InvalidInput {
            tool: CLICK_NAME.to_string(),
            message: format!("selector matched no element: {}", input.selector),
        });
    }
    let settle = Duration::from_millis(
        input.settle_ms.unwrap_or(DEFAULT_SETTLE_MS).min(MAX_SETTLE_MS),
    );
    tokio::time::sleep(settle).await;
    let snapshot = session
        .page_snapshot(text_limit(input.max_chars))
        .await
        .map_err(cdp_failure)?;
    session.last_used = std::time::Instant::now();

    Ok(snapshot_result(
        snapshot,
        json!({ "selector": input.selector, "clicked": true }),
    ))
}

#[derive(Debug, Deserialize)]
struct ScreenshotInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    full_page: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TypeInput {
    selector: String,
    text: String,
    #[serde(default)]
    max_chars: Option<usize>,
    #[serde(default)]
    settle_ms: Option<u64>,
}

pub async fn screenshot(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let base = require_cdp(SCREENSHOT_NAME)?;
    let input: ScreenshotInput = parse_input(SCREENSHOT_NAME, value)?;
    let sandbox = ctx.sandbox()?;

    let relative = match input.path {
        Some(path) if !path.trim().is_empty() => {
            let path = path.trim().to_string();
            if path.to_ascii_lowercase().ends_with(".png") {
                path
            } else {
                format!("{path}.png")
            }
        }
        _ => format!(
            ".evohime/screenshots/{}-{}.png",
            ctx.task_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_millis())
                .unwrap_or(0)
        ),
    };
    let target = sandbox.resolve_for_write(&relative)?;

    let session = cdp::session_for_task(ctx.task_id, &base, OPEN_TIMEOUT)
        .await
        .map_err(cdp_failure)?;
    let mut session = session.lock().await;
    let encoded = session
        .capture_screenshot(input.full_page.unwrap_or(false))
        .await
        .map_err(cdp_failure)?;
    session.last_used = std::time::Instant::now();
    drop(session);

    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.as_bytes())
        .map_err(|error| cdp_failure(format!("screenshot decode failed: {error}")))?;
    if bytes.len() > MAX_SCREENSHOT_BYTES {
        return Err(ToolError::InvalidInput {
            tool: SCREENSHOT_NAME.to_string(),
            message: format!(
                "screenshot exceeds {} MiB limit",
                MAX_SCREENSHOT_BYTES / (1024 * 1024)
            ),
        });
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| ToolError::Execution(format!("screenshot dir failed: {error}")))?;
    }
    tokio::fs::write(&target, &bytes)
        .await
        .map_err(|error| ToolError::Execution(format!("screenshot write failed: {error}")))?;

    Ok(ToolResult {
        output: format!("Screenshot saved to {relative} ({} bytes)", bytes.len()),
        structured: json!({
            "path": relative,
            "bytes": bytes.len(),
            "full_page": input.full_page.unwrap_or(false),
        }),
    })
}

pub async fn type_text(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let base = require_cdp(TYPE_NAME)?;
    let input: TypeInput = parse_input(TYPE_NAME, value)?;
    if input.selector.trim().is_empty() {
        return Err(ToolError::InvalidInput {
            tool: TYPE_NAME.to_string(),
            message: "selector must not be empty".into(),
        });
    }
    if input.text.chars().count() > MAX_TYPE_TEXT_CHARS {
        return Err(ToolError::InvalidInput {
            tool: TYPE_NAME.to_string(),
            message: format!("text exceeds {MAX_TYPE_TEXT_CHARS} character limit"),
        });
    }

    let session = cdp::session_for_task(ctx.task_id, &base, OPEN_TIMEOUT)
        .await
        .map_err(cdp_failure)?;
    let mut session = session.lock().await;
    let typed = session
        .type_text(&input.selector, &input.text)
        .await
        .map_err(cdp_failure)?;
    if !typed {
        return Err(ToolError::InvalidInput {
            tool: TYPE_NAME.to_string(),
            message: format!("selector matched no element: {}", input.selector),
        });
    }
    let settle = Duration::from_millis(
        input.settle_ms.unwrap_or(DEFAULT_SETTLE_MS).min(MAX_SETTLE_MS),
    );
    tokio::time::sleep(settle).await;
    let snapshot = session
        .page_snapshot(text_limit(input.max_chars))
        .await
        .map_err(cdp_failure)?;
    session.last_used = std::time::Instant::now();

    // The typed text is deliberately absent from structured output and logs:
    // it may contain credentials the caller chose to enter.
    Ok(snapshot_result(
        snapshot,
        json!({
            "selector": input.selector,
            "typed": true,
            "text_length": input.text.chars().count(),
        }),
    ))
}

pub async fn close(ctx: &ToolContext, _value: Value) -> Result<ToolResult, ToolError> {
    require_cdp(CLOSE_NAME)?;
    let closed = match cdp::take_session(ctx.task_id).await {
        Some(session) => {
            if let Ok(session) = std::sync::Arc::try_unwrap(session) {
                session.into_inner().close().await;
            }
            true
        }
        None => false,
    };
    Ok(ToolResult {
        output: if closed {
            "Browser session closed".to_string()
        } else {
            "No active browser session for this task".to_string()
        },
        structured: json!({ "closed": closed }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::json;
    use tempfile::tempdir;
    use tokio_tungstenite::tungstenite::Message;
    use uuid::Uuid;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn ctx() -> (tempfile::TempDir, ToolContext) {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::new_v4(),
            session_id: None,
            progress_tx: None,
        };
        (dir, ctx)
    }

    /// Fake DevTools websocket endpoint: answers Page.enable/Page.navigate/
    /// Runtime.evaluate, emits loadEventFired, and counts clicks so a
    /// read-after-click observes changed state.
    async fn spawn_mock_ws() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut ws = tokio_tungstenite::accept_async(stream).await.expect("accept ws");
                    let mut clicks = 0u32;
                    let mut typed = 0u32;
                    let mut current_url = "about:blank".to_string();
                    while let Some(Ok(Message::Text(text))) = ws.next().await {
                        let request: serde_json::Value =
                            serde_json::from_str(&text).expect("request json");
                        let id = request["id"].as_u64().expect("id");
                        let method = request["method"].as_str().expect("method");
                        match method {
                            "Page.navigate" => {
                                current_url =
                                    request["params"]["url"].as_str().unwrap_or("").to_string();
                                let reply = json!({ "id": id, "result": { "frameId": "f" } });
                                ws.send(Message::Text(reply.to_string().into())).await.unwrap();
                                let event =
                                    json!({ "method": "Page.loadEventFired", "params": {} });
                                ws.send(Message::Text(event.to_string().into())).await.unwrap();
                            }
                            "Page.captureScreenshot" => {
                                use base64::Engine as _;
                                let data = base64::engine::general_purpose::STANDARD
                                    .encode(b"fake png bytes");
                                let reply = json!({ "id": id, "result": { "data": data } });
                                ws.send(Message::Text(reply.to_string().into())).await.unwrap();
                            }
                            "Runtime.evaluate" => {
                                let expression =
                                    request["params"]["expression"].as_str().unwrap_or("");
                                let value = if expression.contains("el.focus()") {
                                    if expression.contains("#missing") {
                                        json!(false)
                                    } else {
                                        typed += 1;
                                        json!(true)
                                    }
                                } else if expression.contains("querySelector") {
                                    if expression.contains("#missing") {
                                        json!(false)
                                    } else {
                                        clicks += 1;
                                        json!(true)
                                    }
                                } else {
                                    json!(serde_json::to_string(&json!({
                                        "url": current_url,
                                        "title": "Mock Page",
                                        "text": format!("clicks={clicks} typed={typed}"),
                                    }))
                                    .unwrap())
                                };
                                let reply = json!({
                                    "id": id,
                                    "result": { "result": { "value": value } },
                                });
                                ws.send(Message::Text(reply.to_string().into())).await.unwrap();
                            }
                            _ => {
                                let reply = json!({ "id": id, "result": {} });
                                ws.send(Message::Text(reply.to_string().into())).await.unwrap();
                            }
                        }
                    }
                });
            }
        });
        port
    }

    async fn mock_devtools(ws_port: u16) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/json/new"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "target-1",
                "webSocketDebuggerUrl":
                    format!("ws://127.0.0.1:{ws_port}/devtools/page/target-1"),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/json/close/target-1"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        server
    }

    #[test]
    fn navigate_click_read_share_one_session() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let _ssrf = crate::ssrf::lock_private_override(Some(true));
        runtime.block_on(async {
            let _env = ENV_LOCK.lock().await;
            let ws_port = spawn_mock_ws().await;
            let devtools = mock_devtools(ws_port).await;
            std::env::set_var(cdp::CDP_URL_ENV, devtools.uri());

            let (_dir, ctx) = ctx();
            let navigated = navigate(
                &ctx,
                json!({ "url": "https://example.com/start", "timeout_ms": 1000 }),
            )
            .await
            .expect("navigate");
            assert_eq!(navigated.structured["url"], "https://example.com/start");
            assert_eq!(navigated.structured["text"], "clicks=0 typed=0");

            let error = click(&ctx, json!({ "selector": "#missing", "settle_ms": 1 }))
                .await
                .expect_err("missing selector rejected");
            assert!(matches!(error, ToolError::InvalidInput { .. }));

            let clicked = click(&ctx, json!({ "selector": "#go", "settle_ms": 1 }))
                .await
                .expect("click");
            assert_eq!(clicked.structured["clicked"], true);
            assert_eq!(clicked.structured["text"], "clicks=1 typed=0");

            // Same tab, same state: read observes the click without navigation.
            let read_back = read(&ctx, json!({})).await.expect("read");
            assert_eq!(read_back.structured["text"], "clicks=1 typed=0");
            assert_eq!(read_back.structured["title"], "Mock Page");

            // Wave 2: typing changes page state and never echoes the text back.
            let typed = type_text(
                &ctx,
                json!({ "selector": "#name", "text": "secret value", "settle_ms": 1 }),
            )
            .await
            .expect("type");
            assert_eq!(typed.structured["typed"], true);
            assert_eq!(typed.structured["text_length"], 12);
            assert!(typed.structured.get("text").is_some());
            assert_eq!(typed.structured["text"], "clicks=1 typed=1");
            assert!(!serde_json::to_string(&typed.structured)
                .unwrap()
                .contains("secret value"));

            let type_error = type_text(
                &ctx,
                json!({ "selector": "#missing", "text": "x", "settle_ms": 1 }),
            )
            .await
            .expect_err("missing selector rejected");
            assert!(matches!(type_error, ToolError::InvalidInput { .. }));

            // Wave 2: screenshot lands inside the workspace sandbox.
            let shot = screenshot(&ctx, json!({ "path": "shots/page" }))
                .await
                .expect("screenshot");
            assert_eq!(shot.structured["path"], "shots/page.png");
            let saved = std::fs::read(ctx.workspace_root.join("shots/page.png"))
                .expect("screenshot file");
            assert_eq!(saved, b"fake png bytes");

            let closed = close(&ctx, json!({})).await.expect("close");
            assert_eq!(closed.structured["closed"], true);
            let closed_again = close(&ctx, json!({})).await.expect("close again");
            assert_eq!(closed_again.structured["closed"], false);

            std::env::remove_var(cdp::CDP_URL_ENV);
        });
    }

    #[tokio::test]
    async fn tools_require_cdp_configuration() {
        let _env = ENV_LOCK.lock().await;
        std::env::remove_var(cdp::CDP_URL_ENV);
        let (_dir, ctx) = ctx();
        let error = navigate(&ctx, json!({ "url": "https://example.com" }))
            .await
            .expect_err("unconfigured rejected");
        assert!(error.to_string().contains(cdp::CDP_URL_ENV));
    }

    #[tokio::test]
    async fn type_rejects_oversized_text() {
        let _env = ENV_LOCK.lock().await;
        std::env::set_var(cdp::CDP_URL_ENV, "http://127.0.0.1:9222");
        let (_dir, ctx) = ctx();
        let error = type_text(
            &ctx,
            json!({ "selector": "#name", "text": "x".repeat(MAX_TYPE_TEXT_CHARS + 1) }),
        )
        .await
        .expect_err("oversized text rejected");
        assert!(matches!(error, ToolError::InvalidInput { .. }));
        std::env::remove_var(cdp::CDP_URL_ENV);
    }

    #[tokio::test]
    async fn navigate_rejects_unsafe_urls() {
        let _env = ENV_LOCK.lock().await;
        std::env::set_var(cdp::CDP_URL_ENV, "http://127.0.0.1:9222");
        let _ssrf = crate::ssrf::lock_private_override(Some(false));
        let (_dir, ctx) = ctx();
        let error = navigate(&ctx, json!({ "url": "http://127.0.0.1:8080/admin" }))
            .await
            .expect_err("ssrf blocked");
        assert!(matches!(error, ToolError::InvalidInput { .. }));
        std::env::remove_var(cdp::CDP_URL_ENV);
    }
}
