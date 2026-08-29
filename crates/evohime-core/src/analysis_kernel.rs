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
pub use evohime_local_storage::task_checkpoint::{
    CheckpointRef, CheckpointSensitivity, Provenance, TaskCheckpointError, TaskCheckpointV1,
};

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[cfg(windows)]
use serde_json::Value;

pub const KERNEL_HOST_REQUEST_VERSION: u32 = 1;
pub const KERNEL_MAX_REQUEST_BYTES: usize = 16 * 1024;
pub const KERNEL_MAX_RESULT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelOperation {
    JsonParse,
    JsonSelect,
    CsvSummary,
    ObjectPut,
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
                | Self::ObjectPut
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

    pub fn mark_crashed(&mut self) {
        self.state = KernelRuntimeState::Crashed;
        self.started_at = None;
        self.last_activity = None;
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

    /// Performs the Core-side admission checks and accounts one request
    /// without executing it. The supervisor worker path uses this before
    /// forwarding a request, so the worker cannot become a second authority.
    pub fn admit(
        &mut self,
        request: &KernelHostRequestV1,
        now: Instant,
    ) -> Result<(), KernelRuntimeError> {
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
        Ok(())
    }

    pub fn accept_output(&mut self, size: usize) -> Result<(), KernelRuntimeError> {
        if size > self.session.limits.output_bytes as usize || size > KERNEL_MAX_RESULT_BYTES {
            self.state = KernelRuntimeState::LimitExceeded;
            self.objects.clear();
            return Err(KernelRuntimeError::LimitExceeded("output"));
        }
        Ok(())
    }

    pub fn execute(
        &mut self,
        request: KernelHostRequestV1,
        now: Instant,
    ) -> Result<KernelHostResponseV1, KernelRuntimeError> {
        self.admit(&request, now)?;
        if matches!(&request.operation, KernelOperation::ObjectPut) {
            #[derive(Deserialize)]
            struct ObjectPutInput {
                logical_name: String,
                type_hint: String,
                value: serde_json::Value,
                sensitivity: KernelSensitivity,
            }
            let input: ObjectPutInput = serde_json::from_slice(&request.args).map_err(|error| {
                KernelRuntimeError::Operation(format!("invalid_object:{error}"))
            })?;
            let value = serde_json::to_vec(&input.value)
                .map_err(|error| KernelRuntimeError::Operation(error.to_string()))?;
            let object = self.put_ephemeral_object(
                input.logical_name,
                input.type_hint,
                value,
                input.sensitivity,
                crate::task_memory::now_millis() as i64,
            )?;
            return Ok(KernelHostResponseV1 {
                version: KERNEL_HOST_REQUEST_VERSION,
                request_id: request.request_id,
                status: KernelResponseStatus::Ok,
                inline_result: None,
                object_ref: Some(object),
                sensitivity: input.sensitivity,
                provenance: "core:analysis-kernel-runtime".into(),
                error_class: None,
            });
        }
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
            KernelOperation::ObjectPut => unreachable!(),
        }?;
        self.accept_output(result.len())?;
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

/// Converts only immutable, checkpointable kernel metadata into the existing
/// TaskCheckpoint reference contract. Ephemeral process memory is rejected so
/// a child or a resumed task can never receive a dangling in-memory handle.
pub fn checkpoint_refs(
    session: &AnalysisKernelSessionV1,
    objects: &[KernelObjectRefV1],
) -> Result<Vec<CheckpointRef>, AnalysisKernelError> {
    session.validate()?;
    let mut refs = vec![CheckpointRef {
        id: session.id.clone(),
        kind: "analysis_kernel.session".into(),
        content_hash: Some(session.content_hash()?),
        sensitivity: CheckpointSensitivity::Internal,
        provenance: Provenance::core("analysis-kernel.session"),
    }];
    for object in objects {
        object.validate()?;
        if object.kernel_id != session.id {
            return Err(AnalysisKernelError::InvalidField("object.kernel_id"));
        }
        if object.persistence != KernelObjectPersistence::Checkpointed {
            return Err(AnalysisKernelError::ProcessMemoryPersistence);
        }
        refs.push(CheckpointRef {
            id: object.id.clone(),
            kind: "analysis_kernel.object".into(),
            content_hash: object.content_hash.clone(),
            sensitivity: match object.sensitivity {
                KernelSensitivity::Public => CheckpointSensitivity::Public,
                KernelSensitivity::Internal => CheckpointSensitivity::Internal,
                KernelSensitivity::Sensitive => CheckpointSensitivity::Sensitive,
                KernelSensitivity::Secret => CheckpointSensitivity::Secret,
            },
            provenance: Provenance::core(object.provenance.clone()),
        });
    }
    Ok(refs)
}

/// Selects a child-visible subset while preserving the same immutable-ref
/// checks. Unknown, duplicate or ephemeral IDs are rejected instead of being
/// silently dropped, keeping child handoff scoped to explicit Core selection.
pub fn selected_child_refs(
    session: &AnalysisKernelSessionV1,
    objects: &[KernelObjectRefV1],
    selected_ids: &[String],
) -> Result<Vec<CheckpointRef>, AnalysisKernelError> {
    if selected_ids.is_empty() || selected_ids.len() > 64 {
        return Err(AnalysisKernelError::InvalidField("selected_ids"));
    }
    let mut selected: Vec<KernelObjectRefV1> = Vec::with_capacity(selected_ids.len());
    for id in selected_ids {
        if selected.iter().any(|object| object.id == *id) {
            return Err(AnalysisKernelError::InvalidField("selected_ids"));
        }
        let object = objects
            .iter()
            .find(|object| object.id == *id)
            .ok_or(AnalysisKernelError::InvalidField("selected_ids"))?;
        selected.push(object.clone());
    }
    let refs = checkpoint_refs(session, &selected)?;
    Ok(refs.into_iter().skip(1).collect())
}

/// Attaches the selected immutable kernel refs to a Core-owned checkpoint and
/// reseals its canonical hash. Callers can place the returned refs in
/// `artifact_refs` or `child_refs` according to the existing workflow scope.
pub fn attach_checkpoint_refs(
    mut checkpoint: TaskCheckpointV1,
    session: &AnalysisKernelSessionV1,
    objects: &[KernelObjectRefV1],
) -> Result<TaskCheckpointV1, TaskCheckpointError> {
    let refs =
        checkpoint_refs(session, objects).map_err(|error| TaskCheckpointError::InvalidField {
            field: "analysis_kernel_refs",
            reason: error.to_string(),
        })?;
    checkpoint.artifact_refs.extend(refs);
    checkpoint.seal()
}

/// Opens the already authenticated Core -> supervisor lifecycle channel for
/// one bounded request. A new connection per command prevents a stale worker
/// stream from surviving a supervisor generation change.
#[cfg(windows)]
pub async fn supervisor_command(request: Value) -> Result<Value, String> {
    use evohime_desktop_ipc::session::{read_launch_context, SessionSecret};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::windows::named_pipe::ClientOptions;

    let context_path = std::env::var_os("EVOHIME_LAUNCH_CONTEXT")
        .ok_or_else(|| "supervisor_context_unavailable".to_string())?;
    let context = read_launch_context(std::path::Path::new(&context_path))
        .map_err(|error| error.to_string())?;
    let pipe = context
        .supervisor_pipe_name
        .as_deref()
        .ok_or_else(|| "supervisor_channel_unavailable".to_string())?;
    let secret = context
        .supervisor_secret
        .as_ref()
        .ok_or_else(|| "supervisor_secret_unavailable".to_string())?;
    let client_id = format!("core-kernel-{}", std::process::id());
    let client = ClientOptions::new()
        .open(pipe)
        .map_err(|error| error.to_string())?;
    let mut channel = BufReader::new(client);
    let mut line = Vec::new();
    if channel
        .read_until(b'\n', &mut line)
        .await
        .map_err(|e| e.to_string())?
        > 16 * 1024
    {
        return Err("supervisor_challenge_too_large".into());
    }
    let challenge: Value = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
    let nonce = challenge
        .get("nonce")
        .and_then(Value::as_str)
        .ok_or_else(|| "supervisor_nonce_missing".to_string())?;
    let proof = SessionSecret::parse(secret.expose())
        .map_err(|error| error.to_string())?
        .proof("core", &client_id, nonce);
    let handshake = serde_json::json!({
        "client_id": client_id,
        "client_role": "core",
        "nonce": nonce,
        "proof": proof,
        "peer": {
            "user_sid": evohime_desktop_ipc::windows_security::current_user_sid().map_err(|e| e.to_string())?,
            "logon_session": evohime_desktop_ipc::windows_security::current_logon_session().map_err(|e| e.to_string())?
        }
    });
    channel
        .get_mut()
        .write_all(
            serde_json::to_string(&handshake)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .await
        .map_err(|e| e.to_string())?;
    channel
        .get_mut()
        .write_all(b"\n")
        .await
        .map_err(|e| e.to_string())?;
    line.clear();
    channel
        .read_until(b'\n', &mut line)
        .await
        .map_err(|e| e.to_string())?;
    let authenticated: Value = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
    if authenticated.get("authenticated") != Some(&Value::Bool(true)) {
        return Err("supervisor_authentication_rejected".into());
    }
    let encoded = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    if encoded.len() > 16 * 1024 {
        return Err("supervisor_request_too_large".into());
    }
    channel
        .get_mut()
        .write_all(&encoded)
        .await
        .map_err(|e| e.to_string())?;
    channel
        .get_mut()
        .write_all(b"\n")
        .await
        .map_err(|e| e.to_string())?;
    line.clear();
    if channel
        .read_until(b'\n', &mut line)
        .await
        .map_err(|e| e.to_string())?
        > 16 * 1024
    {
        return Err("supervisor_response_too_large".into());
    }
    serde_json::from_slice(&line).map_err(|e| e.to_string())
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

    #[test]
    fn object_put_keeps_value_in_runtime_and_returns_metadata_only_ref() {
        let now = Instant::now();
        let mut runtime = KernelRuntime::new(session()).unwrap();
        runtime.start(now).unwrap();
        let response = runtime
            .execute(
                request(
                    KernelOperation::ObjectPut,
                    br#"{"logical_name":"rows","type_hint":"json","value":{"x":7},"sensitivity":"internal"}"#.to_vec(),
                ),
                now,
            )
            .unwrap();
        assert!(response.inline_result.is_none());
        let reference = response.object_ref.unwrap();
        assert_eq!(reference.logical_name, "rows");
        assert_eq!(reference.persistence, KernelObjectPersistence::Ephemeral);
        assert!(reference.content_hash.is_none());
        assert!(reference.artifact_locator.is_none());
    }

    #[test]
    fn checkpoint_refs_reject_ephemeral_memory_and_preserve_kernel_identity() {
        let now = Instant::now();
        let mut runtime = KernelRuntime::new(session()).unwrap();
        runtime.start(now).unwrap();
        let ephemeral = runtime
            .put_ephemeral_object(
                "rows".into(),
                "json".into(),
                b"{}".to_vec(),
                KernelSensitivity::Internal,
                2,
            )
            .unwrap();
        assert!(matches!(
            checkpoint_refs(runtime.session(), &[ephemeral]),
            Err(AnalysisKernelError::ProcessMemoryPersistence)
        ));
        let refs = checkpoint_refs(runtime.session(), &[]).unwrap();
        assert_eq!(refs[0].id, "kernel");
        assert_eq!(refs[0].kind, "analysis_kernel.session");
        assert!(refs[0]
            .content_hash
            .as_ref()
            .is_some_and(|hash| hash.len() == 64));
    }

    #[test]
    fn selected_child_refs_require_explicit_checkpointable_objects() {
        let object = KernelObjectRefV1 {
            id: "object-1".into(),
            kernel_id: "kernel".into(),
            logical_name: "rows".into(),
            type_hint: "json".into(),
            size: 2,
            sensitivity: KernelSensitivity::Internal,
            persistence: KernelObjectPersistence::Checkpointed,
            content_hash: Some("c".repeat(64)),
            artifact_locator: Some("artifact://kernel/object-1".into()),
            provenance: "core:analysis-kernel".into(),
            created_at_ms: 2,
            invalidated_at_ms: None,
        };
        let refs = selected_child_refs(
            &session(),
            std::slice::from_ref(&object),
            &["object-1".into()],
        )
        .unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, "object-1");
        assert!(matches!(
            selected_child_refs(&session(), &[object], &["missing".into()]),
            Err(AnalysisKernelError::InvalidField("selected_ids"))
        ));
    }

    #[test]
    fn crash_is_terminal_for_ephemeral_state_until_explicit_reset() {
        let now = Instant::now();
        let mut runtime = KernelRuntime::new(session()).unwrap();
        runtime.start(now).unwrap();
        runtime
            .put_ephemeral_object(
                "rows".into(),
                "json".into(),
                b"{}".to_vec(),
                KernelSensitivity::Internal,
                2,
            )
            .unwrap();
        runtime.mark_crashed();
        assert_eq!(runtime.state(), KernelRuntimeState::Crashed);
        assert!(matches!(
            runtime.execute(request(KernelOperation::JsonParse, b"{}".to_vec()), now),
            Err(KernelRuntimeError::NotRunning)
        ));
        runtime.reset();
        runtime.start(now).unwrap();
        assert!(runtime
            .execute(request(KernelOperation::JsonParse, b"{}".to_vec()), now)
            .is_ok());
    }
}
