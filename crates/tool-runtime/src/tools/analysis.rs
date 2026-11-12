//! Declared `analysis.run` tool: bounded access to the Core-owned analysis kernel.
//!
//! This module only shapes and validates input and formats the bounded result.
//! Execution is delegated to the [`crate::KernelHost`] attached to the registry,
//! so session ownership, admission, limits, idempotency and recovery stay in
//! Core. Without a host the call fails closed instead of running in process.

use crate::{AnalysisOperation, AnalysisSensitivity, KernelHost, KernelToolRequest};
use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// Registry identifier for the declared analysis kernel tool.
pub const NAME: &str = "analysis.run";
/// Input and output summary exposed in tool catalogs.
pub const DESCRIPTION: &str = "Run one bounded computation in the Core-owned Persistent Analysis Kernel. Input: { operation: json_parse|csv_summary|json_select|object_put, args, logical_name?, type_hint?, sensitivity? }. No filesystem, network, shell or credential access.";
/// The kernel grants no extra authority, so the tool declares no permissions.
pub const PERMISSIONS: &[Permission] = &[];
/// Maximum duration advertised to the tool registry.
pub const TIMEOUT: Duration = Duration::from_secs(60);
/// Maximum accepted canonical argument payload, matching the kernel request bound.
pub const MAX_ARGS_BYTES: usize = 16 * 1024;
/// Maximum rendered inline result size returned to the model.
pub const MAX_INLINE_RESULT_BYTES: usize = 512 * 1024;

/// Operation names accepted by the declared input schema.
pub const OPERATIONS: &[&str] = &["json_parse", "json_select", "csv_summary", "object_put"];
/// Sensitivity names accepted by the declared input schema.
pub const SENSITIVITIES: &[&str] = &["public", "internal", "sensitive"];

/// Builds the canonical, non-permissive input schema for `analysis.run`.
///
/// `args` carries the operation payload, which is text for `json_parse`/
/// `csv_summary` and an object for `json_select`/`object_put`. The two branches
/// are mutually exclusive, so `anyOf` states the dual shape without introducing
/// JSON-Schema keywords that provider-side declaration translators reject. Which
/// branch matches which operation is enforced by [`parse_input`] before any
/// effect, so a mismatched pair fails closed with `InvalidInput`.
pub fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": OPERATIONS,
                "description": "Kernel operation to run"
            },
            "args": {
                "description": "Operation payload: bounded text for json_parse/csv_summary, or an object for json_select/object_put",
                "anyOf": [
                    {"type": "string", "maxLength": MAX_ARGS_BYTES},
                    {
                        "type": "object",
                        "properties": {
                            "value": {},
                            "path": {
                                "type": "array",
                                "items": {"type": "string"}
                            }
                        },
                        "additionalProperties": false
                    }
                ]
            },
            "logical_name": {
                "type": "string",
                "maxLength": 128,
                "description": "Optional logical name for object_put"
            },
            "type_hint": {
                "type": "string",
                "maxLength": 128,
                "description": "Optional type label for object_put"
            },
            "sensitivity": {
                "type": "string",
                "enum": SENSITIVITIES,
                "description": "Optional sensitivity for object_put; secret objects are rejected"
            }
        },
        "required": ["operation", "args"],
        "additionalProperties": false
    })
}

fn invalid(message: impl Into<String>) -> ToolError {
    ToolError::InvalidInput {
        tool: NAME.to_string(),
        message: message.into(),
    }
}

fn bounded_name(input: &Value, field: &str) -> Result<Option<String>, ToolError> {
    match input.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            if value.is_empty() || value.chars().any(char::is_control) || value.len() > 128 {
                return Err(invalid(format!("{field} is invalid")));
            }
            Ok(Some(value.clone()))
        }
        Some(_) => Err(invalid(format!("{field} must be a string"))),
    }
}

