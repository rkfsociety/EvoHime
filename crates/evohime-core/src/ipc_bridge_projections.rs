
pub(crate) fn validate_skill_workspace(
    value: &str,
) -> Result<std::path::PathBuf, crate::skill_registry::SkillRegistryError> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err(crate::skill_registry::SkillRegistryError::UnsafePath(
            "workspace".into(),
        ));
    }
    let path = std::path::Path::new(value);
    if !path.is_absolute() || !path.is_dir() {
        return Err(crate::skill_registry::SkillRegistryError::UnsafePath(
            "workspace".into(),
        ));
    }
    path.canonicalize()
        .map_err(|error| crate::skill_registry::SkillRegistryError::Io(error.to_string()))
}

pub(crate) fn skill_metadata_projection(
    metadata: crate::skill_registry::SkillMetadataV1,
) -> generated::SkillMetadataProjection {
    generated::SkillMetadataProjection {
        schema_version: metadata.schema_version,
        skill_id: bounded_skill_field(&metadata.skill_id),
        name: bounded_skill_field(&metadata.name),
        description: bounded_skill_field(&metadata.description),
        version: bounded_skill_field(&metadata.version),
        scope: bounded_skill_field(&metadata.scope),
        source_kind: metadata.source_kind.as_str().into(),
        source_ref: bounded_skill_field(&metadata.source_ref),
        content_hash: bounded_skill_field(&metadata.content_hash),
        allowed_tools: bounded_skill_list(metadata.allowed_tools),
        required_capabilities: bounded_skill_list(metadata.required_capabilities),
        disable_model_invocation: metadata.disable_model_invocation,
        reference_count: metadata.reference_count.min(u32::MAX as usize) as u32,
        validation_status: match serde_json::to_string(&metadata.validation_status) {
            Ok(value) => value.trim_matches('"').into(),
            Err(error) => {
                tracing::warn!(%error, "failed to serialize model validation status");
                "invalid".into()
            }
        },
        validation_error_code: metadata.validation_error_code.unwrap_or_default(),
        warnings: metadata
            .warnings
            .into_iter()
            .take(16)
            .map(|warning| bounded_skill_field(&warning))
            .collect(),
        trust_decision: bounded_skill_field(&metadata.trust_decision),
        risk_class: bounded_skill_field(&metadata.risk_class),
        findings_count: metadata.findings_count.min(u32::MAX as usize) as u32,
    }
}

pub(crate) fn skill_diagnostic_projection(
    diagnostic: crate::skill_registry::SkillDiagnostic,
) -> generated::SkillDiagnosticProjection {
    generated::SkillDiagnosticProjection {
        code: bounded_skill_field(&diagnostic.code),
        skill_id: bounded_skill_field(&diagnostic.skill_id),
        source_kind: diagnostic.source_kind.as_str().into(),
        source_ref: bounded_skill_field(&diagnostic.source_ref),
        message: bounded_skill_field(&diagnostic.message),
    }
}

pub(crate) fn bounded_skill_field(value: &str) -> String {
    value.chars().take(512).collect()
}
pub(crate) fn bounded_skill_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .take(crate::skill_registry::MAX_LIST_ITEMS)
        .map(|value| bounded_skill_field(&value))
        .collect()
}
pub(crate) fn skill_content_error(skill_id: &str, code: &str) -> generated::SkillContentResult {
    generated::SkillContentResult {
        schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
        skill_id: bounded_skill_field(skill_id),
        error_code: code.into(),
        error_message: "Skill не удалось загрузить; содержимое не выдано.".into(),
        ..Default::default()
    }
}
pub(crate) fn skill_reference_error(
    skill_id: &str,
    reference: &str,
    code: &str,
) -> generated::SkillReferenceResult {
    generated::SkillReferenceResult {
        schema_version: crate::skill_registry::SKILL_SCHEMA_VERSION,
        skill_id: bounded_skill_field(skill_id),
        reference: bounded_skill_field(reference),
        error_code: code.into(),
        error_message: "Reference не удалось загрузить; содержимое не выдано.".into(),
        ..Default::default()
    }
}

