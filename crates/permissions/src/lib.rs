//! Permission checks, scoped overrides, and approval audit (roadmap P2).
//!
//! Global modes persist via `app_settings.permissions`.
//! Session overrides + path grants persist via `app_settings.permission_scopes` (Stage 7.22).

mod pattern;
mod policy;

pub use pattern::glob_match;
pub use policy::{PolicyRule, PolicyRuleSet};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, RwLock};
use tracing;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    FilesystemRead,
    FilesystemWrite,
    ShellExecute,
    GitRead,
    GitWrite,
    BrowserAccess,
    McpCall,
    MemorySearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Ask,
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Allowed,
    NeedsApproval,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPreview {
    pub kind: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default)]
    pub truncated: bool,
}

const MAX_APPROVAL_PREVIEW_TEXT_BYTES: usize = 8 * 1024;

impl ApprovalPreview {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub task_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    pub tool_name: String,
    pub permission: Permission,
    /// Relative path, URL, or `"workspace"`.
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Hash of the tool name, normalized scope, and exact canonical input.
    #[serde(default)]
    pub call_hash: String,
    pub preview: ApprovalPreview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Granted,
    Denied,
}

#[derive(Debug, Clone)]
struct ApprovalRecord {
    request: ApprovalRequest,
    state: ApprovalState,
    #[allow(dead_code)]
    created_at: Instant,
}

/// Context for a scoped permission check.
#[derive(Debug, Clone, Default)]
pub struct PermissionCheck<'a> {
    pub session_id: Option<Uuid>,
    pub path: Option<&'a str>,
    pub command: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathGrant {
    pub permission: Permission,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    pub mode: PermissionMode,
    /// Unix millis; `None` means until cleared / process restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOverride {
    pub session_id: Uuid,
    pub permission: Permission,
    pub mode: PermissionMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalAuditEntry {
    pub approval_id: Uuid,
    pub task_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<Uuid>,
    pub tool_name: String,
    pub permission: Permission,
    pub scope: String,
    pub decision: ApprovalState,
    pub at_ms: u64,
    /// True when grant also installed a temporary path allow for the session.
    #[serde(default)]
    pub remembered_path: bool,
}

/// Durable snapshot of session overrides + path grants (Stage 7.22).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionScopesSnapshot {
    #[serde(default)]
    pub session_overrides: Vec<SessionOverride>,
    #[serde(default)]
    pub path_grants: Vec<PathGrant>,
}

