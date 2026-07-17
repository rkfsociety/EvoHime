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
    pub fn function(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatResult {
    pub content: String,
    pub tool_calls: Vec<NativeToolCall>,
}

impl ChatResult {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}