pub(crate) fn valid_goal_token(value: &str) -> bool {
    valid_checkpoint_token(value, crate::goal::GOAL_MAX_ID_CHARS)
}

pub(crate) fn valid_goal_action(goal_id: &str, expected_version: u64, idempotency_key: &str) -> bool {
    valid_goal_token(goal_id) && expected_version > 0 && valid_goal_token(idempotency_key)
}

pub(crate) fn goal_criteria_from_request(
    criteria: &[generated::GoalCriterionInput],
) -> Result<Vec<crate::goal::GoalCriterionV1>, ()> {
    criteria
        .iter()
        .map(|criterion| {
            let kind = match criterion.kind.as_str() {
                "manual" => crate::goal::GoalCriterionKind::Manual,
                "gate" => crate::goal::GoalCriterionKind::Gate,
                "workflow_evidence" => crate::goal::GoalCriterionKind::WorkflowEvidence,
                "artifact" => crate::goal::GoalCriterionKind::Artifact,
                _ => return Err(()),
            };
            if !valid_goal_token(&criterion.id)
                || criterion.statement.trim().is_empty()
                || criterion.statement.len() > crate::goal::GOAL_MAX_TEXT_CHARS
            {
                return Err(());
            }
            Ok(crate::goal::GoalCriterionV1::new(
                &criterion.id,
                kind,
                &criterion.statement,
            ))
        })
        .collect()
}

pub(crate) fn goal_storage_error_code(error: &StorageError) -> &'static str {
    match error {
        StorageError::Goal(error) => error.code(),
        StorageError::VersionConflict { .. } => "stale_version",
        StorageError::DeduplicationConflict { .. } => "idempotency_conflict",
        _ => "storage_failed",
    }
}

pub(crate) fn goal_storage_error_message(error: &StorageError) -> String {
    match error {
        StorageError::Goal(crate::goal::GoalError::NotFound(_)) => "Цель не найдена.".into(),
        StorageError::Goal(crate::goal::GoalError::ReferenceNotFound { .. }) => {
            "Связанный runtime-объект не найден или недоступен.".into()
        }
        StorageError::VersionConflict { .. } => {
            "Состояние цели уже изменилось; обнови проекцию.".into()
        }
        StorageError::DeduplicationConflict { .. } => {
            "Ключ idempotency уже использован для другой команды.".into()
        }
        StorageError::Goal(crate::goal::GoalError::CompletionEvidenceMissing) => {
            "Цель нельзя завершить без подтверждённых Core evidence.".into()
        }
        StorageError::Goal(crate::goal::GoalError::InvalidField { .. }) => {
            "Контракт цели нарушен.".into()
        }
        _ => "Операция с целью не записалась.".into(),
    }
}

pub(crate) fn goal_projection_error(goal_id: &str, error_code: &str) -> generated::GoalProjection {
    generated::GoalProjection {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: if valid_goal_token(goal_id) {
            goal_id.to_owned()
        } else {
            String::new()
        },
        error_code: error_code.into(),
        recovery_warning: "Проекция цели недоступна; автоматическое продолжение запрещено.".into(),
        ..Default::default()
    }
}

