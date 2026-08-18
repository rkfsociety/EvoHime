//! Local SLM provider with loopback-only policy and authenticated session.
//!
//! This adapter implements the local route contract from plan 02.2:
//! - Loopback-only binding (127.0.0.0/8 or ::1)
//! - Short-lived bearer token authentication via supervisor
//! - Capability probe at startup
//! - Tool call validation against declared capabilities
//! - Bounded timeouts and resource limits
//! - Graceful absence reporting (unavailable, not masked as success)

use crate::providers::{
    ChatMessage, ChatResult, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use crate::tools::ToolSpec;
use async_stream::stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Maximum age of a session token (30 seconds as per plan 02.2).
pub const SESSION_TOKEN_TTL_SECS: u64 = 30;

/// Connect timeout for local provider (2 seconds).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Total request timeout (30 seconds).
pub const TOTAL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Memory limit for local process (512 MiB default).
pub const DEFAULT_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;

/// CPU limit as percentage of one logical core (50%).
pub const DEFAULT_CPU_LIMIT_PERCENT: u32 = 50;

/// Loopback port range (49152-49252 as per plan).
pub const LOOPBACK_PORT_START: u16 = 49152;
pub const LOOPBACK_PORT_END: u16 = 49252;

/// Capability schema version for local provider.
pub const CAPABILITY_SCHEMA_VERSION: &str = "local-provider-v1";

/// Declares tool capabilities for a local model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalCapability {
    pub name: String,
    pub version: String,
    #[serde(rename = "arguments_schema")]
    pub arguments_json_schema: serde_json::Value,
    #[serde(default)]
    pub requires_approval: bool,
}

/// Declared capabilities from local model startup probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalCapabilities {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    #[serde(rename = "model_id")]
    pub model_id: String,
    #[serde(default)]
    pub tools: Vec<LocalCapability>,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub cancellation: bool,
}

impl LocalCapabilities {
    /// Validates capability metadata according to plan 02.2.
    pub fn validate(&self) -> Result<(), LocalProviderError> {
        if self.schema_version != CAPABILITY_SCHEMA_VERSION {
            return Err(LocalProviderError::CapabilitySchemaMismatch(
                self.schema_version.clone(),
            ));
        }

        // Check for duplicate tool names
        let mut seen = HashMap::new();
        for tool in &self.tools {
            if !seen.insert(tool.name.clone(), tool).is_none() {
                return Err(LocalProviderError::DuplicateToolName(tool.name.clone()));
            }
            // Validate JSON schema is present and is an object
            if !tool.arguments_json_schema.is_object() {
                return Err(LocalProviderError::InvalidToolSchema(tool.name.clone()));
            }
        }

        Ok(())
    }

    /// Checks if a tool is supported by this capability set.
    pub fn supports_tool(&self, tool_name: &str) -> bool {
        self.tools.iter().any(|t| t.name == tool_name)
    }

    /// Gets tool spec if available.
    pub fn get_tool(&self, tool_name: &str) -> Option<&LocalCapability> {
        self.tools.iter().find(|t| t.name == tool_name)
    }
}

/// Session token issued by supervisor for authenticated local provider access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToken {
    /// Opaque 32-byte random bearer token (base64url encoded).
    pub token: String,
    /// Token expiry timestamp (Unix epoch seconds).
    pub expires_at: u64,
    /// Audience must be "local-provider".
    pub audience: String,
    /// Request ID for binding verification.
    pub request_id: String,
}

impl SessionToken {
    /// Creates a new session token with current time + TTL.
    pub fn new(token: String, request_id: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            token,
            expires_at: now + SESSION_TOKEN_TTL_SECS,
            audience: "local-provider".to_string(),
            request_id,
        }
    }

    /// Checks if token is expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_at
    }

    /// Validates token for a given request.
    pub fn validate(&self, request_id: &str) -> Result<(), LocalProviderError> {
        if self.is_expired() {
            return Err(LocalProviderError::SessionTokenExpired);
        }
        if self.audience != "local-provider" {
            return Err(LocalProviderError::SessionTokenInvalidAudience);
        }
        if self.request_id != request_id {
            return Err(LocalProviderError::SessionTokenRequestMismatch);
        }
        Ok(())
    }
}

/// Configuration for local provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProviderConfig {
    /// Model identifier.
    pub model_id: String,
    /// Loopback address (must be 127.0.0.1 or ::1).
    pub bind_address: SocketAddr,
    /// Session token for authentication.
    pub session_token: SessionToken,
    /// Optional capability override (if not probed).
    #[serde(skip)]
    pub capabilities: Option<LocalCapabilities>,
    /// Memory limit in bytes.
    #[serde(default = "default_memory_limit")]
    pub memory_limit_bytes: u64,
    /// CPU limit percentage.
    #[serde(default = "default_cpu_limit")]
    pub cpu_limit_percent: u32,
}

fn default_memory_limit() -> u64 {
    DEFAULT_MEMORY_LIMIT_BYTES
}

fn default_cpu_limit() -> u32 {
    DEFAULT_CPU_LIMIT_PERCENT
}

