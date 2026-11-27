#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Shared IPC contract, capability descriptors, and bounded frame codec for
//! isolated EvoHime service modules.
//!
//! All extracted Core subsystems (`evals`, `media-voice`, `research`,
//! `remote-a2a`, `ambient`, `computer-use`, `workflow-engine`, `rag-memory`)
//! implement this contract so `evohime-core` can discover, authorize, and invoke
//! them uniformly without compile-time coupling to module internals.

use std::collections::BTreeSet;
use std::io::{self, Read, Write};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Current wire-level contract version for service modules.
pub const SERVICE_CONTRACT_VERSION: u32 = 1;

/// Maximum serialized payload size (in bytes) for a single service frame.
pub const MAX_SERVICE_FRAME_BYTES: usize = 1024 * 1024;

/// Maximum number of capabilities a single service module may advertise.
pub const MAX_CAPABILITIES_PER_SERVICE: usize = 128;

/// Maximum length of a `service_id` or capability name.
pub const MAX_SERVICE_ID_LEN: usize = 64;

/// Typed error category returned across the Core / service-module boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorCode {
    /// Target service module is not running or cannot be reached.
    Unavailable,
    /// Service invocation exceeded its execution or IPC timeout.
    Timeout,
    /// Caller supplied malformed or out-of-bounds parameters.
    InvalidInput,
    /// Core policy gate denied the requested capability or permission scope.
    PermissionDenied,
    /// Requested capability or resource does not exist on the service.
    NotFound,
    /// Unhandled internal failure inside the service module.
    Internal,
}

/// Structured error returned by a service module or frame codec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{code:?}: {message}")]
pub struct ServiceContractError {
    /// Machine-readable error classification.
    pub code: ServiceErrorCode,
    /// Human-readable diagnostic summary without secret values.
    pub message: String,
}

impl ServiceContractError {
    /// Creates a new [`ServiceContractError`] with the given code and message.
    pub fn new(code: ServiceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Creates an [`ServiceErrorCode::Unavailable`] error.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::Unavailable, message)
    }

    /// Creates an [`ServiceErrorCode::InvalidInput`] error.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::InvalidInput, message)
    }

    /// Creates an [`ServiceErrorCode::PermissionDenied`] error.
    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::PermissionDenied, message)
    }

    /// Creates an [`ServiceErrorCode::NotFound`] error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ServiceErrorCode::NotFound, message)
    }
}

/// Runtime lifecycle status reported by a service module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceHealthStatus {
    /// Service process is initializing and not yet ready for invocations.
    Starting,
    /// Service is healthy and accepting requests.
    Ready,
    /// Service is running with reduced functionality.
    Degraded,
    /// Service is draining in-flight work before exit.
    Stopping,
    /// Service cannot accept requests.
    Unavailable,
}

/// Health snapshot returned by a service module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceHealthReport {
    /// Canonical module identifier (e.g. `evals`, `media-voice`).
    pub service_id: String,
    /// Semantic version of the running module binary.
    pub version: String,
    /// Current lifecycle status.
    pub status: ServiceHealthStatus,
    /// Number of currently active requests inside the module.
    pub active_requests: u32,
    /// Optional status note.
    pub detail: Option<String>,
}

/// Declarative description of one capability exposed by a service module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceCapabilityDescriptor {
    /// Unique capability identifier (e.g. `media.generate_image`).
    pub name: String,
    /// Human-readable description for tool catalog projection.
    pub description: String,
    /// JSON Schema describing expected input payload.
    pub input_schema: serde_json::Value,
    /// Permissions that Core must authorize before dispatching `Invoke`.
    pub required_permissions: Vec<String>,
    /// Whether this capability emits incremental `StreamChunk` responses.
    pub supports_streaming: bool,
}

/// Full capability advertisement returned by `DescribeCapabilities`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceDescriptor {
    /// Canonical module identifier.
    pub service_id: String,
    /// Semantic version of the module binary.
    pub version: String,
    /// Contract version implemented by the module.
    pub contract_version: u32,
    /// Capabilities exposed by this module.
    pub capabilities: Vec<ServiceCapabilityDescriptor>,
}

/// Request sent from `evohime-core` to an isolated service module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceRequest {
    /// Lightweight liveness and contract-version check.
    Ping,
    /// Requests the current [`ServiceHealthReport`].
    Health,
    /// Requests the module's [`ServiceDescriptor`].
    DescribeCapabilities,
    /// Executes one capability after Core permission checks pass.
    Invoke {
        /// Unique correlation identifier for this invocation.
        request_id: String,
        /// Target capability name from [`ServiceDescriptor`].
        capability: String,
        /// Validated JSON input payload.
        payload: serde_json::Value,
        /// Optional Core session identifier for audit correlation.
        caller_session_id: Option<String>,
    },
    /// Requests graceful shutdown of the service module.
    Shutdown,
}