pub(crate) fn goal_projection(
    goal: &crate::goal::GoalV1,
    recovery_warning: &str,
) -> generated::GoalProjection {
    generated::GoalProjection {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: bounded_checkpoint_text(&goal.id),
        version: goal.version,
        workspace_id: bounded_checkpoint_text(&goal.workspace_id),
        chat_id: goal.chat_id.clone().unwrap_or_default(),
        objective: bounded_checkpoint_text(&goal.objective),
        success_criteria: goal
            .success_criteria
            .iter()
            .take(crate::goal::GOAL_MAX_CRITERIA)
            .map(goal_criterion_projection)
            .collect(),
        status: goal.status.as_str().into(),
        progress_summary: bounded_checkpoint_text(&goal.progress_summary),
        completed_criteria: goal
            .completed_criteria
            .iter()
            .take(crate::goal::GOAL_MAX_CRITERIA)
            .cloned()
            .collect(),
        remaining_criteria: goal
            .remaining_criteria
            .iter()
            .take(crate::goal::GOAL_MAX_CRITERIA)
            .cloned()
            .collect(),
        blockers: goal
            .blockers
            .iter()
            .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
            .map(|value| bounded_checkpoint_text(value))
            .collect(),
        next_action: goal.next_action.clone().unwrap_or_default(),
        workflow_run_ids: goal
            .workflow_run_ids
            .iter()
            .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
            .cloned()
            .collect(),
        child_run_ids: goal
            .child_run_ids
            .iter()
            .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
            .cloned()
            .collect(),
        checkpoint_id: goal.checkpoint_id.clone().unwrap_or_default(),
        token_budget: goal.token_budget.unwrap_or_default(),
        cost_budget_micros: goal.cost_budget_micros.unwrap_or_default(),
        continuation_budget: goal.continuation_budget.unwrap_or_default(),
        created_at_ms: goal.created_at_ms,
        updated_at_ms: goal.updated_at_ms,
        content_hash: bounded_checkpoint_text(&goal.content_hash),
        recovery_warning: bounded_checkpoint_text(recovery_warning),
        error_code: String::new(),
    }
}

pub(crate) fn goal_criterion_projection(
    criterion: &crate::goal::GoalCriterionV1,
) -> generated::GoalCriterionProjection {
    generated::GoalCriterionProjection {
        id: bounded_checkpoint_text(&criterion.id),
        kind: criterion.kind.as_str().into(),
        statement: bounded_checkpoint_text(&criterion.statement),
        status: criterion.status.as_str().into(),
        evidence_ref: criterion.evidence_ref.clone().unwrap_or_default(),
        verifier_id: criterion.verifier_id.clone().unwrap_or_default(),
        verifier_version: criterion.verifier_version.clone().unwrap_or_default(),
        verified_at_ms: criterion.verified_at_ms.unwrap_or_default(),
        provenance: match criterion.provenance {
            crate::goal::GoalProvenance::User => "user",
            crate::goal::GoalProvenance::Core => "core",
        }
        .into(),
    }
}

pub(crate) fn goal_action_result_from_mutation(
    result: crate::goal::GoalMutationResult,
) -> generated::GoalActionResult {
    generated::GoalActionResult {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: bounded_checkpoint_text(&result.goal.id),
        action: bounded_checkpoint_text(&result.action),
        applied: result.applied,
        deduplicated: result.deduplicated,
        goal_version: result.goal.version,
        sequence_id: result.event_sequence,
        goal: Some(goal_projection(&result.goal, "")),
        ..Default::default()
    }
}

pub(crate) fn goal_action_error(
    goal_id: &str,
    action: &str,
    error_code: &str,
    error_message: &str,
) -> generated::GoalActionResult {
    generated::GoalActionResult {
        schema_version: crate::goal::GOAL_SCHEMA_VERSION,
        goal_id: if valid_goal_token(goal_id) {
            goal_id.to_owned()
        } else {
            String::new()
        },
        action: bounded_checkpoint_text(action),
        error_code: bounded_checkpoint_text(error_code),
        error_message: bounded_checkpoint_text(error_message),
        ..Default::default()
    }
}

pub(crate) fn valid_checkpoint_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
}

pub(crate) fn valid_checkpoint_workspace(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.bytes().any(|byte| byte.is_ascii_control())
}

pub(crate) fn analysis_kernel_projection_error(code: &str) -> generated::AnalysisKernelProjection {
    generated::AnalysisKernelProjection {
        schema_version: crate::analysis_kernel::ANALYSIS_KERNEL_SCHEMA_VERSION,
        error_code: code.into(),
        ..Default::default()
    }
}

