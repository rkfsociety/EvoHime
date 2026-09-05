pub enum CoreCommand {
    StartTask {
        task_id: String,
        prompt: String,
        workspace_root: Option<PathBuf>,
        preferred_route_hint: Option<String>,
    },
    StopTask {
        task_id: String,
    },
    /// Эпизод постоянного слушания закрылся: разобрать его в кандидатов
    /// памяти (04.6). Ответа нет намеренно — извлечение идёт после того, как
    /// эпизод уже закрыт, и не должно никого ждать.
    ExtractAmbientMemory {
        episode_id: String,
    },
    ResolveRoutingDecision {
        trace_id: String,
        approve: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CreateProject {
        client_id: String,
        request_id: String,
        command_hash: String,
        project_id: String,
        title: String,
        workspace_path: String,
        source_ref: Option<String>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CreateTask {
        client_id: String,
        request_id: String,
        command_hash: String,
        item: WorkItemRecord,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    UpdateTaskStatus {
        client_id: String,
        request_id: String,
        command_hash: String,
        task_id: String,
        expected_version: i64,
        status: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    AddTaskEdge {
        client_id: String,
        request_id: String,
        command_hash: String,
        from_task_id: String,
        to_task_id: String,
        kind: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskGraph {
        project_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    NextReadyTask {
        project_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ImportPrd {
        client_id: String,
        request_id: String,
        command_hash: String,
        import_id: String,
        project_id: String,
        origin: String,
        version: String,
        source_text: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskHistory {
        task_id: String,
        limit: usize,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskContext {
        project_id: String,
        task_id: String,
        max_chars: usize,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskPlanSpec {
        project_id: String,
        task_id: String,
        max_chars: usize,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PlanArtifact {
        operation: String,
        artifact_json: Vec<u8>,
        artifact_id: String,
        expected_version: u64,
        status: String,
        policy_snapshot_hash: String,
        task_id: Option<String>,
        workflow_run_id: Option<String>,
        correlation_id: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    WorkspaceStateCheckpoint {
        operation: String,
        project_id: String,
        task_id: Option<String>,
        checkpoint_id: Option<String>,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    IncrementalChangeProtocol {
        operation: String,
        run_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        observed_fingerprint: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    RevisionSafeWorkspaceFiles {
        operation: String,
        project_id: String,
        logical_path: String,
        content: Vec<u8>,
        expected_hash: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    TaskWorktreeIsolation {
        operation: String,
        project_id: String,
        task_id: String,
        worktree_id: String,
        branch: String,
        base_commit: String,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    TeamResourceBudget {
        operation: String,
        owner_scope: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ComposableTerminationConditions {
        operation: String,
        owner_scope: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    WorkspaceBootstrapManifest {
        operation: String,
        project_id: String,
        workspace_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    TeamCoordinationPolicies {
        operation: String,
        team_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    TypedAgentHandoffContract {
        operation: String,
        handoff_id: String,
        packet_json: Vec<u8>,
        actor: String,
        reason: String,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    SchemaDrivenAgentConfiguration {
        operation: String,
        scope: String,
        payload: Vec<u8>,
        expected_revision: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ExperienceReplayLibrary {
        operation: String,
        scope: String,
        scope_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    RuntimeInterventionPipeline {
        operation: String,
        run_id: String,
        payload: Vec<u8>,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CodeDiagnosticsFeedbackLoop {
        operation: String,
        workspace_root_id: String,
        payload: Vec<u8>,
        baseline_snapshot_id: String,
        expected_revision: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    WorkflowOptimizationLab {
        operation: String,
        run_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CoreTopicSubscriptionEventBus {
        operation: String,
        payload: Vec<u8>,
        capability: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    DependencyAwareTaskGraph {
        operation: String,
        graph_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        grants: Vec<String>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    DeclarativeAgentComponentRegistry {
        operation: String,
        registry_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    TypedContextReferences {
        operation: String,
        ref_id: String,
        payload: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    SafeUiExtensionFramework {
        operation: String,
        extension_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CapabilityWorkbench {
        operation: String,
        instance_id: String,
        owner_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        grants: Vec<String>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    TeamCoordinator {
        operation: String,
        work_item_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ProjectInstructionStack {
        operation: String,
        workspace_root: String,
        payload: Vec<u8>,
        relevant_paths: Vec<String>,
        expected_revision: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    WorkspaceSets {
        operation: String,
        set_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    KnowledgeSourceRegistryProjectRole {
        operation: String,
        source_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    DurableRemoteTaskBridge {
        operation: String,
        remote_task_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    MessageInterventionPolicies {
        operation: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    BatchInvocationRuntime {
        operation: String,
        batch_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PolicyAwareToolResultCache {
        operation: String,
        cache_key: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CodeAnchoredIntentMarkers {
        operation: String,
        file_path: String,
        revision: String,
        payload: Vec<u8>,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ModelPurposeRouting {
        operation: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    LocalModelRuntimeManager {
        operation: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ArchitectureSnapshot {
        operation: String,
        snapshot_id: String,
        workspace_root: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    AgentGitChangeSets {
        operation: String,
        change_set_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ArchitectEditorModelPipeline {
        operation: String,
        pipeline_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    EventVisualizerRegistry {
        operation: String,
        visualizer_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ReasoningOperatorLibrary {
        operation: String,
        operator_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    OutputGuardrailPipeline {
        operation: String,
        pipeline_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CustomizationInventory {
        operation: String,
        item_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    StandingApprovalProfiles {
        operation: String,
        profile_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ApprovalPolicyProfiles {
        operation: String,
        profile_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CheckpointForking {
        operation: String,
        fork_run_id: String,
        payload: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PrivacyTelemetryGovernance {
        operation: String,
        category: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ConversationBridgeAdapters {
        operation: String,
        bridge_id: String,
        payload: Vec<u8>,
        expected_revision: u64,
        idempotency_key: String,
        correlation_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskSnapshot {
        project_id: String,
        task_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    RestoreTaskSnapshot {
        project_id: String,
        task_id: String,
        snapshot_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetBuildPolicy {
        project_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    SaveBuildPolicy {
        project_id: String,
        policy_json: Vec<u8>,
        expected_version: i64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ApplyApprovedBuild {
        project_id: String,
        run_id: String,
        task_id: String,
        approved_build_json: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PrepareBuild {
        project_id: String,
        proposal_json: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded, read-only Core Doctor diagnostic. `project_id` is optional;
    /// when set, the permissions probe is grounded in that project's real
    /// workspace path. `protocol_major`/`expected_protocol_major` and
    /// `provider`/`approval_required` are supplied by the IPC layer, which
    /// is where that state actually lives.
    RunDoctor {
        project_id: String,
        protocol_major: Option<u32>,
        expected_protocol_major: u32,
        provider: crate::doctor::ProviderProbe,
        approval_required: bool,
        registered_tools: u32,
        expected_tools: u32,
        unavailable_tools: Vec<String>,
        detail_level: crate::doctor::DetailLevel,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded, read-only support snapshot. It never persists diagnostics or
    /// reads raw prompts, workspace files, credentials, or tool payloads.
    CreateDiagnosticsSnapshot {
        project_id: String,
        conversation_id: String,
        run_id: String,
        max_event_count: u32,
        max_log_bytes: u32,
        protocol_major: Option<u32>,
        expected_protocol_major: u32,
        provider: crate::doctor::ProviderProbe,
        approval_required: bool,
        registered_tools: u32,
        expected_tools: u32,
        unavailable_tools: Vec<String>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Exports the local `logs/core.jsonl` (and `supervisor.jsonl`, when
    /// present) plus recent `run_tool_metrics` aggregates to a caller-chosen
    /// destination path, redacted the same way hook payloads are. Never
    /// touches eval fixtures or feedback storage.
    ExportDoctorLogs {
        destination_path: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CreateDatabaseBackup {
        operation_id: String,
        destination_path: String,
        progress: mpsc::UnboundedSender<BackupProgress>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PrepareDatabaseRestore {
        operation_id: String,
        backup_path: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    RestoreDatabase {
        operation_id: String,
        backup_path: String,
        approval_id: String,
        progress: mpsc::UnboundedSender<BackupProgress>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CancelDatabaseOperation {
        operation_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Captures one bounded, redacted piece of offline research evidence and
    /// persists it against the real `research_evidence` table, tied to
    /// `work_item_id` via `provenance_link`. Redaction and validation happen
    /// in `research::ResearchEvidence::capture` before anything is stored.
    SaveResearchEvidence {
        work_item_id: String,
        source_kind: String,
        source_ref: String,
        title: String,
        publisher: String,
        content_type: String,
        raw_excerpt: String,
        retrieved_at_ms: u64,
        ttl_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists previously saved research evidence for a work item.
    ListResearchEvidence {
        work_item_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Performs a real, policy-gated, SSRF-protected HTTP GET against `url`,
    /// driving `research_fetch::run_research_fetch` through the real
    /// `research_pipeline` state machine, then persists the resulting
    /// `ResearchEvidence` the same way `SaveResearchEvidence` does. `title`
    /// is caller-supplied; content-type/publisher are derived from the
    /// response and URL. No search-engine integration and no LLM-based
    /// summarization happen here (see `research_fetch` module docs).
    RunResearchFetch {
        work_item_id: String,
        url: String,
        title: String,
        allowed_domains: Vec<String>,
        max_bytes: u64,
        max_latency_ms: u64,
        max_cost_micros: u64,
        ttl_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Creates one bounded Memory v1 record. `memory_domain::MemoryDomain`
    /// runs validation, TTL expansion and content redaction server-side
    /// (its in-memory storage is not used: the real `memory_entries` table,
    /// via `memory_store`, is the sole source of truth); `id` and
    /// `created_at_ms` are computed here, never trusted from the caller.
    CreateMemory {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        title: String,
        content: String,
        provenance_kind: String,
        provenance_id: String,
        provenance_locator: String,
        privacy: String,
        ttl_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists non-forgotten Memory v1 records for one exact scope.
    ListMemory {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        include_archived: bool,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lexical, deterministic search over Memory v1 records for one exact
    /// scope.
    SearchMemory {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        query: String,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Archives a memory record. Per the Memory v1 plan, this requires an
    /// out-of-band approval token (`approval_id`), validated the same way
    /// `memory_api::Approval` validates it: mirrors the `ApplyApprovedBuild`
    /// trust model, where the client presents proof that the operation was
    /// already approved before this command is sent.
    ArchiveMemory {
        id: String,
        approval_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Permanently erases a memory record's title/content. Also requires an
    /// out-of-band approval token; see `ArchiveMemory`. Writes a tombstone
    /// carrying only metadata and a digest.
    ForgetMemory {
        id: String,
        approval_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Creates/inspects a Core-owned MemoryView and records bounded adaptive
    /// recall decisions. The payload contains no memory bodies or credentials.
    MemoryViewsAndAdaptiveRecall {
        operation: String,
        view_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ModelEditProtocolRegistry {
        operation: String,
        protocol_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    RemoteConversationChannels {
        operation: String,
        connection_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PromptCachePlanner {
        operation: String,
        plan_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    DeclarativeRuntimeComponents {
        operation: String,
        component_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GuidedCalibrationSessions {
        operation: String,
        session_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ExtensionConformanceKit {
        operation: String,
        subject_id: String,
        payload: Vec<u8>,
        expected_version: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PersistentAgentOrganizationRegistry {
        operation: String,
        agent_id: String,
        owner_scope: String,
        actor: String,
        payload: Vec<u8>,
        expected_revision: u64,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Reads one memory record including its body. `sensitive`, forgotten and
    /// empty records come back redacted: `ListMemory` never carries a body,
    /// and this is the only path that can.
    GetMemory {
        id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists the pending-confirmation queue plus per-state counters for one
    /// exact scope. Metadata only.
    ListMemoryPending {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        limit: u32,
        /// When non-empty, Core derives the workspace scope id itself, which
        /// is the scope memory extraction writes under.
        workspace_path: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Deterministic conflicts between pending records and the currently
    /// active memory of the same `kind + canonical_subject + scope`. Reading
    /// conflicts never changes any record: an unresolved conflict leaves the
    /// old entry active and the new one pending.
    GetMemoryConflicts {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        limit: u32,
        workspace_path: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Confirms one or more pending records. Requires an out-of-band approval
    /// token (`approval_id`) and an `idempotency_key`; repeating the same
    /// request is safe and reports the actual current state of each id.
    ConfirmMemory {
        ids: Vec<String>,
        approval_id: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Rejects one or more pending records. Same trust model as
    /// `ConfirmMemory`; a rejected record is terminal and never reopens.
    RejectMemory {
        ids: Vec<String>,
        approval_id: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Edits a pending candidate before confirmation, or keeps it only for the
    /// current session. Neither action confirms anything by itself.
    ReviseMemoryCandidate {
        id: String,
        statement: String,
        session_only: bool,
        session_id: String,
        approval_id: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Resolves a conflict by an explicit user choice: `old_id` is superseded
    /// by `new_id` with a mandatory reason. Supersede happens only here, never
    /// automatically.
    SupersedeMemory {
        old_id: String,
        new_id: String,
        reason: String,
        approval_id: String,
        idempotency_key: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Installs (or, when a manifest of the same name already exists,
    /// updates) one bounded capability manifest into the local catalog.
    /// `manifest_json` is validated via
    /// `capability_registry::CapabilityManifest`'s own bounds plus
    /// `validate_registry`/`validate_update` against the manifests already
    /// persisted, before anything is written. `local_archive` carries only
    /// an audit path. `https_archive` treats `source_path` as an HTTPS URL,
    /// downloads it through the shared SSRF guard, and requires the trusted
    /// out-of-band SHA-256 in `expected_content_hash` to match before any
    /// catalog write.
    InstallCapability {
        manifest_json: String,
        install_source: String,
        source_path: String,
        expected_content_hash: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists installed capability manifests, newest-first.
    ListCapabilities {
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Deterministic intent/tool/domain match against the installed
    /// catalog, via `capability_registry::match_capabilities`.
    MatchCapabilities {
        intent: String,
        required_tools: Vec<String>,
        required_domains: Vec<String>,
        requested_risk: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Removes one installed capability manifest by id (manifest name).
    RemoveCapability {
        id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Runtime/UI wiring for capability-registry selection
    /// (`capability_selection::select_for_task`/`reconcile_with_pin`): runs
    /// the deterministic matcher for the query, reconciles against any
    /// selection already persisted for `task_id`, persists the reconciled
    /// state, and returns it.
    GetCapabilitySelection {
        task_id: String,
        intent: String,
        required_tools: Vec<String>,
        required_domains: Vec<String>,
        requested_risk: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Pins the selection persisted for `task_id`
    /// (`capability_selection::pin`) so future `GetCapabilitySelection`
    /// calls cannot silently swap it. Fails if no selection is persisted
    /// yet for `task_id`.
    PinCapabilitySelection {
        task_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Explicitly switches the selection persisted for `task_id` to
    /// `manifest_name` (`capability_selection::replace`), re-deriving
    /// permissions/reasons against the same query.
    ReplaceCapabilitySelection {
        task_id: String,
        manifest_name: String,
        intent: String,
        required_tools: Vec<String>,
        required_domains: Vec<String>,
        requested_risk: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates and persists one bounded, redacted task handoff between
    /// child roles (`child_roles::HandoffEnvelope::new`). This only records
    /// the handoff; it does not deliver or act on it for any real child
    /// agent -- runtime wiring remains a later, dedicated task per
    /// `child_roles.rs`'s own scope note.
    RequestChildHandoff {
        handoff_id: String,
        task_id: String,
        kind: String,
        from_role: String,
        from_name: String,
        to_role: String,
        to_name: String,
        purpose: String,
        payload: std::collections::HashMap<String, String>,
        sequence: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists persisted child handoffs for a task, in sequence order.
    ListChildHandoffs {
        task_id: String,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates (`child_runtime::ChildTaskRequest::validate`) and persists
    /// one bounded, read-only child task request. Rejects any request with
    /// a non-read-only `requested_capabilities` entry, any nested child
    /// (`parent_is_child = true`), or oversized context/output -- the same
    /// pure contract used by the unit tests, enforced end-to-end here. Core
    /// does not act on an accepted request: it is stored as a durable
    /// record of an approved read-only child task descriptor for whatever
    /// later spawns it (out of scope for this task).
    SubmitChildRequest {
        child_task_id: String,
        parent_task_id: String,
        role: String,
        kind: String,
        reduced_context: Vec<String>,
        max_output_bytes: u32,
        requested_capabilities: Vec<String>,
        parent_is_child: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates (`child_runtime::accept_report`, against the matching
    /// stored `SubmitChildRequest`) and persists one child report. Rejects
    /// a task-id mismatch, secret-like content, duplicate sources, or a
    /// missing/invalid matching request.
    SubmitChildReport {
        child_task_id: String,
        status: String,
        summary: String,
        findings: Vec<String>,
        sources: Vec<String>,
        confidence_percent: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Persists one bounded, redacted feedback record (useful/not-useful,
    /// optional correction, optional rejection reason) against the real
    /// `feedback_entries` table. `run_id` must correlate to an existing
    /// `runs.id`; `subject_ref` is an existing tool-call/effect/approval id
    /// when the feedback is about a specific result, not a newly minted
    /// correlation id. Local-only: this command never sends data anywhere,
    /// see `evohime_local_storage::feedback_store::external_telemetry_allowed`.
    SubmitFeedback {
        run_id: String,
        task_id: Option<String>,
        subject_ref: Option<String>,
        signal: String,
        correction: Option<String>,
        rejection_reason: Option<String>,
        outcome: Option<String>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists feedback for one run (newest first) plus the local aggregation
    /// (signal counts, top rejection reasons/outcomes by frequency).
    ListFeedback {
        run_id: String,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Incremental bounded workspace indexing. The scanner and SQLite
    /// generation are owned by Core; UI supplies only the selected root.
    IndexWorkspace {
        workspace_path: String,
        enable_embeddings: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Controlled full rebuild. The previous published generation remains
    /// visible until the new one passes consistency checks and publication.
    RebuildIndex {
        workspace_path: String,
        enable_embeddings: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CancelWorkspaceIndex {
        workspace_path: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded lexical/hybrid retrieval with planner/checker diagnostics and
    /// validated source metadata.
    SearchWorkspaceKnowledge {
        workspace_path: String,
        query: String,
        path_filter: Option<String>,
        language_filter: Option<String>,
        hybrid: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Read-only bounded status projection for the selected workspace.
    GetIndexStatus {
        workspace_path: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// План 01.5: bounded projection состава контекста последних model call.
    GetContextLedger {
        task_id: String,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded чтение scratchpad задачи с фильтром по категории и статусу.
    ListTaskScratchpad {
        task_id: String,
        category: Option<String>,
        status: Option<String>,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Очистка task-scoped scratchpad. Mutation с записью аудита.
    ClearTaskScratchpad {
        task_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Принудительное сжатие текущей сборки контекста задачи.
    SummarizeContextNow {
        task_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// `pin/unpin item`: выставляет флаг `pinned` из 01.1.
    PinContextItem {
        task_id: String,
        item_id: String,
        pinned: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Чтение полного содержимого артефакта с повторной policy-проверкой.
    ReadContextArtifact {
        task_id: String,
        locator: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    RetainChild {
        child: crate::retained_child::RetainedChildV1,
        now_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetRetainedChild {
        parent_id: String,
        child_id: String,
        now_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    SendChildFollowUp {
        request: crate::retained_child::ChildFollowUpRequestV1,
        now_ms: u64,
        busy: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ListRetainedChildren {
        parent_id: String,
        now_ms: u64,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    DeleteRetainedChild {
        parent_id: String,
        child_id: String,
        expected_registry_version: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CoreEvent {
    ModelContext {
        task_id: String,
        workspace_path: String,
        model: String,
        system_prompt: String,
        user_prompt: String,
        tools: Vec<String>,
        estimated_tokens: usize,
        context_limit_tokens: usize,
        /// План 01.5: additive bounded projection состава контекста. Старые
        /// клиенты игнорируют неизвестное поле, поэтому major bump не нужен.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<Box<crate::context_budget::ModelContextProjection>>,
    },
    /// Terminal Core-owned routing decision. Intermediate attempts stay in
    /// diagnostics; the renderer receives this bounded projection only.
    RoutingTrace {
        task_id: String,
        trace: evohime_model_gateway::RoutingTrace,
    },
    /// Non-terminal request to approve a policy-controlled reroute.
    PendingRoutingApproval {
        task_id: String,
        trace_id: String,
        run_id: String,
        route_id: String,
        expires_at_ms: u64,
    },
    TaskStarted {
        task_id: String,
        prompt: String,
    },
    AssistantDelta {
        task_id: String,
        content: String,
    },
    ToolStarted {
        task_id: String,
        tool_name: String,
    },
    ToolOutput {
        task_id: String,
        tool_name: String,
        output: String,
    },
    ApprovalRequired {
        task_id: String,
        approval_id: String,
        tool_name: String,
        permission: String,
        scope: String,
        preview: evohime_permissions::ApprovalPreview,
    },
    TaskCompleted {
        task_id: String,
        final_message: String,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },
    TaskStopped {
        task_id: String,
    },
    ReviewProgress {
        review_id: String,
        stage: String,
        status: String,
        model: Option<String>,
        completed: usize,
        total: usize,
    },
    RevisionProgress {
        revision_id: String,
        status: String,
        model: String,
    },
    StorageProgress {
        operation_id: String,
        progress: BackupProgress,
    },
    WorkspaceIndexProgress {
        workspace_path: String,
        progress: crate::workspace_rag::IndexProgress,
    },
    WorkspaceRetrievalProgress {
        workspace_path: String,
        progress: crate::workspace_rag::RetrievalProgress,
    },
    /// Bounded Core-owned child workflow projection for UI/timeline consumers.
    ChildWorkflowProjection {
        task_id: String,
        projection: crate::child_workflow::ChildProjection,
    },
    /// Bounded projection события durable workflow run (план 06.2).
    ///
    /// Полезная нагрузка ограничена идентификаторами, состояниями и кодами:
    /// ни prompt, ни сырой вывод child, ни содержимое контекста в неё не
    /// попадают.
    WorkflowProgress {
        run_id: String,
        projection: Box<crate::workflow_runtime::WorkflowEventProjection>,
    },
    /// Bounded durable projection for workspace bootstrap lifecycle changes.
    WorkspaceBootstrapManifest {
        workspace_id: String,
        operation: String,
        status: String,
        manifest_id: String,
        revision: u64,
        content_hash: String,
        projection_json: String,
    },
    PolicyAwareToolResultCache {
        operation: String,
        cache_key: String,
        version: u64,
        projection_json: String,
    },
    CodeAnchoredIntentMarkers {
        operation: String,
        version: u64,
        projection_json: String,
    },
    ModelPurposeRouting {
        operation: String,
        version: u64,
        projection_json: String,
    },
    LocalModelRuntimeManager {
        operation: String,
        version: u64,
        projection_json: String,
    },
    ArchitectureSnapshot {
        snapshot_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    TypedAgentHandoffContract {
        handoff_id: String,
        operation: String,
        state: String,
        version: u64,
        projection_json: String,
    },
    SchemaDrivenAgentConfiguration {
        scope: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    ExperienceReplayLibrary {
        scope: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    RuntimeInterventionPipeline {
        run_id: String,
        operation: String,
        projection_json: String,
    },
    CodeDiagnosticsFeedbackLoop {
        workspace_root_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    WorkflowOptimizationLab {
        run_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    CoreTopicSubscriptionEventBus {
        operation: String,
        projection_json: String,
    },
    DependencyAwareTaskGraph {
        graph_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    DeclarativeAgentComponentRegistry {
        registry_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    TypedContextReferences {
        ref_id: String,
        operation: String,
        projection_json: String,
    },
    SafeUiExtensionFramework {
        extension_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    CapabilityWorkbench {
        instance_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    TeamCoordinator {
        work_item_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    ProjectInstructionStack {
        workspace_root: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    WorkspaceSets {
        set_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    KnowledgeSourceRegistryProjectRole {
        source_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    DurableRemoteTaskBridge {
        remote_task_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    MessageInterventionPolicies {
        operation: String,
        version: u64,
        projection_json: String,
    },
    BatchInvocationRuntime {
        batch_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    AgentGitChangeSets {
        change_set_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    ArchitectEditorModelPipeline {
        pipeline_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    EventVisualizerRegistry {
        visualizer_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    ReasoningOperatorLibrary {
        operator_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    OutputGuardrailPipeline {
        pipeline_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    CustomizationInventory {
        item_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    StandingApprovalProfiles {
        profile_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    ApprovalPolicyProfiles {
        profile_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    CheckpointForking {
        fork_run_id: String,
        operation: String,
        version: u64,
        projection_json: String,
    },
    PrivacyTelemetryGovernance {
        operation: String,
        category: String,
        version: u64,
        projection_json: String,
    },
    ConversationBridgeAdapters {
        operation: String,
        bridge_id: String,
        revision: u64,
        projection_json: String,
    },
    MemoryViewsAndAdaptiveRecall {
        operation: String,
        view_id: String,
        version: u64,
        projection_json: String,
    },
    ModelEditProtocolRegistry {
        operation: String,
        protocol_id: String,
        version: u64,
        projection_json: String,
    },
    RemoteConversationChannels {
        operation: String,
        connection_id: String,
        version: u64,
        projection_json: String,
    },
    PromptCachePlanner {
        operation: String,
        plan_id: String,
        version: u64,
        projection_json: String,
    },
    DeclarativeRuntimeComponents {
        operation: String,
        component_id: String,
        version: u64,
        projection_json: String,
    },
    GuidedCalibrationSessions {
        operation: String,
        session_id: String,
        version: u64,
        projection_json: String,
    },
    ExtensionConformanceKit {
        operation: String,
        subject_id: String,
        version: u64,
        projection_json: String,
    },
    PersistentAgentOrganizationRegistry {
        agent_id: String,
        operation: String,
        revision: u64,
        projection_json: String,
    },
    /// Bounded durable routing decision projection.
    TeamCoordinationPolicies {
        team_id: String,
        operation: String,
        status: String,
        version: u64,
        projection_json: String,
    },
    /// Marks the point after which review history is shown. The journal is
    /// append-only, so clearing hides earlier reviews instead of deleting them.
    ReviewHistoryCleared {
        marker_id: String,
    },
}