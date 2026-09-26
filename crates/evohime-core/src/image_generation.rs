//! Core-owned image-generation contracts and bounded output validation.
//!
//! Prompts and provider bytes stay ephemeral. Durable rows store request hashes,
//! route/capability snapshots and verified ArtifactStore references only.

use evohime_model_gateway::provider_contract::ImageOutputOperation;
use sha2::{Digest, Sha256};

/// Current Core image-generation contract version.
pub const CONTRACT_VERSION: &str = "core-image-generation/v1";
/// Maximum UTF-8 bytes in a prompt accepted by Core.
pub const MAX_PROMPT_BYTES: usize = 8 * 1024;
/// Maximum input image count.
pub const MAX_INPUT_IMAGES: usize = 4;
/// Maximum aggregate encoded input bytes.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum encoded provider response per image.
pub const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum aggregate encoded provider response per request.
pub const MAX_TOTAL_OUTPUT_BYTES: usize = MAX_OUTPUT_BYTES * 4;
/// Maximum provider execution deadline accepted by Core.
pub const MAX_PROVIDER_DEADLINE_MS: u64 = 120_000;
/// Maximum output count per request.
pub const MAX_OUTPUT_COUNT: u8 = 4;
/// Maximum output width or height.
pub const MAX_DIMENSION: u32 = 4096;
/// Maximum decoded output pixel count.
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// Maximum decoded RGBA bytes allocated during validation.
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;
/// Stable ArtifactStore binary content kind.
pub const ARTIFACT_KIND: &str = "generated_image";

/// MIME type accepted after byte-signature and platform decoder validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMimeType {
    /// PNG encoded image.
    Png,
    /// JPEG encoded image.
    Jpeg,
}

impl ImageMimeType {
    /// Returns the canonical MIME type for IPC and artifact metadata.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            _ => None,
        }
    }
}

/// Validated metadata and original encoded bytes for one provider image.
pub struct ValidatedImage {
    /// Canonical MIME type verified against the decoded container.
    pub mime_type: ImageMimeType,
    /// Decoded width in pixels.
    pub width: u32,
    /// Decoded height in pixels.
    pub height: u32,
    /// SHA-256 digest over the original encoded bytes.
    pub sha256: String,
    /// Original encoded image bytes, held only until ArtifactStore publication.
    pub bytes: Vec<u8>,
}

/// Typed refusal from Core-side image validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageValidationError {
    /// Request or output exceeds a byte or dimension bound.
    LimitExceeded,
    /// Declared MIME does not match an allow-listed image container.
    MimeMismatch,
    /// Input bytes do not contain a supported PNG or JPEG signature.
    InvalidSignature,
    /// Native decoder could not validate the complete image.
    DecodeFailed,
    /// Current operating system has no supported image decoder.
    DecoderUnavailable,
    /// Decoded dimensions or output allocation exceed the pixel bound.
    DimensionsExceeded,
}

/// Validates a provider image by declared MIME, byte signature, decoded dimensions and pixels.
///
/// The full raster is decoded into a bounded RGBA buffer before the encoded bytes
/// can be published through ArtifactStore.
pub fn validate_output(
    declared_mime: &str,
    bytes: Vec<u8>,
) -> Result<ValidatedImage, ImageValidationError> {
    if bytes.is_empty() || bytes.len() > MAX_OUTPUT_BYTES {
        return Err(ImageValidationError::LimitExceeded);
    }
    let mime = ImageMimeType::parse(declared_mime).ok_or(ImageValidationError::MimeMismatch)?;
    if signature_mime(&bytes) != Some(mime) {
        return Err(ImageValidationError::InvalidSignature);
    }
    let (width, height) = decode_dimensions(&bytes, mime)?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ImageValidationError::DimensionsExceeded);
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(ImageValidationError::DimensionsExceeded)?;
    let decoded_bytes = pixels
        .checked_mul(4)
        .ok_or(ImageValidationError::DimensionsExceeded)?;
    if pixels > MAX_PIXELS || decoded_bytes > MAX_DECODED_BYTES as u64 {
        return Err(ImageValidationError::DimensionsExceeded);
    }
    Ok(ValidatedImage {
        mime_type: mime,
        width,
        height,
        sha256: hex::encode(Sha256::digest(&bytes)),
        bytes,
    })
}

fn signature_mime(bytes: &[u8]) -> Option<ImageMimeType> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageMimeType::Png)
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some(ImageMimeType::Jpeg)
    } else {
        None
    }
}

#[cfg(windows)]
fn decode_dimensions(
    bytes: &[u8],
    mime: ImageMimeType,
) -> Result<(u32, u32), ImageValidationError> {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng,
        GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, WICBitmapDitherTypeNone,
        WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .is_ok();
    if !initialized {
        return Err(ImageValidationError::DecodeFailed);
    }
    let result = (|| unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|_| ImageValidationError::DecodeFailed)?;
        let stream = factory
            .CreateStream()
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        stream
            .InitializeFromMemory(bytes)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let expected_container = match mime {
            ImageMimeType::Png => GUID_ContainerFormatPng,
            ImageMimeType::Jpeg => GUID_ContainerFormatJpeg,
        };
        if decoder
            .GetContainerFormat()
            .map_err(|_| ImageValidationError::DecodeFailed)?
            != expected_container
        {
            return Err(ImageValidationError::MimeMismatch);
        }
        if decoder
            .GetFrameCount()
            .map_err(|_| ImageValidationError::DecodeFailed)?
            != 1
        {
            return Err(ImageValidationError::DecodeFailed);
        }
        let frame = decoder
            .GetFrame(0)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let mut width = 0;
        let mut height = 0;
        frame
            .GetSize(&mut width, &mut height)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(ImageValidationError::DimensionsExceeded)?;
        let rgba_bytes = pixels
            .checked_mul(4)
            .ok_or(ImageValidationError::DimensionsExceeded)?;
        if width == 0
            || height == 0
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
            || pixels > MAX_PIXELS
            || rgba_bytes > MAX_DECODED_BYTES as u64
        {
            return Err(ImageValidationError::DimensionsExceeded);
        }
        let converter = factory
            .CreateFormatConverter()
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        let stride = width
            .checked_mul(4)
            .ok_or(ImageValidationError::DimensionsExceeded)?;
        let buffer_len =
            usize::try_from(rgba_bytes).map_err(|_| ImageValidationError::DimensionsExceeded)?;
        let mut rgba = vec![0u8; buffer_len];
        converter
            .CopyPixels(std::ptr::null(), stride, &mut rgba)
            .map_err(|_| ImageValidationError::DecodeFailed)?;
        Ok((width, height))
    })();
    unsafe { CoUninitialize() };
    result
}

#[cfg(not(windows))]
fn decode_dimensions(
    _bytes: &[u8],
    _mime: ImageMimeType,
) -> Result<(u32, u32), ImageValidationError> {
    Err(ImageValidationError::DecoderUnavailable)
}

