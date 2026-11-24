//! OpenAI Responses API provider.
//!
//! This is deliberately separate from the Chat Completions provider: Codex
//! models are exposed through Responses, and the wire format for function
//! calls is different.

use crate::config::{LiteRouterConfig, ModelRateLimitConfig, RequestPacingConfig};
use crate::providers::{
    ChatFuture, ChatMessage, ChatRequestOptions, ChatRole, ModelProvider, ProviderError,
    ProviderKind, TokenStream,
};
use crate::retry::{
    compute_backoff, is_retryable_status, read_bounded_provider_error_body, RetryPolicy,
};
use crate::tools::{ChatResult, ChatStreamItem, LlmUsage, NativeToolCall, ToolSpec};
use async_stream::stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Provider adapter for the OpenAI Responses API.
#[derive(Debug)]
pub struct OpenAIResponsesProvider {
    config: LiteRouterConfig,
    client: Client,
    retry: RetryPolicy,
    request_gate: Arc<Mutex<ResponsesRequestGate>>,
    pacing: RequestPacingConfig,
    model_rate_limits: HashMap<String, ModelRateLimitConfig>,
}

#[derive(Debug, Default)]
struct ResponsesRequestGate {
    last_request: Option<Instant>,
    model_requests: HashMap<String, VecDeque<Instant>>,
}

impl OpenAIResponsesProvider {
    /// Creates the adapter with retry defaults loaded from the environment.
    pub fn new(config: LiteRouterConfig) -> Result<Self, ProviderError> {
        Self::with_retry(config, RetryPolicy::from_env())
    }

    /// Creates the adapter with an explicit retry policy and no request pacing.
    pub fn with_retry(config: LiteRouterConfig, retry: RetryPolicy) -> Result<Self, ProviderError> {
        Self::with_pacing(config, retry, RequestPacingConfig::disabled())
    }

    /// Creates the adapter with explicit retry and request pacing.
    pub fn with_pacing(
        config: LiteRouterConfig,
        retry: RetryPolicy,
        pacing: RequestPacingConfig,
    ) -> Result<Self, ProviderError> {
        Self::with_pacing_and_model_limits(config, retry, pacing, HashMap::new())
    }

    /// Creates the adapter with optional per-model request limits.
    pub fn with_pacing_and_model_limits(
        config: LiteRouterConfig,
        retry: RetryPolicy,
        pacing: RequestPacingConfig,
        model_rate_limits: HashMap<String, ModelRateLimitConfig>,
    ) -> Result<Self, ProviderError> {
        if config.api_key.trim().is_empty() {
            return Err(ProviderError::Config(
                "OpenAI Responses API key must not be empty".into(),
            ));
        }
        let client = crate::client_builder_with_identity()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| ProviderError::Http(error.to_string()))?;
        Ok(Self {
            config,
            client,
            retry,
            request_gate: Arc::new(Mutex::new(ResponsesRequestGate::default())),
            pacing: pacing.normalized(),
            model_rate_limits,
        })
    }

    async fn wait_for_request_slot(&self, model: &str) {
        let mut previous = self.request_gate.lock().await;
        loop {
            let mut delay = self.pacing.interval_for_model(model).and_then(|interval| {
                previous.last_request.and_then(|started| {
                    let elapsed = started.elapsed();
                    (elapsed < interval).then_some(interval - elapsed)
                })
            });
            if let Some(limits) = self
                .model_rate_limits
                .get(model)
                .filter(|limits| limits.has_request_caps())
                .or_else(|| {
                    self.model_rate_limits
                        .get("*")
                        .filter(|limits| limits.has_request_caps())
                })
            {
                let now = Instant::now();
                let key = if self
                    .model_rate_limits
                    .get(model)
                    .is_some_and(|limits| limits.has_request_caps())
                {
                    model
                } else {
                    "*"
                };
                let timestamps = previous.model_requests.entry(key.to_owned()).or_default();
                while timestamps.front().is_some_and(|started| {
                    now.duration_since(*started) >= Duration::from_secs(86_400)
                }) {
                    timestamps.pop_front();
                }
                for (cap, window) in limits.request_windows() {
                    if cap == 0 {
                        continue;
                    }
                    let count = timestamps
                        .iter()
                        .filter(|started| now.duration_since(**started) < window)
                        .count();
                    if count >= cap as usize {
                        if let Some(wait) = timestamps
                            .iter()
                            .filter(|started| now.duration_since(**started) < window)
                            .nth(count - cap as usize)
                            .map(|started| window.saturating_sub(now.duration_since(*started)))
                        {
                            delay = Some(delay.map_or(wait, |current| current.max(wait)));
                        }
                    }
                }
            }
            if let Some(delay) = delay {
                drop(previous);
                tokio::time::sleep(delay).await;
                previous = self.request_gate.lock().await;
                continue;
            }
            let now = Instant::now();
            previous.last_request = Some(now);
            if self
                .model_rate_limits
                .get(model)
                .filter(|limits| limits.has_request_caps())
                .or_else(|| {
                    self.model_rate_limits
                        .get("*")
                        .filter(|limits| limits.has_request_caps())
                })
                .is_some()
            {
                previous
                    .model_requests
                    .entry(
                        if self
                            .model_rate_limits
                            .get(model)
                            .is_some_and(|limits| limits.has_request_caps())
                        {
                            model
                        } else {
                            "*"
                        }
                        .to_owned(),
                    )
                    .or_default()
                    .push_back(now);
            }
            return;
        }
    }

    fn responses_url(&self) -> String {
        format!("{}/responses", self.config.base_url.trim_end_matches('/'))
    }

    async fn request(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        stream: bool,
        options: ChatRequestOptions,
    ) -> Result<reqwest::Response, ProviderError> {
        let mut body = json!({
            "model": model,
            "input": messages.iter().map(response_input).collect::<Vec<_>>(),
            "tools": tools.map(|items| items.iter().map(response_tool).collect::<Vec<_>>()),
            "stream": stream,
            "store": false,
        });
        if let Some(max_output_tokens) = options.max_output_tokens {
            body["max_output_tokens"] = json!(max_output_tokens);
        }
        let max_retries = options.max_retries.unwrap_or(self.retry.max_retries);
        let mut attempt = 0;
        loop {
            self.wait_for_request_slot(model).await;
            match self
                .client
                .post(self.responses_url())
                .bearer_auth(&self.config.api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    if is_retryable_status(status) && attempt < max_retries {
                        tokio::time::sleep(compute_backoff(attempt, &self.retry, None)).await;
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    let body = read_bounded_provider_error_body(response).await;
                    let mut diagnostics = body.diagnostics;
                    diagnostics.provider_attempts = attempt.saturating_add(1);
                    return Err(ProviderError::api_with_diagnostics(
                        status.as_u16(),
                        &diagnostics,
                    ));
                }
                Err(error) if attempt < max_retries => {
                    tokio::time::sleep(compute_backoff(attempt, &self.retry, None)).await;
                    attempt = attempt.saturating_add(1);
                    let _ = error;
                }
                Err(error) => return Err(ProviderError::Http(error.to_string())),
            }
        }
    }
}

