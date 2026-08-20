//! Authenticated loopback adapter for a supervisor-owned local SLM.
//!
//! The adapter deliberately has a separate configuration type: a local
//! capability is not a cloud API key and is never copied into shell state.

use super::{ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream};
use crate::config::LiteRouterConfig;
use crate::providers::literouter::LiteRouterProvider;
use crate::retry::RetryPolicy;
use crate::tools::{NativeToolCall, ToolSpec};
use std::time::Duration;

pub const LOCAL_SESSION_TTL_MS: u64 = 30_000;
pub const LOCAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
pub const LOCAL_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LocalToolCapability {
    pub name: String,
    pub version: String,
    pub arguments_schema: serde_json::Value,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LocalCapabilityMetadata {
    pub schema_version: String,
    pub model_id: String,
    pub capability_epoch: u64,
    pub cancellation: bool,
    pub tools: Vec<LocalToolCapability>,
}

/// Bounded request capability issued by the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSessionCapability {
    token_hash: [u8; 32],
    pub request_id: String,
    pub audience: String,
    expires_at_ms: u64,
    redeemed: bool,
}

impl ProviderSessionCapability {
    pub fn issue(token: &[u8], request_id: impl Into<String>, now_ms: u64) -> Self {
        use sha2::{Digest, Sha256};
        let mut hash = [0; 32];
        hash.copy_from_slice(&Sha256::digest(token));
        Self {
            token_hash: hash,
            request_id: request_id.into(),
            audience: "local-provider".into(),
            expires_at_ms: now_ms.saturating_add(LOCAL_SESSION_TTL_MS),
            redeemed: false,
        }
    }

    pub fn redeem(
        &mut self,
        token: &[u8],
        request_id: &str,
        audience: &str,
        now_ms: u64,
    ) -> Result<(), ProviderError> {
        use sha2::{Digest, Sha256};
        let actual = Sha256::digest(token);
        if self.redeemed
            || now_ms > self.expires_at_ms
            || request_id != self.request_id
            || audience != self.audience
            || !constant_time_eq(actual.as_slice(), &self.token_hash)
        {
            return Err(ProviderError::Config("provider_session_invalid".into()));
        }
        self.redeemed = true;
        Ok(())
    }
}

#[derive(Debug)]
pub struct LocalProvider {
    inner: LiteRouterProvider,
    capability: LocalCapabilityMetadata,
}

impl LocalProvider {
    pub fn new(config: LiteRouterConfig) -> Result<Self, ProviderError> {
        if config.api_key.trim().is_empty() {
            return Err(ProviderError::Config(
                "local provider session capability is missing".into(),
            ));
        }
        let capability = LocalCapabilityMetadata {
            schema_version: "local-capability-v1".into(),
            model_id: config.model.clone(),
            capability_epoch: 1,
            cancellation: true,
            tools: Vec::new(),
        };
        let inner = LiteRouterProvider::with_retry(config, RetryPolicy::none())?;
        Ok(Self { inner, capability })
    }

    pub fn capability(&self) -> &LocalCapabilityMetadata {
        &self.capability
    }

    pub fn validate_capability(capability: &LocalCapabilityMetadata) -> Result<(), ProviderError> {
        if capability.schema_version.split('-').last() != Some("v1")
            || capability.model_id.trim().is_empty()
            || capability.capability_epoch == 0
        {
            return Err(ProviderError::Config("capability_probe_failed".into()));
        }
        let mut names = std::collections::BTreeSet::new();
        for tool in &capability.tools {
            if tool.name.trim().is_empty()
                || !names.insert(&tool.name)
                || !tool.arguments_schema.is_object()
                || (tool.requires_approval && tool.version.trim().is_empty())
            {
                return Err(ProviderError::Config("capability_probe_failed".into()));
            }
        }
        Ok(())
    }

