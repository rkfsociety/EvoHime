//! Core-owned analysis kernel host for the declared `analysis_run` tool.
//!
//! This implementation reuses the existing IPC kernel runtime, session storage,
//! and supervisor worker. It does not create a second runtime, duplicate dispatch,
//! or bypass the existing permission path.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use crate::analysis_kernel::{self, KernelLimitsV1, KernelRuntime};
use crate::EventJournal;
use evohime_local_storage::analysis_kernel::{
    AnalysisKernelError, AnalysisKernelSessionV1, AnalysisKernelStore, KernelStatus,
    ANALYSIS_KERNEL_SCHEMA_VERSION,
};
use evohime_tool_runtime::kernel_host::{
    AnalysisOperation, KernelHost, KernelTaskBinding, KernelToolError, KernelToolRequest,
    KernelToolResult,
};
use sha2::Digest;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Canonical hash of the `analysis_run` tool manifest for use in session policy.
///
/// This is a deterministically computed SHA-256 digest of the canonical JSON
/// representation of the tool's manifest. It is computed once and used to bind
/// kernel sessions to the exact declared tool contract.
pub fn analysis_run_manifest_hash() -> &'static str {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let registry = evohime_tool_runtime::ToolRegistry::bootstrap();
        let raw_hash = match registry.manifest_for(evohime_tool_runtime::analysis::NAME) {
            Some(manifest) => match manifest.canonical_hash() {
                Ok(hash) => hash,
                Err(error) => {
                    tracing::warn!(%error, "failed to compute canonical hash for analysis_run manifest; using agent policy hash");
                    AGENT_KERNEL_POLICY_HASH.to_string()
                }
            },
            None => {
                tracing::warn!("analysis_run tool manifest not found; using agent policy hash");
                AGENT_KERNEL_POLICY_HASH.to_string()
            }
        };
        raw_hash
            .strip_prefix("sha256:")
            .unwrap_or(&raw_hash)
            .to_string()
    })
}

/// Policy hash used for kernel sessions created by the agent.
///
/// The agent uses the default `trusted-local-1` runtime with minimal policy
/// constraints; this hash represents that explicit policy snapshot.
pub const AGENT_KERNEL_POLICY_HASH: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"; // SHA-256("")