/// Response sent from an isolated service module back to `evohime-core`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceResponse {
    /// Acknowledgement of [`ServiceRequest::Ping`].
    Pong {
        /// Contract version supported by the module.
        contract_version: u32,
    },
    /// Response to [`ServiceRequest::Health`].
    Health(ServiceHealthReport),
    /// Response to [`ServiceRequest::DescribeCapabilities`].
    Capabilities(ServiceDescriptor),
    /// Final result of a non-streaming [`ServiceRequest::Invoke`].
    InvokeResult {
        /// Correlation identifier copied from [`ServiceRequest::Invoke`].
        request_id: String,
        /// JSON output payload.
        payload: serde_json::Value,
    },
    /// Incremental chunk of a streaming [`ServiceRequest::Invoke`].
    StreamChunk {
        /// Correlation identifier copied from [`ServiceRequest::Invoke`].
        request_id: String,
        /// Zero-based monotonic sequence number within this request.
        seq: u64,
        /// JSON chunk payload.
        payload: serde_json::Value,
        /// `true` when this chunk completes the stream.
        is_final: bool,
    },
    /// Error response for a failed request.
    Error {
        /// Optional correlation identifier if the failure belongs to `Invoke`.
        request_id: Option<String>,
        /// Typed error payload.
        error: ServiceContractError,
    },
    /// Acknowledgement of [`ServiceRequest::Shutdown`].
    ShuttingDown,
}

/// Validates that an identifier (`service_id` or capability name) is non-empty,
/// bounded, and uses safe ASCII characters (`a-z`, `0-9`, `-`, `_`, `.`).
pub fn validate_identifier(value: &str, field_name: &str) -> Result<(), ServiceContractError> {
    if value.is_empty() || value.len() > MAX_SERVICE_ID_LEN {
        return Err(ServiceContractError::invalid_input(format!(
            "{field_name} length must be between 1 and {MAX_SERVICE_ID_LEN}"
        )));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(ServiceContractError::invalid_input(format!(
            "{field_name} contains unsupported characters"
        )));
    }
    Ok(())
}

/// Validates a [`ServiceDescriptor`] for contract version, bounds, and
/// duplicate capability names.
pub fn validate_descriptor(descriptor: &ServiceDescriptor) -> Result<(), ServiceContractError> {
    validate_identifier(&descriptor.service_id, "service_id")?;
    if descriptor.version.trim().is_empty() {
        return Err(ServiceContractError::invalid_input(
            "service version must not be empty",
        ));
    }
    if descriptor.contract_version != SERVICE_CONTRACT_VERSION {
        return Err(ServiceContractError::invalid_input(format!(
            "unsupported contract_version: {} (expected {SERVICE_CONTRACT_VERSION})",
            descriptor.contract_version
        )));
    }
    if descriptor.capabilities.len() > MAX_CAPABILITIES_PER_SERVICE {
        return Err(ServiceContractError::invalid_input(format!(
            "capabilities count exceeds maximum {MAX_CAPABILITIES_PER_SERVICE}"
        )));
    }
    let mut seen = BTreeSet::new();
    for cap in &descriptor.capabilities {
        validate_identifier(&cap.name, "capability.name")?;
        if !seen.insert(cap.name.as_str()) {
            return Err(ServiceContractError::invalid_input(format!(
                "duplicate capability name: {}",
                cap.name
            )));
        }
    }
    Ok(())
}

