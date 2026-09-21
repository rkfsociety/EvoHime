use evohime_permissions::{ApprovalPreview, Permission, PermissionDecision, PermissionEngine};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, time::Duration};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::tools;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid input for {tool}: {message}")]
    InvalidInput { tool: String, message: String },
    #[error("permission denied: {0:?}")]
    PermissionDenied(Permission),
    #[error("resource not found for {tool}: {path}{hint}")]
    NotFound {
        tool: String,
        path: String,
        hint: String,
    },
    #[error("approval required for {}: {}", .0.tool, .0.approval_id)]
    NeedsApproval(Box<ApprovalRequired>),
    #[error("approval does not match this call")]
    ApprovalMismatch,
    #[error("approval was denied for this call")]
    ApprovalDenied,
    #[error("tool execution failed: {0}")]
    Execution(String),
    #[error("tool timed out after {0:?}")]
    TimedOut(Duration),
}

/// Payload of [`ToolError::NeedsApproval`].
///
/// Boxed inside the error on purpose: it carries the whole tool input and the
/// approval preview, so inlining it would put ~250 bytes on every
/// `Result<_, ToolError>` in the crate, including the happy path.
#[derive(Debug, Clone)]
pub struct ApprovalRequired {
    pub tool: String,
    pub permission: Permission,
    pub scope: String,
    pub approval_id: uuid::Uuid,
    pub input: Value,
    pub preview: ApprovalPreview,
}

#[derive(Debug, Clone)]
pub struct ToolProgress {
    pub stream: &'static str,
    pub delta: String,
}

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace_root: PathBuf,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    /// Optional live progress channel (e.g. shell stdout/stderr chunks).
    pub progress_tx: Option<tokio::sync::mpsc::UnboundedSender<ToolProgress>>,
}

impl ToolContext {
    pub fn sandbox(&self) -> Result<crate::WorkspaceSandbox, ToolError> {
        crate::WorkspaceSandbox::new(&self.workspace_root)
    }

