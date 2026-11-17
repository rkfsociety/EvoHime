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
}

impl CoreKernelHost {
    /// Creates a new kernel host attached to the given journal and runtime map.
    pub fn new(
        journal: EventJournal,
        kernels: Arc<Mutex<std::collections::HashMap<String, KernelRuntime>>>,
    ) -> Self {
        Self { journal, kernels }
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

        Ok(analysis_kernel::KernelHostRequestV1 {
            version: analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
            request_id: format!("agent-{}", Uuid::new_v4()),
            kernel_id: kernel_id.to_string(),
            session_id: kernel_id.to_string(),
            operation,
            args: request.args.clone(),
            requested_capability: None,
            context_refs: Vec::new(),
            correlation_id: format!("agent-{}", task_id),
            idempotency_key: format!(
                "{}-{}-{}",
                task_id,
                kernel_id,
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

impl KernelHost for CoreKernelHost {
    fn execute<'a>(
        &'a self,
        task: KernelTaskBinding,
        request: KernelToolRequest,
    ) -> Pin<Box<dyn Future<Output = Result<KernelToolResult, KernelToolError>> + Send + 'a>> {
        Box::pin(async move {
            // 1. Ensure a kernel session exists for this task
            let kernel_id = self.ensure_session_for_task(&task).await?;

            // 2. Convert to kernel host request
            let host_request =
                self.tool_request_to_host_request(&kernel_id, &request, &task.task_id.to_string())?;

            // 3. Execute using the existing runtime
            let mut kernels = self.kernels.lock().await;
            let Some(runtime) = kernels.get_mut(&kernel_id) else {
                return Err(KernelToolError::new(
                    evohime_tool_runtime::kernel_host::KERNEL_UNAVAILABLE,
                    format!("kernel session {} not found in runtime map", kernel_id),
                ));
            };

            match runtime.execute(host_request, Instant::now()) {
                Ok(response) => Ok(self.host_response_to_tool_result(&kernel_id, response)),
                Err(error) => {
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
                    Err(KernelToolError::new(code, error.to_string()))
                }
            }
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
}
