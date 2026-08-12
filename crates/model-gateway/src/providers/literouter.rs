use crate::config::LiteRouterConfig;
use crate::providers::{
    ChatFuture, ChatMessage, ModelProvider, ProviderError, ProviderKind, ThinkingConfig,
    TokenStream,
};
use crate::retry::{
    classify_rate_limit, compute_backoff, is_retryable_status, parse_retry_after_seconds,
    RateLimitClass, RetryPolicy,
};
use crate::tools::{ChatResult, ChatStreamItem, LlmUsage, NativeToolCall, ToolSpec};
use async_stream::stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// LiteRouter — OpenAI-compatible provider.
///
/// API docs: `docs/providers/literouter.md`
#[derive(Debug)]
pub struct LiteRouterProvider {
    config: LiteRouterConfig,
    client: Client,
    retry: RetryPolicy,
    request_gate: Arc<Mutex<Option<Instant>>>,
}

impl LiteRouterProvider {
    const PAID_REQUEST_INTERVAL: Duration = Duration::from_secs(5);
    const FREE_REQUEST_INTERVAL: Duration = Duration::from_millis(18_948);

    pub fn new(config: LiteRouterConfig) -> Result<Self, ProviderError> {
        Self::with_retry(config, RetryPolicy::from_env())
    }

    pub fn with_retry(config: LiteRouterConfig, retry: RetryPolicy) -> Result<Self, ProviderError> {
        if config.api_key.is_empty() {
            return Err(ProviderError::Config(
                "LiteRouter API key must not be empty".to_string(),
            ));
        }

        let client = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| ProviderError::Http(error.to_string()))?;

        Ok(Self {
            config,
            client,
            retry,
            request_gate: Arc::new(Mutex::new(None)),
        })
    }

    pub fn config(&self) -> &LiteRouterConfig {
        &self.config
    }

    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry
    }

    pub fn chat_completions_url(&self) -> String {
        self.config.chat_completions_url()
    }

    async fn send_chat_request(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        stream: bool,
    ) -> Result<reqwest::Response, ProviderError> {
        self.send_chat_request_internal(model, messages, tools, stream, None)
            .await
    }

    async fn send_chat_request_with_thinking(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        thinking: Option<ThinkingConfig>,
    ) -> Result<reqwest::Response, ProviderError> {
        self.send_chat_request_internal(model, messages, tools, true, thinking)
            .await
    }

    async fn send_chat_request_internal(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: Option<&[ToolSpec]>,
        stream: bool,
        thinking: Option<ThinkingConfig>,
    ) -> Result<reqwest::Response, ProviderError> {
        let tools = tools.map(|specs| {
            specs
                .iter()
                .cloned()
                .map(|mut spec| {
                    // LiteRouter may translate the OpenAI-compatible request to
                    // Gemini, whose function-declaration schema rejects this
                    // otherwise common JSON-Schema keyword.
                    strip_gemini_unsupported_schema_fields(&mut spec.function.parameters);
                    spec
                })
                .collect::<Vec<ToolSpec>>()
        });
        let has_tools = tools.as_ref().is_some_and(|specs| !specs.is_empty());
        let body = ChatCompletionRequest {
            model: model.to_string(),
            messages: messages.iter().map(ApiMessage::from_chat_message).collect(),
            stream,
            stream_options: stream.then_some(StreamOptions {
                include_usage: true,
            }),
            tools,
            tool_choice: if has_tools {
                Some(Value::String("auto".into()))
            } else {
                None
            },
            thinking,
        };

        let mut attempt: u32 = 0;
        loop {
            // Count every provider attempt, including retries, against the
            // free-tier budget and spread requests across the hour.
            self.wait_for_request_slot(model).await;
            let send_result = self
                .client
                .post(self.config.chat_completions_url())
                .bearer_auth(&self.config.api_key)
                .json(&body)
                .send()
                .await;

            match send_result {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let retry_after = parse_retry_after_seconds(response.headers());
                    let text = response.text().await.unwrap_or_default();
                    let rate_limit = classify_rate_limit(status, &text);
                    if matches!(rate_limit, Some(RateLimitClass::Exhausted)) {
                        return Err(ProviderError::Api(format!(
                            "{status}: provider quota exhausted: {text}"
                        )));
                    }
                    let rate_limit_retryable = matches!(
                        rate_limit,
                        Some(RateLimitClass::Transient | RateLimitClass::Unknown)
                    );
                    let rate_limit_cap = matches!(rate_limit, Some(RateLimitClass::Unknown))
                        .then_some(2)
                        .unwrap_or(self.retry.max_retries);
                    if (is_retryable_status(status) || rate_limit_retryable)
                        && attempt < rate_limit_cap
                    {
                        let delay = compute_backoff(attempt, &self.retry, retry_after);
                        tokio::time::sleep(delay).await;
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    return Err(ProviderError::Api(format!("{status}: {text}")));
                }
                Err(error) => {
                    if attempt < self.retry.max_retries {
                        let delay = compute_backoff(attempt, &self.retry, None);
                        tokio::time::sleep(delay).await;
                        attempt = attempt.saturating_add(1);
                        continue;
                    }
                    return Err(ProviderError::Http(error.to_string()));
                }
            }
        }
    }

    async fn wait_for_request_slot(&self, model: &str) {
        let interval = request_interval_for_model(model);
        let mut last_request = self.request_gate.lock().await;
        if let Some(previous) = *last_request {
            let elapsed = previous.elapsed();
            if elapsed < interval {
                tokio::time::sleep(interval - elapsed).await;
            }
        }
        *last_request = Some(Instant::now());
    }
}