    pub fn emit_progress(&self, stream: &'static str, delta: impl Into<String>) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(ToolProgress {
                stream,
                delta: delta.into(),
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub structured: Value,
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub permissions: &'static [Permission],
    pub timeout: Duration,
}

impl ToolDefinition {
    /// Статический adapter для legacy builtin-регистраций. Поля registry
    /// остаются source-of-truth до полной миграции деклараций инструментов;
    /// схема при этом всегда явная и fail-closed.
    pub fn manifest(&self) -> crate::ToolManifest {
        let is_utility = self.name.starts_with("utility.");
        crate::ToolManifest {
            kind: crate::MANIFEST_KIND.into(),
            tool_id: self.name.into(),
            version: "1.0.0".into(),
            display_name: self.name.into(),
            description: self.description.into(),
            input_schema: crate::builtin_input_schema(self.name),
            output_schema: serde_json::json!({"type":"object"}),
            capability_class: if is_utility {
                "local_computation".into()
            } else {
                self.permissions
                    .first()
                    .map(|p| format!("{p:?}"))
                    .unwrap_or_else(|| "none".into())
            },
            side_effect: if self.permissions.iter().any(|p| {
                matches!(
                    p,
                    Permission::FilesystemWrite | Permission::ShellExecute | Permission::GitWrite
                )
            }) {
                crate::SideEffectClass::Mutating
            } else {
                crate::SideEffectClass::ReadOnly
            },
            provider_identity: "builtin".into(),
            required_permissions: self.permissions.to_vec(),
            approval: if self.permissions.is_empty() {
                crate::ApprovalMode::Never
            } else {
                crate::ApprovalMode::OnPermission
            },
            workspace_scope: "workspace".into(),
            network_domains: vec![],
            secret_references: vec![],
            timeout_ms: self.timeout.as_millis().min(u64::MAX as u128) as u64,
            output_size_limit: if is_utility {
                crate::developer_utilities::MAX_OUTPUT_BYTES as u64
            } else {
                512 * 1024
            },
            retry_class: "bounded".into(),
            supports_cancellation: true,
            origin: crate::ToolOrigin::Builtin,
            source_reference: "evohime://builtin".into(),
            package_hash: None,
            license: None,
            compatible_core: ">=0.1".into(),
            protocol_version: "1".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ToolPreflightDecision {
    Allowed {
        scope: String,
        preview: ApprovalPreview,
    },
    Denied(Permission),
    ApprovalRequired {
        permission: Permission,
        scope: String,
        preview: ApprovalPreview,
    },
}

#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<&'static str, ToolDefinition>,
    manifest_cache: HashMap<&'static str, crate::ToolManifest>,
    permissions: PermissionEngine,
}

impl ToolRegistry {
    pub fn bootstrap() -> Self {
        Self::bootstrap_with_permissions(PermissionEngine::new())
    }

    pub fn bootstrap_with_permissions(permissions: PermissionEngine) -> Self {
        let mut registry = Self::with_permissions(permissions);
        registry.register(ToolDefinition {
            name: tools::agent::NAME,
            description: tools::agent::DESCRIPTION,
            permissions: tools::agent::PERMISSIONS,
            timeout: tools::agent::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::filesystem::NAME,
            description: tools::filesystem::DESCRIPTION,
            permissions: tools::filesystem::PERMISSIONS,
            timeout: tools::filesystem::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::write::NAME,
            description: tools::write::DESCRIPTION,
            permissions: tools::write::PERMISSIONS,
            timeout: tools::write::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::patch::NAME,
            description: tools::patch::DESCRIPTION,
            permissions: tools::patch::PERMISSIONS,
            timeout: tools::patch::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::search::NAME,
            description: tools::search::DESCRIPTION,
            permissions: tools::search::PERMISSIONS,
            timeout: tools::search::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::list::NAME,
            description: tools::list::DESCRIPTION,
            permissions: tools::list::PERMISSIONS,
            timeout: tools::list::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::shell::NAME,
            description: tools::shell::DESCRIPTION,
            permissions: tools::shell::PERMISSIONS,
            timeout: tools::shell::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::STATUS_NAME,
            description: tools::git::STATUS_DESCRIPTION,
            permissions: tools::git::STATUS_PERMISSIONS,
            timeout: tools::git::STATUS_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::DIFF_NAME,
            description: tools::git::DIFF_DESCRIPTION,
            permissions: tools::git::DIFF_PERMISSIONS,
            timeout: tools::git::DIFF_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::COMMIT_NAME,
            description: tools::git::COMMIT_DESCRIPTION,
            permissions: tools::git::COMMIT_PERMISSIONS,
            timeout: tools::git::COMMIT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::PULL_NAME,
            description: tools::git::PULL_DESCRIPTION,
            permissions: tools::git::PULL_PERMISSIONS,
            timeout: tools::git::PULL_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::PUSH_NAME,
            description: tools::git::PUSH_DESCRIPTION,
            permissions: tools::git::PUSH_PERMISSIONS,
            timeout: tools::git::PUSH_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::LOG_NAME,
            description: tools::git::LOG_DESCRIPTION,
            permissions: tools::git::LOG_PERMISSIONS,
            timeout: tools::git::LOG_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::SHOW_NAME,
            description: tools::git::SHOW_DESCRIPTION,
            permissions: tools::git::SHOW_PERMISSIONS,
            timeout: tools::git::SHOW_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::BLAME_NAME,
            description: tools::git::BLAME_DESCRIPTION,
            permissions: tools::git::BLAME_PERMISSIONS,
            timeout: tools::git::BLAME_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git::CHANGED_FILES_NAME,
            description: tools::git::CHANGED_FILES_DESCRIPTION,
            permissions: tools::git::CHANGED_FILES_PERMISSIONS,
            timeout: tools::git::CHANGED_FILES_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::mcp::NAME,
            description: tools::mcp::DESCRIPTION,
            permissions: tools::mcp::PERMISSIONS,
            timeout: tools::mcp::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::memory::NAME,
            description: tools::memory::DESCRIPTION,
            permissions: tools::memory::PERMISSIONS,
            timeout: tools::memory::TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser::OPEN_NAME,
            description: tools::browser::OPEN_DESCRIPTION,
            permissions: tools::browser::OPEN_PERMISSIONS,
            timeout: tools::browser::OPEN_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser::EXTRACT_NAME,
            description: tools::browser::EXTRACT_DESCRIPTION,
            permissions: tools::browser::EXTRACT_PERMISSIONS,
            timeout: tools::browser::EXTRACT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::NAVIGATE_NAME,
            description: tools::browser_session::NAVIGATE_DESCRIPTION,
            permissions: tools::browser_session::NAVIGATE_PERMISSIONS,
            timeout: tools::browser_session::NAVIGATE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::READ_NAME,
            description: tools::browser_session::READ_DESCRIPTION,
            permissions: tools::browser_session::READ_PERMISSIONS,
            timeout: tools::browser_session::READ_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::CLICK_NAME,
            description: tools::browser_session::CLICK_DESCRIPTION,
            permissions: tools::browser_session::CLICK_PERMISSIONS,
            timeout: tools::browser_session::CLICK_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::SCREENSHOT_NAME,
            description: tools::browser_session::SCREENSHOT_DESCRIPTION,
            permissions: tools::browser_session::SCREENSHOT_PERMISSIONS,
            timeout: tools::browser_session::SCREENSHOT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::TYPE_NAME,
            description: tools::browser_session::TYPE_DESCRIPTION,
            permissions: tools::browser_session::TYPE_PERMISSIONS,
            timeout: tools::browser_session::TYPE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::browser_session::CLOSE_NAME,
            description: tools::browser_session::CLOSE_DESCRIPTION,
            permissions: tools::browser_session::CLOSE_PERMISSIONS,
            timeout: tools::browser_session::CLOSE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::http::NAME,
            description: tools::http::DESCRIPTION,
            permissions: tools::http::PERMISSIONS,
            timeout: tools::http::TIMEOUT,
        });

        // ======== Advanced Git Operations ========
        registry.register(ToolDefinition {
            name: tools::git_advanced::BRANCH_NAME,
            description: tools::git_advanced::BRANCH_DESCRIPTION,
            permissions: tools::git_advanced::BRANCH_PERMISSIONS,
            timeout: tools::git_advanced::BRANCH_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::MERGE_NAME,
            description: tools::git_advanced::MERGE_DESCRIPTION,
            permissions: tools::git_advanced::MERGE_PERMISSIONS,
            timeout: tools::git_advanced::MERGE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::RESET_NAME,
            description: tools::git_advanced::RESET_DESCRIPTION,
            permissions: tools::git_advanced::RESET_PERMISSIONS,
            timeout: tools::git_advanced::RESET_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::REVERT_NAME,
            description: tools::git_advanced::REVERT_DESCRIPTION,
            permissions: tools::git_advanced::REVERT_PERMISSIONS,
            timeout: tools::git_advanced::REVERT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::CHERRY_PICK_NAME,
            description: tools::git_advanced::CHERRY_PICK_DESCRIPTION,
            permissions: tools::git_advanced::CHERRY_PICK_PERMISSIONS,
            timeout: tools::git_advanced::CHERRY_PICK_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::REBASE_NAME,
            description: tools::git_advanced::REBASE_DESCRIPTION,
            permissions: tools::git_advanced::REBASE_PERMISSIONS,
            timeout: tools::git_advanced::REBASE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::TAG_NAME,
            description: tools::git_advanced::TAG_DESCRIPTION,
            permissions: tools::git_advanced::TAG_PERMISSIONS,
            timeout: tools::git_advanced::TAG_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::STASH_NAME,
            description: tools::git_advanced::STASH_DESCRIPTION,
            permissions: tools::git_advanced::STASH_PERMISSIONS,
            timeout: tools::git_advanced::STASH_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_advanced::REMOTE_NAME,
            description: tools::git_advanced::REMOTE_DESCRIPTION,
            permissions: tools::git_advanced::REMOTE_PERMISSIONS,
            timeout: tools::git_advanced::REMOTE_TIMEOUT,
        });

        // ======== Advanced Filesystem Operations ========
        registry.register(ToolDefinition {
            name: tools::filesystem_advanced::DELETE_NAME,
            description: tools::filesystem_advanced::DELETE_DESCRIPTION,
            permissions: tools::filesystem_advanced::DELETE_PERMISSIONS,
            timeout: tools::filesystem_advanced::DELETE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_worktree::CREATE_NAME,
            description: tools::git_worktree::CREATE_DESCRIPTION,
            permissions: tools::git_worktree::CREATE_PERMISSIONS,
            timeout: tools::git_worktree::CREATE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_worktree::REMOVE_NAME,
            description: tools::git_worktree::REMOVE_DESCRIPTION,
            permissions: tools::git_worktree::REMOVE_PERMISSIONS,
            timeout: tools::git_worktree::REMOVE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::git_worktree::PREFLIGHT_NAME,
            description: tools::git_worktree::PREFLIGHT_DESCRIPTION,
            permissions: tools::git_worktree::PREFLIGHT_PERMISSIONS,
            timeout: tools::git_worktree::PREFLIGHT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::filesystem_advanced::MOVE_NAME,
            description: tools::filesystem_advanced::MOVE_DESCRIPTION,
            permissions: tools::filesystem_advanced::MOVE_PERMISSIONS,
            timeout: tools::filesystem_advanced::MOVE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::filesystem_advanced::COPY_NAME,
            description: tools::filesystem_advanced::COPY_DESCRIPTION,
            permissions: tools::filesystem_advanced::COPY_PERMISSIONS,
            timeout: tools::filesystem_advanced::COPY_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::filesystem_advanced::STAT_NAME,
            description: tools::filesystem_advanced::STAT_DESCRIPTION,
            permissions: tools::filesystem_advanced::STAT_PERMISSIONS,
            timeout: tools::filesystem_advanced::STAT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::filesystem_advanced::MKDIR_NAME,
            description: tools::filesystem_advanced::MKDIR_DESCRIPTION,
            permissions: tools::filesystem_advanced::MKDIR_PERMISSIONS,
            timeout: tools::filesystem_advanced::MKDIR_TIMEOUT,
        });

        // ======== Desktop Applications ========
        registry.register(ToolDefinition {
            name: tools::app::OPEN_NAME,
            description: tools::app::OPEN_DESCRIPTION,
            permissions: tools::app::OPEN_PERMISSIONS,
            timeout: tools::app::OPEN_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::app::LIST_NAME,
            description: tools::app::LIST_DESCRIPTION,
            permissions: tools::app::LIST_PERMISSIONS,
            timeout: tools::app::LIST_TIMEOUT,
        });

        // ======== Process Operations ========
        registry.register(ToolDefinition {
            name: tools::process::NAME,
            description: tools::process::DESCRIPTION,
            permissions: tools::process::PERMISSIONS,
            timeout: tools::process::TIMEOUT,
        });

        // ======== Cargo/Rust Build Tools ========
        registry.register(ToolDefinition {
            name: tools::cargo::BUILD_NAME,
            description: tools::cargo::BUILD_DESCRIPTION,
            permissions: tools::cargo::BUILD_PERMISSIONS,
            timeout: tools::cargo::BUILD_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::cargo::TEST_NAME,
            description: tools::cargo::TEST_DESCRIPTION,
            permissions: tools::cargo::TEST_PERMISSIONS,
            timeout: tools::cargo::TEST_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::cargo::FMT_NAME,
            description: tools::cargo::FMT_DESCRIPTION,
            permissions: tools::cargo::FMT_PERMISSIONS,
            timeout: tools::cargo::FMT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::cargo::CLIPPY_NAME,
            description: tools::cargo::CLIPPY_DESCRIPTION,
            permissions: tools::cargo::CLIPPY_PERMISSIONS,
            timeout: tools::cargo::CLIPPY_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::cargo::CHECK_NAME,
            description: tools::cargo::CHECK_DESCRIPTION,
            permissions: tools::cargo::CHECK_PERMISSIONS,
            timeout: tools::cargo::CHECK_TIMEOUT,
        });

        // ======== Archive Operations ========
        registry.register(ToolDefinition {
            name: tools::archive::CREATE_NAME,
            description: tools::archive::CREATE_DESCRIPTION,
            permissions: tools::archive::CREATE_PERMISSIONS,
            timeout: tools::archive::CREATE_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::archive::EXTRACT_NAME,
            description: tools::archive::EXTRACT_DESCRIPTION,
            permissions: tools::archive::EXTRACT_PERMISSIONS,
            timeout: tools::archive::EXTRACT_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::archive::LIST_NAME,
            description: tools::archive::LIST_DESCRIPTION,
            permissions: tools::archive::LIST_PERMISSIONS,
            timeout: tools::archive::LIST_TIMEOUT,
        });

        // ======== Logs Operations ========
        registry.register(ToolDefinition {
            name: tools::logs::TAIL_NAME,
            description: tools::logs::TAIL_DESCRIPTION,
            permissions: tools::logs::TAIL_PERMISSIONS,
            timeout: tools::logs::TAIL_TIMEOUT,
        });
        registry.register(ToolDefinition {
            name: tools::logs::GREP_NAME,
            description: tools::logs::GREP_DESCRIPTION,
            permissions: tools::logs::GREP_PERMISSIONS,
            timeout: tools::logs::GREP_TIMEOUT,
        });
        for &name in crate::developer_utilities::ALL_NAMES {
            registry.register(ToolDefinition {
                name,
                description: crate::developer_utilities::DESCRIPTION,
                permissions: crate::developer_utilities::PERMISSIONS,
                timeout: crate::developer_utilities::TIMEOUT,
            });
        }

        registry
    }

    pub fn new() -> Self {
        Self::with_permissions(PermissionEngine::new())
    }

    pub fn with_permissions(permissions: PermissionEngine) -> Self {
        Self {
            tools: HashMap::new(),
            manifest_cache: HashMap::new(),
            permissions,
        }
    }

    pub fn register(&mut self, definition: ToolDefinition) {
        let manifest = definition.manifest();
        self.manifest_cache.insert(definition.name, manifest);
        self.tools.insert(definition.name, definition);
    }

    pub fn list(&self) -> Vec<&ToolDefinition> {
        let mut items: Vec<_> = self.tools.values().collect();
        items.sort_by_key(|tool| tool.name);
        items
    }

    pub fn manifests(&self) -> Vec<crate::ToolManifest> {
        self.list()
            .into_iter()
            .filter_map(|tool| self.manifest_cache.get(tool.name).cloned())
            .collect()
    }

    pub fn manifest_for(&self, name: &str) -> Option<crate::ToolManifest> {
        self.manifest_cache.get(name).cloned()
    }

    /// Performs the exact policy/scope check without creating an in-memory
    /// approval and without dispatching a tool. Core uses this boundary to
    /// append a durable pre receipt before any effect.
    pub async fn preflight(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: &Value,
    ) -> Result<ToolPreflightDecision, ToolError> {
        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_owned()))?;
        if name == tools::patch::NAME {
            tools::patch::validate_input(input)?;
        }
        let scope = scope_from_input(name, input);
        let command = command_from_input(name, input);
        let subject = canonical_policy_subject(ctx, name, input, command.as_deref())?;
        let preview = approval_preview(name, &scope, command.as_deref(), input);
        for permission in definition.permissions {
            let check = evohime_permissions::PermissionCheck {
                session_id: ctx.session_id,
                path: Some(scope.as_str()),
                command: command.as_deref(),
            };
            let decision = match subject.as_deref() {
                Some(value) => {
                    self.permissions
                        .check_scoped_with_subject(*permission, &check, value)
                        .await
                }
                None => self.permissions.check_scoped(*permission, &check).await,
            };
            match decision {
                PermissionDecision::Allowed => {}
                PermissionDecision::Denied => {
                    return Ok(ToolPreflightDecision::Denied(*permission))
                }
                PermissionDecision::NeedsApproval => {
                    return Ok(ToolPreflightDecision::ApprovalRequired {
                        permission: *permission,
                        scope,
                        preview,
                    })
                }
            }
        }
        Ok(ToolPreflightDecision::Allowed { scope, preview })
    }

