//! Schema-first model output with provider-native and synthetic-tool fallback.

use crate::{ChatMessage, ModelGateway, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const STRUCTURED_RESPONSE_SCHEMA_VERSION: u32 = 1;
pub const MAX_SCHEMA_BYTES: usize = 64 * 1024;
pub const MAX_REPAIR_ATTEMPTS: u32 = 2;
pub const MAX_TOTAL_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStrategy {
    Auto,
    ProviderNative,
    SyntheticTool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResponseContract {
    pub schema_version: u32,
    pub contract_id: String,
    pub revision: u64,
    pub schema: Value,
    pub strategy: ResponseStrategy,
    pub contract_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseResult {
    pub contract_id: String,
    pub contract_hash: String,
    pub strategy: ResponseStrategy,
    pub attempts: u32,
    pub value: Value,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResponseError {
    #[error("unsupported structured response version: {0}")]
    UnsupportedVersion(u32),
    #[error("structured response schema is invalid: {0}")]
    Schema(String),
    #[error("structured response parse failed")]
    Parse,
    #[error("structured response validation failed: {0}")]
    Validation(String),
    #[error("multiple structured outputs returned")]
    Multiple,
    #[error("structured response strategy is unsupported")]
    Unsupported,
    #[error("structured response repair limit exceeded")]
    RepairLimit,
    #[error("provider unavailable: {0}")]
    Provider(String),
}

impl ResponseContract {
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
        value.contract_hash = value.compute_hash();
        Ok(value)
    }
    pub fn compute_hash(&self) -> String {
        let mut copy = self.clone();
        copy.contract_hash.clear();
        hex::encode(Sha256::digest(
            serde_json::to_vec(&copy).expect("contract serializes"),
        ))
    }
    pub fn validate_schema(&self) -> Result<(), ResponseError> {
        if self.schema_version != STRUCTURED_RESPONSE_SCHEMA_VERSION {
            return Err(ResponseError::UnsupportedVersion(self.schema_version));
        }
        if self.contract_id.trim().is_empty() || self.contract_id.chars().count() > 128 {
            return Err(ResponseError::Schema("contract_id".into()));
        }
        let bytes =
            serde_json::to_vec(&self.schema).map_err(|_| ResponseError::Schema("json".into()))?;
        if bytes.len() > MAX_SCHEMA_BYTES || !self.schema.is_object() {
            return Err(ResponseError::Schema("root_or_size".into()));
        }
        if !self.contract_hash.is_empty() && self.contract_hash != self.compute_hash() {
            return Err(ResponseError::Schema("contract_hash".into()));
        }
        Ok(())
    }
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

impl ModelGateway {
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
            .map_err(|error| ResponseError::Provider(error.to_string()))?;
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
                .map_err(|error| ResponseError::Provider(error.to_string()))?;
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
        assert_eq!(c.compute_hash(), c.contract_hash);
        assert!(c.validate_value(&json!({"ok":true})).is_ok());
        assert!(matches!(
            c.validate_value(&json!({})),
            Err(ResponseError::Validation(_))
        ));
    }
}
