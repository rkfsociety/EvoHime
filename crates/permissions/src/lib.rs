#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![deny(missing_docs)]
//! Permission checks, scoped overrides, and approval audit (roadmap P2).
//!
//! Global modes persist via `app_settings.permissions`.
//! Session overrides + path grants persist via `app_settings.permission_scopes` (Stage 7.22).
//!
//! ```
//! use evohime_permissions::{glob_match, Permission, PermissionMode, PolicyRule, PolicyRuleSet};
//!
//! let rules = PolicyRuleSet::new(vec![PolicyRule {
//!     permission: Permission::FilesystemRead,
//!     pattern: "*.env".into(),
//!     mode: PermissionMode::Deny,
//! }]);
//! assert!(glob_match("*.env", "backend/.env"));
//! assert_eq!(rules.resolve(Permission::FilesystemRead, "backend/.env"), Some(PermissionMode::Deny));
//! ```

mod pattern;
mod policy;

/// Case-insensitive wildcard matcher used by permission policy rules.
pub use pattern::glob_match;
/// Permission rule and ordered rule-set types.
pub use policy::{PolicyRule, PolicyRuleSet};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

/// Capability category controlled by the permission engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read files within an authorized workspace.
    FilesystemRead,
    /// Create, modify, or delete workspace files.
    FilesystemWrite,
    /// Execute a shell command.
    ShellExecute,
    /// Inspect repository state and history.
    GitRead,
    /// Change repository state or publish changes.
    GitWrite,
    /// Access an interactive browser session.
    BrowserAccess,
    /// Call a Model Context Protocol server.
    McpCall,
    /// Search stored project memory.
    MemorySearch,
    /// Ambient microphone capture (plan 04).  Default `Deny`; never touched by
    /// [`PermissionEngine::set_all_modes`].
    MicrophoneListen,
}

/// Configured outcome when a permission check reaches this mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Require an approval for each matching operation.
    Ask,
    /// Permit the operation subject to harder policy denies.
    Allow,
    /// Reject the operation without requesting approval.
    Deny,
}

/// Result of resolving a permission check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    /// The operation is allowed by the effective policy.
    Allowed,
    /// The operation requires an approval token.
    NeedsApproval,
    /// The operation is denied.
    Denied,
}

/// Bounded user-facing description attached to an approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPreview {
    /// Short operation category shown in the approval UI.
    pub kind: String,
    /// Human-readable summary of the requested operation.
    pub summary: String,
    /// Optional shell command associated with the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Optional working directory for the command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Optional filesystem path affected by the operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional additional context shown to the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Whether one or more preview fields were shortened to the size limit.
    #[serde(default)]
    pub truncated: bool,
}

const MAX_APPROVAL_PREVIEW_TEXT_BYTES: usize = 8 * 1024;

impl ApprovalPreview {
    /// Truncates every text field to the approval preview size limit.
    pub fn bounded(mut self) -> Self {
        let (kind, kind_truncated) = bound_preview_text(self.kind);
        let (summary, summary_truncated) = bound_preview_text(self.summary);
        let (command, command_truncated) = bound_preview_option(self.command);
        let (cwd, cwd_truncated) = bound_preview_option(self.cwd);
        let (path, path_truncated) = bound_preview_option(self.path);
        let (details, details_truncated) = bound_preview_option(self.details);
        self.kind = kind;
        self.summary = summary;
        self.command = command;
        self.cwd = cwd;
        self.path = path;
        self.details = details;
        self.truncated |= kind_truncated
            || summary_truncated
            || command_truncated
            || cwd_truncated
            || path_truncated
            || details_truncated;
        self
    }
}

fn bound_preview_option(value: Option<String>) -> (Option<String>, bool) {
    value.map_or((None, false), |value| {
        let (value, truncated) = bound_preview_text(value);
        (Some(value), truncated)
    })
}

fn bound_preview_text(value: String) -> (String, bool) {
    let mut end = value.len().min(MAX_APPROVAL_PREVIEW_TEXT_BYTES);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), end < value.len())
}

impl Default for ApprovalPreview {
    fn default() -> Self {
        Self {
            kind: "operation".to_string(),
            summary: "Операция требует разрешения".to_string(),
            command: None,
            cwd: None,
            path: None,
            details: None,
            truncated: false,
        }
    }
}