    pub async fn execute(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
    ) -> Result<ToolResult, ToolError> {
        self.execute_with_cancellation(ctx, name, input, CancellationToken::new())
            .await
    }

    pub async fn execute_with_cancellation(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;

        if name == tools::patch::NAME {
            tools::patch::validate_input(&input)?;
        }

        let scope = scope_from_input(name, &input);
        let command = command_from_input(name, &input);
        let canonical_subject = canonical_policy_subject(ctx, name, &input, command.as_deref())?;
        let preview = approval_preview(name, &scope, command.as_deref(), &input);
        for permission in definition.permissions {
            let check = evohime_permissions::PermissionCheck {
                session_id: ctx.session_id,
                path: Some(scope.as_str()),
                command: command.as_deref(),
            };
            let decision = match canonical_subject.as_deref() {
                Some(subject) => {
                    self.permissions
                        .check_scoped_with_subject(*permission, &check, subject)
                        .await
                }
                None => self.permissions.check_scoped(*permission, &check).await,
            };
            match decision {
                PermissionDecision::Allowed => {}
                PermissionDecision::Denied => return Err(ToolError::PermissionDenied(*permission)),
                PermissionDecision::NeedsApproval => {
                    let approval = self
                        .permissions
                        .create_approval_scoped_for_call_with_command_and_preview(
                            evohime_permissions::CallIdentity {
                                task_id: ctx.task_id,
                                session_id: ctx.session_id,
                                tool_name: name,
                                permission: *permission,
                                scope: &scope,
                                input: &input,
                            },
                            command.clone(),
                            preview.clone(),
                        )
                        .await;
                    return Err(ToolError::NeedsApproval(Box::new(ApprovalRequired {
                        tool: name.to_string(),
                        permission: *permission,
                        scope: approval.scope,
                        approval_id: approval.id,
                        input: input.clone(),
                        preview: approval.preview,
                    })));
                }
            }
        }

        let execution = async {
            match name {
                name if crate::developer_utilities::ALL_NAMES.contains(&name) => {
                    crate::developer_utilities::execute(ctx, name, input).await
                }
                tools::filesystem::NAME => tools::filesystem::execute(ctx, input).await,
                tools::write::NAME => tools::write::execute(ctx, input).await,
                tools::patch::NAME => tools::patch::execute(ctx, input).await,
                tools::search::NAME => tools::search::execute(ctx, input).await,
                tools::list::NAME => tools::list::execute(ctx, input).await,
                tools::shell::NAME => tools::shell::execute(ctx, input, cancellation.clone()).await,
                tools::git::STATUS_NAME => tools::git::status(ctx, input).await,
                tools::git::DIFF_NAME => tools::git::diff(ctx, input).await,
                tools::git::COMMIT_NAME => tools::git::commit(ctx, input).await,
                tools::git::PULL_NAME => tools::git::pull(ctx, input).await,
                tools::git::PUSH_NAME => tools::git::push(ctx, input).await,
                tools::git::LOG_NAME => tools::git::log(ctx, input).await,
                tools::git::SHOW_NAME => tools::git::show(ctx, input).await,
                tools::git::BLAME_NAME => tools::git::blame(ctx, input).await,
                tools::git::CHANGED_FILES_NAME => tools::git::changed_files(ctx, input).await,
                tools::mcp::NAME => tools::mcp::execute(ctx, input).await,
                tools::memory::NAME => tools::memory::execute(ctx, input).await,
                tools::agent::NAME => tools::agent::execute(ctx, input).await,
                tools::browser::OPEN_NAME => tools::browser::open(ctx, input).await,
                tools::browser::EXTRACT_NAME => tools::browser::extract(ctx, input).await,
                tools::browser_session::NAVIGATE_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::READ_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::CLICK_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::SCREENSHOT_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::TYPE_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::CLOSE_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::http::NAME => tools::http::fetch(ctx, input).await,

                // Advanced Git Operations
                tools::git_advanced::BRANCH_NAME => tools::git_advanced::branch(ctx, input).await,
                tools::git_advanced::MERGE_NAME => tools::git_advanced::merge(ctx, input).await,
                tools::git_advanced::RESET_NAME => tools::git_advanced::reset(ctx, input).await,
                tools::git_advanced::REVERT_NAME => tools::git_advanced::revert(ctx, input).await,
                tools::git_advanced::CHERRY_PICK_NAME => {
                    tools::git_advanced::cherry_pick(ctx, input).await
                }
                tools::git_advanced::REBASE_NAME => tools::git_advanced::rebase(ctx, input).await,
                tools::git_advanced::TAG_NAME => tools::git_advanced::tag(ctx, input).await,
                tools::git_advanced::STASH_NAME => tools::git_advanced::stash(ctx, input).await,
                tools::git_advanced::REMOTE_NAME => tools::git_advanced::remote(ctx, input).await,

                // Advanced Filesystem Operations
                tools::filesystem_advanced::DELETE_NAME => {
                    tools::filesystem_advanced::delete(ctx, input).await
                }
                tools::git_worktree::CREATE_NAME => tools::git_worktree::create(ctx, input).await,
                tools::git_worktree::REMOVE_NAME => tools::git_worktree::remove(ctx, input).await,
                tools::git_worktree::PREFLIGHT_NAME => {
                    tools::git_worktree::preflight(ctx, input).await
                }
                tools::filesystem_advanced::MOVE_NAME => {
                    tools::filesystem_advanced::move_file(ctx, input).await
                }
                tools::filesystem_advanced::COPY_NAME => {
                    tools::filesystem_advanced::copy(ctx, input).await
                }
                tools::filesystem_advanced::STAT_NAME => {
                    tools::filesystem_advanced::stat(ctx, input).await
                }
                tools::filesystem_advanced::MKDIR_NAME => {
                    tools::filesystem_advanced::mkdir(ctx, input).await
                }

                // Desktop Applications
                tools::app::OPEN_NAME => tools::app::open(ctx, input).await,
                tools::app::LIST_NAME => tools::app::list(ctx, input).await,

                // Process Operations
                tools::process::NAME => tools::process::execute(ctx, input).await,

                // Cargo/Rust Build Tools
                tools::cargo::BUILD_NAME => tools::cargo::build(ctx, input).await,
                tools::cargo::TEST_NAME => tools::cargo::test(ctx, input).await,
                tools::cargo::FMT_NAME => tools::cargo::fmt(ctx, input).await,
                tools::cargo::CLIPPY_NAME => tools::cargo::clippy(ctx, input).await,
                tools::cargo::CHECK_NAME => tools::cargo::check(ctx, input).await,

                // Archive Operations
                tools::archive::CREATE_NAME => tools::archive::create(ctx, input).await,
                tools::archive::EXTRACT_NAME => tools::archive::extract(ctx, input).await,
                tools::archive::LIST_NAME => tools::archive::list(ctx, input).await,

                // Logs Operations
                tools::logs::TAIL_NAME => tools::logs::tail(ctx, input).await,
                tools::logs::GREP_NAME => tools::logs::grep(ctx, input).await,

                _ => Err(ToolError::UnknownTool(name.to_string())),
            }
        };

        tokio::select! {
            _ = cancellation.cancelled() => Err(ToolError::Execution("tool cancelled".to_string())),
            result = tokio::time::timeout(definition.timeout, execution) => match result {
                Ok(result) => result,
                Err(_) => Err(ToolError::TimedOut(definition.timeout)),
            },
        }
    }

