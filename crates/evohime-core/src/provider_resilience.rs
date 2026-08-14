//! Wave VII: Provider resilience and graceful degradation.
//!
//! Handles provider errors (timeouts, overload, network failures), retries with
//! exponential backoff, and graceful fallbacks for unavailable components.
//!
//! Environment variables:
//! - `EVOHIME_MODEL_TIMEOUT_SECS` (default 120) — model timeout in seconds
//! - `EVOHIME_PROVIDER_RETRY_MAX` (default 3) — maximum retry attempts
//! - `EVOHIME_PROVIDER_BACKOFF_BASE_MS` (default 500) — base backoff in ms

use evohime_model_gateway::providers::ProviderError;
use serde::Serialize;
use std::time::Duration;

/// Configuration for provider error handling.
#[derive(Clone, Debug)]
pub struct ProviderResilienceConfig {
    pub model_timeout_secs: u64,
    pub retry_max: u32,
    pub backoff_base_ms: u64,
}

impl Default for ProviderResilienceConfig {
    fn default() -> Self {
        Self {
            model_timeout_secs: env_u64("EVOHIME_MODEL_TIMEOUT_SECS", 120),
            retry_max: env_u32("EVOHIME_PROVIDER_RETRY_MAX", 3),
            backoff_base_ms: env_u64("EVOHIME_PROVIDER_BACKOFF_BASE_MS", 500),
        }
    }
}

/// Classifies a provider error as retriable or terminal.
pub fn is_retriable_error(error: &ProviderError) -> bool {
    match error {
        ProviderError::Http(msg) => {
            // Network timeout, connection refused, etc.
            msg.contains("timeout")
                || msg.contains("connection")
                || msg.contains("temporarily unavailable")
        }
        ProviderError::Api(msg) => {
            // Rate limit (429), overload (503), etc.
            msg.contains("429")
                || msg.contains("503")
                || msg.contains("rate limit")
                || msg.contains("overload")
        }
        ProviderError::Config(_) => false, // Config errors are not retriable
        ProviderError::Stream(_) => true,  // Transient streaming error
    }
}

/// Computes exponential backoff with bounded delay.
pub fn provider_backoff(attempt: u32, config: &ProviderResilienceConfig) -> Duration {
    let factor = 2u32.saturating_pow(attempt.min(5));
    let scaled = Duration::from_millis(config.backoff_base_ms)
        .checked_mul(factor)
        .unwrap_or(Duration::from_secs(30));
    if scaled > Duration::from_secs(30) {
        Duration::from_secs(30)
    } else {
        scaled
    }
}

/// Task result indicating success or failure with optional details.
#[derive(Clone, Debug, Serialize)]
pub enum TaskResult {
    Success,
    Failed(String),
}

/// Handles a provider error and produces a task result.
/// Returns TaskFailed if error is terminal, or Ok(TaskResult::Failed) for logging retriable errors.
pub fn handle_provider_error(
    error: &ProviderError,
    _config: &ProviderResilienceConfig,
) -> TaskResult {
    if is_retriable_error(error) {
        TaskResult::Failed(format!("provider error (retriable): {}", error))
    } else {
        TaskResult::Failed(format!("provider error (terminal): {}", error))
    }
}

/// Default fallback tool specs when provider specs are unavailable.
pub fn default_tool_specs() -> Vec<evohime_model_gateway::ToolSpec> {
    vec![
        evohime_model_gateway::ToolSpec::function(
            "filesystem.list",
            "List directory contents",
            serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } }
            }),
        ),
        evohime_model_gateway::ToolSpec::function(
            "filesystem.read",
            "Read file contents",
            serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        evohime_model_gateway::ToolSpec::function(
            "filesystem.search",
            "Search file contents",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["query"]
            }),
        ),
    ]
}

