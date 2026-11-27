#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `media-voice` service module implementing `evohime-service-contract`.

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
                let prompt = payload
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input("prompt must be a non-empty string")
                    })?;
                Ok(serde_json::json!({
                    "prompt": prompt,
                    "status": "queued"
                }))
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
}
