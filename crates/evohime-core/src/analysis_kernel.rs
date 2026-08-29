//! Core authority facade for the Persistent Analysis Kernel (plan 28).
//!
//! Runtime execution is intentionally not implemented in this contract layer:
//! this facade makes the storage boundary explicit and ensures callers cannot
//! persist process memory or bypass ArtifactStore references.

pub use evohime_local_storage::analysis_kernel::{
    AnalysisKernelError, AnalysisKernelSessionV1, AnalysisKernelStore, KernelLimitsV1,
    KernelObjectPersistence, KernelObjectRefV1, KernelSensitivity, KernelStatus,
    ANALYSIS_KERNEL_SCHEMA_VERSION, ANALYSIS_KERNEL_VERSION,
};

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const KERNEL_HOST_REQUEST_VERSION: u32 = 1;
pub const KERNEL_MAX_REQUEST_BYTES: usize = 16 * 1024;
pub const KERNEL_MAX_RESULT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelOperation {
    JsonParse,
    JsonSelect,
    CsvSummary,
    ArtifactRead,
    ToolRequest,
    Filesystem,
    Network,
    Shell,
    Credentials,
}

impl KernelOperation {
    pub const fn required_capability(&self) -> Option<&'static str> {
        match self {
            Self::ArtifactRead => Some("artifact.read"),
            Self::ToolRequest => Some("tool.request"),
            _ => None,
        }
    }

    fn allowed(self) -> bool {
        matches!(
            self,
            Self::JsonParse
                | Self::JsonSelect
                | Self::CsvSummary
                | Self::ArtifactRead
                | Self::ToolRequest
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelHostRequestV1 {
    pub version: u32,
    pub request_id: String,
    pub kernel_id: String,
    pub session_id: String,
    pub operation: KernelOperation,
    pub args: Vec<u8>,
    pub requested_capability: Option<String>,
    pub context_refs: Vec<String>,
    pub correlation_id: String,
    pub idempotency_key: String,
}

impl KernelHostRequestV1 {
    pub fn validate(&self) -> Result<(), AnalysisKernelError> {
        if self.version != KERNEL_HOST_REQUEST_VERSION {
            return Err(AnalysisKernelError::UnsupportedVersion(self.version));
        }
        for (field, value) in [
            ("request_id", self.request_id.as_str()),
            ("kernel_id", self.kernel_id.as_str()),
            ("session_id", self.session_id.as_str()),
            ("correlation_id", self.correlation_id.as_str()),
            ("idempotency_key", self.idempotency_key.as_str()),
        ] {
            if value.is_empty()
                || value.len()
                    > evohime_local_storage::analysis_kernel::ANALYSIS_KERNEL_MAX_ID_BYTES
            {
                return Err(AnalysisKernelError::InvalidField(field));
            }
        }
        if self.args.len() > KERNEL_MAX_REQUEST_BYTES {
            return Err(AnalysisKernelError::RequestTooLarge(self.args.len()));
        }
        if self.context_refs.len() > 32 || self.context_refs.iter().any(|value| value.len() > 256) {
            return Err(AnalysisKernelError::InvalidField("context_refs"));
        }
        if !self.operation.clone().allowed() {
            return Err(AnalysisKernelError::ForbiddenOperation);
        }
        match (
            self.operation.required_capability(),
            self.requested_capability.as_deref(),
        ) {
            (None, None) => {}
            (Some(required), Some(requested)) if requested == required => {}
            _ => return Err(AnalysisKernelError::ForbiddenCapability),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelResponseStatus {
    Ok,
    Denied,
    Invalid,
    LimitExceeded,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelHostResponseV1 {
    pub version: u32,
    pub request_id: String,
    pub status: KernelResponseStatus,
    pub inline_result: Option<Vec<u8>>,
    pub object_ref: Option<KernelObjectRefV1>,
    pub sensitivity: KernelSensitivity,
    pub provenance: String,
    pub error_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelRuntimeState {
    Created,
    Running,
    Stopped,
    Crashed,
    Reset,
    LimitExceeded,
}

#[derive(Debug, thiserror::Error)]
pub enum KernelRuntimeError {
    #[error("analysis kernel is not running")]
    NotRunning,
    #[error("analysis kernel host request denied: {0}")]
    Denied(String),
    #[error("analysis kernel runtime limit exceeded: {0}")]
    LimitExceeded(&'static str),
    #[error("analysis kernel operation failed: {0}")]
    Operation(String),
    #[error("analysis kernel contract error: {0}")]
    Contract(#[from] AnalysisKernelError),
}

/// In-process facade for the worker contract. The map is deliberately
/// ephemeral and is never serialized; durable state is only the metadata
/// manifest in `AnalysisKernelStore` and ArtifactStore references.
pub struct KernelRuntime {
    session: AnalysisKernelSessionV1,
    state: KernelRuntimeState,
    started_at: Option<Instant>,
    last_activity: Option<Instant>,
    request_count: u32,
    objects: HashMap<String, Vec<u8>>,
}

impl KernelRuntime {
    pub fn new(session: AnalysisKernelSessionV1) -> Result<Self, AnalysisKernelError> {
        session.validate()?;
        Ok(Self {
            session,
            state: KernelRuntimeState::Created,
            started_at: None,
            last_activity: None,
            request_count: 0,
            objects: HashMap::new(),
        })
    }

    pub fn session(&self) -> &AnalysisKernelSessionV1 {
        &self.session
    }
    pub fn state(&self) -> KernelRuntimeState {
        self.state.clone()
    }

    pub fn start(&mut self, now: Instant) -> Result<(), KernelRuntimeError> {
        if !matches!(
            self.state,
            KernelRuntimeState::Created | KernelRuntimeState::Stopped | KernelRuntimeState::Reset
        ) {
            return Err(KernelRuntimeError::Operation(
                "invalid_start_transition".into(),
            ));
        }
        self.state = KernelRuntimeState::Running;
        self.started_at = Some(now);
        self.last_activity = Some(now);
        self.request_count = 0;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.state = KernelRuntimeState::Stopped;
        self.objects.clear();
    }

    pub fn reset(&mut self) {
        self.state = KernelRuntimeState::Reset;
        self.started_at = None;
        self.last_activity = None;
        self.request_count = 0;
        self.objects.clear();
    }

    /// Registers only a bounded in-process value and returns metadata. The
    /// bytes are never serialized or exposed to the renderer; checkpointed
    /// values must go through Core's ArtifactStore separately.
    pub fn put_ephemeral_object(
        &mut self,
        logical_name: String,
        type_hint: String,
        value: Vec<u8>,
        sensitivity: KernelSensitivity,
        now_ms: i64,
    ) -> Result<KernelObjectRefV1, KernelRuntimeError> {
        if !matches!(self.state, KernelRuntimeState::Running) {
            return Err(KernelRuntimeError::NotRunning);
        }
        let limits = &self.session.limits;
        if value.len() > limits.object_bytes as usize {
            return Err(KernelRuntimeError::LimitExceeded("object_bytes"));
        }
        let current_bytes: usize = self.objects.values().map(Vec::len).sum();
        if !self.objects.contains_key(&logical_name)
            && self.objects.len() >= limits.object_count as usize
        {
            return Err(KernelRuntimeError::LimitExceeded("object_count"));
        }
        let replacing = self.objects.get(&logical_name).map_or(0, Vec::len);
        if current_bytes
            .saturating_sub(replacing)
            .saturating_add(value.len())
            > limits.object_bytes as usize
        {
            return Err(KernelRuntimeError::LimitExceeded("object_bytes"));
        }
        let size = value.len() as u64;
        self.objects.insert(logical_name.clone(), value);
        let reference = KernelObjectRefV1 {
            id: format!("{}:{}", self.session.id, logical_name),
            kernel_id: self.session.id.clone(),
            logical_name,
            type_hint,
            size,
            sensitivity,
            persistence: KernelObjectPersistence::Ephemeral,
            content_hash: None,
            artifact_locator: None,
            provenance: "core:analysis-kernel-runtime".into(),
            created_at_ms: now_ms,
            invalidated_at_ms: None,
        };
        reference.validate()?;
        Ok(reference)
    }

    pub fn execute(
        &mut self,
        request: KernelHostRequestV1,
        now: Instant,
    ) -> Result<KernelHostResponseV1, KernelRuntimeError> {
        request.validate()?;
        if !matches!(self.state, KernelRuntimeState::Running) {
            return Err(KernelRuntimeError::NotRunning);
        }
        let started = self.started_at.ok_or(KernelRuntimeError::NotRunning)?;
        if now.duration_since(started)
            > Duration::from_millis(self.session.limits.lifetime_timeout_ms)
            || self.last_activity.is_some_and(|last| {
                now.duration_since(last)
                    > Duration::from_millis(self.session.limits.idle_timeout_ms)
            })
        {
            self.state = KernelRuntimeState::LimitExceeded;
            self.objects.clear();
            return Err(KernelRuntimeError::LimitExceeded("time"));
        }
        self.request_count = self.request_count.saturating_add(1);
        if self.request_count > self.session.limits.host_requests_per_minute {
            self.state = KernelRuntimeState::LimitExceeded;
            self.objects.clear();
            return Err(KernelRuntimeError::LimitExceeded("host_request_rate"));
        }
        if contains_sensitive_marker(&request.args) {
            return Err(KernelRuntimeError::Contract(
                AnalysisKernelError::SensitiveInlinePayload,
            ));
        }
        self.last_activity = Some(now);
        let result = match request.operation {
            KernelOperation::JsonParse => parse_json(&request.args),
            KernelOperation::JsonSelect => select_json(&request.args),
            KernelOperation::CsvSummary => csv_summary(&request.args),
            KernelOperation::ArtifactRead => Err(KernelRuntimeError::Denied(
                "artifact_read_requires_core_host_bridge".into(),
            )),
            KernelOperation::ToolRequest => Err(KernelRuntimeError::Denied(
                "tool_request_requires_core_authorization".into(),
            )),
            KernelOperation::Filesystem
            | KernelOperation::Network
            | KernelOperation::Shell
            | KernelOperation::Credentials => unreachable!(),
        }?;
        if result.len() > self.session.limits.output_bytes as usize
            || result.len() > KERNEL_MAX_RESULT_BYTES
        {
            self.state = KernelRuntimeState::LimitExceeded;
            self.objects.clear();
            return Err(KernelRuntimeError::LimitExceeded("output"));
        }
        let response = KernelHostResponseV1 {
            version: KERNEL_HOST_REQUEST_VERSION,
            request_id: request.request_id,
            status: KernelResponseStatus::Ok,
            inline_result: Some(result),
            object_ref: None,
            sensitivity: KernelSensitivity::Internal,
            provenance: "core:analysis-kernel".into(),
            error_class: None,
        };
        let _ = &self.objects;
        Ok(response)
    }
}

fn parse_json(args: &[u8]) -> Result<Vec<u8>, KernelRuntimeError> {
    let value: serde_json::Value = serde_json::from_slice(args)
        .map_err(|e| KernelRuntimeError::Operation(format!("invalid_json:{e}")))?;
    serde_json::to_vec(&value).map_err(|e| KernelRuntimeError::Operation(e.to_string()))
}

fn contains_sensitive_marker(args: &[u8]) -> bool {
    let lower = String::from_utf8_lossy(args).to_ascii_lowercase();
    [
        "api_key",
        "apikey",
        "access_token",
        "password",
        "secret",
        "private_key",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn select_json(args: &[u8]) -> Result<Vec<u8>, KernelRuntimeError> {
    #[derive(Deserialize)]
    struct Input {
        value: serde_json::Value,
        path: Vec<String>,
    }
    let input: Input = serde_json::from_slice(args)
        .map_err(|e| KernelRuntimeError::Operation(format!("invalid_select:{e}")))?;
    let mut value = &input.value;
    for key in input.path {
        value = value
            .get(&key)
            .ok_or_else(|| KernelRuntimeError::Operation("path_not_found".into()))?;
    }
    serde_json::to_vec(value).map_err(|e| KernelRuntimeError::Operation(e.to_string()))
}

fn csv_summary(args: &[u8]) -> Result<Vec<u8>, KernelRuntimeError> {
    let text = std::str::from_utf8(args)
        .map_err(|_| KernelRuntimeError::Operation("csv_not_utf8".into()))?;
    let mut lines = text.lines();
    let columns = lines.next().map_or(0, |line| line.split(',').count());
    let rows = lines.count();
    serde_json::to_vec(&serde_json::json!({"columns": columns, "rows": rows}))
        .map_err(|e| KernelRuntimeError::Operation(e.to_string()))
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use std::time::Instant;

    fn session() -> AnalysisKernelSessionV1 {
        AnalysisKernelSessionV1 {
            schema_version: 1,
            id: "kernel".into(),
            task_id: "task".into(),
            workspace_id: "ws".into(),
            runtime_version: "trusted-local-1".into(),
            package_manifest_hash: "a".repeat(64),
            policy_hash: "b".repeat(64),
            status: KernelStatus::Created,
            revision: 0,
            limits: KernelLimitsV1::default(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    fn request(operation: KernelOperation, args: Vec<u8>) -> KernelHostRequestV1 {
        KernelHostRequestV1 {
            version: 1,
            request_id: "request".into(),
            kernel_id: "kernel".into(),
            session_id: "session".into(),
            operation,
            args,
            requested_capability: None,
            context_refs: vec![],
            correlation_id: "corr".into(),
            idempotency_key: "idem".into(),
        }
    }

    #[test]
    fn pure_analysis_works_but_direct_effects_are_rejected() {
        let now = Instant::now();
        let mut runtime = KernelRuntime::new(session()).unwrap();
        runtime.start(now).unwrap();
        let response = runtime
            .execute(
                request(
                    KernelOperation::JsonSelect,
                    br#"{"value":{"x":7},"path":["x"]}"#.to_vec(),
                ),
                now,
            )
            .unwrap();
        assert_eq!(response.inline_result.unwrap(), b"7");
        let denied = runtime.execute(request(KernelOperation::Filesystem, vec![]), now);
        assert!(matches!(
            denied,
            Err(KernelRuntimeError::Contract(
                AnalysisKernelError::ForbiddenOperation
            ))
        ));
    }

    #[test]
    fn output_limit_forces_resettable_terminal_state() {
        let now = Instant::now();
        let mut value = session();
        value.limits.output_bytes = 1;
        let mut runtime = KernelRuntime::new(value).unwrap();
        runtime.start(now).unwrap();
        assert!(matches!(
            runtime.execute(
                request(KernelOperation::JsonParse, br#"{"x":1}"#.to_vec()),
                now
            ),
            Err(KernelRuntimeError::LimitExceeded("output"))
        ));
        assert_eq!(runtime.state(), KernelRuntimeState::LimitExceeded);
        runtime.reset();
        assert_eq!(runtime.state(), KernelRuntimeState::Reset);
    }

    #[test]
    fn sensitive_inline_payload_is_not_returned() {
        let now = Instant::now();
        let mut runtime = KernelRuntime::new(session()).unwrap();
        runtime.start(now).unwrap();
        let result = runtime.execute(
            request(
                KernelOperation::JsonParse,
                br#"{"api_key":"do-not-return"}"#.to_vec(),
            ),
            now,
        );
        assert!(matches!(
            result,
            Err(KernelRuntimeError::Contract(
                AnalysisKernelError::SensitiveInlinePayload
            ))
        ));
    }

    #[test]
    fn capability_is_exact_and_ephemeral_objects_are_bounded() {
        let now = Instant::now();
        let mut runtime = KernelRuntime::new(session()).unwrap();
        runtime.start(now).unwrap();
        let mut request = request(KernelOperation::JsonParse, b"{}".to_vec());
        request.requested_capability = Some("tool.request".into());
        assert!(matches!(
            runtime.execute(request, now),
            Err(KernelRuntimeError::Contract(
                AnalysisKernelError::ForbiddenCapability
            ))
        ));
        let reference = runtime
            .put_ephemeral_object(
                "rows".into(),
                "json".into(),
                b"{}".to_vec(),
                KernelSensitivity::Internal,
                2,
            )
            .unwrap();
        assert_eq!(reference.persistence, KernelObjectPersistence::Ephemeral);
        assert!(reference.artifact_locator.is_none());
    }
}