impl ModelProvider for OpenAIResponsesProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAIResponses
    }
    fn model_name(&self) -> &str {
        &self.config.model
    }
    fn base_url(&self) -> &str {
        &self.config.base_url
    }

    fn supports_structured_output(&self) -> bool {
        true
    }

    fn supports_tool_calls(&self) -> bool {
        true
    }

    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        self.stream_chat_with_model(&self.config.model, messages)
    }

    fn stream_chat_with_model(&self, model: &str, messages: &[ChatMessage]) -> TokenStream {
        let provider = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            retry: self.retry.clone(),
            model_rate_limits: self.model_rate_limits.clone(),
            request_gate: Arc::clone(&self.request_gate),
            pacing: self.pacing,
        };
        let model = model.to_string();
        let messages = messages.to_vec();
        Box::pin(stream! {
            let response = match provider
                .request(&model, &messages, None, true, ChatRequestOptions::default())
                .await
            {
                Ok(response) => response,
                Err(error) => { yield Err(error); return; }
            };
            let mut buffer = String::new();
            let mut bytes = response.bytes_stream();
            while let Some(chunk) = bytes.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => { yield Err(ProviderError::Stream(error.to_string())); return; }
                };
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(line) = take_sse_line(&mut buffer) {
                    if let Some(item) = parse_stream_event(&line) { yield item; }
                }
            }
            if !buffer.trim().is_empty() {
                if let Some(item) = parse_stream_event(buffer.trim()) { yield item; }
            }
        })
    }

    fn chat_with_tools(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> ChatFuture {
        let provider = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            retry: self.retry.clone(),
            model_rate_limits: self.model_rate_limits.clone(),
            request_gate: Arc::clone(&self.request_gate),
            pacing: self.pacing,
        };
        let model = model
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.config.model)
            .to_string();
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        Box::pin(async move {
            let response = provider
                .request(
                    &model,
                    &messages,
                    Some(&tools),
                    false,
                    ChatRequestOptions::default(),
                )
                .await?;
            let payload: Value = response
                .json()
                .await
                .map_err(|error| ProviderError::Api(error.to_string()))?;
            Ok(parse_result(&payload))
        })
    }

    fn chat_with_tools_with_options(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        options: ChatRequestOptions,
    ) -> ChatFuture {
        let provider = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            retry: self.retry.clone(),
            model_rate_limits: self.model_rate_limits.clone(),
            request_gate: Arc::clone(&self.request_gate),
            pacing: self.pacing,
        };
        let model = model
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.config.model)
            .to_string();
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        Box::pin(async move {
            let response = provider
                .request(&model, &messages, Some(&tools), false, options)
                .await?;
            let payload: Value = response
                .json()
                .await
                .map_err(|error| ProviderError::Api(error.to_string()))?;
            Ok(parse_result(&payload))
        })
    }
}