/// Идентичность вызова, к которой привязан approval.
///
/// Отдельный тип, а не пять позиционных аргументов: approval сверяется с
/// вызовом целиком, и перепутанные местами `tool_name`/`scope` дали бы
/// «совпадение» там, где его нет.
#[derive(Debug, Clone, Copy)]
pub struct CallIdentity<'a> {
    /// Task that requested the tool call.
    pub task_id: Uuid,
    /// Session associated with the task, when available.
    pub session_id: Option<Uuid>,
    /// Name of the tool being called.
    pub tool_name: &'a str,
    /// Capability required by the tool call.
    pub permission: Permission,
    /// Resource scope checked by the permission engine.
    pub scope: &'a str,
    /// Exact JSON input supplied to the tool.
    pub input: &'a serde_json::Value,
}

impl CallIdentity<'_> {
    /// Совпадает ли выданный approval с этим вызовом. Scope сравнивается
    /// нормализованным, вход — каноническим хешем, как при выдаче.
    fn matches(&self, request: &ApprovalRequest) -> bool {
        request.task_id == self.task_id
            && request.session_id == self.session_id
            && request.tool_name == self.tool_name
            && request.permission == self.permission
            && request.scope == normalize_scope_path(self.scope)
            && request.call_hash == canonical_call_hash(self.tool_name, &request.scope, self.input)
    }
}

/// Approval request bound to a task, capability, scope, and tool input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique approval identifier.
    pub id: Uuid,
    /// Task that owns this request.
    pub task_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Session that owns this request, when available.
    pub session_id: Option<Uuid>,
    /// Name of the tool awaiting approval.
    pub tool_name: String,
    /// Capability required by the operation.
    pub permission: Permission,
    /// Relative path, URL, or `"workspace"`.
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional command associated with the operation.
    pub command: Option<String>,
    /// Hash of the tool name, normalized scope, and exact canonical input.
    #[serde(default)]
    pub call_hash: String,
    /// Bounded description displayed to the user.
    pub preview: ApprovalPreview,
}

/// Lifecycle state of an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    /// Awaiting a user decision.
    Pending,
    /// Approved by the user.
    Granted,
    /// Rejected by the user.
    Denied,
}

#[derive(Debug, Clone)]
struct ApprovalRecord {
    request: ApprovalRequest,
    state: ApprovalState,
}

/// Context for a scoped permission check.
#[derive(Debug, Clone, Default)]
pub struct PermissionCheck<'a> {
    /// Session used to resolve session-specific overrides.
    pub session_id: Option<Uuid>,
    /// Optional resource path used to resolve path grants.
    pub path: Option<&'a str>,
    /// Optional command used to match command policy rules.
    pub command: Option<&'a str>,
}

/// Persistable permission grant scoped to a resource path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathGrant {
    /// Capability granted or denied for this path.
    pub permission: Permission,
    /// Normalized resource path covered by the grant.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Session to which this grant applies, or `None` for all sessions.
    pub session_id: Option<Uuid>,
    /// Decision applied to matching checks.
    pub mode: PermissionMode,
    /// Unix millis; `None` means until cleared / process restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

/// Persistable permission override scoped to one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOverride {
    /// Session receiving the override.
    pub session_id: Uuid,
    /// Capability affected by the override.
    pub permission: Permission,
    /// Decision used for this capability in the session.
    pub mode: PermissionMode,
}

/// Recorded decision retained for approval auditing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalAuditEntry {
    /// Approval request identifier.
    pub approval_id: Uuid,
    /// Task that requested the operation.
    pub task_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Session associated with the operation, when available.
    pub session_id: Option<Uuid>,
    /// Name of the tool that requested access.
    pub tool_name: String,
    /// Capability checked by the engine.
    pub permission: Permission,
    /// Resource scope evaluated by the engine.
    pub scope: String,
    /// Exact canonical call binding retained for offline approval audit.
    pub call_hash: String,
    /// Final approval decision.
    pub decision: ApprovalState,
    /// Unix timestamp in milliseconds when the decision was recorded.
    pub at_ms: u64,
    /// True when grant also installed a temporary path allow for the session.
    #[serde(default)]
    pub remembered_path: bool,
}

/// Durable snapshot of session overrides + path grants (Stage 7.22).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionScopesSnapshot {
    /// Session-specific capability overrides.
    #[serde(default)]
    pub session_overrides: Vec<SessionOverride>,
    /// Path-specific grants.
    #[serde(default)]
    pub path_grants: Vec<PathGrant>,
}

const DEFAULT_TEMP_GRANT_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_AUDIT_ENTRIES: usize = 200;

