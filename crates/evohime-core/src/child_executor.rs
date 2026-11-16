//! Core-owned executor for one typed child node.
//!
//! The executor deliberately reuses the existing owners instead of creating a
//! second agent loop:
//! - [`crate::agent_role_profiles`] resolves and pins the registered role;
//! - [`crate::child_contracts`] carries the typed request/report contract;
//! - [`crate::child_runtime`] re-checks the read-only boundary;
//! - [`crate::child_workflow`] owns the lifecycle and fan-in;
//! - [`evohime_tool_runtime::ToolRegistry`] performs the bounded read-only
//!   tool calls under its existing permission engine.
//!
//! A child never receives a new wallet, a new grant set, or a writable tool.
//! Its capabilities are the intersection of the parent grants, the read-only
//! policy, the role profile, and the requested capabilities. Nested child
//! delegation remains forbidden by [`crate::child_contracts`].

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::agent_role_profiles::{
    canonical_hash as profile_hash, AgentRoleProfile, AgentRoleProfilesRegistry, BudgetDefaults,
    ContractField, ExecutionMode, RunState, StartRuntimeInput,
};
use crate::child_contracts::{
    accept_typed_report, ChildBudget, ContractError, CorrelationContext, CorrelationId, Provenance,
    TypedChildReport, TypedChildTaskRequest, TypedReportStatus,
};
use crate::child_roles::{
    ChildRole, HandoffEnvelope, HandoffEnvelopeInput, HandoffKind, HandoffPayload, RoleIdentity,
};
use crate::child_runtime::{ChildReport, ChildReportStatus, ChildTaskKind, ChildTaskRequest};
use crate::child_workflow::{Coordinator, CoordinatorState};
use crate::workflow::NodeBudget;
use crate::workflow_adapters::{ChildExecutor, ChildFuture, ChildOutcome};
use crate::workflow_runtime::NodeError;
use crate::EventJournal;
use evohime_tool_runtime::{ToolContext, ToolError, ToolRegistry};

/// Maximum read-only tool calls a single child may dispatch.
pub const CHILD_TOOL_CALL_LIMIT: u32 = 4;
/// Maximum wall-clock seconds a single child may consume.
pub const CHILD_MAX_SECONDS: u64 = 180;
/// Capabilities a child may request from the read-only runtime contract.
pub const READ_ONLY_CAPABILITIES: &[&str] = &[
    "workspace.read",
    "workspace.search",
    "git.diff",
    "git.status",
];

/// Maps a grant or capability name to its canonical read-only capability.
///
/// The function returns `None` for write, shell, network, elevation, or
/// unknown names. Callers must treat `None` as a policy rejection, not as an
/// empty capability.
pub(crate) fn canonical_capability(value: &str) -> Option<&'static str> {
    match value.split(':').next().unwrap_or(value) {
        "fs.read" | "workspace.read" | "filesystem.read" | "filesystem.list" => {
            Some("workspace.read")
        }
        "workspace.search" | "filesystem.search" => Some("workspace.search"),
        "git.diff" => Some("git.diff"),
        "git.status" => Some("git.status"),
        _ => None,
    }
}

/// Maps a registered role tool id to its canonical capability.
fn capability_for_tool(tool: &str) -> Option<&'static str> {
    canonical_capability(tool)
}

/// Returns a bounded Core-owned profile for a known built-in role.
pub(crate) fn builtin_profile(role: &str) -> Option<AgentRoleProfile> {
    let (objective, tools) = match role {
        "coordinator" => (
            "Coordinate bounded read-only child work",
            vec![
                "filesystem.search",
                "filesystem.list",
                "git.status",
                "git.diff",
            ],
        ),
        "researcher" => (
            "Gather and summarize bounded repository evidence",
            vec![
                "filesystem.search",
                "filesystem.list",
                "git.status",
                "git.diff",
            ],
        ),
        "planner" => (
            "Draft a bounded plan without applying changes",
            vec!["filesystem.search", "filesystem.list"],
        ),
        "implementer" => (
            "Prepare an authorized change without writing as a child",
            vec![
                "filesystem.search",
                "filesystem.list",
                "git.status",
                "git.diff",
            ],
        ),
        "tester" => (
            "Review a bounded test plan without executing it",
            vec![
                "filesystem.search",
                "filesystem.list",
                "git.status",
                "git.diff",
            ],
        ),
        "reviewer" => (
            "Review bounded changes against requirements",
            vec![
                "filesystem.search",
                "filesystem.list",
                "git.status",
                "git.diff",
            ],
        ),
        "security-reviewer" => (
            "Review bounded trust boundaries without mutating state",
            vec![
                "filesystem.search",
                "filesystem.list",
                "git.status",
                "git.diff",
            ],
        ),
        _ => return None,
    };
    Some(AgentRoleProfile {
        schema_version: crate::agent_role_profiles::CONTRACT_VERSION,
        id: role.into(),
        revision: 1,
        objective: objective.into(),
        constraints: vec!["read_only".into(), "no_nested_children".into()],
        skills: Vec::new(),
        tools: tools.into_iter().map(str::to_owned).collect(),
        strategy: "read_only_evidence_then_report".into(),
        input_contract: vec![ContractField {
            name: "purpose".into(),
            type_name: "string".into(),
            required: true,
        }],
        output_contract: vec![ContractField {
            name: "report".into(),
            type_name: "child_report".into(),
            required: true,
        }],
        budget_defaults: BudgetDefaults {
            timeout_ms: CHILD_MAX_SECONDS * 1_000,
            max_steps: 1,
            max_output_bytes: crate::child_contracts::MAX_OUTPUT_BYTES as u32,
        },
        execution_mode: ExecutionMode::Ai,
    })
}

/// One resolved role profile plus the runtime pin created from it.
struct ResolvedRole {
    profile: AgentRoleProfile,
    content_hash: String,
    effective_grants: Vec<String>,
}

/// Bounded output of the read-only role run before it becomes a typed report.
struct RoleRunOutput {
    summary: String,
    findings: Vec<String>,
    sources: Vec<String>,
    errors: Vec<String>,
    successful_calls: u32,
}

impl RoleRunOutput {
    fn status(&self) -> TypedReportStatus {
        if self.successful_calls > 0 {
            TypedReportStatus::Complete
        } else if self.errors.is_empty() {
            TypedReportStatus::Rejected
        } else {
            TypedReportStatus::Partial
        }
    }
}

/// Executes one typed child request through the existing Core owners.
///
/// The executor is cheap to clone at the adapter boundary, but it is normally
/// kept as a single `Arc<dyn ChildExecutor>` for a workflow adapter.
pub struct RegisteredChildExecutor {
    journal: EventJournal,
    tools: Option<Arc<ToolRegistry>>,
    workspace_root: PathBuf,
    coordinator: Option<crate::TaskCoordinator>,
    parent_capabilities: Vec<String>,
    parent_budget: Option<NodeBudget>,
    capacity: crate::bounded_tasks::BoundedTaskGroup,
}

