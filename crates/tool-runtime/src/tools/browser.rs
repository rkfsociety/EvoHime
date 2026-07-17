use crate::{ssrf, ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use reqwest::{redirect::Policy, Client, Url};
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

pub const OPEN_NAME: &str = "browser.open";
pub const OPEN_DESCRIPTION: &str = "Open a web page and summarize its contents";
pub const OPEN_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

pub const EXTRACT_NAME: &str = "browser.extract";
pub const EXTRACT_DESCRIPTION: &str = "Extract data from a web page using a CSS selector";
pub const EXTRACT_PERMISSIONS: &[Permission] = &[Permission::BrowserAccess];
pub const EXTRACT_TIMEOUT: Duration = Duration::from_secs(20);

const DEFAULT_OPEN_LIMIT: usize = 4_000;
const MAX_OPEN_LIMIT: usize = 12_000;
const DEFAULT_EXTRACT_LIMIT: usize = 12;
const MAX_EXTRACT_LIMIT: usize = 100;

#[derive(Debug, Deserialize)]
struct OpenInput {
    url: String,
    #[serde(default)]
    max_chars: Option<usize>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ExtractInput {
    url: String,
    selector: String,
    #[serde(default)]
    attribute: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

pub async fn open(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let _ = ctx.sandbox()?;
    let input: OpenInput =
        serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
            tool: OPEN_NAME.to_string(),
            message: error.to_string(),
        })?;

    let source_url = input.url.clone();
    let url = validate_url(&source_url, OPEN_NAME)?;
    let timeout = timeout_from_input(input.timeout_ms, OPEN_TIMEOUT);
    let page = fetch_page(url, timeout).await?;
    let document = Html::parse_document(&page.body);

    let title = extract_first_text(&document, "title");
    let description = extract_meta_content(&document, "meta[name=\"description\"]");
    let text = extract_body_text(&document);
    let limit = input
        .max_chars
        .unwrap_or(DEFAULT_OPEN_LIMIT)
        .clamp(1, MAX_OPEN_LIMIT);
    let preview = truncate(&text, limit);
    let links = extract_links(&document, &page.final_url, 12);

    Ok(ToolResult {
        output: render_open_output(
            &page.final_url,
            title.as_deref(),
            description.as_deref(),
            &preview,
            &links,
        ),
        structured: json!({
            "url": source_url,
            "final_url": page.final_url.as_str(),
            "status_code": page.status_code,
            "title": title,
            "description": description,
            "text": preview,
            "links": links,
        }),
    })
}

pub async fn extract(ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let _ = ctx.sandbox()?;
    let input: ExtractInput =
        serde_json::from_value(value).map_err(|error| ToolError::InvalidInput {
            tool: EXTRACT_NAME.to_string(),
            message: error.to_string(),
        })?;

    let source_url = input.url.clone();
    let url = validate_url(&source_url, EXTRACT_NAME)?;
    let timeout = timeout_from_input(input.timeout_ms, EXTRACT_TIMEOUT);
    let page = fetch_page(url, timeout).await?;
    let document = Html::parse_document(&page.body);
    let selector = Selector::parse(&input.selector).map_err(|error| ToolError::InvalidInput {
        tool: EXTRACT_NAME.to_string(),
        message: format!("invalid selector: {error}"),
    })?;
    let limit = input
        .limit
        .unwrap_or(DEFAULT_EXTRACT_LIMIT)
        .clamp(1, MAX_EXTRACT_LIMIT);
    let items = document
        .select(&selector)
        .take(limit)
        .map(|element| {
            let text = collapse_whitespace(&element.text().collect::<Vec<_>>().join(" "));
            let attribute_value = input
                .attribute
                .as_deref()
                .and_then(|attribute| element.value().attr(attribute))
                .map(|value| value.to_string());

            match attribute_value {
                Some(attribute) => json!({
                    "text": text,
                    "attribute": attribute,
                }),
                None => json!({
                    "text": text,
                }),
            }
        })
        .collect::<Vec<_>>();

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string()),
        structured: json!({
            "url": source_url,
            "final_url": page.final_url.as_str(),
            "selector": input.selector,
            "count": items.len(),
            "items": items,
        }),
    })
}

