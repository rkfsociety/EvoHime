#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
//! Tool manifests, execution policy, registries, and built-in tool adapters.
//!
//! The crate exposes capability checks and metadata used by Core to prepare,
//! authorize, and run tools. Concrete adapters are grouped by tool category.

/// Action lifecycle contracts and console-facing requests.
pub mod action;
/// Installed application catalog and executable resolution.
pub mod app_catalog;
/// Developer-oriented utility tools.
pub mod developer_utilities;
/// Named execution policy profiles and their resolution.
pub mod execution_policy_profiles;
/// Tool manifest schema and validation contracts.
pub mod manifest;
/// Network access capability and host allow-list controls.
pub mod network_capability;
mod registry;
/// Workspace file operations guarded against revision races.
pub mod revision_safe_workspace_files;
mod risk;
mod sandbox;
mod shell_env;
mod ssrf;
/// Bounded tool lifecycle telemetry.
pub mod telemetry;
/// Catalog and status for available toolkits.
pub mod toolkit;
mod tools;

/// Public action execution contracts.
pub use action::{ActionConsole, ActionRequest, ActionStatus};
/// Application catalog types and catalog filename.
pub use app_catalog::{AppCatalog, AppEntry, Resolution as AppResolution, CATALOG_FILE_NAME};
/// Execution policy profile types and errors.
pub use execution_policy_profiles::{
    ExecutionPolicyError, ExecutionPolicyProfile, ResolvedExecutionProfile,
};
/// Manifest validation, side effect, and origin contracts.
pub use manifest::{
    builtin_input_schema, ApprovalMode, ManifestError, SideEffectClass, ToolManifest, ToolOrigin,
    MANIFEST_KIND,
};
/// Registry, tool context, preflight, and result contracts.
pub use registry::{
    ApprovalRequired, ToolContext, ToolDefinition, ToolError, ToolPreflightDecision, ToolProgress,
    ToolRegistry, ToolResult,
};
/// Tool risk classification API.
pub use risk::{classify_call_risk, ToolRiskLevel};
/// Workspace sandbox used to scope filesystem access.
pub use sandbox::WorkspaceSandbox;
/// SSRF defenses and process-wide network capability controls.
pub use ssrf::{
    allow_private_targets, assert_safe_http_url, effective_host_allowlist, host_allowlist_from_env,
    lock_host_allowlist, lock_private_override, HostAllowlistGuard, PrivateOverrideGuard,
};
/// Tool lifecycle telemetry API.
pub use telemetry::{TelemetryBuffer, TelemetrySummary, ToolLifecycle, ToolTelemetryEvent};
/// Toolkit catalog types and status.
pub use toolkit::{ToolkitCatalog, ToolkitEntry, ToolkitError, ToolkitStatus};
/// Built-in agent tool adapter.
pub use tools::agent;
/// Built-in archive tool adapter.
pub use tools::archive;
/// Built-in browser tool adapter.
pub use tools::browser;
/// Built-in Cargo tool adapter.
pub use tools::cargo;
/// Built-in filesystem tool adapter.
pub use tools::filesystem;
/// Extended filesystem tool adapter.
pub use tools::filesystem_advanced;
/// Built-in Git tool adapter.
pub use tools::git;
/// Extended Git tool adapter.
pub use tools::git_advanced;
/// Log inspection tool adapter.
pub use tools::logs;
/// Model Context Protocol tool adapter.
pub use tools::mcp;
/// Project memory tool adapter.
pub use tools::memory;
/// Process management tool adapter.
pub use tools::process;
/// Patch, search, shell, and file-writing adapters.
pub use tools::{patch, search, shell, write};