const DEFAULT_TEMP_GRANT_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_AUDIT_ENTRIES: usize = 200;

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

    pub async fn mode(&self, permission: Permission) -> PermissionMode {
        self.modes
            .read()
            .await
            .get(&permission)
            .copied()
            .unwrap_or(PermissionMode::Ask)
    }

    pub async fn set_mode(&self, permission: Permission, mode: PermissionMode) {
        self.modes.write().await.insert(permission, mode);
    }

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

    pub async fn clear_session_mode(&self, session_id: Uuid, permission: Permission) {
        self.session_modes
            .write()
            .await
            .remove(&(session_id, permission));
    }

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
    pub async fn check(&self, permission: Permission) -> PermissionDecision {
        self.check_scoped(permission, &PermissionCheck::default())
            .await
    }

    /// Resolve a scoped decision against a canonical policy subject.
    ///
    /// Callers that accepted a display name or user-facing path must provide
    /// the canonical, resolved subject here so policy rules cannot be bypassed
    /// by presenting a different label to the permission engine.
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

    pub async fn set_policy_rules(&self, rules: PolicyRuleSet) {
        *self.policy_rules.write().await = rules;
    }

    pub async fn policy_rules(&self) -> PolicyRuleSet {
        self.policy_rules.read().await.clone()
    }

    pub async fn create_approval_scoped_for_call(
        &self,
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
        input: &serde_json::Value,
    ) -> ApprovalRequest {
        self.create_approval_scoped_for_call_with_command(
            task_id, session_id, tool_name, permission, scope, None, input,
        )
        .await
    }

    pub async fn create_approval_scoped_for_call_with_command(
        &self,
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
        command: Option<String>,
        input: &serde_json::Value,
    ) -> ApprovalRequest {
        self.create_approval_scoped_for_call_with_command_and_preview(
            task_id,
            session_id,
            tool_name,
            permission,
            scope,
            command,
            input,
            ApprovalPreview::default(),
        )
        .await
    }

    pub async fn create_approval_scoped_for_call_with_command_and_preview(
        &self,
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: impl Into<String>,
        permission: Permission,
        scope: impl Into<String>,
        command: Option<String>,
        input: &serde_json::Value,
        preview: ApprovalPreview,
    ) -> ApprovalRequest {
        let tool_name = tool_name.into();
        let scope = normalize_scope_path(scope.into());
        let call_hash = canonical_call_hash(&tool_name, &scope, input);
        let request = ApprovalRequest {
            id: Uuid::new_v4(),
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
                created_at: Instant::now(),
            },
        );
        self.push_audit(ApprovalAuditEntry {
            approval_id: request.id,
            task_id: request.task_id,
            session_id: request.session_id,
            tool_name: request.tool_name.clone(),
            permission: request.permission,
            scope: request.scope.clone(),
            decision: ApprovalState::Pending,
            at_ms: now_ms(),
            remembered_path: false,
        })
        .await;
        request
    }

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
            decision: state,
            at_ms: now_ms(),
            remembered_path,
        })
        .await;

        Some(state)
    }

    pub async fn approval(&self, id: Uuid) -> Option<(ApprovalRequest, ApprovalState)> {
        self.approvals
            .read()
            .await
            .get(&id)
            .map(|r| (r.request.clone(), r.state))
    }

    pub async fn approval_matches_call(
        &self,
        id: Uuid,
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: &str,
        permission: Permission,
        scope: &str,
        input: &serde_json::Value,
    ) -> Option<ApprovalState> {
        self.approvals.read().await.get(&id).and_then(|record| {
            let request = &record.request;
            (request.task_id == task_id
                && request.session_id == session_id
                && request.tool_name == tool_name
                && request.permission == permission
                && request.scope == normalize_scope_path(scope)
                && request.call_hash == canonical_call_hash(tool_name, &request.scope, input))
            .then_some(record.state)
        })
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
        task_id: Uuid,
        session_id: Option<Uuid>,
        tool_name: &str,
        permission: Permission,
        scope: &str,
        input: &serde_json::Value,
    ) -> Option<ApprovalState> {
        let mut approvals = self.approvals.write().await;
        let matches = approvals.get(&id).is_some_and(|record| {
            let request = &record.request;
            request.task_id == task_id
                && request.session_id == session_id
                && request.tool_name == tool_name
                && request.permission == permission
                && request.scope == normalize_scope_path(scope)
                && request.call_hash == canonical_call_hash(tool_name, &request.scope, input)
        });
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

    pub async fn audit_log(&self) -> Vec<ApprovalAuditEntry> {
        self.audit.read().await.clone()
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
    path.as_ref()
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
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

fn fingerprint_input(input: &serde_json::Value) -> String {
    match input {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
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
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
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

pub fn canonical_call_hash(tool_name: &str, scope: &str, input: &serde_json::Value) -> String {
    let payload = format!("{}\n{}\n{}", tool_name, scope, fingerprint_input(input));
    let digest = Sha256::digest(payload.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;

    #[test]
    fn default_policy_allows_read_and_asks_for_write() {
        block_on(async {
            let engine = PermissionEngine::new();
            assert_eq!(
                engine.check(Permission::FilesystemRead).await,
                PermissionDecision::Allowed
            );
            assert_eq!(
                engine.check(Permission::FilesystemWrite).await,
                PermissionDecision::NeedsApproval
            );
        });
    }

    #[test]
    fn approval_is_one_shot() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let request = engine
                .create_approval(
                    task_id,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    "a.txt",
                )
                .await;
            assert_eq!(request.task_id, task_id);
            assert_eq!(
                engine.resolve(request.id, true).await,
                Some(ApprovalState::Granted)
            );
            assert_eq!(engine.resolve(request.id, false).await, None);
        });
    }

    #[test]
    fn approval_call_hash_is_canonical_and_scope_is_normalized() {
        block_on(async {
            let engine = PermissionEngine::new();
            let input = serde_json::from_str::<serde_json::Value>(
                r#"{"path":".\\src\\main.rs","content":"x","metadata":{"b":2,"a":1}}"#,
            )
            .unwrap();
            let equivalent = serde_json::from_str::<serde_json::Value>(
                r#"{"metadata":{"a":1,"b":2},"content":"x","path":".\\src\\main.rs"}"#,
            )
            .unwrap();
            let request = engine
                .create_approval_scoped_for_call(
                    Uuid::new_v4(),
                    None,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    r#".\src\main.rs"#,
                    &input,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            assert_eq!(
                engine
                    .approval_matches_call(
                        request.id,
                        request.task_id,
                        None,
                        "filesystem.write",
                        Permission::FilesystemWrite,
                        "src/main.rs",
                        &equivalent,
                    )
                    .await,
                Some(ApprovalState::Granted)
            );
        });
    }

    #[test]
    fn claimed_approval_is_removed_atomically() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input = serde_json::json!({"path": "a.txt", "content": "x"});
            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    "a.txt",
                    &input,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            assert_eq!(
                engine
                    .claim_approval_for_call(
                        request.id,
                        task_id,
                        None,
                        "filesystem.write",
                        Permission::FilesystemWrite,
                        "a.txt",
                        &input,
                    )
                    .await,
                Some(ApprovalState::Granted)
            );
            assert_eq!(
                engine
                    .claim_approval_for_call(
                        request.id,
                        task_id,
                        None,
                        "filesystem.write",
                        Permission::FilesystemWrite,
                        "a.txt",
                        &input,
                    )
                    .await,
                None
            );
        });
    }

    #[test]
    fn session_override_beats_global_mode() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            engine
                .set_session_mode(session, Permission::FilesystemWrite, PermissionMode::Allow)
                .await;
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: None,
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Allowed
            );
            assert_eq!(
                engine.check(Permission::FilesystemWrite).await,
                PermissionDecision::NeedsApproval
            );
        });
    }

    #[test]
    fn path_grant_allows_matching_prefix() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            engine
                .set_path_grant(
                    Permission::FilesystemWrite,
                    "docs",
                    PermissionMode::Allow,
                    Some(session),
                    None,
                )
                .await;
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("docs/readme.md"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Allowed
            );
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("src/main.rs"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::NeedsApproval
            );
        });
    }

    #[test]
    fn grant_remembers_path_for_session() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            let task = Uuid::new_v4();
            let request = engine
                .create_approval_scoped(
                    task,
                    Some(session),
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    "tmp/note.txt",
                )
                .await;
            engine
                .resolve_with_options(request.id, true, true)
                .await
                .expect("granted");

            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("tmp/note.txt"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Allowed
            );

            let audit = engine.audit_log().await;
            assert!(audit.iter().any(|entry| {
                entry.decision == ApprovalState::Granted && entry.remembered_path
            }));
            assert!(audit
                .iter()
                .any(|entry| entry.decision == ApprovalState::Pending));
        });
    }

    #[test]
    fn audit_sender_receives_pending_and_resolved() {
        block_on(async {
            let engine = PermissionEngine::new();
            let (tx, mut rx) = mpsc::unbounded_channel();
            engine.attach_audit_sender(tx).await;

            let request = engine
                .create_approval(
                    Uuid::new_v4(),
                    "shell.execute",
                    Permission::ShellExecute,
                    "workspace",
                )
                .await;
            engine.resolve(request.id, false).await.expect("denied");

            let pending = rx.recv().await.expect("pending audit");
            assert_eq!(pending.decision, ApprovalState::Pending);
            assert_eq!(pending.approval_id, request.id);

            let denied = rx.recv().await.expect("denied audit");
            assert_eq!(denied.decision, ApprovalState::Denied);
            assert_eq!(denied.approval_id, request.id);
        });
    }

    #[test]
    fn path_deny_overrides_session_allow() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            engine
                .set_session_mode(session, Permission::ShellExecute, PermissionMode::Allow)
                .await;
            engine
                .set_path_grant(
                    Permission::ShellExecute,
                    "secrets",
                    PermissionMode::Deny,
                    Some(session),
                    None,
                )
                .await;
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::ShellExecute,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("secrets/key.env"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
        });
    }

    #[test]
    fn scopes_snapshot_roundtrip_preserves_grants() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            engine
                .set_session_mode(session, Permission::FilesystemWrite, PermissionMode::Allow)
                .await;
            engine
                .set_path_grant(
                    Permission::FilesystemWrite,
                    "src",
                    PermissionMode::Allow,
                    Some(session),
                    Some(Duration::from_secs(3_600)),
                )
                .await;
            engine
                .set_path_grant(
                    Permission::ShellExecute,
                    "scripts",
                    PermissionMode::Deny,
                    None,
                    None,
                )
                .await;

            let snapshot = engine.export_scopes().await;
            let restored = PermissionEngine::new();
            restored.import_scopes(snapshot).await;

            assert_eq!(
                restored
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("src/main.rs"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Allowed
            );
            assert_eq!(
                restored
                    .check_scoped(
                        Permission::ShellExecute,
                        &PermissionCheck {
                            session_id: None,
                            path: Some("scripts/run.sh"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
            assert_eq!(restored.list_session_overrides().await.len(), 1);
            assert_eq!(restored.list_path_grants().await.len(), 2);
        });
    }

    #[test]
    fn import_scopes_skips_expired_path_grants() {
        block_on(async {
            let session = Uuid::new_v4();
            let snapshot = PermissionScopesSnapshot {
                session_overrides: vec![],
                path_grants: vec![PathGrant {
                    permission: Permission::FilesystemWrite,
                    path: "tmp".into(),
                    session_id: Some(session),
                    mode: PermissionMode::Allow,
                    expires_at_ms: Some(1), // far in the past
                }],
            };
            let engine = PermissionEngine::new();
            engine.import_scopes(snapshot).await;
            assert!(engine.list_path_grants().await.is_empty());
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("tmp/a.txt"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::NeedsApproval
            );
        });
    }

    #[test]
    fn hard_policy_deny_beats_runtime_grants_and_session_modes() {
        block_on(async {
            let session = Uuid::new_v4();
            let engine = PermissionEngine::new();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::ShellExecute,
                    pattern: "rm *".into(),
                    mode: PermissionMode::Deny,
                }]))
                .await;
            engine
                .set_session_mode(session, Permission::ShellExecute, PermissionMode::Allow)
                .await;
            engine
                .set_path_grant(
                    Permission::ShellExecute,
                    "workspace",
                    PermissionMode::Allow,
                    Some(session),
                    None,
                )
                .await;
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::ShellExecute,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("workspace"),
                            command: Some("rm -rf target"),
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
        });
    }

    #[test]
    fn policy_deny_overrides_path_grant() {
        block_on(async {
            let session = Uuid::new_v4();
            let engine = PermissionEngine::new();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::ShellExecute,
                    pattern: "rm *".into(),
                    mode: PermissionMode::Deny,
                }]))
                .await;
            engine
                .set_path_grant(
                    Permission::ShellExecute,
                    "workspace",
                    PermissionMode::Allow,
                    Some(session),
                    None,
                )
                .await;
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::ShellExecute,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("workspace"),
                            command: Some("rm -rf /"),
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
        });
    }

    #[test]
    fn policy_deny_overrides_session_mode() {
        block_on(async {
            let session = Uuid::new_v4();
            let engine = PermissionEngine::new();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::ShellExecute,
                    pattern: "rm *".into(),
                    mode: PermissionMode::Deny,
                }]))
                .await;
            engine
                .set_session_mode(session, Permission::ShellExecute, PermissionMode::Allow)
                .await;
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::ShellExecute,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: None,
                            command: Some("rm target/debug"),
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
        });
    }

    #[test]
    fn path_grant_beats_allow_policy() {
        block_on(async {
            let engine = PermissionEngine::new();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::FilesystemWrite,
                    pattern: "docs/*".into(),
                    mode: PermissionMode::Allow,
                }]))
                .await;
            // Global mode is Ask for FilesystemWrite
            assert_eq!(
                engine.mode(Permission::FilesystemWrite).await,
                PermissionMode::Ask
            );
            // Policy rule says Allow
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: None,
                            path: Some("docs/readme.md"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Allowed
            );
            // But matching path grant takes precedence
            let session = Uuid::new_v4();
            engine
                .set_path_grant(
                    Permission::FilesystemWrite,
                    "docs",
                    PermissionMode::Deny,
                    Some(session),
                    None,
                )
                .await;
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: Some(session),
                            path: Some("docs/readme.md"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
        });
    }

    #[test]
    fn missing_policy_rule_uses_global_mode() {
        block_on(async {
            let engine = PermissionEngine::new();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::FilesystemWrite,
                    pattern: "secrets/*".into(),
                    mode: PermissionMode::Deny,
                }]))
                .await;
            // Matching rule denies
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: None,
                            path: Some("secrets/key.env"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::Denied
            );
            // Non-matching path falls back to global mode (Ask for FilesystemWrite)
            assert_eq!(
                engine
                    .check_scoped(
                        Permission::FilesystemWrite,
                        &PermissionCheck {
                            session_id: None,
                            path: Some("src/main.rs"),
                            command: None,
                        },
                    )
                    .await,
                PermissionDecision::NeedsApproval
            );
        });
    }

    #[test]
    fn command_field_defaults_to_none_in_existing_checks() {
        block_on(async {
            let engine = PermissionEngine::new();
            let session = Uuid::new_v4();
            engine
                .set_path_grant(
                    Permission::FilesystemWrite,
                    "docs",
                    PermissionMode::Allow,
                    Some(session),
                    None,
                )
                .await;
            // Existing code using PermissionCheck without command should still work
            let check = PermissionCheck {
                session_id: Some(session),
                path: Some("docs/readme.md"),
                // command is not specified, defaults to None
                ..Default::default()
            };
            assert_eq!(
                engine
                    .check_scoped(Permission::FilesystemWrite, &check)
                    .await,
                PermissionDecision::Allowed
            );
        });
    }

    #[test]
    fn canonical_policy_subject_cannot_be_replaced_by_display_name() {
        block_on(async {
            let engine = PermissionEngine::new();
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::FilesystemRead,
                    pattern: "c:/workspace/secrets/*".into(),
                    mode: PermissionMode::Deny,
                }]))
                .await;

            let check = PermissionCheck {
                path: Some("friendly-name/token.txt"),
                ..PermissionCheck::default()
            };
            assert_eq!(
                engine
                    .check_scoped_with_subject(
                        Permission::FilesystemRead,
                        &check,
                        r"C:\workspace\secrets\token.txt",
                    )
                    .await,
                PermissionDecision::Denied
            );
            assert_eq!(
                engine
                    .check_scoped(Permission::FilesystemRead, &check)
                    .await,
                PermissionDecision::Allowed
            );
        });
    }

    #[test]
    fn approval_matches_detects_different_inputs() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input1 = serde_json::json!({"cmd": "cargo", "args": ["--version"]});
            let input2 = serde_json::json!({"cmd": "cargo", "args": ["publish"]});

            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "shell.execute",
                    Permission::ShellExecute,
                    "workspace",
                    &input1,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            // Same hash matches
            assert!(
                engine
                    .approval_matches(request.id, &request.call_hash)
                    .await
            );

            // Different input has different hash
            let different_hash = canonical_call_hash("shell.execute", "workspace", &input2);
            assert_ne!(request.call_hash, different_hash);
            assert!(!engine.approval_matches(request.id, &different_hash).await);
        });
    }

    #[test]
    fn approval_matches_rejects_denied_approval() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input = serde_json::json!({"cmd": "rm", "args": ["-rf", "/"]});

            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "shell.execute",
                    Permission::ShellExecute,
                    "workspace",
                    &input,
                )
                .await;
            engine.resolve(request.id, false).await.expect("denied");

            // Denied approval should not match even with correct hash
            assert!(
                !engine
                    .approval_matches(request.id, &request.call_hash)
                    .await
            );
        });
    }

    #[test]
    fn approval_matches_rejects_pending_approval() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input = serde_json::json!({"path": "file.txt", "content": "hello"});

            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    "file.txt",
                    &input,
                )
                .await;

            // Pending approval should not match
            assert!(
                !engine
                    .approval_matches(request.id, &request.call_hash)
                    .await
            );
        });
    }

    #[test]
    fn approval_for_git_commit_rejects_different_message() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input1 = serde_json::json!({"message": "feat: add new feature"});
            let input2 = serde_json::json!({"message": "fix: repair broken thing"});

            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "git.commit",
                    Permission::GitWrite,
                    "workspace",
                    &input1,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            // Different commit message has different hash
            let different_hash = canonical_call_hash("git.commit", "workspace", &input2);
            assert!(!engine.approval_matches(request.id, &different_hash).await);
        });
    }

    #[test]
    fn approval_for_file_write_rejects_different_content() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input1 = serde_json::json!({"path": "src/main.rs", "content": "fn main() {}"});
            let input2 =
                serde_json::json!({"path": "src/main.rs", "content": "fn main() { panic!(); }"});

            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    "src/main.rs",
                    &input1,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            // Same path but different content should not match
            let different_hash = canonical_call_hash("filesystem.write", "src/main.rs", &input2);
            assert!(!engine.approval_matches(request.id, &different_hash).await);
        });
    }

    #[test]
    fn call_hash_is_stable_across_json_key_order() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input1 = serde_json::json!({"b": 2, "a": 1, "c": {"z": 26, "x": 24}});
            let input2 = serde_json::json!({"c": {"x": 24, "z": 26}, "a": 1, "b": 2});

            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "mcp.call",
                    Permission::McpCall,
                    "workspace",
                    &input1,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            // Same input with different key order should have same hash
            let equivalent_hash = canonical_call_hash("mcp.call", "workspace", &input2);
            assert_eq!(request.call_hash, equivalent_hash);
            assert!(engine.approval_matches(request.id, &equivalent_hash).await);
        });
    }

    #[test]
    fn hard_deny_policy_stops_granted_approval_execution() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input = serde_json::json!({"cmd": "rm -rf /"});

            // Create and grant approval
            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "shell.execute",
                    Permission::ShellExecute,
                    "workspace",
                    &input,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            // Approval matches initially
            assert!(
                engine
                    .approval_matches(request.id, &request.call_hash)
                    .await
            );

            // Now add a hard deny policy for this command
            engine
                .set_policy_rules(PolicyRuleSet::new(vec![PolicyRule {
                    permission: Permission::ShellExecute,
                    pattern: "rm *".into(),
                    mode: PermissionMode::Deny,
                }]))
                .await;

            // Check with same input still shows approval matches
            // (approval_matches only checks approval state, not policy)
            assert!(
                engine
                    .approval_matches(request.id, &request.call_hash)
                    .await
            );

            // But check_scoped now denies due to hard policy
            let check = PermissionCheck {
                session_id: None,
                path: Some("workspace"),
                command: Some("rm -rf /"),
            };
            assert_eq!(
                engine.check_scoped(Permission::ShellExecute, &check).await,
                PermissionDecision::Denied
            );
        });
    }

    #[test]
    fn approval_scope_normalization_consistent_with_hash() {
        block_on(async {
            let engine = PermissionEngine::new();
            let task_id = Uuid::new_v4();
            let input = serde_json::json!({"content": "test"});

            // Create approval with backslashes
            let request = engine
                .create_approval_scoped_for_call(
                    task_id,
                    None,
                    "filesystem.write",
                    Permission::FilesystemWrite,
                    r".\src\main.rs",
                    &input,
                )
                .await;
            engine.resolve(request.id, true).await.expect("granted");

            // The request should have normalized scope (converted to forward slashes)
            assert!(request.scope.contains("src/main.rs"));

            // And hash should match
            let expected_hash = canonical_call_hash("filesystem.write", &request.scope, &input);
            assert_eq!(request.call_hash, expected_hash);
            assert!(engine.approval_matches(request.id, &expected_hash).await);
        });
    }

    #[test]
    fn approval_matches_returns_false_for_nonexistent_id() {
        block_on(async {
            let engine = PermissionEngine::new();
            let fake_id = Uuid::new_v4();
            let call_hash = "some_hash".to_string();

            assert!(!engine.approval_matches(fake_id, &call_hash).await);
        });
    }
}