/// Concurrent permission policy, approvals, scoped grants, and audit log.
#[derive(Clone)]
pub struct PermissionEngine {
    modes: Arc<RwLock<HashMap<Permission, PermissionMode>>>,
    policy_rules: Arc<RwLock<PolicyRuleSet>>,
    session_modes: Arc<RwLock<HashMap<(Uuid, Permission), PermissionMode>>>,
    path_grants: Arc<RwLock<Vec<StoredPathGrant>>>,
    approvals: Arc<RwLock<HashMap<Uuid, ApprovalRecord>>>,
    audit: Arc<RwLock<Vec<ApprovalAuditEntry>>>,
    /// Optional durable sink for Core's local SQLite event journal.
    audit_tx: Arc<RwLock<Option<mpsc::UnboundedSender<ApprovalAuditEntry>>>>,
}

#[derive(Debug, Clone)]
struct StoredPathGrant {
    permission: Permission,
    path: String,
    session_id: Option<Uuid>,
    mode: PermissionMode,
    expires_at: Option<Instant>,
}

impl Default for PermissionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionEngine {
    /// Creates an engine with the built-in default modes.
    pub fn new() -> Self {
        let mut modes = HashMap::new();
        modes.insert(Permission::FilesystemRead, PermissionMode::Allow);
        modes.insert(Permission::FilesystemWrite, PermissionMode::Ask);
        modes.insert(Permission::ShellExecute, PermissionMode::Ask);
        modes.insert(Permission::GitRead, PermissionMode::Allow);
        modes.insert(Permission::GitWrite, PermissionMode::Ask);
        modes.insert(Permission::BrowserAccess, PermissionMode::Ask);
        modes.insert(Permission::McpCall, PermissionMode::Ask);
        modes.insert(Permission::MemorySearch, PermissionMode::Ask);
        // Explicit, not omitted: `mode()` falls back to `Ask` for a missing
        // key, so "not listed" would mean "prompt for the microphone".
        modes.insert(Permission::MicrophoneListen, PermissionMode::Deny);
        Self {
            modes: Arc::new(RwLock::new(modes)),
            policy_rules: Arc::new(RwLock::new(PolicyRuleSet::default())),
            session_modes: Arc::new(RwLock::new(HashMap::new())),
            path_grants: Arc::new(RwLock::new(Vec::new())),
            approvals: Arc::new(RwLock::new(HashMap::new())),
            audit: Arc::new(RwLock::new(Vec::new())),
            audit_tx: Arc::new(RwLock::new(None)),
        }
    }

    /// Attach an unbounded sender used to persist audit entries (Stage 7.23).
    pub async fn attach_audit_sender(&self, tx: mpsc::UnboundedSender<ApprovalAuditEntry>) {
        *self.audit_tx.write().await = Some(tx);
    }

    /// Returns the configured global mode for a capability.
    pub async fn mode(&self, permission: Permission) -> PermissionMode {
        self.modes
            .read()
            .await
            .get(&permission)
            .copied()
            .unwrap_or(PermissionMode::Ask)
    }

    /// Sets the global mode for one capability.
    pub async fn set_mode(&self, permission: Permission, mode: PermissionMode) {
        self.modes.write().await.insert(permission, mode);
    }

    /// Sets the global mode for every permission **except**
    /// [`Permission::MicrophoneListen`].
    ///
    /// The exclusion lives here and only here.  Electron re-sends the stored
    /// workspace mode on every workspace open, and the `PermissionMode` branch
    /// in `ipc_bridge.rs` calls this for any value (`full` → `Allow`,
    /// `read_only` → `Deny`, anything else → `Ask`), so without the exclusion
    /// a routine mode change would silently open the microphone.
    /// Sets the same global mode for every capability except microphone capture.
    pub async fn set_all_modes(&self, mode: PermissionMode) {
        let mut modes = self.modes.write().await;
        for permission in [
            Permission::FilesystemRead,
            Permission::FilesystemWrite,
            Permission::ShellExecute,
            Permission::GitRead,
            Permission::GitWrite,
            Permission::BrowserAccess,
            Permission::McpCall,
            Permission::MemorySearch,
        ] {
            modes.insert(permission, mode);
        }
    }

    /// Sets a session-specific mode for one capability.
    pub async fn set_session_mode(
        &self,
        session_id: Uuid,
        permission: Permission,
        mode: PermissionMode,
    ) {
        self.session_modes
            .write()
            .await
            .insert((session_id, permission), mode);
    }