impl RegisteredChildExecutor {
    /// Creates an executor rooted at one workspace.
    pub fn new(journal: EventJournal, workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            journal,
            tools: None,
            workspace_root: workspace_root.into(),
            coordinator: None,
            parent_capabilities: Vec::new(),
            parent_budget: None,
            capacity: crate::bounded_tasks::BoundedTaskGroup::new(
                crate::bounded_tasks::DEFAULT_CAPACITY,
            ),
        }
    }

    /// Attaches the Core tool registry used for bounded read-only calls.
    pub fn with_tools(mut self, tools: Option<Arc<ToolRegistry>>) -> Self {
        self.tools = tools;
        self
    }

    /// Emits child lifecycle projections through the coordinator when present.
    pub fn with_coordinator(mut self, coordinator: Option<crate::TaskCoordinator>) -> Self {
        self.coordinator = coordinator;
        self
    }

    /// Supplies the parent capability ceiling for defense-in-depth checks.
    pub fn with_parent_capabilities(
        mut self,
        grants: impl IntoIterator<Item = String>,
        budget: Option<NodeBudget>,
    ) -> Self {
        self.parent_capabilities = grants.into_iter().collect();
        self.parent_budget = budget;
        self
    }

    /// Supplies the parent budget ceiling used by the executor.
    pub fn with_parent_budget(mut self, budget: NodeBudget) -> Self {
        self.parent_budget = Some(budget);
        self
    }

    /// Resolves a persisted role profile or a Core-owned built-in profile.
    ///
    /// Persisted profiles always win over the built-in definition with the
    /// same id, so an operator can revise a role without changing the build.
    async fn resolve_role(
        &self,
        request: &TypedChildTaskRequest,
    ) -> Result<ResolvedRole, NodeError> {
        let mut registered = BTreeMap::new();
        {
            let database = self.journal.database.lock().await;
            let rows = evohime_local_storage::agent_role_profiles_store::load_all_json(
                database.connection(),
            )
            .map_err(|error| {
                NodeError::permanent(
                    "child_role_registry_unavailable",
                    format!("role profile storage: {error}"),
                )
            })?;
            for row in rows {
                let profile: AgentRoleProfile = serde_json::from_slice(&row).map_err(|_| {
                    NodeError::permanent("child_role_registry_corrupt", "invalid role profile row")
                })?;
                registered.insert(profile.id.clone(), profile);
            }
        }
        let profile = registered
            .get(&request.role)
            .cloned()
            .or_else(|| builtin_profile(&request.role))
            .ok_or_else(|| {
                NodeError::permanent(
                    "child_role_not_registered",
                    format!("role {} is not registered", request.role),
                )
            })?;
        crate::agent_role_profiles::validate_profile(&profile).map_err(|error| {
            NodeError::permanent("child_role_registry_invalid", error.to_string())
        })?;
        let content_hash = profile_hash(&profile).map_err(|error| {
            NodeError::permanent("child_role_registry_invalid", error.to_string())
        })?;

        let parent = self.parent_capability_names(request)?;
        let requested = self.requested_capability_names(request)?;
        let registry = self.profile_capabilities(&profile);
        let policy = READ_ONLY_CAPABILITIES
            .iter()
            .map(|capability| (*capability).to_owned())
            .collect::<Vec<_>>();
        let mut roles = AgentRoleProfilesRegistry::default();
        roles.profiles.insert(profile.id.clone(), profile.clone());
        let runtime = roles
            .start(StartRuntimeInput {
                run_id: sanitize_id(&request.child_task_id),
                profile_id: &profile.id,
                revision: profile.revision,
                grants: requested.clone(),
                parent: &parent,
                policy: &policy,
                registry: &registry,
            })
            .map_err(|error| {
                NodeError::permanent(
                    "child_effect_forbidden",
                    format!("role capability intersection rejected: {error}"),
                )
            })?;
        if runtime.state != RunState::Pinned {
            return Err(NodeError::permanent(
                "child_role_not_pinned",
                "role runtime was not pinned",
            ));
        }
        if !requested
            .iter()
            .all(|capability| runtime.effective_grants.contains(capability))
        {
            return Err(NodeError::permanent(
                "child_effect_forbidden",
                "requested capability is outside the parent/policy/profile intersection",
            ));
        }
        Ok(ResolvedRole {
            profile,
            content_hash,
            effective_grants: runtime.effective_grants,
        })
    }

    fn parent_capability_names(
        &self,
        request: &TypedChildTaskRequest,
    ) -> Result<Vec<String>, NodeError> {
        let mut values = self.parent_capabilities.clone();
        if values.is_empty() {
            values.extend(request.grants.iter().map(|grant| grant.grant_type.clone()));
        }
        let mut result = BTreeSet::new();
        for value in values {
            let Some(capability) = canonical_capability(&value) else {
                return Err(NodeError::permanent(
                    "child_effect_forbidden",
                    format!("parent capability {value} is not read-only"),
                ));
            };
            result.insert(capability.to_owned());
        }
        Ok(result.into_iter().collect())
    }

    fn requested_capability_names(
        &self,
        request: &TypedChildTaskRequest,
    ) -> Result<Vec<String>, NodeError> {
        let values = if request.requested_capabilities.is_empty() {
            request
                .grants
                .iter()
                .map(|grant| grant.grant_type.clone())
                .collect::<Vec<_>>()
        } else {
            request.requested_capabilities.clone()
        };
        if values.is_empty() {
            return Err(NodeError::permanent(
                "child_capability_missing",
                "a child request must select at least one read-only capability",
            ));
        }
        let mut result = BTreeSet::new();
        for value in values {
            let Some(capability) = canonical_capability(&value) else {
                return Err(NodeError::permanent(
                    "child_effect_forbidden",
                    format!("requested capability {value} is not read-only"),
                ));
            };
            result.insert(capability.to_owned());
        }
        Ok(result.into_iter().collect())
    }

    fn profile_capabilities(&self, profile: &AgentRoleProfile) -> Vec<String> {
        let mut result = BTreeSet::new();
        for tool in &profile.tools {
            if let Some(capability) = capability_for_tool(tool) {
                result.insert(capability.to_owned());
            }
        }
        result.into_iter().collect()
    }

    fn bounded_budget(&self, request: &TypedChildTaskRequest) -> ChildBudget {
        let mut budget = request.budget.clone().unwrap_or_default();
        budget.max_tool_calls = Some(
            budget
                .max_tool_calls
                .unwrap_or(CHILD_TOOL_CALL_LIMIT as u64)
                .min(CHILD_TOOL_CALL_LIMIT as u64),
        );
        budget.max_time_seconds = Some(
            budget
                .max_time_seconds
                .unwrap_or(CHILD_MAX_SECONDS)
                .min(CHILD_MAX_SECONDS),
        );
        budget
    }

    fn ensure_budget_ceiling(&self, request: &TypedChildTaskRequest) -> Result<(), NodeError> {
        let Some(parent) = &self.parent_budget else {
            return Ok(());
        };
        let within = request.budget.as_ref().is_none_or(|budget| {
            budget.max_tokens.unwrap_or(0) <= parent.max_tokens
                && budget.max_time_seconds.unwrap_or(0) <= parent.max_seconds
                && budget.max_tool_calls.unwrap_or(0) <= parent.max_tool_calls
        });
        if within {
            Ok(())
        } else {
            Err(NodeError::permanent(
                "child_cannot_escalate_budget",
                "child budget exceeds the parent remainder",
            ))
        }
    }

    fn runtime_request(
        &self,
        request: &TypedChildTaskRequest,
        effective: &[String],
    ) -> Result<ChildTaskRequest, NodeError> {
        let runtime_request = ChildTaskRequest {
            child_task_id: request.child_task_id.clone(),
            parent_task_id: request.parent_task_id.clone(),
            role: request.role.clone(),
            kind: runtime_kind(&request.role),
            reduced_context: request.reduced_context.clone(),
            max_output_bytes: request.max_output_bytes,
            requested_capabilities: effective.to_vec(),
            parent_is_child: request.parent_is_child,
        };
        runtime_request.validate().map_err(runtime_error)?;
        Ok(runtime_request)
    }

    /// Finds and validates the TeamSession pinned to this workflow run.
    ///
    /// A run without an explicit TeamSession proceeds with the profile pin
    /// recorded in the child checkpoint. If a session is linked to the run, its
    /// pinned protocol must contain the exact role profile and grant ceiling.
    async fn pinned_team_session(
        &self,
        request: &TypedChildTaskRequest,
        role: &ResolvedRole,
    ) -> Result<Option<String>, NodeError> {
        let database = self.journal.database.lock().await;
        let rows = evohime_local_storage::team_sop_protocols_store::load_sessions_json(
            database.connection(),
        )
        .map_err(|error| {
            NodeError::permanent(
                "child_session_unavailable",
                format!("session storage: {error}"),
            )
        })?;
        drop(database);

        for row in rows {
            let session: crate::team_sop_protocols::TeamSession = match serde_json::from_slice(&row)
            {
                Ok(session) => session,
                Err(_) => continue,
            };
            if session.workflow_run_id.as_deref() != Some(request.parent_task_id.as_str()) {
                continue;
            }
            if !matches!(
                session.status,
                crate::team_sop_protocols::SessionStatus::Pinned
                    | crate::team_sop_protocols::SessionStatus::Running
                    | crate::team_sop_protocols::SessionStatus::Paused
            ) {
                return Err(NodeError::permanent(
                    "child_session_not_active",
                    "the pinned TeamSession is not active",
                ));
            }
            let protocol: crate::team_sop_protocols::TeamProtocol =
                serde_json::from_slice(&session.snapshot.protocol_json).map_err(|_| {
                    NodeError::permanent("child_session_corrupt", "invalid TeamSession protocol")
                })?;
            let Some(slot) = protocol
                .participants
                .iter()
                .find(|slot| slot.role_profile_ref.id == role.profile.id)
            else {
                return Err(NodeError::permanent(
                    "child_role_not_in_team_session",
                    "the pinned TeamSession has no slot for this role",
                ));
            };
            if slot.role_profile_ref.revision != role.profile.revision
                || slot.role_profile_ref.content_hash != role.content_hash
            {
                return Err(NodeError::permanent(
                    "child_session_profile_stale",
                    "the TeamSession pins a different role profile revision",
                ));
            }
            if !slot.grants_ceiling.is_empty()
                && !role
                    .effective_grants
                    .iter()
                    .all(|grant| slot.grants_ceiling.iter().any(|ceiling| ceiling == grant))
            {
                return Err(NodeError::permanent(
                    "child_grant_exceeds_team_session",
                    "effective grants exceed the TeamSession ceiling",
                ));
            }
            return Ok(Some(session.id));
        }
        Ok(None)
    }

    async fn emit_projection(
        &self,
        request: &TypedChildTaskRequest,
        state: CoordinatorState,
        report: Option<&TypedChildReport>,
        reason: Option<String>,
        lease_live: bool,
    ) {
        let projection = crate::child_workflow::ChildProjection {
            event_id: format!("{}:{}", request.child_task_id, state.as_str()),
            parent_task_id: request.parent_task_id.clone(),
            child_task_id: request.child_task_id.clone(),
            role: request.role.clone(),
            revision: report
                .and_then(|report| report.revision)
                .unwrap_or_default(),
            state,
            reason_code: reason,
            parent_sequence: request.correlation.parent_sequence,
            budget: request.budget.clone(),
            lease_live,
            dead_letter: state == CoordinatorState::Failed,
            report_status: report.map(|report| report_status_str(report.status).to_owned()),
            report_summary: report
                .map(|report| crate::child_workflow::bounded_report_summary(&report.summary)),
        };
        let event = crate::CoreEvent::ChildWorkflowProjection {
            task_id: request.parent_task_id.clone(),
            projection,
        };
        if let Some(coordinator) = &self.coordinator {
            coordinator.emit(event).await;
        } else {
            let _ = self.journal.record(&event).await;
        }
    }

    async fn persist_checkpoint(
        &self,
        request: &TypedChildTaskRequest,
        state: CoordinatorState,
        report: Option<&TypedChildReport>,
        reason: Option<String>,
    ) -> Result<(), NodeError> {
        let now_ms = crate::task_memory::now_millis() as i64;
        let record = evohime_local_storage::domains::agents::CoordinatorCheckpointRecord {
            schema_version: 1,
            child_task_id: request.child_task_id.clone(),
            parent_task_id: request.parent_task_id.clone(),
            revision: report
                .and_then(|report| report.revision)
                .unwrap_or_default() as i64,
            state: state.as_str().into(),
            failure_reason: reason.clone(),
            dead_letter: state == CoordinatorState::Failed,
            report_json: report
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    NodeError::permanent("child_report_serialization_failed", error.to_string())
                })?,
            evidence_locators_json: report
                .map(|report| serde_json::to_string(&report.sources))
                .transpose()
                .map_err(|error| {
                    NodeError::permanent("child_report_serialization_failed", error.to_string())
                })?,
            provenance_hashes_json: report
                .map(|report| serde_json::to_string(&report.provenance))
                .transpose()
                .map_err(|error| {
                    NodeError::permanent("child_report_serialization_failed", error.to_string())
                })?,
            parent_sequence: request.correlation.parent_sequence as i64,
            lease_deadline_monotonic_ms: Some(
                now_ms + crate::child_workflow::DEFAULT_LEASE_MS as i64,
            ),
            lease_created_monotonic_ms: Some(now_ms),
            lease_clock_boot_id: Some("current".into()),
            lease_holder_process_id: Some(std::process::id().to_string()),
            last_transition_event: format!("child.{}", state.as_str()),
            last_transition_at_ms: now_ms,
            created_at_ms: now_ms,
        };
        self.journal
            .save_coordinator_checkpoint(&record)
            .await
            .map_err(|error| NodeError::permanent("child_checkpoint_unavailable", error))?;
        self.emit_projection(request, state, report, reason, !state.terminal())
            .await;
        Ok(())
    }

    async fn transition(
        &self,
        coordinator: &mut Coordinator,
        request: &TypedChildTaskRequest,
        next: CoordinatorState,
        reason: Option<String>,
    ) -> Result<(), NodeError> {
        coordinator
            .transition(&request.child_task_id, next, reason.clone())
            .map_err(|error| {
                NodeError::permanent("child_transition_rejected", format!("{error:?}"))
            })?;
        self.persist_checkpoint(request, next, None, reason).await
    }

    async fn persist_handoff(
        &self,
        request: &TypedChildTaskRequest,
        role: &ResolvedRole,
        session: Option<&str>,
        kind: HandoffKind,
        purpose: &str,
        status: &str,
    ) -> Result<(), NodeError> {
        let (from, to) = match kind {
            HandoffKind::Delegate => (
                RoleIdentity::builtin(ChildRole::Coordinator),
                role_identity(&role.profile.id)?,
            ),
            HandoffKind::ReturnResult => (
                role_identity(&role.profile.id)?,
                RoleIdentity::builtin(ChildRole::Coordinator),
            ),
            _ => (
                RoleIdentity::builtin(ChildRole::Coordinator),
                role_identity(&role.profile.id)?,
            ),
        };
        let payload = HandoffPayload::new([
            ("child_task_id".to_owned(), request.child_task_id.clone()),
            ("role".to_owned(), role.profile.id.clone()),
            ("profile_hash".to_owned(), role.content_hash.clone()),
            (
                "team_session".to_owned(),
                session.unwrap_or("unscoped").to_owned(),
            ),
            (
                "budget".to_owned(),
                bounded_budget_text(request.budget.as_ref()),
            ),
        ])
        .map_err(|error| NodeError::permanent("child_handoff_rejected", error.to_string()))?;
        let envelope = HandoffEnvelope::new(HandoffEnvelopeInput {
            handoff_id: sanitize_id(&format!(
                "{}-{}-{}",
                handoff_kind_str(kind),
                request.parent_task_id,
                request.child_task_id
            )),
            task_id: request.parent_task_id.clone(),
            kind,
            from: from.clone(),
            to: to.clone(),
            purpose: purpose.chars().take(512).collect(),
            payload,
            sequence: request.correlation.parent_sequence,
        })
        .map_err(|error| NodeError::permanent("child_handoff_rejected", error.to_string()))?;
        let record = evohime_local_storage::domains::agents::HandoffRecord {
            handoff_id: envelope.handoff_id.clone(),
            task_id: envelope.task_id.clone(),
            kind: handoff_kind_str(kind).into(),
            status: status.into(),
            from_role: role_identity_text(&from),
            to_role: role_identity_text(&to),
            sequence: envelope.sequence,
            envelope_json: envelope.to_deterministic_json().map_err(|error| {
                NodeError::permanent("child_handoff_rejected", error.to_string())
            })?,
        };
        self.journal
            .save_child_handoff(&record)
            .await
            .map_err(|error| NodeError::permanent("child_handoff_unavailable", error))
    }

    async fn execute_role_tools(
        &self,
        request: &TypedChildTaskRequest,
        role: &ResolvedRole,
        budget: &ChildBudget,
        session: Option<&str>,
    ) -> Result<RoleRunOutput, NodeError> {
        let Some(tools) = &self.tools else {
            return Err(NodeError::permanent(
                "child_tool_registry_unavailable",
                "the Core tool registry is not attached",
            ));
        };
        let max_calls = budget
            .max_tool_calls
            .unwrap_or(CHILD_TOOL_CALL_LIMIT as u64)
            .min(CHILD_TOOL_CALL_LIMIT as u64) as usize;
        let deadline = Instant::now()
            + Duration::from_secs(
                budget
                    .max_time_seconds
                    .unwrap_or(CHILD_MAX_SECONDS)
                    .min(CHILD_MAX_SECONDS),
            );
        let mut task_bytes = [0_u8; 16];
        task_bytes.copy_from_slice(
            &<sha2::Sha256 as sha2::Digest>::digest(request.child_task_id.as_bytes())[..16],
        );
        let context = ToolContext {
            workspace_root: self.workspace_root.clone(),
            task_id: uuid::Uuid::from_bytes(task_bytes),
            session_id: session.and_then(|session| uuid::Uuid::parse_str(session).ok()),
            progress_tx: None,
        };
        let plan = role_tool_plan(&role.effective_grants, request);
        let mut output = RoleRunOutput {
            summary: String::new(),
            findings: Vec::new(),
            sources: Vec::new(),
            errors: Vec::new(),
            successful_calls: 0,
        };
        for (index, (tool_name, input)) in plan.into_iter().enumerate() {
            if index >= max_calls || Instant::now() >= deadline {
                break;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let result =
                tokio::time::timeout(remaining, tools.execute(&context, tool_name, input)).await;
            match result {
                Ok(Ok(result)) => {
                    output.successful_calls += 1;
                    collect_tool_evidence(&mut output, tool_name, &result.structured);
                }
                Ok(Err(ToolError::PermissionDenied(_)))
                | Ok(Err(ToolError::WorkspaceBoundaryDenied(_)))
                | Ok(Err(ToolError::NeedsApproval(_)))
                | Ok(Err(ToolError::ApprovalMismatch))
                | Ok(Err(ToolError::ApprovalDenied)) => {
                    return Err(NodeError::permanent(
                        "child_effect_forbidden",
                        "the read-only capability was denied by the existing policy",
                    ));
                }
                Ok(Err(ToolError::UnknownTool(_))) => {
                    return Err(NodeError::permanent(
                        "child_tool_unavailable",
                        "the selected role tool is not registered",
                    ));
                }
                Ok(Err(_)) => {
                    output
                        .errors
                        .push(format!("{tool_name}: bounded read failed"));
                    output.findings.push(format!("{tool_name}: ошибка чтения"));
                }
                Err(_) => {
                    output
                        .errors
                        .push(format!("{tool_name}: child_time_budget"));
                    output.findings.push(format!("{tool_name}: лимит времени"));
                    break;
                }
            }
        }
        output.summary = if output.findings.is_empty() {
            format!(
                "Роль {} выполнила read-only проверку без замечаний.",
                role.profile.id
            )
        } else {
            format!(
                "Роль {} выполнила read-only проверку: {} инструмент(ов), {} источников.",
                role.profile.id,
                output.successful_calls,
                output.sources.len()
            )
        };
        Ok(output)
    }

    async fn execute(&self, request: &TypedChildTaskRequest) -> Result<ChildOutcome, NodeError> {
        let permit = self.capacity.try_acquire().ok_or_else(|| {
            NodeError::transient(
                "child_capacity_exhausted",
                "the bounded child task group is full",
            )
        })?;
        let outcome = self.execute_with_permit(request).await;
        drop(permit);
        outcome
    }

    async fn execute_with_permit(
        &self,
        request: &TypedChildTaskRequest,
    ) -> Result<ChildOutcome, NodeError> {
        request.validate().map_err(contract_error)?;
        self.ensure_budget_ceiling(request)?;
        let role = self.resolve_role(request).await?;
        let runtime_request = self.runtime_request(request, &role.effective_grants)?;
        let budget = self.bounded_budget(request);
        let session = self.pinned_team_session(request, &role).await?;

        let mut coordinator = Coordinator::new();
        coordinator
            .create(request, crate::task_memory::now_millis())
            .map_err(|error| {
                NodeError::permanent("child_checkpoint_rejected", format!("{error:?}"))
            })?;
        self.persist_checkpoint(request, CoordinatorState::Created, None, None)
            .await?;
        self.persist_handoff(
            request,
            &role,
            session.as_deref(),
            HandoffKind::Delegate,
            "delegate read-only child task",
            "pending",
        )
        .await?;
        self.transition(&mut coordinator, request, CoordinatorState::Queued, None)
            .await?;
        self.transition(&mut coordinator, request, CoordinatorState::Running, None)
            .await?;

        let run = match self
            .execute_role_tools(request, &role, &budget, session.as_deref())
            .await
        {
            Ok(run) => run,
            Err(error) => {
                let _ = self
                    .transition(
                        &mut coordinator,
                        request,
                        CoordinatorState::Failed,
                        Some(error.code.clone()),
                    )
                    .await;
                return Err(error);
            }
        };
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::Validating,
            None,
        )
        .await?;

        let report = self.build_report(request, &role, &runtime_request, run)?;
        report.validate().map_err(contract_error)?;
        let report = accept_typed_report(request, &report).map_err(contract_error)?;
        let mut report = report;
        if let Some(revision) = request.max_revisions {
            report.revision = Some(revision.min(1));
        }
        report.accepted = Some(true);
        self.persist_checkpoint(
            request,
            CoordinatorState::WaitingParentAcceptance,
            Some(&report),
            None,
        )
        .await?;
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::WaitingParentAcceptance,
            None,
        )
        .await?;
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::Accepted,
            Some("parent_gate".into()),
        )
        .await?;
        self.persist_checkpoint(
            request,
            CoordinatorState::Accepted,
            Some(&report),
            Some("parent_gate".into()),
        )
        .await?;
        self.store_report(request, &report).await?;
        self.persist_handoff(
            request,
            &role,
            session.as_deref(),
            HandoffKind::ReturnResult,
            &report.summary,
            "completed",
        )
        .await?;
        self.publish_report_message(request, &report, session.as_deref())?;

        let status = match report.status {
            TypedReportStatus::Complete => "accepted",
            TypedReportStatus::Partial => "partial",
            TypedReportStatus::Rejected => "rejected",
            TypedReportStatus::Failed => "failed",
        };
        Ok(ChildOutcome {
            status: status.into(),
            summary: report.summary.clone(),
            findings: report.findings.clone(),
            evidence: report.sources.len() as u32,
            report_json: serde_json::to_string(&report).unwrap_or_default(),
        })
    }

    fn build_report(
        &self,
        request: &TypedChildTaskRequest,
        role: &ResolvedRole,
        runtime_request: &ChildTaskRequest,
        run: RoleRunOutput,
    ) -> Result<TypedChildReport, NodeError> {
        let status = run.status();
        let runtime_status = match status {
            TypedReportStatus::Complete => ChildReportStatus::Complete,
            TypedReportStatus::Partial => ChildReportStatus::Partial,
            TypedReportStatus::Rejected | TypedReportStatus::Failed => ChildReportStatus::Rejected,
        };
        let runtime_report = ChildReport {
            child_task_id: request.child_task_id.clone(),
            status: runtime_status,
            summary: run.summary.clone(),
            findings: run.findings.clone(),
            sources: run.sources.clone(),
            confidence_percent: if run.successful_calls > 0 { 60 } else { 40 },
        };
        // The runtime contract is the read-only boundary; it must validate the
        // same report the typed contract is about to accept.
        crate::child_runtime::accept_report(runtime_request, &runtime_report)
            .map_err(runtime_error)?;

        let output_data = json!({
            "status": match runtime_status {
                ChildReportStatus::Complete => "complete",
                ChildReportStatus::Partial => "partial",
                ChildReportStatus::Rejected => "rejected",
            },
            "summary": run.summary.clone(),
            "findings": run.findings.clone(),
        })
        .to_string();
        let evidence_hash = Provenance::compute_hash(&run.sources.join("\n"));
        let provenance = Provenance::new(request.correlation.parent_sequence)
            .with_input_hash(Provenance::compute_hash(&request.purpose))
            .map_err(contract_error)?
            .with_evidence_hash(evidence_hash)
            .map_err(contract_error)?
            .with_tool_version("child-executor/1")
            .map_err(contract_error)?
            .with_schema_version(format!(
                "{}.{}",
                request.contract_version.major, request.contract_version.minor
            ))
            .map_err(contract_error)?
            .mark_completed();
        let evidence = run
            .sources
            .iter()
            .take(crate::child_contracts::MAX_EVIDENCE)
            .map(|source| crate::child_contracts::EvidenceItem {
                locator: source.clone(),
                summary: format!("read-only evidence from role {}", role.profile.id),
                content_hash: Provenance::compute_hash(source),
            })
            .collect();
        let mut report = TypedChildReport::new(
            request.child_task_id.clone(),
            request.parent_task_id.clone(),
            request.correlation.clone(),
            provenance,
        )
        .map_err(contract_error)?
        .with_status(status)
        .with_summary(run.summary)
        .map_err(contract_error)?
        .with_findings(run.findings)
        .map_err(contract_error)?
        .with_sources(run.sources)
        .map_err(contract_error)?
        .with_confidence(if run.successful_calls > 0 { 60 } else { 40 })
        .with_output_data(output_data)
        .map_err(contract_error)?
        .with_evidence(evidence)
        .map_err(contract_error)?;
        if !run.errors.is_empty() {
            report = report.with_errors(run.errors);
        }
        report.revision = Some(0);
        Ok(report)
    }

    async fn store_report(
        &self,
        request: &TypedChildTaskRequest,
        report: &TypedChildReport,
    ) -> Result<(), NodeError> {
        let record = evohime_local_storage::domains::agents::ChildReportRecord {
            child_task_id: report.child_task_id.clone(),
            parent_task_id: request.parent_task_id.clone(),
            status: report_status_str(report.status).to_owned(),
            confidence_percent: report.confidence_percent,
            report_json: serde_json::to_string(report).map_err(|error| {
                NodeError::permanent("child_report_serialization_failed", error.to_string())
            })?,
        };
        self.journal
            .save_child_report(&record)
            .await
            .map_err(|error| NodeError::permanent("child_report_unavailable", error))
    }

    /// Publishes a bounded child-to-parent response on the existing causal
    /// collaboration bus contract. The bus owns validation; no second board
    /// or message store is created here.
    fn publish_report_message(
        &self,
        request: &TypedChildTaskRequest,
        report: &TypedChildReport,
        session: Option<&str>,
    ) -> Result<(), NodeError> {
        use crate::causal_collaboration_bus::{
            canonical_hash, validate, Address, CollaborationMessage, DeliveryState, MessageKind,
            Sensitivity,
        };
        let payload = json!({
            "child_task_id": report.child_task_id,
            "status": report_status_str(report.status),
            "summary": report.summary,
        })
        .to_string()
        .into_bytes();
        let payload_hash = Provenance::compute_hash(&String::from_utf8_lossy(&payload));
        let message = CollaborationMessage {
            schema_version: crate::causal_collaboration_bus::CONTRACT_VERSION,
            message_id: sanitize_id(&format!("{}-response", report.child_task_id)),
            session_id: sanitize_id(session.unwrap_or(&request.parent_task_id)),
            protocol_hash: Provenance::compute_hash(&format!(
                "{}:{}",
                request.parent_task_id, report.child_task_id
            )),
            sender: Address::DirectRoleInstance {
                slot_id: sanitize_id(&report.child_task_id),
                ordinal: 0,
            },
            receiver: Address::Parent,
            kind: MessageKind::Response,
            correlation_id: sanitize_id(request.correlation.task_id.as_str()),
            causation_id: sanitize_id(request.correlation.child_id.as_str()),
            sequence: request.correlation.parent_sequence,
            idempotency_key: sanitize_id(&format!("{}-response", report.child_task_id)),
            payload,
            payload_hash,
            sensitivity: Sensitivity::Internal,
            provenance_id: sanitize_id(&format!("child-executor:{}", report.child_task_id)),
        };
        validate(&message).map_err(|error| {
            NodeError::permanent("child_bus_message_rejected", error.to_string())
        })?;
        let _hash = canonical_hash(&message).map_err(|error| {
            NodeError::permanent("child_bus_message_rejected", error.to_string())
        })?;
        let _delivery = DeliveryState::Consumed;
        Ok(())
    }
}