    pub fn validate_tool_call(&self, call: &NativeToolCall) -> Result<(), ProviderError> {
        if call.id.trim().is_empty() || call.name.trim().is_empty() {
            return Err(ProviderError::Config("tool_call_malformed".into()));
        }
        let args: serde_json::Value = serde_json::from_str(&call.arguments)
            .map_err(|_| ProviderError::Config("tool_call_malformed".into()))?;
        if !args.is_object() {
            return Err(ProviderError::Config("tool_call_malformed".into()));
        }
        let Some(capability) = self
            .capability
            .tools
            .iter()
            .find(|tool| tool.name == call.name)
        else {
            return Err(ProviderError::Config("tool_call_malformed".into()));
        };
        if capability.requires_approval && capability.version.is_empty() {
            return Err(ProviderError::Config("tool_call_malformed".into()));
        }
        if !schema_accepts_object(&capability.arguments_schema, &args) {
            return Err(ProviderError::Config("tool_call_malformed".into()));
        }
        Ok(())
    }

    pub fn validate_loopback(base_url: &str) -> Result<(), ProviderError> {
        let url = reqwest::Url::parse(base_url)
            .map_err(|_| ProviderError::Config("loopback_policy_violation".into()))?;
        let host = url.host_str().unwrap_or_default();
        if url.scheme() != "http" || !matches!(host, "localhost" | "127.0.0.1" | "::1") {
            return Err(ProviderError::Config("loopback_policy_violation".into()));
        }
        Ok(())
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn schema_accepts_object(schema: &serde_json::Value, value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if schema
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|kind| kind != "object")
    {
        return false;
    }
    if let Some(required) = schema.get("required").and_then(serde_json::Value::as_array) {
        if required
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|name| !object.contains_key(name))
        {
            return false;
        }
    }
    if let Some(properties) = schema
        .get("properties")
        .and_then(serde_json::Value::as_object)
    {
        for (name, definition) in properties {
            if let Some(actual) = object.get(name) {
                if let Some(kind) = definition.get("type").and_then(serde_json::Value::as_str) {
                    let valid = match kind {
                        "string" => actual.is_string(),
                        "boolean" => actual.is_boolean(),
                        "number" | "integer" => actual.is_number(),
                        "object" => actual.is_object(),
                        "array" => actual.is_array(),
                        _ => true,
                    };
                    if !valid {
                        return false;
                    }
                }
            }
        }
    }
    true
}

impl ModelProvider for LocalProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Local
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn base_url(&self) -> &str {
        self.inner.base_url()
    }
    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        self.inner.stream_chat(messages)
    }
    fn stream_chat_with_model(&self, model: &str, messages: &[ChatMessage]) -> TokenStream {
        self.inner.stream_chat_with_model(model, messages)
    }
    fn chat_with_tools(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> super::ChatFuture {
        self.inner.chat_with_tools(model, messages, tools)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_is_single_use_and_request_bound() {
        let mut capability = ProviderSessionCapability::issue(b"secret", "r1", 100);
        assert!(capability
            .redeem(b"secret", "r1", "local-provider", 30_099)
            .is_ok());
        assert!(capability
            .redeem(b"secret", "r1", "local-provider", 30_099)
            .is_err());
    }
    #[test]
    fn rejects_non_loopback() {
        assert!(LocalProvider::validate_loopback("http://10.0.0.2:1234/v1").is_err());
        assert!(LocalProvider::validate_loopback("http://127.0.0.1:1234/v1").is_ok());
    }

    #[test]
    fn malformed_and_unknown_tool_calls_are_rejected() {
        let provider = LocalProvider::new(LiteRouterConfig {
            api_key: "cap".into(),
            base_url: "http://127.0.0.1:1/v1".into(),
            model: "slm".into(),
        })
        .unwrap();
        assert!(provider
            .validate_tool_call(&NativeToolCall {
                id: "".into(),
                name: "x".into(),
                arguments: "{}".into()
            })
            .is_err());
        assert!(provider
            .validate_tool_call(&NativeToolCall {
                id: "1".into(),
                name: "x".into(),
                arguments: "[]".into()
            })
            .is_err());
    }
}
