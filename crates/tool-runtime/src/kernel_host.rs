//! Kernel host contract used by the declared `analysis.run` tool.
//!
//! The tool runtime only knows this trait and the bounded request/result
//! vocabulary below. Session ownership, admission, limits, idempotency and
//! recovery stay in Core; the runtime never reaches SQLite or the supervisor
//! pipe directly. When no host is attached the tool fails closed with a typed
//! `analysis_kernel_unavailable` error instead of executing in process.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use uuid::Uuid;

/// Typed error code: the registry has no Core-owned kernel host attached.
pub const KERNEL_UNAVAILABLE: &str = "analysis_kernel_unavailable";
/// Typed error code: a kernel limit (`cpu`, `output`, rate, lifetime) was hit.
pub const KERNEL_LIMIT_EXCEEDED: &str = "analysis_kernel_limit_exceeded";
/// Typed error code: the requested operation needs a capability the kernel denies.
pub const KERNEL_FORBIDDEN_CAPABILITY: &str = "analysis_kernel_forbidden_capability";
/// Typed error code: the session is fenced after a crash and needs an explicit reset.
pub const KERNEL_CRASHED: &str = "analysis_kernel_crashed";
/// Typed error code: inline arguments carried a sensitive marker.
pub const KERNEL_SENSITIVE_INLINE_PAYLOAD: &str = "analysis_kernel_sensitive_inline_payload";
/// Typed error code: the kernel rejected the payload shape for the operation.
pub const KERNEL_INVALID_INPUT: &str = "analysis_kernel_invalid_input";
/// Typed error code: the bounded inline result cannot be returned safely.
pub const KERNEL_OUTPUT_TOO_LARGE: &str = "analysis_kernel_output_too_large";
/// Typed error code: persistent storage behind the kernel failed.
pub const KERNEL_STORAGE_FAILED: &str = "analysis_kernel_storage_failed";

/// Operations the agent may request. Everything else stays unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisOperation {
    /// Parses JSON text and returns its normalized form.
    JsonParse,
    /// Selects one value from a JSON document by key path.
    JsonSelect,
    /// Counts columns and rows of CSV text.
    CsvSummary,
    /// Stores a bounded ephemeral object and returns its reference.
    ObjectPut,
}

impl AnalysisOperation {
    /// Returns the stable snake-case operation name used by Core and IPC.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JsonParse => "json_parse",
            Self::JsonSelect => "json_select",
            Self::CsvSummary => "csv_summary",
            Self::ObjectPut => "object_put",
        }
    }

    /// Parses a stable snake-case operation name.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "json_parse" => Some(Self::JsonParse),
            "json_select" => Some(Self::JsonSelect),
            "csv_summary" => Some(Self::CsvSummary),
            "object_put" => Some(Self::ObjectPut),
            _ => None,
        }
    }
}

/// Sensitivity a caller may request for a stored object.
///
/// `secret` is intentionally absent: the kernel rejects secret objects, so the
/// contract fails closed before an effect instead of accepting then refusing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisSensitivity {
    /// Unrestricted local display.
    Public,
    /// Application-internal data.
    Internal,
    /// Data that must not be inlined into provider requests.
    Sensitive,
}

impl AnalysisSensitivity {
    /// Returns the stable snake-case sensitivity name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Sensitive => "sensitive",
        }
    }

    /// Parses a stable snake-case sensitivity name.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "internal" => Some(Self::Internal),
            "sensitive" => Some(Self::Sensitive),
            _ => None,
        }
    }
}

/// Core-owned identity of the task that requested the kernel call.
///
/// Every value is computed by Core from the task context; the model cannot
/// supply or override it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelTaskBinding {
    /// Task that owns the kernel session.
    pub task_id: Uuid,
    /// Optional permission session of the task.
    pub session_id: Option<Uuid>,
    /// Absolute workspace root; the host derives the workspace id from it.
    pub workspace_root: PathBuf,
}

/// Bounded request handed to the Core-owned kernel host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelToolRequest {
    /// Operation admitted by the tool schema.
    pub operation: AnalysisOperation,
    /// Canonical JSON payload for the operation, at most 16 KiB.
    pub args: Vec<u8>,
    /// Optional logical name for `object_put`.
    pub logical_name: Option<String>,
    /// Optional type hint for `object_put`.
    pub type_hint: Option<String>,
    /// Optional requested sensitivity for `object_put`.
    pub sensitivity: Option<AnalysisSensitivity>,
}

/// Bounded metadata reference returned for `object_put`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelToolObjectRef {
    /// Stable object reference identifier.
    pub id: String,
    /// Logical name chosen by the caller.
    pub logical_name: String,
    /// Type label supplied by the caller.
    pub type_hint: String,
    /// Object size in bytes.
    pub size: u64,
    /// Stored sensitivity classification.
    pub sensitivity: String,
    /// Whether the object is ephemeral or artifact-backed.
    pub persistence: String,
    /// Optional content hash for the object bytes.
    pub content_hash: Option<String>,
}

/// Bounded successful response returned by the kernel host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelToolResult {
    /// Session that produced the result.
    pub kernel_id: String,
    /// Inline JSON result, present for compute operations.
    pub inline_result: Option<Vec<u8>>,
    /// Object reference, present for `object_put`.
    pub object_ref: Option<KernelToolObjectRef>,
    /// Sensitivity classification of the produced value.
    pub sensitivity: String,
    /// Provenance label recorded by Core.
    pub provenance: String,
}

/// Typed failure returned by the kernel host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelToolError {
    /// Stable typed code from this module.
    pub code: String,
    /// Bounded human-readable detail.
    pub message: String,
}

impl KernelToolError {
    /// Creates a typed kernel error.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for KernelToolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            formatter.write_str(&self.code)
        } else {
            write!(formatter, "{}: {}", self.code, self.message)
        }
    }
}

impl std::error::Error for KernelToolError {}

/// Core-owned execution boundary for declared analysis kernel calls.
///
/// Implementations must reuse the existing kernel runtime, session storage and
/// supervisor worker. They must not create a second runtime or bypass
/// `PermissionEngine`.
pub trait KernelHost: Send + Sync {
    /// Executes one bounded kernel request for the given task binding.
    fn execute<'a>(
        &'a self,
        task: KernelTaskBinding,
        request: KernelToolRequest,
    ) -> Pin<Box<dyn Future<Output = Result<KernelToolResult, KernelToolError>> + Send + 'a>>;
}
