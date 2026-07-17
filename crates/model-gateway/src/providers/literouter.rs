use crate::config::LiteRouterConfig;
use crate::providers::{
    ChatFuture, ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use crate::retry::{
    compute_backoff, is_retryable_status, parse_retry_after_seconds, RetryPolicy,
};
use crate::tools::{ChatResult, NativeToolCall, ToolSpec};
use async_stream::stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

/// LiteRouter — OpenAI-compatible provider.
///
/// API docs: `docs/providers/literouter.md`
#[derive(Debug)]
pub struct LiteRouterProvider {
    config: LiteRouterConfig,
    client: Client,
    retry: RetryPolicy,
}

impl LiteRouterProvider {
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
            .build()
            .map_err(|error| ProviderError::Http(error.to_string()))?;

        Ok(Self {
            config,
            client,
            retry,
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
        let body = ChatCompletionRequest {
            model: model.to_string(),
            messages: messages
                .iter()
                .map(|message| ApiMessage {
                    role: message.role.as_str().to_string(),
                    content: message.content.clone(),
                })
                .collect(),
            stream,
            tools: tools.map(|specs| specs.to_vec()),
            tool_choice: if tools.is_some_and(|specs| !specs.is_empty()) {
                Some(Value::String("auto".into()))
            } else {
                None
            },
        };

        let mut attempt: u32 = 0;
        loop {
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
                    if is_retryable_status(status) && attempt < self.retry.max_retries {
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
        let request_messages = messages.to_vec();
        let tools = tools.to_vec();

        Box::pin(async move {
            let response = provider
                .send_chat_request(&model, &request_messages, Some(&tools), false)
                .await?;
            let payload: CompletionResponse = response
                .json()
                .await
                .map_err(|error| ProviderError::Api(error.to_string()))?;
            Ok(payload.into_chat_result())
        })
    }
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ApiMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolSpec>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
}

#[derive(Debug, serde::Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompletionResponse {
    #[serde(default)]
    choices: Vec<CompletionChoice>,
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

impl CompletionResponse {
    fn into_chat_result(self) -> ChatResult {
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
            tool_calls,
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

fn parse_sse_line(line: &str) -> Option<Result<String, ProviderError>> {
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

    let content = chunk
        .choices
        .first()
        .and_then(|choice| choice.delta.content.clone())
        .unwrap_or_default();

    if content.is_empty() {
        return None;
    }

    Some(Ok(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolSpec;
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
    fn parses_sse_delta() {
        let result = parse_sse_line(r#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#)
            .expect("parsed")
            .expect("ok");

        assert_eq!(result, "Hi");
    }

    #[test]
    fn ignores_done_marker() {
        assert!(parse_sse_line("data: [DONE]").is_none());
    }

    #[test]
    fn completion_response_extracts_tool_calls() {
        let payload = CompletionResponse {
            choices: vec![CompletionChoice {
                message: CompletionMessage {
                    content: Some(String::new()),
                    tool_calls: vec![ApiToolCall {
                        id: Some("call_1".into()),
                        function: Some(ApiFunctionCall {
                            name: Some("filesystem.read".into()),
                            arguments: Some(r#"{"path":"README.md"}"#.into()),
                        }),
                    }],
                },
            }],
        };
        let result = payload.into_chat_result();
        assert!(result.content.is_empty());
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].name, "filesystem.read");
        assert!(result.tool_calls[0].arguments.contains("README.md"));
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
}