/// Serializes a contract message into a length-prefixed frame (`u32` big-endian
/// length + UTF-8 JSON payload), enforcing [`MAX_SERVICE_FRAME_BYTES`].
pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, ServiceContractError> {
    let payload = serde_json::to_vec(value)
        .map_err(|err| ServiceContractError::invalid_input(format!("serialize failed: {err}")))?;
    if payload.len() > MAX_SERVICE_FRAME_BYTES {
        return Err(ServiceContractError::invalid_input(format!(
            "frame payload {} bytes exceeds limit {MAX_SERVICE_FRAME_BYTES}",
            payload.len()
        )));
    }
    let len = u32::try_from(payload.len())
        .map_err(|_| ServiceContractError::invalid_input("frame payload length overflows u32"))?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decodes a length-prefixed frame produced by [`encode_frame`], enforcing
/// [`MAX_SERVICE_FRAME_BYTES`] before deserializing.
pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T, ServiceContractError> {
    if frame.len() < 4 {
        return Err(ServiceContractError::invalid_input(
            "frame is shorter than 4-byte header",
        ));
    }
    let len_bytes: [u8; 4] = [frame[0], frame[1], frame[2], frame[3]];
    let payload_len = u32::from_be_bytes(len_bytes) as usize;
    if payload_len > MAX_SERVICE_FRAME_BYTES {
        return Err(ServiceContractError::invalid_input(format!(
            "frame payload {payload_len} bytes exceeds limit {MAX_SERVICE_FRAME_BYTES}"
        )));
    }
    let expected_total = 4 + payload_len;
    if frame.len() != expected_total {
        return Err(ServiceContractError::invalid_input(format!(
            "frame length mismatch: got {}, expected {expected_total}",
            frame.len()
        )));
    }
    serde_json::from_slice(&frame[4..])
        .map_err(|err| ServiceContractError::invalid_input(format!("deserialize failed: {err}")))
}

/// Trait implemented by every isolated EvoHime service module.
pub trait ServiceHandler: Send + Sync {
    /// Returns the static capability advertisement for this service.
    fn descriptor(&self) -> ServiceDescriptor;

    /// Returns the current runtime health report for this service.
    fn health(&self) -> ServiceHealthReport;

    /// Executes one capability request and returns its JSON output payload.
    fn invoke(
        &self,
        capability: &str,
        payload: serde_json::Value,
        caller_session_id: Option<&str>,
    ) -> Result<serde_json::Value, ServiceContractError>;
}

/// Dispatches one [`ServiceRequest`] against a [`ServiceHandler`] implementation.
pub fn dispatch_request<H: ServiceHandler>(
    handler: &H,
    request: ServiceRequest,
) -> ServiceResponse {
    match request {
        ServiceRequest::Ping => ServiceResponse::Pong {
            contract_version: SERVICE_CONTRACT_VERSION,
        },
        ServiceRequest::Health => ServiceResponse::Health(handler.health()),
        ServiceRequest::DescribeCapabilities => ServiceResponse::Capabilities(handler.descriptor()),
        ServiceRequest::Invoke {
            request_id,
            capability,
            payload,
            caller_session_id,
        } => {
            let descriptor = handler.descriptor();
            if !descriptor
                .capabilities
                .iter()
                .any(|cap| cap.name == capability)
            {
                return ServiceResponse::Error {
                    request_id: Some(request_id),
                    error: ServiceContractError::not_found(format!(
                        "capability '{capability}' is not supported by '{}'",
                        descriptor.service_id
                    )),
                };
            }
            match handler.invoke(&capability, payload, caller_session_id.as_deref()) {
                Ok(result) => ServiceResponse::InvokeResult {
                    request_id,
                    payload: result,
                },
                Err(error) => ServiceResponse::Error {
                    request_id: Some(request_id),
                    error,
                },
            }
        }
        ServiceRequest::Shutdown => ServiceResponse::ShuttingDown,
    }
}

/// Serves length-prefixed [`ServiceRequest`] / [`ServiceResponse`] frames over
/// an arbitrary synchronous byte stream until EOF or [`ServiceRequest::Shutdown`].
pub fn serve_stream<H: ServiceHandler, R: Read, W: Write>(
    handler: &H,
    reader: &mut R,
    writer: &mut W,
) -> io::Result<()> {
    let mut len_buf = [0u8; 4];
    loop {
        match reader.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(err) => return Err(err),
        }
        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len > MAX_SERVICE_FRAME_BYTES {
            let err_resp = ServiceResponse::Error {
                request_id: None,
                error: ServiceContractError::invalid_input(format!(
                    "frame payload {payload_len} bytes exceeds limit {MAX_SERVICE_FRAME_BYTES}"
                )),
            };
            let encoded = encode_frame(&err_resp)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.message))?;
            writer.write_all(&encoded)?;
            writer.flush()?;
            return Ok(());
        }
        let mut payload = vec![0u8; payload_len];
        reader.read_exact(&mut payload)?;
        let request: ServiceRequest = match serde_json::from_slice(&payload) {
            Ok(req) => req,
            Err(err) => {
                let err_resp = ServiceResponse::Error {
                    request_id: None,
                    error: ServiceContractError::invalid_input(format!(
                        "invalid request json: {err}"
                    )),
                };
                let encoded = encode_frame(&err_resp)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.message))?;
                writer.write_all(&encoded)?;
                writer.flush()?;
                continue;
            }
        };
        let is_shutdown = matches!(request, ServiceRequest::Shutdown);
        let response = dispatch_request(handler, request);
        let encoded = encode_frame(&response)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.message))?;
        writer.write_all(&encoded)?;
        writer.flush()?;
        if is_shutdown {
            return Ok(());
        }
    }
}