    pub async fn execute_after_durable_approval(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;
        // Approval is an authorization result, not a substitute for the
        // tool's input validator. Keep the post-approval path fail-closed
        // when a caller constructs an approval outside `execute`.
        if name == tools::patch::NAME {
            tools::patch::validate_input(&input)?;
        }
        let scope = scope_from_input(name, &input);
        let command = command_from_input(name, &input);
        let canonical_subject = canonical_policy_subject(ctx, name, &input, command.as_deref())?;
        for permission in definition.permissions {
            let check = evohime_permissions::PermissionCheck {
                session_id: ctx.session_id,
                path: Some(scope.as_str()),
                command: command.as_deref(),
            };
            let decision = match canonical_subject.as_deref() {
                Some(subject) => {
                    self.permissions
                        .check_scoped_with_subject(*permission, &check, subject)
                        .await
                }
                None => self.permissions.check_scoped(*permission, &check).await,
            };
            if matches!(decision, evohime_permissions::PermissionDecision::Denied) {
                return Err(ToolError::PermissionDenied(*permission));
            }
        }
        let execution = async {
            match name {
                name if crate::developer_utilities::ALL_NAMES.contains(&name) => {
                    crate::developer_utilities::execute(ctx, name, input).await
                }
                tools::filesystem::NAME => tools::filesystem::execute(ctx, input).await,
                tools::write::NAME => tools::write::execute(ctx, input).await,
                tools::patch::NAME => tools::patch::execute(ctx, input).await,
                tools::search::NAME => tools::search::execute(ctx, input).await,
                tools::list::NAME => tools::list::execute(ctx, input).await,
                tools::shell::NAME => tools::shell::execute(ctx, input, cancellation.clone()).await,
                tools::git::STATUS_NAME => tools::git::status(ctx, input).await,
                tools::git::DIFF_NAME => tools::git::diff(ctx, input).await,
                tools::git::COMMIT_NAME => tools::git::commit(ctx, input).await,
                tools::git::PULL_NAME => tools::git::pull(ctx, input).await,
                tools::git::PUSH_NAME => tools::git::push(ctx, input).await,
                tools::mcp::NAME => tools::mcp::execute(ctx, input).await,
                tools::memory::NAME => tools::memory::execute(ctx, input).await,
                tools::agent::NAME => tools::agent::execute(ctx, input).await,
                tools::browser::OPEN_NAME => tools::browser::open(ctx, input).await,
                tools::browser::EXTRACT_NAME => tools::browser::extract(ctx, input).await,
                tools::browser_session::NAVIGATE_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::READ_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::CLICK_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::SCREENSHOT_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::TYPE_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::browser_session::CLOSE_NAME => {
                    tools::browser_session::legacy_disabled(ctx, input).await
                }
                tools::http::NAME => tools::http::fetch(ctx, input).await,

                // Advanced Git Operations
                tools::git_advanced::BRANCH_NAME => tools::git_advanced::branch(ctx, input).await,
                tools::git_advanced::MERGE_NAME => tools::git_advanced::merge(ctx, input).await,
                tools::git_advanced::RESET_NAME => tools::git_advanced::reset(ctx, input).await,
                tools::git_advanced::REVERT_NAME => tools::git_advanced::revert(ctx, input).await,
                tools::git_advanced::CHERRY_PICK_NAME => {
                    tools::git_advanced::cherry_pick(ctx, input).await
                }
                tools::git_advanced::REBASE_NAME => tools::git_advanced::rebase(ctx, input).await,
                tools::git_advanced::TAG_NAME => tools::git_advanced::tag(ctx, input).await,
                tools::git_advanced::STASH_NAME => tools::git_advanced::stash(ctx, input).await,
                tools::git_advanced::REMOTE_NAME => tools::git_advanced::remote(ctx, input).await,

                // Advanced Filesystem Operations
                tools::filesystem_advanced::DELETE_NAME => {
                    tools::filesystem_advanced::delete(ctx, input).await
                }
                tools::git_worktree::CREATE_NAME => tools::git_worktree::create(ctx, input).await,
                tools::git_worktree::REMOVE_NAME => tools::git_worktree::remove(ctx, input).await,
                tools::git_worktree::PREFLIGHT_NAME => {
                    tools::git_worktree::preflight(ctx, input).await
                }
                tools::filesystem_advanced::MOVE_NAME => {
                    tools::filesystem_advanced::move_file(ctx, input).await
                }
                tools::filesystem_advanced::COPY_NAME => {
                    tools::filesystem_advanced::copy(ctx, input).await
                }
                tools::filesystem_advanced::STAT_NAME => {
                    tools::filesystem_advanced::stat(ctx, input).await
                }
                tools::filesystem_advanced::MKDIR_NAME => {
                    tools::filesystem_advanced::mkdir(ctx, input).await
                }

                // Desktop Applications
                tools::app::OPEN_NAME => tools::app::open(ctx, input).await,
                tools::app::LIST_NAME => tools::app::list(ctx, input).await,

                // Process Operations
                tools::process::NAME => tools::process::execute(ctx, input).await,

                // Cargo/Rust Build Tools
                tools::cargo::BUILD_NAME => tools::cargo::build(ctx, input).await,
                tools::cargo::TEST_NAME => tools::cargo::test(ctx, input).await,
                tools::cargo::FMT_NAME => tools::cargo::fmt(ctx, input).await,
                tools::cargo::CLIPPY_NAME => tools::cargo::clippy(ctx, input).await,
                tools::cargo::CHECK_NAME => tools::cargo::check(ctx, input).await,

                // Archive Operations
                tools::archive::CREATE_NAME => tools::archive::create(ctx, input).await,
                tools::archive::EXTRACT_NAME => tools::archive::extract(ctx, input).await,
                tools::archive::LIST_NAME => tools::archive::list(ctx, input).await,

                // Logs Operations
                tools::logs::TAIL_NAME => tools::logs::tail(ctx, input).await,
                tools::logs::GREP_NAME => tools::logs::grep(ctx, input).await,

                _ => Err(ToolError::UnknownTool(name.to_string())),
            }
        };
        tokio::select! {
            _ = cancellation.cancelled() => Err(ToolError::Execution("tool cancelled".to_string())),
            result = tokio::time::timeout(definition.timeout, execution) => match result {
                Ok(result) => result,
                Err(_) => Err(ToolError::TimedOut(definition.timeout)),
            },
        }
    }