impl ChildExecutor for RegisteredChildExecutor {
    fn run<'a>(&'a self, request: &'a TypedChildTaskRequest) -> ChildFuture<'a> {
        Box::pin(async move { self.execute(request).await })
    }
}

fn runtime_kind(role: &str) -> ChildTaskKind {
    match role {
        "researcher" => ChildTaskKind::CodeSearch,
        "reviewer" | "security-reviewer" => ChildTaskKind::ThreatModelReview,
        "tester" => ChildTaskKind::TestPlanReview,
        "planner" | "implementer" | "coordinator" => ChildTaskKind::Documentation,
        _ => ChildTaskKind::Documentation,
    }
}

fn role_tool_plan(
    effective: &[String],
    request: &TypedChildTaskRequest,
) -> Vec<(&'static str, Value)> {
    let mut plan = Vec::new();
    let query = request.purpose.chars().take(240).collect::<String>();
    if effective
        .iter()
        .any(|capability| capability == "workspace.search")
    {
        plan.push((
            "filesystem.search",
            json!({ "query": query, "limit": 8, "ignore_case": true }),
        ));
    }
    if effective
        .iter()
        .any(|capability| capability == "workspace.read")
    {
        plan.push(("filesystem.list", json!({ "path": "." })));
    }
    if effective
        .iter()
        .any(|capability| capability == "git.status")
    {
        plan.push(("git.status", json!({})));
    }
    if effective.iter().any(|capability| capability == "git.diff") {
        plan.push(("git.diff", json!({})));
    }
    plan
}

