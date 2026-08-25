//! OpenAI Responses API provider.
//!
//! This is deliberately separate from the Chat Completions provider: Codex
//! models are exposed through Responses, and the wire format for function
//! calls is different.

use crate::config::LiteRouterConfig;
use crate::providers::{
    ChatFuture, ChatMessage, ChatRole, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use crate::retry::{compute_backoff, is_retryable_status, RetryPolicy};
use crate::tools::{ChatResult, ChatStreamItem, LlmUsage, NativeToolCall, ToolSpec};
use async_stream::stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug)]
pub struct OpenAIResponsesProvider {
    config: LiteRouterConfig,
    client: Client,
    retry: RetryPolicy,
}

impl OpenAIResponsesProvider {
    pub fn new(config: LiteRouterConfig) -> Result<Self, ProviderError> {
        Self::with_retry(config, RetryPolicy::from_env())
    }

    pub fn with_retry(config: LiteRouterConfig, retry: RetryPolicy) -> Result<Self, ProviderError> {
        if config.api_key.trim().is_empty() {
            return Err(ProviderError::Config(
                "OpenAI Responses API key must not be empty".into(),
            ));
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| ProviderError::Http(error.to_string()))?;
        Ok(Self {
            config,
            client,
            retry,
        })
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
    ) -> Result<reqwest::Response, ProviderError> {
        let body = json!({
            "model": model,
            "input": messages.iter().map(response_input).collect::<Vec<_>>(),
            "tools": tools.map(|items| items.iter().map(response_tool).collect::<Vec<_>>()),
            "stream": stream,
            "store": false,
        });
        let mut attempt = 0;
        loop {
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
                    let text = response.text().await.unwrap_or_default();
                    if is_retryable_status(status) && attempt < self.retry.max_retries {
                        tokio::time::sleep(compute_backoff(attempt, &self.retry, None)).await;
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    return Err(ProviderError::Api(format!("{status}: {text}")));
                }
                Err(error) if attempt < self.retry.max_retries => {
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

    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        self.stream_chat_with_model(&self.config.model, messages)
    }

    fn stream_chat_with_model(&self, model: &str, messages: &[ChatMessage]) -> TokenStream {
        let provider = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            retry: self.retry.clone(),
        };
        let model = model.to_string();
        let messages = messages.to_vec();
        Box::pin(stream! {
            let response = match provider.request(&model, &messages, None, true).await {
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
        };
        let model = model
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.config.model)
            .to_string();
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        Box::pin(async move {
            let response = provider
                .request(&model, &messages, Some(&tools), false)
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
    json!({ "type": "function", "name": tool.function.name, "description": tool.function.description, "parameters": tool.function.parameters, "strict": false })
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
            value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Responses API error")
                .to_string(),
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
    fn parses_function_call_output() {
        let value = json!({"output":[{"type":"function_call","call_id":"call_1","name":"git_status","arguments":"{}"}]});
        let result = parse_result(&value);
        assert_eq!(result.tool_calls[0].name, "git_status");
        assert_eq!(result.tool_calls[0].id, "call_1");
    }
}