pub(crate) fn refinement_projection(
    row: evohime_local_storage::refinement_store::CandidateRow,
) -> generated::RefinementProjection {
    generated::RefinementProjection {
        schema_version: crate::refinement::CONTRACT_VERSION,
        candidate_id: row.id,
        revision: row.revision as u64,
        owner_scope: row.owner_scope,
        kind: row.kind,
        target: row.target,
        status: row.status,
        pattern_key: row.pattern_key,
        title: row.title,
        evidence_count: row.evidence_count,
        conflict_count: row.conflict_count,
        confidence: row.confidence,
        content_hash: row.content_hash,
        policy_snapshot_hash: row.policy_snapshot_hash,
        version: row.version as u64,
        error_code: row.error_code.unwrap_or_default(),
        updated_at_ms: row.updated_at_ms,
    }
}

pub(crate) fn refinement_projection_error(code: &str) -> generated::RefinementProjection {
    generated::RefinementProjection {
        schema_version: crate::refinement::CONTRACT_VERSION,
        error_code: code.into(),
        ..Default::default()
    }
}

pub(crate) fn refinement_action_error(
    request: &generated::RefinementAction,
    code: &str,
) -> generated::RefinementActionResult {
    generated::RefinementActionResult {
        schema_version: crate::refinement::CONTRACT_VERSION,
        candidate_id: request.candidate_id.clone(),
        revision: request.revision,
        action: request.action.clone(),
        error_code: code.into(),
        ..Default::default()
    }
}

