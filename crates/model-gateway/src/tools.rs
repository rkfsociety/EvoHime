//! OpenAI-compatible tool calling types (Stage 7.28).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub kind: String,
    pub function: FunctionSpec,
}

impl ToolSpec {
    pub fn function(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
    ) -> Self {
        Self {
            kind: "function".into(),
            function: FunctionSpec {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON arguments object as a string (OpenAI shape).
    pub arguments: String,
}

/// Token usage from an OpenAI-compatible chat completion (TokenJam / GenAI semconv).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl LlmUsage {
    pub fn from_parts(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
        }
    }

    pub fn is_empty(self) -> bool {
        self.prompt_tokens == 0 && self.completion_tokens == 0 && self.total_tokens == 0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatResult {
    pub content: String,
    pub tool_calls: Vec<NativeToolCall>,
    pub usage: Option<LlmUsage>,
}

impl ChatResult {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// Streaming chat item: text delta or final usage (when `stream_options.include_usage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatStreamItem {
    Delta(String),
    Usage(LlmUsage),
}