    /// Compatibility-only wrapper for old in-process callers. Production
    /// Core paths must claim the durable receipt approval first and then call
    /// [`Self::execute_after_durable_approval`].
    #[cfg(test)]
    pub async fn execute_after_approval(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        approval_id: Uuid,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        let definition = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;
        let scope = scope_from_input(name, &input);
        for permission in definition.permissions {
            match self
                .permissions
                .claim_approval_for_call(
                    approval_id,
                    evohime_permissions::CallIdentity {
                        task_id: ctx.task_id,
                        session_id: ctx.session_id,
                        tool_name: name,
                        permission: *permission,
                        scope: &scope,
                        input: &input,
                    },
                )
                .await
            {
                Some(evohime_permissions::ApprovalState::Granted) => {}
                Some(evohime_permissions::ApprovalState::Denied) => {
                    return Err(ToolError::ApprovalDenied)
                }
                Some(evohime_permissions::ApprovalState::Pending) | None => {
                    return Err(ToolError::ApprovalMismatch)
                }
            }
        }
        self.execute_after_durable_approval(ctx, name, input, cancellation)
            .await
    }

    pub async fn execute_cancellable(
        &self,
        ctx: &ToolContext,
        name: &str,
        input: Value,
        cancellation: CancellationToken,
    ) -> Result<ToolResult, ToolError> {
        let cancellation_wait = cancellation.clone();
        tokio::select! {
            _ = cancellation_wait.cancelled() => Err(ToolError::Execution("tool cancelled".to_string())),
            result = self.execute_with_cancellation(ctx, name, input, cancellation) => result,
        }
    }