pub(crate) async fn write_refinement_projection<W: AsyncWrite + Unpin>(
    writer: &mut W,
    projection: generated::RefinementProjection,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "refinement.candidate".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::Refinement(projection)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

pub(crate) async fn write_refinement_list_projection<W: AsyncWrite + Unpin>(
    writer: &mut W,
    projection: generated::RefinementListProjection,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "refinement.list".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::RefinementList(projection)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

pub(crate) async fn write_refinement_action_result<W: AsyncWrite + Unpin>(
    writer: &mut W,
    result: generated::RefinementActionResult,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "refinement.action".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::RefinementAction(result)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

pub(crate) fn analysis_kernel_projection(
    session: &crate::analysis_kernel::AnalysisKernelSessionV1,
    object_count: usize,
    error_code: &str,
) -> generated::AnalysisKernelProjection {
    generated::AnalysisKernelProjection {
        schema_version: session.schema_version,
        kernel_id: session.id.clone(),
        task_id: session.task_id.clone(),
        workspace_id: session.workspace_id.clone(),
        runtime_version: session.runtime_version.clone(),
        package_manifest_hash: session.package_manifest_hash.clone(),
        policy_hash: session.policy_hash.clone(),
        status: session.status.as_str().into(),
        revision: session.revision,
        limits_json: serde_json::to_vec(&session.limits).unwrap_or_default(),
        object_count: object_count as u32,
        truncated: object_count > 1024,
        error_code: error_code.into(),
    }
}

pub(crate) fn analysis_kernel_object_ref(
    object: &crate::analysis_kernel::KernelObjectRefV1,
) -> generated::AnalysisKernelObjectRef {
    generated::AnalysisKernelObjectRef {
        id: object.id.clone(),
        logical_name: object.logical_name.clone(),
        type_hint: object.type_hint.clone(),
        size: object.size,
        sensitivity: object.sensitivity.as_str().into(),
        persistence: object.persistence.as_str().into(),
        content_hash: object.content_hash.clone().unwrap_or_default(),
        artifact_locator: object.artifact_locator.clone().unwrap_or_default(),
        provenance: object.provenance.clone(),
    }
}

pub(crate) fn analysis_kernel_result_error(request_id: &str, code: &str) -> generated::AnalysisKernelResult {
    generated::AnalysisKernelResult {
        schema_version: crate::analysis_kernel::KERNEL_HOST_REQUEST_VERSION,
        request_id: request_id.into(),
        status: "error".into(),
        error_class: code.into(),
        sensitivity: "internal".into(),
        provenance: "core:analysis-kernel".into(),
        ..Default::default()
    }
}

pub(crate) fn kernel_error_code(error: &crate::analysis_kernel::KernelRuntimeError) -> &'static str {
    match error {
        crate::analysis_kernel::KernelRuntimeError::NotRunning => "kernel_not_running",
        crate::analysis_kernel::KernelRuntimeError::Denied(_) => "host_request_denied",
        crate::analysis_kernel::KernelRuntimeError::LimitExceeded(_) => "limit_exceeded",
        crate::analysis_kernel::KernelRuntimeError::Operation(_) => "operation_failed",
        crate::analysis_kernel::KernelRuntimeError::Contract(error) => match error {
            crate::analysis_kernel::AnalysisKernelError::ForbiddenOperation => {
                "forbidden_operation"
            }
            crate::analysis_kernel::AnalysisKernelError::ForbiddenCapability => {
                "forbidden_capability"
            }
            crate::analysis_kernel::AnalysisKernelError::RequestTooLarge(_) => "request_too_large",
            _ => "invalid_argument",
        },
    }
}

pub(crate) fn kernel_storage_error_code(error: &evohime_local_storage::StorageError) -> &'static str {
    match error {
        evohime_local_storage::StorageError::AnalysisKernel(
            crate::analysis_kernel::AnalysisKernelError::VersionConflict { .. },
        ) => "stale_revision",
        _ => "storage_failed",
    }
}

pub(crate) async fn write_analysis_kernel_projection<W: AsyncWrite + Unpin>(
    writer: &mut W,
    projection: generated::AnalysisKernelProjection,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: projection.task_id.clone(),
        event_type: "analysis_kernel.projection".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::AnalysisKernel(projection)),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

pub(crate) async fn write_analysis_kernel_result<W: AsyncWrite + Unpin>(
    writer: &mut W,
    result: generated::AnalysisKernelResult,
    core_instance_id: &str,
    session_epoch: u64,
) -> Result<(), FrameError> {
    let event = generated::EventEnvelope {
        protocol: Some(protocol()),
        sequence_id: 0,
        task_id: String::new(),
        event_type: "analysis_kernel.result".into(),
        payload: Vec::new(),
        core_instance_id: core_instance_id.into(),
        session_epoch,
        event: Some(generated::event_envelope::Event::AnalysisKernelResult(
            result,
        )),
    };
    transport::write_frame(writer, &event.encode_to_vec()).await
}

pub(crate) fn checkpoint_status_text(status: crate::task_checkpoint::CheckpointStatus) -> String {
    match serde_json::to_string(&status) {
        Ok(value) => value.trim_matches('"').to_owned(),
        Err(error) => {
            tracing::warn!(%error, "failed to serialize checkpoint status");
            "unknown".into()
        }
    }
}

pub(crate) fn conversation_event_log_error(
    operation: &str,
    conversation_id: &str,
    error_code: &str,
) -> generated::ConversationEventLogEvent {
    generated::ConversationEventLogEvent {
        schema_version: crate::conversation_event_log::CONTRACT_VERSION,
        operation: operation.into(),
        conversation_id: conversation_id.chars().take(128).collect(),
        events: Vec::new(),
        oldest_sequence: 0,
        newest_sequence: 0,
        has_older: false,
        has_newer: false,
        earliest_available_sequence: 0,
        error_code: error_code.into(),
    }
}

pub(crate) fn conversation_accept_error_code(error: &StorageError) -> String {
    match error {
        StorageError::ConversationEventLog(
            evohime_local_storage::conversation_event_log_store::ConversationStoreError::InvalidInput,
        )
        | StorageError::InvalidInput(_) => "invalid_argument",
        StorageError::ConversationEventLog(
            evohime_local_storage::conversation_event_log_store::ConversationStoreError::IdempotencyConflict,
        ) => "idempotency_conflict",
        _ => "storage_unavailable",
    }
    .into()
}

pub(crate) fn conversation_event_log_error_with_earliest(
    operation: &str,
    conversation_id: &str,
    error_code: &str,
    earliest_available_sequence: u64,
) -> generated::ConversationEventLogEvent {
    generated::ConversationEventLogEvent {
        earliest_available_sequence,
        ..conversation_event_log_error(operation, conversation_id, error_code)
    }
}

pub(crate) fn checkpoint_disposition_text(disposition: crate::task_checkpoint::RecoveryDisposition) -> String {
    match serde_json::to_string(&disposition) {
        Ok(value) => value.trim_matches('"').to_owned(),
        Err(error) => {
            tracing::warn!(%error, "failed to serialize checkpoint disposition");
            "blocked".into()
        }
    }
}

pub(crate) fn bounded_checkpoint_text(value: &str) -> String {
    value
        .chars()
        .take(TASK_CHECKPOINT_IPC_MAX_TEXT_BYTES)
        .collect()
}

pub(crate) fn bounded_checkpoint_event_type(value: &str) -> String {
    if value.is_empty() || value.len() > 128 || value.bytes().any(|byte| byte.is_ascii_control()) {
        "unknown".into()
    } else {
        value.to_owned()
    }
}

pub(crate) fn checkpoint_error_code(error: &StorageError) -> &'static str {
    match error {
        StorageError::TaskCheckpoint(error) => error.code(),
        _ => "storage_failed",
    }
}