/// Computes a deterministic digest over a request without persisting prompt or image bytes.
pub fn request_hash(
    operation: ImageOutputOperation,
    prompt: &str,
    width: u32,
    height: u32,
    count: u8,
    mime_type: &str,
    deadline_ms: u64,
    input_hashes: &[String],
    mask_hash: Option<&str>,
    workspace_hash: Option<&str>,
) -> Result<String, ImageValidationError> {
    if prompt.trim().is_empty()
        || prompt.len() > MAX_PROMPT_BYTES
        || width == 0
        || width > MAX_DIMENSION
        || height == 0
        || height > MAX_DIMENSION
        || u64::from(width).saturating_mul(u64::from(height)) > MAX_PIXELS
        || count == 0
        || count > MAX_OUTPUT_COUNT
        || !(1..=MAX_PROVIDER_DEADLINE_MS).contains(&deadline_ms)
        || ImageMimeType::parse(mime_type).is_none()
        || input_hashes.len() > MAX_INPUT_IMAGES
        || input_hashes
            .iter()
            .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || mask_hash.is_some_and(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        || workspace_hash.is_some_and(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        || (operation == ImageOutputOperation::Generate
            && (!input_hashes.is_empty() || mask_hash.is_some()))
        || (operation == ImageOutputOperation::Edit
            && (input_hashes.len() != 1 || mask_hash.is_some()))
        || (operation == ImageOutputOperation::MaskEdit
            && (input_hashes.len() != 1 || mask_hash.is_none()))
    {
        return Err(ImageValidationError::LimitExceeded);
    }
    #[derive(serde::Serialize)]
    struct Request<'a> {
        version: &'static str,
        operation: ImageOutputOperation,
        prompt_hash: String,
        width: u32,
        height: u32,
        count: u8,
        mime_type: &'a str,
        deadline_ms: u64,
        input_hashes: &'a [String],
        mask_hash: Option<&'a str>,
        workspace_hash: Option<&'a str>,
    }
    let canonical = serde_json::to_vec(&Request {
        version: CONTRACT_VERSION,
        operation,
        prompt_hash: hex::encode(Sha256::digest(prompt.as_bytes())),
        width,
        height,
        count,
        mime_type,
        deadline_ms,
        input_hashes,
        mask_hash,
        workspace_hash,
    })
    .map_err(|_| ImageValidationError::DecodeFailed)?;
    Ok(hex::encode(Sha256::digest(canonical)))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestImageProvider {
        png: Vec<u8>,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        input_image_calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl evohime_model_gateway::providers::ModelProvider for TestImageProvider {
        fn kind(&self) -> evohime_model_gateway::providers::ProviderKind {
            evohime_model_gateway::providers::ProviderKind::Mock
        }

        fn model_name(&self) -> &str {
            "test-image-provider"
        }

        fn base_url(&self) -> &str {
            "mock://image"
        }

        fn image_output_capability(
            &self,
        ) -> Option<evohime_model_gateway::provider_contract::ImageOutputCapability> {
            Some(
                evohime_model_gateway::provider_contract::ImageOutputCapability {
                    schema_version: "image-output-capability/v1".into(),
                    provenance: "core-image-generation-test".into(),
                    capability_epoch: 1,
                    operations: vec![ImageOutputOperation::Generate, ImageOutputOperation::Edit],
                    mime_types: vec!["image/png".into()],
                    privacy_boundary: evohime_model_gateway::PrivacyClass::Restricted,
                    execution_class:
                        evohime_model_gateway::provider_contract::ExecutionClass::Local,
                    max_bytes: 1024,
                    max_width: 1,
                    max_height: 1,
                    max_pixels: 1,
                    max_outputs: 1,
                },
            )
        }

        fn generate_image<'a>(
            &'a self,
            request: evohime_model_gateway::provider_contract::ImageProviderRequest,
        ) -> evohime_model_gateway::ImageOutputFuture<'a> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.input_image_calls.fetch_add(
                request.input_images.len(),
                std::sync::atomic::Ordering::SeqCst,
            );
            Box::pin(async move {
                Ok(vec![evohime_model_gateway::ProviderImageOutput {
                    mime_type: "image/png".into(),
                    bytes: self.png.clone(),
                }])
            })
        }

        fn stream_chat(
            &self,
            _messages: &[evohime_model_gateway::providers::ChatMessage],
        ) -> evohime_model_gateway::providers::TokenStream {
            Box::pin(futures_util::stream::empty::<
                Result<
                    evohime_model_gateway::tools::ChatStreamItem,
                    evohime_model_gateway::providers::ProviderError,
                >,
            >())
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn generated_png_is_decoded_published_and_projected_as_metadata() {
        use base64::Engine;

        let path = std::env::temp_dir().join(format!(
            "evohime-image-publication-{}.db",
            uuid::Uuid::now_v7()
        ));
        let journal = crate::EventJournal::open(&path).expect("journal opens");
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC")
            .expect("test PNG decodes from base64");
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let input_image_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let gateway = std::sync::Arc::new(evohime_model_gateway::ModelGateway::from_provider(
            std::sync::Arc::new(TestImageProvider {
                png: png.clone(),
                calls: calls.clone(),
                input_image_calls: input_image_calls.clone(),
            }),
        ));
        let runtime = ImageGenerationRuntime::new(journal.clone(), gateway.clone());
        let projection = runtime
            .start(
                "client-image-test",
                ImageGenerationRequest {
                    job_id: "image-job-success".into(),
                    idempotency_key: "image-job-success".into(),
                    operation: ImageOutputOperation::Generate,
                    prompt: "a single pixel".into(),
                    width: 1,
                    height: 1,
                    count: 1,
                    mime_type: "image/png".into(),
                    deadline_ms: None,
                    input_images: Vec::new(),
                    mask_image: None,
                },
            )
            .await
            .expect("image job completes");
        assert_eq!(projection.state, "completed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(
            input_image_calls.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        let metadata = projection.result_json.expect("result metadata exists");
        let artifact = metadata["artifacts"][0].clone();
        let locator = artifact["locator"]
            .as_str()
            .expect("artifact locator exists");
        assert_eq!(artifact["mime_type"], "image/png");
        assert_eq!(artifact["width"], 1);
        assert_eq!(artifact["height"], 1);
        let database = journal.database().lock().await;
        let store =
            evohime_local_storage::domains::workflow::ArtifactStore::new(database.connection());
        let stored = store
            .read_bytes(
                locator,
                "client-image-test",
                &[],
                ARTIFACT_KIND,
                image_now_ms(),
            )
            .expect("published artifact is owner-readable");
        assert_eq!(stored, png);
        assert!(!String::from_utf8_lossy(&projection.snapshot_json).contains("single pixel"));
        drop(database);
        let edited = runtime
            .start(
                "client-image-test",
                ImageGenerationRequest {
                    job_id: "image-job-edit".into(),
                    idempotency_key: "image-job-edit".into(),
                    operation: ImageOutputOperation::Edit,
                    prompt: "make the pixel blue".into(),
                    width: 1,
                    height: 1,
                    count: 1,
                    mime_type: "image/png".into(),
                    deadline_ms: None,
                    input_images: vec![ImageArtifactInput {
                        locator: locator.into(),
                        mime_type: "image/png".into(),
                        artifact_kind: ARTIFACT_KIND.into(),
                    }],
                    mask_image: None,
                },
            )
            .await
            .expect("existing owner artifact is edited");
        assert_eq!(edited.state, "completed");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(
            input_image_calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let content_hash = artifact["content_hash"]
            .as_str()
            .expect("artifact content hash exists");
        let database = journal.database().lock().await;
        database
            .connection()
            .execute(
                "UPDATE task_artifacts SET content=?1 WHERE content_hash=?2",
                rusqlite::params![b"corrupt".as_slice(), content_hash],
            )
            .expect("test corrupts the stored artifact");
        drop(database);
        let restarted_runtime = ImageGenerationRuntime::new(journal.clone(), gateway);
        let recovered = restarted_runtime
            .get_job("client-image-test", "image-job-success")
            .await
            .expect("job is read after restart")
            .expect("job remains visible to owner");
        assert_eq!(recovered.state, "failed");
        assert_eq!(
            recovered.error_code.as_deref(),
            Some("published_artifact_integrity_failed")
        );
        drop(restarted_runtime);
        drop(runtime);
        drop(journal);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cancellation_before_dispatch_persists_terminal_job_without_provider_call() {
        use base64::Engine;

        let path =
            std::env::temp_dir().join(format!("evohime-image-cancel-{}.db", uuid::Uuid::now_v7()));
        let journal = crate::EventJournal::open(&path).expect("journal opens");
        let png = base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADUlEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC")
            .expect("test PNG decodes from base64");
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let runtime = ImageGenerationRuntime::new(
            journal.clone(),
            std::sync::Arc::new(evohime_model_gateway::ModelGateway::from_provider(
                std::sync::Arc::new(TestImageProvider {
                    png,
                    calls: calls.clone(),
                    input_image_calls: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                }),
            )),
        );
        runtime
            .reserve_job("image-job-cancel", "client-image-test")
            .expect("job reservation succeeds");
        assert!(runtime
            .cancel_job("client-image-test", "image-job-cancel")
            .await
            .expect("early cancellation succeeds"));
        let projection = runtime
            .start_with_permit(
                "client-image-test",
                ImageGenerationRequest {
                    job_id: "image-job-cancel".into(),
                    idempotency_key: "image-job-cancel".into(),
                    operation: ImageOutputOperation::Generate,
                    prompt: "cancel this request".into(),
                    width: 1,
                    height: 1,
                    count: 1,
                    mime_type: "image/png".into(),
                    deadline_ms: None,
                    input_images: Vec::new(),
                    mask_image: None,
                },
                runtime.try_reserve_slot().expect("worker slot available"),
            )
            .await
            .expect("cancelled job remains queryable");
        assert_eq!(projection.state, "cancelled");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        drop(runtime);
        drop(journal);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn early_cancel_is_retained_and_worker_admission_is_bounded() {
        let path = std::env::temp_dir().join(format!(
            "evohime-image-generation-{}.db",
            uuid::Uuid::now_v7()
        ));
        let journal = crate::EventJournal::open(&path).expect("journal opens");
        let runtime = ImageGenerationRuntime::new(
            journal,
            std::sync::Arc::new(evohime_model_gateway::mock_gateway(Vec::new())),
        );
        runtime
            .reserve_job("image-job-1", "client-1")
            .expect("job reserved");
        assert!(!runtime
            .cancel_job("other-client", "image-job-1")
            .await
            .expect("other client cannot cancel"));
        assert!(runtime
            .cancel_job("client-1", "image-job-1")
            .await
            .expect("early cancel"));
        assert!(runtime.active.lock().expect("active lock")["image-job-1"]
            .cancellation
            .is_cancelled());
        let permits = (0..4)
            .map(|_| runtime.try_reserve_slot().expect("within capacity"))
            .collect::<Vec<_>>();
        assert!(runtime.try_reserve_slot().is_err());
        drop(permits);
        assert!(runtime.try_reserve_slot().is_ok());
        drop(runtime);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn request_hash_is_stable_and_does_not_include_raw_prompt_in_metadata() {
        let inputs = vec!["a".repeat(64)];
        let first = request_hash(
            ImageOutputOperation::Edit,
            "draw a blue square",
            64,
            64,
            1,
            "image/png",
            MAX_PROVIDER_DEADLINE_MS,
            &inputs,
            None,
            Some(&"c".repeat(64)),
        )
        .expect("valid image request");
        let second = request_hash(
            ImageOutputOperation::Edit,
            "draw a blue square",
            64,
            64,
            1,
            "image/png",
            MAX_PROVIDER_DEADLINE_MS,
            &inputs,
            None,
            Some(&"c".repeat(64)),
        )
        .expect("valid image request");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn request_hash_enforces_operation_input_and_mask_invariants() {
        assert!(request_hash(
            ImageOutputOperation::Generate,
            "make an image",
            32,
            32,
            1,
            "image/png",
            MAX_PROVIDER_DEADLINE_MS,
            &["a".repeat(64)],
            None,
            None,
        )
        .is_err());
        assert!(request_hash(
            ImageOutputOperation::MaskEdit,
            "edit this",
            32,
            32,
            1,
            "image/png",
            MAX_PROVIDER_DEADLINE_MS,
            &["a".repeat(64)],
            None,
            None,
        )
        .is_err());
        assert!(request_hash(
            ImageOutputOperation::Generate,
            "make an image",
            32,
            32,
            1,
            "image/png",
            0,
            &[],
            None,
            None,
        )
        .is_err());
    }

    #[test]
    fn unsupported_or_mismatched_mime_and_raw_bytes_fail_closed() {
        assert!(matches!(
            validate_output("image/jpeg", b"\x89PNG\r\n\x1a\n".to_vec()),
            Err(ImageValidationError::InvalidSignature)
        ));
        assert!(matches!(
            validate_output("image/webp", b"RIFF0000WEBP".to_vec()),
            Err(ImageValidationError::MimeMismatch)
        ));
        assert!(matches!(
            validate_output("image/png", vec![0; MAX_OUTPUT_BYTES + 1]),
            Err(ImageValidationError::LimitExceeded)
        ));
    }
}

/// Reference to an existing Core-owned image artifact.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageArtifactInput {
    /// ArtifactStore locator; no filesystem paths or remote URLs are accepted.
    pub locator: String,
    /// Caller-declared MIME, checked against decoded bytes.
    pub mime_type: String,
    /// Artifact hash domain known to an existing Core producer.
    pub artifact_kind: String,
}

/// Ephemeral Core image operation request; its prompt is never logged or persisted.
#[derive(serde::Deserialize)]
pub struct ImageGenerationRequest {
    /// Caller-generated stable job id returned immediately by authenticated IPC.
    #[serde(default)]
    pub job_id: String,
    /// Task-scoped idempotency key.
    pub idempotency_key: String,
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
    /// Optional bounded provider deadline; defaults to the Core maximum.
    #[serde(default)]
    pub deadline_ms: Option<u64>,
    /// Existing images; Generate requires none, Edit/MaskEdit require one.
    pub input_images: Vec<ImageArtifactInput>,
    /// Optional mask image; required exactly for MaskEdit.
    pub mask_image: Option<ImageArtifactInput>,
}

/// Bounded Core projection for image capability inspection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageCapabilityProjection {
    /// Typed availability state.
    pub state: String,
    /// Stable reason when no compatible capability exists.
    pub reason_code: String,
    /// Non-secret adapter capability provenance.
    pub provenance: String,
    /// Core image contract version.
    pub contract_version: String,
    /// Provider capability epoch, when available.
    pub capability_epoch: u64,
    /// Supported operation names.
    pub operations: Vec<String>,
    /// Supported MIME types.
    pub mime_types: Vec<String>,
    /// Stable bounded image limits.
    pub max_bytes: u64,
    /// Stable maximum dimension.
    pub max_dimension: u32,
    /// Stable maximum decoded pixel count.
    pub max_pixels: u64,
    /// Whether this build has a native decoder for accepted output formats.
    pub decoder_available: bool,
}

impl ImageCapabilityProjection {
    /// Builds a bounded unavailable capability projection for gateway recovery states.
    pub fn unavailable(state: &str, reason_code: &str) -> Self {
        Self {
            state: state.into(),
            reason_code: reason_code.into(),
            provenance: String::new(),
            contract_version: CONTRACT_VERSION.into(),
            capability_epoch: 0,
            operations: Vec::new(),
            mime_types: Vec::new(),
            max_bytes: MAX_OUTPUT_BYTES as u64,
            max_dimension: MAX_DIMENSION,
            max_pixels: MAX_PIXELS,
            decoder_available: cfg!(windows),
        }
    }
}

/// Result of a Core image operation, containing metadata and ArtifactStore references only.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageJobProjection {
    /// Core-generated job identifier.
    pub job_id: String,
    /// Durable lifecycle state.
    pub state: String,
    /// Current optimistic revision.
    pub revision: u64,
    /// Request content digest; prompt text is never returned.
    pub request_hash: String,
    /// Safe metadata snapshot.
    pub snapshot_json: Vec<u8>,
    /// Artifact references and verified dimensions, when complete.
    pub result_json: Option<serde_json::Value>,
    /// Stable reason code only.
    pub error_code: Option<String>,
}

/// Errors from image-generation orchestration.
#[derive(Debug, thiserror::Error)]
pub enum ImageGenerationError {
    /// Request bounds or artifact references are invalid.
    #[error("invalid image request: {0}")]
    InvalidRequest(&'static str),
    /// No provider route implements the requested image operation.
    #[error("image generation unsupported: {0}")]
    Unsupported(&'static str),
    /// Durable metadata could not be read or written.
    #[error("image job persistence failed")]
    Persistence,
    /// Provider request ended with an ambiguous remote outcome.
    #[error("image provider outcome unknown")]
    UnknownOutcome,
    /// Validated output could not be published.
    #[error("image artifact publication failed")]
    ArtifactPublication,
}

/// Core-owned image job runtime using ModelGateway and existing local owners.
pub struct ImageGenerationRuntime {
    journal: crate::EventJournal,
    gateway: std::sync::Arc<evohime_model_gateway::ModelGateway>,
    route_id: String,
    active: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, ActiveImageJob>>>,
    verified_artifacts: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<(String, u64)>>>,
    slots: std::sync::Arc<tokio::sync::Semaphore>,
}

impl ImageGenerationRuntime {
    /// Reconciles durable image jobs independently of provider configuration.
    pub async fn recover_durable_jobs(
        journal: &crate::EventJournal,
    ) -> Result<usize, ImageGenerationError> {
        let database = journal.database().lock().await;
        evohime_local_storage::domains::image_generation::ImageGenerationStore::new(
            database.connection(),
        )
        .recover_after_restart(image_now_ms())
        .map_err(|_| ImageGenerationError::Persistence)
    }

    /// Creates an image runtime for the gateway's Core-selected default route.
    pub fn new(
        journal: crate::EventJournal,
        gateway: std::sync::Arc<evohime_model_gateway::ModelGateway>,
    ) -> Self {
        Self {
            journal,
            route_id: gateway.default_route_id().to_owned(),
            gateway,
            active: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            verified_artifacts: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            slots: std::sync::Arc::new(tokio::sync::Semaphore::new(4)),
        }
    }

    /// Returns capability metadata without exposing provider endpoints or credentials.
    pub fn capability(&self) -> ImageCapabilityProjection {
        let capability = match self
            .gateway
            .image_output_capability_for_route(&self.route_id)
        {
            Ok(Some(capability)) => capability,
            Ok(None) => {
                return ImageCapabilityProjection::unavailable(
                    "unsupported",
                    "no_image_output_adapter",
                );
            }
            Err(evohime_model_gateway::providers::ProviderError::Config(reason))
                if reason == "invalid_image_output_capability" =>
            {
                return ImageCapabilityProjection::unavailable(
                    "stale",
                    "invalid_image_output_capability",
                );
            }
            Err(_) => {
                return ImageCapabilityProjection::unavailable("unknown", "route_unavailable");
            }
        };
        let operations = capability
            .operations
            .iter()
            .map(|operation| match operation {
                ImageOutputOperation::Generate => "generate",
                ImageOutputOperation::Edit => "edit",
                ImageOutputOperation::MaskEdit => "mask_edit",
            })
            .map(str::to_owned)
            .collect();
        ImageCapabilityProjection {
            state: "supported".into(),
            reason_code: String::new(),
            provenance: capability.provenance,
            contract_version: CONTRACT_VERSION.into(),
            capability_epoch: capability.capability_epoch,
            operations,
            mime_types: capability.mime_types,
            max_bytes: capability.max_bytes.min(MAX_TOTAL_OUTPUT_BYTES as u64),
            max_dimension: capability
                .max_width
                .min(capability.max_height)
                .min(MAX_DIMENSION),
            max_pixels: capability.max_pixels.min(MAX_PIXELS),
            decoder_available: cfg!(windows),
        }
    }

    /// Rejects requests that cannot be admitted before IPC reports them accepted.
    pub fn preflight_request(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<(), ImageGenerationError> {
        if !cfg!(windows) {
            return Err(ImageGenerationError::Unsupported(
                "image_decoder_unavailable",
            ));
        }
        let capability = self
            .gateway
            .image_output_capability_for_route(&self.route_id)
            .map_err(|_| ImageGenerationError::Unsupported("route_unavailable"))?
            .ok_or(ImageGenerationError::Unsupported("no_image_output_adapter"))?;
        if !capability.operations.contains(&request.operation)
            || !capability.mime_types.contains(&request.mime_type)
            || request.width == 0
            || request.height == 0
            || request.width > capability.max_width.min(MAX_DIMENSION)
            || request.height > capability.max_height.min(MAX_DIMENSION)
            || u64::from(request.width).saturating_mul(u64::from(request.height))
                > capability.max_pixels.min(MAX_PIXELS)
            || request.count == 0
            || request.count > capability.max_outputs.min(MAX_OUTPUT_COUNT)
        {
            return Err(ImageGenerationError::Unsupported(
                "operation_or_bounds_unavailable",
            ));
        }
        if request.idempotency_key != request.job_id
            || request.job_id.is_empty()
            || request.job_id.len() > 128
            || !request
                .job_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ImageGenerationError::InvalidRequest("job_id_invalid"));
        }
        if request
            .deadline_ms
            .is_some_and(|deadline| !(1..=MAX_PROVIDER_DEADLINE_MS).contains(&deadline))
        {
            return Err(ImageGenerationError::InvalidRequest("deadline_limit"));
        }
        collect_input_specs(request)?;
        Ok(())
    }

    /// Reconciles interrupted image jobs before accepting IPC requests.
    pub async fn recover_after_restart(&self) -> Result<usize, ImageGenerationError> {
        Self::recover_durable_jobs(&self.journal).await
    }

    /// Returns an image job only to the authenticated client that created it.
    pub async fn get_job(
        &self,
        client_id: &str,
        job_id: &str,
    ) -> Result<Option<ImageJobProjection>, ImageGenerationError> {
        let database = self.journal.database().lock().await;
        let mut record =
            evohime_local_storage::domains::image_generation::ImageGenerationStore::new(
                database.connection(),
            )
            .get(job_id)
            .map_err(|_| ImageGenerationError::Persistence)?;
        let Some(mut record) = record.take().filter(|record| record.task_id == client_id) else {
            return Ok(None);
        };
        if record.state == evohime_local_storage::image_generation_store::ImageJobState::Completed {
            let cache_key = (job_id.to_owned(), record.revision);
            let already_verified = self
                .verified_artifacts
                .lock()
                .map_err(|_| ImageGenerationError::Persistence)?
                .contains(&cache_key);
            if !already_verified {
                let store = evohime_local_storage::domains::workflow::ArtifactStore::new(
                    database.connection(),
                );
                if verify_published_artifacts(&store, client_id, record.result_json.as_deref()) {
                    let mut verified = self
                        .verified_artifacts
                        .lock()
                        .map_err(|_| ImageGenerationError::Persistence)?;
                    if verified.len() >= 256 {
                        verified.clear();
                    }
                    verified.insert(cache_key);
                } else {
                    let transitioned = evohime_local_storage::domains::image_generation::ImageGenerationStore::new(
                        database.connection(),
                    )
                    .transition(
                        job_id,
                        record.revision,
                        evohime_local_storage::image_generation_store::ImageJobState::Failed,
                        &record.snapshot_json,
                        None,
                        Some("published_artifact_integrity_failed"),
                        image_now_ms(),
                    )
                    .map_err(|_| ImageGenerationError::Persistence)?;
                    if transitioned {
                        record.state =
                            evohime_local_storage::image_generation_store::ImageJobState::Failed;
                        record.revision = record.revision.saturating_add(1);
                        record.result_json = None;
                        record.error_code = Some("published_artifact_integrity_failed".into());
                    } else {
                        return Err(ImageGenerationError::Persistence);
                    }
                }
            }
        }
        Ok(Some(project_record(record)))
    }

    /// Cancels a job only before its provider dispatch marker is durable.
    pub async fn cancel_job(
        &self,
        client_id: &str,
        job_id: &str,
    ) -> Result<bool, ImageGenerationError> {
        let record = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::domains::image_generation::ImageGenerationStore::new(
                database.connection(),
            )
            .get(job_id)
            .map_err(|_| ImageGenerationError::Persistence)?
        };
        let Some(record) = record else {
            if let Ok(active) = self.active.lock() {
                if let Some(job) = active.get(job_id).filter(|job| job.client_id == client_id) {
                    job.cancellation.cancel();
                    return Ok(true);
                }
            }
            return Ok(false);
        };
        if record.task_id != client_id {
            return Ok(false);
        }
        if !matches!(
            record.state,
            evohime_local_storage::image_generation_store::ImageJobState::Preflight
                | evohime_local_storage::image_generation_store::ImageJobState::Queued
        ) {
            return Ok(false);
        }
        if !transition_job(
            &self.journal,
            job_id,
            record.revision,
            evohime_local_storage::image_generation_store::ImageJobState::Cancelled,
            &record.snapshot_json,
            None,
            Some("cancelled_before_dispatch"),
            image_now_ms(),
        )
        .await?
        {
            return Ok(false);
        }
        if let Ok(active) = self.active.lock() {
            if let Some(job) = active.get(job_id).filter(|job| job.client_id == client_id) {
                job.cancellation.cancel();
            }
        }
        Ok(true)
    }

    async fn load_input_artifacts(
        &self,
        client_id: &str,
        inputs: &[&ImageArtifactInput],
        mask: Option<&ImageArtifactInput>,
    ) -> Result<Vec<Vec<u8>>, ImageGenerationError> {
        let mut ordered = inputs.to_vec();
        if let Some(mask) = mask {
            ordered.push(mask);
        }
        let mut decoded_inputs = Vec::with_capacity(ordered.len());
        let mut total_input_bytes = 0usize;
        for input in ordered {
            let expected_hash = artifact_locator_hash(&input.locator)?;
            if input.locator.len() > 512
                || !matches!(
                    input.artifact_kind.as_str(),
                    "generated_image" | "image_input" | "browser_screenshot"
                )
            {
                return Err(ImageGenerationError::InvalidRequest(
                    "input_artifact_ref_invalid",
                ));
            }
            let encoded = {
                let database = self.journal.database().lock().await;
                let store = evohime_local_storage::domains::workflow::ArtifactStore::new(
                    database.connection(),
                );
                let reference = store
                    .get_ref(&input.locator)
                    .map_err(|_| ImageGenerationError::Persistence)?
                    .ok_or(ImageGenerationError::InvalidRequest(
                        "input_artifact_missing",
                    ))?;
                if reference.task_id != client_id
                    || reference.privacy != evohime_context_budget::item::Privacy::Workspace
                    || reference.content_hash != expected_hash
                {
                    return Err(ImageGenerationError::InvalidRequest(
                        "input_artifact_owner_or_hash",
                    ));
                }
                store
                    .read_bytes_bounded(
                        &input.locator,
                        client_id,
                        &[],
                        &input.artifact_kind,
                        image_now_ms(),
                        MAX_OUTPUT_BYTES as u64,
                    )
                    .map_err(|_| {
                        ImageGenerationError::InvalidRequest("input_artifact_unreadable")
                    })?
            };
            total_input_bytes = total_input_bytes.saturating_add(encoded.len());
            if total_input_bytes > MAX_INPUT_BYTES {
                return Err(ImageGenerationError::InvalidRequest("input_bytes_limit"));
            }
            let mime = input.mime_type.clone();
            let image = tokio::task::spawn_blocking(move || validate_output(&mime, encoded))
                .await
                .map_err(|_| ImageGenerationError::InvalidRequest("input_decode_failed"))?
                .map_err(|_| ImageGenerationError::InvalidRequest("input_decode_failed"))?;
            decoded_inputs.push(image.bytes);
        }
        Ok(decoded_inputs)
    }

    /// Runs one validated generation/edit request and persists only bounded metadata.
    pub async fn start(
        &self,
        client_id: &str,
        request: ImageGenerationRequest,
    ) -> Result<ImageJobProjection, ImageGenerationError> {
        let permit = self
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ImageGenerationError::Persistence)?;
        self.start_with_permit(client_id, request, permit).await
    }

    /// Reserves one bounded worker slot without queueing unbounded detached jobs.
    pub fn try_reserve_slot(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, ImageGenerationError> {
        self.slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| ImageGenerationError::Unsupported("image_job_capacity_reached"))
    }

    /// Reserves an early cancellation token for a job before its durable row exists.
    pub fn reserve_job(&self, job_id: &str, client_id: &str) -> Result<(), ImageGenerationError> {
        if job_id.is_empty()
            || job_id.len() > 128
            || client_id.is_empty()
            || client_id.len() > 128
            || !job_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ImageGenerationError::InvalidRequest("job_id_invalid"));
        }
        let mut active = self
            .active
            .lock()
            .map_err(|_| ImageGenerationError::Persistence)?;
        if let Some(existing) = active.get(job_id) {
            return if existing.client_id == client_id {
                Err(ImageGenerationError::InvalidRequest("job_already_active"))
            } else {
                Err(ImageGenerationError::InvalidRequest(
                    "job_id_owned_by_other_client",
                ))
            };
        }
        active.insert(
            job_id.to_owned(),
            ActiveImageJob {
                client_id: client_id.to_owned(),
                cancellation: tokio_util::sync::CancellationToken::new(),
            },
        );
        Ok(())
    }

    /// Executes a previously admitted image request while holding its bounded slot.
    pub async fn start_with_permit(
        &self,
        client_id: &str,
        request: ImageGenerationRequest,
        permit: tokio::sync::OwnedSemaphorePermit,
    ) -> Result<ImageJobProjection, ImageGenerationError> {
        let job_id = request.job_id.clone();
        if job_id.is_empty()
            || job_id.len() > 128
            || request.idempotency_key != job_id
            || !job_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(ImageGenerationError::InvalidRequest("job_id_invalid"));
        }
        let token = {
            let mut active = self
                .active
                .lock()
                .map_err(|_| ImageGenerationError::Persistence)?;
            if active
                .get(&job_id)
                .is_some_and(|job| job.client_id != client_id)
            {
                return Err(ImageGenerationError::InvalidRequest(
                    "job_id_owned_by_other_client",
                ));
            }
            active
                .entry(job_id.clone())
                .or_insert_with(|| ActiveImageJob {
                    client_id: client_id.to_owned(),
                    cancellation: tokio_util::sync::CancellationToken::new(),
                })
                .cancellation
                .clone()
        };
        let _active_guard = ActiveImageJobGuard {
            active: self.active.clone(),
            job_id: job_id.clone(),
        };
        let capability = self
            .gateway
            .image_output_capability_for_route(&self.route_id)
            .map_err(|_| ImageGenerationError::Unsupported("route_unavailable"))?
            .ok_or(ImageGenerationError::Unsupported("no_image_output_adapter"))?;
        if !capability.operations.contains(&request.operation)
            || !capability.mime_types.contains(&request.mime_type)
            || request.width > capability.max_width
            || request.height > capability.max_height
            || request.count > capability.max_outputs
        {
            return Err(ImageGenerationError::Unsupported(
                "operation_or_bounds_unavailable",
            ));
        }
        if !cfg!(windows) {
            return Err(ImageGenerationError::Unsupported(
                "image_decoder_unavailable",
            ));
        }
        if client_id.is_empty() || client_id.len() > 128 {
            return Err(ImageGenerationError::InvalidRequest("client_id_limit"));
        }
        let input_specs = collect_input_specs(&request)?;
        let mut input_hashes = input_specs
            .iter()
            .map(|input| artifact_locator_hash(&input.locator))
            .collect::<Result<Vec<_>, _>>()?;
        let mask_index = request.mask_image.as_ref().map(|_| input_hashes.len());
        if let Some(mask) = request.mask_image.as_ref() {
            input_hashes.push(artifact_locator_hash(&mask.locator)?);
        }
        let mask_hash = mask_index.map(|index| input_hashes[index].as_str());
        let request_hash = request_hash(
            request.operation,
            &request.prompt,
            request.width,
            request.height,
            request.count,
            &request.mime_type,
            request.deadline_ms.unwrap_or(MAX_PROVIDER_DEADLINE_MS),
            &input_hashes[..mask_index.unwrap_or(input_hashes.len())],
            mask_hash,
            None,
        )
        .map_err(|_| ImageGenerationError::InvalidRequest("request_bounds"))?;
        let now_ms = image_now_ms();
        let snapshot_json = serde_json::to_vec(&serde_json::json!({
            "contract_version": CONTRACT_VERSION,
            "operation": request.operation,
            "request_hash": request_hash,
            "route_id": self.route_id,
            "capability_epoch": capability.capability_epoch,
            "capability_snapshot": {
                "provenance": capability.provenance,
                "operations": capability.operations,
                "mime_types": capability.mime_types,
                "max_bytes": capability.max_bytes.min(MAX_TOTAL_OUTPUT_BYTES as u64),
                "max_width": capability.max_width.min(MAX_DIMENSION),
                "max_height": capability.max_height.min(MAX_DIMENSION),
                "max_pixels": capability.max_pixels.min(MAX_PIXELS),
                "max_outputs": capability.max_outputs.min(MAX_OUTPUT_COUNT),
                "privacy_boundary": capability.privacy_boundary,
                "execution_class": capability.execution_class,
            },
            "policy_hash": hex::encode(Sha256::digest(b"core-image-policy/v1|restricted|cloud=false|artifact-export=explicit")),
            "width": request.width,
            "height": request.height,
            "count": request.count,
            "mime_type": request.mime_type,
            "deadline_ms": request.deadline_ms.unwrap_or(MAX_PROVIDER_DEADLINE_MS),
            "input_hashes": input_hashes,
            "allow_cloud": false,
            "privacy": "restricted"
        }))
        .map_err(|_| ImageGenerationError::Persistence)?;
        let record = evohime_local_storage::image_generation_store::ImageJobRecord {
            job_id: job_id.clone(),
            task_id: client_id.into(),
            idempotency_key: request.idempotency_key.clone(),
            request_hash: request_hash.clone(),
            state: evohime_local_storage::image_generation_store::ImageJobState::Preflight,
            revision: 1,
            snapshot_json,
            result_json: None,
            error_code: None,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        let inserted = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::domains::image_generation::ImageGenerationStore::new(
                database.connection(),
            )
            .insert_preflight(&record)
            .map_err(|_| ImageGenerationError::Persistence)?
        };
        let record = match inserted {
            evohime_local_storage::image_generation_store::InsertImageJobOutcome::Created(
                record,
            ) => record,
            evohime_local_storage::image_generation_store::InsertImageJobOutcome::Existing(
                record,
            ) => {
                drop(permit);
                return Ok(project_record(record));
            }
        };
        if !transition_job(
            &self.journal,
            &job_id,
            record.revision,
            evohime_local_storage::image_generation_store::ImageJobState::Queued,
            &record.snapshot_json,
            None,
            None,
            image_now_ms(),
        )
        .await?
        {
            drop(permit);
            return Err(ImageGenerationError::Persistence);
        }
        if token.is_cancelled() {
            let _ = transition_job(
                &self.journal,
                &job_id,
                record.revision + 1,
                evohime_local_storage::image_generation_store::ImageJobState::Cancelled,
                &record.snapshot_json,
                None,
                Some("cancelled_before_dispatch"),
                image_now_ms(),
            )
            .await?;
            drop(permit);
            return self
                .get_job(client_id, &job_id)
                .await?
                .ok_or(ImageGenerationError::Persistence);
        }
        let input_bytes = match self
            .load_input_artifacts(client_id, &input_specs, request.mask_image.as_ref())
            .await
        {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = transition_job(
                    &self.journal,
                    &job_id,
                    record.revision + 1,
                    evohime_local_storage::image_generation_store::ImageJobState::Failed,
                    &record.snapshot_json,
                    None,
                    Some(error.reason_code()),
                    image_now_ms(),
                )
                .await?;
                drop(permit);
                return Err(error);
            }
        };
        let provider_request = evohime_model_gateway::ImageProviderRequest {
            operation: request.operation,
            prompt: request.prompt,
            width: request.width,
            height: request.height,
            count: request.count,
            mime_type: request.mime_type.clone(),
            required_privacy: evohime_model_gateway::PrivacyClass::Restricted,
            allow_cloud: false,
            input_images: input_bytes[..mask_index.unwrap_or(input_bytes.len())].to_vec(),
            mask_image: mask_index.and_then(|index| input_bytes.get(index).cloned()),
        };
        if let Err(error) = self.gateway.preflight_image_output_for_route(
            &self.route_id,
            Some(capability.capability_epoch),
            &provider_request,
        ) {
            let reason = if matches!(
                error,
                evohime_model_gateway::providers::ProviderError::ImageCapabilityStale
            ) {
                "image_capability_stale"
            } else {
                "image_route_preflight_rejected"
            };
            let _ = transition_job(
                &self.journal,
                &job_id,
                record.revision + 1,
                evohime_local_storage::image_generation_store::ImageJobState::Failed,
                &record.snapshot_json,
                None,
                Some(reason),
                image_now_ms(),
            )
            .await?;
            drop(permit);
            return Err(ImageGenerationError::Unsupported(reason));
        }
        if !transition_job(
            &self.journal,
            &job_id,
            record.revision + 1,
            evohime_local_storage::image_generation_store::ImageJobState::Dispatched,
            &record.snapshot_json,
            None,
            None,
            image_now_ms(),
        )
        .await?
        {
            drop(permit);
            return Err(ImageGenerationError::Persistence);
        }
        let provider_outputs = tokio::time::timeout(
            std::time::Duration::from_millis(
                request.deadline_ms.unwrap_or(MAX_PROVIDER_DEADLINE_MS),
            ),
            self.gateway.generate_image_for_route_at_epoch(
                &self.route_id,
                Some(capability.capability_epoch),
                provider_request,
            ),
        )
        .await;
        let provider_outputs = match provider_outputs {
            Ok(Ok(outputs)) if !token.is_cancelled() => outputs,
            Ok(Err(
                evohime_model_gateway::providers::ProviderError::ImageCapabilityStale
                | evohime_model_gateway::providers::ProviderError::ImagePreflightRejected,
            )) => {
                let _ = transition_job(
                    &self.journal,
                    &job_id,
                    record.revision + 2,
                    evohime_local_storage::image_generation_store::ImageJobState::Failed,
                    &record.snapshot_json,
                    None,
                    Some("image_capability_or_route_stale"),
                    image_now_ms(),
                )
                .await?;
                drop(permit);
                return Err(ImageGenerationError::Unsupported(
                    "image_capability_or_route_stale",
                ));
            }
            _ => {
                let _ = transition_job(
                    &self.journal,
                    &job_id,
                    record.revision + 2,
                    evohime_local_storage::image_generation_store::ImageJobState::UnknownOutcome,
                    &record.snapshot_json,
                    None,
                    Some("provider_outcome_unknown"),
                    image_now_ms(),
                )
                .await?;
                drop(permit);
                return Err(ImageGenerationError::UnknownOutcome);
            }
        };
        if provider_outputs.len() != usize::from(request.count) {
            let _ = transition_job(
                &self.journal,
                &job_id,
                record.revision + 2,
                evohime_local_storage::image_generation_store::ImageJobState::Failed,
                &record.snapshot_json,
                None,
                Some("provider_output_count_mismatch"),
                image_now_ms(),
            )
            .await?;
            drop(permit);
            return Err(ImageGenerationError::InvalidRequest(
                "provider_output_count_mismatch",
            ));
        }
        let mut validated = Vec::with_capacity(provider_outputs.len());
        let mut total_output_bytes = 0usize;
        for output in provider_outputs {
            total_output_bytes = total_output_bytes.saturating_add(output.bytes.len());
            if output.bytes.len() as u64 > capability.max_bytes.min(MAX_OUTPUT_BYTES as u64)
                || total_output_bytes
                    > capability.max_bytes.min(MAX_TOTAL_OUTPUT_BYTES as u64) as usize
            {
                let _ = transition_job(
                    &self.journal,
                    &job_id,
                    record.revision + 2,
                    evohime_local_storage::image_generation_store::ImageJobState::Failed,
                    &record.snapshot_json,
                    None,
                    Some("provider_output_bytes_limit"),
                    image_now_ms(),
                )
                .await?;
                drop(permit);
                return Err(ImageGenerationError::InvalidRequest(
                    "provider_output_bytes_limit",
                ));
            }
            let requested_mime = request.mime_type.clone();
            let decoded = tokio::task::spawn_blocking(move || {
                validate_output(&output.mime_type, output.bytes)
            })
            .await;
            let validated_image = match decoded {
                Ok(Ok(image)) => image,
                _ => {
                    let _ = transition_job(
                        &self.journal,
                        &job_id,
                        record.revision + 2,
                        evohime_local_storage::image_generation_store::ImageJobState::Failed,
                        &record.snapshot_json,
                        None,
                        Some("provider_output_decode_failed"),
                        image_now_ms(),
                    )
                    .await?;
                    drop(permit);
                    return Err(ImageGenerationError::InvalidRequest(
                        "provider_output_decode_failed",
                    ));
                }
            };
            if validated_image.mime_type.as_str() != requested_mime
                || validated_image.width != request.width
                || validated_image.height != request.height
            {
                let _ = transition_job(
                    &self.journal,
                    &job_id,
                    record.revision + 2,
                    evohime_local_storage::image_generation_store::ImageJobState::Failed,
                    &record.snapshot_json,
                    None,
                    Some("provider_output_validation_failed"),
                    image_now_ms(),
                )
                .await?;
                drop(permit);
                return Err(ImageGenerationError::InvalidRequest(
                    "provider_output_validation_failed",
                ));
            }
            validated.push(validated_image);
        }
        let publication = {
            let database = self.journal.database().lock().await;
            let store =
                evohime_local_storage::domains::workflow::ArtifactStore::new(database.connection());
            let batch = validated
                .iter()
                .map(
                    |image| evohime_local_storage::domains::workflow::BinaryArtifactInput {
                        kind: ARTIFACT_KIND,
                        task_id: client_id,
                        owner_task_id: client_id,
                        content: &image.bytes,
                        privacy: evohime_context_budget::item::Privacy::Workspace,
                    },
                )
                .collect::<Vec<_>>();
            store.offload_bytes_batch(&batch, image_now_ms())
        };
        let output_refs = match publication {
            Ok(references) => references,
            Err(_) => {
                let _ = transition_job(
                    &self.journal,
                    &job_id,
                    record.revision + 2,
                    evohime_local_storage::image_generation_store::ImageJobState::Failed,
                    &record.snapshot_json,
                    None,
                    Some("artifact_publication_failed"),
                    image_now_ms(),
                )
                .await;
                drop(permit);
                return Err(ImageGenerationError::ArtifactPublication);
            }
        };
        let result_json = serde_json::to_vec(&serde_json::json!({
            "artifacts": output_refs.iter().zip(validated.iter()).map(|(reference,image)| serde_json::json!({
                "locator": reference.locator,
                "content_hash": reference.content_hash,
                "mime_type": image.mime_type.as_str(),
                "width": image.width,
                "height": image.height,
                "sha256": image.sha256,
            })).collect::<Vec<_>>(),
        }))
        .map_err(|_| ImageGenerationError::Persistence)?;
        if !transition_job(
            &self.journal,
            &job_id,
            record.revision + 2,
            evohime_local_storage::image_generation_store::ImageJobState::Completed,
            &record.snapshot_json,
            Some(&result_json),
            None,
            image_now_ms(),
        )
        .await?
        {
            drop(permit);
            return Err(ImageGenerationError::Persistence);
        }
        drop(permit);
        self.get_job(client_id, &job_id)
            .await?
            .ok_or(ImageGenerationError::Persistence)
    }
}