/// Core-owned host that bridges the declared tool to the existing kernel runtime.
#[derive(Clone)]
pub struct CoreKernelHost {
    /// Shared journal for durable session storage.
    journal: EventJournal,
    /// Process-local map of active kernel runtimes (shared with IPC commands).
    kernels: Arc<Mutex<std::collections::HashMap<String, KernelRuntime>>>,
    /// Serializes lazy session creation per task so parallel tool calls reuse one session.
    session_init_lock: Arc<Mutex<()>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedKernelObjectRef {
    id: String,
    logical_name: String,
    type_hint: String,
    size: u64,
    sensitivity: String,
    persistence: String,
    content_hash: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedKernelResult {
    kernel_id: String,
    inline_result: Option<Vec<u8>>,
    object_ref: Option<CachedKernelObjectRef>,
    sensitivity: String,
    provenance: String,
}

impl From<&evohime_tool_runtime::kernel_host::KernelToolResult> for CachedKernelResult {
    fn from(value: &evohime_tool_runtime::kernel_host::KernelToolResult) -> Self {
        Self {
            kernel_id: value.kernel_id.clone(),
            inline_result: value.inline_result.clone(),
            object_ref: value.object_ref.as_ref().map(|obj| CachedKernelObjectRef {
                id: obj.id.clone(),
                logical_name: obj.logical_name.clone(),
                type_hint: obj.type_hint.clone(),
                size: obj.size,
                sensitivity: obj.sensitivity.clone(),
                persistence: obj.persistence.clone(),
                content_hash: obj.content_hash.clone(),
            }),
            sensitivity: value.sensitivity.clone(),
            provenance: value.provenance.clone(),
        }
    }
}

impl From<CachedKernelResult> for evohime_tool_runtime::kernel_host::KernelToolResult {
    fn from(value: CachedKernelResult) -> Self {
        Self {
            kernel_id: value.kernel_id,
            inline_result: value.inline_result,
            object_ref: value.object_ref.map(|obj| {
                evohime_tool_runtime::kernel_host::KernelToolObjectRef {
                    id: obj.id,
                    logical_name: obj.logical_name,
                    type_hint: obj.type_hint,
                    size: obj.size,
                    sensitivity: obj.sensitivity,
                    persistence: obj.persistence,
                    content_hash: obj.content_hash,
                }
            }),
            sensitivity: value.sensitivity,
            provenance: value.provenance,
        }
    }
}

fn operation_name(operation: AnalysisOperation) -> &'static str {
    match operation {
        AnalysisOperation::JsonParse => "json_parse",
        AnalysisOperation::JsonSelect => "json_select",
        AnalysisOperation::CsvSummary => "csv_summary",
        AnalysisOperation::ObjectPut => "object_put",
    }
}

impl CoreKernelHost {
    /// Creates a new kernel host attached to the given journal and runtime map.
    pub fn new(
        journal: EventJournal,
        kernels: Arc<Mutex<std::collections::HashMap<String, KernelRuntime>>>,
    ) -> Self {
        Self {
            journal,
            kernels,
            session_init_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Stops all active kernel sessions belonging to `task_id` when the task finishes.
    pub async fn stop_sessions_for_task(&self, task_id: &str) {
        let sessions = {
            let database = self.journal.database().lock().await;
            let store = AnalysisKernelStore::new(database.connection());
            store.list_sessions_for_task(task_id).unwrap_or_default()
        };
        let now = crate::task_memory::now_millis() as i64;
        for session in sessions {
            if !matches!(
                session.status,
                KernelStatus::Created | KernelStatus::Running | KernelStatus::Reset
            ) {
                continue;
            }
            {
                let mut kernels = self.kernels.lock().await;
                if let Some(mut runtime) = kernels.remove(&session.id) {
                    runtime.stop();
                }
            }
            #[cfg(windows)]
            if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some() {
                let _ = analysis_kernel::supervisor_command(serde_json::json!({
                    "op": "kernel_stop",
                    "kernel_id": session.id,
                }))
                .await;
            }
            let database = self.journal.database().lock().await;
            let store = AnalysisKernelStore::new(database.connection());
            let _ = store.set_status(&session.id, session.revision, KernelStatus::Stopped, now);
        }
    }

    /// Computes a deterministic workspace identifier from an absolute workspace root.
    fn workspace_id_from_root(workspace_root: &Path) -> String {
        // Use the same deterministic hashing as IPC commands for consistency
        crate::task_memory::project_scope_id(&workspace_root.to_string_lossy())
    }

    /// Ensures a kernel session exists for the given task binding, creating it
    /// lazily on the first agent call. Returns the session id.
    async fn ensure_session_for_task(
        &self,
        binding: &KernelTaskBinding,
    ) -> Result<String, KernelToolError> {
        let _init_guard = self.session_init_lock.lock().await;
        let workspace_id = Self::workspace_id_from_root(&binding.workspace_root);
        let task_id = binding.task_id.to_string();

        // Check if a session already exists for this task
        let existing = {
            let database = self.journal.database().lock().await;
            let store = AnalysisKernelStore::new(database.connection());
            store
                .list_sessions_for_task(&task_id)
                .map_err(|error| {
                    KernelToolError::new(
                        evohime_tool_runtime::kernel_host::KERNEL_STORAGE_FAILED,
                        format!("could not query existing sessions: {error}"),
                    )
                })?
                .into_iter()
                .find(|session| {
                    session.workspace_id == workspace_id
                        && matches!(
                            session.status,
                            KernelStatus::Created | KernelStatus::Running | KernelStatus::Reset
                        )
                })
        };

        if let Some(session) = existing {
            return Ok(session.id);
        }

        // No suitable session exists; create a new one with default limits
        let now = crate::task_memory::now_millis() as i64;
        let session = AnalysisKernelSessionV1 {
            schema_version: ANALYSIS_KERNEL_SCHEMA_VERSION,
            id: format!("kernel-{}", Uuid::new_v4()),
            task_id,
            workspace_id,
            runtime_version: "trusted-local-1".into(),
            package_manifest_hash: analysis_run_manifest_hash().into(),
            policy_hash: AGENT_KERNEL_POLICY_HASH.into(),
            status: KernelStatus::Created,
            revision: 0,
            limits: KernelLimitsV1::default(),
            created_at_ms: now,
            updated_at_ms: now,
        };

        if session.validate().is_err() {
            return Err(KernelToolError::new(
                evohime_tool_runtime::kernel_host::KERNEL_INVALID_INPUT,
                "kernel session validation failed",
            ));
        }

        #[cfg(windows)]
        if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some() {
            let launch = analysis_kernel::supervisor_command(serde_json::json!({
                "op": "kernel_launch",
                "kernel_id": session.id,
                "package_manifest_hash": session.package_manifest_hash,
            }))
            .await;
            if !matches!(launch, Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)))
            {
                return Err(KernelToolError::new(
                    evohime_tool_runtime::kernel_host::KERNEL_UNAVAILABLE,
                    "kernel worker unavailable",
                ));
            }
        }

        {
            let database = self.journal.database().lock().await;
            let store = AnalysisKernelStore::new(database.connection());
            store.create_session(&session).map_err(|error| {
                KernelToolError::new(
                    evohime_tool_runtime::kernel_host::KERNEL_STORAGE_FAILED,
                    format!("could not create session: {error}"),
                )
            })?;

            store
                .set_status(&session.id, session.revision, KernelStatus::Running, now)
                .map_err(|error| {
                    KernelToolError::new(
                        evohime_tool_runtime::kernel_host::KERNEL_STORAGE_FAILED,
                        format!("could not start session: {error}"),
                    )
                })?;
        }

        // Create runtime and insert into process-local map
        let mut session = session;
        session.status = KernelStatus::Running;
        session.revision = session.revision.saturating_add(1);
        let mut runtime = KernelRuntime::new(session.clone()).map_err(|error| {
            KernelToolError::new(
                evohime_tool_runtime::kernel_host::KERNEL_INVALID_INPUT,
                format!("could not create runtime: {error}"),
            )
        })?;

        if runtime.start(Instant::now()).is_err() {
            return Err(KernelToolError::new(
                evohime_tool_runtime::kernel_host::KERNEL_UNAVAILABLE,
                "runtime start failed",
            ));
        }

        self.kernels
            .lock()
            .await
            .insert(session.id.clone(), runtime);

        Ok(session.id)
    }