pub(crate) fn task_checkpoint_projection_error(
    task_id: &str,
    error_code: &str,
) -> generated::TaskCheckpointProjection {
    generated::TaskCheckpointProjection {
        schema_version: crate::task_checkpoint::TASK_CHECKPOINT_VERSION,
        task_id: if valid_checkpoint_token(task_id, 128) {
            task_id.to_owned()
        } else {
            String::new()
        },
        recovery_disposition: "blocked".into(),
        recovery_warning: "Проекция checkpoint недоступна; автоматическое продолжение запрещено."
            .into(),
        error_code: error_code.into(),
        ..Default::default()
    }
}

pub(crate) fn task_checkpoint_projection(
    task_id: &str,
    recovery: crate::task_checkpoint::TaskCheckpointRecovery,
    max_replay_events: usize,
) -> generated::TaskCheckpointProjection {
    let Some(checkpoint) = recovery.checkpoint else {
        return generated::TaskCheckpointProjection {
            schema_version: crate::task_checkpoint::TASK_CHECKPOINT_VERSION,
            task_id: task_id.to_owned(),
            recovery_disposition: "no_checkpoint".into(),
            recovery_warning: "Для задачи ещё нет сохранённого checkpoint.".into(),
            ..Default::default()
        };
    };
    let blockers = checkpoint
        .blockers
        .iter()
        .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
        .map(|item| bounded_checkpoint_text(&item.text))
        .collect();
    let mut refs = Vec::new();
    for reference in checkpoint
        .workflow_refs
        .iter()
        .chain(checkpoint.child_refs.iter())
        .chain(checkpoint.artifact_refs.iter())
        .take(TASK_CHECKPOINT_IPC_MAX_ITEMS)
    {
        refs.push(generated::TaskCheckpointRef {
            kind: bounded_checkpoint_text(&reference.kind),
            id: bounded_checkpoint_text(&reference.id),
            content_hash: reference.content_hash.clone().unwrap_or_default(),
            sensitivity: match serde_json::to_string(&reference.sensitivity) {
                Ok(value) => value.trim_matches('"').to_owned(),
                Err(error) => {
                    tracing::warn!(%error, "failed to serialize checkpoint sensitivity");
                    "internal".into()
                }
            },
        });
    }
    let policy_id = checkpoint
        .workflow_refs
        .iter()
        .find(|reference| reference.kind == "policy_snapshot")
        .map(|reference| bounded_checkpoint_text(&reference.id))
        .unwrap_or_default();
    let replayed_event_types = recovery
        .replayed_events
        .iter()
        .take(max_replay_events)
        .map(|event| bounded_checkpoint_event_type(&event.event_type))
        .collect();
    generated::TaskCheckpointProjection {
        schema_version: checkpoint.version,
        checkpoint_id: bounded_checkpoint_text(&checkpoint.id),
        task_id: task_id.to_owned(),
        workspace_id: bounded_checkpoint_text(&checkpoint.workspace_id),
        parent_checkpoint_id: checkpoint.parent_checkpoint_id.unwrap_or_default(),
        status: checkpoint_status_text(checkpoint.status),
        source_event_seq: checkpoint.source_event_seq,
        created_at: checkpoint.created_at,
        completed_count: checkpoint.completed_items.len().min(u32::MAX as usize) as u32,
        remaining_count: checkpoint.remaining_items.len().min(u32::MAX as usize) as u32,
        blocker_count: checkpoint.blockers.len().min(u32::MAX as usize) as u32,
        blockers,
        refs,
        recovery_disposition: checkpoint_disposition_text(recovery.disposition),
        recovery_warning: recovery
            .warning
            .as_deref()
            .map(bounded_checkpoint_text)
            .unwrap_or_default(),
        replayed_event_types,
        can_request_resume: recovery.disposition
            == crate::task_checkpoint::RecoveryDisposition::Replayable,
        replayed_event_count: recovery.replayed_events.len().min(u32::MAX as usize) as u32,
        policy_id,
        error_code: String::new(),
    }
}

