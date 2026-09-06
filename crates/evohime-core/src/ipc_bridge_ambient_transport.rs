use super::*;

impl IpcBridge {
    pub fn with_selected_model(mut self, selected: SelectedModel) -> Self {
        self.selected_model = selected;
        self
    }

    /// Streams journal entries newer than `after_sequence` to a connected
    /// client and returns the sequence it has now seen.
    ///
    /// Task progress reaches the shell this way rather than straight from the
    /// in-memory broadcast: the journal is what assigns sequence numbers, and
    /// the shell relies on them for resync after a reconnect.
    pub async fn push_journal_tail<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        after_sequence: u64,
    ) -> Result<u64, IpcBridgeError> {
        let batch = self
            .journal
            .replay_bounded(after_sequence as i64, 256)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let mut last_sequence = after_sequence;
        for record in batch.events {
            last_sequence = record.sequence_id as u64;
            let task_is_conversation_bound = if record.task_id.is_empty() {
                false
            } else {
                let database = self.journal.database().lock().await;
                match evohime_local_storage::domains::audit::task_binding(
                    database.connection(),
                    &record.task_id,
                ) {
                    Ok(binding) => binding.is_some(),
                    Err(_) => true,
                }
            };
            // Typed ledger rows (план 08-1/08-2) carry ExecutionEventV1 JSON
            // in payload; project it additively into the oneof without
            // touching the generic event_type/payload backward-compat path.
            let execution_event = record
                .event_type
                .starts_with("ledger.")
                .then(|| decode_typed_execution_event(&record.payload))
                .flatten();
            let typed_event = if record.event_type == "project_instruction_stack.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("ProjectInstructionStack").unwrap_or(&value);
                        Some(generated::event_envelope::Event::ProjectInstructionStack(
                            generated::ProjectInstructionStackEvent {
                                schema_version: 1,
                                workspace_root: event.get("workspace_root")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                revision: event
                                    .get("revision")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "team_coordinator.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("TeamCoordinator").unwrap_or(&value);
                        Some(generated::event_envelope::Event::TeamCoordinator(
                            generated::TeamCoordinatorEvent {
                                schema_version: 1,
                                work_item_id: event.get("work_item_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                revision: event
                                    .get("revision")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: event
                                    .get("status")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "conversation.event" {
                let subscription = self.conversation_subscription.lock().await.clone();
                decode_conversation_event(&record.payload).and_then(|conversation| {
                    let allowed = subscription
                        .as_ref()
                        .is_some_and(|(conversation_id, kinds)| {
                            conversation_id == &conversation.conversation_id
                                && (kinds.is_empty() || kinds.contains(&conversation.kind))
                        });
                    allowed.then(|| {
                        generated::event_envelope::Event::ConversationEventLog(
                            generated::ConversationEventLogEvent {
                                schema_version: crate::conversation_event_log::CONTRACT_VERSION,
                                operation: "live".into(),
                                conversation_id: conversation.conversation_id.clone(),
                                oldest_sequence: conversation.sequence,
                                newest_sequence: conversation.sequence,
                                has_older: false,
                                has_newer: false,
                                earliest_available_sequence: 0,
                                error_code: String::new(),
                                events: vec![conversation],
                            },
                        )
                    })
                })
            } else if record.event_type == "workspace_sets.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("WorkspaceSets").unwrap_or(&value);
                        Some(generated::event_envelope::Event::WorkspaceSets(
                            generated::WorkspaceSetsEvent {
                                schema_version: 1,
                                set_id: event.get("set_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "knowledge_source_registry.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value
                            .get("KnowledgeSourceRegistryProjectRole")
                            .unwrap_or(&value);
                        Some(generated::event_envelope::Event::KnowledgeSourceRegistry(
                            generated::KnowledgeSourceRegistryProjectRoleEvent {
                                schema_version: 1,
                                source_id: event.get("source_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                            },
                        ))
                    })
            } else if record.event_type == "durable_remote_task_bridge.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("DurableRemoteTaskBridge").unwrap_or(&value);
                        Some(generated::event_envelope::Event::DurableRemoteTaskBridge(
                            generated::DurableRemoteTaskBridgeEvent {
                                schema_version: 1,
                                remote_task_id: event.get("remote_task_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "message_intervention_policies.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("MessageInterventionPolicies").unwrap_or(&value);
                        Some(
                            generated::event_envelope::Event::MessageInterventionPolicies(
                                generated::MessageInterventionPoliciesEvent {
                                    schema_version: 1,
                                    operation: event.get("operation")?.as_str()?.to_owned(),
                                    version: event
                                        .get("version")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or_default(),
                                    status: String::new(),
                                    error_code: String::new(),
                                    projection_json: event
                                        .get("projection_json")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or("{}")
                                        .as_bytes()
                                        .to_vec(),
                                    truncated: false,
                                },
                            ),
                        )
                    })
            } else if record.event_type == "batch_invocation_runtime.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("BatchInvocationRuntime").unwrap_or(&value);
                        Some(generated::event_envelope::Event::BatchInvocationRuntime(
                            generated::BatchInvocationRuntimeEvent {
                                schema_version: 1,
                                batch_id: event.get("batch_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "policy_aware_tool_result_cache.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("PolicyAwareToolResultCache").unwrap_or(&value);
                        Some(
                            generated::event_envelope::Event::PolicyAwareToolResultCache(
                                generated::PolicyAwareToolResultCacheEvent {
                                    schema_version: 1,
                                    operation: event.get("operation")?.as_str()?.to_owned(),
                                    version: event
                                        .get("version")
                                        .and_then(serde_json::Value::as_u64)
                                        .unwrap_or_default(),
                                    status: String::new(),
                                    error_code: String::new(),
                                    projection_json: event
                                        .get("projection_json")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or("{}")
                                        .as_bytes()
                                        .to_vec(),
                                    truncated: false,
                                },
                            ),
                        )
                    })
            } else if record.event_type == "code_anchored_intent_markers.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("CodeAnchoredIntentMarkers").unwrap_or(&value);
                        Some(generated::event_envelope::Event::CodeAnchoredIntentMarkers(
                            generated::CodeAnchoredIntentMarkersEvent {
                                schema_version: 1,
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "model_purpose_routing.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("ModelPurposeRouting").unwrap_or(&value);
                        Some(generated::event_envelope::Event::ModelPurposeRouting(
                            generated::ModelPurposeRoutingEvent {
                                schema_version: 1,
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "local_model_runtime_manager.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("LocalModelRuntimeManager").unwrap_or(&value);
                        Some(generated::event_envelope::Event::LocalModelRuntimeManager(
                            generated::LocalModelRuntimeManagerEvent {
                                schema_version: 1,
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "architecture_snapshot.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .and_then(|value| {
                        let event = value.get("ArchitectureSnapshot").unwrap_or(&value);
                        Some(generated::event_envelope::Event::ArchitectureSnapshot(
                            generated::ArchitectureSnapshotEvent {
                                schema_version: 1,
                                snapshot_id: event.get("snapshot_id")?.as_str()?.to_owned(),
                                operation: event.get("operation")?.as_str()?.to_owned(),
                                version: event
                                    .get("version")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or(1),
                                status: String::new(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        ))
                    })
            } else if record.event_type == "persistent_agent_organization_registry.result" {
                serde_json::from_slice::<serde_json::Value>(&record.payload)
                    .ok()
                    .map(|value| {
                        let event = value
                            .get("PersistentAgentOrganizationRegistry")
                            .unwrap_or(&value);
                        generated::event_envelope::Event::PersistentAgentOrganizationRegistry(
                            generated::PersistentAgentOrganizationRegistryEvent {
                                schema_version: 1,
                                request_id: String::new(),
                                agent_id: event
                                    .get("agent_id")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                operation: event
                                    .get("operation")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                revision: event
                                    .get("revision")
                                    .and_then(serde_json::Value::as_u64)
                                    .unwrap_or_default(),
                                status: event
                                    .get("status")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                error_code: String::new(),
                                projection_json: event
                                    .get("projection_json")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("{}")
                                    .as_bytes()
                                    .to_vec(),
                                truncated: false,
                            },
                        )
                    })
            } else {
                execution_event
                    .map(|event| generated::event_envelope::Event::ExecutionEvent(Box::new(event)))
            };
            if record.event_type == "conversation.event" && typed_event.is_none() {
                continue;
            }
            let payload = if task_is_conversation_bound {
                serde_json::to_vec(
                    &serde_json::json!({"redacted": true, "conversation_projection": true}),
                )?
            } else {
                record.payload
            };
            let event = generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: record.sequence_id as u64,
                task_id: record.task_id,
                event_type: record.event_type,
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: typed_event,
            };
            transport::write_frame(writer, &event.encode_to_vec()).await?;
        }
        Ok(last_sequence)
    }

    /// Sequence the journal has already durably recorded.
    pub async fn latest_sequence(&self) -> u64 {
        self.journal.latest_sequence().await.max(0) as u64
    }

    /// Listener that fires whenever a task emits, so the server knows there is
    /// a journal tail worth flushing.
    /// Signal that fires once an event is durably journalled. The pipe server
    /// pushes the journal tail on this instead of on the broadcast itself,
    /// which used to overtake the writer and strand the last event of a task.
    pub fn journalled(&self) -> Option<tokio::sync::watch::Receiver<u64>> {
        self.coordinator
            .as_ref()
            .map(|coordinator| coordinator.journalled())
    }

    pub(crate) fn receipt_status(&self) -> serde_json::Value {
        let manager = &self.receipt_keys;
        let active = manager.active_path().exists();
        let history = manager.history_path().exists();
        let status = if !active && !history {
            "not_initialized".to_string()
        } else if !active || !history {
            "key.recovery_required".to_string()
        } else if manager.journal_path().exists() {
            "key.rotation_incomplete".to_string()
        } else {
            match manager.verify_history(None) {
                Ok(VerificationStatus::Verified) => "verified_unpinned".to_string(),
                Ok(VerificationStatus::Untrusted) => {
                    let loaded = manager.load_history().ok();
                    if loaded.as_ref().is_some_and(|items| {
                        items.iter().any(|item| {
                            matches!(item.continuity.as_str(), "broken" | "compromised")
                        })
                    }) {
                        return serde_json::json!({
                            "status": "key.trust_required",
                            "key_id": manager.load_signer().ok().map(|(metadata, _)| metadata.key_id),
                            "history_present": history,
                            "active_present": active,
                            "rotation_journal_present": manager.journal_path().exists(),
                        });
                    }
                    let genesis =
                        loaded.and_then(|items| items.first().map(|item| item.new_key_id.clone()));
                    match genesis.and_then(|key| manager.trusted_genesis(&key).ok()) {
                        Some(true) => "trusted".to_string(),
                        _ => "key.trust_required".to_string(),
                    }
                }
                Ok(VerificationStatus::Broken) => "key.history_incomplete".to_string(),
                Ok(VerificationStatus::Unsupported) => "unsupported".to_string(),
                Err(error) => error.to_string(),
            }
        };
        let key_id = std::fs::read(manager.active_path())
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<evohime_receipts::key_lifecycle::ActiveKeyMetadata>(&bytes)
                    .ok()
            })
            .map(|metadata| metadata.key_id);
        serde_json::json!({"status": status, "key_id": key_id, "history_present": history, "active_present": active, "rotation_journal_present": manager.journal_path().exists()})
    }

    pub(crate) async fn take_receipt_approval<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        approval_id: &str,
        operation: &str,
    ) -> Result<bool, IpcBridgeError> {
        let Some(approvals) = &self.approvals else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        let Ok(id) = uuid::Uuid::parse_str(approval_id) else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        if approvals.consume_approved(id).await {
            Ok(true)
        } else {
            self.write_response(writer, "key.approval_required", serde_json::to_vec(&serde_json::json!({"operation": operation, "approval_id": id.to_string(), "error_code":"approval.required"}))?).await?;
            Ok(false)
        }
    }

    pub(crate) async fn dispatch_save_continuation_policy(
        &self,
        request: generated::SaveContinuationPolicy,
        client_id: &str,
        request_id: &str,
        command_hash: &str,
    ) -> Result<Vec<u8>, String> {
        let policy: crate::continuation::ContinuationPolicyV1 =
            serde_json::from_slice(&request.policy_json)
                .map_err(|_| "invalid_argument".to_string())?;
        if request.policy_json.len() > crate::continuation::MAX_POLICY_BYTES
            || (!request.owner_scope.is_empty() && request.owner_scope != policy.scope.owner_scope)
            || (!request.actor.is_empty() && request.actor != policy.actor)
        {
            return Err("invalid_argument".into());
        }
        policy
            .validate()
            .map_err(|_| "invalid_policy".to_string())?;
        for gate in &policy.gates {
            let available = match gate.kind {
                crate::continuation::GateKind::Tool => {
                    self.tools
                        .as_ref()
                        .is_some_and(|tools| tools.manifest_for(&gate.capability_ref).is_some())
                        && gate.capability_ref != "shell"
                        && !gate.capability_ref.starts_with("shell.")
                }
                crate::continuation::GateKind::Workflow => {
                    crate::workflow_templates::template(&gate.capability_ref).is_some()
                }
                crate::continuation::GateKind::Evidence => self
                    .workflow_registry
                    .provider(&gate.capability_ref)
                    .is_some(),
                crate::continuation::GateKind::Approval => gate.capability_ref == "approval",
            };
            if !available {
                return Err("gate_unavailable".into());
            }
        }
        let canonical = policy
            .canonical_json()
            .map_err(|_| "invalid_policy".to_string())?;
        let result = serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::continuation::POLICY_SCHEMA_VERSION,
            "policy_id": policy.id,
            "revision": policy.revision,
            "content_hash": policy.content_hash,
            "enabled": policy.enabled
        }))
        .map_err(|_| "serialization_failed".to_string())?;
        let journal = self.journal.clone();
        let database = journal.database().lock().await;
        if let Some(previous) = database
            .record_deduplicated(client_id, request_id, command_hash, &[])
            .map_err(|_| "idempotency_conflict".to_string())?
        {
            return Ok(previous);
        }
        evohime_local_storage::domains::runs::save_policy(
            database.connection(),
            &evohime_local_storage::domains::runs::PolicyRecord {
                policy_id: policy.id.clone(),
                revision: policy.revision as i64,
                owner_scope: policy.scope.owner_scope.clone(),
                actor: policy.actor.clone(),
                enabled: policy.enabled,
                canonical_json: canonical,
                content_hash: policy.content_hash.clone(),
                created_at_ms: policy.created_at_ms,
                updated_at_ms: policy.updated_at_ms,
            },
        )
        .map_err(|_| "storage_failed".to_string())?;
        database
            .record_deduplicated(client_id, request_id, command_hash, &result)
            .map_err(|_| "idempotency_conflict".to_string())?;
        Ok(result)
    }

    pub(crate) async fn dispatch_start_continuation(
        &self,
        request: generated::StartContinuationRun,
    ) -> Result<Vec<u8>, String> {
        if request.run_id.is_empty()
            || request.policy_id.is_empty()
            || request.owner_scope.is_empty()
            || request.idempotency_key.is_empty()
            || request.task_id.is_empty()
        {
            return Err("invalid_argument".into());
        }
        let journal = self.journal.clone();
        let database = journal.database().lock().await;
        if let Some(existing) = evohime_local_storage::domains::runs::get_run_by_idempotency(
            database.connection(),
            &request.owner_scope,
            &request.idempotency_key,
        )
        .map_err(|_| "storage_failed".to_string())?
        {
            if existing.run_id == request.run_id
                && existing.task_id == request.task_id
                && existing.policy_id == request.policy_id
                && existing.policy_revision == request.policy_revision as i64
            {
                return continuation_public_json(&existing, &[]);
            }
            return Err("idempotency_conflict".into());
        }
        let policy = evohime_local_storage::domains::runs::get_policy(
            database.connection(),
            &request.policy_id,
            request.policy_revision as i64,
            &request.owner_scope,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "policy_not_found".to_string())?;
        if !policy.enabled {
            return Err("policy_disabled".into());
        }
        let policy_json: crate::continuation::ContinuationPolicyV1 =
            serde_json::from_slice(&policy.canonical_json)
                .map_err(|_| "policy_corrupt".to_string())?;
        let now = crate::task_memory::now_millis() as i64;
        let record = evohime_local_storage::domains::runs::RunRecord {
            run_id: request.run_id.clone(),
            idempotency_key: request.idempotency_key,
            task_id: request.task_id,
            owner_scope: request.owner_scope,
            policy_id: request.policy_id,
            policy_revision: request.policy_revision as i64,
            policy_hash: policy.content_hash,
            goal_id: (!request.goal_id.is_empty()).then_some(request.goal_id),
            goal_version: (request.goal_version > 0).then_some(request.goal_version as i64),
            state: "running".into(),
            continuation_index: 0,
            max_continuations: policy_json.budget.max_continuations as i64,
            max_model_turns: policy_json.budget.max_model_turns as i64,
            used_model_turns: 0,
            token_budget: policy_json.budget.max_tokens.map(|v| v as i64),
            token_used: 0,
            cost_budget_micros: policy_json.budget.max_cost_micros.map(|v| v as i64),
            cost_used_micros: 0,
            stop_reason: None,
            prompt: None,
            workspace_path: None,
            created_at_ms: now,
            updated_at_ms: now,
        };
        evohime_local_storage::domains::runs::create_run(database.connection(), &record)
            .map_err(|error| {
                if matches!(error, rusqlite::Error::SqliteFailure(_, _)) {
                    "run_exists"
                } else {
                    "storage_failed"
                }
                .to_string()
            })?;
        continuation_public_json(&record, &[])
    }

    pub(crate) async fn dispatch_get_continuation(
        &self,
        request: generated::GetContinuationRun,
    ) -> Result<Vec<u8>, String> {
        let database = self.journal.database().lock().await;
        let run = evohime_local_storage::domains::runs::get_run(
            database.connection(),
            &request.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "run_not_found".to_string())?;
        let gates = evohime_local_storage::domains::runs::list_latest_gate_results(
            database.connection(),
            &run.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?;
        continuation_public_json(&run, &gates)
    }

    pub(crate) async fn dispatch_stop_continuation(
        &self,
        request: generated::StopContinuation,
    ) -> Result<Vec<u8>, String> {
        if request.run_id.is_empty() || request.expected_state != "running" {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        evohime_local_storage::domains::runs::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::domains::runs::TransitionActionInput {
                run_id: &request.run_id,
                idempotency_key: &request.idempotency_key,
                action: "stop",
                expected_state: &request.expected_state,
                next_state: "stopped",
                stop_reason: "user_stop",
                now_ms: crate::task_memory::now_millis() as i64,
            },
        )
        .map_err(|_| "storage_failed".to_string())
    }

    pub(crate) async fn dispatch_transition_continuation(
        &self,
        run_id: String,
        idempotency_key: String,
        expected_state: String,
        next_state: &'static str,
        action: &'static str,
    ) -> Result<Vec<u8>, String> {
        if run_id.is_empty()
            || idempotency_key.is_empty()
            || (expected_state != "running" && expected_state != "paused")
        {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        evohime_local_storage::domains::runs::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::domains::runs::TransitionActionInput {
                run_id: &run_id,
                idempotency_key: &idempotency_key,
                action,
                expected_state: &expected_state,
                next_state,
                stop_reason: action,
                now_ms: crate::task_memory::now_millis() as i64,
            },
        )
        .map_err(|_| "storage_failed".to_string())
    }

    pub(crate) async fn dispatch_resume_continuation(
        &self,
        request: generated::ResumeContinuation,
    ) -> Result<evohime_local_storage::domains::runs::RunRecord, String> {
        if request.run_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.expected_state != "paused"
        {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        let run = evohime_local_storage::domains::runs::get_run(
            database.connection(),
            &request.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "run_not_found".to_string())?;
        if run.prompt.is_none() || run.workspace_path.is_none() {
            return Err("resume_context_unavailable".into());
        }
        let _action_result = evohime_local_storage::domains::runs::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::domains::runs::TransitionActionInput {
                run_id: &request.run_id,
                idempotency_key: &request.idempotency_key,
                action: "resume",
                expected_state: "paused",
                next_state: "running",
                stop_reason: "approval_resolution",
                now_ms: crate::task_memory::now_millis() as i64,
            },
        )
        .map_err(|_| "storage_failed".to_string())?;
        evohime_local_storage::domains::runs::get_run(database.connection(), &request.run_id)
            .map_err(|_| "storage_failed".to_string())?
            .ok_or_else(|| "run_not_found".into())
    }
}