    /// Converts a tool request to a kernel host request.
    fn tool_request_to_host_request(
        &self,
        kernel_id: &str,
        request: &KernelToolRequest,
        task_id: &str,
    ) -> Result<analysis_kernel::KernelHostRequestV1, KernelToolError> {
        let operation = match request.operation {
            AnalysisOperation::JsonParse => analysis_kernel::KernelOperation::JsonParse,
            AnalysisOperation::JsonSelect => analysis_kernel::KernelOperation::JsonSelect,
            AnalysisOperation::CsvSummary => analysis_kernel::KernelOperation::CsvSummary,
            AnalysisOperation::ObjectPut => analysis_kernel::KernelOperation::ObjectPut,
        };

        let args = match request.operation {
            AnalysisOperation::ObjectPut => {
                let value: serde_json::Value = serde_json::from_slice(&request.args).map_err(|error| {
                    KernelToolError::new(
                        evohime_tool_runtime::kernel_host::KERNEL_INVALID_INPUT,
                        format!("invalid object_put value: {error}"),
                    )
                })?;
                let logical_name = request.logical_name.as_deref().unwrap_or("object");
                let type_hint = request.type_hint.as_deref().unwrap_or("json");
                let sensitivity = match request.sensitivity {
                    Some(AnalysisSensitivity::Public) => "public",
                    Some(AnalysisSensitivity::Confidential) => "confidential",
                    Some(AnalysisSensitivity::Internal) | None => "internal",
                };
                serde_json::to_vec(&serde_json::json!({
                    "logical_name": logical_name,
                    "type_hint": type_hint,
                    "value": value,
                    "sensitivity": sensitivity,
                }))
                .map_err(|error| {
                    KernelToolError::new(
                        evohime_tool_runtime::kernel_host::KERNEL_INVALID_INPUT,
                        format!("could not encode object_put payload: {error}"),
                    )
                })?
            }
            _ => request.args.clone(),
        };

        Ok(analysis_kernel::KernelHostRequestV1 {
            version: analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
            request_id: format!("agent-{}", Uuid::new_v4()),
            kernel_id: kernel_id.to_string(),
            session_id: kernel_id.to_string(),
            operation,
            args,
            requested_capability: None,
            context_refs: Vec::new(),
            correlation_id: format!("agent-{}", task_id),
            idempotency_key: format!(
                "{}-{}-{}-{}",
                task_id,
                kernel_id,
                operation_name(request.operation),
                hex::encode(sha2::Sha256::digest(&request.args))
            ),
        })
    }