fn verify_published_artifacts(
    store: &evohime_local_storage::domains::workflow::ArtifactStore<'_>,
    owner_id: &str,
    result_json: Option<&[u8]>,
) -> bool {
    let Some(artifacts) = result_json
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        .and_then(|result| result.get("artifacts").cloned())
        .and_then(|artifacts| artifacts.as_array().cloned())
        .filter(|artifacts| !artifacts.is_empty() && artifacts.len() <= MAX_OUTPUT_COUNT as usize)
    else {
        return false;
    };
    for artifact in artifacts {
        let Some(locator) = artifact.get("locator").and_then(serde_json::Value::as_str) else {
            return false;
        };
        let Some(expected_content_hash) = artifact
            .get("content_hash")
            .and_then(serde_json::Value::as_str)
        else {
            return false;
        };
        let Some(expected_sha256) = artifact.get("sha256").and_then(serde_json::Value::as_str)
        else {
            return false;
        };
        let Ok(Some(reference)) = store.get_ref(locator) else {
            return false;
        };
        if reference.task_id != owner_id || reference.content_hash != expected_content_hash {
            return false;
        }
        let Ok(bytes) = store.read_bytes_bounded(
            locator,
            owner_id,
            &[],
            ARTIFACT_KIND,
            image_now_ms(),
            MAX_OUTPUT_BYTES as u64,
        ) else {
            return false;
        };
        if hex::encode(Sha256::digest(bytes)) != expected_sha256 {
            return false;
        }
    }
    true
}