fn collect_tool_evidence(output: &mut RoleRunOutput, tool_name: &str, structured: &Value) {
    match tool_name {
        "filesystem.search" => {
            let count = structured
                .get("matches")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            output
                .findings
                .push(format!("filesystem.search: совпадений {count}"));
            if let Some(matches) = structured.get("matches").and_then(Value::as_array) {
                for item in matches.iter().take(crate::child_contracts::MAX_SOURCES) {
                    let path = item.get("path").and_then(Value::as_str).unwrap_or("");
                    let line = item.get("line").and_then(Value::as_u64).unwrap_or_default();
                    if path.is_empty() {
                        continue;
                    }
                    let source = if line > 0 {
                        format!("{path}:{line}")
                    } else {
                        path.to_owned()
                    };
                    if !output.sources.contains(&source) {
                        output.sources.push(source);
                    }
                }
            }
        }
        "filesystem.list" => {
            output
                .findings
                .push("filesystem.list: каталог workspace прочитан".into());
            output.sources.push("workspace:.".into());
        }
        "git.status" => {
            output
                .findings
                .push("git.status: состояние репозитория прочитано".into());
            output.sources.push("git:status".into());
        }
        "git.diff" => {
            output.findings.push("git.diff: изменения прочитаны".into());
            output.sources.push("git:diff".into());
        }
        _ => output
            .findings
            .push(format!("{tool_name}: read-only вызов завершён")),
    }
    output.sources.sort();
    output.sources.dedup();
    output.sources.truncate(crate::child_contracts::MAX_SOURCES);
}

