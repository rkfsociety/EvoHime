//! Schema-first model output with provider-native and synthetic-tool fallback.

use crate::providers::ProviderError;
use crate::{ChatMessage, ModelGateway, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Schema version for structured response contracts.
pub const STRUCTURED_RESPONSE_SCHEMA_VERSION: u32 = 1;
/// Maximum serialized bytes accepted for one output schema.
pub const MAX_SCHEMA_BYTES: usize = 64 * 1024;
/// Maximum identifier length for a structured response contract.
pub const MAX_CONTRACT_ID_BYTES: usize = 128;
/// Maximum repair attempts after invalid model output.
pub const MAX_REPAIR_ATTEMPTS: u32 = 2;
/// Maximum total provider attempts including initial request and repairs.
pub const MAX_TOTAL_ATTEMPTS: u32 = 3;

/// Supported methods for requesting constrained model output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStrategy {
    /// Select a supported strategy from route capabilities.
    Auto,
    /// Require the provider's native structured-output mode.
    ProviderNative,
    /// Encode the schema as a synthetic function tool call.
    SyntheticTool,
}

/// Immutable structured-output contract with schema and strategy metadata.
/// Versioned schema and strategy used to constrain one model response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResponseContract {
    /// Contract schema version.
    pub schema_version: u32,
    /// Stable caller-provided contract identifier.
    pub contract_id: String,
    /// Monotonic contract revision.
    pub revision: u64,
    /// JSON Schema object describing accepted output.
    pub schema: Value,
    /// Provider-native or synthetic strategy policy.
    pub strategy: ResponseStrategy,
    /// Digest of the contract with this field cleared.
    pub contract_hash: String,
}

/// Parsed model value and evidence describing how it satisfied the contract.
/// Validated model output paired with the contract that accepted it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseResult {
    /// Identifier of the response contract.
    pub contract_id: String,
    /// Digest of the response contract.
    pub contract_hash: String,
    /// Strategy that produced the validated value.
    pub strategy: ResponseStrategy,
    /// Number of provider attempts consumed.
    pub attempts: u32,
    /// Parsed output value that passed schema validation.
    pub value: Value,
}

/// Contract, parsing, validation, or provider failure during structured generation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResponseError {
    /// Contract schema version is not supported by this crate.
    #[error("unsupported structured response version: {0}")]
    UnsupportedVersion(u32),
    /// Contract schema or identifier is malformed or exceeds a bound.
    #[error("structured response schema is invalid: {0}")]
    Schema(String),
    /// Provider output could not be parsed as the required JSON value.
    #[error("structured response parse failed")]
    Parse,
    /// Parsed output does not satisfy the contract schema.
    #[error("structured response validation failed: {0}")]
    Validation(String),
    /// Provider returned more than one synthetic structured result.
    #[error("multiple structured outputs returned")]
    Multiple,
    /// The selected route does not support the required response strategy.
    #[error("structured response strategy is unsupported")]
    Unsupported,
    /// Invalid output exhausted the configured repair attempts.
    #[error("structured response repair limit exceeded")]
    RepairLimit,
    /// Provider request failed while generating structured output.
    #[error("provider unavailable: {0}")]
    Provider(String),
}

impl ResponseContract {
    /// Creates a validated contract and computes its content hash.
    pub fn new(
        id: impl Into<String>,
        revision: u64,
        schema: Value,
        strategy: ResponseStrategy,
    ) -> Result<Self, ResponseError> {
        let mut value = Self {
            schema_version: STRUCTURED_RESPONSE_SCHEMA_VERSION,
            contract_id: id.into(),
            revision,
            schema,
            strategy,
            contract_hash: String::new(),
        };
        value.validate_schema()?;
        value.contract_hash = value.compute_hash()?;
        Ok(value)
    }
    /// Computes the contract digest with the self-referential hash field cleared.
    pub fn compute_hash(&self) -> Result<String, ResponseError> {
        let mut copy = self.clone();
        copy.contract_hash.clear();
        let bytes =
            serde_json::to_vec(&copy).map_err(|_| ResponseError::Schema("contract_json".into()))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
    /// Validates schema bounds, contract identity, and any stored digest.
    pub fn validate_schema(&self) -> Result<(), ResponseError> {
        if self.schema_version != STRUCTURED_RESPONSE_SCHEMA_VERSION {
            return Err(ResponseError::UnsupportedVersion(self.schema_version));
        }
        if !valid_contract_id(&self.contract_id) {
            return Err(ResponseError::Schema("contract_id".into()));
        }
        let bytes =
            serde_json::to_vec(&self.schema).map_err(|_| ResponseError::Schema("json".into()))?;
        if bytes.len() > MAX_SCHEMA_BYTES || !self.schema.is_object() {
            return Err(ResponseError::Schema("root_or_size".into()));
        }
        if !self.contract_hash.is_empty()
            && (self.contract_hash.len() != 64
                || !self
                    .contract_hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || self.contract_hash != self.compute_hash()?)
        {
            return Err(ResponseError::Schema("contract_hash".into()));
        }
        Ok(())
    }
    /// Checks a parsed model value against supported JSON Schema constraints.
    pub fn validate_value(&self, value: &Value) -> Result<(), ResponseError> {
        self.validate_schema()?;
        if self
            .schema
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("object")
            == "object"
            && !value.is_object()
        {
            return Err(ResponseError::Validation("root_type".into()));
        }
        if let Some(required) = self.schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if value.get(name).is_none() {
                    return Err(ResponseError::Validation(format!("required:{name}")));
                }
            }
        }
        if let (Some(properties), Some(object)) = (
            self.schema.get("properties").and_then(Value::as_object),
            value.as_object(),
        ) {
            for (name, rule) in properties {
                if let (Some(actual), Some(kind)) =
                    (object.get(name), rule.get("type").and_then(Value::as_str))
                {
                    let valid = match kind {
                        "string" => actual.is_string(),
                        "number" => actual.is_number(),
                        "integer" => actual.as_i64().is_some(),
                        "boolean" => actual.is_boolean(),
                        "array" => actual.is_array(),
                        "object" => actual.is_object(),
                        _ => true,
                    };
                    if !valid {
                        return Err(ResponseError::Validation(format!("type:{name}")));
                    }
                }
            }
        }
        Ok(())
    }
}