fn project_record(
    record: evohime_local_storage::image_generation_store::ImageJobRecord,
) -> ImageJobProjection {
    ImageJobProjection {
        job_id: record.job_id,
        state: record.state.as_str().into(),
        revision: record.revision,
        request_hash: record.request_hash,
        snapshot_json: record.snapshot_json,
        result_json: record
            .result_json
            .as_deref()
            .and_then(|bytes| serde_json::from_slice(bytes).ok()),
        error_code: record.error_code,
    }
}

fn collect_input_specs(
    request: &ImageGenerationRequest,
) -> Result<Vec<&ImageArtifactInput>, ImageGenerationError> {
    if request.idempotency_key.trim().is_empty()
        || request.idempotency_key.len() > 128
        || request.prompt.trim().is_empty()
        || request.prompt.len() > MAX_PROMPT_BYTES
        || request.width == 0
        || request.height == 0
        || request.width > MAX_DIMENSION
        || request.height > MAX_DIMENSION
        || u64::from(request.width).saturating_mul(u64::from(request.height)) > MAX_PIXELS
        || request.count == 0
        || request.count > MAX_OUTPUT_COUNT
        || ImageMimeType::parse(&request.mime_type).is_none()
        || request.input_images.len() > MAX_INPUT_IMAGES
    {
        return Err(ImageGenerationError::InvalidRequest("request_bounds"));
    }
    let valid_inputs = match request.operation {
        ImageOutputOperation::Generate => {
            request.input_images.is_empty() && request.mask_image.is_none()
        }
        ImageOutputOperation::Edit => {
            request.input_images.len() == 1 && request.mask_image.is_none()
        }
        ImageOutputOperation::MaskEdit => {
            request.input_images.len() == 1 && request.mask_image.is_some()
        }
    };
    if !valid_inputs {
        return Err(ImageGenerationError::InvalidRequest(
            "operation_input_mismatch",
        ));
    }
    for input in request.input_images.iter().chain(request.mask_image.iter()) {
        artifact_locator_hash(&input.locator)?;
    }
    Ok(request.input_images.iter().collect())
}