/// Runs [`serve_stream`] on standard input and standard output.
pub fn serve_stdio<H: ServiceHandler>(handler: &H) -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    serve_stream(handler, &mut reader, &mut writer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_invoke_request_and_response() {
        let req = ServiceRequest::Invoke {
            request_id: "req-1".to_owned(),
            capability: "media.generate_image".to_owned(),
            payload: serde_json::json!({ "prompt": "a tiny robot" }),
            caller_session_id: Some("sess-42".to_owned()),
        };
        let encoded = encode_frame(&req).expect("encode request");
        let decoded: ServiceRequest = decode_frame(&encoded).expect("decode request");
        assert_eq!(req, decoded);

        let resp = ServiceResponse::InvokeResult {
            request_id: "req-1".to_owned(),
            payload: serde_json::json!({ "path": "out.png" }),
        };
        let encoded_resp = encode_frame(&resp).expect("encode response");
        let decoded_resp: ServiceResponse = decode_frame(&encoded_resp).expect("decode response");
        assert_eq!(resp, decoded_resp);
    }

    #[test]
    fn encode_rejects_oversized_frame() {
        let huge = "a".repeat(MAX_SERVICE_FRAME_BYTES + 1);
        let req = ServiceRequest::Invoke {
            request_id: "req-big".to_owned(),
            capability: "test.op".to_owned(),
            payload: serde_json::json!({ "blob": huge }),
            caller_session_id: None,
        };
        let err = encode_frame(&req).expect_err("oversized payload must fail");
        assert_eq!(err.code, ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn validate_descriptor_rejects_duplicates_and_bad_ids() {
        let valid = ServiceDescriptor {
            service_id: "media-voice".to_owned(),
            version: "0.1.0".to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![ServiceCapabilityDescriptor {
                name: "media.generate_image".to_owned(),
                description: "Generate image".to_owned(),
                input_schema: serde_json::json!({"type": "object"}),
                required_permissions: vec!["media.write".to_owned()],
                supports_streaming: false,
            }],
        };
        assert!(validate_descriptor(&valid).is_ok());

        let mut dup = valid.clone();
        dup.capabilities.push(dup.capabilities[0].clone());
        assert_eq!(
            validate_descriptor(&dup)
                .expect_err("duplicate must fail")
                .code,
            ServiceErrorCode::InvalidInput
        );

        let mut bad_id = valid;
        bad_id.service_id = "Bad/Id".to_owned();
        assert_eq!(
            validate_descriptor(&bad_id)
                .expect_err("bad id must fail")
                .code,
            ServiceErrorCode::InvalidInput
        );
    }

    struct EchoHandler;

    impl ServiceHandler for EchoHandler {
        fn descriptor(&self) -> ServiceDescriptor {
            ServiceDescriptor {
                service_id: "echo".to_owned(),
                version: "0.1.0".to_owned(),
                contract_version: SERVICE_CONTRACT_VERSION,
                capabilities: vec![ServiceCapabilityDescriptor {
                    name: "echo.say".to_owned(),
                    description: "Echo payload".to_owned(),
                    input_schema: serde_json::json!({"type": "object"}),
                    required_permissions: vec![],
                    supports_streaming: false,
                }],
            }
        }

        fn health(&self) -> ServiceHealthReport {
            ServiceHealthReport {
                service_id: "echo".to_owned(),
                version: "0.1.0".to_owned(),
                status: ServiceHealthStatus::Ready,
                active_requests: 0,
                detail: None,
            }
        }

        fn invoke(
            &self,
            _capability: &str,
            payload: serde_json::Value,
            _caller_session_id: Option<&str>,
        ) -> Result<serde_json::Value, ServiceContractError> {
            Ok(payload)
        }
    }

    #[test]
    fn serve_stream_processes_ping_invoke_and_shutdown() {
        let mut input = Vec::new();
        input.extend(encode_frame(&ServiceRequest::Ping).expect("ping"));
        input.extend(
            encode_frame(&ServiceRequest::Invoke {
                request_id: "s1".to_owned(),
                capability: "echo.say".to_owned(),
                payload: serde_json::json!({"msg": "hi"}),
                caller_session_id: None,
            })
            .expect("invoke"),
        );
        input.extend(encode_frame(&ServiceRequest::Shutdown).expect("shutdown"));

        let mut reader = io::Cursor::new(input);
        let mut output = Vec::new();
        serve_stream(&EchoHandler, &mut reader, &mut output).expect("serve_stream");

        let mut cursor = output.as_slice();
        let mut read_next = || -> ServiceResponse {
            let len = u32::from_be_bytes([cursor[0], cursor[1], cursor[2], cursor[3]]) as usize;
            let frame = &cursor[..4 + len];
            cursor = &cursor[4 + len..];
            decode_frame(frame).expect("decode response")
        };

        assert!(matches!(read_next(), ServiceResponse::Pong { .. }));
        assert!(matches!(
            read_next(),
            ServiceResponse::InvokeResult { ref request_id, .. } if request_id == "s1"
        ));
        assert!(matches!(read_next(), ServiceResponse::ShuttingDown));
        assert!(cursor.is_empty());
    }
}
