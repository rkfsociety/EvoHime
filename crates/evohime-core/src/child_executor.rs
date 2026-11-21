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
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::agent_role_profiles::{
    canonical_hash as profile_hash, AgentRoleProfile, AgentRoleProfilesRegistry, BudgetDefaults,
    ContractField, ExecutionMode, RunState, StartRuntimeInput,
};
use crate::child_contracts::{
    accept_typed_report, ChildBudget, ContractError, CorrelationContext, CorrelationId,
    DelegationGrant, Provenance, TypedChildReport, TypedChildTaskRequest, TypedReportStatus,
};
use crate::child_roles::{
    ChildRole, HandoffEnvelope, HandoffEnvelopeInput, HandoffKind, HandoffPayload, RoleIdentity,
};
use crate::child_runtime::{ChildReport, ChildReportStatus, ChildTaskKind, ChildTaskRequest};
use crate::child_workflow::{Coordinator, CoordinatorState};
use crate::hive_settings::HiveSettings;
use crate::lazy_delegation::{
    DelegationAuthority, DelegationDecision, DelegationExtensionRequest, DelegationProgressPolicy,
    LazyDelegationSnapshot, LazyDelegationState, QueenDelegationExtension,
};
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
    "git_diff",
    "git_status",
];

/// Maps a grant or capability name to its canonical read-only capability.
///
/// The function returns `None` for write, shell, network, elevation, or
/// unknown names. Callers must treat `None` as a policy rejection, not as an
/// empty capability.
pub(crate) fn canonical_capability(value: &str) -> Option<&'static str> {
    match value.split(':').next().unwrap_or(value) {
        "eva_self_development"
        | "eva_workspace_read_file"
        | "eva_workspace_write_file"
        | "eva_workspace_run_command"
        | "eva_workspace_git" => None,
        "fs.read" | "workspace.read" | "filesystem_read" | "filesystem_list" => {
            Some("workspace.read")
        }
        "workspace.search" | "filesystem_search" => Some("workspace.search"),
        "git_diff" => Some("git_diff"),
        "git_status" => Some("git_status"),
        // Durable child requests, receipts, and grants written before the tool
        // ids were renamed still carry the dotted spelling. They resolve to the
        // same read-only capability and never grant anything else.
        "filesystem.read" | "filesystem.list" => Some("workspace.read"),
        "filesystem.search" => Some("workspace.search"),
        "git.diff" => Some("git_diff"),
        "git.status" => Some("git_status"),
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
                "filesystem_search",
                "filesystem_list",
                "git_status",
                "git_diff",
            ],
        ),
        "researcher" => (
            "Gather and summarize bounded repository evidence",
            vec![
                "filesystem_search",
                "filesystem_list",
                "git_status",
                "git_diff",
            ],
        ),
        "planner" => (
            "Draft a bounded plan without applying changes",
            vec!["filesystem_search", "filesystem_list"],
        ),
        "implementer" => (
            "Prepare an authorized change without writing as a child",
            vec![
                "filesystem_search",
                "filesystem_list",
                "git_status",
                "git_diff",
            ],
        ),
        "tester" => (
            "Review a bounded test plan without executing it",
            vec![
                "filesystem_search",
                "filesystem_list",
                "git_status",
                "git_diff",
            ],
        ),
        "reviewer" => (
            "Review bounded changes against requirements",
            vec![
                "filesystem_search",
                "filesystem_list",
                "git_status",
                "git_diff",
            ],
        ),
        "security-reviewer" => (
            "Review bounded trust boundaries without mutating state",
            vec![
                "filesystem_search",
                "filesystem_list",
                "git_status",
                "git_diff",
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

/// Bounded summary of one sub-agent the worker was allowed to spawn.
struct DelegatedSummary {
    child_task_id: String,
    status: String,
    summary: String,
}

/// Bounded output of the read-only role run before it becomes a typed report.
struct RoleRunOutput {
    summary: String,
    findings: Vec<String>,
    sources: Vec<String>,
    errors: Vec<String>,
    successful_calls: u32,
    delegated: Vec<DelegatedSummary>,
    lazy: LazyDelegationSnapshot,
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

/// Default queen authority: never widen a worker ceiling automatically.
///
/// A worker that exhausts its ceiling finishes with a partial report. Widening
/// the grant is a separate, explicit queen decision made through
/// [`crate::lazy_delegation::extend_delegation`].
struct DenyDelegationExtensions;

impl DelegationAuthority for DenyDelegationExtensions {
    fn decide_extension(
        &self,
        _request: &DelegationExtensionRequest,
        _grant: &DelegationGrant,
        _settings: &HiveSettings,
        _workflow_spawned: u32,
    ) -> Result<Option<QueenDelegationExtension>, ContractError> {
        Ok(None)
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
    delegation_authority: Arc<dyn DelegationAuthority>,
    delegation_policy: DelegationProgressPolicy,
    workflow_spawned: Option<Arc<AtomicU32>>,
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
            delegation_authority: Arc::new(DenyDelegationExtensions),
            delegation_policy: DelegationProgressPolicy::default(),
            workflow_spawned: None,
        }
    }

    /// Overrides the deterministic progress assessment.
    ///
    /// Production uses [`DelegationProgressPolicy::StopOnComplete`]: one
    /// complete sub-agent report is enough. Other values exist for the queen's
    /// own assessment and for tests that need a fixed number of spawns.
    pub fn with_delegation_policy(mut self, policy: DelegationProgressPolicy) -> Self {
        self.delegation_policy = policy;
        self
    }

    /// Installs the queen-side authority consulted when a worker reports that
    /// its ceiling is exhausted.
    ///
    /// The default denies every extension: widening a worker ceiling is an
    /// explicit queen decision, never an automatic consequence of a request.
    pub fn with_delegation_authority(mut self, authority: Arc<dyn DelegationAuthority>) -> Self {
        self.delegation_authority = authority;
        self
    }

    /// Shares a workflow-wide sub-agent counter between child executors.
    ///
    /// The counter is the last guard in [`HiveSettings`]: every reserved spawn
    /// increments it, and a worker may not start a sub-agent once the workflow
    /// ceiling is reached.
    pub fn with_workflow_subagent_counter(mut self, counter: Arc<AtomicU32>) -> Self {
        self.workflow_spawned = Some(counter);
        self
    }

    fn workflow_spawned(&self) -> u32 {
        self.workflow_spawned
            .as_ref()
            .map(|counter| counter.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    fn note_workflow_spawn(&self) {
        if let Some(counter) = &self.workflow_spawned {
            counter.fetch_add(1, Ordering::Relaxed);
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

    fn projection_sink(&self) -> crate::child_workflow::ChildProjectionSink<'_> {
        match &self.coordinator {
            Some(coordinator) => {
                crate::child_workflow::ChildProjectionSink::Coordinator(coordinator)
            }
            None => crate::child_workflow::ChildProjectionSink::Journal(&self.journal),
        }
    }

    async fn persist_checkpoint(
        &self,
        request: &TypedChildTaskRequest,
        state: CoordinatorState,
        report: Option<&TypedChildReport>,
        reason: Option<String>,
        lazy: LazyDelegationSnapshot,
    ) -> Result<(), NodeError> {
        crate::child_workflow::persist_child_checkpoint(
            &self.journal,
            self.projection_sink(),
            request,
            state,
            report,
            reason,
            lazy,
        )
        .await
        .map_err(crate::child_workflow::ChildLifecycleError::into_node_error)
    }

    async fn transition(
        &self,
        coordinator: &mut Coordinator,
        request: &TypedChildTaskRequest,
        next: CoordinatorState,
        reason: Option<String>,
        lazy: LazyDelegationSnapshot,
    ) -> Result<(), NodeError> {
        coordinator
            .transition(&request.child_task_id, next, reason.clone())
            .map_err(|error| {
                NodeError::permanent("child_transition_rejected", format!("{error:?}"))
            })?;
        self.persist_checkpoint(request, next, None, reason, lazy)
            .await
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
        crate::child_workflow::save_handoff_envelope(&self.journal, &envelope, status)
            .await
            .map_err(crate::child_workflow::ChildLifecycleError::into_node_error)
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
            eva_workspace_root: None,
            self_development_profile: None,
            agent_depth: 1,
            self_development_test_gate: std::sync::Arc::new(tokio::sync::Mutex::new(
                evohime_tool_runtime::self_development::TestGate::default(),
            )),
        };
        let plan = role_tool_plan(&role.effective_grants, request);
        let mut output = RoleRunOutput {
            summary: String::new(),
            findings: Vec::new(),
            sources: Vec::new(),
            errors: Vec::new(),
            successful_calls: 0,
            delegated: Vec::new(),
            lazy: LazyDelegationSnapshot::default(),
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

    /// Returns the worker budget remainder after its own read-only calls.
    fn remaining_budget(
        &self,
        request: &TypedChildTaskRequest,
        run: &RoleRunOutput,
        elapsed: Duration,
    ) -> ChildBudget {
        let mut budget = request.budget.clone().unwrap_or_default();
        if let Some(calls) = budget.max_tool_calls {
            budget.max_tool_calls = Some(calls.saturating_sub(u64::from(run.successful_calls)));
        }
        if let Some(seconds) = budget.max_time_seconds {
            budget.max_time_seconds = Some(seconds.saturating_sub(elapsed.as_secs()));
        }
        budget
    }

    /// Runs the lazy delegation loop permitted by the queen's grant.
    ///
    /// The worker never converts the ceiling into a batch. It reserves one
    /// sub-agent budget share, runs exactly one sub-agent, waits for its
    /// accepted report, and only then decides:
    /// - finish when the report solves the task and keep the unused budget;
    /// - continue while the ceiling and the remainder allow another sub-agent;
    /// - ask the queen when the ceiling is exhausted, and finish when she
    ///   declines.
    ///
    /// Every reservation is recorded in [`LazyDelegationSnapshot`], which the
    /// caller projects to the parent. The loop is bounded by the queen ceiling,
    /// the hive safety guard, and the worker budget remainder, so it always
    /// terminates.
    async fn run_delegated_lazily(
        &self,
        request: &TypedChildTaskRequest,
        run: &mut RoleRunOutput,
        elapsed: Duration,
    ) -> Result<(), NodeError> {
        let Some(grant) = request.delegation.clone() else {
            return Ok(());
        };
        let settings = self
            .journal
            .hive_settings()
            .await
            .map_err(|error| NodeError::permanent("hive_settings_unavailable", error))?;
        let capacity = self.remaining_budget(request, run, elapsed);
        let mut grant = grant;
        let mut state =
            LazyDelegationState::new(&grant, capacity.clone(), &settings, self.workflow_spawned())
                .map_err(contract_error)?;
        let policy = self.delegation_policy;
        let mut usages = Vec::new();

        loop {
            // The workflow-wide guard is the last safety net: another worker
            // may have spent shared capacity since this worker started.
            if self.workflow_spawned() >= settings.max_total_subagents_per_workflow {
                break;
            }
            let allocation = match state.try_spawn(grant.budget.clone()) {
                Ok(allocation) => allocation,
                // The ceiling or the remainder is the queen-decided stopping
                // point; the worker finishes with what it already collected.
                Err(ContractError::DelegationCeilingExceeded { .. })
                | Err(ContractError::DelegationBudgetExhausted)
                | Err(ContractError::DelegationParallelLimitExceeded { .. }) => break,
                Err(error) => return Err(contract_error(error)),
            };
            self.note_workflow_spawn();
            let nested =
                delegated_request(request, &grant, &capacity, &allocation, state.spawned())?;
            let started = Instant::now();
            let outcome = self.run(&nested).await?;
            let report =
                serde_json::from_str::<TypedChildReport>(&outcome.report_json).map_err(|_| {
                    NodeError::permanent(
                        "child_report_rejected",
                        "the sub-agent did not return a typed accepted report",
                    )
                })?;
            merge_delegated_output(run, &report);
            let usage = observed_usage(&allocation, started.elapsed());
            usages.push(usage.clone());
            state.complete(&allocation, &usage);

            match state.assess(policy, report.status) {
                DelegationDecision::Finish => break,
                DelegationDecision::Continue => {
                    state.record_lazy_wait();
                }
                DelegationDecision::RequestExtension => {
                    match self
                        .ask_queen(request, &mut state, &grant, &settings)
                        .await?
                    {
                        Some(extension) => {
                            grant = extension.grant().clone();
                            state
                                .apply_queen_extension(&extension)
                                .map_err(contract_error)?;
                        }
                        None => {
                            state.cancel_extension_request();
                            break;
                        }
                    }
                }
            }
        }

        // The consumed budget must stay inside the remainder the worker owned
        // before the fan-out started, even across an approved extension. Unused
        // reservations are refunded, so the sum is measured on actual usage.
        crate::lazy_delegation::validate_allocations_within(&usages, &capacity)
            .map_err(contract_error)?;
        run.lazy = state.snapshot();
        Ok(())
    }

    /// Asks the queen-side authority whether the worker ceiling may grow.
    ///
    /// A worker that cannot even form a request (for example because its
    /// budget, not its ceiling, is exhausted) gets no extension. The authority
    /// is explicit: the default executor denies, so a runtime never widens a
    /// grant on its own.
    async fn ask_queen(
        &self,
        worker: &TypedChildTaskRequest,
        state: &mut LazyDelegationState,
        grant: &DelegationGrant,
        settings: &HiveSettings,
    ) -> Result<Option<QueenDelegationExtension>, NodeError> {
        let Ok(mut request) = state.request_extension("worker reached its delegated ceiling")
        else {
            return Ok(None);
        };
        request.child_task_id = worker.child_task_id.clone();
        request.parent_task_id = worker.parent_task_id.clone();
        request.validate().map_err(contract_error)?;
        // Publish the pending request before the queen decides. The authority
        // call is synchronous, so without this checkpoint the
        // `extension_requested` flag would never reach a projection.
        self.persist_checkpoint(
            worker,
            CoordinatorState::Running,
            None,
            Some("delegation_extension_requested".into()),
            state.snapshot(),
        )
        .await?;
        self.delegation_authority
            .decide_extension(&request, grant, settings, self.workflow_spawned())
            .map_err(contract_error)
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
        self.persist_checkpoint(
            request,
            CoordinatorState::Created,
            None,
            None,
            LazyDelegationSnapshot::default(),
        )
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
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::Queued,
            None,
            LazyDelegationSnapshot::default(),
        )
        .await?;
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::Running,
            None,
            LazyDelegationSnapshot::default(),
        )
        .await?;

        let started = Instant::now();
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
                        LazyDelegationSnapshot::default(),
                    )
                    .await;
                return Err(error);
            }
        };
        let mut run = run;
        if let Err(error) = self
            .run_delegated_lazily(request, &mut run, started.elapsed())
            .await
        {
            let _ = self
                .transition(
                    &mut coordinator,
                    request,
                    CoordinatorState::Failed,
                    Some(error.code.clone()),
                    LazyDelegationSnapshot::default(),
                )
                .await;
            return Err(error);
        }
        let lazy = run.lazy;
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::Validating,
            None,
            lazy,
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
            lazy,
        )
        .await?;
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::WaitingParentAcceptance,
            None,
            lazy,
        )
        .await?;
        self.transition(
            &mut coordinator,
            request,
            CoordinatorState::Accepted,
            Some("parent_gate".into()),
            lazy,
        )
        .await?;
        // Accepted reports go through the same shared writer as the legacy
        // `SubmitChildReport` command: acceptance, storage, checkpoint,
        // projection, and the bounded collaboration message.
        let report = self
            .record_accepted_report(request, &report, session.as_deref(), lazy)
            .await?;
        self.persist_handoff(
            request,
            &role,
            session.as_deref(),
            HandoffKind::ReturnResult,
            &report.summary,
            "completed",
        )
        .await?;

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

        let delegated_children: Vec<Value> = run
            .delegated
            .iter()
            .map(|delegated| {
                json!({
                    "child_task_id": delegated.child_task_id,
                    "status": delegated.status,
                    "summary": delegated.summary,
                })
            })
            .collect();
        let output_data = json!({
            "status": match runtime_status {
                ChildReportStatus::Complete => "complete",
                ChildReportStatus::Partial => "partial",
                ChildReportStatus::Rejected => "rejected",
            },
            "summary": run.summary.clone(),
            "findings": run.findings.clone(),
            // Legacy single-sub-agent field kept for replay compatibility.
            "delegated_child": delegated_children.first().cloned(),
            "delegated_children": delegated_children,
            "spawned_subagents": run.lazy.spawned_subagents,
            "planned_ceiling": run.lazy.planned_ceiling,
            "extensions_granted": run.lazy.extensions_granted,
            "lazy_waiting": run.lazy.lazy_waiting,
            "in_flight_subagents": run.lazy.in_flight_subagents,
            "extension_requested": run.lazy.extension_requested,
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

    pub(crate) async fn record_accepted_report(
        &self,
        request: &TypedChildTaskRequest,
        report: &TypedChildReport,
        session: Option<&str>,
        lazy: LazyDelegationSnapshot,
    ) -> Result<TypedChildReport, NodeError> {
        crate::child_workflow::record_accepted_child_report(
            &self.journal,
            self.projection_sink(),
            request,
            report,
            session,
            crate::task_memory::now_millis() as i64,
            lazy,
        )
        .await
        .map_err(crate::child_workflow::ChildLifecycleError::into_node_error)
    }
}

impl ChildExecutor for RegisteredChildExecutor {
    fn run<'a>(&'a self, request: &'a TypedChildTaskRequest) -> ChildFuture<'a> {
        Box::pin(async move {
            // The runtime marker is created inside this task only. IPC and
            // legacy command handlers never observe it, so `parent_is_child`
            // from the wire cannot claim a child context it does not have.
            //
            // Depth is owned by the runtime, not by the wire: a top-level
            // queen delegation creates depth 1, and a worker running inside a
            // marker may create exactly one depth-2 sub-agent. The sub-agent
            // request must be marked `parent_is_child` and the parent context
            // must carry the queen's delegation grant.
            let parent = crate::child_execution_context::current();
            let depth = match &parent {
                None => {
                    if request.parent_is_child {
                        return Err(contract_error(ContractError::NestedChildForbidden));
                    }
                    1
                }
                Some(context) => {
                    if !request.parent_is_child {
                        return Err(contract_error(ContractError::NestedChildForbidden));
                    }
                    context.depth().saturating_add(1)
                }
            };
            // TODO: заменить на чтение из HiveSettings::max_delegation_depth
            if depth > crate::child_contracts::MAX_CHILD_DEPTH {
                return Err(contract_error(ContractError::ChildDepthExceeded));
            }
            if let Some(context) = &parent {
                if !context.delegation_permitted() {
                    return Err(contract_error(ContractError::DelegationNotGranted));
                }
            }
            let context = crate::child_execution_context::ChildExecutionContext::new(
                request.child_task_id.clone(),
                request.parent_task_id.clone(),
                depth,
                // TODO: заменить на чтение из HiveSettings::max_delegation_depth
                request.has_delegation_grant() && depth < crate::child_contracts::MAX_CHILD_DEPTH,
            );
            crate::child_execution_context::scope(context, self.execute(request)).await
        })
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

/// Stable identifier of one lazily spawned sub-agent.
///
/// The first sub-agent keeps the legacy `:hop2` name so replayed reports and
/// existing projections stay readable; later siblings get a bounded ordinal.
/// Colon stays because it already separates task and child segments in the
/// existing correlation ids. The suffix is bounded so the contract id limit is
/// never exceeded.
fn delegated_child_id(parent: &str, ordinal: u32) -> String {
    let suffix = if ordinal <= 1 {
        ":hop2".to_owned()
    } else {
        format!(":hop2-{ordinal}")
    };
    let available = crate::child_contracts::MAX_ID_CHARS.saturating_sub(suffix.len());
    let mut id: String = parent.chars().take(available).collect();
    id.push_str(&suffix);
    id
}

/// Builds one permitted sub-agent request from the queen's grant.
///
/// The worker cannot widen anything: grants are checked as a subset of the
/// worker's own grants, the queen grant budget is checked against the worker
/// budget, and the request budget is the deterministic share reserved by
/// [`LazyDelegationState::try_spawn`] rather than a new wallet. The nested
/// request never carries its own delegation, so depth 3 stays unreachable.
fn delegated_request(
    request: &TypedChildTaskRequest,
    grant: &DelegationGrant,
    remaining: &ChildBudget,
    allocation: &ChildBudget,
    ordinal: u32,
) -> Result<TypedChildTaskRequest, NodeError> {
    crate::child_contracts::validate_grant_subset(&grant.grants, &request.grants)
        .map_err(contract_error)?;
    // The queen grant must fit the worker remainder, never exceed it.
    crate::child_contracts::validate_budget_subset(&grant.budget, &Some(remaining.clone()))
        .map_err(contract_error)?;
    // The reserved allocation is a share of the remainder; narrowing it by the
    // grant keeps the sub-agent inside both limits.
    let budget = match &grant.budget {
        Some(grant_budget) => narrow_budget(grant_budget, allocation),
        None => allocation.clone(),
    };

    let child_task_id = delegated_child_id(&request.child_task_id, ordinal);
    let correlation = CorrelationContext::new(
        CorrelationId::new(request.child_task_id.clone()).map_err(contract_error)?,
        CorrelationId::new(child_task_id.clone()).map_err(contract_error)?,
        request.correlation.parent_sequence.saturating_add(1),
    );
    let mut nested = TypedChildTaskRequest::new(
        child_task_id,
        request.child_task_id.clone(),
        grant.role.clone(),
        grant.purpose.clone(),
        correlation,
    )
    .map_err(contract_error)?
    .with_context(request.reduced_context.clone())
    .map_err(contract_error)?
    .with_max_output_bytes(
        request
            .max_output_bytes
            .min(crate::child_contracts::MAX_OUTPUT_BYTES),
    )
    .map_err(contract_error)?
    .with_capabilities(grant.requested_capabilities.clone())
    .map_err(contract_error)?
    .with_grants(grant.grants.clone())
    .map_err(contract_error)?
    .with_budget(budget)
    .with_input_context_ids(request.input_context_ids.clone())
    .map_err(contract_error)?
    .with_output_offload(request.allow_output_offload, request.output_privacy.clone());
    if let Some(criteria) = &grant.acceptance_criteria {
        nested = nested
            .with_acceptance_criteria(criteria.clone())
            .map_err(contract_error)?;
    }
    nested.parent_is_child = true;
    nested.validate().map_err(contract_error)?;
    Ok(nested)
}

/// Narrows a request budget field-by-field so a missing grant field inherits
/// the worker remainder instead of becoming an unbounded wallet.
fn narrow_budget(child: &ChildBudget, parent: &ChildBudget) -> ChildBudget {
    fn lower(child: Option<u64>, parent: Option<u64>) -> Option<u64> {
        match (child, parent) {
            (Some(child), Some(parent)) => Some(child.min(parent)),
            (None, parent) => parent,
            (child, None) => child,
        }
    }
    ChildBudget {
        max_tokens: lower(child.max_tokens, parent.max_tokens),
        max_time_seconds: lower(child.max_time_seconds, parent.max_time_seconds),
        max_tool_calls: lower(child.max_tool_calls, parent.max_tool_calls),
    }
}

/// Merges the accepted sub-agent report into the worker report before fan-in.
fn merge_delegated_output(run: &mut RoleRunOutput, report: &TypedChildReport) {
    let status = report_status_str(report.status).to_owned();
    let summary: String = report
        .summary
        .chars()
        .take(crate::child_workflow::MAX_PROJECTION_SUMMARY_CHARS)
        .collect();
    if run.findings.len() < crate::child_contracts::MAX_CONTEXT_ITEMS {
        run.findings
            .push(format!("подагент {}: {status}", report.child_task_id));
    }
    for finding in &report.findings {
        if run.findings.len() >= crate::child_contracts::MAX_CONTEXT_ITEMS {
            break;
        }
        run.findings.push(finding.clone());
    }
    for source in &report.sources {
        if run.sources.len() >= crate::child_contracts::MAX_SOURCES {
            break;
        }
        if !run.sources.contains(source) {
            run.sources.push(source.clone());
        }
    }
    run.sources.sort();
    run.sources.dedup();
    run.summary = format!(
        "{} Подагент {}: {}.",
        run.summary, report.child_task_id, summary
    )
    .chars()
    .take(crate::child_contracts::MAX_REPORT_CHARS)
    .collect();
    // TODO: заменить на чтение из HiveSettings::max_delegation_depth
    if run.delegated.len() < crate::hive_settings::MAX_SUBAGENT_HARD_LIMIT as usize {
        run.delegated.push(DelegatedSummary {
            child_task_id: report.child_task_id.clone(),
            status,
            summary,
        });
    }
}

/// Budget consumed by one finished sub-agent run.
///
/// The token and tool-call reservations are treated as fully consumed because
/// the accepted child report is the only observable boundary; unused wall-clock
/// time is returned to the worker pool by [`LazyDelegationState::complete`].
fn observed_usage(allocation: &ChildBudget, elapsed: Duration) -> ChildBudget {
    let seconds = elapsed.as_secs();
    ChildBudget {
        max_tokens: allocation.max_tokens,
        // The reservation is the cap: bookkeeping overhead of the wait must not
        // report more wall-clock time than the sub-agent was allowed to spend.
        max_time_seconds: Some(
            allocation
                .max_time_seconds
                .map_or(seconds, |limit| seconds.min(limit)),
        ),
        max_tool_calls: allocation.max_tool_calls,
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
            "filesystem_search",
            json!({ "query": query, "limit": 8, "ignore_case": true }),
        ));
    }
    if effective
        .iter()
        .any(|capability| capability == "workspace.read")
    {
        plan.push(("filesystem_list", json!({ "path": "." })));
    }
    if effective
        .iter()
        .any(|capability| capability == "git_status")
    {
        plan.push(("git_status", json!({})));
    }
    if effective.iter().any(|capability| capability == "git_diff") {
        plan.push(("git_diff", json!({})));
    }
    plan
}

fn collect_tool_evidence(output: &mut RoleRunOutput, tool_name: &str, structured: &Value) {
    match tool_name {
        "filesystem_search" => {
            let count = structured
                .get("matches")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            output
                .findings
                .push(format!("filesystem_search: совпадений {count}"));
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
        "filesystem_list" => {
            output
                .findings
                .push("filesystem_list: каталог workspace прочитан".into());
            output.sources.push("workspace:.".into());
        }
        "git_status" => {
            output
                .findings
                .push("git_status: состояние репозитория прочитано".into());
            output.sources.push("git:status".into());
        }
        "git_diff" => {
            output.findings.push("git_diff: изменения прочитаны".into());
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
        ContractError::ChildContextRequired => ("child_context_missing", false),
        ContractError::ChildDepthExceeded => ("child_depth_exceeded", false),
        ContractError::DelegationNotGranted => ("delegation_not_granted", false),
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
        ChildRuntimeError::ChildContextRequired => "child_context_missing",
        ChildRuntimeError::ChildDepthExceeded => "child_depth_exceeded",
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

    fn delegation_grant(grants: &[&str], budget: Option<ChildBudget>) -> DelegationGrant {
        DelegationGrant {
            role: "researcher".into(),
            purpose: "Проверь тестовый модуль".into(),
            grants: grants
                .iter()
                .map(|grant| Grant::new(*grant).expect("delegation grant"))
                .collect(),
            requested_capabilities: grants
                .iter()
                .filter_map(|grant| canonical_capability(grant).map(str::to_owned))
                .collect(),
            budget,
            acceptance_criteria: None,
            max_children: 1,
        }
    }

    fn workspace_with_file() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("workspace");
        std::fs::write(directory.path().join("main.rs"), "fn main() {}\n").expect("write file");
        directory
    }

    /// Worker request with a budget wide enough to finance a lazy fan-out.
    fn worker_with_ceiling(ceiling: u32, tool_calls: u64) -> TypedChildTaskRequest {
        request("researcher", &["fs.read", "filesystem_search"])
            .with_budget(
                ChildBudget::new()
                    .with_tokens(16_000)
                    .with_time(120)
                    .with_tool_calls(tool_calls),
            )
            .with_delegation(DelegationGrant {
                max_children: ceiling,
                ..delegation_grant(
                    &["filesystem_search"],
                    Some(
                        ChildBudget::new()
                            .with_tokens(4_000)
                            .with_time(30)
                            .with_tool_calls(2),
                    ),
                )
            })
            .expect("queen delegation grant")
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
                    delegation: crate::workflow::ChildDelegationPolicy::default(),
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
    fn nested_child_claim_without_runtime_context_is_rejected_for_every_builtin_role() {
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
                Err(ContractError::ChildContextRequired),
                "role {role} must not be able to claim a child context from the wire"
            );
        }
    }

    #[tokio::test]
    async fn depth_two_delegation_completes_and_fans_in_through_the_worker() {
        use crate::child_workflow::{ChildProjection, CoordinatorState};
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal.clone(), workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
        let hop1 = request("researcher", &["fs.read", "filesystem_search"])
            .with_delegation(delegation_grant(
                &["filesystem_search"],
                Some(
                    ChildBudget::new()
                        .with_tokens(4_000)
                        .with_time(30)
                        .with_tool_calls(2),
                ),
            ))
            .expect("queen delegation grant");

        let outcome = executor.run(&hop1).await.expect("worker with sub-agent");
        assert_eq!(outcome.status, "accepted");

        let worker_checkpoint = journal
            .get_coordinator_checkpoint("run-1:worker")
            .await
            .expect("worker checkpoint query")
            .expect("worker checkpoint");
        assert_eq!(worker_checkpoint.state, "accepted");
        let sub_checkpoint = journal
            .get_coordinator_checkpoint("run-1:worker:hop2")
            .await
            .expect("sub-agent checkpoint query")
            .expect("sub-agent checkpoint");
        assert_eq!(sub_checkpoint.state, "accepted");

        let mut projections: Vec<ChildProjection> = journal
            .replay(0, 512)
            .await
            .expect("journal replay")
            .into_iter()
            .filter(|record| record.event_type == "child.workflow")
            .filter_map(|record| serde_json::from_slice(&record.payload).ok())
            .collect();
        projections.retain(|projection| projection.state == CoordinatorState::Accepted);
        let worker = projections
            .iter()
            .find(|projection| {
                projection.child_task_id == "run-1:worker" && projection.report_status.is_some()
            })
            .expect("worker accepted projection");
        let sub = projections
            .iter()
            .find(|projection| {
                projection.child_task_id == "run-1:worker:hop2"
                    && projection.report_status.is_some()
            })
            .expect("sub-agent accepted projection");
        assert_eq!(worker.depth, 1);
        assert_eq!(sub.depth, 2);
        assert_eq!(worker.report_status.as_deref(), Some("complete"));
        assert_eq!(sub.report_status.as_deref(), Some("complete"));
        for projection in [worker, sub] {
            let summary = projection
                .report_summary
                .as_deref()
                .expect("bounded projection summary");
            assert!(summary.chars().count() <= 512);
            assert_eq!(
                projection.event_id,
                format!("{}:accepted", projection.child_task_id)
            );
        }

        let report: TypedChildReport =
            serde_json::from_str(&outcome.report_json).expect("worker typed report");
        let output: serde_json::Value =
            serde_json::from_str(report.output_data.as_deref().expect("output data"))
                .expect("output json");
        assert_eq!(
            output["delegated_child"]["child_task_id"],
            serde_json::json!("run-1:worker:hop2")
        );
        assert_eq!(
            output["delegated_child"]["status"],
            serde_json::json!("complete")
        );
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.contains("run-1:worker:hop2")));
    }

    #[tokio::test]
    async fn a_worker_with_ceiling_eight_stops_after_three_subagents() {
        use crate::child_workflow::{ChildProjection, CoordinatorState};

        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal.clone(), workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_delegation_policy(DelegationProgressPolicy::StopAfter(3));

        let outcome = executor
            .run(&worker_with_ceiling(8, 16))
            .await
            .expect("lazy fan-out");
        assert_eq!(outcome.status, "accepted");

        let report: TypedChildReport =
            serde_json::from_str(&outcome.report_json).expect("worker typed report");
        let output: serde_json::Value =
            serde_json::from_str(report.output_data.as_deref().expect("output data"))
                .expect("output json");
        assert_eq!(output["spawned_subagents"], json!(3));
        assert_eq!(output["planned_ceiling"], json!(8));
        assert_eq!(output["extensions_granted"], json!(0));
        assert_eq!(output["lazy_waiting"], json!(2));
        let children = output["delegated_children"]
            .as_array()
            .expect("delegated children");
        assert_eq!(children.len(), 3);
        assert_eq!(children[0]["child_task_id"], json!("run-1:worker:hop2"));
        assert_eq!(children[2]["child_task_id"], json!("run-1:worker:hop2-3"));

        let worker_projection = journal
            .replay(0, 1024)
            .await
            .expect("journal replay")
            .into_iter()
            .filter(|record| record.event_type == "child.workflow")
            .filter_map(|record| serde_json::from_slice::<ChildProjection>(&record.payload).ok())
            .rfind(|projection| {
                projection.child_task_id == "run-1:worker"
                    && projection.state == CoordinatorState::Accepted
            })
            .expect("accepted worker projection");
        assert_eq!(worker_projection.spawned_subagents, 3);
        assert_eq!(worker_projection.planned_ceiling, 8);
        assert_eq!(worker_projection.lazy_waiting, 2);
        assert!(!worker_projection.extension_requested);
        assert_eq!(worker_projection.in_flight_subagents, 0);
    }

    #[tokio::test]
    async fn a_worker_stops_at_the_ceiling_when_the_queen_declines_the_extension() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal.clone(), workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_delegation_policy(DelegationProgressPolicy::StopAfter(5));

        let outcome = executor
            .run(&worker_with_ceiling(2, 16))
            .await
            .expect("ceilings stop the fan-out");
        assert_eq!(outcome.status, "accepted");

        let report: TypedChildReport =
            serde_json::from_str(&outcome.report_json).expect("worker typed report");
        let output: serde_json::Value =
            serde_json::from_str(report.output_data.as_deref().expect("output data"))
                .expect("output json");
        assert_eq!(output["spawned_subagents"], json!(2));
        assert_eq!(output["planned_ceiling"], json!(2));
        assert_eq!(output["extensions_granted"], json!(0));
        // The default queen authority declined: the worker is not waiting.
        assert_eq!(output["extension_requested"], json!(false));
        assert_eq!(
            output["delegated_children"]
                .as_array()
                .expect("delegated children")
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn a_queen_can_extend_the_ceiling_from_two_to_five() {
        use crate::lazy_delegation::{extend_delegation, DelegationExtensionRequest};

        struct QueenExtendsToFive;
        impl DelegationAuthority for QueenExtendsToFive {
            fn decide_extension(
                &self,
                request: &DelegationExtensionRequest,
                grant: &DelegationGrant,
                settings: &HiveSettings,
                workflow_spawned: u32,
            ) -> Result<Option<QueenDelegationExtension>, ContractError> {
                let widened = DelegationExtensionRequest::new(
                    request.child_task_id.clone(),
                    request.parent_task_id.clone(),
                    request.current_ceiling,
                    5,
                    request.spawned,
                    "queen widens the ceiling once",
                )?;
                extend_delegation(grant, &widened, settings, workflow_spawned).map(Some)
            }
        }

        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal.clone(), workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_delegation_authority(Arc::new(QueenExtendsToFive))
            .with_delegation_policy(DelegationProgressPolicy::StopAfter(5));

        let outcome = executor
            .run(&worker_with_ceiling(2, 16))
            .await
            .expect("queen extension");
        assert_eq!(outcome.status, "accepted");

        let report: TypedChildReport =
            serde_json::from_str(&outcome.report_json).expect("worker typed report");
        let output: serde_json::Value =
            serde_json::from_str(report.output_data.as_deref().expect("output data"))
                .expect("output json");
        assert_eq!(output["spawned_subagents"], json!(5));
        assert_eq!(output["planned_ceiling"], json!(5));
        assert_eq!(output["extensions_granted"], json!(1));
        let children = output["delegated_children"]
            .as_array()
            .expect("delegated children");
        assert_eq!(children.len(), 5);
        assert_eq!(children[4]["child_task_id"], json!("run-1:worker:hop2-5"));

        // The pending request is observable before the queen answers, so the
        // operations panel can show "ожидает решения" instead of only the
        // final counters.
        let pending = journal
            .replay(0, 2048)
            .await
            .expect("journal replay")
            .into_iter()
            .filter(|record| record.event_type == "child.workflow")
            .filter_map(|record| {
                serde_json::from_slice::<crate::child_workflow::ChildProjection>(&record.payload)
                    .ok()
            })
            .find(|projection| {
                projection.child_task_id == "run-1:worker" && projection.extension_requested
            })
            .expect("pending extension request projection");
        assert_eq!(pending.spawned_subagents, 2);
        assert_eq!(pending.planned_ceiling, 2);
        assert_eq!(pending.extensions_granted, 0);
    }

    #[tokio::test]
    async fn the_workflow_guard_stops_fan_out_across_workers() {
        let (journal, _database_dir) = journal();
        let settings = HiveSettings {
            max_subagents_per_worker: 8,
            max_parallel_subagents: 4,
            max_total_subagents_per_workflow: 4,
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("persist hive settings");
        let workspace = workspace_with_file();
        // Another worker already spent two of the four workflow-wide slots.
        let counter = Arc::new(AtomicU32::new(2));
        let executor = RegisteredChildExecutor::new(journal.clone(), workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())))
            .with_delegation_policy(DelegationProgressPolicy::StopAfter(5))
            .with_workflow_subagent_counter(Arc::clone(&counter));

        let outcome = executor
            .run(&worker_with_ceiling(4, 16))
            .await
            .expect("guarded fan-out");
        assert_eq!(outcome.status, "accepted");
        assert_eq!(counter.load(Ordering::Relaxed), 4);

        let report: TypedChildReport =
            serde_json::from_str(&outcome.report_json).expect("worker typed report");
        let output: serde_json::Value =
            serde_json::from_str(report.output_data.as_deref().expect("output data"))
                .expect("output json");
        assert_eq!(output["spawned_subagents"], json!(2));
        assert_eq!(output["planned_ceiling"], json!(2));
    }

    #[tokio::test]
    async fn a_worker_without_a_delegation_grant_cannot_spawn_hop_two() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
        let mut nested = request("researcher", &["fs.read"]);
        nested.parent_is_child = true;
        let context = crate::child_execution_context::ChildExecutionContext::new(
            "run-1:worker",
            "run-1",
            1,
            false,
        );
        let error = crate::child_execution_context::scope(context, executor.run(&nested))
            .await
            .expect_err("missing queen grant");
        assert_eq!(error.code, "delegation_not_granted");
    }

    #[tokio::test]
    async fn a_sub_agent_at_depth_two_cannot_spawn_hop_three() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path())
            .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
        let mut nested = request("researcher", &["fs.read"]);
        nested.parent_is_child = true;
        let context = crate::child_execution_context::ChildExecutionContext::new(
            "run-1:worker:hop2",
            "run-1:worker",
            2,
            true,
        );
        let error = crate::child_execution_context::scope(context, executor.run(&nested))
            .await
            .expect_err("depth three must be rejected");
        assert_eq!(error.code, "child_depth_exceeded");
        assert_eq!(
            error.message,
            "nested delegation beyond one extra hop is forbidden"
        );
    }

    fn finished_run(successful_calls: u32) -> RoleRunOutput {
        RoleRunOutput {
            summary: "worker".into(),
            findings: Vec::new(),
            sources: Vec::new(),
            errors: Vec::new(),
            successful_calls,
            delegated: Vec::new(),
            lazy: LazyDelegationSnapshot::default(),
        }
    }

    #[tokio::test]
    async fn delegated_request_uses_subset_grants_and_the_worker_remainder() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path());
        let hop1 = request("researcher", &["fs.read", "filesystem_search"]);
        let remaining =
            executor.remaining_budget(&hop1, &finished_run(1), std::time::Duration::ZERO);
        assert_eq!(remaining.max_tool_calls, Some(3), "one call already spent");

        let grant = delegation_grant(
            &["filesystem_search"],
            Some(
                ChildBudget::new()
                    .with_tokens(4_000)
                    .with_time(30)
                    .with_tool_calls(2),
            ),
        );
        let context = crate::child_execution_context::ChildExecutionContext::new(
            "run-1:worker",
            "run-1",
            1,
            true,
        );
        let nested = crate::child_execution_context::scope(context, async {
            delegated_request(&hop1, &grant, &remaining, &remaining, 1)
        })
        .await
        .expect("subset delegation");
        assert!(nested.parent_is_child);
        assert!(nested.delegation.is_none());
        assert!(nested.grants_are_subset_of(&hop1.grants));
        assert_eq!(nested.grants.len(), 1);
        assert_eq!(nested.grants[0].grant_type, "filesystem_search");
        let budget = nested.budget.as_ref().expect("nested budget");
        assert_eq!(budget.max_tokens, Some(4_000));
        assert_eq!(budget.max_tool_calls, Some(2));
        assert!(budget.is_within_parent(hop1.budget.as_ref().expect("worker budget")));
        assert!(budget.max_tool_calls.unwrap_or(u64::MAX) <= remaining.max_tool_calls.unwrap_or(0));
    }

    #[tokio::test]
    async fn hop_two_cannot_escalate_grants_or_budget() {
        let (journal, _database_dir) = journal();
        let workspace = workspace_with_file();
        let executor = RegisteredChildExecutor::new(journal, workspace.path());
        let hop1 = request("researcher", &["fs.read", "filesystem_search"]);
        let remaining =
            executor.remaining_budget(&hop1, &finished_run(0), std::time::Duration::ZERO);
        let context = crate::child_execution_context::ChildExecutionContext::new(
            "run-1:worker",
            "run-1",
            1,
            true,
        );

        let wider_grants = crate::child_execution_context::scope(context.clone(), async {
            delegated_request(
                &hop1,
                &delegation_grant(&["workspace.write"], None),
                &remaining,
                &remaining,
                1,
            )
        })
        .await
        .expect_err("grant escalation");
        assert_eq!(wider_grants.code, "child_effect_forbidden");

        let bigger_budget = crate::child_execution_context::scope(context, async {
            delegated_request(
                &hop1,
                &delegation_grant(
                    &["filesystem_search"],
                    Some(
                        ChildBudget::new()
                            .with_tokens(9_000)
                            .with_time(30)
                            .with_tool_calls(2),
                    ),
                ),
                &remaining,
                &remaining,
                1,
            )
        })
        .await
        .expect_err("budget escalation");
        assert_eq!(bigger_budget.code, "child_cannot_escalate_budget");
    }

    #[tokio::test]
    async fn hop_two_cannot_request_write_shell_network_or_elevation() {
        for capability in [
            "workspace.write",
            "shell_execute",
            "network.fetch",
            "elevation",
        ] {
            let (journal, _database_dir) = journal();
            let workspace = workspace_with_file();
            let executor = RegisteredChildExecutor::new(journal, workspace.path())
                .with_tools(Some(Arc::new(ToolRegistry::bootstrap())));
            let hop1 = request("researcher", &["fs.read", capability]);
            let remaining =
                executor.remaining_budget(&hop1, &finished_run(0), std::time::Duration::ZERO);
            let grant = delegation_grant(&[capability], None);
            let context = crate::child_execution_context::ChildExecutionContext::new(
                "run-1:worker",
                "run-1",
                1,
                true,
            );
            let nested = crate::child_execution_context::scope(context.clone(), async {
                delegated_request(&hop1, &grant, &remaining, &remaining, 1)
            })
            .await
            .expect("contract shape is built before the runtime policy check");
            let error = crate::child_execution_context::scope(context, executor.run(&nested))
                .await
                .expect_err("forbidden nested capability");
            assert_eq!(error.code, "child_effect_forbidden", "{capability}");
        }
    }

    #[test]
    fn delegation_capabilities_are_outside_the_read_only_contract() {
        for capability in [
            "child.spawn",
            "child.dispatch",
            "agent.spawn",
            "workflow_start",
            "workflow.create",
        ] {
            assert!(
                canonical_capability(capability).is_none(),
                "{capability} must not map to a read-only capability"
            );
        }
        let profile = builtin_profile("coordinator").expect("coordinator profile");
        assert!(profile
            .tools
            .iter()
            .all(|tool| !tool.starts_with("eva_workspace_")));
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
            "shell_execute",
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