fn artifact_locator_hash(locator: &str) -> Result<String, ImageGenerationError> {
    let body = locator
        .strip_prefix("artifact://")
        .ok_or(ImageGenerationError::InvalidRequest(
            "input_artifact_ref_invalid",
        ))?;
    let (owner, hash) = body
        .rsplit_once('/')
        .ok_or(ImageGenerationError::InvalidRequest(
            "input_artifact_ref_invalid",
        ))?;
    if owner.trim().is_empty()
        || hash.len() != 64
        || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ImageGenerationError::InvalidRequest(
            "input_artifact_ref_invalid",
        ));
    }
    Ok(hash.to_ascii_lowercase())
}

struct ActiveImageJobGuard {
    active: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, ActiveImageJob>>>,
    job_id: String,
}

struct ActiveImageJob {
    client_id: String,
    cancellation: tokio_util::sync::CancellationToken,
}

impl Drop for ActiveImageJobGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.job_id);
        }
    }
}

async fn transition_job(
    journal: &crate::EventJournal,
    job_id: &str,
    expected_revision: u64,
    state: evohime_local_storage::image_generation_store::ImageJobState,
    snapshot_json: &[u8],
    result_json: Option<&[u8]>,
    error_code: Option<&str>,
    now_ms: i64,
) -> Result<bool, ImageGenerationError> {
    let database = journal.database().lock().await;
    evohime_local_storage::domains::image_generation::ImageGenerationStore::new(
        database.connection(),
    )
    .transition(
        job_id,
        expected_revision,
        state,
        snapshot_json,
        result_json,
        error_code,
        now_ms,
    )
    .map_err(|_| ImageGenerationError::Persistence)
}

impl ImageGenerationError {
    /// Stable redacted error code suitable for authenticated IPC projections.
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(_) => "invalid_request",
            Self::Unsupported(_) => "unsupported",
            Self::Persistence => "storage_error",
            Self::UnknownOutcome => "unknown_outcome",
            Self::ArtifactPublication => "artifact_publication_failed",
        }
    }

    fn reason_code(&self) -> &'static str {
        match self {
            Self::InvalidRequest(reason) | Self::Unsupported(reason) => reason,
            Self::Persistence => "storage_error",
            Self::UnknownOutcome => "provider_outcome_unknown",
            Self::ArtifactPublication => "artifact_publication_failed",
        }
    }
}

fn image_now_ms() -> i64 {
    crate::task_memory::now_millis().min(i64::MAX as u64) as i64
}