/// Readonly mode tool filter: allows only read-only tools.
pub fn filter_readonly_tools(
    tools: &[evohime_model_gateway::ToolSpec],
) -> Vec<evohime_model_gateway::ToolSpec> {
    let readonly_names = ["filesystem.list", "filesystem.read", "filesystem.search"];
    tools
        .iter()
        .filter(|tool| readonly_names.contains(&tool.function.name.as_str()))
        .cloned()
        .collect()
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_retriable_errors() {
        let http_timeout = ProviderError::Http("timeout waiting for response".to_string());
        assert!(is_retriable_error(&http_timeout));

        let api_rate_limit = ProviderError::Api("429 Too Many Requests".to_string());
        assert!(is_retriable_error(&api_rate_limit));

        let config_error = ProviderError::Config("invalid config".to_string());
        assert!(!is_retriable_error(&config_error));
    }

    #[test]
    fn computes_exponential_backoff() {
        let config = ProviderResilienceConfig {
            model_timeout_secs: 120,
            retry_max: 3,
            backoff_base_ms: 100,
        };
        assert_eq!(provider_backoff(0, &config), Duration::from_millis(100));
        assert_eq!(provider_backoff(1, &config), Duration::from_millis(200));
        assert_eq!(provider_backoff(2, &config), Duration::from_millis(400));
        assert!(provider_backoff(5, &config) <= Duration::from_secs(30));
    }

    #[test]
    fn default_specs_include_readonly_tools() {
        let specs = default_tool_specs();
        assert!(specs.iter().any(|s| s.function.name == "filesystem.list"));
        assert!(specs.iter().any(|s| s.function.name == "filesystem.read"));
        assert!(specs.iter().any(|s| s.function.name == "filesystem.search"));
    }

    #[test]
    fn readonly_filter_removes_mutation_tools() {
        let all_specs = vec![
            evohime_model_gateway::ToolSpec::function(
                "filesystem.list",
                "List",
                serde_json::json!({}),
            ),
            evohime_model_gateway::ToolSpec::function(
                "filesystem.write",
                "Write",
                serde_json::json!({}),
            ),
            evohime_model_gateway::ToolSpec::function(
                "shell.execute",
                "Execute",
                serde_json::json!({}),
            ),
        ];
        let readonly = filter_readonly_tools(&all_specs);
        assert_eq!(readonly.len(), 1);
        assert_eq!(readonly[0].function.name, "filesystem.list");
    }

    #[test]
    fn model_timeout_respects_env() {
        let original = std::env::var("EVOHIME_MODEL_TIMEOUT_SECS").ok();
        std::env::set_var("EVOHIME_MODEL_TIMEOUT_SECS", "300");
        let config = ProviderResilienceConfig::default();
        assert_eq!(config.model_timeout_secs, 300);
        if let Some(original) = original {
            std::env::set_var("EVOHIME_MODEL_TIMEOUT_SECS", original);
        } else {
            std::env::remove_var("EVOHIME_MODEL_TIMEOUT_SECS");
        }
    }

    #[test]
    fn retry_max_respects_env() {
        let original = std::env::var("EVOHIME_PROVIDER_RETRY_MAX").ok();
        std::env::set_var("EVOHIME_PROVIDER_RETRY_MAX", "5");
        let config = ProviderResilienceConfig::default();
        assert_eq!(config.retry_max, 5);
        if let Some(original) = original {
            std::env::set_var("EVOHIME_PROVIDER_RETRY_MAX", original);
        } else {
            std::env::remove_var("EVOHIME_PROVIDER_RETRY_MAX");
        }
    }

    #[test]
    fn handle_provider_error_creates_task_result() {
        let config = ProviderResilienceConfig::default();
        let error = ProviderError::Http("timeout".to_string());
        let result = handle_provider_error(&error, &config);
        match result {
            TaskResult::Failed(msg) => {
                assert!(msg.contains("retriable"));
            }
            TaskResult::Success => panic!("expected Failed result"),
        }
    }

    #[test]
    fn classifies_network_timeout_as_retriable() {
        let network_error = ProviderError::Http("connection timeout after 30s".to_string());
        assert!(is_retriable_error(&network_error));
    }

    #[test]
    fn classifies_rate_limit_429_as_retriable() {
        let rate_limit = ProviderError::Api("429 Too Many Requests".to_string());
        assert!(is_retriable_error(&rate_limit));
    }

    #[test]
    fn classifies_service_unavailable_503_as_retriable() {
        let overload = ProviderError::Api("503 Service Unavailable".to_string());
        assert!(is_retriable_error(&overload));
    }

    #[test]
    fn classifies_config_error_as_terminal() {
        let config_error = ProviderError::Config("API key not set".to_string());
        assert!(!is_retriable_error(&config_error));
    }

    #[test]
    fn classifies_streaming_error_as_retriable() {
        let stream_error = ProviderError::Stream("connection lost".to_string());
        assert!(is_retriable_error(&stream_error));
    }

    #[test]
    fn backoff_delay_is_bounded() {
        let config = ProviderResilienceConfig {
            model_timeout_secs: 120,
            retry_max: 3,
            backoff_base_ms: 100,
        };
        // Even with high attempt numbers, backoff should not exceed max
        assert!(provider_backoff(10, &config) <= Duration::from_secs(30));
        assert!(provider_backoff(20, &config) <= Duration::from_secs(30));
    }

    #[test]
    fn default_tool_specs_are_sorted_by_name() {
        let specs = default_tool_specs();
        let names: Vec<_> = specs.iter().map(|s| &s.function.name).collect();
        let mut sorted_names = names.clone();
        sorted_names.sort();
        assert_eq!(names, sorted_names);
    }

    #[test]
    fn readonly_filter_preserves_read_only_tools_in_order() {
        let all_specs = vec![
            evohime_model_gateway::ToolSpec::function(
                "filesystem.search",
                "Search",
                serde_json::json!({}),
            ),
            evohime_model_gateway::ToolSpec::function(
                "filesystem.read",
                "Read",
                serde_json::json!({}),
            ),
            evohime_model_gateway::ToolSpec::function(
                "filesystem.list",
                "List",
                serde_json::json!({}),
            ),
        ];
        let readonly = filter_readonly_tools(&all_specs);
        assert_eq!(readonly.len(), 3);
        assert_eq!(readonly[0].function.name, "filesystem.search");
        assert_eq!(readonly[1].function.name, "filesystem.read");
        assert_eq!(readonly[2].function.name, "filesystem.list");
    }

    #[test]
    fn backoff_respects_base_delay_setting() {
        let config1 = ProviderResilienceConfig {
            model_timeout_secs: 120,
            retry_max: 3,
            backoff_base_ms: 100,
        };
        let config2 = ProviderResilienceConfig {
            model_timeout_secs: 120,
            retry_max: 3,
            backoff_base_ms: 500,
        };
        assert_eq!(provider_backoff(0, &config1), Duration::from_millis(100));
        assert_eq!(provider_backoff(0, &config2), Duration::from_millis(500));
        assert!(provider_backoff(1, &config2) > provider_backoff(1, &config1));
    }
}