    /// Removes a session-specific mode and restores inherited behavior.
    pub async fn clear_session_mode(&self, session_id: Uuid, permission: Permission) {
        self.session_modes
            .write()
            .await
            .remove(&(session_id, permission));
    }

    /// Adds or replaces a path grant, optionally scoped to a session and TTL.
    pub async fn set_path_grant(
        &self,
        permission: Permission,
        path: impl Into<String>,
        mode: PermissionMode,
        session_id: Option<Uuid>,
        ttl: Option<Duration>,
    ) {
        let path = normalize_scope_path(path.into());
        let expires_at = ttl.map(|duration| Instant::now() + duration);
        let mut grants = self.path_grants.write().await;
        grants.retain(|grant| {
            !(grant.permission == permission
                && grant.path == path
                && grant.session_id == session_id)
        });
        grants.push(StoredPathGrant {
            permission,
            path,
            session_id,
            mode,
            expires_at,
        });
    }

    /// Removes a matching path grant.
    pub async fn clear_path_grant(
        &self,
        permission: Permission,
        path: &str,
        session_id: Option<Uuid>,
    ) {
        let path = normalize_scope_path(path);
        self.path_grants.write().await.retain(|grant| {
            !(grant.permission == permission
                && grant.path == path
                && grant.session_id == session_id)
        });
    }

    /// Global-only check (settings / legacy callers).
    /// Resolves a capability using its global mode and policy rules.
    pub async fn check(&self, permission: Permission) -> PermissionDecision {
        self.check_scoped(permission, &PermissionCheck::default())
            .await
    }

    /// Resolve a scoped decision against a canonical policy subject.
    ///
    /// Callers that accepted a display name or user-facing path must provide
    /// the canonical, resolved subject here so policy rules cannot be bypassed
    /// by presenting a different label to the permission engine.
    /// Resolves a scoped capability check against the supplied subject.
    pub async fn check_scoped_with_subject(
        &self,
        permission: Permission,
        check: &PermissionCheck<'_>,
        canonical_subject: &str,
    ) -> PermissionDecision {
        self.check_scoped_inner(permission, check, Some(canonical_subject))
            .await
    }

    /// Resolve decision with path/session overrides.
    ///
    /// Priority (most specific first):
    /// 1. a matching hard policy `Deny` (cannot be overridden)
    /// 2. matching path grant (session-scoped preferred over global)
    /// 3. session permission mode
    /// 4. matching policy `Allow`/`Ask`
    /// 5. global mode
    /// Resolves a capability check using the path and command in `check`.
    pub async fn check_scoped(
        &self,
        permission: Permission,
        check: &PermissionCheck<'_>,
    ) -> PermissionDecision {
        self.check_scoped_inner(permission, check, None).await
    }

    async fn check_scoped_inner(
        &self,
        permission: Permission,
        check: &PermissionCheck<'_>,
        canonical_subject: Option<&str>,
    ) -> PermissionDecision {
        self.purge_expired_grants().await;

        let subject = canonical_subject
            .map(normalize_scope_path)
            .or_else(|| {
                check
                    .command
                    .map(str::to_owned)
                    .or_else(|| check.path.map(normalize_scope_path))
            })
            .unwrap_or_else(|| "workspace".to_string());
        let policy_mode = self.policy_rules.read().await.resolve(permission, &subject);
        if policy_mode == Some(PermissionMode::Deny) {
            tracing::trace!(
                permission = ?permission,
                subject = ?subject,
                "permission denied by hard policy rule"
            );
            return PermissionDecision::Denied;
        }

        if let Some(path) = check.path.map(normalize_scope_path) {
            if let Some(mode) = self
                .find_path_mode(permission, &path, check.session_id)
                .await
            {
                tracing::trace!(
                    permission = ?permission,
                    path = ?path,
                    session_id = ?check.session_id,
                    mode = ?mode,
                    "permission resolved by path grant"
                );
                return mode_to_decision(mode);
            }
        }

        if let Some(session_id) = check.session_id {
            if let Some(mode) = self
                .session_modes
                .read()
                .await
                .get(&(session_id, permission))
                .copied()
            {
                tracing::trace!(
                    permission = ?permission,
                    session_id = ?session_id,
                    mode = ?mode,
                    "permission resolved by session override"
                );
                return mode_to_decision(mode);
            }
        }

        if let Some(mode) = policy_mode {
            tracing::trace!(
                permission = ?permission,
                subject = ?subject,
                mode = ?mode,
                "permission resolved by policy rule"
            );
            return mode_to_decision(mode);
        }

        let mode = self.mode(permission).await;
        tracing::trace!(
            permission = ?permission,
            mode = ?mode,
            "permission resolved by global mode"
        );
        mode_to_decision(mode)
    }