pub(crate) fn task_checkpoint_action_result(
    task_id: String,
    checkpoint_id: String,
    action: String,
    applied: bool,
    deduplicated: bool,
    error_code: &str,
    error_message: &str,
) -> generated::TaskCheckpointActionResult {
    generated::TaskCheckpointActionResult {
        task_id: if valid_checkpoint_token(&task_id, 128) {
            task_id
        } else {
            String::new()
        },
        checkpoint_id: if valid_checkpoint_token(&checkpoint_id, 128) {
            checkpoint_id
        } else {
            String::new()
        },
        action: matches!(action.as_str(), "acknowledge_recovery" | "request_resume")
            .then_some(action)
            .unwrap_or_default(),
        applied,
        deduplicated,
        error_code: error_code.into(),
        error_message: bounded_checkpoint_text(error_message),
        sequence_id: 0,
    }
}

pub(crate) fn task_checkpoint_action_result_from_record(
    record: TaskCheckpointActionRecord,
) -> generated::TaskCheckpointActionResult {
    task_checkpoint_action_result(
        record.task_id,
        record.checkpoint_id,
        record.action,
        record.applied,
        record.deduplicated,
        &record.error_code,
        &record.error_message,
    )
}

/// Результат `SetAmbientListening` в одном месте: неизвестный код не
/// притворяется успехом, а пустая строка означает «ошибки не было».
pub(crate) fn listening_result(
    state: evohime_listener_contract::ListeningState,
    code: Option<evohime_listener_contract::AmbientErrorCode>,
) -> serde_json::Value {
    serde_json::json!({
        "state": state,
        "error_code": code.map(|code| code.as_str()).unwrap_or(""),
    })
}

/// Разбирает отметку времени ambient-строки в миллисекунды эпохи.
///
/// Формат задаёт `crate::ambient::timestamp_ms`; неразбираемое значение
/// становится нулём, а не «сейчас»: выдуманное время выглядело бы как
/// свежий эпизод.
pub(crate) fn parse_timestamp_ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp_millis())
        .unwrap_or(0)
}