fn response_input(message: &ChatMessage) -> Value {
    match message.role {
        ChatRole::Tool => {
            json!({ "type": "function_call_output", "call_id": message.tool_call_id, "output": message.content })
        }
        _ => json!({ "role": message.role.as_str(), "content": message.content }),
    }
}

fn response_tool(tool: &ToolSpec) -> Value {
    // The declared name is already the canonical Core id, so it goes to the
    // provider unchanged and a returned call needs no reverse lookup.
    json!({ "type": "function", "name": tool.function.name, "description": tool.function.description, "parameters": tool.function.parameters, "strict": tool.function.strict.unwrap_or(false) })
}

fn parse_stream_event(line: &str) -> Option<Result<ChatStreamItem, ProviderError>> {
    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        return None;
    }
    let value: Value = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(error) => return Some(Err(ProviderError::Stream(error.to_string()))),
    };
    match value.get("type").and_then(Value::as_str) {
        Some("response.output_text.delta") => value
            .get("delta")
            .and_then(Value::as_str)
            .map(|text| Ok(ChatStreamItem::Delta(text.to_string()))),
        Some("response.reasoning_summary_text.delta") | Some("response.reasoning_text.delta") => {
            value
                .get("delta")
                .and_then(Value::as_str)
                .map(|text| Ok(ChatStreamItem::Thinking(text.to_string())))
        }
        Some("response.completed") => value
            .get("response")
            .map(|response| Ok(ChatStreamItem::Usage(parse_usage(response)))),
        Some("error") => Some(Err(ProviderError::Api(
            "Responses API returned an error event".into(),
        ))),
        _ => None,
    }
}

fn parse_result(payload: &Value) -> ChatResult {
    let mut result = ChatResult {
        usage: Some(parse_usage(payload)),
        ..ChatResult::default()
    };
    if result.usage.is_some_and(LlmUsage::is_empty) {
        result.usage = None;
    }
    for item in payload
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match item.get("type").and_then(Value::as_str) {
            Some("message") => {
                if let Some(content) = item.get("content").and_then(Value::as_array) {
                    for part in content {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            result.content.push_str(text);
                        }
                    }
                }
            }
            Some("function_call") => {
                let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
                if !name.is_empty() {
                    result.tool_calls.push(NativeToolCall {
                        id: item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(Value::as_str)
                            .unwrap_or("call")
                            .to_string(),
                        name: name.to_string(),
                        arguments: item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}")
                            .to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    result
}

fn parse_usage(value: &Value) -> LlmUsage {
    let usage = value.get("usage").unwrap_or(value);
    LlmUsage::from_parts(
        usage
            .get("input_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        usage
            .get("output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
    )
}

fn take_sse_line(buffer: &mut String) -> Option<String> {
    let end = buffer.find('\n')?;
    let line = buffer[..end].trim_end_matches('\r').to_string();
    buffer.drain(..=end);
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_responses_text_delta() {
        let item =
            parse_stream_event("data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}")
                .expect("event")
                .expect("ok");
        assert_eq!(item, ChatStreamItem::Delta("hi".into()));
    }

    #[test]
    fn response_tool_declaration_uses_provider_safe_name() {
        let tool = ToolSpec::function("filesystem_read", "Read a file", json!({"type": "object"}));
        let value = response_tool(&tool);
        assert_eq!(value["name"], "filesystem_read");
    }

    #[test]
    fn parses_function_call_output() {
        let value = json!({"output":[{"type":"function_call","call_id":"call_1","name":"git_status","arguments":"{}"}]});
        let result = parse_result(&value);
        assert_eq!(result.tool_calls[0].name, "git_status");
        assert_eq!(result.tool_calls[0].id, "call_1");
    }

    #[test]
    fn stream_error_event_does_not_expose_provider_message() {
        let result = parse_stream_event(
            r#"data: {"type":"error","message":"https://provider.test/?token=secret"}"#,
        );
        let error = result.expect("error event").expect_err("event must fail");
        assert!(!error.to_string().contains("provider.test"));
        assert!(!error.to_string().contains("secret"));
    }
}