    /// Creates an approval request without session or input binding.
    pub async fn create_approval(
        &self,
        task_id: Uuid,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
    ) -> ApprovalRequest {
        self.create_approval_scoped_for_call(
            task_id,
            None,
            tool_name,
            permission,
            scope,
            &serde_json::Value::Null,
        )
        .await
    }

    /// Creates an approval request scoped to an optional session.
    pub async fn create_approval_scoped(
        &self,
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
    ) -> ApprovalRequest {
        self.create_approval_scoped_for_call(
            task_id,
            session_id,
            tool_name,
            permission,
            scope,
            &serde_json::Value::Null,
        )
        .await
    }

    /// Replaces the ordered permission policy rules.
    pub async fn set_policy_rules(&self, rules: PolicyRuleSet) {
        *self.policy_rules.write().await = rules;
    }

    /// Returns a clone of the active ordered policy rules.
    pub async fn policy_rules(&self) -> PolicyRuleSet {
        self.policy_rules.read().await.clone()
    }

    /// Creates an approval request cryptographically bound to the exact call.
    pub async fn create_approval_scoped_for_call(
        &self,
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
        input: &serde_json::Value,
    ) -> ApprovalRequest {
        let tool_name = tool_name.into();
        let scope = scope.into();
        self.create_approval_scoped_for_call_with_command(
            CallIdentity {
                task_id,
                session_id,
                tool_name: &tool_name,
                permission,
                scope: &scope,
                input,
            },
            None,
        )
        .await
    }

    /// Creates a call-bound approval with an optional command preview.
    pub async fn create_approval_scoped_for_call_with_command(
        &self,
        call: CallIdentity<'_>,
        command: Option<String>,
    ) -> ApprovalRequest {
        self.create_approval_scoped_for_call_with_command_and_preview(
            call,
            command,
            ApprovalPreview::default(),
        )
        .await
    }

    /// Creates a call-bound approval with command and bounded UI preview.
    pub async fn create_approval_scoped_for_call_with_command_and_preview(
        &self,
        call: CallIdentity<'_>,
        command: Option<String>,
        preview: ApprovalPreview,
    ) -> ApprovalRequest {
        let CallIdentity {
            task_id,
            session_id,
            permission,
            input,
            ..
        } = call;
        let tool_name = call.tool_name.to_string();
        let scope = normalize_scope_path(call.scope);
        let call_hash = canonical_call_hash(&tool_name, &scope, input);
        let request = ApprovalRequest {
            id: Uuid::now_v7(),
            task_id,
            session_id,
            tool_name,
            permission,
            scope,
            command,
            call_hash,
            preview: preview.bounded(),
        };
        self.approvals.write().await.insert(
            request.id,
            ApprovalRecord {
                request: request.clone(),
                state: ApprovalState::Pending,
            },
        );
        self.push_audit(ApprovalAuditEntry {
            approval_id: request.id,
            task_id: request.task_id,
            session_id: request.session_id,
            tool_name: request.tool_name.clone(),
            permission: request.permission,
            scope: request.scope.clone(),
            call_hash: request.call_hash.clone(),
            decision: ApprovalState::Pending,
            at_ms: now_ms(),
            remembered_path: false,
        })
        .await;
        request
    }

    /// Records the user's decision for a pending approval request.
    pub async fn resolve(&self, id: Uuid, granted: bool) -> Option<ApprovalState> {
        self.resolve_with_options(id, granted, false).await
    }

    /// Resolve an approval. When `remember_path` is true and the grant succeeds,
    /// installs a session-scoped temporary allow for the approval scope path.
    pub async fn resolve_with_options(
        &self,
        id: Uuid,
        granted: bool,
        remember_path: bool,
    ) -> Option<ApprovalState> {
        let mut approvals = self.approvals.write().await;
        let record = approvals.get_mut(&id)?;
        if record.state != ApprovalState::Pending {
            return None;
        }
        let request = record.request.clone();
        record.state = if granted {
            ApprovalState::Granted
        } else {
            ApprovalState::Denied
        };
        let state = record.state;
        drop(approvals);

        let mut remembered_path = false;
        if granted && remember_path && is_rememberable_scope(&request.scope) {
            if let Some(session_id) = request.session_id {
                self.set_path_grant(
                    request.permission,
                    &request.scope,
                    PermissionMode::Allow,
                    Some(session_id),
                    Some(DEFAULT_TEMP_GRANT_TTL),
                )
                .await;
                remembered_path = true;
            }
        }

        self.push_audit(ApprovalAuditEntry {
            approval_id: request.id,
            task_id: request.task_id,
            session_id: request.session_id,
            tool_name: request.tool_name,
            permission: request.permission,
            scope: request.scope,
            call_hash: request.call_hash,
            decision: state,
            at_ms: now_ms(),
            remembered_path,
        })
        .await;

        Some(state)
    }