struct Page {
    final_url: Url,
    status_code: u16,
    body: String,
}

async fn fetch_page(url: Url, timeout: Duration) -> Result<Page, ToolError> {
    let client = Client::builder()
        .timeout(timeout)
        .redirect(Policy::custom(|attempt| {
            if ssrf::assert_safe_http_url(attempt.url()).is_ok() {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("EvoHime/0.1 browser tool")
        .build()
        .map_err(|error| ToolError::Execution(format!("client setup failed: {error}")))?;

    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| ToolError::Execution(format!("browser request failed: {error}")))?;

    let status = response.status();
    let final_url = response.url().clone();
    ssrf::assert_safe_http_url(&final_url).map_err(|message| ToolError::InvalidInput {
        tool: "browser".to_string(),
        message: format!("ssrf blocked final url: {message}"),
    })?;
    let body = response
        .text()
        .await
        .map_err(|error| ToolError::Execution(format!("failed to read response: {error}")))?;

    if !status.is_success() {
        return Err(ToolError::Execution(format!(
            "browser endpoint returned {}: {}",
            status,
            truncate(&body, 2_000)
        )));
    }

    Ok(Page {
        final_url,
        status_code: status.as_u16(),
        body,
    })
}

fn validate_url(value: &str, tool: &str) -> Result<Url, ToolError> {
    let url = Url::parse(value).map_err(|error| ToolError::InvalidInput {
        tool: tool.to_string(),
        message: error.to_string(),
    })?;
    ssrf::assert_safe_http_url(&url).map_err(|message| ToolError::InvalidInput {
        tool: tool.to_string(),
        message,
    })?;
    Ok(url)
}

fn timeout_from_input(timeout_ms: Option<u64>, default_timeout: Duration) -> Duration {
    Duration::from_millis(
        timeout_ms
            .unwrap_or(default_timeout.as_millis() as u64)
            .min(default_timeout.as_millis() as u64),
    )
}

fn extract_first_text(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .next()
        .map(|element| collapse_whitespace(&element.text().collect::<Vec<_>>().join(" ")))
        .filter(|text| !text.is_empty())
}

fn extract_meta_content(document: &Html, selector: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|element| element.value().attr("content"))
        .map(collapse_whitespace)
        .filter(|text| !text.is_empty())
}

fn extract_body_text(document: &Html) -> String {
    let selector = Selector::parse("body").ok();
    let raw = selector
        .and_then(|selector| document.select(&selector).next())
        .map(|element| element.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_else(|| document.root_element().text().collect::<Vec<_>>().join(" "));
    collapse_whitespace(&raw)
}

fn extract_links(document: &Html, base_url: &Url, limit: usize) -> Vec<Value> {
    let selector = match Selector::parse("a[href]") {
        Ok(selector) => selector,
        Err(_) => return Vec::new(),
    };

    document
        .select(&selector)
        .filter_map(|element| {
            let href = element.value().attr("href")?;
            let text = collapse_whitespace(&element.text().collect::<Vec<_>>().join(" "));
            let resolved = base_url.join(href).unwrap_or_else(|_| base_url.clone());
            Some(json!({
                "text": text,
                "href": resolved.as_str(),
            }))
        })
        .take(limit)
        .collect()
}

fn render_open_output(
    final_url: &Url,
    title: Option<&str>,
    description: Option<&str>,
    text: &str,
    links: &[Value],
) -> String {
    let mut output = String::new();
    if let Some(title) = title {
        output.push_str("Title: ");
        output.push_str(title);
        output.push('\n');
    }
    output.push_str("URL: ");
    output.push_str(final_url.as_str());
    output.push_str("\n\n");
    if let Some(description) = description {
        output.push_str("Description: ");
        output.push_str(description);
        output.push_str("\n\n");
    }
    output.push_str(text);
    if !links.is_empty() {
        output.push_str("\n\nLinks:\n");
        for link in links {
            let text = link.get("text").and_then(Value::as_str).unwrap_or("");
            let href = link.get("href").and_then(Value::as_str).unwrap_or("");
            output.push_str("- ");
            if text.is_empty() {
                output.push_str(href);
            } else {
                output.push_str(text);
                output.push_str(" -> ");
                output.push_str(href);
            }
            output.push('\n');
        }
    }
    truncate(&output, 8_000)
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
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

    fn allow_private_targets_for_mock_server() -> crate::ssrf::PrivateOverrideGuard {
        crate::ssrf::lock_private_override(Some(true))
    }

    #[test]
    fn open_fetches_page_and_summarizes_content() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let _guard = allow_private_targets_for_mock_server();
        runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/page"))
                .respond_with(ResponseTemplate::new(200).set_body_string(
                    r#"
                    <html>
                      <head>
                        <title>Demo Page</title>
                        <meta name="description" content="A small sample page">
                      </head>
                      <body>
                        <main>
                          <h1>Hello browser</h1>
                          <p>This is a sample page for browser.open.</p>
                          <a href="/docs">Docs</a>
                        </main>
                      </body>
                    </html>
                "#,
                ))
                .mount(&server)
                .await;

            let (_dir, ctx) = ctx();
            let result = open(
                &ctx,
                json!({
                    "url": format!("{}/page", server.uri()),
                    "max_chars": 200
                }),
            )
            .await
            .expect("open succeeds");

            assert_eq!(result.structured["title"], "Demo Page");
            assert!(result.output.contains("Hello browser"));
            assert_eq!(
                result.structured["links"][0]["href"],
                format!("{}/docs", server.uri())
            );
        });
    }

    #[test]
    fn extract_selects_matching_nodes() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let _guard = allow_private_targets_for_mock_server();
        runtime.block_on(async {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/page"))
                .respond_with(ResponseTemplate::new(200).set_body_string(
                    r#"
                    <html>
                      <body>
                        <ul>
                          <li data-id="alpha">First</li>
                          <li data-id="beta">Second</li>
                        </ul>
                      </body>
                    </html>
                "#,
                ))
                .mount(&server)
                .await;

            let (_dir, ctx) = ctx();
            let result = extract(
                &ctx,
                json!({
                    "url": format!("{}/page", server.uri()),
                    "selector": "li",
                    "attribute": "data-id",
                    "limit": 1
                }),
            )
            .await
            .expect("extract succeeds");

            assert_eq!(result.structured["count"], 1);
            assert_eq!(result.structured["items"][0]["text"], "First");
            assert_eq!(result.structured["items"][0]["attribute"], "alpha");
        });
    }

    #[tokio::test]
    async fn rejects_non_http_urls() {
        let (_dir, ctx) = ctx();
        let error = open(
            &ctx,
            json!({
                "url": "ftp://example.com"
            }),
        )
        .await
        .expect_err("url rejected");

        assert!(matches!(error, ToolError::InvalidInput { .. }));
    }

    #[test]
    fn rejects_loopback_without_escape_hatch() {
        let _guard = crate::ssrf::lock_private_override(Some(false));
        let (_dir, ctx) = ctx();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let error = runtime
            .block_on(open(
                &ctx,
                json!({
                    "url": "http://127.0.0.1:9/"
                }),
            ))
            .expect_err("loopback blocked");
        assert!(matches!(error, ToolError::InvalidInput { .. }));
        let message = error.to_string();
        assert!(
            message.contains("blocked") || message.contains("127.0.0.1"),
            "unexpected message: {message}"
        );
    }
}