/// Builds a stage 01.4 `ReceiptFilter` from bounded IPC request fields. Empty
/// strings mean "no filter" for that field; a non-empty value must be a valid
/// 01.1 typed identifier or RFC3339 timestamp, or the whole request is
/// rejected rather than silently ignored.
pub(crate) fn receipt_filter_from_request(
    task_id: &str,
    run_id: &str,
    action_id: &str,
    from_rfc3339: &str,
    to_rfc3339: &str,
) -> Result<evohime_receipts::export::ReceiptFilter, &'static str> {
    let parse_ms = |value: &str| -> Result<Option<i64>, &'static str> {
        if value.is_empty() {
            return Ok(None);
        }
        chrono::DateTime::parse_from_rfc3339(value)
            .map(|parsed| Some(parsed.timestamp_millis()))
            .map_err(|_| "receipts.invalid_filter")
    };
    Ok(evohime_receipts::export::ReceiptFilter {
        task_id: (!task_id.is_empty()).then(|| task_id.to_string()),
        run_id: (!run_id.is_empty()).then(|| run_id.to_string()),
        action_id: (!action_id.is_empty()).then(|| action_id.to_string()),
        from_ms: parse_ms(from_rfc3339)?,
        to_ms: parse_ms(to_rfc3339)?,
    })
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Publishes a review event on the path a connected shell actually reads.
///
/// The pipe server flushes its journal tail only on the coordinator's
/// `journalled` signal, and that signal is raised by the coordinator's own
/// journal writer. An event recorded straight into the journal is durable but
/// stays invisible until some later event wakes the pump, which left a running
/// review looking frozen in the UI. Recording directly is the fallback for a
/// bridge built without a coordinator.
pub(crate) async fn publish_review_event(
    coordinator: &Option<TaskCoordinator>,
    journal: &EventJournal,
    event: CoreEvent,
) {
    match coordinator {
        Some(coordinator) => coordinator.emit(event).await,
        None => {
            let _ = journal.record(&event).await;
        }
    }
}

/// Drops the "which models reviewed which files" preamble that
/// `format_review_markdown` prepends. It is provenance for the reader, and
/// feeding it to the editing model invites those model names into the plan.
pub(crate) fn strip_review_header(final_markdown: &str) -> String {
    match final_markdown.split_once("\n---\n\n") {
        Some((header, body)) if header.starts_with("<!-- Контекст EvoHime") => {
            body.to_string()
        }
        _ => final_markdown.to_string(),
    }
}

pub(crate) fn revision_result_from_event(payload: &[u8]) -> Option<crate::plan_review::RevisionResult> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let message = value
        .get("TaskCompleted")
        .and_then(|item| item.get("final_message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("final_message")
                .and_then(serde_json::Value::as_str)
        })?;
    serde_json::from_str(message).ok()
}

pub(crate) fn review_result_from_event(payload: &[u8]) -> Option<crate::plan_review::ReviewResult> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let message = value
        .get("TaskCompleted")
        .and_then(|item| item.get("final_message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("final_message")
                .and_then(serde_json::Value::as_str)
        })?;
    serde_json::from_str(message).ok()
}

pub(crate) fn protocol() -> generated::ProtocolVersion {
    generated::ProtocolVersion {
        major: PROTOCOL_MAJOR,
        minor: PROTOCOL_MINOR,
    }
}

pub(crate) fn core_info() -> generated::CoreInfo {
    generated::CoreInfo {
        protocol: Some(protocol()),
        core_version: env!("CARGO_PKG_VERSION").into(),
        build_revision: option_env!("EVOHIME_BUILD_REVISION")
            .unwrap_or("unknown")
            .into(),
        runtime_revision: "rust-core".into(),
        capabilities: vec![
            "replay".into(),
            "resync".into(),
            "task_checkpoint".into(),
            "skills".into(),
            "goals".into(),
            "workflow_builder".into(),
            "persistent_agent_organization_registry".into(),
        ],
        feature_flags: vec!["authenticated-ipc".into()],
        max_frame_bytes: evohime_desktop_ipc::MAX_FRAME_BYTES as u32,
        max_replay_events: evohime_desktop_ipc::MAX_REPLAY_EVENTS as u32,
        max_snapshot_bytes: evohime_desktop_ipc::MAX_RESYNC_SNAPSHOT_BYTES as u32,
    }
}