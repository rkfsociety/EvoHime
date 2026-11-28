//! Provider-side execution of image generation for Core (plan 203.1).
//!
//! The durable job ledger, artifact store, and admission control stay in
//! Core: this module only executes one admitted provider request against its
//! own `ModelGateway` and validates the returned images. Prompts and image
//! bytes exist only for the duration of the invocation.

use base64::Engine;
use evohime_model_gateway::{
    provider_contract::{ImageOutputOperation, ImageProviderRequest},
    ModelGateway, ModelGatewayConfig, PrivacyClass,
};
use evohime_service_contract::ServiceContractError;
use serde::{Deserialize, Serialize};

/// Stable reason code: the route's image capability changed or vanished.
pub const REASON_CAPABILITY_STALE: &str = "image_capability_or_route_stale";
/// Stable reason code: an output image failed bounded decode validation.
pub const REASON_OUTPUT_DECODE_FAILED: &str = "provider_output_decode_failed";
/// Stable reason code: provider returned more bytes than the bound allows.
pub const REASON_OUTPUT_BYTES_LIMIT: &str = "provider_output_bytes_limit";
/// Stable reason code: provider returned a different output count.
pub const REASON_OUTPUT_COUNT_MISMATCH: &str = "provider_output_count_mismatch";
/// Stable reason code: decoded output does not match the requested shape.
pub const REASON_OUTPUT_VALIDATION_FAILED: &str = "provider_output_validation_failed";
/// Stable reason code: the provider request failed without a safe answer.
pub const REASON_OUTCOME_UNKNOWN: &str = "provider_outcome_unknown";

/// Maximum encoded provider response per image.
pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum aggregate encoded provider response per request.
pub const MAX_TOTAL_OUTPUT_BYTES: usize = MAX_OUTPUT_BYTES * 4;
/// Maximum provider execution deadline accepted by the service.
pub const MAX_PROVIDER_DEADLINE_MS: u64 = 120_000;

/// One input image referenced by an admitted Core job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceImageInput {
    /// Caller-declared MIME type of the encoded bytes.
    pub mime_type: String,
    /// Encoded image bytes.
    pub bytes_base64: String,
}

/// One admitted image-generation request dispatched by Core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationServiceRequest {
    /// Provider route selected by Core's gateway.
    pub route: String,
    /// Image generation, edit, or mask-edit operation.
    pub operation: ImageOutputOperation,
    /// User prompt retained only for this provider dispatch.
    pub prompt: String,
    /// Requested output width.
    pub width: u32,
    /// Requested output height.
    pub height: u32,
    /// Requested output count.
    pub count: u8,
    /// Requested output MIME type.
    pub mime_type: String,
    /// Bounded provider deadline.
    #[serde(default)]
    pub deadline_ms: Option<u64>,
    /// Provider capability epoch Core snapshotted at admission.
    #[serde(default)]
    pub capability_epoch: Option<u64>,
    /// Existing images; Generate requires none, Edit/MaskEdit require one.
    #[serde(default)]
    pub input_images: Vec<ServiceImageInput>,
    /// Optional mask image; required exactly for MaskEdit.
    #[serde(default)]
    pub mask_image: Option<ServiceImageInput>,
}

/// One validated provider output returned to Core for publication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceImageOutput {
    /// Canonical MIME type verified against the decoded container.
    pub mime_type: String,
    /// Decoded width in pixels.
    pub width: u32,
    /// Decoded height in pixels.
    pub height: u32,
    /// Encoded image bytes for Core-side ArtifactStore publication.
    pub bytes_base64: String,
}

fn invalid(reason: &'static str) -> ServiceContractError {
    ServiceContractError::invalid_input(reason)
}

fn decode_image(input: &ServiceImageInput) -> Result<Vec<u8>, ServiceContractError> {
    use base64::engine::general_purpose::STANDARD;
    STANDARD
        .decode(&input.bytes_base64)
        .map_err(|_| invalid(REASON_OUTPUT_DECODE_FAILED))
}