    /// Converts a kernel host response to a tool result.
    fn host_response_to_tool_result(
        &self,
        kernel_id: &str,
        response: analysis_kernel::KernelHostResponseV1,
    ) -> KernelToolResult {
        KernelToolResult {
            kernel_id: kernel_id.to_string(),
            inline_result: response.inline_result,
            object_ref: response.object_ref.map(|object| {
                evohime_tool_runtime::kernel_host::KernelToolObjectRef {
                    id: object.id,
                    logical_name: object.logical_name,
                    type_hint: object.type_hint,
                    size: object.size,
                    sensitivity: object.sensitivity.as_str().into(),
                    persistence: object.persistence.as_str().into(),
                    content_hash: object.content_hash,
                }
            }),
            sensitivity: response.sensitivity.as_str().into(),
            provenance: response.provenance,
        }
    }
}

impl CoreKernelHost {
    async fn persist_session_status(&self, kernel_id: &str, status: KernelStatus) {
        let now = crate::task_memory::now_millis() as i64;
        let database = self.journal.database().lock().await;
        let store = AnalysisKernelStore::new(database.connection());
        if let Ok(Some(session)) = store.get_session(kernel_id) {
            let _ = store.set_status(kernel_id, session.revision, status, now);
        }
    }

    fn map_runtime_error(error: analysis_kernel::KernelRuntimeError) -> KernelToolError {
        let code = match error {
            analysis_kernel::KernelRuntimeError::NotRunning => {
                evohime_tool_runtime::kernel_host::KERNEL_UNAVAILABLE
            }
            analysis_kernel::KernelRuntimeError::Denied(_) => {
                evohime_tool_runtime::kernel_host::KERNEL_FORBIDDEN_CAPABILITY
            }
            analysis_kernel::KernelRuntimeError::LimitExceeded(_) => {
                evohime_tool_runtime::kernel_host::KERNEL_LIMIT_EXCEEDED
            }
            analysis_kernel::KernelRuntimeError::Operation(_) => {
                evohime_tool_runtime::kernel_host::KERNEL_INVALID_INPUT
            }
            analysis_kernel::KernelRuntimeError::Contract(
                AnalysisKernelError::SensitiveInlinePayload,
            ) => evohime_tool_runtime::kernel_host::KERNEL_SENSITIVE_INLINE_PAYLOAD,
            analysis_kernel::KernelRuntimeError::Contract(_) => {
                evohime_tool_runtime::kernel_host::KERNEL_INVALID_INPUT
            }
        };
        KernelToolError::new(code, error.to_string())
    }
}

impl KernelHost for CoreKernelHost {
    fn execute<'a>(
        &'a self,
        task: KernelTaskBinding,
        request: KernelToolRequest,
    ) -> Pin<Box<dyn Future<Output = Result<KernelToolResult, KernelToolError>> + Send + 'a>> {
        Box::pin(async move {
            let op_name = operation_name(request.operation);
            let kernel_id = self.ensure_session_for_task(&task).await?;
            let host_request =
                self.tool_request_to_host_request(&kernel_id, &request, &task.task_id.to_string())?;
            let idempotency_key = host_request.idempotency_key.clone();

            {
                let database = self.journal.database().lock().await;
                let store = AnalysisKernelStore::new(database.connection());
                if let Ok(Some(cached_bytes)) =
                    store.get_idempotency(&kernel_id, &idempotency_key, op_name)
                {
                    if let Ok(cached) = serde_json::from_slice::<CachedKernelResult>(&cached_bytes)
                    {
                        return Ok(cached.into());
                    }
                }
            }

            let mut kernels = self.kernels.lock().await;
            let Some(runtime) = kernels.get_mut(&kernel_id) else {
                return Err(KernelToolError::new(
                    evohime_tool_runtime::kernel_host::KERNEL_UNAVAILABLE,
                    format!("kernel session {} not found in runtime map", kernel_id),
                ));
            };

            #[cfg(windows)]
            let tool_result = if std::env::var_os("EVOHIME_LAUNCH_CONTEXT").is_some()
                && !matches!(request.operation, AnalysisOperation::ObjectPut)
            {
                let worker_args = match host_request.operation {
                    analysis_kernel::KernelOperation::CsvSummary => {
                        serde_json::Value::String(String::from_utf8_lossy(&host_request.args).into())
                    }
                    _ => serde_json::from_slice(&host_request.args).unwrap_or_else(|_| {
                        serde_json::Value::String(String::from_utf8_lossy(&host_request.args).into())
                    }),
                };
                if let Err(error) = runtime.admit(&host_request, Instant::now()) {
                    let is_limit = matches!(error, analysis_kernel::KernelRuntimeError::LimitExceeded(_));
                    drop(kernels);
                    if is_limit {
                        self.persist_session_status(&kernel_id, KernelStatus::LimitExceeded)
                            .await;
                    }
                    return Err(Self::map_runtime_error(error));
                }
                drop(kernels);
                let worker_response = analysis_kernel::supervisor_command(serde_json::json!({
                    "op": "kernel_execute",
                    "kernel_id": kernel_id,
                    "request": {
                        "request_id": host_request.request_id,
                        "operation": op_name,
                        "args": worker_args,
                    },
                }))
                .await;
                let response = match worker_response {
                    Ok(value) if value.get("accepted") == Some(&serde_json::Value::Bool(true)) => {
                        value
                            .get("response")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null)
                    }
                    Ok(value) => {
                        if let Some(runtime) = self.kernels.lock().await.get_mut(&kernel_id) {
                            runtime.mark_crashed();
                        }
                        self.persist_session_status(&kernel_id, KernelStatus::Crashed)
                            .await;
                        return Err(KernelToolError::new(
                            evohime_tool_runtime::kernel_host::KERNEL_UNAVAILABLE,
                            value
                                .get("reason")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("worker_unavailable"),
                        ));
                    }
                    Err(error) => {
                        if let Some(runtime) = self.kernels.lock().await.get_mut(&kernel_id) {
                            runtime.mark_crashed();
                        }
                        self.persist_session_status(&kernel_id, KernelStatus::Crashed)
                            .await;
                        return Err(KernelToolError::new(
                            evohime_tool_runtime::kernel_host::KERNEL_UNAVAILABLE,
                            format!("worker unavailable: {error}"),
                        ));
                    }
                };
                if response.get("status").and_then(serde_json::Value::as_str) != Some("ok") {
                    return Err(KernelToolError::new(
                        evohime_tool_runtime::kernel_host::KERNEL_INVALID_INPUT,
                        response
                            .get("error_class")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("worker_error"),
                    ));
                }
                let inline_result =
                    serde_json::to_vec(response.get("result").unwrap_or(&serde_json::Value::Null))
                        .unwrap_or_default();
                let mut kernels = self.kernels.lock().await;
                if let Some(runtime) = kernels.get_mut(&kernel_id) {
                    if let Err(error) = runtime.accept_output(inline_result.len()) {
                        let is_limit =
                            matches!(error, analysis_kernel::KernelRuntimeError::LimitExceeded(_));
                        drop(kernels);
                        if is_limit {
                            self.persist_session_status(&kernel_id, KernelStatus::LimitExceeded)
                                .await;
                        }
                        return Err(Self::map_runtime_error(error));
                    }
                }
                KernelToolResult {
                    kernel_id: kernel_id.clone(),
                    inline_result: Some(inline_result),
                    object_ref: None,
                    sensitivity: analysis_kernel::KernelSensitivity::Internal
                        .as_str()
                        .into(),
                    provenance: "core:analysis-kernel-worker".into(),
                }
            } else {
                match runtime.execute(host_request, Instant::now()) {
                    Ok(response) => {
                        let object_ref = response.object_ref.clone();
                        let result = self.host_response_to_tool_result(&kernel_id, response);
                        drop(kernels);
                        if let Some(object) = object_ref {
                            let database = self.journal.database().lock().await;
                            let store = AnalysisKernelStore::new(database.connection());
                            let _ = store.put_object(&object);
                        }
                        result
                    }
                    Err(error) => {
                        let is_limit =
                            matches!(error, analysis_kernel::KernelRuntimeError::LimitExceeded(_));
                        drop(kernels);
                        if is_limit {
                            self.persist_session_status(&kernel_id, KernelStatus::LimitExceeded)
                                .await;
                        }
                        return Err(Self::map_runtime_error(error));
                    }
                }
            };

            #[cfg(not(windows))]
            let tool_result = match runtime.execute(host_request, Instant::now()) {
                Ok(response) => {
                    let object_ref = response.object_ref.clone();
                    let result = self.host_response_to_tool_result(&kernel_id, response);
                    drop(kernels);
                    if let Some(object) = object_ref {
                        let database = self.journal.database().lock().await;
                        let store = AnalysisKernelStore::new(database.connection());
                        let _ = store.put_object(&object);
                    }
                    result
                }
                Err(error) => {
                    let is_limit =
                        matches!(error, analysis_kernel::KernelRuntimeError::LimitExceeded(_));
                    drop(kernels);
                    if is_limit {
                        self.persist_session_status(&kernel_id, KernelStatus::LimitExceeded)
                            .await;
                    }
                    return Err(Self::map_runtime_error(error));
                }
            };

            if let Ok(encoded) = serde_json::to_vec(&CachedKernelResult::from(&tool_result)) {
                if encoded.len() <= evohime_local_storage::analysis_kernel::ANALYSIS_KERNEL_MAX_IDEMPOTENCY_RESULT_BYTES {
                    let now = crate::task_memory::now_millis() as i64;
                    let database = self.journal.database().lock().await;
                    let store = AnalysisKernelStore::new(database.connection());
                    let _ = store.put_idempotency(
                        &kernel_id,
                        &idempotency_key,
                        op_name,
                        &encoded,
                        now,
                    );
                }
            }

            Ok(tool_result)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_tool_runtime::kernel_host::{KernelTaskBinding, KernelToolRequest};
    use std::path::PathBuf;
    use uuid::Uuid;

    fn test_binding() -> KernelTaskBinding {
        KernelTaskBinding {
            task_id: Uuid::new_v4(),
            session_id: None,
            workspace_root: PathBuf::from("/test/workspace"),
        }
    }

    fn test_request() -> KernelToolRequest {
        KernelToolRequest {
            operation: AnalysisOperation::JsonParse,
            args: br#"{"test": "data"}"#.to_vec(),
            logical_name: None,
            type_hint: None,
            sensitivity: None,
        }
    }

    #[test]
    fn workspace_id_is_deterministic() {
        let root1 = PathBuf::from("/test/workspace");
        let root2 = PathBuf::from("/test/workspace");
        let root3 = PathBuf::from("/other/workspace");

        let id1 = CoreKernelHost::workspace_id_from_root(&root1);
        let id2 = CoreKernelHost::workspace_id_from_root(&root2);
        let id3 = CoreKernelHost::workspace_id_from_root(&root3);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
        assert!(!id1.is_empty());
    }

    #[test]
    fn manifest_hash_is_valid_sha256() {
        let hash = analysis_run_manifest_hash();
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    async fn test_host() -> CoreKernelHost {
        let journal = EventJournal::open_in_memory()
            .await
            .expect("in-memory journal");
        let kernels = Arc::new(Mutex::new(std::collections::HashMap::new()));
        CoreKernelHost::new(journal, kernels)
    }

    #[tokio::test]
    async fn first_call_creates_and_second_reuses_session() {
        let host = test_host().await;
        let binding = test_binding();
        let first = host
            .execute(binding.clone(), test_request())
            .await
            .expect("first call succeeds");
        let second = host
            .execute(
                binding.clone(),
                KernelToolRequest {
                    operation: AnalysisOperation::CsvSummary,
                    args: b"a,b\n1,2\n3,4".to_vec(),
                    logical_name: None,
                    type_hint: None,
                    sensitivity: None,
                },
            )
            .await
            .expect("second call reuses session");

        assert_eq!(first.kernel_id, second.kernel_id);
        assert!(first.inline_result.is_some());
        assert!(second.inline_result.is_some());

        let db = host.journal.database().lock().await;
        let store = AnalysisKernelStore::new(db.connection());
        let sessions = store
            .list_sessions_for_task(&binding.task_id.to_string())
            .expect("list sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].package_manifest_hash, analysis_run_manifest_hash());
        assert_eq!(sessions[0].policy_hash, AGENT_KERNEL_POLICY_HASH);
    }

    #[tokio::test]
    async fn idempotent_repeat_returns_cached_result() {
        let host = test_host().await;
        let binding = test_binding();
        let req = test_request();
        let first = host
            .execute(binding.clone(), req.clone())
            .await
            .expect("first call succeeds");

        host.kernels.lock().await.clear();

        let second = host
            .execute(binding, req)
            .await
            .expect("idempotent repeat succeeds from cache");
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn sensitive_inline_argument_is_rejected() {
        let host = test_host().await;
        let binding = test_binding();
        let err = host
            .execute(
                binding,
                KernelToolRequest {
                    operation: AnalysisOperation::JsonParse,
                    args: br#"{"token":"sk-secret-key-1234567890"}"#.to_vec(),
                    logical_name: None,
                    type_hint: None,
                    sensitivity: None,
                },
            )
            .await
            .expect_err("sensitive payload must be rejected");
        assert_eq!(
            err.code,
            evohime_tool_runtime::kernel_host::KERNEL_SENSITIVE_INLINE_PAYLOAD
        );
    }

    #[tokio::test]
    async fn output_limit_exceeded_marks_session_limit_exceeded() {
        let host = test_host().await;
        let binding = test_binding();
        let first = host
            .execute(binding.clone(), test_request())
            .await
            .expect("initial call");
        {
            let mut kernels = host.kernels.lock().await;
            let mut session = kernels.get(&first.kernel_id).unwrap().session().clone();
            session.limits.max_output_bytes = 4;
            let mut limited = KernelRuntime::new(session).unwrap();
            limited.start(Instant::now()).unwrap();
            kernels.insert(first.kernel_id.clone(), limited);
        }

        let err = host
            .execute(
                binding.clone(),
                KernelToolRequest {
                    operation: AnalysisOperation::JsonParse,
                    args: br#"{"long_key":"long_value_1234567890"}"#.to_vec(),
                    logical_name: None,
                    type_hint: None,
                    sensitivity: None,
                },
            )
            .await
            .expect_err("output budget exceeded");
        assert_eq!(
            err.code,
            evohime_tool_runtime::kernel_host::KERNEL_LIMIT_EXCEEDED
        );

        let db = host.journal.database().lock().await;
        let store = AnalysisKernelStore::new(db.connection());
        let saved = store.get_session(&first.kernel_id).unwrap().unwrap();
        assert_eq!(saved.status, KernelStatus::LimitExceeded);
    }

    #[tokio::test]
    async fn parallel_calls_in_same_task_create_single_session() {
        let host = test_host().await;
        let binding = test_binding();
        let req1 = KernelToolRequest {
            operation: AnalysisOperation::JsonParse,
            args: br#"{"a":1}"#.to_vec(),
            logical_name: None,
            type_hint: None,
            sensitivity: None,
        };
        let req2 = KernelToolRequest {
            operation: AnalysisOperation::JsonParse,
            args: br#"{"b":2}"#.to_vec(),
            logical_name: None,
            type_hint: None,
            sensitivity: None,
        };
        let (r1, r2) = tokio::join!(
            host.execute(binding.clone(), req1),
            host.execute(binding.clone(), req2)
        );
        let r1 = r1.expect("first parallel call");
        let r2 = r2.expect("second parallel call");
        assert_eq!(r1.kernel_id, r2.kernel_id);

        let db = host.journal.database().lock().await;
        let store = AnalysisKernelStore::new(db.connection());
        let sessions = store
            .list_sessions_for_task(&binding.task_id.to_string())
            .unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[tokio::test]
    async fn restart_recovery_fences_running_session_and_next_call_creates_new() {
        let host = test_host().await;
        let binding = test_binding();
        let first = host
            .execute(binding.clone(), test_request())
            .await
            .expect("first call");

        host.kernels.lock().await.clear();
        let recovered = host.journal.recover_analysis_kernels().await.unwrap();
        assert_eq!(recovered, 1);

        let second = host
            .execute(
                binding.clone(),
                KernelToolRequest {
                    operation: AnalysisOperation::JsonParse,
                    args: br#"{"after":"restart"}"#.to_vec(),
                    logical_name: None,
                    type_hint: None,
                    sensitivity: None,
                },
            )
            .await
            .expect("next call creates fresh session");
        assert_ne!(first.kernel_id, second.kernel_id);

        let db = host.journal.database().lock().await;
        let store = AnalysisKernelStore::new(db.connection());
        let old = store.get_session(&first.kernel_id).unwrap().unwrap();
        let new = store.get_session(&second.kernel_id).unwrap().unwrap();
        assert_eq!(old.status, KernelStatus::Crashed);
        assert_eq!(new.status, KernelStatus::Running);
    }

    #[tokio::test]
    async fn stop_sessions_for_task_stops_active_session() {
        let host = test_host().await;
        let binding = test_binding();
        let first = host
            .execute(binding.clone(), test_request())
            .await
            .expect("first call");
        host.stop_sessions_for_task(&binding.task_id.to_string())
            .await;

        assert!(!host.kernels.lock().await.contains_key(&first.kernel_id));
        let db = host.journal.database().lock().await;
        let store = AnalysisKernelStore::new(db.connection());
        let saved = store.get_session(&first.kernel_id).unwrap().unwrap();
        assert_eq!(saved.status, KernelStatus::Stopped);
    }
}