fn request_interval_for_model(model: &str) -> Duration {
    if model.trim().to_ascii_lowercase().ends_with(":free") {
        LiteRouterProvider::FREE_REQUEST_INTERVAL
    } else {
        LiteRouterProvider::PAID_REQUEST_INTERVAL
    }
}

fn strip_gemini_unsupported_schema_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("additionalProperties");
            for child in object.values_mut() {
                strip_gemini_unsupported_schema_fields(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_gemini_unsupported_schema_fields(item);
            }
        }
        _ => {}
    }
}

impl ModelProvider for LiteRouterProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::LiteRouter
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
            request_gate: Arc::clone(&self.request_gate),
        };
        let request_messages = messages.to_vec();
        let model = model.to_string();

        Box::pin(stream! {
            let response = match provider
                .send_chat_request(&model, &request_messages, None, true)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    yield Err(error);
                    return;
                }
            };

            let mut buffer = String::new();
            let mut byte_stream = response.bytes_stream();

            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Err(ProviderError::Stream(error.to_string()));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line) = take_sse_line(&mut buffer) {
                    if let Some(result) = parse_sse_line(&line) {
                        yield result;
                    }
                }
            }

            if !buffer.trim().is_empty() {
                if let Some(result) = parse_sse_line(buffer.trim()) {
                    yield result;
                }
            }
        })
    }

    fn stream_with_thinking(
        &self,
        messages: &[ChatMessage],
        thinking: Option<ThinkingConfig>,
        tools: Option<&[ToolSpec]>,
    ) -> TokenStream {
        let provider = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            retry: self.retry.clone(),
            request_gate: Arc::clone(&self.request_gate),
        };
        let request_messages = messages.to_vec();
        let tools = tools.map(|t| t.to_vec());

        Box::pin(stream! {
            let response = match provider
                .send_chat_request_with_thinking(&provider.config.model, &request_messages, tools.as_deref(), thinking)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    yield Err(error);
                    return;
                }
            };

            let mut buffer = String::new();
            let mut byte_stream = response.bytes_stream();

            while let Some(chunk) = byte_stream.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => {
                        yield Err(ProviderError::Stream(error.to_string()));
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line) = take_sse_line(&mut buffer) {
                    if let Some(result) = parse_sse_line(&line) {
                        yield result;
                    }
                }
            }

            if !buffer.trim().is_empty() {
                if let Some(result) = parse_sse_line(buffer.trim()) {
                    yield result;
                }
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
            request_gate: Arc::clone(&self.request_gate),
        };
        let model = model
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.config.model)
            .to_string();
        let request_messages = messages.to_vec();
        let tools = tools.to_vec();

        Box::pin(async move {
            let mut attempt = 0;
            loop {
                let response = provider
                    .send_chat_request(&model, &request_messages, Some(&tools), false)
                    .await?;
                match response.json::<CompletionResponse>().await {
                    Ok(payload) => return Ok(payload.into_chat_result()),
                    Err(error) if attempt < provider.retry.max_retries => {
                        let delay = compute_backoff(attempt, &provider.retry, None);
                        tokio::time::sleep(delay).await;
                        attempt = attempt.saturating_add(1);
                        let _ = error;
                    }
                    Err(error) => return Err(ProviderError::Api(error.to_string())),
                }
            }
        })
    }
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ApiMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<crate::providers::ThinkingConfig>,
}