    /// Returns an approval request and its current state, if it exists.
    pub async fn approval(&self, id: Uuid) -> Option<(ApprovalRequest, ApprovalState)> {
        self.approvals
            .read()
            .await
            .get(&id)
            .map(|r| (r.request.clone(), r.state))
    }

    /// Verifies an approval against a call and returns its current state.
    pub async fn approval_matches_call(
        &self,
        id: Uuid,
        call: CallIdentity<'_>,
    ) -> Option<ApprovalState> {
        self.approvals
            .read()
            .await
            .get(&id)
            .and_then(|record| call.matches(&record.request).then_some(record.state))
    }

    /// Atomically claim a granted approval for execution.
    ///
    /// Approval tokens are deliberately consumed before the tool starts. This
    /// prevents concurrent or replayed requests from executing the same
    /// approved mutation twice. A failed tool call must request a fresh
    /// approval rather than silently reusing the old authorization.
    pub async fn claim_approval_for_call(
        &self,
        id: Uuid,
        call: CallIdentity<'_>,
    ) -> Option<ApprovalState> {
        let mut approvals = self.approvals.write().await;
        let matches = approvals
            .get(&id)
            .is_some_and(|record| call.matches(&record.request));
        if !matches {
            return None;
        }

        let state = approvals.get(&id).map(|record| record.state)?;
        if state == ApprovalState::Granted {
            approvals.remove(&id);
        }
        Some(state)
    }

    /// Check if approval matches a specific call based on call_hash alone.
    /// Returns true if the approval exists, is in Granted state, and the call_hash matches.
    pub async fn approval_matches(&self, id: Uuid, call_hash: &str) -> bool {
        if let Some(record) = self.approvals.read().await.get(&id) {
            record.state == ApprovalState::Granted && record.request.call_hash == call_hash
        } else {
            false
        }
    }

    /// Returns all persisted session-specific capability overrides.
    pub async fn list_session_overrides(&self) -> Vec<SessionOverride> {
        self.session_modes
            .read()
            .await
            .iter()
            .map(|((session_id, permission), mode)| SessionOverride {
                session_id: *session_id,
                permission: *permission,
                mode: *mode,
            })
            .collect()
    }

    /// Returns active path grants as persistable records.
    pub async fn list_path_grants(&self) -> Vec<PathGrant> {
        self.purge_expired_grants().await;
        let now = Instant::now();
        self.path_grants
            .read()
            .await
            .iter()
            .map(|grant| PathGrant {
                permission: grant.permission,
                path: grant.path.clone(),
                session_id: grant.session_id,
                mode: grant.mode,
                expires_at_ms: grant.expires_at.map(|at| {
                    let remaining = at.saturating_duration_since(now);
                    now_ms().saturating_add(remaining.as_millis() as u64)
                }),
            })
            .collect()
    }

    /// Export session overrides + path grants for durable storage.
    pub async fn export_scopes(&self) -> PermissionScopesSnapshot {
        PermissionScopesSnapshot {
            session_overrides: self.list_session_overrides().await,
            path_grants: self.list_path_grants().await,
        }
    }

    /// Replace in-memory session overrides + path grants from a durable snapshot.
    /// Expired path grants are skipped.
    pub async fn import_scopes(&self, snapshot: PermissionScopesSnapshot) {
        let mut session_modes = HashMap::new();
        for override_item in snapshot.session_overrides {
            session_modes.insert(
                (override_item.session_id, override_item.permission),
                override_item.mode,
            );
        }
        *self.session_modes.write().await = session_modes;

        let wall_now = now_ms();
        let instant_now = Instant::now();
        let mut grants = Vec::with_capacity(snapshot.path_grants.len());
        for grant in snapshot.path_grants {
            let expires_at = match grant.expires_at_ms {
                Some(ms) if ms <= wall_now => continue,
                Some(ms) => Some(instant_now + Duration::from_millis(ms.saturating_sub(wall_now))),
                None => None,
            };
            grants.push(StoredPathGrant {
                permission: grant.permission,
                path: normalize_scope_path(grant.path),
                session_id: grant.session_id,
                mode: grant.mode,
                expires_at,
            });
        }
        *self.path_grants.write().await = grants;
    }

