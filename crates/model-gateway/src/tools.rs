//! OpenAI-compatible tool calling types (Stage 7.28).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// OpenAI-compatible declaration of one callable function tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    /// Tool declaration kind, normally `function`.
    pub kind: String,
    /// Function name, description, and argument schema.
    pub function: FunctionSpec,
}

impl ToolSpec {
    /// Creates a function tool specification with JSON Schema parameters.
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
                strict: None,
                manifest_hash: None,
            },
        }
    }
}

/// Function name and parameter schema sent to a compatible provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionSpec {
    /// Stable function name invoked by the model.
    pub name: String,
    /// Human-readable summary of the function's behavior.
    pub description: String,
    /// JSON Schema object for accepted function arguments.
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Whether the provider must enforce strict schema conformance.
    pub strict: Option<bool>,
    /// Immutable Core-owned tool contract used to bind model calls to the
    /// manifest snapshot that produced this loadout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<String>,
}

/// Native provider tool call requested by an assistant response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeToolCall {
    /// Provider-generated identifier used to pair the tool result.
    pub id: String,
    /// Function name selected by the model.
    pub name: String,
    /// Raw JSON arguments object as a string (OpenAI shape).
    pub arguments: String,
}

/// Token usage from an OpenAI-compatible chat completion (TokenJam / GenAI semconv).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmUsage {
    /// Input/prompt tokens reported by the provider.
    pub prompt_tokens: u32,
    /// Generated completion tokens reported by the provider.
    pub completion_tokens: u32,
    /// Total tokens reported by the provider.
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Input tokens used to create provider-side cached context.
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Input tokens read from provider-side cached context.
    pub cache_read_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Tokens attributed to extended-thinking output.
    pub thinking_tokens: Option<u32>,
}

impl LlmUsage {
    /// Builds usage totals from prompt and completion token counts.
    pub fn from_parts(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            thinking_tokens: None,
        }
    }

    /// Reports whether the core prompt/completion/total counts are all zero.
    pub fn is_empty(self) -> bool {
        self.prompt_tokens == 0 && self.completion_tokens == 0 && self.total_tokens == 0
    }
}

/// Complete chat result including text, reasoning, tool calls, and token usage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatResult {
    /// User-facing assistant text.
    pub content: String,
    /// Optional provider reasoning content kept separate from user-facing text.
    pub thinking: Option<String>,
    /// Native tool calls requested by the assistant.
    pub tool_calls: Vec<NativeToolCall>,
    /// Token usage metadata when reported by the provider.
    pub usage: Option<LlmUsage>,
}

impl ChatResult {
    /// Reports whether the response requests at least one tool call.
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// Streaming chat item: text delta, thinking content, or final usage (when `stream_options.include_usage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatStreamItem {
    /// New assistant text content.
    Delta(String),
    /// Provider reasoning content, separate from user-facing text.
    Thinking(String),
    /// Final token-usage event.
    Usage(LlmUsage),
}