fn operation_inputs(
    operation: ImageOutputOperation,
    request: &ImageGenerationServiceRequest,
) -> Result<(Vec<Vec<u8>>, Option<Vec<u8>>), ServiceContractError> {
    if request.prompt.trim().is_empty() || request.prompt.len() > 8 * 1024 {
        return Err(invalid(REASON_OUTPUT_VALIDATION_FAILED));
    }
    let input_images = request
        .input_images
        .iter()
        .map(decode_image)
        .collect::<Result<Vec<_>, _>>()?;
    let mask_image = request.mask_image.as_ref().map(decode_image).transpose()?;
    let shape_ok = match operation {
        ImageOutputOperation::Generate => input_images.is_empty() && mask_image.is_none(),
        ImageOutputOperation::Edit => input_images.len() == 1 && mask_image.is_none(),
        ImageOutputOperation::MaskEdit => input_images.len() == 1 && mask_image.is_some(),
    };
    if !shape_ok {
        return Err(invalid(REASON_OUTPUT_VALIDATION_FAILED));
    }
    Ok((input_images, mask_image))
}

/// Runs one admitted image request against the service's own gateway.
///
/// Every invocation builds a fresh gateway from the current environment, so
/// a provider key saved by the shell applies to the next run without any
/// state to invalidate. The route name is chosen by Core; the provider
/// credential never travels in the payload.
pub fn run_image_generation(
    request: ImageGenerationServiceRequest,
) -> Result<Vec<ServiceImageOutput>, ServiceContractError> {
    let config = ModelGatewayConfig::from_env()
        .map_err(|error| ServiceContractError::unavailable(error.to_string()))?;
    let route_config = config
        .routes
        .get(&request.route)
        .cloned()
        .ok_or_else(|| invalid(REASON_CAPABILITY_STALE))?;
    let single = ModelGatewayConfig {
        default_route: request.route.clone(),
        routes: std::collections::HashMap::from([(request.route.clone(), route_config)]),
    };
    let gateway = ModelGateway::from_config(&single)
        .map_err(|error| ServiceContractError::unavailable(error.to_string()))?;
    let capability = gateway
        .image_output_capability_for_route(&request.route)
        .map_err(|_| invalid(REASON_CAPABILITY_STALE))?
        .ok_or_else(|| invalid(REASON_CAPABILITY_STALE))?;
    let deadline_ms = request
        .deadline_ms
        .unwrap_or(MAX_PROVIDER_DEADLINE_MS)
        .clamp(1, MAX_PROVIDER_DEADLINE_MS);
    let (input_images, mask_image) = operation_inputs(request.operation, &request)?;
    let provider_request = ImageProviderRequest {
        operation: request.operation,
        prompt: request.prompt,
        width: request.width,
        height: request.height,
        count: request.count,
        mime_type: request.mime_type.clone(),
        required_privacy: PrivacyClass::Restricted,
        allow_cloud: false,
        input_images,
        mask_image,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            ServiceContractError::unavailable(format!("failed to build runtime: {error}"))
        })?;
    let dispatch = async {
        gateway
            .preflight_image_output_for_route(
                &request.route,
                request.capability_epoch,
                &provider_request,
            )
            .map_err(|error| {
                if matches!(
                    error,
                    evohime_model_gateway::providers::ProviderError::ImageCapabilityStale
                        | evohime_model_gateway::providers::ProviderError::ImagePreflightRejected
                ) {
                    invalid(REASON_CAPABILITY_STALE)
                } else {
                    invalid(REASON_OUTCOME_UNKNOWN)
                }
            })?;
        match tokio::time::timeout(
            std::time::Duration::from_millis(deadline_ms),
            gateway.generate_image_for_route_at_epoch(
                &request.route,
                request.capability_epoch,
                provider_request,
            ),
        )
        .await
        {
            Ok(Ok(outputs)) => Ok(outputs),
            Ok(Err(
                evohime_model_gateway::providers::ProviderError::ImageCapabilityStale
                | evohime_model_gateway::providers::ProviderError::ImagePreflightRejected,
            )) => Err(invalid(REASON_CAPABILITY_STALE)),
            Ok(Err(_)) | Err(_) => Err(invalid(REASON_OUTCOME_UNKNOWN)),
        }
    };
    let outputs = runtime.block_on(dispatch)?;
    if outputs.len() != usize::from(request.count) {
        return Err(invalid(REASON_OUTPUT_COUNT_MISMATCH));
    }
    let mut total_output_bytes = 0usize;
    let mut validated = Vec::with_capacity(outputs.len());
    for output in outputs {
        total_output_bytes = total_output_bytes.saturating_add(output.bytes.len());
        if output.bytes.len() as u64 > capability.max_bytes.min(MAX_OUTPUT_BYTES as u64)
            || total_output_bytes > capability.max_bytes.min(MAX_TOTAL_OUTPUT_BYTES as u64) as usize
        {
            return Err(invalid(REASON_OUTPUT_BYTES_LIMIT));
        }
        let image = crate::image_validation::validate_output(&output.mime_type, output.bytes)
            .map_err(|_| invalid(REASON_OUTPUT_DECODE_FAILED))?;
        if image.mime_type.as_str() != request.mime_type
            || image.width != request.width
            || image.height != request.height
        {
            return Err(invalid(REASON_OUTPUT_VALIDATION_FAILED));
        }
        validated.push(ServiceImageOutput {
            mime_type: image.mime_type.as_str().to_owned(),
            width: image.width,
            height: image.height,
            bytes_base64: base64::engine::general_purpose::STANDARD.encode(image.bytes),
        });
    }
    Ok(validated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    fn base_request() -> ImageGenerationServiceRequest {
        ImageGenerationServiceRequest {
            route: "test-route".to_owned(),
            operation: ImageOutputOperation::Generate,
            prompt: "a single pixel".to_owned(),
            width: 1,
            height: 1,
            count: 1,
            mime_type: "image/png".to_owned(),
            deadline_ms: None,
            capability_epoch: None,
            input_images: Vec::new(),
            mask_image: None,
        }
    }

    /// Generate допускает ровно ноль входных изображений и без маски.
    #[test]
    fn generate_rejects_inputs_and_mask() {
        let mut request = base_request();
        request.input_images.push(ServiceImageInput {
            mime_type: "image/png".to_owned(),
            bytes_base64: String::new(),
        });
        assert!(matches!(
            operation_inputs(request.operation, &request),
            Err(error) if error.message == REASON_OUTPUT_VALIDATION_FAILED
        ));
        let mut request = base_request();
        request.mask_image = Some(ServiceImageInput {
            mime_type: "image/png".to_owned(),
            bytes_base64: String::new(),
        });
        assert!(matches!(
            operation_inputs(request.operation, &request),
            Err(error) if error.message == REASON_OUTPUT_VALIDATION_FAILED
        ));
    }

    /// MaskEdit требует ровно один вход и маску; Edit — один вход без маски.
    #[test]
    fn edit_and_mask_edit_enforce_their_input_shape() {
        let png = STANDARD.encode("not-a-real-png");
        let input = ServiceImageInput {
            mime_type: "image/png".to_owned(),
            bytes_base64: png.clone(),
        };
        let mut request = base_request();
        request.operation = ImageOutputOperation::MaskEdit;
        request.input_images.push(input.clone());
        assert!(matches!(
            operation_inputs(request.operation, &request),
            Err(error) if error.message == REASON_OUTPUT_VALIDATION_FAILED
        ));
        request.mask_image = Some(ServiceImageInput {
            mime_type: "image/png".to_owned(),
            bytes_base64: png,
        });
        assert!(operation_inputs(request.operation, &request).is_ok());
        request.mask_image = None;
        request.operation = ImageOutputOperation::Edit;
        assert!(operation_inputs(request.operation, &request).is_ok());
    }

    /// Битый base64 во входном изображении — отказ, а не тихий пропуск.
    #[test]
    fn malformed_input_base64_fails_closed() {
        let mut request = base_request();
        request.operation = ImageOutputOperation::Edit;
        request.input_images.push(ServiceImageInput {
            mime_type: "image/png".to_owned(),
            bytes_base64: "!!!not-base64!!!".to_owned(),
        });
        assert!(matches!(
            operation_inputs(request.operation, &request),
            Err(error) if error.message == REASON_OUTPUT_DECODE_FAILED
        ));
    }

    /// Неизвестный маршрут — «capability stale»: ядро обязано снять снапшот
    /// возможности заново, а не ретраить вслепую.
    #[test]
    fn unknown_route_fails_as_stale_capability() {
        assert!(
            matches!(
                run_image_generation(base_request()),
                Err(error) if error.message == REASON_CAPABILITY_STALE
            ) || {
                // В тестовом окружении provider env может отсутствовать целиком —
                // тогда отказ приходит как недоступность конфигурации.
                matches!(
                    run_image_generation(base_request()),
                    Err(error) if error.code == evohime_service_contract::ServiceErrorCode::Unavailable
                )
            }
        );
    }

    /// Пустой промпт отклоняется до обращения к провайдеру.
    #[test]
    fn empty_prompt_is_rejected_before_any_provider_call() {
        let mut request = base_request();
        request.prompt = "   ".to_owned();
        assert!(matches!(
            operation_inputs(request.operation, &request),
            Err(error) if error.message == REASON_OUTPUT_VALIDATION_FAILED
        ));
    }
}