    /// Returns the bounded in-memory approval audit history.
    pub async fn audit_log(&self) -> Vec<ApprovalAuditEntry> {
        self.audit.read().await.clone()
    }

    /// The single normative entry point for scope normalization referenced by
    /// the receipt runtime (01.3): rejects an embedded `\n` (framing
    /// invariant shared with `canonical_call_hash`), then normalizes
    /// filesystem-path-shaped scopes via the existing path rules and leaves
    /// non-path scopes (URLs, opaque tool identifiers) trimmed but otherwise
    /// unchanged. Callers must not normalize scope themselves before hashing.
    pub fn normalize_scope(&self, scope: &str) -> Result<String, String> {
        if scope.contains('\n') {
            return Err("scope must not contain a newline".to_owned());
        }
        let trimmed = scope.trim();
        if trimmed.contains('/') || trimmed.contains('\\') {
            Ok(normalize_scope_path(trimmed))
        } else {
            Ok(trimmed.to_owned())
        }
    }

    async fn find_path_mode(
        &self,
        permission: Permission,
        path: &str,
        session_id: Option<Uuid>,
    ) -> Option<PermissionMode> {
        let grants = self.path_grants.read().await;
        let mut best: Option<(usize, PermissionMode)> = None;
        for grant in grants.iter() {
            if grant.permission != permission {
                continue;
            }
            if let Some(expires_at) = grant.expires_at {
                if Instant::now() >= expires_at {
                    continue;
                }
            }
            if !path_matches(&grant.path, path) {
                continue;
            }
            // Prefer session-scoped grants when session matches; skip foreign session grants.
            match (grant.session_id, session_id) {
                (Some(grant_session), Some(session)) if grant_session == session => {
                    let rank = 1_000 + grant.path.len();
                    if best.map(|(r, _)| rank > r).unwrap_or(true) {
                        best = Some((rank, grant.mode));
                    }
                }
                (Some(_), _) => continue,
                (None, _) => {
                    let rank = grant.path.len();
                    if best.map(|(r, _)| rank > r).unwrap_or(true) {
                        best = Some((rank, grant.mode));
                    }
                }
            }
        }
        best.map(|(_, mode)| mode)
    }

    async fn purge_expired_grants(&self) {
        let now = Instant::now();
        self.path_grants.write().await.retain(|grant| {
            grant
                .expires_at
                .map(|expires_at| now < expires_at)
                .unwrap_or(true)
        });
    }

    async fn push_audit(&self, entry: ApprovalAuditEntry) {
        {
            let mut audit = self.audit.write().await;
            audit.push(entry.clone());
            if audit.len() > MAX_AUDIT_ENTRIES {
                let overflow = audit.len() - MAX_AUDIT_ENTRIES;
                audit.drain(0..overflow);
            }
        }
        if let Some(tx) = self.audit_tx.read().await.as_ref() {
            let _ = tx.send(entry);
        }
    }
}

fn mode_to_decision(mode: PermissionMode) -> PermissionDecision {
    match mode {
        PermissionMode::Allow => PermissionDecision::Allowed,
        PermissionMode::Ask => PermissionDecision::NeedsApproval,
        PermissionMode::Deny => PermissionDecision::Denied,
    }
}

fn normalize_scope_path(path: impl AsRef<str>) -> String {
    let normalized = path
        .as_ref()
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string();

    // `Path::canonicalize` on Windows may return the NT extended-length
    // spelling (`//?/C:/...`), while user policy files normally contain the
    // regular drive spelling (`C:/...`). Treat both as the same subject so a
    // hard deny cannot depend on which spelling the filesystem API returned.
    normalized
        .strip_prefix("//?/")
        .or_else(|| normalized.strip_prefix("//./"))
        .unwrap_or(&normalized)
        .to_string()
}

fn is_rememberable_scope(scope: &str) -> bool {
    let scope = scope.trim();
    !scope.is_empty()
        && scope != "workspace"
        && !scope.starts_with("http://")
        && !scope.starts_with("https://")
}