fn contract_error(error: ContractError) -> NodeError {
    let (code, retryable) = match error {
        ContractError::NestedChildForbidden => ("nested_child_forbidden", false),
        ContractError::GrantEscalation { .. } | ContractError::GrantDrift => {
            ("child_effect_forbidden", false)
        }
        ContractError::BudgetExceedsParent => ("child_cannot_escalate_budget", false),
        ContractError::ForbiddenCapability(_) => ("child_effect_forbidden", false),
        ContractError::OutputTooLarge { .. } | ContractError::ContentTooLarge { .. } => {
            ("child_output_rejected", false)
        }
        _ => ("child_report_rejected", false),
    };
    NodeError {
        code: code.into(),
        message: error.to_string(),
        retryable,
    }
}

fn runtime_error(error: crate::child_runtime::ChildRuntimeError) -> NodeError {
    use crate::child_runtime::ChildRuntimeError;
    let code = match error {
        ChildRuntimeError::NestedChildForbidden => "nested_child_forbidden",
        ChildRuntimeError::ForbiddenCapability(_) => "child_effect_forbidden",
        ChildRuntimeError::OutputTooLarge { .. } => "child_output_rejected",
        ChildRuntimeError::TaskMismatch => "child_report_rejected",
        ChildRuntimeError::SecretLikeContent => "child_report_rejected",
        _ => "child_request_rejected",
    };
    NodeError {
        code: code.into(),
        message: error.to_string(),
        retryable: false,
    }
}

