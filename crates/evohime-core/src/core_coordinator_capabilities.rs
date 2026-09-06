use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::InstallCapability {
            manifest_json,
            install_source,
            source_path,
            expected_content_hash,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                if install_source != "local_archive" && install_source != "https_archive" {
                    return Err(format!(
                        "unsupported capability install source: {install_source}"
                    ));
                }
                let candidate: crate::capability_registry::CapabilityManifest =
                    serde_json::from_str(&manifest_json).map_err(|error| error.to_string())?;
                candidate.validate().map_err(|error| error.to_string())?;
                let expected_manifest_source = if install_source == "https_archive" {
                    crate::capability_registry::InstallSource::HttpsArchive
                } else {
                    crate::capability_registry::InstallSource::LocalArchive
                };
                if candidate.install.source != expected_manifest_source {
                    return Err(
                        "manifest install source does not match the requested installer"
                            .to_string(),
                    );
                }
                if install_source == "https_archive" {
                    verify_https_capability_archive(&source_path, &expected_content_hash).await?;
                }
                let existing_records = journal
                    .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                    .await?;
                let mut existing_manifests = Vec::with_capacity(existing_records.len());
                for record in &existing_records {
                    let manifest: crate::capability_registry::CapabilityManifest =
                        serde_json::from_str(&record.manifest_json)
                            .map_err(|error| error.to_string())?;
                    existing_manifests.push(manifest);
                }
                if let Some(current) = existing_manifests
                    .iter()
                    .find(|manifest| manifest.name == candidate.name)
                {
                    crate::capability_registry::validate_update(current, &candidate)
                        .map_err(|error| error.to_string())?;
                } else {
                    let mut proposed = existing_manifests.clone();
                    proposed.push(candidate.clone());
                    crate::capability_registry::validate_registry(&proposed)
                        .map_err(|error| error.to_string())?;
                }
                let store_record =
                    evohime_local_storage::capability_store::CapabilityManifestRecord {
                        id: candidate.name.clone(),
                        kind: capability_manifest_kind(&candidate),
                        version: candidate.version.clone(),
                        risk_class: capability_risk_class_str(candidate.risk_class).to_string(),
                        content_hash: candidate.content_hash.clone(),
                        manifest_json: serde_json::to_string(&candidate)
                            .map_err(|error| error.to_string())?,
                    };
                journal.save_capability_manifest(&store_record).await?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Approval,
                    candidate.name.clone(),
                    "capability.installed",
                    [
                        ("manifest_id".to_owned(), candidate.name.clone()),
                        ("version".to_owned(), candidate.version.clone()),
                        ("install_source".to_owned(), install_source),
                        ("source_path".to_owned(), source_path),
                        (
                            "expected_content_hash".to_owned(),
                            if expected_content_hash.is_empty() {
                                "not_provided".to_owned()
                            } else {
                                expected_content_hash
                            },
                        ),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "manifest": candidate }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListCapabilities { limit, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let records = journal.list_capability_manifests(limit).await?;
                let manifests = records
                    .iter()
                    .map(|record| {
                        serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                            &record.manifest_json,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_vec(&serde_json::json!({ "manifests": manifests }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::MatchCapabilities {
            intent,
            required_tools,
            required_domains,
            requested_risk,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let requested_risk = parse_capability_risk_class(&requested_risk)?;
                let records = journal
                    .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                    .await?;
                let manifests = records
                    .iter()
                    .map(|record| {
                        serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                            &record.manifest_json,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let query = crate::capability_registry::MatchQuery {
                    intent,
                    required_tools,
                    required_domains,
                    requested_risk,
                };
                let matches = crate::capability_registry::match_capabilities(&manifests, &query)
                    .map_err(|error| error.to_string())?;
                serde_json::to_vec(&serde_json::json!({ "matches": matches }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::RemoveCapability { id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let removed = journal.remove_capability_manifest(&id).await?;
                if !removed {
                    return Err("capability manifest was not found".to_string());
                }
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Approval,
                    id.clone(),
                    "capability.removed",
                    [("manifest_id".to_owned(), id.clone())],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "id": id, "removed": true }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::GetCapabilitySelection {
            task_id,
            intent,
            required_tools,
            required_domains,
            requested_risk,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let requested_risk = parse_capability_risk_class(&requested_risk)?;
                let records = journal
                    .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                    .await?;
                let manifests = records
                    .iter()
                    .map(|record| {
                        serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                            &record.manifest_json,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let query = crate::capability_registry::MatchQuery {
                    intent,
                    required_tools,
                    required_domains,
                    requested_risk,
                };
                let stored = journal.get_capability_selection(&task_id).await?;
                let current_state = stored
                    .map(|record| {
                        serde_json::from_str::<
                                crate::capability_selection::CapabilitySelectionState,
                            >(&record.state_json)
                            .map_err(|error| error.to_string())
                    })
                    .transpose()?;
                let auto_match = crate::capability_selection::select_for_task(&manifests, &query);
                let reconciled = crate::capability_selection::reconcile_with_pin(
                    current_state.as_ref(),
                    auto_match,
                )
                .map_err(|error| error.to_string())?;
                let state_json =
                    serde_json::to_string(&reconciled).map_err(|error| error.to_string())?;
                let selection_record =
                    evohime_local_storage::capability_selection_store::CapabilitySelectionRecord {
                        task_id: task_id.clone(),
                        origin: capability_selection_origin_to_store(reconciled.origin),
                        manifest_name: reconciled.selection.manifest_name.clone(),
                        state_json,
                    };
                journal.save_capability_selection(&selection_record).await?;
                serde_json::to_vec(&reconciled).map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::PinCapabilitySelection { task_id, reply } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let stored = journal
                    .get_capability_selection(&task_id)
                    .await?
                    .ok_or_else(|| {
                        "no capability selection recorded for this task yet".to_string()
                    })?;
                let current_state = serde_json::from_str::<
                    crate::capability_selection::CapabilitySelectionState,
                >(&stored.state_json)
                .map_err(|error| error.to_string())?;
                let pinned = crate::capability_selection::pin(current_state);
                let state_json =
                    serde_json::to_string(&pinned).map_err(|error| error.to_string())?;
                let selection_record =
                    evohime_local_storage::capability_selection_store::CapabilitySelectionRecord {
                        task_id: task_id.clone(),
                        origin: capability_selection_origin_to_store(pinned.origin),
                        manifest_name: pinned.selection.manifest_name.clone(),
                        state_json,
                    };
                journal.save_capability_selection(&selection_record).await?;
                serde_json::to_vec(&pinned).map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ReplaceCapabilitySelection {
            task_id,
            manifest_name,
            intent,
            required_tools,
            required_domains,
            requested_risk,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let requested_risk = parse_capability_risk_class(&requested_risk)?;
                let records = journal
                    .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                    .await?;
                let manifests = records
                    .iter()
                    .map(|record| {
                        serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                            &record.manifest_json,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let query = crate::capability_registry::MatchQuery {
                    intent,
                    required_tools,
                    required_domains,
                    requested_risk,
                };
                let replaced =
                    crate::capability_selection::replace(&manifests, &query, &manifest_name)
                        .map_err(|error| error.to_string())?;
                let state_json =
                    serde_json::to_string(&replaced).map_err(|error| error.to_string())?;
                let selection_record =
                    evohime_local_storage::capability_selection_store::CapabilitySelectionRecord {
                        task_id: task_id.clone(),
                        origin: capability_selection_origin_to_store(replaced.origin),
                        manifest_name: replaced.selection.manifest_name.clone(),
                        state_json,
                    };
                journal.save_capability_selection(&selection_record).await?;
                serde_json::to_vec(&replaced).map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::RequestChildHandoff {
            handoff_id,
            task_id,
            kind,
            from_role,
            from_name,
            to_role,
            to_name,
            purpose,
            payload,
            sequence,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let parsed_kind = handoff_kind_from_str(&kind)?;
                let from = role_identity_from_parts(&from_role, &from_name)?;
                let to = role_identity_from_parts(&to_role, &to_name)?;
                let handoff_payload = crate::child_roles::HandoffPayload::new(payload)
                    .map_err(|error| error.to_string())?;
                let envelope = crate::child_roles::HandoffEnvelope::new(
                    crate::child_roles::HandoffEnvelopeInput {
                        handoff_id: handoff_id.clone(),
                        task_id: task_id.clone(),
                        kind: parsed_kind,
                        from: from.clone(),
                        to: to.clone(),
                        purpose,
                        payload: handoff_payload,
                        sequence,
                    },
                )
                .map_err(|error| error.to_string())?;
                let record = evohime_local_storage::child_store::HandoffRecord {
                    handoff_id: envelope.handoff_id.clone(),
                    task_id: envelope.task_id.clone(),
                    kind: handoff_kind_str(envelope.kind).to_string(),
                    status: handoff_status_str(envelope.status).to_string(),
                    from_role: role_identity_display(&from),
                    to_role: role_identity_display(&to),
                    sequence: envelope.sequence,
                    envelope_json: envelope.to_deterministic_json(),
                };
                journal.save_child_handoff(&record).await?;
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    task_id.clone(),
                    "child.handoff.requested",
                    [
                        ("handoff_id".to_owned(), envelope.handoff_id.clone()),
                        ("task_id".to_owned(), task_id),
                        ("from_role".to_owned(), record.from_role.clone()),
                        ("to_role".to_owned(), record.to_role.clone()),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "handoff": envelope }))
                    .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::ListChildHandoffs {
            task_id,
            limit,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let records = journal.list_child_handoffs(&task_id, limit).await?;
                let handoffs = records
                    .iter()
                    .map(|record| {
                        serde_json::from_str::<crate::child_roles::HandoffEnvelope>(
                            &record.envelope_json,
                        )
                        .map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_vec(&serde_json::json!({
                    "task_id": task_id,
                    "handoffs": handoffs,
                }))
                .map_err(|error| error.to_string())
            }
            .await;
            let _ = reply.send(result);
        }
        CoreCommand::SubmitChildRequest {
            child_task_id,
            parent_task_id,
            role,
            kind,
            reduced_context,
            max_output_bytes,
            requested_capabilities,
            parent_is_child,
            reply,
        } => {
            let journal = state.lock().await.journal.clone();
            let result = async {
                let journal =
                    journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                let parsed_kind = child_task_kind_from_str(&kind)?;
                let request = crate::child_runtime::ChildTaskRequest {
                    child_task_id: child_task_id.clone(),
                    parent_task_id: parent_task_id.clone(),
                    role: role.clone(),
                    kind: parsed_kind,
                    reduced_context,
                    max_output_bytes: max_output_bytes as usize,
                    requested_capabilities,
                    parent_is_child,
                };
                // The real bounded contract runs here: rejects nested
                // children, any non-read-only requested capability, and
                // oversized context/output. This is the same
                // `ChildTaskRequest::validate` used by the pure unit
                // tests, now enforced on the live IPC path.
                request.validate().map_err(|error| error.to_string())?;
                let parent_sequence = journal.next_child_parent_sequence(&parent_task_id).await?;
                let typed_correlation = crate::child_contracts::CorrelationContext::new(
                    crate::child_contracts::CorrelationId::new(parent_task_id.clone())
                        .map_err(|error| error.to_string())?,
                    crate::child_contracts::CorrelationId::new(child_task_id.clone())
                        .map_err(|error| error.to_string())?,
                    parent_sequence,
                );
                let typed_request = crate::child_contracts::TypedChildTaskRequest::new(
                    child_task_id.clone(),
                    parent_task_id.clone(),
                    role.clone(),
                    format!("{kind} child workflow"),
                    typed_correlation,
                )
                .map_err(|error| error.to_string())?
                .with_context(request.reduced_context.clone())
                .map_err(|error| error.to_string())?
                .with_max_output_bytes(request.max_output_bytes)
                .map_err(|error| error.to_string())?
                .with_capabilities(request.requested_capabilities.clone())
                .map_err(|error| error.to_string())?;
                crate::child_contracts::validate_contract_version(
                    typed_request.contract_version,
                    crate::child_contracts::CONTRACT_VERSION,
                )
                .map_err(|error| error.to_string())?;
                typed_request
                    .validate()
                    .map_err(|error| error.to_string())?;
                let request_json =
                    serde_json::to_string(&request).map_err(|error| error.to_string())?;
                let record = evohime_local_storage::child_store::ChildTaskRequestRecord {
                    child_task_id: request.child_task_id.clone(),
                    parent_task_id: request.parent_task_id.clone(),
                    role: request.role.clone(),
                    kind: child_task_kind_str(request.kind).to_string(),
                    request_json,
                };
                journal.save_child_task_request(&record).await?;
                let now_ms = task_memory::now_millis() as i64;
                journal
                    .save_coordinator_checkpoint(
                        &evohime_local_storage::child_store::CoordinatorCheckpointRecord {
                            schema_version: 1,
                            child_task_id: request.child_task_id.clone(),
                            parent_task_id: request.parent_task_id.clone(),
                            revision: 0,
                            state: "created".into(),
                            failure_reason: None,
                            dead_letter: false,
                            report_json: None,
                            evidence_locators_json: None,
                            provenance_hashes_json: None,
                            parent_sequence: parent_sequence as i64,
                            lease_deadline_monotonic_ms: Some(
                                now_ms + crate::child_workflow::DEFAULT_LEASE_MS as i64,
                            ),
                            lease_created_monotonic_ms: Some(now_ms),
                            lease_clock_boot_id: Some("current".into()),
                            lease_holder_process_id: Some(std::process::id().to_string()),
                            last_transition_event: "child.request.submitted".into(),
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
                        task_id: request.parent_task_id.clone(),
                        projection: crate::child_workflow::ChildProjection {
                            event_id: format!("{}:created", request.child_task_id),
                            parent_task_id: request.parent_task_id.clone(),
                            child_task_id: request.child_task_id.clone(),
                            role: request.role.clone(),
                            revision: 0,
                            state: crate::child_workflow::CoordinatorState::Created,
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
                    parent_task_id.clone(),
                    "child.request.submitted",
                    [
                        ("child_task_id".to_owned(), request.child_task_id.clone()),
                        ("parent_task_id".to_owned(), parent_task_id.clone()),
                        ("role".to_owned(), request.role.clone()),
                    ],
                )
                .await;
                serde_json::to_vec(&serde_json::json!({ "request": request }))
                    .map_err(|error| error.to_string())
            }
            .await;
            if let Err(error) = &result {
                TaskCoordinator::record_audit(
                    &state,
                    crate::audit::AuditKind::Evidence,
                    parent_task_id.clone(),
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