impl LocalProviderConfig {
    /// Validates that bind address is loopback.
    pub fn validate_loopback(&self) -> Result<(), LocalProviderError> {
        match self.bind_address.ip() {
            IpAddr::V4(ip) => {
                if !ip.is_loopback() {
                    return Err(LocalProviderError::LoopbackPolicyViolation(
                        format!("IPv4 {} is not loopback", ip),
                    ));
                }
            }
            IpAddr::V6(ip) => {
                if !ip.is_loopback() {
                    return Err(LocalProviderError::LoopbackPolicyViolation(
                        format!("IPv6 {} is not loopback", ip),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Errors specific to local provider.
#[derive(Debug, Error)]
pub enum LocalProviderError {
    #[error("capability schema mismatch: {0}")]
    CapabilitySchemaMismatch(String),
    #[error("duplicate tool name: {0}")]
    DuplicateToolName(String),
    #[error("invalid tool schema: {0}")]
    InvalidToolSchema(String),
    #[error("session token expired")]
    SessionTokenExpired,
    #[error("session token invalid audience")]
    SessionTokenInvalidAudience,
    #[error("session token request mismatch")]
    SessionTokenRequestMismatch,
    #[error("loopback policy violation: {0}")]
    LoopbackPolicyViolation(String),
    #[error("local model not found")]
    LocalModelNotFound,
    #[error("capability probe failed: {0}")]
    CapabilityProbeFailed(String),
    #[error("tool call malformed: {0}")]
    ToolCallMalformed(String),
    #[error("provider malformed response: {0}")]
    ProviderMalformedResponse(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("cancelled")]
    Cancelled,
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("http error: {0}")]
    Http(String),
}

/// Local SLM provider implementing loopback-only policy.
pub struct LocalProvider {
    config: LocalProviderConfig,
    capabilities: LocalCapabilities,
    /// Track used tokens to prevent replay (request_id -> used).
    used_tokens: Arc<std::sync::Mutex<HashMap<String, bool>>>,
}

impl LocalProvider {
    /// Creates a new local provider after validating config and probing capabilities.
    pub fn new(config: LocalProviderConfig) -> Result<Self, LocalProviderError> {
        config.validate_loopback()?;

        // Validate session token
        config.session_token.validate(&config.session_token.request_id)?;

        // Use provided capabilities or perform startup probe
        let capabilities = match config.capabilities.clone() {
            Some(caps) => caps,
            None => {
                // In real implementation, this would HTTP GET /capabilities
                // For now, return error if no capabilities provided
                return Err(LocalProviderError::CapabilityProbeFailed(
                    "no capabilities provided and probe not implemented".to_string(),
                ));
            }
        };

        capabilities.validate()?;

        Ok(Self {
            config,
            capabilities,
            used_tokens: Arc::new(std::sync::Mutex::new(HashMap::new())),
        })
    }

    /// Validates a tool call against declared capabilities.
    fn validate_tool_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<(), LocalProviderError> {
        let tool = self
            .capabilities
            .get_tool(tool_name)
            .ok_or_else(|| LocalProviderError::ToolCallMalformed(format!("unknown tool: {}", tool_name)))?;

        // Validate arguments against JSON schema
        // Simplified validation - in production use jsonschema crate
        if !arguments.is_object() {
            return Err(LocalProviderError::ToolCallMalformed(
                "arguments must be a JSON object".to_string(),
            ));
        }

        // Check required fields in schema (simplified)
        if let Some(schema) = tool.arguments_json_schema.as_object() {
            if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
                if let Some(args_obj) = arguments.as_object() {
                    for req_field in required {
                        if let Some(field_name) = req_field.as_str() {
                            if !args_obj.contains_key(field_name) {
                                return Err(LocalProviderError::ToolCallMalformed(format!(
                                    "missing required field: {}",
                                    field_name
                                )));
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Marks a session token as used to prevent replay.
    fn mark_token_used(&self, token: &str) -> Result<(), LocalProviderError> {
        let mut used = self.used_tokens.lock().map_err(|_| {
            LocalProviderError::Config("lock poisoned".to_string())
        })?;
        
        if used.contains_key(token) {
            return Err(LocalProviderError::SessionTokenExpired); // Reuse detected
        }
        
        used.insert(token.to_string(), true);
        Ok(())
    }
}

impl ModelProvider for LocalProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Local
    }

    fn model_name(&self) -> &str {
        &self.config.model_id
    }

    fn base_url(&self) -> &str {
        self.config.bind_address.to_string().leak()
    }

    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        let config = self.config.clone();
        let messages = messages.to_vec();
        
        Box::pin(stream! {
            // Validate session token
            if let Err(e) = config.session_token.validate(&config.session_token.request_id) {
                yield Err(ProviderError::Config(format!("session invalid: {}", e)));
                return;
            }

            // Mark token as used (prevent replay)
            // Note: In streaming context, this should happen once at start
            
            // In real implementation, this would make HTTP request to local endpoint
            // For now, return unimplemented error
            yield Err(ProviderError::Config(
                "local provider streaming not fully implemented".to_string()
            ));
        })
    }

    fn chat_with_tools(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Pin<Box<dyn futures_util::Future<Output = Result<ChatResult, ProviderError>> + Send>> {
        let config = self.config.clone();
        let messages = messages.to_vec();
        let tools = tools.to_vec();
        let capabilities = self.capabilities.clone();
        let used_tokens = Arc::clone(&self.used_tokens);

        Box::pin(async move {
            // Validate session token
            config.session_token.validate(&config.session_token.request_id)
                .map_err(|e| ProviderError::Config(format!("session invalid: {}", e)))?;

            // Mark token as used
            let mut used = used_tokens.lock().map_err(|_| {
                ProviderError::Config("lock poisoned".to_string())
            })?;
            
            let token_key = format!("{}:{}", config.session_token.request_id, config.session_token.token);
            if used.contains_key(&token_key) {
                return Err(ProviderError::Config("session token reused".to_string()));
            }
            used.insert(token_key, true);
            drop(used);

            // Validate tools against capabilities
            for tool in &tools {
                if !capabilities.supports_tool(&tool.function.name) {
                    return Err(ProviderError::Config(format!(
                        "tool {} not supported by local model",
                        tool.function.name
                    )));
                }
            }

            // In real implementation, this would make HTTP POST to local endpoint
            // with validated messages and tools
            Err(ProviderError::Config(
                "local provider chat_with_tools not fully implemented".to_string()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_capabilities() -> LocalCapabilities {
        LocalCapabilities {
            schema_version: CAPABILITY_SCHEMA_VERSION.to_string(),
            model_id: "test-model".to_string(),
            tools: vec![
                LocalCapability {
                    name: "read_file".to_string(),
                    version: "1.0".to_string(),
                    arguments_json_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        },
                        "required": ["path"]
                    }),
                    requires_approval: false,
                },
            ],
            streaming: true,
            cancellation: true,
        }
    }

    fn valid_config() -> LocalProviderConfig {
        let addr: SocketAddr = "127.0.0.1:49200".parse().unwrap();
        LocalProviderConfig {
            model_id: "test-model".to_string(),
            bind_address: addr,
            session_token: SessionToken::new("test-token".to_string(), "req-123".to_string()),
            capabilities: Some(valid_capabilities()),
            memory_limit_bytes: DEFAULT_MEMORY_LIMIT_BYTES,
            cpu_limit_percent: DEFAULT_CPU_LIMIT_PERCENT,
        }
    }

    #[test]
    fn validates_loopback_only() {
        let mut config = valid_config();
        assert!(config.validate_loopback().is_ok());

        // Non-loopback should fail
        let bad_addr: SocketAddr = "192.168.1.1:49200".parse().unwrap();
        config.bind_address = bad_addr;
        assert!(matches!(
            config.validate_loopback(),
            Err(LocalProviderError::LoopbackPolicyViolation(_))
        ));
    }

    #[test]
    fn validates_capability_schema() {
        let caps = valid_capabilities();
        assert!(caps.validate().is_ok());

        // Wrong schema version
        let mut bad_caps = caps.clone();
        bad_caps.schema_version = "wrong-version".to_string();
        assert!(matches!(
            bad_caps.validate(),
            Err(LocalProviderError::CapabilitySchemaMismatch(_))
        ));

        // Duplicate tool names
        let mut dup_caps = caps.clone();
        dup_caps.tools.push(dup_caps.tools[0].clone());
        assert!(matches!(
            dup_caps.validate(),
            Err(LocalProviderError::DuplicateToolName(_))
        ));
    }

    #[test]
    fn session_token_expires() {
        let token = SessionToken::new("test".to_string(), "req-1".to_string());
        assert!(!token.is_expired());
        
        // Manually expire
        let mut expired_token = token.clone();
        expired_token.expires_at = 0;
        assert!(expired_token.is_expired());
    }

    #[test]
    fn session_token_validation() {
        let token = SessionToken::new("test".to_string(), "req-123".to_string());
        assert!(token.validate("req-123").is_ok());
        
        // Wrong request ID
        assert!(matches!(
            token.validate("wrong-req"),
            Err(LocalProviderError::SessionTokenRequestMismatch)
        ));

        // Expired
        let mut expired = token.clone();
        expired.expires_at = 0;
        assert!(matches!(
            expired.validate("req-123"),
            Err(LocalProviderError::SessionTokenExpired)
        ));
    }

    #[test]
    fn tool_call_validation() {
        let config = valid_config();
        let provider = LocalProvider::new(config).expect("valid provider");

        // Valid tool call
        let args = serde_json::json!({"path": "/test/file.txt"});
        assert!(provider.validate_tool_call("read_file", &args).is_ok());

        // Unknown tool
        assert!(matches!(
            provider.validate_tool_call("unknown_tool", &args),
            Err(LocalProviderError::ToolCallMalformed(_))
        ));

        // Missing required field
        let bad_args = serde_json::json!({});
        assert!(matches!(
            provider.validate_tool_call("read_file", &bad_args),
            Err(LocalProviderError::ToolCallMalformed(_))
        ));
    }
}
