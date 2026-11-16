use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::SubmitChildReport {
            child_task_id,
            status,
            summary,
            findings,
            sources,
            confidence_percent,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let parsed_status = child_report_status_from_str(&status)?;
                let confidence_percent: u8 = u8::try_from(confidence_percent)
                    .map_err(|_| "confidence_percent must be between 0 and 255".to_string())?;
                let report = crate::child_runtime::ChildReport {
                    child_task_id: child_task_id.clone(),
                    status: parsed_status,
                    summary,
                    findings,
                    sources,
                    confidence_percent,
                };
                let stored_request = journal
                    .get_child_task_request(&child_task_id)
                    .await?
                    .ok_or_else(|| {
                        "no matching child task request found for child_task_id".to_string()
                    })?;
                let parent_sequence = journal
                    .get_coordinator_checkpoint(&child_task_id)
                    .await?
                    .map(|checkpoint| checkpoint.parent_sequence as u64)
                    .unwrap_or(0);
                let request: crate::child_runtime::ChildTaskRequest =
                    serde_json::from_str(&stored_request.request_json)
                        .map_err(|error| error.to_string())?;
                let typed_request = crate::child_contracts::TypedChildTaskRequest::new(
                    request.child_task_id.clone(),
                    request.parent_task_id.clone(),
                    request.role.clone(),
                    "legacy child workflow",
                    crate::child_contracts::CorrelationContext::new(
                        crate::child_contracts::CorrelationId::new(request.parent_task_id.clone())
                            .map_err(|error| error.to_string())?,
                        crate::child_contracts::CorrelationId::new(request.child_task_id.clone())
                            .map_err(|error| error.to_string())?,
                        parent_sequence,
                    ),
                )
                .map_err(|error| error.to_string())?
                .with_context(request.reduced_context.clone())
                .map_err(|error| error.to_string())?
                .with_max_output_bytes(request.max_output_bytes)
                .map_err(|error| error.to_string())?
                .with_capabilities(request.requested_capabilities.clone())
                .map_err(|error| error.to_string())?;
                let typed_status = match report.status {
                    crate::child_runtime::ChildReportStatus::Complete => {
                        crate::child_contracts::TypedReportStatus::Complete
                    }
                    crate::child_runtime::ChildReportStatus::Partial => {
                        crate::child_contracts::TypedReportStatus::Partial
                    }
                    crate::child_runtime::ChildReportStatus::Rejected => {
                        crate::child_contracts::TypedReportStatus::Rejected
                    }
                };
                let typed_report = crate::child_contracts::TypedChildReport::new(
                    report.child_task_id.clone(),
                    request.parent_task_id.clone(),
                    typed_request.correlation.clone(),
                    crate::child_contracts::Provenance::new(parent_sequence).mark_completed(),
                )
                .map_err(|error| error.to_string())?
                .with_status(typed_status)
                .with_summary(report.summary.clone())
                .map_err(|error| error.to_string())?
                .with_findings(report.findings.clone())
                .map_err(|error| error.to_string())?
                .with_sources(report.sources.clone())
                .map_err(|error| error.to_string())?
                .with_confidence(report.confidence_percent);
                // The real bounded contract runs here: re-validates the
                // request, validates the report's own bounds, rejects
                // secret-like content and duplicate sources, and
                // rejects a child_task_id mismatch -- the same
                // `accept_report` used by the pure unit tests, now
                // enforced on the live IPC path.
                let accepted = crate::child_runtime::accept_report(&request, &report)
                    .map_err(|error| error.to_string())?;
                // Acceptance, storage, checkpoint, projection, and the bounded
                // collaboration message are owned by one shared writer, so a
                // report accepted over legacy IPC is indistinguishable from an
                // executor-accepted report.
                let _typed_accepted = crate::child_workflow::record_accepted_child_report(
                    &journal,
                    crate::child_workflow::ChildProjectionSink::CoordinatorState(&state),
                    &typed_request,
                    &typed_report,
                    None,
                    task_memory::now_millis() as i64,
                )
                .await
                .map_err(|error| error.message().to_owned())?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    stored_request.parent_task_id.clone(),
                    "child.report.accepted",
                    [
                        ("child_task_id".to_owned(), accepted.child_task_id.clone()),
                        (
                            "parent_task_id".to_owned(),
                            stored_request.parent_task_id.clone(),
                        ),
                        (
                            "confidence_percent".to_owned(),
                            accepted.confidence_percent.to_string(),
                        ),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "report": accepted }))
                    .map_err(|error| error.to_string())
            }
            .await;
            if let Err(error) = &result {
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    child_task_id.clone(),
                    "child.contract.rejected",
                    [("reason".to_owned(), error.clone())],
                )
                .await;
            }
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child_contracts::{
        CorrelationContext, CorrelationId, Provenance, TypedChildReport, TypedChildTaskRequest,
        TypedReportStatus,
    };
    use crate::child_workflow::{ChildProjection, CoordinatorState};
    use std::time::{Duration, Instant};

    async fn dispatch_request(
        coordinator: &TaskCoordinator,
        parent_is_child: bool,
    ) -> Result<Vec<u8>, String> {
        let (reply, response) = tokio::sync::oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildRequest {
                child_task_id: "child-1".into(),
                parent_task_id: "task-1".into(),
                role: "researcher".into(),
                kind: "code_search".into(),
                reduced_context: vec!["inspect src".into()],
                max_output_bytes: 4096,
                requested_capabilities: vec!["workspace.read".into()],
                parent_is_child,
                reply,
            })
            .await
            .expect("dispatch child request");
        response.await.expect("request reply channel")
    }

    async fn dispatch_report(coordinator: &TaskCoordinator) -> Result<Vec<u8>, String> {
        let (reply, response) = tokio::sync::oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildReport {
                child_task_id: "child-1".into(),
                status: "complete".into(),
                summary: "found one relevant module".into(),
                findings: vec!["module is bounded".into()],
                sources: vec!["src/lib.rs:10".into()],
                confidence_percent: 90,
                reply,
            })
            .await
            .expect("dispatch child report");
        response.await.expect("report reply channel")
    }

    type CheckpointRecord = evohime_local_storage::domains::agents::CoordinatorCheckpointRecord;

    fn strip_clock_fields(mut record: CheckpointRecord) -> CheckpointRecord {
        record.lease_deadline_monotonic_ms = None;
        record.lease_created_monotonic_ms = None;
        record.last_transition_at_ms = 0;
        record.created_at_ms = 0;
        record
    }

    fn strip_completed_at(value: Option<String>) -> Option<String> {
        fn strip(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("completed_at");
                    map.remove("created_at");
                    for value in map.values_mut() {
                        strip(value);
                    }
                }
                serde_json::Value::Array(items) => {
                    for value in items {
                        strip(value);
                    }
                }
                _ => {}
            }
        }
        value.map(|text| {
            let mut parsed: serde_json::Value =
                serde_json::from_str(&text).expect("stored json projection");
            strip(&mut parsed);
            parsed.to_string()
        })
    }

    fn normalize_record(mut record: CheckpointRecord) -> CheckpointRecord {
        record.report_json = strip_completed_at(record.report_json);
        record.provenance_hashes_json = strip_completed_at(record.provenance_hashes_json);
        strip_clock_fields(record)
    }

    async fn wait_for_accepted_projection(journal: &EventJournal) -> ChildProjection {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let records = journal.replay(0, 256).await.expect("journal replay");
            if let Some(projection) = records.iter().rev().find_map(|record| {
                if record.event_type != "child.workflow" {
                    return None;
                }
                let projection: ChildProjection = serde_json::from_slice(&record.payload).ok()?;
                (projection.state == CoordinatorState::Accepted
                    && projection.report_summary.is_some())
                .then_some(projection)
            }) {
                return projection;
            }
            assert!(
                Instant::now() < deadline,
                "the accepted projection was not journalled"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn legacy_ipc_and_executor_write_the_same_accepted_projection_and_checkpoint() {
        let legacy_dir = tempfile::tempdir().expect("legacy temp dir");
        let legacy_journal =
            EventJournal::open(legacy_dir.path().join("legacy.db")).expect("legacy journal");
        let (legacy_coordinator, _events) =
            TaskCoordinator::new_with_journal(8, None, legacy_journal.clone());

        dispatch_request(&legacy_coordinator, false)
            .await
            .expect("legacy request accepted");
        dispatch_report(&legacy_coordinator)
            .await
            .expect("legacy report accepted");

        let legacy_checkpoint = legacy_journal
            .get_coordinator_checkpoint("child-1")
            .await
            .expect("legacy checkpoint query")
            .expect("legacy checkpoint");
        let legacy_projection = wait_for_accepted_projection(&legacy_journal).await;

        let executor_dir = tempfile::tempdir().expect("executor temp dir");
        let executor_journal =
            EventJournal::open(executor_dir.path().join("executor.db")).expect("executor journal");
        let executor = crate::child_executor::RegisteredChildExecutor::new(
            executor_journal.clone(),
            executor_dir.path(),
        );
        let request = TypedChildTaskRequest::new(
            "child-1",
            "task-1",
            "researcher",
            "legacy child workflow",
            CorrelationContext::new(
                CorrelationId::new("task-1").expect("parent id"),
                CorrelationId::new("child-1").expect("child id"),
                1,
            ),
        )
        .expect("typed request")
        .with_context(vec!["inspect src".into()])
        .expect("context")
        .with_max_output_bytes(4096)
        .expect("output bound")
        .with_capabilities(vec!["workspace.read".into()])
        .expect("capabilities");
        let report = TypedChildReport::new(
            "child-1",
            "task-1",
            request.correlation.clone(),
            Provenance::new(1).mark_completed(),
        )
        .expect("typed report")
        .with_status(TypedReportStatus::Complete)
        .with_summary("found one relevant module")
        .expect("summary")
        .with_findings(vec!["module is bounded".into()])
        .expect("findings")
        .with_sources(vec!["src/lib.rs:10".into()])
        .expect("sources")
        .with_confidence(90);
        executor
            .record_accepted_report(&request, &report, None)
            .await
            .expect("executor accepted report");

        let executor_checkpoint = executor_journal
            .get_coordinator_checkpoint("child-1")
            .await
            .expect("executor checkpoint query")
            .expect("executor checkpoint");
        let executor_projection = wait_for_accepted_projection(&executor_journal).await;

        assert_eq!(
            normalize_record(legacy_checkpoint),
            normalize_record(executor_checkpoint)
        );
        assert_eq!(legacy_projection, executor_projection);
        assert_eq!(executor_projection.event_id, "child-1:accepted");
        assert_eq!(
            executor_projection.report_summary.as_deref(),
            Some("found one relevant module")
        );
        assert!(
            executor_projection
                .report_summary
                .as_deref()
                .map_or(0, |summary| summary.chars().count())
                <= 512
        );
    }

    #[tokio::test]
    async fn legacy_command_handler_cannot_inherit_or_forge_the_child_marker() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = EventJournal::open(directory.path().join("legacy.db")).expect("journal");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal);

        let context =
            crate::child_execution_context::ChildExecutionContext::new("child-1", "task-1");
        let result = crate::child_execution_context::scope(context, async {
            dispatch_request(&coordinator, true).await
        })
        .await;
        let error = result.expect_err("a forged parent_is_child claim must be rejected");
        assert!(
            error.contains("child execution context"),
            "unexpected rejection: {error}"
        );
    }
}
