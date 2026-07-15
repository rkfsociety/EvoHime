use crate::config::LiteRouterConfig;
use crate::providers::{
    ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use async_stream::stream;
use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;

/// LiteRouter — OpenAI-compatible provider.
///
/// API docs: `docs/providers/literouter.md`
pub struct LiteRouterProvider {
    config: LiteRouterConfig,
    client: Client,
}

impl LiteRouterProvider {
    pub fn new(config: LiteRouterConfig) -> Result<Self, ProviderError> {
        if config.api_key.is_empty() {
            return Err(ProviderError::Config(
                "LiteRouter API key must not be empty".to_string(),
            ));
        }

        let client = Client::builder()
            .build()
            .map_err(|error| ProviderError::Http(error.to_string()))?;

        Ok(Self { config, client })
    }

    pub fn config(&self) -> &LiteRouterConfig {
        &self.config
    }

    pub fn chat_completions_url(&self) -> String {
        self.config.chat_completions_url()
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
        let config = self.config.clone();
        let client = self.client.clone();
        let request_messages = messages.to_vec();

        Box::pin(stream! {
            let body = ChatCompletionRequest {
                model: config.model.clone(),
                messages: request_messages
                    .iter()
                    .map(|message| ApiMessage {
                        role: message.role.as_str().to_string(),
                        content: message.content.clone(),
                    })
                    .collect(),
                stream: true,
            };

            let response = match client
                .post(config.chat_completions_url())
                .bearer_auth(&config.api_key)
                .json(&body)
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    yield Err(ProviderError::Http(error.to_string()));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                yield Err(ProviderError::Api(format!("{status}: {text}")));
                return;
            }

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
}

#[derive(Debug, serde::Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ApiMessage>,
    stream: bool,
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
        let result = parse_sse_line(
            r#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#,
        )
        .expect("parsed")
        .expect("ok");

        assert_eq!(result, "Hi");
    }

    #[test]
    fn ignores_done_marker() {
        assert!(parse_sse_line("data: [DONE]").is_none());
    }
}