fn valid_contract_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CONTRACT_ID_BYTES
        && value == value.trim()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

impl ModelGateway {
    /// Requests a schema-constrained response and validates the returned JSON value.
    pub async fn structured_response(
        &self,
        route: &str,
        model: Option<&str>,
        messages: &[ChatMessage],
        contract: &ResponseContract,
    ) -> Result<ResponseResult, ResponseError> {
        contract.validate_schema()?;
        let native_supported = self
            .route_supports_structured_output(route)
            .map_err(|error| ResponseError::Provider(provider_error_code(&error).into()))?;
        let strategy = match contract.strategy {
            ResponseStrategy::SyntheticTool => ResponseStrategy::SyntheticTool,
            ResponseStrategy::ProviderNative if native_supported => {
                ResponseStrategy::ProviderNative
            }
            ResponseStrategy::ProviderNative => return Err(ResponseError::Unsupported),
            ResponseStrategy::Auto if native_supported => ResponseStrategy::ProviderNative,
            ResponseStrategy::Auto => ResponseStrategy::SyntheticTool,
        };
        let mut tool = ToolSpec::function(
            "__evohime_structured_output",
            "Return the contract value.",
            contract.schema.clone(),
        );
        tool.function.strict = Some(strategy == ResponseStrategy::ProviderNative);
        for attempt in 1..=MAX_TOTAL_ATTEMPTS {
            let result = self
                .chat_with_tools_for_route(route, model, messages, std::slice::from_ref(&tool))
                .await
                .map_err(|error| ResponseError::Provider(provider_error_code(&error).into()))?;
            let calls = result
                .tool_calls
                .iter()
                .filter(|call| call.name == tool.function.name)
                .collect::<Vec<_>>();
            if calls.len() > 1 {
                return Err(ResponseError::Multiple);
            }
            let raw = calls
                .first()
                .map(|call| call.arguments.clone())
                .or_else(|| (!result.content.trim().is_empty()).then_some(result.content))
                .ok_or(ResponseError::Parse)?;
            let value: Value = serde_json::from_str(&raw).map_err(|_| ResponseError::Parse)?;
            if contract.validate_value(&value).is_ok() {
                return Ok(ResponseResult {
                    contract_id: contract.contract_id.clone(),
                    contract_hash: contract.contract_hash.clone(),
                    strategy,
                    attempts: attempt,
                    value,
                });
            }
        }
        Err(ResponseError::RepairLimit)
    }
}

fn provider_error_code(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::Config(_) => "provider_configuration",
        ProviderError::Http(_) => "provider_http",
        ProviderError::Api(_) => "provider_api",
        ProviderError::Stream(_) => "provider_stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn hash_and_validation_are_deterministic() {
        let c = ResponseContract::new(
            "demo",
            1,
            json!({"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}),
            ResponseStrategy::Auto,
        )
        .unwrap();
        assert_eq!(c.compute_hash().unwrap(), c.contract_hash);
        assert!(c.validate_value(&json!({"ok":true})).is_ok());
        assert!(matches!(
            c.validate_value(&json!({})),
            Err(ResponseError::Validation(_))
        ));
    }

    #[test]
    fn provider_failures_are_projected_to_stable_codes() {
        let cases = [
            (
                ProviderError::Config("https://provider.test/?token=secret".into()),
                "provider_configuration",
            ),
            (
                ProviderError::Http("provider body with secret".into()),
                "provider_http",
            ),
            (
                ProviderError::Api("raw provider response".into()),
                "provider_api",
            ),
            (
                ProviderError::Stream("raw stream diagnostics".into()),
                "provider_stream",
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(provider_error_code(&error), expected);
            assert!(!provider_error_code(&error).contains("secret"));
        }
    }

    #[test]
    fn contract_metadata_rejects_url_like_ids_and_invalid_hashes() {
        assert!(matches!(
            ResponseContract::new(
                "https://provider.test/?prompt=secret",
                1,
                json!({"type":"object"}),
                ResponseStrategy::Auto,
            ),
            Err(ResponseError::Schema(value)) if value == "contract_id"
        ));

        let mut contract = ResponseContract::new(
            "safe.contract",
            1,
            json!({"type":"object"}),
            ResponseStrategy::Auto,
        )
        .expect("contract");
        contract.contract_hash = "z".repeat(64);
        assert_eq!(
            contract.validate_schema(),
            Err(ResponseError::Schema("contract_hash".into()))
        );
    }
}
