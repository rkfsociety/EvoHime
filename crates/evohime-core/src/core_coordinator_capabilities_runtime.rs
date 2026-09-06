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
                let typed_accepted = journal
                    .accept_typed_child_report(
                        &typed_request,
                        &typed_report,
                        task_memory::now_millis() as i64,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                // The real bounded contract runs here: re-validates the
                // request, validates the report's own bounds, rejects
                // secret-like content and duplicate sources, and
                // rejects a child_task_id mismatch -- the same
                // `accept_report` used by the pure unit tests, now
                // enforced on the live IPC path.
                let accepted = crate::child_runtime::accept_report(&request, &report)
                    .map_err(|error| error.to_string())?;
                let report_json =
                    serde_json::to_string(&accepted).map_err(|error| error.to_string())?;
                let record = evohime_local_storage::domains::agents::ChildReportRecord {
                    child_task_id: accepted.child_task_id.clone(),
                    parent_task_id: stored_request.parent_task_id.clone(),
                    status: child_report_status_str(accepted.status).to_string(),
                    confidence_percent: accepted.confidence_percent,
                    report_json,
                };
                journal.save_child_report(&record).await?;
                let now_ms = task_memory::now_millis() as i64;
                journal
                    .save_coordinator_checkpoint(
                        &evohime_local_storage::domains::agents::CoordinatorCheckpointRecord {
                            schema_version: 1,
                            child_task_id: accepted.child_task_id.clone(),
                            parent_task_id: stored_request.parent_task_id.clone(),
                            revision: typed_accepted.revision.unwrap_or(0) as i64,
                            state: "accepted".into(),
                            failure_reason: None,
                            dead_letter: false,
                            report_json: Some(
                                serde_json::to_string(&typed_accepted)
                                    .map_err(|error| error.to_string())?,
                            ),
                            evidence_locators_json: None,
                            provenance_hashes_json: Some(
                                serde_json::to_string(&typed_accepted.provenance)
                                    .map_err(|error| error.to_string())?,
                            ),
                            parent_sequence: parent_sequence as i64,
                            lease_deadline_monotonic_ms: None,
                            lease_created_monotonic_ms: None,
                            lease_clock_boot_id: None,
                            lease_holder_process_id: None,
                            last_transition_event: "child.report.accepted".into(),
                            last_transition_at_ms: now_ms,
                            created_at_ms: now_ms,
                        },
                    )
                    .await?;
                let _ = state
                    .lock()
                    .await
                    .events
                    .send(CoreEvent::ChildWorkflowProjection {
                        task_id: stored_request.parent_task_id.clone(),
                        projection: crate::child_workflow::ChildProjection {
                            event_id: format!("{}:accepted", accepted.child_task_id),
                            parent_task_id: stored_request.parent_task_id.clone(),
                            child_task_id: accepted.child_task_id.clone(),
                            role: request.role.clone(),
                            revision: typed_accepted.revision.unwrap_or(0),
                            state: crate::child_workflow::CoordinatorState::Accepted,
                            reason_code: None,
                            parent_sequence,
                            budget: typed_request.budget.clone(),
                            lease_live: false,
                            dead_letter: false,
                        },
                    });
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