fn role_identity(profile_id: &str) -> Result<RoleIdentity, NodeError> {
    match crate::core_domains::child_role_from_str(profile_id) {
        Ok(role) if role != ChildRole::Custom => Ok(RoleIdentity::builtin(role)),
        _ => RoleIdentity::custom(profile_id)
            .map_err(|error| NodeError::permanent("child_handoff_rejected", error.to_string())),
    }
}

fn role_identity_text(identity: &RoleIdentity) -> String {
    match &identity.name {
        Some(name) => format!("custom:{name}"),
        None => format!("{:?}", identity.role).to_ascii_lowercase(),
    }
}

fn handoff_kind_str(kind: HandoffKind) -> &'static str {
    match kind {
        HandoffKind::Delegate => "delegate",
        HandoffKind::ReturnResult => "return_result",
        HandoffKind::RequestReview => "request_review",
        HandoffKind::RequestRetry => "request_retry",
    }
}

pub(crate) fn report_status_str(status: TypedReportStatus) -> &'static str {
    match status {
        TypedReportStatus::Complete => "complete",
        TypedReportStatus::Partial => "partial",
        TypedReportStatus::Rejected => "rejected",
        TypedReportStatus::Failed => "failed",
    }
}

fn bounded_budget_text(budget: Option<&ChildBudget>) -> String {
    let Some(budget) = budget else {
        return "none".into();
    };
    format!(
        "tokens={} seconds={} calls={}",
        budget
            .max_tokens
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".into()),
        budget
            .max_time_seconds
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".into()),
        budget
            .max_tool_calls
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".into()),
    )
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '/') {
                character
            } else {
                '-'
            }
        })
        .take(128)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child_contracts::{CorrelationContext, CorrelationId, Grant, Schema};

    fn journal() -> (EventJournal, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = EventJournal::open(directory.path().join("core.db")).expect("event journal");
        (journal, directory)
    }

    fn request(role: &str, grants: &[&str]) -> TypedChildTaskRequest {
        TypedChildTaskRequest::new(
            "run-1:worker",
            "run-1",
            role,
            "Проверь файл main.rs",
            CorrelationContext::new(
                CorrelationId::new("run-1").expect("task id"),
                CorrelationId::new("run-1:worker").expect("child id"),
                1,
            ),
        )
        .expect("request")
        .with_context(vec!["workspace root".into()])
        .expect("context")
        .with_capabilities(
            grants
                .iter()
                .filter_map(|grant| canonical_capability(grant).map(str::to_owned))
                .collect(),
        )
        .expect("capabilities")
        .with_grants(
            grants
                .iter()
                .map(|grant| Grant::new(*grant).expect("grant"))
                .collect(),
        )
        .expect("grants")
        .with_budget(
            ChildBudget::new()
                .with_tokens(8_000)
                .with_time(60)
                .with_tool_calls(4),
        )
        .with_output_schema(Schema::new().with_content_type("application/json"))
    }

    fn workspace_with_file() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("workspace");
        std::fs::write(directory.path().join("main.rs"), "fn main() {}\n").expect("write file");
        directory
    }

    #[tokio::test]
    async fn a_queen_dispatches_a_registered_researcher_and_accepts_its_report() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal.clone(), workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_parent_capabilities(["fs.read".to_owned()], None);
        let outcome = executor
            .run(&request("researcher", &["fs.read"]))
            .await
            .expect("child run");
        assert_eq!(outcome.status, "accepted");
        assert!(!outcome.report_json.is_empty());

        let checkpoint = journal
            .get_coordinator_checkpoint("run-1:worker")
            .await
            .expect("checkpoint query")
            .expect("checkpoint");
        assert_eq!(checkpoint.state, "accepted");
        assert!(checkpoint.report_json.is_some());
        let handoffs = journal
            .list_child_handoffs("run-1", 8)
            .await
            .expect("handoffs");
        assert!(handoffs
            .iter()
            .any(|handoff| handoff.kind == "return_result" && handoff.status == "completed"));
    }

    #[tokio::test]
    async fn a_workflow_run_fans_in_an_accepted_child_report() {
        use crate::workflow::{
            ApprovalPolicy, CancellationPolicy, ChildActionProfile, ExecutionPolicy, NodeBudget,
            NodeType, PortType, RetryPolicy, WorkflowBudget, WorkflowEdge, WorkflowGraph,
            WorkflowNode, WORKFLOW_CONTRACT_VERSION,
        };
        struct Approved;
        impl crate::workflow_runtime::WorkflowApprovalGate for Approved {
            fn decide<'a>(
                &'a self,
                _run_id: &'a str,
                _node: &'a WorkflowNode,
            ) -> crate::workflow_runtime::ApprovalFuture<'a> {
                Box::pin(async { crate::workflow_runtime::ApprovalOutcome::Approved })
            }
        }
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let policy = ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts: 1,
                backoff_ms: 0,
                retryable_errors: Vec::new(),
            },
            timeout_ms: 5_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: false,
                reason: None,
            },
        };
        let child = WorkflowNode::new(
            "worker",
            NodeType::Child {
                child: ChildActionProfile {
                    role: "researcher".into(),
                    goal: "Проверь main.rs".into(),
                    output_schema: None,
                    context_allowlist: Vec::new(),
                    artifact_allowlist: Vec::new(),
                    grants: vec!["fs.read".into()],
                    budget: NodeBudget::default(),
                    max_revisions: 1,
                },
            },
            policy.clone(),
        )
        .with_output("report", PortType::Json);
        let merge = WorkflowNode::new("merge", NodeType::Transform, policy)
            .with_input("report", PortType::Json, true)
            .with_output("summary", PortType::Text);
        let graph = WorkflowGraph {
            contract: WORKFLOW_CONTRACT_VERSION.into(),
            graph_id: "child-executor.e2e".into(),
            version: 1,
            entry_node: "worker".into(),
            nodes: vec![child, merge],
            edges: vec![WorkflowEdge::data("worker", "report", "merge", "report")],
            budget: WorkflowBudget::default(),
        };
        let adapter =
            crate::workflow_adapters::CoreNodeAdapter::new(journal.clone(), workspace.path())
                .with_tools(Arc::new(ToolRegistry::bootstrap()));
        let runtime = crate::workflow_runtime::WorkflowRuntime::new(
            journal.clone(),
            Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
            Arc::new(adapter),
            Arc::new(Approved),
            "test",
        );
        runtime
            .start(crate::workflow_runtime::StartWorkflowRequest {
                run_id: "run-e2e".into(),
                task_id: "task-e2e".into(),
                workspace_path: workspace.path().to_string_lossy().into_owned(),
                template_id: "test".into(),
                template_version: 1,
                inputs: BTreeMap::new(),
                graph,
                parent: crate::ipc_bridge::workflow_parent_capabilities(),
            })
            .await
            .expect("start");
        let outcome = runtime.drive("run-e2e").await.expect("drive");
        assert_eq!(
            outcome.state,
            evohime_local_storage::workflow_store::RunState::Completed
        );
        let checkpoint = journal
            .get_coordinator_checkpoint("run-e2e:worker")
            .await
            .expect("checkpoint")
            .expect("accepted checkpoint");
        assert_eq!(checkpoint.state, "accepted");
        let run = journal
            .workflow_run("run-e2e")
            .await
            .expect("run")
            .expect("stored run");
        assert_eq!(
            run.state,
            evohime_local_storage::workflow_store::RunState::Completed
        );
    }

    #[tokio::test]
    async fn nested_child_requests_are_rejected() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
        let mut nested = request("researcher", &["fs.read"]);
        nested.parent_is_child = true;
        let error = executor.run(&nested).await.expect_err("nested child");
        assert_eq!(error.code, "nested_child_forbidden");
    }

    #[tokio::test]
    async fn coordinator_child_cannot_spawn_nested_children() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
        let mut nested = request("coordinator", &["fs.read"]);
        nested.parent_is_child = true;
        let error = executor
            .run(&nested)
            .await
            .expect_err("coordinator child spawn");
        assert_eq!(error.code, "nested_child_forbidden");
    }

    #[test]
    fn nested_child_is_rejected_for_every_builtin_role_before_capability_checks() {
        for role in [
            "coordinator",
            "researcher",
            "planner",
            "implementer",
            "tester",
            "reviewer",
            "security-reviewer",
        ] {
            let mut request = request(role, &["fs.read"]);
            request.parent_is_child = true;
            assert_eq!(
                request.validate(),
                Err(ContractError::NestedChildForbidden),
                "role {role} must not delegate a nested child"
            );
        }
    }

    #[test]
    fn delegation_capabilities_are_outside_the_read_only_contract() {
        for capability in [
            "child.spawn",
            "child.dispatch",
            "agent.spawn",
            "workflow.start",
            "workflow.create",
        ] {
            assert!(
                canonical_capability(capability).is_none(),
                "{capability} must not map to a read-only capability"
            );
        }
        let profile = builtin_profile("coordinator").expect("coordinator profile");
        assert!(profile
            .constraints
            .iter()
            .any(|constraint| constraint == "no_nested_children"));
        for tool in &profile.tools {
            assert!(
                capability_for_tool(tool).is_some(),
                "{tool} must stay inside the read-only tool set"
            );
        }
    }

    #[test]
    fn projected_report_status_is_a_stable_bounded_value() {
        for status in [
            TypedReportStatus::Complete,
            TypedReportStatus::Partial,
            TypedReportStatus::Rejected,
            TypedReportStatus::Failed,
        ] {
            let value = report_status_str(status);
            assert!(!value.is_empty());
            assert!(value.chars().count() <= 16);
        }
    }

    #[tokio::test]
    async fn unregistered_roles_are_rejected() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
        let error = executor
            .run(&request("unregistered-role", &["fs.read"]))
            .await
            .expect_err("unknown role");
        assert_eq!(error.code, "child_role_not_registered");
    }

    #[tokio::test]
    async fn write_shell_network_and_elevation_capabilities_are_rejected() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
        for capability in [
            "workspace.write",
            "fs.write",
            "shell.execute",
            "network.fetch",
            "elevation",
        ] {
            let error = executor
                .run(&request("researcher", &[capability]))
                .await
                .expect_err("forbidden capability");
            assert_eq!(error.code, "child_effect_forbidden", "{capability}");
        }
    }

    #[tokio::test]
    async fn child_budget_cannot_escalate_the_parent_remainder() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_parent_budget(NodeBudget {
                max_tokens: 4_000,
                max_seconds: 60,
                max_tool_calls: 4,
            });
        let error = executor
            .run(&request("researcher", &["fs.read"]))
            .await
            .expect_err("budget escalation");
        assert_eq!(error.code, "child_cannot_escalate_budget");
    }

    #[tokio::test]
    async fn persisted_role_profiles_are_pinned_and_builtin_roles_remain_available() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let mut profile = builtin_profile("researcher").expect("built-in profile");
        profile.revision = 2;
        profile.objective = "Persisted research role".into();
        let hash = profile_hash(&profile).expect("profile hash");
        {
            let database = journal.database.lock().await;
            evohime_local_storage::agent_role_profiles_store::save_revision(
                database.connection(),
                &profile.id,
                profile.revision,
                &hash,
                &serde_json::to_vec(&profile).expect("profile json"),
                1,
            )
            .expect("profile save");
        }
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_parent_capabilities(["fs.read".to_owned()], None);
        let outcome = executor
            .run(&request("researcher", &["fs.read"]))
            .await
            .expect("run");
        assert_eq!(outcome.status, "accepted");
    }

    #[tokio::test]
    async fn a_pinned_team_session_must_match_the_role_profile() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let profile = builtin_profile("researcher").expect("profile");
        let hash = profile_hash(&profile).expect("hash");
        let protocol = crate::team_sop_protocols::TeamProtocol {
            schema_version: crate::team_sop_protocols::CONTRACT_VERSION,
            id: "team-1".into(),
            version: 1,
            name: "Bounded research".into(),
            description: "Read-only research team".into(),
            objective: "Collect evidence".into(),
            participants: vec![crate::team_sop_protocols::TeamParticipantSlot {
                slot_id: "researcher".into(),
                role_profile_ref: crate::team_sop_protocols::RoleProfileRef {
                    id: profile.id.clone(),
                    revision: profile.revision,
                    content_hash: hash.clone(),
                },
                cardinality_min: 1,
                cardinality_max: 1,
                required: true,
                grants_ceiling: READ_ONLY_CAPABILITIES
                    .iter()
                    .map(|v| (*v).to_owned())
                    .collect(),
                allowed_peer_routes: vec!["parent".into()],
            }],
            phases: vec![crate::team_sop_protocols::TeamPhase {
                id: "research".into(),
                name: "Research".into(),
                owners: vec!["researcher".into()],
                trigger: "start".into(),
                required_inputs: vec!["purpose".into()],
                expected_outputs: vec!["report".into()],
                execution_mode: crate::team_sop_protocols::ExecutionMode::Sequential,
                exit_criteria: vec!["report accepted".into()],
                max_review_iterations: 1,
            }],
            handoffs: Vec::new(),
            review_policies: Vec::new(),
            completion_contract: crate::team_sop_protocols::CompletionContract {
                required_phases: vec!["research".into()],
                required_evidence: vec!["report".into()],
            },
            budget_policy: None,
            escalation_policy: None,
            content_hash: String::new(),
        };
        let mut protocol = protocol;
        protocol.content_hash = crate::team_sop_protocols::canonical_hash(&protocol).expect("hash");
        let session = crate::team_sop_protocols::TeamSession {
            id: "session-1".into(),
            snapshot: crate::team_sop_protocols::ProtocolSnapshot {
                protocol_id: "team-1".into(),
                version: 1,
                content_hash: protocol.content_hash.clone(),
                protocol_json: serde_json::to_vec(&protocol).expect("protocol json"),
            },
            current_phase: "research".into(),
            completed_phases: Vec::new(),
            review_iterations: 0,
            status: crate::team_sop_protocols::SessionStatus::Pinned,
            workflow_run_id: Some("run-1".into()),
            version: 1,
        };
        {
            let database = journal.database.lock().await;
            let saved = evohime_local_storage::team_sop_protocols_store::save_session(
                database.connection(),
                evohime_local_storage::team_sop_protocols_store::SaveSessionInput {
                    id: &session.id,
                    protocol_id: "team-1",
                    protocol_version: 1,
                    hash: &protocol.content_hash,
                    snapshot: &serde_json::to_vec(&session).expect("session json"),
                    status: "pinned",
                    phase: &session.current_phase,
                    version: session.version,
                    now_ms: 1,
                },
            )
            .expect("session save");
            assert!(saved);
        }
        let executor = RegisteredChildExecutor::new(journal.clone(), workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_parent_capabilities(["fs.read".to_owned()], None);
        let outcome = executor
            .run(&request("researcher", &["fs.read"]))
            .await
            .expect("run");
        assert_eq!(outcome.status, "accepted");
        let checkpoint = journal
            .get_coordinator_checkpoint("run-1:worker")
            .await
            .expect("checkpoint")
            .expect("accepted checkpoint");
        assert_eq!(checkpoint.state, "accepted");
    }
}