fn path_matches(grant_path: &str, request_path: &str) -> bool {
    if grant_path == request_path {
        return true;
    }
    // Prefix grant: `src/` covers `src/lib.rs`.
    let prefix = if grant_path.ends_with('/') {
        grant_path.to_string()
    } else {
        format!("{grant_path}/")
    };
    request_path.starts_with(&prefix)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Version of the [`fingerprint_input`] typed-projection rules. Recorded
/// alongside every receipt action (`receipt_actions.fingerprint_input_version`
/// in `crates/evohime-receipts`) so a future rule change cannot silently
/// reinterpret an already-durable hash. Must stay equal to
/// `fingerprint_input_version` in `contracts/receipts/v1/limits.json`
/// (cross-checked by a test in `crates/evohime-receipts`).
pub const FINGERPRINT_INPUT_VERSION: u8 = 1;

/// Largest magnitude integer exactly representable as an IEEE-754 binary64 /
/// JCS number (2^53 - 1). Integers outside this range are wrapped in a typed
/// object instead of being written as a bare JSON number, so a JS-based
/// verifier can never silently round them.
const SAFE_INTEGER_BOUND: i64 = 9_007_199_254_740_991;

/// Produces a deterministic JSON fingerprint for binding approvals to inputs.
pub fn fingerprint_input(input: &serde_json::Value) -> String {
    match input {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => fingerprint_number(value),
        serde_json::Value::String(value) => serde_json::to_string(value).unwrap_or_default(),
        serde_json::Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(fingerprint_input)
                .collect::<Vec<_>>()
                .join(",")
        ),
        serde_json::Value::Object(values) => {
            if let (
                Some(serde_json::Value::String(encoding)),
                Some(serde_json::Value::String(value)),
                Some(serde_json::Value::String(kind)),
            ) = (
                values.get("encoding"),
                values.get("value"),
                values.get("type"),
            ) {
                if kind == "bytes" && encoding == "base64url" && is_unpadded_base64url(value) {
                    return format!(
                        "{{\"type\":\"bytes\",\"encoding\":\"base64url\",\"value\":{}}}",
                        serde_json::to_string(value).unwrap_or_default()
                    );
                }
            }
            let mut keys = values.keys().collect::<Vec<_>>();
            // RFC 8785 JCS orders object keys by UTF-16 code unit, not by
            // Rust's default UTF-8 byte order — the two differ for
            // characters outside the Basic Multilingual Plane.
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| {
                        format!(
                            "{}:{}",
                            serde_json::to_string(key).unwrap_or_default(),
                            fingerprint_input(&values[key])
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}

/// `serde_json::Value` cannot hold `NaN`/`Infinity` (parsing rejects both),
/// so no explicit fail-closed branch is needed for them here — the
/// unreachable case is enforced by the type itself, not by runtime code.
///
/// Note: `serde_json::Value` also has no bytes/binary variant, so the plan's
/// `{"type":"bytes","encoding":"base64url","value":...}` binary projection
/// is not a distinct code path — a caller that has already base64url-encoded
/// binary data into a plain JSON object of that shape is fingerprinted by
/// the generic object rule above and produces the identical bytes.
fn fingerprint_number(value: &serde_json::Number) -> String {
    if let Some(int_value) = value.as_i64() {
        if (-SAFE_INTEGER_BOUND..=SAFE_INTEGER_BOUND).contains(&int_value) {
            return int_value.to_string();
        }
        return typed_int64(int_value.to_string());
    }
    if let Some(uint_value) = value.as_u64() {
        if uint_value <= SAFE_INTEGER_BOUND as u64 {
            return uint_value.to_string();
        }
        return typed_int64(uint_value.to_string());
    }
    let Some(float_value) = value.as_f64() else {
        return value.to_string();
    };
    if float_value == 0.0 {
        return "0".to_string();
    }
    if float_value.is_finite() && float_value.fract() == 0.0 && float_value.abs() < 1e21 {
        return format!("{:.0}", float_value);
    }
    value.to_string()
}

fn is_unpadded_base64url(value: &str) -> bool {
    !value.contains('=')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn typed_int64(decimal: String) -> String {
    // Keys are fixed and already in JCS UTF-16 order ("type" < "value").
    format!(
        "{{\"type\":\"int64\",\"value\":{}}}",
        serde_json::to_string(&decimal).unwrap_or_default()
    )
}

/// Hashes a tool name, scope, and canonical input into an approval binding.
pub fn canonical_call_hash(tool_name: &str, scope: &str, input: &serde_json::Value) -> String {
    let payload = format!("{}\n{}\n{}", tool_name, scope, fingerprint_input(input));
    let digest = Sha256::digest(payload.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