    pub async fn execute_parallel(
        &self,
        ctx: &ToolContext,
        calls: Vec<(String, Value)>,
        cancellation: CancellationToken,
    ) -> Vec<Result<ToolResult, ToolError>> {
        let futures = calls.into_iter().map(|(name, input)| {
            let token = cancellation.clone();
            async move {
                self.execute_with_cancellation(ctx, &name, input, token)
                    .await
            }
        });
        futures_util::future::join_all(futures).await
    }

    pub fn permissions(&self) -> &PermissionEngine {
        &self.permissions
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::bootstrap()
    }
}

/// Build the canonical subject used by hard policy rules.
///
/// Path aliases must be resolved before policy evaluation so a relative or
/// traversed spelling cannot evade an absolute deny rule.
fn canonical_policy_subject(
    ctx: &ToolContext,
    tool_name: &str,
    input: &Value,
    command: Option<&str>,
) -> Result<Option<String>, ToolError> {
    if let Some(command) = command {
        return Ok(Some(command.to_string()));
    }

    let is_path_tool = matches!(
        tool_name,
        tools::filesystem::NAME
            | tools::write::NAME
            | tools::patch::NAME
            | tools::search::NAME
            | tools::list::NAME
    );
    if !is_path_tool {
        return Ok(None);
    }

    let sandbox = ctx.sandbox()?;
    let path = input.get("path").and_then(Value::as_str).unwrap_or(".");
    let resolved = if tool_name == tools::write::NAME {
        sandbox.resolve_for_write(path)?
    } else {
        sandbox.resolve_existing_for_tool(path, tool_name)?
    };
    let subject = resolved.display().to_string().replace('\\', "/");
    Ok(Some(
        subject.strip_prefix("//?/").unwrap_or(&subject).to_string(),
    ))
}

fn scope_from_input(tool_name: &str, input: &Value) -> String {
    if tool_name == tools::shell::NAME {
        if let Some((_, _, Some(cwd))) = tools::shell::resolve_invocation(input) {
            return cwd.replace('\\', "/");
        }
    }
    if let Some(path) = input.get("path").and_then(Value::as_str) {
        return path.replace('\\', "/");
    }
    if let Some(cwd) = input.get("cwd").and_then(Value::as_str) {
        return cwd.replace('\\', "/");
    }
    if let Some(url) = input.get("url").and_then(Value::as_str) {
        return url.to_string();
    }
    if tool_name.starts_with("browser.") {
        return "browser".to_string();
    }
    "workspace".to_string()
}

/// Build the subject matched by permission rules.
fn command_from_input(tool_name: &str, input: &Value) -> Option<String> {
    if tool_name == tools::shell::NAME {
        let (program, args, _) = tools::shell::resolve_invocation(input)?;
        return Some(
            std::iter::once(program)
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let command = match tool_name {
        tools::git::STATUS_NAME => "git status",
        tools::git::DIFF_NAME => "git diff",
        tools::git::COMMIT_NAME => "git commit",
        tools::git::PULL_NAME => "git pull",
        tools::git::PUSH_NAME => "git push",
        _ => return None,
    };
    Some(command.to_string())
}

const MAX_APPROVAL_PREVIEW_DETAILS: usize = 8 * 1024;

/// Build the user-facing part of an approval without forwarding an unbounded
/// copy of the model's complete tool input to the shell. The exact input is
/// still bound by the approval call hash and is rechecked by Core on execute.
fn approval_preview(
    tool_name: &str,
    scope: &str,
    command: Option<&str>,
    input: &Value,
) -> ApprovalPreview {
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .map(|value| value.replace('\\', "/"));
    let cwd = input
        .get("cwd")
        .and_then(Value::as_str)
        .map(|value| value.replace('\\', "/"))
        .or_else(|| (tool_name == tools::shell::NAME).then(|| scope.to_string()));

    match tool_name {
        tools::app::OPEN_NAME => ApprovalPreview {
            kind: "app_open".into(),
            summary: "Открыть приложение".into(),
            command: input.get("app").and_then(Value::as_str).map(str::to_string),
            cwd: None,
            path: None,
            details: None,
            truncated: false,
        },
        tools::shell::NAME => ApprovalPreview {
            kind: "command".into(),
            summary: format!(
                "Запустить команду{}",
                cwd.as_deref()
                    .map(|value| format!(" в {value}"))
                    .unwrap_or_default()
            ),
            command: command.map(str::to_string),
            cwd,
            path: None,
            details: None,
            truncated: false,
        },
        tools::write::NAME => {
            let content = input
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let bytes = content.len();
            let (details, truncated) = bounded_preview(content);
            ApprovalPreview {
                kind: "file_write".into(),
                summary: format!("Записать файл ({bytes} байт)"),
                command: None,
                cwd: None,
                path,
                details: (!details.is_empty()).then_some(details),
                truncated,
            }
        }
        tools::patch::NAME => {
            let (details, truncated) = bounded_preview(
                input
                    .get("patch")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            ApprovalPreview {
                kind: "diff".into(),
                summary: "Применить unified diff".into(),
                command: None,
                cwd: None,
                path,
                details: (!details.is_empty()).then_some(details),
                truncated,
            }
        }
        tools::git::COMMIT_NAME => {
            let (details, truncated) = bounded_preview(
                input
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            ApprovalPreview {
                kind: "git_commit".into(),
                summary: "Создать git-коммит".into(),
                command: command.map(str::to_string),
                cwd: None,
                path: None,
                details: (!details.is_empty()).then_some(details),
                truncated,
            }
        }
        _ => {
            let (details, truncated) = bounded_preview(&preview_summary(input));
            ApprovalPreview {
                kind: "operation".into(),
                summary: format!("Выполнить {tool_name}"),
                command: command.map(str::to_string),
                cwd,
                path,
                details: (!details.is_empty()).then_some(details),
                truncated,
            }
        }
    }
}

fn preview_summary(input: &Value) -> String {
    let Some(object) = input.as_object() else {
        return String::new();
    };
    let mut fields = Vec::new();
    for (key, value) in object {
        if matches!(key.as_str(), "content" | "patch") || is_sensitive_key(key) {
            continue;
        }
        let rendered = match value {
            Value::String(text) => text.clone(),
            _ => value.to_string(),
        };
        fields.push(format!("{key}={rendered}"));
    }
    fields.join("\n")
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    ["secret", "token", "password", "api_key", "private_key"]
        .iter()
        .any(|needle| key.contains(needle))
}

fn bounded_preview(value: &str) -> (String, bool) {
    let mut end = value.len().min(MAX_APPROVAL_PREVIEW_DETAILS);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = end < value.len();
    let mut result = value[..end].to_string();
    if truncated {
        result.push_str("\n… preview truncated");
    }
    (result, truncated)
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