#[derive(Debug, serde::Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, serde::Serialize)]
struct ApiMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ApiRequestToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct ApiRequestToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ApiRequestFunction,
}

#[derive(Debug, serde::Serialize)]
struct ApiRequestFunction {
    name: String,
    arguments: String,
}

impl ApiMessage {
    fn from_chat_message(message: &ChatMessage) -> Self {
        Self {
            role: message.role.as_str().to_string(),
            content: message.content.clone(),
            tool_calls: message
                .tool_calls
                .iter()
                .map(|call| ApiRequestToolCall {
                    id: call.id.clone(),
                    kind: "function",
                    function: ApiRequestFunction {
                        name: call.name.replace('.', "_"),
                        arguments: call.arguments.clone(),
                    },
                })
                .collect(),
            tool_call_id: message.tool_call_id.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
}

#[derive(Debug, Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompletionResponse {
    #[serde(default)]
    choices: Vec<CompletionChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct CompletionChoice {
    #[serde(default)]
    message: CompletionMessage,
}

#[derive(Debug, Deserialize, Default)]
struct CompletionMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ApiToolCall>,
}

#[derive(Debug, Deserialize)]
struct ApiToolCall {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ApiFunctionCall>,
}

#[derive(Debug, Deserialize)]
struct ApiFunctionCall {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

impl ApiUsage {
    fn into_llm_usage(self) -> LlmUsage {
        let mut usage = LlmUsage {
            prompt_tokens: self.prompt_tokens,
            completion_tokens: self.completion_tokens,
            total_tokens: self.total_tokens,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            thinking_tokens: None,
        };
        if usage.total_tokens == 0 {
            usage.total_tokens = usage.prompt_tokens.saturating_add(usage.completion_tokens);
        }
        usage
    }
}

impl CompletionResponse {
    fn into_chat_result(self) -> ChatResult {
        let usage = self
            .usage
            .map(ApiUsage::into_llm_usage)
            .filter(|u| !u.is_empty());
        let message = self
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .unwrap_or_default();
        let tool_calls = message
            .tool_calls
            .into_iter()
            .enumerate()
            .filter_map(|(index, call)| {
                let function = call.function?;
                let name = function.name?.trim().to_string();
                if name.is_empty() {
                    return None;
                }
                Some(NativeToolCall {
                    id: call
                        .id
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| format!("call_{index}")),
                    name,
                    arguments: function.arguments.unwrap_or_else(|| "{}".into()),
                })
            })
            .collect();
        ChatResult {
            content: message.content.unwrap_or_default(),
            thinking: message.thinking,
            tool_calls,
            usage,
        }
    }
}

fn take_sse_line(buffer: &mut String) -> Option<String> {
    if let Some(index) = buffer.find('\n') {
        let line = buffer.drain(..=index).collect::<String>();
        return Some(line.trim_end_matches(['\r', '\n']).to_string());
    }
    None
}

fn parse_sse_line(line: &str) -> Option<Result<ChatStreamItem, ProviderError>> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        return None;
    }

    let chunk: StreamChunk = match serde_json::from_str(data) {
        Ok(chunk) => chunk,
        Err(error) => return Some(Err(ProviderError::Stream(error.to_string()))),
    };

    if let Some(usage) = chunk.usage {
        let usage = usage.into_llm_usage();
        if !usage.is_empty() {
            return Some(Ok(ChatStreamItem::Usage(usage)));
        }
    }

    if let Some(choice) = chunk.choices.first() {
        // Handle thinking chunks (Wave 3B: extended reasoning)
        if let Some(thinking) = choice.delta.thinking.clone() {
            if !thinking.is_empty() {
                return Some(Ok(ChatStreamItem::Thinking(thinking)));
            }
        }

        // Handle content chunks
        if let Some(content) = choice.delta.content.clone() {
            if !content.is_empty() {
                return Some(Ok(ChatStreamItem::Delta(content)));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatMessage;
    use crate::tools::{NativeToolCall, ToolSpec};
    use serde_json::json;

    #[test]
    fn creates_provider_with_config() {
        let provider = LiteRouterProvider::new(LiteRouterConfig {
            api_key: "lr_test".to_string(),
            base_url: "https://api.literouter.com/v1".to_string(),
            model: "deepseek:free".to_string(),
        })
        .expect("provider created");

        assert_eq!(provider.kind(), ProviderKind::LiteRouter);
        assert_eq!(provider.model_name(), "deepseek:free");
        assert_eq!(
            provider.chat_completions_url(),
            "https://api.literouter.com/v1/chat/completions"
        );
    }

    #[test]
    fn rejects_empty_api_key() {
        let error = LiteRouterProvider::new(LiteRouterConfig {
            api_key: String::new(),
            base_url: "https://api.literouter.com/v1".to_string(),
            model: "deepseek:free".to_string(),
        })
        .expect_err("empty key rejected");

        assert!(error.to_string().contains("API key"));
    }

    #[test]
    fn serializes_assistant_tool_call_and_tool_observation_messages() {
        let call = NativeToolCall {
            id: "call-1".into(),
            name: "filesystem_read".into(),
            arguments: r#"{"path":"src/lib.rs"}"#.into(),
        };
        let assistant = ChatMessage::assistant_tool_calls("", vec![call.clone()]);
        let observation = ChatMessage::tool_observation("call-1", r#"{"ok":true}"#);

        let assistant_payload = ApiMessage::from_chat_message(&assistant);
        let observation_payload = ApiMessage::from_chat_message(&observation);

        assert_eq!(assistant_payload.role, "assistant");
        assert_eq!(assistant_payload.tool_calls[0].id, "call-1");
        assert_eq!(
            assistant_payload.tool_calls[0].function.name,
            "filesystem_read"
        );
        assert_eq!(
            assistant_payload.tool_calls[0].function.arguments,
            call.arguments
        );
        assert_eq!(observation_payload.role, "tool");
        assert_eq!(observation_payload.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(observation_payload.content, r#"{"ok":true}"#);
    }

    #[test]
    fn parses_sse_delta() {
        let result = parse_sse_line(r#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#)
            .expect("parsed")
            .expect("ok");

        assert_eq!(result, ChatStreamItem::Delta("Hi".into()));
    }

    #[test]
    fn parses_sse_usage_chunk() {
        let result = parse_sse_line(
            r#"data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":34,"total_tokens":46}}"#,
        )
        .expect("parsed")
        .expect("ok");

        assert_eq!(
            result,
            ChatStreamItem::Usage(LlmUsage {
                prompt_tokens: 12,
                completion_tokens: 34,
                total_tokens: 46,
                ..Default::default()
            })
        );
    }

    #[test]
    fn ignores_done_marker() {
        assert!(parse_sse_line("data: [DONE]").is_none());
    }

    #[test]
    fn completion_response_extracts_tool_calls_and_usage() {
        let payload = CompletionResponse {
            choices: vec![CompletionChoice {
                message: CompletionMessage {
                    content: Some(String::new()),
                    thinking: None,
                    tool_calls: vec![ApiToolCall {
                        id: Some("call_1".into()),
                        function: Some(ApiFunctionCall {
                            name: Some("filesystem.read".into()),
                            arguments: Some(r#"{"path":"README.md"}"#.into()),
                        }),
                    }],
                },
            }],
            usage: Some(ApiUsage {
                prompt_tokens: 100,
                completion_tokens: 20,
                total_tokens: 120,
            }),
        };
        let result = payload.into_chat_result();
        assert!(result.content.is_empty());
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "filesystem.read");
        assert!(result.tool_calls[0].arguments.contains("README.md"));
        assert_eq!(
            result.usage,
            Some(LlmUsage {
                prompt_tokens: 100,
                completion_tokens: 20,
                total_tokens: 120,
                ..Default::default()
            })
        );
    }

    #[test]
    fn tool_spec_serializes_openai_shape() {
        let spec = ToolSpec::function(
            "filesystem.read",
            "Read a file",
            json!({"type":"object","properties":{"path":{"type":"string"}}}),
        );
        let value = serde_json::to_value(&spec).expect("serialize");
        assert_eq!(value["type"], "function");
        assert_eq!(value["function"]["name"], "filesystem.read");
    }

    #[test]
    fn strips_additional_properties_only_for_gemini_tools() {
        let mut schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "nested": {
                    "type": "object",
                    "additionalProperties": false
                }
            }
        });
        strip_gemini_unsupported_schema_fields(&mut schema);
        assert!(schema.get("additionalProperties").is_none());
        assert!(schema["properties"]["nested"]
            .get("additionalProperties")
            .is_none());
    }

    #[test]
    fn spreads_free_models_across_190_requests_per_hour() {
        assert_eq!(
            request_interval_for_model("some-model:free"),
            Duration::from_millis(18_948)
        );
        assert_eq!(
            request_interval_for_model("gemini-3.1-flash-lite-thinking"),
            Duration::from_secs(5)
        );
    }
}
