#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `media-voice` service module implementing `evohime-service-contract`.

/// Provider-side execution of admitted image-generation requests.
pub mod image_generation;
/// Bounded image container validation and Windows WIC decoding.
pub mod image_validation;
pub mod optional_voice_output_adapter;
pub mod vision_contract;
/// Deterministic parser for wake-word voice commands.
pub mod voice_command;
/// Voice input dictation profile contract and validation.
pub mod voice_input_dictation;

use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

/// Service handler for image generation, vision analysis, and voice synthesis.
#[derive(Debug, Default)]
pub struct MediaVoiceService;

impl MediaVoiceService {
    /// Creates a new [`MediaVoiceService`] instance.
    pub fn new() -> Self {
        Self
    }
}

impl ServiceHandler for MediaVoiceService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "media-voice".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "media.generate_image".to_owned(),
                    description: "Generates an image from a validated prompt.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["prompt"],
                        "properties": { "prompt": { "type": "string" } }
                    }),
                    required_permissions: vec!["media.generate".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "media.describe_vision".to_owned(),
                    description: "Analyzes an image using the vision contract.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["image_path"],
                        "properties": { "image_path": { "type": "string" } }
                    }),
                    required_permissions: vec!["media.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "voice.synthesize".to_owned(),
                    description: "Synthesizes optional voice output from text.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["text"],
                        "properties": { "text": { "type": "string" } }
                    }),
                    required_permissions: vec!["voice.output".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "voice.dictation_validate".to_owned(),
                    description: "Validates a voice input dictation profile.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["voice.input".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "voice.dictation_availability".to_owned(),
                    description: "Checks voice dictation profile availability.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object" }),
                    required_permissions: vec!["voice.input".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "voice.command_parse".to_owned(),
                    description: "Parses a wake-word voice utterance into a structured intent."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["text"],
                        "properties": { "text": { "type": "string" } }
                    }),
                    required_permissions: vec!["voice.input".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "media.validate_image".to_owned(),
                    description: "Validates and decodes an encoded PNG/JPEG image within bounds."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["declared_mime", "bytes_base64"],
                        "properties": {
                            "declared_mime": { "type": "string" },
                            "bytes_base64": { "type": "string" }
                        }
                    }),
                    required_permissions: vec!["media.read".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "media-voice".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            status: ServiceHealthStatus::Ready,
            active_requests: 0,
            detail: None,
        }
    }

    fn invoke(
        &self,
        capability: &str,
        payload: serde_json::Value,
        _caller_session_id: Option<&str>,
    ) -> Result<serde_json::Value, ServiceContractError> {
        match capability {
            "media.generate_image" => {
                let request: image_generation::ImageGenerationServiceRequest =
                    serde_json::from_value(payload).map_err(|error| {
                        ServiceContractError::invalid_input(format!(
                            "invalid image generation request: {error}"
                        ))
                    })?;
                let outputs = image_generation::run_image_generation(request)?;
                serde_json::to_value(serde_json::json!({ "outputs": outputs })).map_err(|error| {
                    ServiceContractError::new(
                        evohime_service_contract::ServiceErrorCode::Internal,
                        format!("failed to serialize image outputs: {error}"),
                    )
                })
            }
            "media.describe_vision" => {
                let image_path = payload
                    .get("image_path")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("image_path must be a non-empty string")
                    })?;
                let path = std::path::Path::new(image_path);
                let ext_ok = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|ext| {
                        matches!(
                            ext.to_ascii_lowercase().as_str(),
                            "png" | "jpg" | "jpeg" | "webp"
                        )
                    });
                if !ext_ok {
                    return Err(ServiceContractError::invalid_input(
                        "unsupported image extension (expected png, jpg, jpeg, or webp)",
                    ));
                }
                let meta = std::fs::metadata(path).map_err(|err| {
                    ServiceContractError::invalid_input(format!("cannot stat {image_path}: {err}"))
                })?;
                const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
                if meta.len() > MAX_IMAGE_BYTES {
                    return Err(ServiceContractError::invalid_input(
                        "image exceeds 20 MiB limit",
                    ));
                }
                Ok(serde_json::json!({
                    "image_path": image_path,
                    "bytes": meta.len(),
                    "status": "validated"
                }))
            }
            "voice.dictation_validate" => {
                let profile: voice_input_dictation::DictationProfile =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!(
                            "invalid dictation profile: {err}"
                        ))
                    })?;
                voice_input_dictation::validate(&profile)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                serde_json::to_value(&profile).map_err(|err| {
                    ServiceContractError::new(
                        evohime_service_contract::ServiceErrorCode::Internal,
                        format!("failed to serialize validated profile: {err}"),
                    )
                })
            }
            "voice.dictation_availability" => {
                let profile: voice_input_dictation::DictationProfile =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!(
                            "invalid dictation profile: {err}"
                        ))
                    })?;
                voice_input_dictation::availability(&profile)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))
            }
            "voice.synthesize" => {
                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("text must be a non-empty string")
                    })?;
                Ok(serde_json::json!({
                    "chars": text.chars().count(),
                    "status": "accepted"
                }))
            }
            "voice.command_parse" => {
                let text = payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ServiceContractError::invalid_input("text must be a string"))?;
                match voice_command::parse(text) {
                    Some(intent) => Ok(serde_json::json!({
                        "matched": true,
                        "kind": intent.kind,
                        "target": intent.target,
                    })),
                    None => Ok(serde_json::json!({
                        "matched": false,
                    })),
                }
            }
            "media.validate_image" => {
                use base64::Engine;
                let declared_mime = payload
                    .get("declared_mime")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("declared_mime must be a string")
                    })?;
                let bytes_b64 = payload
                    .get("bytes_base64")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("bytes_base64 must be a string")
                    })?;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(bytes_b64)
                    .map_err(|_| ServiceContractError::invalid_input("invalid_base64_image"))?;
                let validated = image_validation::validate_output(declared_mime, bytes)
                    .map_err(|err| ServiceContractError::invalid_input(err.to_string()))?;
                Ok(serde_json::json!({
                    "mime_type": validated.mime_type.as_str(),
                    "width": validated.width,
                    "height": validated.height,
                    "sha256": validated.sha256,
                }))
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown media-voice capability: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_service_contract::{
        dispatch_request, validate_descriptor, ServiceRequest, ServiceResponse,
    };

    #[test]
    fn descriptor_and_dispatch_work() {
        let svc = MediaVoiceService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");

        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "m1".to_owned(),
                capability: "voice.synthesize".to_owned(),
                payload: serde_json::json!({ "text": "hello" }),
                caller_session_id: None,
            },
        );
        match resp {
            ServiceResponse::InvokeResult {
                request_id,
                payload,
            } => {
                assert_eq!(request_id, "m1");
                assert_eq!(payload["chars"], 5);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn voice_command_parse_capability_parses_and_ignores() {
        let svc = MediaVoiceService::new();
        let matched = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "vc1".to_owned(),
                capability: "voice.command_parse".to_owned(),
                payload: serde_json::json!({ "text": "Ева, открой хром" }),
                caller_session_id: None,
            },
        );
        match matched {
            ServiceResponse::InvokeResult { payload, .. } => {
                assert_eq!(payload["matched"], true);
                assert_eq!(payload["kind"], "open_app");
                assert_eq!(payload["target"], "хром");
            }
            other => panic!("unexpected response: {other:?}"),
        }

        let ignored = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "vc2".to_owned(),
                capability: "voice.command_parse".to_owned(),
                payload: serde_json::json!({ "text": "открой окно" }),
                caller_session_id: None,
            },
        );
        match ignored {
            ServiceResponse::InvokeResult { payload, .. } => {
                assert_eq!(payload["matched"], false);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