/// Validates tool input and converts it into the bounded host request.
///
/// # Errors
///
/// Returns [`ToolError::InvalidInput`] when the operation is unknown, the
/// payload shape does not match the operation, a name field is malformed, the
/// arguments exceed [`MAX_ARGS_BYTES`], or the payload is not valid JSON.
pub fn parse_input(input: &Value) -> Result<KernelToolRequest, ToolError> {
    let operation = input
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("operation is required"))?;
    let operation = AnalysisOperation::parse(operation)
        .ok_or_else(|| invalid("operation is not supported by the analysis kernel"))?;
    let logical_name = bounded_name(input, "logical_name")?;
    let type_hint = bounded_name(input, "type_hint")?;
    let sensitivity = match input.get("sensitivity").and_then(Value::as_str) {
        None => None,
        Some(value) => Some(
            AnalysisSensitivity::parse(value)
                .ok_or_else(|| invalid("sensitivity is not supported"))?,
        ),
    };
    let args = match input.get("args") {
        Some(Value::String(text)) => {
            if !matches!(
                operation,
                AnalysisOperation::JsonParse | AnalysisOperation::CsvSummary
            ) {
                return Err(invalid("args must be an object for this operation"));
            }
            text.as_bytes().to_vec()
        }
        Some(value @ Value::Object(_)) => {
            let payload = match operation {
                AnalysisOperation::JsonSelect => value.clone(),
                AnalysisOperation::ObjectPut => json!({
                    "logical_name": logical_name.clone().unwrap_or_else(|| "object".to_string()),
                    "type_hint": type_hint.clone().unwrap_or_else(|| "json".to_string()),
                    "value": value.get("value").cloned().unwrap_or(Value::Null),
                    "sensitivity": sensitivity.map(AnalysisSensitivity::as_str).unwrap_or("internal"),
                }),
                AnalysisOperation::JsonParse | AnalysisOperation::CsvSummary => {
                    return Err(invalid("args must be a string for this operation"))
                }
            };
            serde_json::to_vec(&payload).map_err(|_| invalid("args could not be encoded"))?
        }
        // `args` is required for every operation: kernel compute has no
        // implicit default payload, and failing closed keeps semantics explicit.
        None => return Err(invalid("args is required")),
        Some(_) => return Err(invalid("args must be a string or an object")),
    };
    if args.len() > MAX_ARGS_BYTES {
        return Err(invalid("args exceeds the kernel request bound"));
    }
    Ok(KernelToolRequest {
        operation,
        args,
        logical_name,
        type_hint,
        sensitivity,
    })
}

/// Executes one declared kernel call through the attached Core-owned host.
///
/// # Errors
///
/// Returns [`ToolError::Execution`] with the typed host code when no host is
/// attached, the kernel refuses the call, or the bounded result cannot be
/// returned.
pub async fn execute(
    ctx: &ToolContext,
    input: Value,
    host: Option<&Arc<dyn KernelHost>>,
) -> Result<ToolResult, ToolError> {
    let request = parse_input(&input)?;
    let operation = request.operation;
    let logical_name = request.logical_name.clone();
    let Some(host) = host else {
        return Err(ToolError::Execution(
            crate::kernel_host::KERNEL_UNAVAILABLE.to_string(),
        ));
    };
    let binding = crate::KernelTaskBinding {
        task_id: ctx.task_id,
        session_id: ctx.session_id,
        workspace_root: ctx.workspace_root.clone(),
    };
    let response = host
        .execute(binding, request)
        .await
        .map_err(|error| ToolError::Execution(error.to_string()))?;

    let inline_bytes = response.inline_result.as_ref().map_or(0, Vec::len);
    if inline_bytes > MAX_INLINE_RESULT_BYTES {
        return Err(ToolError::Execution(
            crate::kernel_host::KERNEL_OUTPUT_TOO_LARGE.to_string(),
        ));
    }
    let inline = response.inline_result.as_ref().map(|bytes| {
        serde_json::from_slice::<Value>(bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
    });
    let object = response.object_ref.as_ref();
    let mut structured = json!({
        "kernel_id": response.kernel_id,
        "operation": operation.as_str(),
        "sensitivity": response.sensitivity,
        "provenance": response.provenance,
    });
    if let Some(object) = object {
        structured["object"] = json!({
            "id": object.id,
            "logical_name": object.logical_name,
            "type_hint": object.type_hint,
            "size": object.size,
            "sensitivity": object.sensitivity,
            "persistence": object.persistence,
            "content_hash": object.content_hash,
        });
    }
    if let Some(inline) = inline {
        structured["result"] = inline.clone();
        let output = format!(
            "analysis.run {} on kernel {}: {}",
            operation.as_str(),
            response.kernel_id,
            inline
        );
        return Ok(ToolResult { output, structured });
    }
    let name = object
        .map(|object| object.logical_name.as_str())
        .or(logical_name.as_deref())
        .unwrap_or("object");
    Ok(ToolResult {
        output: format!(
            "analysis.run {} on kernel {}: object '{}' stored without inlining its value",
            operation.as_str(),
            response.kernel_id,
            name
        ),
        structured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_explicit_and_closed() {
        let schema = input_schema();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], Value::Bool(false));
        assert_eq!(schema["required"], json!(["operation", "args"]));
        assert!(schema["properties"].get("input").is_none());
        assert_eq!(schema["properties"]["operation"]["enum"], json!(OPERATIONS));
        assert_eq!(
            schema["properties"]["sensitivity"]["enum"],
            json!(SENSITIVITIES)
        );
        assert_eq!(
            schema["properties"]["args"]["anyOf"][1]["additionalProperties"],
            Value::Bool(false)
        );
        assert!(
            schema.get("allOf").is_none(),
            "provider-side declaration translators reject allOf/if/then"
        );
    }

    /// Validates `input` against the declared schema the model and the catalog
    /// probe both see, so schema drift fails here instead of at runtime.
    fn schema_accepts(input: &Value) -> bool {
        let schema = input_schema();
        let compiled =
            jsonschema::JSONSchema::compile(&schema).expect("analysis.run schema compiles");
        let accepted = compiled.validate(input).is_ok();
        accepted
    }

    #[test]
    fn declared_schema_accepts_only_the_shapes_the_adapter_parses() {
        // Accepted: exactly the payload the adapter forwards to the kernel.
        assert!(schema_accepts(
            &json!({"operation":"json_parse","args":"{\"a\":1}"})
        ));
        assert!(schema_accepts(
            &json!({"operation":"csv_summary","args":"a,b\n1,2\n"})
        ));
        assert!(schema_accepts(&json!({
            "operation":"json_select",
            "args":{"value":{"x":7},"path":["x"]}
        })));
        assert!(schema_accepts(&json!({
            "operation":"object_put",
            "args":{"value":{"x":1}},
            "logical_name":"table",
            "type_hint":"json",
            "sensitivity":"internal"
        })));
        assert!(schema_accepts(&json!({
            "operation":"object_put",
            "args":{"value":null}
        })));

        // Rejected before any effect: unknown or forbidden operations.
        assert!(!schema_accepts(&json!({"operation":"filesystem"})));
        assert!(!schema_accepts(&json!({"operation":"artifact_read"})));
        assert!(!schema_accepts(
            &json!({"operation":"json_parse","args":"{}","extra":1})
        ));
        assert!(!schema_accepts(
            &json!({"operation":"object_put","sensitivity":"secret"})
        ));
        assert!(!schema_accepts(&json!({"operation":"json_parse"})));

        // Rejected: `args` is neither bounded text nor a known object payload.
        assert!(!schema_accepts(&json!({"operation":"json_parse","args":7})));
        assert!(!schema_accepts(&json!({
            "operation":"json_select",
            "args":{"value":{"x":1},"path":"x"}
        })));
        assert!(!schema_accepts(&json!({
            "operation":"json_select",
            "args":{"value":{"x":1},"selector":"x"}
        })));
    }

    /// The schema admits the dual `args` shape generically; the operation/args
    /// pairing is enforced by the adapter, so this proves the stricter check
    /// lives in `parse_input` rather than in the JSON Schema branch.
    #[test]
    fn operation_args_pairing_is_enforced_by_the_adapter() {
        let object_args_for_text_op = json!({"operation":"json_parse","args":{"value":1}});
        let string_args_for_structured_op = json!({"operation":"json_select","args":"{}"});
        assert!(schema_accepts(&object_args_for_text_op));
        assert!(schema_accepts(&string_args_for_structured_op));
        assert!(parse_input(&object_args_for_text_op).is_err());
        assert!(parse_input(&string_args_for_structured_op).is_err());
    }

    #[test]
    fn parse_input_rejects_shape_mismatch() {
        assert!(parse_input(&json!({})).is_err());
        assert!(parse_input(&json!({"operation":"filesystem"})).is_err());
        assert!(parse_input(&json!({"operation":"json_parse","args":{"value":1}})).is_err());
        assert!(parse_input(&json!({"operation":"json_select","args":"{}"})).is_err());
        assert!(parse_input(
            &json!({"operation":"object_put","args":{"value":1},"sensitivity":"secret"})
        )
        .is_err());
    }

    #[test]
    fn parse_input_builds_host_request() {
        let request = parse_input(&json!({"operation":"json_parse","args":"{\"a\":1}"}))
            .expect("json_parse input is valid");
        assert_eq!(request.operation, AnalysisOperation::JsonParse);
        assert_eq!(request.args, b"{\"a\":1}".to_vec());

        let request = parse_input(&json!({
            "operation":"object_put",
            "args":{"value":{"x":1}},
            "logical_name":"table",
            "type_hint":"json",
            "sensitivity":"internal"
        }))
        .expect("object_put input is valid");
        let payload: Value =
            serde_json::from_slice(&request.args).expect("object_put payload is json");
        assert_eq!(payload["logical_name"], json!("table"));
        assert_eq!(payload["sensitivity"], json!("internal"));
        assert_eq!(payload["value"], json!({"x":1}));
    }

    #[test]
    fn parse_input_enforces_request_bound() {
        let oversized = "a".repeat(MAX_ARGS_BYTES + 1);
        assert!(parse_input(&json!({"operation":"csv_summary","args":oversized})).is_err());
    }
}
