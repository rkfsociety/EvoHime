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
                match evohime_local_storage::conversation_event_log_store::task_binding(
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
        evohime_local_storage::continuation_store::save_policy(
            database.connection(),
            &evohime_local_storage::continuation_store::PolicyRecord {
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
        if let Some(existing) = evohime_local_storage::continuation_store::get_run_by_idempotency(
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
        let policy = evohime_local_storage::continuation_store::get_policy(
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
        let record = evohime_local_storage::continuation_store::RunRecord {
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
        evohime_local_storage::continuation_store::create_run(database.connection(), &record)
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
        let run = evohime_local_storage::continuation_store::get_run(
            database.connection(),
            &request.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "run_not_found".to_string())?;
        let gates = evohime_local_storage::continuation_store::list_latest_gate_results(
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
        evohime_local_storage::continuation_store::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::continuation_store::TransitionActionInput {
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
        evohime_local_storage::continuation_store::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::continuation_store::TransitionActionInput {
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
    ) -> Result<evohime_local_storage::continuation_store::RunRecord, String> {
        if request.run_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.expected_state != "paused"
        {
            return Err("invalid_argument".into());
        }
        let mut database = self.journal.database().lock().await;
        let run = evohime_local_storage::continuation_store::get_run(
            database.connection(),
            &request.run_id,
        )
        .map_err(|_| "storage_failed".to_string())?
        .ok_or_else(|| "run_not_found".to_string())?;
        if run.prompt.is_none() || run.workspace_path.is_none() {
            return Err("resume_context_unavailable".into());
        }
        let _action_result = evohime_local_storage::continuation_store::apply_transition_action(
            database.connection_mut(),
            evohime_local_storage::continuation_store::TransitionActionInput {
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
        evohime_local_storage::continuation_store::get_run(database.connection(), &request.run_id)
            .map_err(|_| "storage_failed".to_string())?
            .ok_or_else(|| "run_not_found".into())
    }

    pub fn process_once<'a, R: AsyncRead + Unpin + 'a, W: AsyncWrite + Unpin + 'a>(
        &'a self,
        reader: &'a mut R,
        writer: &'a mut W,
    ) -> Pin<Box<dyn Future<Output = Result<(), IpcBridgeError>> + 'a>> {
        Box::pin(async move {
            let payload = transport::read_frame(reader).await?;
            let command = generated::CommandEnvelope::decode(payload.as_slice())?;
            let request_id = command.request_id.clone();
            let client_id = command.client_id.clone();
            let command_hash = hex_encode(&payload);
            match command.command {
                Some(generated::command_envelope::Command::Handshake(_)) => {
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "core.ready".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: Some(generated::event_envelope::Event::Ready(generated::Ready {
                            protocol: Some(protocol()),
                            core_version: env!("CARGO_PKG_VERSION").into(),
                            core_info: Some(core_info()),
                        })),
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::GetReceiptKeyStatus(_)) => {
                    let mut status = self.receipt_status();
                    if let Ok(mut database) = self.journal.database().try_lock() {
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        ) {
                            if let Ok(counts) = runtime.counts() {
                                if let Some(object) = status.as_object_mut() {
                                    object.insert(
                                        "runtime_counts".into(),
                                        serde_json::json!({
                                            "pending": counts.pending,
                                            "pending_recovery": counts.pending_recovery,
                                            "quarantined": counts.quarantined,
                                            "approval_pending": counts.approval_pending,
                                        }),
                                    );
                                    if let Ok((rate, version)) = runtime.audit_sampling_config() {
                                        object.insert("audit_sampling".into(), serde_json::json!({"rate": rate, "policy_version": version}));
                                    }
                                    if let Ok(metrics) = runtime.metrics() {
                                        object.insert(
                                            "runtime_metrics".into(),
                                            serde_json::json!(metrics.counters),
                                        );
                                    }
                                    if let Ok(diagnostics) = runtime.diagnostic_counts() {
                                        object.insert(
                                            "runtime_diagnostics".into(),
                                            serde_json::json!(diagnostics),
                                        );
                                    }
                                    if let Ok(rotation) = runtime.storage_rotation_job() {
                                        object.insert("storage_rotation".into(), serde_json::json!(rotation.map(|job| serde_json::json!({"job_id": job.job_id, "old_key_id": job.old_key_id, "new_key_id": job.new_key_id, "cursor": job.cursor, "generation": job.generation, "state": job.state}))));
                                    }
                                }
                            }
                        }
                    }
                    self.write_response(writer, "key.status", serde_json::to_vec(&status)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ClosePendingReceiptAction(request)) => {
                    if !request.operator_confirmed
                        || request.action_id.is_empty()
                        || request.input_json.len()
                            > evohime_receipts::runtime::MAX_CALL_INPUT_BYTES
                    {
                        self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let action_id = uuid::Uuid::parse_str(&request.action_id)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let input: serde_json::Value = serde_json::from_str(&request.input_json)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let mut database = self.journal.database().lock().await;
                    let (task_id, run_id, tool_name, normalized_scope, policy_id, decision, state, approval_id, parent_approval_ref): (String,String,String,String,String,String,String,Option<String>,Option<String>) = database.connection().query_row(
                    "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,state,approval_id,parent_approval_ref FROM receipt_actions WHERE action_id=?1",
                    [action_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                    if state != "pending_recovery" {
                        self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery"}))?).await?;
                        return Ok(());
                    }
                    let policy_decision = match decision.as_str() {
                        "allow" => evohime_receipts::runtime::PolicyDecision::Allow,
                        "approval_required" => {
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired
                        }
                        "deny" => evohime_receipts::runtime::PolicyDecision::Deny,
                        _ => {
                            self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id,
                        run_id,
                        tool_name,
                        policy_id,
                        normalized_scope,
                        input,
                        policy_decision,
                        approval_id: approval_id
                            .and_then(|value| uuid::Uuid::parse_str(&value).ok()),
                        parent_approval_ref,
                        preview: "unknown result closure".into(),
                    };
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .refuse(&receipt_request, "recovery_pending")
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":true,"action_id":request.action_id,"receipt_hash":receipt_hash,"completion_source":"reconciliation"}))?).await?;
                }
                Some(generated::command_envelope::Command::SetReceiptAuditSamplingRate(
                    request,
                )) => {
                    if request.rate > 100 {
                        self.write_response(writer, "receipt.sampling_rate", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    runtime
                        .set_audit_sampling_rate(true, request.rate as u8)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.sampling_rate", serde_json::to_vec(&serde_json::json!({"ok":true,"rate":request.rate,"policy_version":evohime_receipts::SAMPLING_POLICY_VERSION}))?).await?;
                }
                Some(generated::command_envelope::Command::ReconcilePendingReceiptAction(
                    request,
                )) => {
                    const MAX_RECONCILIATION_INPUT_BYTES: usize =
                        evohime_receipts::runtime::MAX_CALL_INPUT_BYTES;
                    let read_only = matches!(
                        request.tool_name.as_str(),
                        "filesystem.read"
                            | "filesystem.list"
                            | "git.status"
                            | "git.diff"
                            | "git.log"
                            | "git.show"
                            | "git.blame"
                            | "git.changed_files"
                            | "workspace.list"
                            | "workspace.read"
                            | "workspace.search"
                    );
                    if request.old_action_id.is_empty()
                        || request.tool_name.len() > 128
                        || !read_only
                        || request.input_json.len() > MAX_RECONCILIATION_INPUT_BYTES
                        || request.workspace_path.is_empty()
                        || request.workspace_path.len() > 32 * 1024
                        || request.workspace_path.contains('\n')
                    {
                        self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let old_action_id = match uuid::Uuid::parse_str(&request.old_action_id) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let input: serde_json::Value = match serde_json::from_str(&request.input_json) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let tools = match self.tools.as_ref() {
                        Some(value) => Arc::clone(value),
                        None => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.tool_unavailable"}))?).await?;
                            return Ok(());
                        }
                    };
                    let (task_id, old_state): (String, String) = {
                        let database = self.journal.database().lock().await;
                        database
                            .connection()
                            .query_row(
                                "SELECT task_id,state FROM receipt_actions WHERE action_id=?1",
                                [old_action_id.to_string()],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .map_err(|error| FrameError::Io(error.to_string()))?
                    };
                    if old_state != "pending_recovery" {
                        self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery"}))?).await?;
                        return Ok(());
                    }
                    let reconciliation_task_id = match task_id.parse() {
                        Ok(task_id) => task_id,
                        Err(error) => {
                            tracing::warn!(%error, task_id, "invalid reconciliation task id; generating one");
                            uuid::Uuid::now_v7()
                        }
                    };
                    let context = ToolContext {
                        workspace_root: std::path::PathBuf::from(&request.workspace_path),
                        task_id: reconciliation_task_id,
                        session_id: None,
                        progress_tx: None,
                    };
                    let (scope, preview) = match tools
                        .preflight(&context, &request.tool_name, &input)
                        .await
                    {
                        Ok(evohime_tool_runtime::ToolPreflightDecision::Allowed {
                            scope,
                            preview,
                        }) => (scope, preview),
                        Ok(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.policy_denied"}))?).await?;
                            return Ok(());
                        }
                        Err(_) => {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.policy_denied"}))?).await?;
                            return Ok(());
                        }
                    };
                    let new_action_id = uuid::Uuid::now_v7();
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id: new_action_id,
                        task_id: task_id.clone(),
                        run_id: format!("reconciliation-{}", new_action_id),
                        tool_name: request.tool_name.clone(),
                        policy_id: "reconciliation:read_only".into(),
                        normalized_scope: scope,
                        input: input.clone(),
                        policy_decision: evohime_receipts::runtime::PolicyDecision::Allow,
                        approval_id: None,
                        parent_approval_ref: None,
                        preview: match serde_json::to_string(&preview) {
                            Ok(preview) => preview,
                            Err(error) => {
                                tracing::warn!(%error, "reconciliation preview serialization failed");
                                "read-only reconciliation".into()
                            }
                        },
                    };
                    {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        if !matches!(
                            runtime
                                .prepare(receipt_request.clone())
                                .map_err(|error| FrameError::Io(error.to_string()))?,
                            evohime_receipts::runtime::PrepareOutcome::Prepared { .. }
                        ) {
                            self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.precondition_failed"}))?).await?;
                            return Ok(());
                        }
                        runtime
                            .mark_started(new_action_id)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                    let result = tools
                        .execute_with_cancellation(
                            &context,
                            &request.tool_name,
                            input,
                            CancellationToken::new(),
                        )
                        .await;
                    let (status, digest, error_category) = match &result {
                        Ok(value) => (
                            "succeeded",
                            evohime_receipts::sha256_hex(value.output.as_bytes()),
                            None,
                        ),
                        Err(_error) => (
                            "failed",
                            evohime_receipts::sha256_hex(b"reconciliation_tool_error"),
                            Some("tool_error"),
                        ),
                    };
                    let receipt_hash = {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        )
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        runtime
                            .mark_returned(new_action_id)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                        match runtime.complete_reconciliation(
                            &receipt_request,
                            old_action_id,
                            status,
                            &digest,
                            error_category,
                        ) {
                            Ok(hash) => hash,
                            Err(_error) => {
                                let _ = runtime
                                    .mark_pending_recovery(new_action_id, "signature_failed");
                                self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery","action_id":new_action_id.to_string()}))?).await?;
                                return Ok(());
                            }
                        }
                    };
                    self.write_response(writer, "receipt.reconciliation", serde_json::to_vec(&serde_json::json!({"ok":true,"old_action_id":old_action_id.to_string(),"action_id":new_action_id.to_string(),"status":status,"receipt_hash":receipt_hash,"completion_source":"reconciliation"}))?).await?;
                }
                Some(generated::command_envelope::Command::UnquarantineReceiptAction(request)) => {
                    if !request.operator_confirmed
                        || request.action_id.is_empty()
                        || request.input_json.len()
                            > evohime_receipts::runtime::MAX_CALL_INPUT_BYTES
                        || request.checkpoint.is_empty()
                        || request.checkpoint.len() > 256
                        || request.checkpoint.contains('\n')
                    {
                        self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let checkpoint_valid = std::fs::read(self.receipt_keys.checkpoint_path())
                        .ok()
                        .and_then(|bytes| {
                            serde_json::from_slice::<
                                evohime_receipts::key_lifecycle::KeyHistoryCheckpoint,
                            >(&bytes)
                            .ok()
                        })
                        .and_then(|checkpoint| {
                            if checkpoint.checkpoint_id != request.checkpoint {
                                return None;
                            }
                            if !self
                                .receipt_keys
                                .trusted_genesis(&checkpoint.genesis_key_id)
                                .ok()?
                            {
                                return Some(false);
                            }
                            let history = self.receipt_keys.load_history().ok()?;
                            Some(
                                evohime_receipts::key_lifecycle::verify_checkpoint(
                                    &checkpoint,
                                    &history,
                                    Some(&checkpoint.genesis_key_id),
                                )
                                .is_ok(),
                            )
                        })
                        .unwrap_or(false);
                    if !checkpoint_valid {
                        self.write_response(
                        writer,
                        "receipt.unquarantine",
                        serde_json::to_vec(
                            &serde_json::json!({"ok":false,"error_code":"receipt.key_untrusted"}),
                        )?,
                    )
                    .await?;
                        return Ok(());
                    }
                    let action_id = match uuid::Uuid::parse_str(&request.action_id) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let input: serde_json::Value = match serde_json::from_str(&request.input_json) {
                        Ok(value) => value,
                        Err(_) => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let mut database = self.journal.database().lock().await;
                    let (task_id, run_id, tool_name, normalized_scope, policy_id, decision, state, approval_id, parent_approval_ref): (String,String,String,String,String,String,String,Option<String>,Option<String>) = database.connection().query_row(
                    "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,state,approval_id,parent_approval_ref FROM receipt_actions WHERE action_id=?1",
                    [action_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                    if state != "quarantined" {
                        self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                        return Ok(());
                    }
                    let policy_decision = match decision.as_str() {
                        "allow" => evohime_receipts::runtime::PolicyDecision::Allow,
                        "approval_required" => {
                            evohime_receipts::runtime::PolicyDecision::ApprovalRequired
                        }
                        "deny" => evohime_receipts::runtime::PolicyDecision::Deny,
                        _ => {
                            self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                            return Ok(());
                        }
                    };
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id,
                        task_id,
                        run_id,
                        tool_name,
                        policy_id,
                        normalized_scope,
                        input,
                        policy_decision,
                        approval_id: approval_id
                            .and_then(|value| uuid::Uuid::parse_str(&value).ok()),
                        parent_approval_ref,
                        preview: "manual quarantine closure".into(),
                    };
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(
                        database.connection_mut(),
                        &signer,
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let receipt_hash = runtime
                        .unquarantine(&receipt_request, true, &request.checkpoint)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "receipt.unquarantine", serde_json::to_vec(&serde_json::json!({"ok":true,"action_id":request.action_id,"receipt_hash":receipt_hash,"state":"refused","dispatch_allowed":false}))?).await?;
                }
                Some(generated::command_envelope::Command::ListReceipts(request)) => {
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.listed",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        100
                    } else {
                        request.limit as i64
                    };
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::list_receipts(
                        database.connection(),
                        &filter,
                        limit,
                    ) {
                        Ok(result) => {
                            self.write_response(
                            writer,
                            "receipts.listed",
                            serde_json::to_vec(&serde_json::json!({
                                "ok": true,
                                "snapshot_last_sequence": result.snapshot_last_sequence.to_string(),
                                "rows": result.rows,
                            }))?,
                        )
                        .await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.listed",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::VerifyReceipts(request)) => {
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.verified",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        500
                    } else {
                        request.limit as i64
                    };
                    let trust_key = if request.trust_key_id.is_empty() {
                        None
                    } else {
                        Some(request.trust_key_id.as_str())
                    };
                    let key_history = self.receipt_keys.load_history().unwrap_or_default();
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::verify_receipts(
                        database.connection(),
                        &key_history,
                        trust_key,
                        &filter,
                        limit,
                    ) {
                        Ok(result) => {
                            self.write_response(
                            writer,
                            "receipts.verified",
                            serde_json::to_vec(&serde_json::json!({
                                "ok": true,
                                "status": result.verification.status,
                                "code": result.verification.code,
                                "requested_count": result.requested_count,
                                "actual_verified_count": result.verification.actual_verified_count,
                                "chain_start_hash": result.verification.chain_start_hash,
                                "chain_end_hash": result.verification.chain_end_hash,
                                "rows": result.verification.rows,
                            }))?,
                        )
                        .await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.verified",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::ExportReceipts(request)) => {
                    if request.replace
                        || request.destination_path.is_empty()
                        || request.destination_path.len() > 4096
                    {
                        self.write_response(writer, "receipts.exported", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":if request.replace { "receipts.unsupported_operation" } else { "receipts.invalid_filter" }}))?).await?;
                        return Ok(());
                    }
                    let filter = match receipt_filter_from_request(
                        &request.task_id,
                        &request.run_id,
                        &request.action_id,
                        &request.from_rfc3339,
                        &request.to_rfc3339,
                    ) {
                        Ok(value) => value,
                        Err(code) => {
                            self.write_response(
                                writer,
                                "receipts.exported",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":code}),
                                )?,
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let limit = if request.limit == 0 {
                        100_000
                    } else {
                        request.limit as i64
                    };
                    let destination = std::path::PathBuf::from(&request.destination_path);
                    let key_history = self.receipt_keys.load_history().unwrap_or_default();
                    let database = self.journal.database().lock().await;
                    match evohime_receipts::export::export_receipts(
                        database.connection(),
                        &key_history,
                        &destination,
                        &filter,
                        limit,
                    ) {
                        Ok(manifest) => {
                            let manifest_sha256 = std::fs::read(destination.join("manifest.json"))
                                .ok()
                                .map(|bytes| evohime_receipts::sha256_hex(&bytes));
                            self.write_response(writer, "receipts.exported", serde_json::to_vec(&serde_json::json!({
                            "ok": true,
                            "export_id": manifest.export_id,
                            "destination_basename": destination.file_name().and_then(|value| value.to_str()),
                            "snapshot_last_sequence": manifest.snapshot_last_sequence.to_string(),
                            "requested_count": manifest.requested_count,
                            "selected_count": manifest.selected_count,
                            "actual_exported_count": manifest.actual_exported_count,
                            "manifest_sha256": manifest_sha256,
                        }))?).await?;
                        }
                        Err(error) => {
                            self.write_response(
                                writer,
                                "receipts.exported",
                                serde_json::to_vec(
                                    &serde_json::json!({"ok":false,"error_code":error.to_string()}),
                                )?,
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::TrustReceiptGenesis(request)) => {
                    if !self
                        .take_receipt_approval(writer, &request.approval_id, "TrustReceiptGenesis")
                        .await?
                    {
                        return Ok(());
                    }
                    let result = self
                        .receipt_keys
                        .trust_genesis(&request.genesis_key_id, &request.source);
                    let payload = match result {
                        Ok(()) => {
                            serde_json::json!({"status": "trusted", "genesis_key_id": request.genesis_key_id})
                        }
                        Err(error) => {
                            serde_json::json!({"status": error.to_string(), "error_code": error.to_string()})
                        }
                    };
                    self.write_response(writer, "key.trust", serde_json::to_vec(&payload)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateNewReceiptGenesis(request)) => {
                    if !self
                        .take_receipt_approval(
                            writer,
                            &request.approval_id,
                            "CreateNewReceiptGenesis",
                        )
                        .await?
                    {
                        return Ok(());
                    }
                    let manager = self.receipt_keys.clone();
                    let database = self.journal.database().clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let mut database = database.blocking_lock();
                        manager.create_new_genesis_with_database(database.connection_mut(), "user")
                    })
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = match result {
                        Ok(key_id) => {
                            serde_json::json!({"status":"recovered", "key_id":key_id, "trust_required":true})
                        }
                        Err(error) => {
                            serde_json::json!({"status":"failed", "error_code":error.to_string()})
                        }
                    };
                    self.write_response(writer, "key.recovery", serde_json::to_vec(&payload)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::RotateReceiptKey(request)) => {
                    if !self
                        .take_receipt_approval(writer, &request.approval_id, "RotateReceiptKey")
                        .await?
                    {
                        return Ok(());
                    }
                    let reason = request.reason.trim().to_string();
                    if !matches!(reason.as_str(), "manual" | "compromise") {
                        self.write_response(
                            writer,
                            "key.rotation_failed",
                            br#"{"error_code":"key.rotation_failed"}"#.to_vec(),
                        )
                        .await?;
                    } else {
                        let manager = self.receipt_keys.clone();
                        let database = self.journal.database().clone();
                        let rotation_reason = reason.clone();
                        let result = tokio::task::spawn_blocking(move || -> Result<(String, Option<String>), String> {
                        let mut database = database.blocking_lock();
                        let protected_count: i64 = database.connection().query_row(
                            "SELECT COUNT(*) FROM receipt_protected_actions",
                            [],
                            |row| row.get(0),
                        ).map_err(|error| error.to_string())?;
                        let storage_key_id: Option<String> = if protected_count > 0 {
                            let signer = super::CoreReceiptSigner(Arc::clone(&manager));
                            let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                                .map_err(|error| error.to_string())?;
                            let existing_job = runtime.storage_rotation_job().map_err(|error| error.to_string())?;
                            let (job_id, old_storage_key_id, new_storage_key_id, generation) = if let Some(job) = existing_job.filter(|job| matches!(job.state.as_str(), "running" | "failed")) {
                                (job.job_id, job.old_key_id, job.new_key_id, job.generation)
                            } else {
                                let old_storage_key_id = manager.storage_key_id().map_err(|error| error.to_string())?;
                                let new_storage_key_id = manager.rotate_storage_key(true).map_err(|error| error.to_string())?;
                                (format!("storage-{}", uuid::Uuid::now_v7()), old_storage_key_id, new_storage_key_id, SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis() as i64).unwrap_or_default())
                            };
                            loop {
                                let progressed = runtime.rewrap_protected_batch(
                                    &job_id,
                                    &old_storage_key_id,
                                    &new_storage_key_id,
                                    generation,
                                    32,
                                    |envelope| manager.rewrap_storage_with_key_id(envelope, &new_storage_key_id).map_err(|_| evohime_receipts::runtime::RuntimeError::Code("storage_key_unavailable")),
                                ).map_err(|error| error.to_string())?;
                                if !progressed { break; }
                            }
                            Some(new_storage_key_id)
                        } else {
                            None
                        };
                        let signing_key_id = manager.rotate_with_database(
                            database.connection_mut(),
                            &rotation_reason,
                            "user",
                        ).map_err(|error| error.to_string())?;
                        Ok((signing_key_id, storage_key_id))
                    })
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                        let payload = match result {
                            Ok((key_id, storage_key_id)) => {
                                serde_json::json!({"status":"rotated", "key_id":key_id, "storage_key_id":storage_key_id, "reason":reason})
                            }
                            Err(error) => {
                                serde_json::json!({"status":"failed", "error_code":error.to_string()})
                            }
                        };
                        self.write_response(writer, "key.rotation", serde_json::to_vec(&payload)?)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::ResyncRequest(request)) => {
                    evohime_desktop_ipc::validate_resync_request(&request)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    if self.stale_generation(&command) {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            request.after_sequence,
                            None,
                            latest,
                            REPLAY_GAP_REASON_STALE_GENERATION,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    let limit = if request.max_events == 0 {
                        evohime_desktop_ipc::DEFAULT_RESYNC_MAX_EVENTS
                    } else {
                        request.max_events
                    } as usize;
                    let batch = self
                        .journal
                        .replay_bounded(request.after_sequence as i64, limit)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let last_sequence = batch
                        .events
                        .last()
                        .map(|record| record.sequence_id as u64)
                        .unwrap_or(request.after_sequence);
                    if batch.gap_detected {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            request.after_sequence,
                            batch.first_available_sequence.map(|value| value as u64),
                            latest,
                            REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    // Снапшот, не влезающий в кадр IPC, раньше обрывал соединение с
                    // оболочкой: она навсегда оставалась без состояния и рисовала
                    // «нет связи». Теперь превышение лимита деградирует до
                    // поштучной отправки тех же событий.
                    let snapshot = if request.include_full_snapshot {
                        let snapshot_json = serde_json::to_vec(&serde_json::json!({
                            "schema_version": 1,
                            "core_instance_id": self.core_instance_id,
                            "session_epoch": self.session_epoch,
                            "snapshot_sequence_id": last_sequence,
                            "after_sequence": request.after_sequence,
                            "actions": typed_snapshot_actions(&batch.events),
                            "events": batch.events.iter().map(|record| serde_json::json!({
                                "sequence_id": record.sequence_id,
                                "task_id": record.task_id,
                                "event_type": record.event_type,
                                "payload": record.payload,
                                "created_at": record.created_at,
                            })).collect::<Vec<_>>(),
                        }))
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                        let candidate = generated::FullSnapshot {
                            sequence_id: last_sequence,
                            snapshot_json,
                        };
                        match evohime_desktop_ipc::validate_full_snapshot(&candidate) {
                            Ok(()) => Some(candidate),
                            Err(error) => {
                                tracing::warn!(
                                    event = "ipc.snapshot_oversized",
                                    error = %error,
                                    events = batch.events.len(),
                                    snapshot_bytes = candidate.snapshot_json.len(),
                                    "снапшот не влез в кадр, переходим на поштучную отправку"
                                );
                                let payload = serde_json::to_vec(&serde_json::json!({
                                    "after_sequence": request.after_sequence,
                                    "last_sequence": last_sequence,
                                    "events": batch.events.len(),
                                    "snapshot_bytes": candidate.snapshot_json.len(),
                                    "reason": "snapshot_too_large",
                                }))
                                .map_err(|error| FrameError::Io(error.to_string()))?;
                                self.write_response(writer, "replay.snapshot_skipped", payload)
                                    .await?;
                                None
                            }
                        }
                    } else {
                        None
                    };
                    if let Some(snapshot) = snapshot {
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: last_sequence,
                            task_id: String::new(),
                            event_type: "replay.full_snapshot".into(),
                            payload: Vec::new(),
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: Some(generated::event_envelope::Event::FullSnapshot(snapshot)),
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    } else {
                        for record in batch.events {
                            let event = generated::EventEnvelope {
                                protocol: Some(protocol()),
                                sequence_id: record.sequence_id as u64,
                                task_id: record.task_id,
                                event_type: record.event_type,
                                payload: record.payload,
                                core_instance_id: self.core_instance_id.clone(),
                                session_epoch: self.session_epoch,
                                event: None,
                            };
                            transport::write_frame(writer, &event.encode_to_vec()).await?;
                        }
                    }
                    // Каждый resync отдаёт не больше `limit` событий за раз. Без
                    // этого флага оболочка узнавала об оставшемся хвосте истории
                    // только по случайному разрыву sequence в живом потоке — и
                    // гонялась за ним кругами, так и не догоняя (план про «нет
                    // связи», возникавшую после больших сессий).
                    let latest_after_batch = self.latest_sequence().await;
                    let end_payload = serde_json::to_vec(&serde_json::json!({
                        "more_available": last_sequence < latest_after_batch,
                        "latest_sequence": latest_after_batch,
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let end = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "resync.end".into(),
                        payload: end_payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &end.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::ReplayEvents(replay)) => {
                    if self.stale_generation(&command) {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            replay.after_sequence,
                            None,
                            latest,
                            REPLAY_GAP_REASON_STALE_GENERATION,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    let batch = self
                        .journal
                        .replay_bounded(replay.after_sequence as i64, 1_000)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let mut last_sequence = batch.last_sequence as u64;
                    if batch.gap_detected {
                        let latest = self.latest_sequence().await;
                        let gap = self.replay_gap_envelope(
                            replay.after_sequence,
                            batch.first_available_sequence.map(|value| value as u64),
                            latest,
                            REPLAY_GAP_REASON_SEQUENCE_RETENTION_EXCEEDED,
                        );
                        transport::write_frame(writer, &gap.encode_to_vec()).await?;
                    }
                    for record in batch.events {
                        last_sequence = record.sequence_id as u64;
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: record.sequence_id as u64,
                            task_id: record.task_id,
                            event_type: record.event_type,
                            payload: record.payload,
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: None,
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    }
                    let end = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "replay.end".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &end.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::SelectModel(request)) => {
                    // Bounded: a model identifier is a short single-line token.
                    let model = request.model.trim();
                    if model.len() > 128 || model.contains(char::is_whitespace) {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(&serde_json::json!({ "reason": "invalid_model" }))
                                .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    }
                    let Some(route) = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                    else {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(
                                &serde_json::json!({ "reason": "provider_not_configured" }),
                            )
                            .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    };
                    let available = evohime_model_gateway::fetch_model_catalog(route)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    if !available.iter().any(|entry| entry.id == model) {
                        self.write_response(
                            writer,
                            "model.select.rejected",
                            serde_json::to_vec(
                                &serde_json::json!({ "reason": "model_not_returned_by_provider" }),
                            )
                            .unwrap_or_default(),
                        )
                        .await?;
                        return Ok(());
                    }
                    self.selected_model.set(model);
                    let payload = match serde_json::to_vec(&self.current_model_config()) {
                        Ok(payload) => payload,
                        Err(error) => {
                            tracing::warn!(%error, "model config serialization failed");
                            b"null".to_vec()
                        }
                    };
                    self.write_response(writer, "model.config", payload).await?;
                }
                Some(generated::command_envelope::Command::ModelConfig(_)) => {
                    let payload = match serde_json::to_vec(&self.current_model_config()) {
                        Ok(payload) => payload,
                        Err(error) => {
                            tracing::warn!(%error, "model config serialization failed");
                            b"null".to_vec()
                        }
                    };
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "model.config".into(),
                        payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::ModelCatalog(request)) => {
                    let mode = if request.mode == "paid" {
                        "paid"
                    } else {
                        "free"
                    };
                    let provider = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                        .map(|route| route.provider.as_str().to_string())
                        .unwrap_or_else(|| "unknown".into());
                    let result = self
                        .gateway_config
                        .as_ref()
                        .and_then(|config| config.routes.get(&config.default_route))
                        .map(|route| async move {
                            evohime_model_gateway::fetch_model_catalog(route)
                                .await
                                .map(|entries| {
                                    entries
                                        .into_iter()
                                        .filter(|entry| {
                                            if mode == "free" {
                                                entry.id.ends_with(":free")
                                            } else {
                                                !entry.id.ends_with(":free")
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                })
                        });
                    let (entries, error) = match result {
                        Some(request) => request.await,
                        None => Err(evohime_model_gateway::providers::ProviderError::Config(
                            "provider is not configured".into(),
                        )),
                    }
                    .map_or_else(
                        |error| (Vec::new(), Some(error.to_string())),
                        |entries| (entries, None),
                    );
                    // Лимиты переживают сессию: планировщик контекста и ревью
                    // должны знать окно модели ещё до первого обновления каталога,
                    // а неудачный запрос не должен стирать то, что уже известно.
                    self.remember_model_limits(&provider, &entries).await;
                    let models = entries
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect::<Vec<_>>();
                    let limits = entries
                        .iter()
                        .map(|entry| {
                            (
                                entry.id.clone(),
                                serde_json::json!({
                                    "context": entry.context_tokens,
                                    "maxOutput": entry.max_output_tokens,
                                }),
                            )
                        })
                        .collect::<serde_json::Map<_, _>>();
                    let payload = serde_json::json!({
                        "mode": mode,
                        "models": models,
                        "limits": limits,
                        "error": error,
                    });
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: 0,
                        task_id: String::new(),
                        event_type: "model.catalog".into(),
                        payload: serde_json::to_vec(&payload).unwrap_or_default(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                Some(generated::command_envelope::Command::StartPlanReview(request)) => {
                    self.start_plan_review(request, writer).await?;
                }
                Some(generated::command_envelope::Command::PlanArtifactCreate(request))
                | Some(generated::command_envelope::Command::PlanArtifactRead(request))
                | Some(generated::command_envelope::Command::PlanArtifactAction(request)) => {
                    let operation = if request.operation.is_empty() {
                        "read".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self.dispatch_plan_artifact(operation, request).await?;
                    self.write_response(writer, "plan_artifact.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::WorkspaceStateCheckpoint(request)) => {
                    let operation = if request.operation.is_empty() {
                        "compare".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_workspace_state_checkpoint(operation, request)
                        .await?;
                    self.write_response(writer, "workspace_state_checkpoint.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::IncrementalChangeProtocol(request)) => {
                    let operation = if request.operation.is_empty() {
                        "status".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_incremental_change_protocol(operation, request)
                        .await?;
                    self.write_response(writer, "incremental_change_protocol.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RevisionSafeWorkspaceFiles(request)) => {
                    let operation = if request.operation.is_empty() {
                        "read".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_revision_safe_workspace_files(operation, request)
                        .await?;
                    self.write_response(writer, "revision_safe_workspace_files.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::TaskWorktreeIsolation(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_task_worktree_isolation(operation, request)
                        .await?;
                    self.write_response(writer, "task_worktree_isolation.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::TeamResourceBudget(request)) => {
                    let operation = if request.operation.is_empty() {
                        "validate_policy".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_team_resource_budget(operation, request)
                        .await?;
                    self.write_response(writer, "team_resource_budget.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ComposableTerminationConditions(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "validate_policy".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_composable_termination_conditions(operation, request)
                        .await?;
                    self.write_response(writer, "composable_termination_conditions.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::WorkspaceBootstrapManifest(request)) => {
                    let operation = if request.operation.is_empty() {
                        "validate".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_workspace_bootstrap_manifest(operation, request)
                        .await?;
                    self.write_response(writer, "workspace_bootstrap_manifest.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::TeamCoordinationPolicies(request)) => {
                    let operation = if request.operation.is_empty() {
                        "validate_policy".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_team_coordination_policies(operation, request)
                        .await?;
                    self.write_response(writer, "team_coordination_policies.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::TypedAgentHandoffContract(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_typed_agent_handoff_contract(operation, request)
                        .await?;
                    self.write_response(writer, "typed_agent_handoff_contract.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SchemaDrivenAgentConfiguration(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "get_schema".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_schema_driven_agent_configuration(operation, request)
                        .await?;
                    self.write_response(writer, "schema_driven_agent_configuration.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ExperienceReplayLibrary(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_experience_replay_library(operation, request)
                        .await?;
                    self.write_response(writer, "experience_replay_library.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RuntimeInterventionPipeline(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "evaluate".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_runtime_intervention_pipeline(operation, request)
                        .await?;
                    self.write_response(writer, "runtime_intervention_pipeline.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CodeDiagnosticsFeedbackLoop(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "status".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_code_diagnostics_feedback_loop(operation, request)
                        .await?;
                    self.write_response(writer, "code_diagnostics_feedback_loop.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::WorkflowOptimizationLab(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get_run".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_workflow_optimization_lab(operation, request)
                        .await?;
                    self.write_response(writer, "workflow_optimization_lab.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CoreTopicSubscriptionEventBus(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "subscribe".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_core_topic_subscription_event_bus(operation, request)
                        .await?;
                    self.write_response(writer, "core_topic_subscription_event_bus.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::DependencyAwareTaskGraph(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_dependency_aware_task_graph(operation, request)
                        .await?;
                    self.write_response(writer, "dependency_aware_task_graph.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::DeclarativeAgentComponentRegistry(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_declarative_agent_component_registry(operation, request)
                        .await?;
                    self.write_response(
                        writer,
                        "declarative_agent_component_registry.result",
                        result,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::TypedContextReferences(request)) => {
                    let operation = if request.operation.is_empty() {
                        "resolve".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_typed_context_references(operation, request)
                        .await?;
                    self.write_response(writer, "typed_context_references.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SafeUiExtensionFramework(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_safe_ui_extension_framework(operation, request)
                        .await?;
                    self.write_response(writer, "safe_ui_extension_framework.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CapabilityWorkbench(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_capability_workbench(operation, request)
                        .await?;
                    self.write_response(writer, "capability_workbench.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::TeamCoordinator(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self.dispatch_team_coordinator(operation, request).await?;
                    self.write_response(writer, "team_coordinator.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ProjectInstructionStack(request)) => {
                    let operation = if request.operation.is_empty() {
                        "discover".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_project_instruction_stack(operation, request)
                        .await?;
                    self.write_response(writer, "project_instruction_stack.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::WorkspaceSets(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self.dispatch_workspace_sets(operation, request).await?;
                    self.write_response(writer, "workspace_sets.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::KnowledgeSourceRegistry(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_knowledge_source_registry(operation, request)
                        .await?;
                    self.write_response(writer, "knowledge_source_registry.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::AgentGitChangeSets(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get_candidate".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_agent_git_change_sets(operation, request)
                        .await?;
                    self.write_response(writer, "agent_git_change_sets.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ArchitectEditorPipeline(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_architect_editor_pipeline(operation, request)
                        .await?;
                    self.write_response(writer, "architect_editor_pipeline.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::EventVisualizerRegistry(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_event_visualizer_registry(operation, request)
                        .await?;
                    self.write_response(writer, "event_visualizer_registry.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReasoningOperatorLibrary(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_reasoning_operator_library(operation, request)
                        .await?;
                    self.write_response(writer, "reasoning_operator_library.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::OutputGuardrailPipeline(request)) => {
                    let operation = if request.operation.is_empty() {
                        "evaluate".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_output_guardrail_pipeline(operation, request)
                        .await?;
                    self.write_response(writer, "output_guardrail_pipeline.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CustomizationInventory(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_customization_inventory(operation, request)
                        .await?;
                    self.write_response(writer, "customization_inventory.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::StandingApprovalProfiles(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_standing_approval_profiles(operation, request)
                        .await?;
                    self.write_response(writer, "standing_approval_profiles.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ApprovalPolicyProfiles(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_approval_policy_profiles(operation, request)
                        .await?;
                    self.write_response(writer, "approval_policy_profiles.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CheckpointForking(request)) => {
                    let operation = if request.operation.is_empty() {
                        "fork".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self.dispatch_checkpoint_forking(operation, request).await?;
                    self.write_response(writer, "checkpoint_forking.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PrivacyTelemetryGovernance(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_privacy_telemetry_governance(operation, request)
                        .await?;
                    self.write_response(writer, "privacy_telemetry_governance.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ConversationBridgeAdapters(request)) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_conversation_bridge_adapters(operation, request)
                        .await?;
                    self.write_response(writer, "conversation_bridge_adapters.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::MemoryViewsAndAdaptiveRecall(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_memory_views_and_adaptive_recall(operation, request)
                        .await?;
                    self.write_response(writer, "memory_views_and_adaptive_recall.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ModelEditProtocolRegistry(request)) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_model_edit_protocol_registry(operation, request)
                        .await?;
                    self.write_response(writer, "model_edit_protocol_registry.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RemoteConversationChannels(request)) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_remote_conversation_channels(operation, request)
                        .await?;
                    self.write_response(writer, "remote_conversation_channels.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PromptCachePlanner(request)) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_prompt_cache_planner(operation, request)
                        .await?;
                    self.write_response(writer, "prompt_cache_planner.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::DeclarativeRuntimeComponents(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_declarative_runtime_components(operation, request)
                        .await?;
                    self.write_response(writer, "declarative_runtime_components.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GuidedCalibrationSessions(request)) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_guided_calibration_sessions(operation, request)
                        .await?;
                    self.write_response(writer, "guided_calibration_sessions.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ExtensionConformanceKit(request)) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_extension_conformance_kit(operation, request)
                        .await?;
                    self.write_response(writer, "extension_conformance_kit.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::DurableRemoteTaskBridge(request)) => {
                    let operation = if request.operation.is_empty() {
                        "status".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_durable_remote_task_bridge(operation, request)
                        .await?;
                    self.write_response(writer, "durable_remote_task_bridge.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::MessageInterventionPolicies(
                    request,
                )) => {
                    let operation = if request.operation.is_empty() {
                        "evaluate".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_message_intervention_policies(operation, request)
                        .await?;
                    self.write_response(writer, "message_intervention_policies.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::BatchInvocationRuntime(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_batch_invocation_runtime(operation, request)
                        .await?;
                    self.write_response(writer, "batch_invocation_runtime.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PolicyAwareToolResultCache(request)) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_policy_aware_tool_result_cache(operation, request)
                        .await?;
                    self.write_response(writer, "policy_aware_tool_result_cache.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CodeAnchoredIntentMarkers(request)) => {
                    let operation = if request.operation.is_empty() {
                        "scan".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_code_anchored_intent_markers(operation, request)
                        .await?;
                    self.write_response(writer, "code_anchored_intent_markers.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ModelPurposeRouting(request)) => {
                    let operation = if request.operation.is_empty() {
                        "get".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_model_purpose_routing(operation, request)
                        .await?;
                    self.write_response(writer, "model_purpose_routing.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::LocalModelRuntimeManager(request)) => {
                    let operation = if request.operation.is_empty() {
                        "inspect".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_local_model_runtime_manager(operation, request)
                        .await?;
                    self.write_response(writer, "local_model_runtime_manager.result", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ArchitectureSnapshot(request)) => {
                    let operation = if request.operation.is_empty() {
                        "current".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let result = self
                        .dispatch_architecture_snapshot(operation, request)
                        .await?;
                    self.write_response(writer, "architecture_snapshot.result", result)
                        .await?;
                }
                Some(
                    generated::command_envelope::Command::PersistentAgentOrganizationRegistry(
                        request,
                    ),
                ) => {
                    let operation = if request.operation.is_empty() {
                        "list".to_owned()
                    } else {
                        request.operation.clone()
                    };
                    let agent_id = request.agent_id.clone();
                    let request_id = request.request_id.clone();
                    let result = self
                        .dispatch_persistent_agent_organization_registry(operation, request)
                        .await?;
                    self.write_persistent_agent_organization_registry_response(
                        writer,
                        &request_id,
                        &agent_id,
                        result,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::StopPlanReview(request)) => {
                    let cancelled = self
                        .review_tasks
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "review.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListPlanReviews(request)) => {
                    let limit = (request.limit as usize).clamp(1, 50);
                    let results = self.review_results.lock().await;
                    let mut items: Vec<_> = results.values().cloned().collect();
                    drop(results);
                    if let Ok(events) = self.journal.review_history(limit).await {
                        for event in events {
                            if let Some(result) = review_result_from_event(&event.payload) {
                                if !items.iter().any(|item| item.review_id == result.review_id) {
                                    items.push(result);
                                }
                            }
                        }
                    }
                    items.sort_by(|left, right| left.review_id.cmp(&right.review_id));
                    items.truncate(limit);
                    self.write_response(
                        writer,
                        "review.list",
                        serde_json::to_vec(&serde_json::json!({ "reviews": items }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ClearPlanReviewHistory(_)) => {
                    // Running reviews keep their own state; only what the history
                    // lists is dropped, and the marker is what listing reads.
                    self.review_results.lock().await.clear();
                    let marker_id = format!("review-history-{}", self.latest_sequence().await);
                    // Recorded directly rather than published: the shell lists again
                    // as soon as this response arrives, and a marker still travelling
                    // through the coordinator's broadcast would not be in the journal
                    // yet, so that listing would return the reviews just cleared.
                    // Nothing subscribes to the marker, so no push is lost.
                    let _ = self
                        .journal
                        .record(&CoreEvent::ReviewHistoryCleared { marker_id })
                        .await;
                    self.write_response(
                        writer,
                        "review.historyCleared",
                        serde_json::to_vec(&serde_json::json!({ "cleared": true }))
                            .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    self.write_response(
                        writer,
                        "review.result",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "result": result,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExportPlanReview(request)) => {
                    let mut result = self
                        .review_results
                        .lock()
                        .await
                        .get(&request.review_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) = self.journal.task_history(&request.review_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| review_result_from_event(&event.payload));
                        }
                    }
                    let result = result.ok_or_else(|| FrameError::Io("review not found".into()))?;
                    let destination = std::path::PathBuf::from(&request.destination_path);
                    if destination.extension().and_then(|value| value.to_str()) != Some("md") {
                        return Err(
                            FrameError::Io("review export must be a Markdown file".into()).into(),
                        );
                    }
                    let content = if request.include_reviewers {
                        serde_json::to_string_pretty(&result).unwrap_or_default()
                    } else {
                        result.final_markdown.clone()
                    };
                    tokio::fs::write(&destination, content)
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(
                        writer,
                        "review.exported",
                        serde_json::to_vec(&serde_json::json!({
                            "review_id": request.review_id,
                            "destination_path": request.destination_path,
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::RevisePlan(request)) => {
                    self.revise_plan(request, writer).await?;
                }
                Some(generated::command_envelope::Command::StopRevision(request)) => {
                    let cancelled = self
                        .revision_tasks
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if let Some(ref token) = cancelled {
                        token.cancel();
                    }
                    self.write_response(
                        writer,
                        "revision.stop.accepted",
                        serde_json::to_vec(&serde_json::json!({
                            "revision_id": request.revision_id,
                            "accepted": cancelled.is_some(),
                        }))
                        .unwrap_or_default(),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SaveRevisedPlan(request)) => {
                    // Правка переживает перезапуск ядра: обновление Евы перезапускает
                    // Core, а нажать «сохранить» пользователь может и после этого.
                    let mut result = self
                        .revision_results
                        .lock()
                        .await
                        .get(&request.revision_id)
                        .cloned();
                    if result.is_none() {
                        if let Ok(events) =
                            self.journal.task_history(&request.revision_id, 10).await
                        {
                            result = events
                                .iter()
                                .rev()
                                .find_map(|event| revision_result_from_event(&event.payload));
                        }
                    }
                    // Отказ отвечает событием, а не ошибкой кадра: ошибка кадра рвёт
                    // соединение с оболочкой, и опечатка в имени файла выглядела бы
                    // как падение ядра.
                    let failure = match &result {
                        None => Some("правка не найдена: запусти её заново".to_string()),
                        Some(_)
                            if std::path::Path::new(&request.destination_path)
                                .extension()
                                .and_then(|value| value.to_str())
                                != Some("md") =>
                        {
                            Some("сохранить план можно только в файл .md".to_string())
                        }
                        Some(_) => None,
                    };
                    let failure = match (failure, result) {
                        (Some(reason), _) => Some(reason),
                        (None, Some(result)) => {
                            tokio::fs::write(&request.destination_path, &result.revised_markdown)
                                .await
                                .err()
                                .map(|error| error.to_string())
                        }
                        (None, None) => Some("правка не найдена: запусти её заново".to_string()),
                    };
                    match failure {
                        Some(error) => {
                            self.write_response(
                                writer,
                                "plan.save_failed",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                    "error": error,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                        None => {
                            self.write_response(
                                writer,
                                "plan.saved",
                                serde_json::to_vec(&serde_json::json!({
                                    "revision_id": request.revision_id,
                                    "destination_path": request.destination_path,
                                }))
                                .unwrap_or_default(),
                            )
                            .await?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::PermissionMode(request)) => {
                    if let Some(tools) = &self.tools {
                        let mode = match request.mode.as_str() {
                            "full" => PermissionMode::Allow,
                            "read_only" => PermissionMode::Deny,
                            _ => PermissionMode::Ask,
                        };
                        tools.permissions().set_all_modes(mode).await;
                        if request.mode == "read_only" {
                            tools
                                .permissions()
                                .set_mode(Permission::FilesystemRead, PermissionMode::Allow)
                                .await;
                            tools
                                .permissions()
                                .set_mode(Permission::GitRead, PermissionMode::Allow)
                                .await;
                        }
                    }
                }
                Some(generated::command_envelope::Command::CreateProject(request)) => {
                    let result = self
                        .dispatch_create_project(client_id, request_id, command_hash, request)
                        .await?;
                    self.write_response(writer, "project.created", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateTask(request)) => {
                    let item = WorkItemRecord {
                        id: request.task_id,
                        project_id: request.project_id,
                        parent_id: (!request.parent_id.is_empty()).then_some(request.parent_id),
                        title: request.title,
                        description: request.description,
                        source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                        acceptance_criteria: request.acceptance_criteria,
                        non_goals: request.non_goals,
                        status: if request.status.is_empty() {
                            "backlog".into()
                        } else {
                            request.status
                        },
                        priority: request.priority,
                        estimate: (request.estimate != 0).then_some(request.estimate),
                        complexity: (!request.complexity.is_empty()).then_some(request.complexity),
                        attempt_count: 0,
                        version: 1,
                    };
                    let result = self
                        .dispatch_create_task(client_id, request_id, command_hash, item)
                        .await?;
                    self.write_response(writer, "task.created", result).await?;
                }
                Some(generated::command_envelope::Command::UpdateTaskStatus(request)) => {
                    let result = self
                        .dispatch_update_status(
                            client_id,
                            request_id,
                            command_hash,
                            request.task_id,
                            request.expected_version,
                            request.status,
                        )
                        .await?;
                    self.write_response(writer, "task.status_updated", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::AddTaskEdge(request)) => {
                    let result = self
                        .dispatch_add_edge(
                            client_id,
                            request_id,
                            command_hash,
                            request.from_task_id,
                            request.to_task_id,
                            request.kind,
                        )
                        .await?;
                    self.write_response(writer, "task.edge_added", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetTaskGraph(request)) => {
                    let result = self.dispatch_get_task_graph(request.project_id).await?;
                    self.write_response(writer, "task.graph", result).await?;
                }
                Some(generated::command_envelope::Command::NextReadyTask(request)) => {
                    let result = self.dispatch_next_ready_task(request.project_id).await?;
                    self.write_response(writer, "task.next_ready", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ImportPrd(request)) => {
                    let result = self
                        .dispatch_import_prd(client_id, request_id, command_hash, request)
                        .await?;
                    self.write_response(writer, "prd.imported", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskHistory(request)) => {
                    let result = self
                        .dispatch_get_task_history(request.task_id, request.limit as usize)
                        .await?;
                    self.write_response(writer, "task.history", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskContext(request)) => {
                    let result = self
                        .dispatch_get_task_context(
                            request.project_id,
                            request.task_id,
                            request.max_chars as usize,
                        )
                        .await?;
                    self.write_response(writer, "task.context", result).await?;
                }
                Some(generated::command_envelope::Command::GetTaskPlanSpec(request)) => {
                    let result = self
                        .dispatch_get_task_plan_spec(
                            request.project_id,
                            request.task_id,
                            request.max_chars as usize,
                        )
                        .await?;
                    self.write_response(writer, "task.plan_spec", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ApplyApprovedBuild(request)) => {
                    let result = self
                        .dispatch_apply_approved_build(
                            request.project_id,
                            request.run_id,
                            request.task_id,
                            request.approved_build_json,
                        )
                        .await?;
                    self.write_response(writer, "build.applied", result).await?;
                }
                Some(generated::command_envelope::Command::PrepareBuild(request)) => {
                    let result = self
                        .dispatch_prepare_build(request.project_id, request.proposal_json)
                        .await?;
                    self.write_response(writer, "build.prepared", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetTaskSnapshot(request)) => {
                    let result = self
                        .dispatch_get_task_snapshot(request.project_id, request.task_id)
                        .await?;
                    self.write_response(writer, "task.snapshot", result).await?;
                }
                Some(generated::command_envelope::Command::RestoreTaskSnapshot(request)) => {
                    let result = self
                        .dispatch_restore_task_snapshot(
                            request.project_id,
                            request.task_id,
                            request.snapshot_id,
                        )
                        .await?;
                    self.write_response(writer, "snapshot.restored", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetBuildPolicy(request)) => {
                    let result = self.dispatch_get_build_policy(request.project_id).await?;
                    self.write_response(writer, "build.policy", result).await?;
                }
                Some(generated::command_envelope::Command::SaveBuildPolicy(request)) => {
                    let result = self
                        .dispatch_save_build_policy(
                            request.project_id,
                            request.policy_json,
                            request.expected_version,
                        )
                        .await?;
                    self.write_response(writer, "build.policy.saved", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::StartTask(start)) => {
                    let has_conversation = !start.conversation_id.is_empty();
                    let has_client_message = !start.client_message_id.is_empty();
                    if has_conversation != has_client_message {
                        self.write_conversation_event_log_response(
                            writer,
                            conversation_event_log_error(
                                "accept",
                                &start.conversation_id,
                                "invalid_argument",
                            ),
                        )
                        .await?;
                        return Ok(());
                    }
                    let mut should_dispatch = true;
                    if has_conversation {
                        let workspace_id =
                            crate::task_memory::project_scope_id(&start.workspace_path);
                        let accepted = self
                            .journal
                            .accept_conversation_message(
                                &start.conversation_id,
                                &workspace_id,
                                &start.task_id,
                                &start.client_message_id,
                                &start.prompt,
                            )
                            .await;
                        let (acceptance, sequence) = match accepted {
                            Ok(value) => value,
                            Err(StorageError::ConversationEventLog(
                                evohime_local_storage::conversation_event_log_store::ConversationStoreError::IdempotencyConflict,
                            )) => {
                                self.write_conversation_event_log_response(
                                    writer,
                                    conversation_event_log_error(
                                        "accept",
                                        &start.conversation_id,
                                        "idempotency_conflict",
                                    ),
                                )
                                .await?;
                                return Ok(());
                            }
                            Err(error) => {
                                self.write_conversation_event_log_response(
                                    writer,
                                    conversation_event_log_error(
                                        "accept",
                                        &start.conversation_id,
                                        &conversation_accept_error_code(&error),
                                    ),
                                )
                                .await?;
                                return Ok(());
                            }
                        };
                        if let Some(coordinator) = &self.coordinator {
                            coordinator.notify_journalled(sequence.max(0) as u64);
                        }
                        should_dispatch = self
                            .journal
                            .claim_conversation_dispatch(
                                &start.conversation_id,
                                &start.client_message_id,
                            )
                            .await
                            .unwrap_or(false);
                        if !should_dispatch && acceptance.dispatch_state == "dispatching" {
                            self.write_conversation_event_log_response(
                                writer,
                                conversation_event_log_error(
                                    "accept",
                                    &start.conversation_id,
                                    "dispatch_unknown",
                                ),
                            )
                            .await?;
                            return Ok(());
                        }
                    }
                    if should_dispatch {
                        if let Some(coordinator) = &self.coordinator {
                            let dispatched = coordinator
                                .dispatch(CoreCommand::StartTask {
                                    task_id: start.task_id,
                                    prompt: start.prompt,
                                    workspace_root: (!start.workspace_path.is_empty())
                                        .then(|| std::path::PathBuf::from(start.workspace_path)),
                                    preferred_route_hint: match start.preferred_route_hint.as_str()
                                    {
                                        "local" | "cloud" => Some(start.preferred_route_hint),
                                        "codex_cli" if start.execution_kind == "coding" => {
                                            Some("codex_cli".into())
                                        }
                                        _ => None,
                                    },
                                })
                                .await;
                            if has_conversation {
                                self.journal
                                    .finish_conversation_dispatch(
                                        &start.conversation_id,
                                        &start.client_message_id,
                                        dispatched.is_ok(),
                                    )
                                    .await
                                    .map_err(|error| FrameError::Io(error.to_string()))?;
                            }
                            dispatched.map_err(|error| FrameError::Io(error.to_string()))?;
                        } else if has_conversation {
                            self.journal
                                .finish_conversation_dispatch(
                                    &start.conversation_id,
                                    &start.client_message_id,
                                    false,
                                )
                                .await
                                .map_err(|error| FrameError::Io(error.to_string()))?;
                        }
                    }
                }
                Some(generated::command_envelope::Command::GetTaskCheckpoint(request)) => {
                    let projection = self.dispatch_get_task_checkpoint(request).await;
                    self.write_task_checkpoint_projection(writer, projection)
                        .await?;
                }
                Some(generated::command_envelope::Command::ResolveTaskCheckpoint(request)) => {
                    let result = self.dispatch_resolve_task_checkpoint(request).await?;
                    self.write_task_checkpoint_action_result(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListSkills(request)) => {
                    self.dispatch_list_skills(request, writer).await?;
                }
                Some(generated::command_envelope::Command::LoadSkill(request)) => {
                    self.dispatch_load_skill(request, writer).await?;
                }
                Some(generated::command_envelope::Command::LoadSkillReference(request)) => {
                    self.dispatch_load_skill_reference(request, writer).await?;
                }
                Some(generated::command_envelope::Command::CreateGoal(request)) => {
                    let result = self.dispatch_create_goal(request, &command_hash).await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::GetGoal(request)) => {
                    let projection = self.dispatch_get_goal(request).await;
                    self.write_goal_projection(writer, projection).await?;
                }
                Some(generated::command_envelope::Command::ListGoals(request)) => {
                    let projection = self.dispatch_list_goals(request).await;
                    self.write_goal_list_projection(writer, projection).await?;
                }
                Some(generated::command_envelope::Command::PauseGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Paused,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::ResumeGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Active,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::CancelGoal(request)) => {
                    let result = self
                        .dispatch_goal_transition(
                            request,
                            crate::goal::GoalStatus::Cancelled,
                            &command_hash,
                        )
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::UpdateGoal(request)) => {
                    let result = self.dispatch_update_goal(request, &command_hash).await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::VerifyGoalCriterion(request)) => {
                    let result = self
                        .dispatch_verify_goal_criterion(request, &command_hash)
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::LinkGoalReference(request)) => {
                    let result = self
                        .dispatch_link_goal_reference(request, &command_hash)
                        .await;
                    self.write_goal_action_result(writer, result).await?;
                }
                Some(generated::command_envelope::Command::SaveContinuationPolicy(request)) => {
                    let result = self
                        .dispatch_save_continuation_policy(
                            request,
                            &client_id,
                            &request_id,
                            &command_hash,
                        )
                        .await;
                    self.write_response(
                        writer,
                        "continuation.policy",
                        result.unwrap_or_else(error_response_payload),
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::StartContinuationRun(request)) => {
                    let result = self.dispatch_start_continuation(request).await;
                    let payload = result.unwrap_or_else(error_response_payload);
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_projection(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.run", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::GetContinuationRun(request)) => {
                    let result = self.dispatch_get_continuation(request).await;
                    let payload = result.unwrap_or_else(error_response_payload);
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_projection(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.run", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::StopContinuation(request)) => {
                    let result = self.dispatch_stop_continuation(request).await;
                    let payload = result.unwrap_or_else(error_response_payload);
                    if serde_json::from_slice::<serde_json::Value>(&payload)
                        .ok()
                        .and_then(|v| v.get("run_id").cloned())
                        .is_some()
                    {
                        self.write_continuation_action(writer, payload).await?;
                    } else {
                        self.write_response(writer, "continuation.action", payload)
                            .await?;
                    }
                }
                Some(generated::command_envelope::Command::ListRetainedChildren(request)) => {
                    let (reply, response) = oneshot::channel();
                    let parent_id = client_id.clone();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::ListRetainedChildren {
                            parent_id,
                            now_ms: crate::task_memory::now_millis(),
                            limit: request.limit,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.list", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetRetainedChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::GetRetainedChild {
                            parent_id: client_id.clone(),
                            child_id: request.child_id,
                            now_ms: crate::task_memory::now_millis(),
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::RetainChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let now_ms = crate::task_memory::now_millis();
                    let child = crate::retained_child::RetainedChildV1 {
                        version: 1,
                        child_id: request.child_id,
                        parent_id: client_id.clone(),
                        family_root_id: if request.family_root_id.is_empty() {
                            client_id.clone()
                        } else {
                            request.family_root_id
                        },
                        role: request.role,
                        stable_name: (!request.stable_name.is_empty())
                            .then_some(request.stable_name),
                        lifecycle: crate::retained_child::RetainedLifecycle::Active,
                        revision: request.revision,
                        active_session_id: None,
                        grant_snapshot_hash: request.grant_snapshot_hash,
                        context_scope_hash: request.context_scope_hash,
                        workspace_state_ref: (!request.workspace_state_ref.is_empty())
                            .then_some(request.workspace_state_ref),
                        last_report_ref: (!request.last_report_ref.is_empty())
                            .then_some(request.last_report_ref),
                        retained_until_ms: if request.retained_until_ms == 0 {
                            now_ms.saturating_add(crate::retained_child::DEFAULT_TTL_MS)
                        } else {
                            request.retained_until_ms
                        },
                        created_at_ms: if request.created_at_ms == 0 {
                            now_ms
                        } else {
                            request.created_at_ms
                        },
                        last_active_at_ms: if request.last_active_at_ms == 0 {
                            now_ms
                        } else {
                            request.last_active_at_ms
                        },
                        registry_version: request.expected_registry_version.saturating_add(1),
                    };
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::RetainChild {
                            child,
                            now_ms,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.retained", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::SendChildFollowUp(request)) => {
                    let (reply, response) = oneshot::channel();
                    let mode = match request.mode.as_str() {
                        "auto" => crate::retained_child::FollowUpMode::Auto,
                        "follow_up" | "" => crate::retained_child::FollowUpMode::FollowUp,
                        "steer" => crate::retained_child::FollowUpMode::Steer,
                        _ => {
                            self.write_response(
                                writer,
                                "retained_child.follow_up",
                                b"{\"error_code\":\"invalid_scope\"}".to_vec(),
                            )
                            .await?;
                            return Ok(());
                        }
                    };
                    let follow = crate::retained_child::ChildFollowUpRequestV1 {
                        version: 1,
                        idempotency_key: request.idempotency_key,
                        parent_id: client_id.clone(),
                        child_id: request.child_id,
                        family_root_id: client_id.clone(),
                        parent_sequence: 0,
                        expected_child_revision: request.expected_child_revision,
                        instruction: request.instruction,
                        context_refs: request.context_refs,
                        requested_grants: request.requested_grants,
                        budget_json: request.budget_json,
                        mode,
                        correlation_id: request.correlation_id,
                    };
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::SendChildFollowUp {
                            request: follow,
                            now_ms: crate::task_memory::now_millis(),
                            busy: false,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.follow_up", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::DeleteRetainedChild(request)) => {
                    let (reply, response) = oneshot::channel();
                    let coordinator = self
                        .coordinator
                        .as_ref()
                        .ok_or_else(|| FrameError::Io("coordinator_unavailable".into()))?;
                    coordinator
                        .dispatch(CoreCommand::DeleteRetainedChild {
                            parent_id: client_id.clone(),
                            child_id: request.child_id,
                            expected_registry_version: request.expected_registry_version,
                            reply,
                        })
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?;
                    let payload = response
                        .await
                        .map_err(|e| FrameError::Io(e.to_string()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "retained_child.delete", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateAnalysisKernel(request)) => {
                    let projection = self.dispatch_create_analysis_kernel(request).await;
                    write_analysis_kernel_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetAnalysisKernel(request)) => {
                    let projection = self.dispatch_get_analysis_kernel(request).await;
                    write_analysis_kernel_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExecuteAnalysisKernel(request)) => {
                    let result = self.dispatch_execute_analysis_kernel(request).await;
                    write_analysis_kernel_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ResetAnalysisKernel(request)) => {
                    let result = self.dispatch_reset_analysis_kernel(request).await;
                    write_analysis_kernel_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListRefinementCandidates(request)) => {
                    let projection = self.dispatch_list_refinement_candidates(request).await;
                    write_refinement_list_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetRefinementCandidate(request)) => {
                    let projection = self.dispatch_get_refinement_candidate(request).await;
                    write_refinement_projection(
                        writer,
                        projection,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::RefinementAction(request)) => {
                    let result = self.dispatch_refinement_action(request).await;
                    write_refinement_action_result(
                        writer,
                        result,
                        &self.core_instance_id,
                        self.session_epoch,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::PreviewWorkflowPackage(request)) => {
                    let result = crate::workflow_package::preview_from_json(
                        &request.graph_json,
                        request.name,
                        request.description,
                        request.portable_argument_keys,
                        &request.credential_slots_json,
                        request.created_at,
                    )
                    .map(|preview| serde_json::json!({
                        "status": "previewed",
                        "package_hash": preview.package_hash,
                        "stripped_fields": preview.stripped_fields,
                        "package": preview.package,
                    }))
                    .map_err(|error| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "preview", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::ExportWorkflowPackage(request)) => {
                    let result = crate::workflow_package::preview_from_json(
                        &request.graph_json,
                        request.name,
                        request.description,
                        request.portable_argument_keys,
                        &request.credential_slots_json,
                        request.created_at,
                    )
                    .and_then(|preview| {
                        crate::workflow_package::write_package(
                            std::path::Path::new(&request.destination_path),
                            &preview.package,
                        )?;
                        Ok(serde_json::json!({"status":"exported","package_hash":preview.package_hash,"stripped_fields":preview.stripped_fields}))
                    })
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "export", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::CommitWorkflowPackage(request)) => {
                    let result = async {
                        let package = crate::workflow_package::parse_bounded(&request.package_json)?;
                        let database = self.journal.database().lock().await;
                        crate::workflow_package::commit_import(
                            &database,
                            std::path::Path::new(&request.source_path),
                            &package,
                            &request.idempotency_key,
                            now_ms(),
                        )
                    }
                    .await
                    .map(|record| serde_json::json!({"status":"committed","import_id":record.import_id,"local_workflow_id":record.local_workflow_id,"package_hash":record.package_hash}))
                    .map_err(|error: crate::workflow_package::WorkflowPackageError| serde_json::json!({"status":"rejected","error_code":error.to_string()}));
                    let payload = match result {
                        Ok(value) | Err(value) => serde_json::to_vec(&value)?,
                    };
                    self.write_package_response(writer, "commit", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::RebindWorkflowPackage(request)) => {
                    let result = async {
                        let package =
                            crate::workflow_package::parse_bounded(&request.package_json)?;
                        let database = self.journal.database().lock().await;
                        crate::workflow_package::persist_rebind(
                            &database,
                            &package,
                            &request.slot_id,
                            &request.local_credential_reference,
                            now_ms(),
                        )
                    }
                    .await;
                    let payload = match result {
                        Ok(value) => serde_json::to_vec(
                            &serde_json::json!({"status":"rebound","binding":value}),
                        )?,
                        Err(error) => serde_json::to_vec(
                            &serde_json::json!({"status":"rejected","error_code":error.to_string()}),
                        )?,
                    };
                    self.write_package_response(writer, "rebind", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::StopTask(stop)) => {
                    if let Some(coordinator) = &self.coordinator {
                        coordinator
                            .dispatch(CoreCommand::StopTask {
                                task_id: stop.task_id,
                            })
                            .await
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                }
                Some(generated::command_envelope::Command::ListWorkspace(request)) => {
                    let listing = crate::workspace::list_directory(
                        request.workspace_path,
                        if request.relative_path.is_empty() {
                            "."
                        } else {
                            &request.relative_path
                        },
                        if request.max_entries == 0 {
                            crate::workspace::MAX_LIST_ENTRIES
                        } else {
                            request.max_entries as usize
                        },
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = serde_json::to_vec(&listing)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "workspace.list", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::PauseContinuation(request)) => {
                    let payload = self
                        .dispatch_transition_continuation(
                            request.run_id,
                            request.idempotency_key,
                            request.expected_state,
                            "paused",
                            "pause",
                        )
                        .await
                        .unwrap_or_else(error_response_payload);
                    self.write_continuation_action(writer, payload).await?;
                }
                Some(generated::command_envelope::Command::ResumeContinuation(request)) => {
                    let run = self.dispatch_resume_continuation(request).await;
                    let payload = match run {
                        Ok(run) => {
                            if let Some(coordinator) = &self.coordinator {
                                if let (Some(prompt), Some(workspace_path)) =
                                    (run.prompt.clone(), run.workspace_path.clone())
                                {
                                    let _ = coordinator
                                        .dispatch(CoreCommand::StartTask {
                                            task_id: run.task_id.clone(),
                                            prompt,
                                            workspace_root: Some(workspace_path.into()),
                                            preferred_route_hint: None,
                                        })
                                        .await;
                                }
                            }
                            serde_json::to_vec(&serde_json::json!({
                                "run_id": run.run_id,
                                "action": "resume",
                                "applied": true,
                                "error_code": ""
                            }))
                            .unwrap_or_default()
                        }
                        Err(error) => error_response_payload(error),
                    };
                    self.write_continuation_action(writer, payload).await?;
                }
                Some(generated::command_envelope::Command::ReadWorkspaceFile(request)) => {
                    let content = crate::workspace::read_text_file(
                        request.workspace_path,
                        &request.relative_path,
                        if request.max_bytes == 0 {
                            crate::workspace::MAX_READ_BYTES
                        } else {
                            request.max_bytes as usize
                        },
                    )
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "path": request.relative_path,
                        "content": content,
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "workspace.file", payload)
                        .await?;
                }
                Some(generated::command_envelope::Command::GitStatus(request)) => {
                    let payload = self
                        .dispatch_git_read(
                            request.workspace_path,
                            "git.status",
                            serde_json::Value::Null,
                            request.max_bytes,
                        )
                        .await?;
                    self.write_response(writer, "git.status", payload).await?;
                }
                Some(generated::command_envelope::Command::GitDiff(request)) => {
                    let input = if request.relative_path.is_empty() {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!({"path": request.relative_path})
                    };
                    let payload = self
                        .dispatch_git_read(
                            request.workspace_path,
                            "git.diff",
                            input,
                            request.max_bytes,
                        )
                        .await?;
                    self.write_response(writer, "git.diff", payload).await?;
                }
                Some(generated::command_envelope::Command::TerminalExecute(request)) => {
                    self.dispatch_terminal_execute(request, writer).await?;
                }
                Some(generated::command_envelope::Command::RunDoctor(request)) => {
                    let result = self
                        .dispatch_run_doctor(
                            request.project_id,
                            request.detail_level,
                            command.protocol,
                        )
                        .await?;
                    self.write_response(writer, "doctor.report", result).await?;
                }
                Some(generated::command_envelope::Command::CreateDiagnosticsSnapshot(request)) => {
                    let result = self
                        .dispatch_create_diagnostics_snapshot(request, command.protocol)
                        .await?;
                    self.write_response(writer, "diagnostics.snapshot", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ExportDoctorLogs(request)) => {
                    let result = self
                        .dispatch_export_doctor_logs(request.destination_path)
                        .await?;
                    self.write_response(writer, "doctor.export.completed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateDatabaseBackup(request)) => {
                    self.dispatch_create_database_backup(
                        request_id,
                        request.destination_path,
                        writer,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::PrepareDatabaseRestore(request)) => {
                    let result = self
                        .dispatch_prepare_database_restore(request_id, request.backup_path)
                        .await?;
                    self.write_response(writer, "storage.restore.preview", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RestoreDatabase(request)) => {
                    self.dispatch_restore_database(
                        request_id,
                        request.backup_path,
                        request.approval_id,
                        writer,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::CancelDatabaseOperation(request)) => {
                    let result = self
                        .dispatch_cancel_database_operation(request.operation_id)
                        .await?;
                    self.write_response(writer, "storage.cancel.requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SaveResearchEvidence(request)) => {
                    let result = self.dispatch_save_research_evidence(request).await?;
                    self.write_response(writer, "research.evidence.saved", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListResearchEvidence(request)) => {
                    let result = self
                        .dispatch_list_research_evidence(request.work_item_id)
                        .await?;
                    self.write_response(writer, "research.evidence.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RunResearchFetch(request)) => {
                    let result = self.dispatch_run_research_fetch(request).await?;
                    self.write_response(writer, "research.fetch.completed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CreateMemory(request)) => {
                    let result = self.dispatch_create_memory(request).await?;
                    self.write_response(writer, "memory.created", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListMemory(request)) => {
                    let result = self.dispatch_list_memory(request).await?;
                    self.write_response(writer, "memory.list", result).await?;
                }
                Some(generated::command_envelope::Command::SearchMemory(request)) => {
                    let result = self.dispatch_search_memory(request).await?;
                    self.write_response(writer, "memory.search", result).await?;
                }
                Some(generated::command_envelope::Command::ArchiveMemory(request)) => {
                    let result = self.dispatch_archive_memory(request).await?;
                    self.write_response(writer, "memory.archived", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetMemory(request)) => {
                    let result = self.dispatch_get_memory(request).await?;
                    self.write_response(writer, "memory.record", result).await?;
                }
                Some(generated::command_envelope::Command::ListMemoryPending(request)) => {
                    let result = self.dispatch_list_memory_pending(request).await?;
                    self.write_response(writer, "memory.pending", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetMemoryConflicts(request)) => {
                    let result = self.dispatch_get_memory_conflicts(request).await?;
                    self.write_response(writer, "memory.conflicts", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ConfirmMemory(request)) => {
                    let result = self.dispatch_confirm_memory(request).await?;
                    self.write_response(writer, "memory.confirmed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RejectMemory(request)) => {
                    let result = self.dispatch_reject_memory(request).await?;
                    self.write_response(writer, "memory.rejected", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReviseMemoryCandidate(request)) => {
                    let result = self.dispatch_revise_memory_candidate(request).await?;
                    self.write_response(writer, "memory.revised", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SupersedeMemory(request)) => {
                    let result = self.dispatch_supersede_memory(request).await?;
                    self.write_response(writer, "memory.superseded", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ForgetMemory(request)) => {
                    let result = self.dispatch_forget_memory(request).await?;
                    self.write_response(writer, "memory.forgotten", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::InstallCapability(request)) => {
                    let result = self.dispatch_install_capability(request).await?;
                    self.write_response(writer, "capability.installed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListCapabilities(request)) => {
                    let result = self.dispatch_list_capabilities(request).await?;
                    self.write_response(writer, "capability.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::MatchCapabilities(request)) => {
                    let result = self.dispatch_match_capabilities(request).await?;
                    self.write_response(writer, "capability.match", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RemoveCapability(request)) => {
                    let result = self.dispatch_remove_capability(request).await?;
                    self.write_response(writer, "capability.removed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListToolkits(request)) => {
                    let result = self.dispatch_list_toolkits(request).await?;
                    self.write_response(writer, "toolkit.list", result).await?;
                }
                Some(generated::command_envelope::Command::EnableToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "rollback",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.enabled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::DisableToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "disabled",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.disabled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RollbackToolkit(request)) => {
                    let result = self
                        .dispatch_toolkit_status(
                            request.toolkit_id,
                            request.version,
                            request.reason,
                            "enabled",
                        )
                        .await?;
                    self.write_response(writer, "toolkit.rolled_back", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetCapabilitySelection(request)) => {
                    let result = self.dispatch_get_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PinCapabilitySelection(request)) => {
                    let result = self.dispatch_pin_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection.pinned", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReplaceCapabilitySelection(request)) => {
                    let result = self.dispatch_replace_capability_selection(request).await?;
                    self.write_response(writer, "capability.selection.replaced", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RequestChildHandoff(request)) => {
                    let result = self.dispatch_request_child_handoff(request).await?;
                    self.write_response(writer, "child.handoff.requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListChildHandoffs(request)) => {
                    let result = self.dispatch_list_child_handoffs(request).await?;
                    self.write_response(writer, "child.handoff.list", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitChildRequest(request)) => {
                    let result = self.dispatch_submit_child_request(request).await?;
                    self.write_response(writer, "child.request.submitted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitChildReport(request)) => {
                    let result = self.dispatch_submit_child_report(request).await?;
                    self.write_response(writer, "child.report.accepted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubmitFeedback(request)) => {
                    let result = self.dispatch_submit_feedback(request).await?;
                    self.write_response(writer, "feedback.submitted", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::IndexWorkspace(request)) => {
                    let result = self.dispatch_index_workspace(request, false).await?;
                    self.write_response(writer, "workspace.indexed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::RebuildIndex(request)) => {
                    let result = self.dispatch_rebuild_index(request).await?;
                    self.write_response(writer, "workspace.indexed", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SearchWorkspaceKnowledge(request)) => {
                    let result = self.dispatch_search_workspace_knowledge(request).await?;
                    self.write_response(writer, "workspace.knowledge", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetIndexStatus(request)) => {
                    let result = self.dispatch_get_index_status(request).await?;
                    self.write_response(writer, "workspace.index_status", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelWorkspaceIndex(request)) => {
                    let result = self.dispatch_cancel_workspace_index(request).await?;
                    self.write_response(writer, "workspace.index_cancelled", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetContextLedger(request)) => {
                    let result = self.dispatch_get_context_ledger(request).await?;
                    self.write_response(writer, "context.ledger", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListTaskScratchpad(request)) => {
                    let result = self.dispatch_list_task_scratchpad(request).await?;
                    self.write_response(writer, "context.scratchpad", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ClearTaskScratchpad(request)) => {
                    let result = self.dispatch_clear_task_scratchpad(request).await?;
                    self.write_response(writer, "context.scratchpad_cleared", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SummarizeContextNow(request)) => {
                    let result = self.dispatch_summarize_context_now(request).await?;
                    self.write_response(writer, "context.summarize_requested", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::PinContextItem(request)) => {
                    let result = self.dispatch_pin_context_item(request).await?;
                    self.write_response(writer, "context.item_pinned", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ReadContextArtifact(request)) => {
                    let result = self.dispatch_read_context_artifact(request).await?;
                    self.write_response(writer, "context.artifact", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListFeedback(request)) => {
                    let result = self.dispatch_list_feedback(request).await?;
                    self.write_response(writer, "feedback.list", result).await?;
                }
                Some(generated::command_envelope::Command::ResolveApproval(resolve)) => {
                    // Cancellation is a terminal rejection at the existing
                    // approval boundary; the immutable approval binding remains
                    // owned by Core and old clients keep the same semantics.
                    let granted = resolve.granted && !resolve.cancel;
                    let approval_id = uuid::Uuid::parse_str(&resolve.approval_id)
                        .map_err(|error| FrameError::Io(format!("invalid approval id: {error}")))?;
                    if let Some(tools) = &self.tools {
                        let _ = tools.permissions().resolve(approval_id, granted).await;
                    }
                    if let Some(approvals) = &self.approvals {
                        let _ = approvals.resolve(approval_id, granted).await;
                    }
                    if !granted {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(
                            database.connection_mut(),
                            &signer,
                        ) {
                            let _ = runtime.deny_approval(approval_id);
                        }
                    }
                    let _ = self
                        .journal
                        .record_audit(
                            &resolve.approval_id,
                            "approval.decision",
                            serde_json::to_vec(&serde_json::json!({
                                "approval_id": resolve.approval_id,
                                "granted": granted,
                                "cancelled": resolve.cancel,
                                "idempotency_key": resolve.idempotency_key,
                                "rejection_reason": resolve.rejection_reason,
                            }))
                            .unwrap_or_default()
                            .as_slice(),
                        )
                        .await;
                    self.record_ledger_approval_decision(&resolve.approval_id, granted)
                        .await;
                    // Узел workflow подтверждается той же командой, что и
                    // инструмент: отдельного пути approval у workflow нет. Если
                    // идентификатор принадлежит узлу, запуск продолжается сам —
                    // иначе он остался бы ждать уже принятого решения.
                    if self
                        .workflow_approvals
                        .resolve(&resolve.approval_id, granted)
                    {
                        if let Some(run_id) = self.workflow_approvals.run_for(&resolve.approval_id)
                        {
                            let workspace = self.journal.workflow_run_workspace(&run_id).await;
                            let _ = self.spawn_workflow_drive(run_id, workspace).await;
                        }
                    }
                }
                Some(generated::command_envelope::Command::ResolveRoutingDecision(resolve)) => {
                    let coordinator = self.coordinator.as_ref().ok_or_else(|| {
                        FrameError::Io("core command queue is not configured".into())
                    })?;
                    let (reply, response) = oneshot::channel();
                    coordinator
                        .dispatch(CoreCommand::ResolveRoutingDecision {
                            trace_id: resolve.trace_id,
                            approve: resolve.approve,
                            reply,
                        })
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let result = response
                        .await
                        .map_err(|_| FrameError::Io("routing decision response dropped".into()))?
                        .map_err(FrameError::Io)?;
                    self.write_response(writer, "routing.decision", result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SetAmbientListening(request)) => {
                    let result = self.dispatch_set_ambient_listening(request).await;
                    self.write_response(writer, "ambient.listening", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAmbientStatus(_)) => {
                    let result = self.dispatch_get_ambient_status().await;
                    self.write_response(writer, "ambient.status", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListAmbientEpisodes(request)) => {
                    let result = self.dispatch_list_ambient_episodes(request).await;
                    self.write_response(writer, "ambient.episodes", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAmbientEpisode(request)) => {
                    let result = self.dispatch_get_ambient_episode(request).await;
                    self.write_response(writer, "ambient.episode", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::DeleteAmbientTranscripts(request)) => {
                    let result = self.dispatch_delete_ambient_transcripts(request).await;
                    self.write_response(writer, "ambient.deleted", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ForgetAmbientWindow(request)) => {
                    let result = self.dispatch_forget_ambient_window(request).await;
                    self.write_response(writer, "ambient.forgotten", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAmbientPolicy(_)) => {
                    let result = self.dispatch_get_ambient_policy().await;
                    self.write_response(writer, "ambient.policy", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::SaveAmbientPolicy(request)) => {
                    let result = self.dispatch_save_ambient_policy(request).await;
                    self.write_response(
                        writer,
                        "ambient.policy_saved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ResolveAmbientProposal(request)) => {
                    let result = self.dispatch_resolve_ambient_proposal(request).await;
                    // Имя ответа отличается от имени журнальной записи
                    // `ambient.proposal`: renderer подписан на неё как на событие,
                    // и ответ на команду не должен подменять собой список карточек.
                    self.write_response(
                        writer,
                        "ambient.proposal_resolved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListAmbientProposals(request)) => {
                    let result = self.dispatch_list_ambient_proposals(request).await;
                    self.write_response(writer, "ambient.proposals", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListVoiceCommands(request)) => {
                    let result = self.dispatch_list_voice_commands(request);
                    self.write_response(
                        writer,
                        "ambient.voice_commands",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ResolveVoiceCommand(request)) => {
                    let result = self.dispatch_resolve_voice_command(request).await;
                    self.write_response(
                        writer,
                        "ambient.voice_command_resolved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListWorkflowTemplates(_)) => {
                    let result = self.dispatch_list_workflow_templates();
                    self.write_response(writer, "workflow.templates", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetWorkflowDefinition(request)) => {
                    let result = self.dispatch_workflow_definition(request);
                    self.write_response(
                        writer,
                        "workflow.definition",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::StartWorkflow(request)) => {
                    let result = self.dispatch_start_workflow(request).await;
                    self.write_response(writer, "workflow.started", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetWorkflowRun(request)) => {
                    let result = self.dispatch_workflow_run(request).await;
                    self.write_response(writer, "workflow.run", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelWorkflow(request)) => {
                    let result = self.dispatch_cancel_workflow(request).await;
                    self.write_response(writer, "workflow.cancelled", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListWorkflowEvents(request)) => {
                    let result = self.dispatch_list_workflow_events(request).await;
                    self.write_response(writer, "workflow.events", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::VisualWorkflowBuilder(request)) => {
                    let result = self.dispatch_visual_workflow_builder(request).await;
                    self.write_response(
                        writer,
                        "workflow_builder.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ConversationalWorkflowComposer(
                    request,
                )) => {
                    let result = self
                        .dispatch_conversational_workflow_composer(request)
                        .await;
                    self.write_response(
                        writer,
                        "workflow_composer.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::IntegrationProviderSdkCatalog(
                    request,
                ))
                | Some(generated::command_envelope::Command::IntegrationProviderSdkAction(
                    request,
                )) => {
                    let result = self.dispatch_integration_provider_sdk(request);
                    self.write_response(
                        writer,
                        "integration_provider_sdk.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::EventTriggerRuntimeList(request))
                | Some(generated::command_envelope::Command::EventTriggerRuntimeAction(request)) => {
                    let result = self.dispatch_event_trigger_runtime(request);
                    self.write_response(
                        writer,
                        "event_trigger_runtime.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::InvocationPresetList(request))
                | Some(generated::command_envelope::Command::InvocationPresetAction(request)) => {
                    let result = self.dispatch_invocation_preset(request).await;
                    self.write_invocation_preset_response(
                        writer,
                        "invocation_preset.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::BenchmarkMatrixList(request))
                | Some(generated::command_envelope::Command::BenchmarkMatrixAction(request)) => {
                    let result = self.dispatch_benchmark_matrix(request);
                    self.write_response(
                        writer,
                        "benchmark_matrix.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::AgentMiddlewarePipelineList(
                    request,
                ))
                | Some(generated::command_envelope::Command::AgentMiddlewarePipelineAction(
                    request,
                )) => {
                    let result = self.dispatch_agent_middleware_pipeline(request);
                    self.write_agent_middleware_pipeline_response(
                        writer,
                        "agent_middleware_pipeline.result",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::StructuredResponse(request)) => {
                    let result = self.dispatch_structured_response(request);
                    self.write_structured_response_response(writer, serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::SensitiveDataGuardrails(request)) => {
                    let result = self.dispatch_sensitive_data_guardrails(request);
                    self.write_sensitive_data_guardrails_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExecutionPolicyProfiles(request)) => {
                    let result = self.dispatch_execution_policy_profiles(request);
                    self.write_execution_policy_profiles_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ModelResiliencePolicy(request)) => {
                    let result = self.dispatch_model_resilience_policy(request);
                    self.write_model_resilience_policy_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExecutionBackendRegistry(request)) => {
                    let result = self.dispatch_execution_backend_registry(request).await;
                    self.write_execution_backend_registry_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ToolSimulationRuntime(request)) => {
                    let result = self.dispatch_tool_simulation_runtime(request).await;
                    self.write_tool_simulation_runtime_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ExternalCodingAgentAdapterList(
                    request,
                ))
                | Some(generated::command_envelope::Command::ExternalCodingAgentAdapterAction(
                    request,
                )) => {
                    let result = self.dispatch_external_coding_agent_adapter(request).await;
                    self.write_external_coding_agent_adapter_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::AgentRoleProfilesList(request))
                | Some(generated::command_envelope::Command::AgentRoleProfilesAction(request)) => {
                    let result = self.dispatch_agent_role_profiles(request).await;
                    self.write_agent_role_profiles_response(writer, serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationSchedules(request)) => {
                    let result = self.dispatch_list_automation_schedules(request).await;
                    self.write_response(
                        writer,
                        "automation.schedules",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::TeamSopProtocolsList(request))
                | Some(generated::command_envelope::Command::TeamSopProtocolsAction(request)) => {
                    let result = self.dispatch_team_sop_protocols(request).await;
                    self.write_team_sop_protocols_response(writer, serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::HumanWorkItems(request)) => {
                    let result = self.dispatch_human_work_items(request).await;
                    self.write_human_work_items_response(writer, serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::AgenticBrowserSession(request)) => {
                    let result = self.dispatch_agentic_browser_session(request).await;
                    self.write_agentic_browser_session_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ArtifactHandoffRegistry(request)) => {
                    let result = self.dispatch_artifact_handoff_registry(request).await;
                    self.write_artifact_handoff_registry_response(
                        writer,
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::GetConversationEvents(request)) => {
                    let result = self
                        .dispatch_conversation_event_log(request, "history")
                        .await;
                    self.write_conversation_event_log_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SubscribeConversationEvents(
                    request,
                )) => {
                    let result = self
                        .dispatch_conversation_event_log(request, "subscribed")
                        .await;
                    self.write_conversation_event_log_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetConversationWorkbench(request)) => {
                    let result = self.dispatch_conversation_workbench(request).await;
                    self.write_conversation_workbench_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CausalCollaborationBus(request)) => {
                    let result = self.dispatch_causal_collaboration_bus(request).await;
                    self.write_causal_collaboration_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::CausalCollaborationBusSubscribe(
                    request,
                )) => {
                    let result = self.dispatch_causal_collaboration_subscribe(request).await;
                    self.write_causal_collaboration_response(writer, result)
                        .await?;
                }
                Some(generated::command_envelope::Command::SaveAutomationSchedule(request)) => {
                    let result = self.dispatch_save_automation_schedule(request).await;
                    self.write_response(
                        writer,
                        "automation.schedule_saved",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::TriggerAutomation(request)) => {
                    let result = self.dispatch_trigger_automation(request).await;
                    self.write_response(
                        writer,
                        "automation.triggered",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationRuns(request)) => {
                    let result = self.dispatch_list_automation_runs(request).await;
                    self.write_response(writer, "automation.runs", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::GetAutomationRun(request)) => {
                    let result = self.dispatch_get_automation_run(request).await;
                    self.write_response(writer, "automation.run", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::ListAutomationEvents(request)) => {
                    let result = self.dispatch_list_automation_events(request).await;
                    self.write_response(writer, "automation.events", serde_json::to_vec(&result)?)
                        .await?;
                }
                Some(generated::command_envelope::Command::CancelAutomationRun(request)) => {
                    let result = self.dispatch_cancel_automation_run(request).await;
                    self.write_response(
                        writer,
                        "automation.cancelled",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                Some(generated::command_envelope::Command::SetAutomationScheduleEnabled(
                    request,
                )) => {
                    let result = self.dispatch_set_automation_schedule_enabled(request).await;
                    self.write_response(
                        writer,
                        "automation.schedule_enabled",
                        serde_json::to_vec(&result)?,
                    )
                    .await?;
                }
                None => {}
            }
            Ok(())
        })
    }

    // ------------------------------------------------------------------
    // Постоянное слушание (план 04.5).
    //
    // Девять команд ходят прямо через мост, а не через очередь задач: им
    // нужны журнал, разрешения и реестр состояния, и ни одна из них не
    // запускает агента. Ответ уходит JSON-полезной нагрузкой тем же
    // `write_response`, что и у чеков.
    // ------------------------------------------------------------------

    /// Включение, пауза и смена устройства — одна команда с тремя полями.
    ///
    /// Порядок здесь и есть контракт: сперва проверки, потом сохранение
    /// намерения на диск, потом команда листенеру. Намерение переживает
    /// отсутствие листенера — иначе включение при упавшем процессе молча
    /// пропало бы, а пользователь считал бы, что микрофон включён.
    pub(crate) async fn dispatch_set_ambient_listening(
        &self,
        request: generated::SetAmbientListening,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;

        let data_dir = self.ambient_data_dir();
        let snapshot = self.ambient.snapshot().await;

        // Идентификатор устройства проходит bounded-контракт 04.1: через это
        // поле нельзя протащить фразу.
        if !request.device_id.is_empty()
            && evohime_listener_contract::DeviceId::new(request.device_id.clone()).is_err()
        {
            return listening_result(snapshot.state, Some(Code::InvalidArgument));
        }
        if !request.device_id.is_empty()
            && !snapshot
                .devices
                .iter()
                .any(|device| device.device_id == request.device_id)
        {
            return listening_result(snapshot.state, Some(Code::DeviceDisconnected));
        }

        // Микрофон открывается только явным именованным вызовом: общий режим
        // доступа его не трогает (инвариант 04.1), поэтому и здесь он
        // выставляется отдельно и по имени.
        if let Some(tools) = &self.tools {
            tools
                .permissions()
                .set_mode(
                    Permission::MicrophoneListen,
                    if request.enabled {
                        PermissionMode::Allow
                    } else {
                        PermissionMode::Deny
                    },
                )
                .await;
        }

        let mut policy = crate::ambient::load_policy(&data_dir);
        policy.paused = request.paused;
        if crate::ambient::save_policy(&data_dir, &policy).is_err() {
            return listening_result(snapshot.state, Some(Code::StorageFailed));
        }
        let control = crate::ambient::AmbientControl {
            enabled: request.enabled,
            device_id: if request.device_id.is_empty() {
                snapshot.active_device_id.clone()
            } else {
                request.device_id.clone()
            },
        };
        if crate::ambient::save_control(&data_dir, &control).is_err() {
            return listening_result(snapshot.state, Some(Code::StorageFailed));
        }

        let sent = self
            .ambient
            .send(crate::ambient::ListenerControl::Policy(Box::new((
                policy,
                control.clone(),
            ))))
            .await;
        if let Err(code) = sent {
            // Листенера нет. Намерение уже сохранено и применится при его
            // следующем подключении, но утверждать, что микрофон включён,
            // нельзя.
            self.ambient
                .set_state(
                    ListeningState::EngineUnavailable,
                    ListeningReason::EngineUnavailable,
                    None,
                )
                .await;
            self.publish_ambient_state().await;
            return listening_result(ListeningState::EngineUnavailable, Some(code));
        }

        // Устройство занято другим приложением — включать нечего, и
        // оптимистичное «запускаюсь» здесь было бы враньём.
        if request.enabled && snapshot.state == ListeningState::DeviceConflict {
            return listening_result(snapshot.state, Some(Code::DeviceConflict));
        }

        // Оптимистичное состояние: настоящее приедет от листенера отдельным
        // `ambient.state`, и именно оно останется в реестре.
        let (state, reason) = if !request.enabled {
            (ListeningState::Stopped, ListeningReason::UserRequest)
        } else if request.paused {
            (ListeningState::PausedByUser, ListeningReason::UserRequest)
        } else {
            (ListeningState::Starting, ListeningReason::UserRequest)
        };
        let device_id = control.device_id.clone();
        if self.ambient.set_state(state, reason, Some(device_id)).await {
            self.publish_ambient_state().await;
        }
        let engine_ready = self.ambient.engine_ready().await;
        let failure =
            (request.enabled && !request.paused && !engine_ready).then_some(Code::EngineNotReady);
        listening_result(state, failure)
    }

    /// Публикует текущее состояние реестра одним `ambient.state`.
    pub(crate) async fn publish_ambient_state(&self) {
        let snapshot = self.ambient.snapshot().await;
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::State {
                state: snapshot.state,
                reason: snapshot.reason,
                active_device_id: evohime_listener_contract::DeviceId::new(
                    snapshot.active_device_id,
                )
                .ok(),
            })
            .await;
    }

    pub(crate) async fn dispatch_get_ambient_status(&self) -> serde_json::Value {
        let snapshot = self.ambient.snapshot().await;
        serde_json::json!({
            "state": snapshot.state,
            "reason": snapshot.reason,
            "active_device_id": snapshot.active_device_id,
            "engine_version": snapshot.engine_version,
            "engine_ready": snapshot.engine_ready,
            "devices": snapshot.devices,
            "watching_devices": snapshot.watching_devices,
        })
    }

    /// Список эпизодов. Текста здесь нет: он отдаётся только
    /// `GetAmbientEpisode` и только по явному клику пользователя.
    pub(crate) async fn dispatch_list_ambient_episodes(
        &self,
        request: generated::ListAmbientEpisodes,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            50usize
        } else {
            (request.limit as usize).min(200)
        };
        // Стор отдаёт свежие первыми и не умеет курсора, поэтому окно
        // вырезается здесь: берётся на одну строку больше запрошенного, и
        // лишняя строка и есть ответ на вопрос «есть ли ещё».
        let records = match self.journal.list_ambient_episodes(limit * 4).await {
            Ok(records) => records,
            Err(code) => return serde_json::json!({ "error_code": code.as_str() }),
        };
        let mut rows: Vec<serde_json::Value> = Vec::new();
        let mut skipping = !request.cursor.is_empty();
        let mut next_cursor = String::new();
        for record in records {
            if skipping {
                if record.episode_id == request.cursor {
                    skipping = false;
                }
                continue;
            }
            let started_at_ms = parse_timestamp_ms(&record.started_at);
            if request.since_ms > 0 && started_at_ms < request.since_ms {
                continue;
            }
            if rows.len() == limit {
                next_cursor = record.episode_id;
                break;
            }
            rows.push(serde_json::json!({
                "episode_id": record.episode_id,
                "started_at_ms": started_at_ms,
                "speech_duration_ms": record.speech_ms,
                "utterance_count": record.utterance_count,
                "extraction_state": record.extraction_state.as_str(),
            }));
        }
        serde_json::json!({ "episodes": rows, "next_cursor": next_cursor })
    }

    /// Единственный путь, по которому распознанный текст пересекает границу
    /// IPC. Вызывается только явным раскрытием эпизода в панели.
    pub(crate) async fn dispatch_get_ambient_episode(
        &self,
        request: generated::GetAmbientEpisode,
    ) -> serde_json::Value {
        if request.episode_id.is_empty() {
            return serde_json::json!({
                "error_code": evohime_listener_contract::AmbientErrorCode::InvalidArgument.as_str()
            });
        }
        match self
            .journal
            .list_ambient_utterances(&request.episode_id, 500)
            .await
        {
            Ok(records) => serde_json::json!({
                "episode_id": request.episode_id,
                "utterances": records
                    .into_iter()
                    .map(|record| serde_json::json!({
                        "utterance_id": record.utterance_id,
                        "started_at_ms": parse_timestamp_ms(&record.started_at),
                        "duration_ms": record.duration_ms,
                        "text": record.text,
                        "language": record.language,
                        "redacted": record.redacted,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Err(code) => serde_json::json!({ "error_code": code.as_str() }),
        }
    }

    /// Удаление транскриптов. Без `confirmed` команда отвергается здесь, а не
    /// только модальным окном оболочки: обход UI не должен давать больше прав.
    pub(crate) async fn dispatch_delete_ambient_transcripts(
        &self,
        request: generated::DeleteAmbientTranscripts,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        if !request.confirmed {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::ConfirmationRequired.as_str(),
            });
        }
        let now_ms = crate::task_memory::now_millis();
        let targets: Vec<String> = if request.all {
            match self.journal.list_ambient_episodes(500).await {
                Ok(records) => records.into_iter().map(|r| r.episode_id).collect(),
                Err(code) => {
                    return serde_json::json!({
                        "deleted_count": 0,
                        "error_code": code.as_str(),
                    })
                }
            }
        } else {
            request.episode_ids
        };
        if targets.is_empty() && !request.all {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::InvalidArgument.as_str(),
            });
        }
        let mut deleted = 0u32;
        for episode_id in targets {
            match self
                .journal
                .delete_ambient_episode(&episode_id, now_ms)
                .await
            {
                Ok(deletion) => {
                    deleted = deleted.saturating_add(deletion.utterances_removed as u32)
                }
                Err(code) => {
                    return serde_json::json!({
                        "deleted_count": deleted,
                        "error_code": code.as_str(),
                    })
                }
            }
        }
        let _ = self
            .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Retention {
                deleted_count: deleted,
                trigger: evohime_listener_contract::RetentionTrigger::Manual,
            })
            .await;
        serde_json::json!({ "deleted_count": deleted, "error_code": "" })
    }

    /// «Забыть последние N минут». Окно приходит в миллисекундах и
    /// округляется вверх: половина минуты — это тоже минута, и оставить её
    /// значило бы не забыть то, что просили забыть.
    pub(crate) async fn dispatch_forget_ambient_window(
        &self,
        request: generated::ForgetAmbientWindow,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        if !request.confirmed {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::ConfirmationRequired.as_str(),
            });
        }
        if request.window_ms <= 0 {
            return serde_json::json!({
                "deleted_count": 0,
                "error_code": Code::InvalidArgument.as_str(),
            });
        }
        let minutes = u32::try_from((request.window_ms + 59_999) / 60_000).unwrap_or(u32::MAX);
        let now_ms = crate::task_memory::now_millis();
        match self.journal.forget_ambient_window(minutes, now_ms).await {
            Ok(deletion) => {
                let deleted = deletion.utterances_removed as u32;
                let _ = self
                    .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Retention {
                        deleted_count: deleted,
                        trigger: evohime_listener_contract::RetentionTrigger::ForgetWindow,
                    })
                    .await;
                serde_json::json!({ "deleted_count": deleted, "error_code": "" })
            }
            Err(code) => serde_json::json!({
                "deleted_count": 0,
                "error_code": code.as_str(),
            }),
        }
    }

    pub(crate) fn ambient_policy_json(policy: &evohime_listener_contract::AmbientPolicy) -> serde_json::Value {
        serde_json::json!({
            "quiet_hours": policy
                .quiet_hours
                .iter()
                .map(|window| serde_json::json!({
                    "start_minute": window.start_minute,
                    "end_minute": window.end_minute,
                }))
                .collect::<Vec<_>>(),
            "blocklist_patterns": policy.process_blocklist,
            "window_title_blocklist": policy.window_title_blocklist,
            "retention_days": policy.retention_days,
            "voice_commands": policy.voice_commands,
            "voice_commands_autorun": policy.voice_commands_autorun,
        })
    }

    pub(crate) async fn dispatch_get_ambient_policy(&self) -> serde_json::Value {
        let policy = crate::ambient::load_policy(&self.ambient_data_dir());
        Self::ambient_policy_json(&policy)
    }

    /// Сохранение политики. Невалидная политика не применяется целиком:
    /// частичное применение превратило бы «запретить zoom» в «слушать всё».
    pub(crate) async fn dispatch_save_ambient_policy(
        &self,
        request: generated::SaveAmbientPolicy,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        let Some(incoming) = request.policy else {
            return serde_json::json!({ "applied": false, "error_code": Code::InvalidArgument.as_str() });
        };
        let data_dir = self.ambient_data_dir();
        let previous = crate::ambient::load_policy(&data_dir);
        let mut quiet_hours = Vec::new();
        for window in &incoming.quiet_hours {
            let (Ok(start), Ok(end)) = (
                u32::try_from(window.start_minute),
                u32::try_from(window.end_minute),
            ) else {
                return serde_json::json!({ "applied": false, "error_code": Code::PolicyInvalid.as_str() });
            };
            match evohime_listener_contract::QuietHours::new(start, end) {
                Ok(window) => quiet_hours.push(window),
                Err(error) => {
                    return serde_json::json!({
                        "applied": false,
                        "error_code": error.code().as_str(),
                    })
                }
            }
        }
        let Ok(retention_days) = u32::try_from(incoming.retention_days) else {
            return serde_json::json!({ "applied": false, "error_code": Code::PolicyInvalid.as_str() });
        };
        let policy = evohime_listener_contract::AmbientPolicy {
            // Пауза не редактируется политикой: она принадлежит переключателю
            // и меняется только `SetAmbientListening`.
            paused: previous.paused,
            quiet_hours,
            process_blocklist: incoming.blocklist_patterns,
            window_title_blocklist: incoming.window_title_blocklist,
            retention_days,
            // Поля добавлены позже самого сообщения: клиент, который о них не
            // знает, не шлёт их вовсе, и сохранённое значение остаётся своим.
            // Подстановка `false` вместо этого выключала бы голосовые команды
            // при любом сохранении блок-листа старым клиентом.
            voice_commands: incoming.voice_commands.unwrap_or(previous.voice_commands),
            voice_commands_autorun: incoming
                .voice_commands_autorun
                .unwrap_or(previous.voice_commands_autorun),
        };
        if let Err(error) = policy.validate() {
            return serde_json::json!({
                "applied": false,
                "error_code": error.code().as_str(),
            });
        }
        if crate::ambient::save_policy(&data_dir, &policy).is_err() {
            return serde_json::json!({ "applied": false, "error_code": Code::StorageFailed.as_str() });
        }
        // Сохранённая политика ничего не значит, пока листенер её не получил:
        // недоступный листенер называется своим кодом, а не «применено».
        let control = crate::ambient::load_control(&data_dir);
        match self
            .ambient
            .send(crate::ambient::ListenerControl::Policy(Box::new((
                policy, control,
            ))))
            .await
        {
            Ok(()) => serde_json::json!({ "applied": true, "error_code": "" }),
            Err(code) => serde_json::json!({ "applied": false, "error_code": code.as_str() }),
        }
    }

    // ------------------------------------------------------------------
    // Workflow orchestration (план 06.3).
    //
    // Мост здесь только курьер. Он не планирует граф, не решает порядок и не
    // выполняет узлы: всё это делает `workflow_runtime`, а наружу уходит
    // bounded projection — идентификаторы, состояния и коды. Ни prompt, ни
    // сырой вывод child, ни содержимое контекста через эти команды не
    // проходят.
    // ------------------------------------------------------------------

    /// Собирает runtime под конкретный рабочий каталог.
    ///
    /// Runtime создаётся на команду, а не хранится: состояние запуска durable,
    /// поэтому «живого» объекта между командами не требуется, а рабочий
    /// каталог у каждого запуска свой.
    pub(crate) fn workflow_runtime(&self, workspace_path: &str) -> crate::workflow_runtime::WorkflowRuntime {
        let mut adapter =
            crate::workflow_adapters::CoreNodeAdapter::new(self.journal.clone(), workspace_path);
        if let Some(tools) = &self.tools {
            adapter = adapter.with_tools(Arc::clone(tools));
        }
        crate::workflow_runtime::WorkflowRuntime::new(
            self.journal.clone(),
            Arc::clone(&self.workflow_registry),
            Arc::new(adapter),
            Arc::clone(&self.workflow_approvals)
                as Arc<dyn crate::workflow_runtime::WorkflowApprovalGate>,
            self.core_instance_id.clone(),
        )
    }

    /// Продолжает запуск в фоне. Команда IPC не ждёт выполнения графа:
    /// состояние durable, и оболочка забирает его отдельным `GetWorkflowRun`.
    pub(crate) async fn spawn_workflow_drive(&self, run_id: String, workspace_path: String) -> bool {
        let runtime = self.workflow_runtime(&workspace_path);
        self.background_tasks
            .try_spawn(async move {
                let _ = runtime.drive(&run_id).await;
            })
            .await
    }

    pub(crate) async fn start_invocation_preset(
        &self,
        preset: crate::invocation_presets::InvocationPreset,
        workspace_path: String,
        idempotency_key: String,
    ) -> Result<String, String> {
        preset.validate().map_err(|error| error.to_string())?;
        let Some(template) = crate::workflow_templates::template(&preset.workflow_id) else {
            return Err("unknown_workflow".into());
        };
        if template.version != preset.workflow_version {
            return Err("needs_migration".into());
        }
        let inputs = preset
            .input_values
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.clone(), value.to_string()))
                    .ok_or_else(|| format!("non_string_input:{key}"))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        let graph = template
            .instantiate(&inputs)
            .map_err(|error| error.code().to_string())?;
        if graph.canonical_hash() != preset.workflow_definition_hash {
            return Err("workflow_definition_drift".into());
        }
        let digest = <sha2::Sha256 as sha2::Digest>::digest(
            format!("{}|{}|{}", preset.id, preset.revision, idempotency_key).as_bytes(),
        );
        let run_id = format!("preset-{}", hex_encode(&digest[..16]));
        if self
            .journal
            .workflow_run(&run_id)
            .await
            .ok()
            .flatten()
            .is_some()
        {
            return Ok(run_id);
        }
        let runtime = self.workflow_runtime(&workspace_path);
        let start = crate::workflow_runtime::StartWorkflowRequest {
            run_id: run_id.clone(),
            task_id: run_id.clone(),
            workspace_path: workspace_path.clone(),
            template_id: template.template_id.clone(),
            template_version: template.version,
            inputs,
            graph,
            parent: workflow_parent_capabilities(),
        };
        let started = runtime
            .start(start)
            .await
            .map_err(|error| error.code().to_string())?;
        if !self
            .spawn_workflow_drive(started.clone(), workspace_path)
            .await
        {
            return Err("background task capacity is exhausted".into());
        }
        Ok(started)
    }

    pub(crate) async fn dispatch_list_automation_schedules(
        &self,
        request: generated::ListAutomationSchedules,
    ) -> serde_json::Value {
        let owner_scope = request.owner_scope;
        if owner_scope.is_empty() || owner_scope.len() > crate::automation::MAX_ID_BYTES {
            return serde_json::json!({
                "schedules": [],
                "error_code": "invalid_owner_scope",
            });
        }
        let limit = request.limit.clamp(1, 256);
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_schedules(
            database.connection(),
            &owner_scope,
            limit,
        ) {
            Ok(schedules) => serde_json::json!({
                "schedules": schedules.into_iter().map(|schedule| serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "definition_id": schedule.definition_id,
                    "revision": schedule.revision,
                    "owner_scope": schedule.owner_scope,
                    "hour": schedule.hour,
                    "minute": schedule.minute,
                    "timezone_minutes": schedule.timezone_minutes,
                    "missed_grace_ms": schedule.missed_grace_ms,
                    "enabled": schedule.enabled,
                    "last_slot": schedule.last_slot,
                    "preset_id": schedule.preset_id,
                    "preset_revision": schedule.preset_revision,
                    "preset_content_hash": schedule.preset_content_hash,
                    "workspace_path": schedule.workspace_path,
                })).collect::<Vec<_>>(),
                "error_code": "",
            }),
            Err(error) => serde_json::json!({
                "schedules": [],
                "error_code": error.to_string(),
            }),
        }
    }

    pub(crate) async fn dispatch_save_automation_schedule(
        &self,
        request: generated::SaveAutomationSchedule,
    ) -> serde_json::Value {
        if request.schedule_id.is_empty()
            || request.schedule_id.len() > crate::automation::MAX_ID_BYTES
            || request.definition_id.is_empty()
            || request.owner_scope.is_empty()
            || request.owner_scope.len() > crate::automation::MAX_ID_BYTES
            || request.revision == 0
        {
            return serde_json::json!({
                "saved": false,
                "error_code": "invalid_schedule_identity",
            });
        }
        if crate::automation_scheduler::DailySchedule::new(
            request.hour as u8,
            request.minute as u8,
            request.timezone_minutes,
            request.missed_grace_ms,
        )
        .is_err()
            || request.hour > 23
            || request.minute > 59
        {
            return serde_json::json!({
                "saved": false,
                "error_code": "invalid_schedule_policy",
            });
        }
        let database = self.journal.database().lock().await;
        if !request.preset_id.is_empty() {
            let valid_preset = evohime_local_storage::invocation_presets_store::read_revision(
                database.connection(),
                &request.owner_scope,
                &request.preset_id,
                request.preset_revision,
            )
            .ok()
            .flatten()
            .and_then(|(content, hash, state)| {
                serde_json::from_str::<crate::invocation_presets::InvocationPreset>(&content)
                    .ok()
                    .filter(|preset| {
                        state == "ready"
                            && preset.content_hash == request.preset_content_hash
                            && preset.canonical_content_hash() == hash
                            && preset.revision == request.preset_revision
                    })
            });
            if valid_preset.is_none() {
                return serde_json::json!({"saved":false,"error_code":"invalid_preset_snapshot"});
            }
        }
        let definition = evohime_local_storage::automation_store::get_definition(
            database.connection(),
            &request.definition_id,
            request.revision,
            &request.owner_scope,
        );
        match definition {
            Ok(None) => serde_json::json!({
                "saved": false,
                "error_code": "unknown_definition",
            }),
            Err(error) => serde_json::json!({
                "saved": false,
                "error_code": error.to_string(),
            }),
            Ok(Some(_)) => {
                let previous = evohime_local_storage::automation_store::get_schedule(
                    database.connection(),
                    &request.schedule_id,
                )
                .ok()
                .flatten();
                let record = evohime_local_storage::automation_store::AutomationScheduleRecord {
                    schedule_id: request.schedule_id.clone(),
                    definition_id: request.definition_id.clone(),
                    revision: request.revision,
                    owner_scope: request.owner_scope.clone(),
                    hour: request.hour as u8,
                    minute: request.minute as u8,
                    timezone_minutes: request.timezone_minutes,
                    missed_grace_ms: request.missed_grace_ms,
                    enabled: request.enabled,
                    last_slot: previous.and_then(|previous| {
                        (previous.definition_id == request.definition_id
                            && previous.revision == request.revision
                            && previous.owner_scope == request.owner_scope
                            && previous.preset_id.as_deref()
                                == (!request.preset_id.is_empty())
                                    .then_some(request.preset_id.as_str())
                            && previous.preset_revision
                                == (!request.preset_id.is_empty())
                                    .then_some(request.preset_revision)
                            && previous.preset_content_hash.as_deref()
                                == (!request.preset_content_hash.is_empty())
                                    .then_some(request.preset_content_hash.as_str()))
                        .then_some(previous.last_slot)
                        .flatten()
                    }),
                    preset_id: (!request.preset_id.is_empty()).then_some(request.preset_id.clone()),
                    preset_revision: (!request.preset_id.is_empty())
                        .then_some(request.preset_revision),
                    preset_content_hash: (!request.preset_content_hash.is_empty())
                        .then_some(request.preset_content_hash.clone()),
                    workspace_path: request.workspace_path.clone(),
                };
                match evohime_local_storage::automation_store::upsert_schedule(
                    database.connection(),
                    &record,
                    now_ms(),
                ) {
                    Ok(()) => serde_json::json!({
                        "saved": true,
                        "schedule_id": record.schedule_id,
                        "error_code": "",
                    }),
                    Err(error) => serde_json::json!({
                        "saved": false,
                        "error_code": error.to_string(),
                    }),
                }
            }
        }
    }

    pub(crate) async fn dispatch_trigger_automation(
        &self,
        request: generated::TriggerAutomation,
    ) -> serde_json::Value {
        if request.definition_id.is_empty()
            || request.owner_scope.is_empty()
            || request.trigger_key.is_empty()
            || request.correlation_id.is_empty()
            || request.idempotency_key.is_empty()
            || request.revision == 0
            || request.input_json.len() > crate::automation::MAX_INPUT_BYTES
            || serde_json::from_str::<serde_json::Value>(&request.input_json).is_err()
        {
            return serde_json::json!({ "accepted": false, "run_id": "", "error_code": "invalid_trigger" });
        }
        let mut database = self.journal.database().lock().await;
        let Some(definition) = evohime_local_storage::automation_store::get_definition(
            database.connection(),
            &request.definition_id,
            request.revision,
            &request.owner_scope,
        )
        .ok()
        .flatten() else {
            return serde_json::json!({ "accepted": false, "run_id": "", "error_code": "unknown_definition" });
        };
        let payload_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
            request.input_json.as_bytes(),
        ));
        let run = evohime_local_storage::automation_store::AutomationRunRecord {
            run_id: uuid::Uuid::new_v4().to_string(),
            definition_id: request.definition_id,
            revision: request.revision,
            owner_scope: request.owner_scope,
            idempotency_key: request.idempotency_key,
            payload_hash,
            state: "admitted".into(),
            generation: 1,
            permission_snapshot: "manual".into(),
            approval_snapshot: "manual".into(),
        };
        let now = now_ms();
        match evohime_local_storage::automation_store::admit_run(database.connection(), &run, now) {
            Ok(evohime_local_storage::automation_store::AdmitRunResult::Existing(existing)) => {
                serde_json::json!({ "accepted": true, "run_id": existing.run_id, "state": existing.state, "deduplicated": true, "error_code": "" })
            }
            Ok(evohime_local_storage::automation_store::AdmitRunResult::IdempotencyConflict {
                ..
            }) => {
                serde_json::json!({ "accepted": false, "run_id": "", "deduplicated": false, "error_code": "idempotency_conflict" })
            }
            Ok(evohime_local_storage::automation_store::AdmitRunResult::Inserted) => {
                let payload = serde_json::json!({
                    "definition_hash": definition.definition_hash,
                    "trigger": request.trigger_key,
                    "correlation_id": request.correlation_id,
                });
                let queued = evohime_local_storage::automation_store::transition_run(
                    database.connection_mut(),
                    evohime_local_storage::automation_store::RunTransition {
                        run_id: &run.run_id,
                        from_state: "admitted",
                        to_state: "queued",
                        generation: 1,
                        event_type: "manual_trigger",
                        payload_json: &payload.to_string(),
                        now_ms: now,
                    },
                )
                .unwrap_or(false);
                serde_json::json!({ "accepted": queued, "run_id": run.run_id, "state": if queued { "queued" } else { "admitted" }, "deduplicated": false, "error_code": if queued { "" } else { "transition_failed" } })
            }
            Err(error) => {
                serde_json::json!({ "accepted": false, "run_id": "", "error_code": error.to_string() })
            }
        }
    }

    pub(crate) async fn dispatch_list_automation_runs(
        &self,
        request: generated::ListAutomationRuns,
    ) -> serde_json::Value {
        if request.owner_scope.is_empty() {
            return serde_json::json!({ "runs": [], "error_code": "invalid_owner_scope" });
        }
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_runs(
            database.connection(),
            &request.owner_scope,
            &request.definition_id,
            request.limit.clamp(1, 256),
        ) {
            Ok(runs) => serde_json::json!({ "runs": runs.into_iter().map(|run| serde_json::json!({
                "run_id": run.run_id, "definition_id": run.definition_id, "revision": run.revision,
                "owner_scope": run.owner_scope, "idempotency_key": run.idempotency_key,
                "state": run.state, "generation": run.generation,
            })).collect::<Vec<_>>(), "error_code": "" }),
            Err(error) => serde_json::json!({ "runs": [], "error_code": error.to_string() }),
        }
    }

    pub(crate) async fn dispatch_get_automation_run(
        &self,
        request: generated::GetAutomationRun,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::get_run(
            database.connection(),
            &request.run_id,
        ) {
            Ok(Some(run)) => serde_json::json!({
                "run_id": run.run_id, "definition_id": run.definition_id, "revision": run.revision,
                "owner_scope": run.owner_scope, "state": run.state, "generation": run.generation,
                "error_code": "",
            }),
            Ok(None) => {
                serde_json::json!({ "run_id": request.run_id, "state": "unknown_state", "error_code": "unknown_run" })
            }
            Err(error) => {
                serde_json::json!({ "run_id": request.run_id, "state": "unknown_state", "error_code": error.to_string() })
            }
        }
    }

    pub(crate) async fn dispatch_list_automation_events(
        &self,
        request: generated::ListAutomationEvents,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        match evohime_local_storage::automation_store::list_run_events(
            database.connection(),
            &request.run_id,
            request.after_sequence,
            request.limit.clamp(1, 256) as u32,
        ) {
            Ok(events) => {
                serde_json::json!({ "run_id": request.run_id, "events": events.into_iter().map(|event| serde_json::json!({
                "sequence": event.run_sequence, "event_type": event.event_type, "generation": event.generation,
                "payload": event.payload_json, "created_at_ms": event.created_at_ms,
            })).collect::<Vec<_>>(), "error_code": "" })
            }
            Err(error) => {
                serde_json::json!({ "run_id": request.run_id, "events": [], "error_code": error.to_string() })
            }
        }
    }

    pub(crate) async fn dispatch_cancel_automation_run(
        &self,
        request: generated::CancelAutomationRun,
    ) -> serde_json::Value {
        let mut database = self.journal.database().lock().await;
        let cancelled = evohime_local_storage::automation_store::cancel_run(
            database.connection_mut(),
            &request.run_id,
            now_ms(),
        )
        .unwrap_or(false);
        serde_json::json!({ "run_id": request.run_id, "cancelled": cancelled, "error_code": if cancelled { "" } else { "not_cancellable" } })
    }

    pub(crate) async fn dispatch_set_automation_schedule_enabled(
        &self,
        request: generated::SetAutomationScheduleEnabled,
    ) -> serde_json::Value {
        let database = self.journal.database().lock().await;
        let enabled = evohime_local_storage::automation_store::set_schedule_enabled(
            database.connection(),
            &request.schedule_id,
            request.enabled,
            now_ms(),
        )
        .unwrap_or(false);
        serde_json::json!({ "schedule_id": request.schedule_id, "enabled": request.enabled, "updated": enabled, "error_code": if enabled { "" } else { "unknown_schedule" } })
    }

    /// Polls every enabled schedule once. The compare-and-swap cursor is
    /// advanced before a trigger is admitted, so a second Core generation
    /// cannot publish the same wall-clock slot. The normal automation runtime
    /// consumes the durable admitted run; this method never executes effects.
    pub async fn poll_automation_schedules(&self) {
        let now = now_ms();
        let schedules = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::automation_store::list_enabled_schedules(database.connection())
                .unwrap_or_default()
        };
        for schedule in schedules {
            let Ok(policy) = crate::automation_scheduler::DailySchedule::new(
                schedule.hour,
                schedule.minute,
                schedule.timezone_minutes,
                schedule.missed_grace_ms,
            ) else {
                continue;
            };
            let cursor = crate::automation_scheduler::SchedulerCursor {
                last_slot: schedule.last_slot.clone(),
            };
            let decision =
                match policy.decide(&schedule.definition_id, schedule.revision, &cursor, now) {
                    Ok(decision) => decision,
                    Err(_) => continue,
                };
            let (slot, idempotency_key, missed) = match decision {
                crate::automation_scheduler::SchedulerDecision::NotDue => continue,
                crate::automation_scheduler::SchedulerDecision::Trigger {
                    slot,
                    idempotency_key,
                } => (slot, idempotency_key, false),
                crate::automation_scheduler::SchedulerDecision::Missed {
                    slot,
                    idempotency_key,
                } => (slot, idempotency_key, true),
            };
            let mut database = self.journal.database().lock().await;
            let Some(definition) = evohime_local_storage::automation_store::get_definition(
                database.connection(),
                &schedule.definition_id,
                schedule.revision,
                &schedule.owner_scope,
            )
            .ok()
            .flatten() else {
                // Не сдвигаем cursor: после восстановления definition следующий
                // poll должен повторить попытку, а не потерять слот.
                continue;
            };
            let advanced = evohime_local_storage::automation_store::advance_schedule_slot(
                database.connection(),
                &schedule.schedule_id,
                schedule.last_slot.as_deref(),
                &slot,
                now,
            )
            .unwrap_or(false);
            if !advanced {
                continue;
            }
            if let (Some(preset_id), Some(preset_revision), Some(preset_hash)) = (
                schedule.preset_id.clone(),
                schedule.preset_revision,
                schedule.preset_content_hash.clone(),
            ) {
                let preset = evohime_local_storage::invocation_presets_store::read_revision(
                    database.connection(),
                    &schedule.owner_scope,
                    &preset_id,
                    preset_revision,
                )
                .ok()
                .flatten()
                .and_then(|(content, stored_hash, state)| {
                    if stored_hash != preset_hash || state != "ready" {
                        return None;
                    }
                    serde_json::from_str::<crate::invocation_presets::InvocationPreset>(&content)
                        .ok()
                        .filter(|preset| {
                            preset.content_hash == preset_hash && preset.revision == preset_revision
                        })
                });
                let workspace_path = schedule.workspace_path.clone();
                let schedule_id = schedule.schedule_id.clone();
                let idempotency_key = idempotency_key.clone();
                drop(database);
                match preset {
                    Some(preset) => {
                        let result = self
                            .start_invocation_preset(preset, workspace_path, idempotency_key)
                            .await;
                        if result.is_err() {
                            let database = self.journal.database().lock().await;
                            let payload = serde_json::json!({"schedule_id":schedule_id,"preset_id":preset_id,"revision":preset_revision,"outcome":"blocked","error_code":result.err().unwrap_or_default()});
                            let _ = database.append_event(
                                &schedule_id,
                                "automation.preset_blocked",
                                &serde_json::to_vec(&payload).unwrap_or_default(),
                            );
                        }
                    }
                    None => {
                        let database = self.journal.database().lock().await;
                        let payload = serde_json::json!({"schedule_id":schedule_id,"preset_id":preset_id,"revision":preset_revision,"outcome":"blocked","error_code":"preset_drift_or_rebinding"});
                        let _ = database.append_event(
                            &schedule_id,
                            "automation.preset_blocked",
                            &serde_json::to_vec(&payload).unwrap_or_default(),
                        );
                    }
                }
                continue;
            }
            if missed {
                let payload = serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "definition_id": schedule.definition_id,
                    "revision": schedule.revision,
                    "slot": slot,
                    "idempotency_key": idempotency_key,
                    "reason": "missed_tick",
                });
                let _ = database.append_event(
                    &schedule.schedule_id,
                    "automation.schedule_missed",
                    &serde_json::to_vec(&payload).unwrap_or_default(),
                );
                continue;
            }
            let input_json = "{}".to_string();
            let payload_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
                input_json.as_bytes(),
            ));
            let run = evohime_local_storage::automation_store::AutomationRunRecord {
                run_id: uuid::Uuid::new_v4().to_string(),
                definition_id: schedule.definition_id.clone(),
                revision: schedule.revision,
                owner_scope: schedule.owner_scope.clone(),
                idempotency_key,
                payload_hash,
                state: "admitted".into(),
                generation: 1,
                permission_snapshot: "scheduler".into(),
                approval_snapshot: "scheduler".into(),
            };
            if let Ok(evohime_local_storage::automation_store::AdmitRunResult::Inserted) =
                evohime_local_storage::automation_store::admit_run(database.connection(), &run, now)
            {
                let payload = serde_json::json!({
                    "schedule_id": schedule.schedule_id,
                    "slot": slot,
                    "definition_hash": definition.definition_hash,
                    "trigger": "timer",
                });
                let _ = evohime_local_storage::automation_store::transition_run(
                    database.connection_mut(),
                    evohime_local_storage::automation_store::RunTransition {
                        run_id: &run.run_id,
                        from_state: "admitted",
                        to_state: "queued",
                        generation: 1,
                        event_type: "scheduled",
                        payload_json: &payload.to_string(),
                        now_ms: now,
                    },
                );
            }
        }
    }

    pub(crate) fn dispatch_list_workflow_templates(&self) -> serde_json::Value {
        let templates: Vec<serde_json::Value> = crate::workflow_templates::catalog()
            .into_iter()
            .map(|template| {
                serde_json::json!({
                    "template_id": template.template_id,
                    "version": template.version,
                    "display_name": template.display_name,
                    "description": template.description,
                    "inputs": template
                        .inputs
                        .iter()
                        .map(|input| serde_json::json!({
                            "name": input.name,
                            "title": input.title,
                            "required": input.required,
                            "max_chars": input.max_chars,
                        }))
                        .collect::<Vec<_>>(),
                    "required_capabilities": template.required_capabilities,
                    "schedule_eligibility": template.schedule_eligibility.as_str(),
                    "preview": template.preview,
                    "node_count": template.graph().nodes.len(),
                })
            })
            .collect();
        serde_json::json!({ "templates": templates, "error_code": "" })
    }

    pub(crate) fn dispatch_workflow_definition(
        &self,
        request: generated::GetWorkflowDefinition,
    ) -> serde_json::Value {
        let Some(template) = crate::workflow_templates::template(&request.template_id) else {
            return serde_json::json!({
                "template_id": request.template_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "edges": Vec::<serde_json::Value>::new(),
                "error_code": "unknown_template",
            });
        };
        let graph = template.graph();
        serde_json::json!({
            "template_id": template.template_id,
            "version": template.version,
            "display_name": template.display_name,
            "graph_id": graph.graph_id,
            "graph_version": graph.version,
            "graph_hash": graph.canonical_hash(),
            "schedule_eligibility": template.schedule_eligibility.as_str(),
            "preview": template.preview,
            "nodes": graph
                .nodes
                .iter()
                .map(|node| serde_json::json!({
                    "node_id": node.id,
                    "action_kind": node.node_type.action_kind(),
                    "approval_required": node.execution.approval.required,
                    "block_id": node
                        .block
                        .as_ref()
                        .map(|block| block.block_id.clone())
                        .unwrap_or_default(),
                    "block_version": node
                        .block
                        .as_ref()
                        .map(|block| block.block_version)
                        .unwrap_or_default(),
                }))
                .collect::<Vec<_>>(),
            "edges": graph
                .edges
                .iter()
                .map(|edge| serde_json::json!({
                    "from_node": edge.from_node,
                    "to_node": edge.to_node,
                    "channel": match edge.channel {
                        crate::workflow::EdgeChannel::Failure => "failure",
                        crate::workflow::EdgeChannel::Data => "data",
                    },
                }))
                .collect::<Vec<_>>(),
            "error_code": "",
        })
    }

    pub(crate) async fn dispatch_start_workflow(
        &self,
        request: generated::StartWorkflow,
    ) -> serde_json::Value {
        let Some(template) = crate::workflow_templates::template(&request.template_id) else {
            return workflow_start_failure("unknown_template");
        };
        let inputs: std::collections::BTreeMap<String, String> = request
            .inputs
            .iter()
            .map(|input| (input.name.clone(), input.value.clone()))
            .collect();
        let graph = match template.instantiate(&inputs) {
            Ok(graph) => graph,
            Err(error) => return workflow_start_failure(error.code()),
        };

        // Идемпотентность: тот же ключ даёт тот же `run_id`, поэтому двойной
        // клик возвращает первый запуск, а не создаёт второй.
        let run_id = if request.idempotency_key.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            let digest = <sha2::Sha256 as sha2::Digest>::digest(
                format!("{}|{}", request.template_id, request.idempotency_key).as_bytes(),
            );
            format!("wf-{}", hex_encode(&digest[..16]))
        };
        if let Ok(Some(existing)) = self.journal.workflow_run(&run_id).await {
            return serde_json::json!({
                "run_id": existing.run_id,
                "state": existing.state.as_str(),
                "graph_hash": existing.graph_hash,
                "deduplicated": true,
                "error_code": "",
            });
        }

        let workspace_path = request.workspace_path.clone();
        let runtime = self.workflow_runtime(&workspace_path);
        let start = crate::workflow_runtime::StartWorkflowRequest {
            run_id: run_id.clone(),
            task_id: if request.task_id.trim().is_empty() {
                run_id.clone()
            } else {
                request.task_id.clone()
            },
            workspace_path: workspace_path.clone(),
            template_id: template.template_id.clone(),
            template_version: template.version,
            inputs,
            graph,
            parent: workflow_parent_capabilities(),
        };
        match runtime.start(start).await {
            Ok(run_id) => {
                if !self
                    .spawn_workflow_drive(run_id.clone(), workspace_path)
                    .await
                {
                    return workflow_start_failure("background_task_capacity_exhausted");
                }
                serde_json::json!({
                    "run_id": run_id,
                    "state": "pending",
                    "graph_hash": "",
                    "deduplicated": false,
                    "error_code": "",
                })
            }
            Err(error) => workflow_start_failure(error.code()),
        }
    }

    pub(crate) async fn dispatch_workflow_run(&self, request: generated::GetWorkflowRun) -> serde_json::Value {
        let workspace = self.journal.workflow_run_workspace(&request.run_id).await;
        let runtime = self.workflow_runtime(&workspace);
        match runtime.projection(&request.run_id).await {
            Ok(Some(projection)) => {
                let mut value = serde_json::to_value(&projection).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    object.insert("error_code".into(), serde_json::json!(""));
                }
                value
            }
            Ok(None) => serde_json::json!({
                "run_id": request.run_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "state": "unknown_state",
                "error_code": "unknown_run",
            }),
            Err(error) => serde_json::json!({
                "run_id": request.run_id,
                "nodes": Vec::<serde_json::Value>::new(),
                "state": "unknown_state",
                "error_code": error.code(),
            }),
        }
    }

    pub(crate) async fn dispatch_cancel_workflow(
        &self,
        request: generated::CancelWorkflow,
    ) -> serde_json::Value {
        let now_ms = crate::task_memory::now_millis() as i64;
        let cancelled = self
            .journal
            .request_workflow_cancel(&request.run_id, now_ms)
            .await
            .unwrap_or(false);
        if cancelled {
            let workspace = self.journal.workflow_run_workspace(&request.run_id).await;
            let _ = self.spawn_workflow_drive(request.run_id.clone(), workspace).await;
        }
        serde_json::json!({
            "run_id": request.run_id,
            "cancelled": cancelled,
            "error_code": if cancelled { "" } else { "not_cancellable" },
        })
    }

    pub(crate) async fn dispatch_list_workflow_events(
        &self,
        request: generated::ListWorkflowEvents,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            100usize
        } else {
            (request.limit as usize).min(500)
        };
        match self
            .journal
            .list_workflow_events(&request.run_id, request.after_sequence, limit)
            .await
        {
            Ok(events) => serde_json::json!({
                "run_id": request.run_id,
                "events": events
                    .into_iter()
                    .map(|event| serde_json::json!({
                        "sequence": event.run_sequence,
                        "node_id": event.node_id,
                        "event_type": event.event_type,
                        "payload": event.payload_json,
                        "created_at_ms": event.created_at_ms,
                    }))
                    .collect::<Vec<_>>(),
                "error_code": "",
            }),
            Err(error) => serde_json::json!({
                "run_id": request.run_id,
                "events": Vec::<serde_json::Value>::new(),
                "error_code": error.to_string(),
            }),
        }
    }

    pub(crate) async fn dispatch_visual_workflow_builder(
        &self,
        request: generated::VisualWorkflowBuilderCommand,
    ) -> serde_json::Value {
        if request.operation == "catalog" {
            let blocks = self.workflow_registry.blocks().map(|block| serde_json::json!({"block_id": block.block_id, "block_version": block.block_version, "display_name": block.display_name, "description": block.description, "action_kind": block.action_kind, "inputs": block.inputs, "outputs": block.outputs})).collect::<Vec<_>>();
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"catalog","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"","truncated":false,"blocks":blocks});
        }
        if request.operation == "recover" {
            let database = self.journal.database().lock().await;
            return match evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            ) {
                Ok(Some((revision, _definition, execution_hash, layout_hash))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"recovered","draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":"","error_code":"","truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"missing","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"corrupt","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "inspect" {
            let run_id = String::from_utf8(request.payload.to_vec()).unwrap_or_default();
            let workspace = self.journal.workflow_run_workspace(&run_id).await;
            let runtime = self.workflow_runtime(&workspace);
            return match runtime.projection(&run_id).await {
                Ok(Some(projection)) => {
                    let value = serde_json::to_value(projection).unwrap_or_default();
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"inspected","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"","truncated":false,"projection":value})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"unknown","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_run","truncated":false})
                }
                Err(_error) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"runtime_error","truncated":false})
                }
            };
        }
        if request.operation == "edit" {
            let database = self.journal.database().lock().await;
            let draft = evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            );
            return match draft {
                Ok(Some((revision, definition_json, _, _)))
                    if revision == request.expected_revision =>
                {
                    let parsed = serde_json::from_slice::<
                        crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
                    >(&definition_json);
                    let command = serde_json::from_slice::<
                        crate::visual_workflow_builder::DraftCommand,
                    >(&request.payload);
                    match (parsed, command) {
                        (Ok(mut definition), Ok(command)) => match command
                            .apply(&mut definition)
                            .and_then(|_| self.validate_visual_workflow_definition(&definition))
                        {
                            Ok(()) => {
                                let definition_json =
                                    serde_json::to_vec(&definition).unwrap_or_default();
                                let layout_json =
                                    serde_json::to_vec(&definition.layout).unwrap_or_default();
                                let execution_hash = definition.execution_hash();
                                let layout_hash = definition.layout_hash();
                                match evohime_local_storage::visual_workflow_builder_store::save_draft(database.connection(), evohime_local_storage::visual_workflow_builder_store::SaveDraft { draft_id: &request.draft_id, owner_scope: &request.owner_scope, expected_revision: revision, definition_json: &definition_json, layout_json: &layout_json, execution_hash: &execution_hash, layout_hash: &layout_hash, composer_provenance_json: None, updated_at_ms: crate::task_memory::now_millis() as i64 }) {
                                    Ok(Ok(next_revision)) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"edited","draft_id":request.draft_id,"revision":next_revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":"","error_code":"","truncated":false}),
                                    Ok(Err(code)) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":code,"truncated":false}),
                                    Err(_) => serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false}),
                                }
                            }
                            Err(error) => {
                                serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":error.to_string(),"truncated":false})
                            }
                        },
                        _ => {
                            serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"invalid_command","truncated":false})
                        }
                    }
                }
                Ok(Some((revision, _, _, _))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"stale_revision","truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "issue_handoff" {
            let database = self.journal.database().lock().await;
            let draft = evohime_local_storage::visual_workflow_builder_store::read_draft(
                database.connection(),
                &request.draft_id,
                &request.owner_scope,
            );
            return match draft {
                Ok(Some((revision, _definition, execution_hash, _layout_hash))) => {
                    let handle = format!("builder-handoff:{}:{}", request.draft_id, revision);
                    let precondition = format!("{}:{}", revision, execution_hash);
                    let result =
                        evohime_local_storage::visual_workflow_builder_store::issue_handoff(
                            database.connection(),
                            evohime_local_storage::visual_workflow_builder_store::Handoff {
                                handle: &handle,
                                draft_id: &request.draft_id,
                                owner_scope: &request.owner_scope,
                                revision,
                                draft_hash: &execution_hash,
                                precondition: &precondition,
                                created_at_ms: crate::task_memory::now_millis() as i64,
                            },
                        );
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":if result.is_ok(){"handoff_issued"}else{"error"},"draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":"","handoff_handle":if result.is_ok(){handle}else{String::new()},"error_code":if result.is_ok(){""}else{"storage_error"},"truncated":false})
                }
                Ok(None) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"unknown_draft","truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":0,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "publish" {
            let handle = String::from_utf8(request.payload.to_vec()).unwrap_or_default();
            let database = self.journal.database().lock().await;
            let published =
                evohime_local_storage::visual_workflow_builder_store::publish_from_handoff(
                    database.connection(),
                    &handle,
                    &request.draft_id,
                    &request.owner_scope,
                    crate::task_memory::now_millis() as i64,
                );
            return match published {
                Ok(Ok((revision, _definition, execution_hash, layout_hash))) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"published","draft_id":request.draft_id,"revision":revision,"execution_hash":execution_hash,"layout_hash":layout_hash,"handoff_handle":handle,"error_code":"","truncated":false})
                }
                Ok(Err(code)) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"conflict","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":code,"truncated":false})
                }
                Err(_) => {
                    serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"error","draft_id":request.draft_id,"revision":request.expected_revision,"execution_hash":"","layout_hash":"","handoff_handle":"","error_code":"storage_error","truncated":false})
                }
            };
        }
        if request.operation == "validate" || request.operation == "save" {
            match serde_json::from_slice::<
                crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
            >(&request.payload)
            {
                Ok(definition) => {
                    match self.validate_visual_workflow_definition(&definition) {
                        Ok(()) if request.operation == "validate" => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "valid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                        }
                        Ok(()) if request.operation == "save" => {
                            let database = self.journal.database().lock().await;
                            let graph_json = serde_json::to_vec(&definition).unwrap_or_default();
                            let layout_json =
                                serde_json::to_vec(&definition.layout).unwrap_or_default();
                            let result =
                            evohime_local_storage::visual_workflow_builder_store::save_draft(
                                database.connection(),
                                evohime_local_storage::visual_workflow_builder_store::SaveDraft { draft_id: &request.draft_id, owner_scope: &request.owner_scope, expected_revision: request.expected_revision, definition_json: &graph_json, layout_json: &layout_json, execution_hash: &definition.execution_hash(), layout_hash: &definition.layout_hash(), composer_provenance_json: None, updated_at_ms: crate::task_memory::now_millis() as i64 },
                            );
                            return match result {
                                Ok(Ok(revision)) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "saved", "draft_id": request.draft_id, "revision": revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                                }
                                Ok(Err(code)) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "conflict", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": code, "truncated": false })
                                }
                                Err(_) => {
                                    serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "error", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": "storage_error", "truncated": false })
                                }
                            };
                        }
                        Ok(()) => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "valid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": definition.execution_hash(), "layout_hash": definition.layout_hash(), "handoff_handle": "", "error_code": "", "truncated": false })
                        }
                        Err(error) => {
                            return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "invalid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": error.to_string(), "truncated": false })
                        }
                    }
                }
                Err(_) => {
                    return serde_json::json!({ "schema_version": 1, "request_id": request.request_id, "status": "invalid", "draft_id": request.draft_id, "revision": request.expected_revision, "execution_hash": "", "layout_hash": "", "handoff_handle": "", "error_code": "invalid_payload", "truncated": false })
                }
            }
        }
        serde_json::json!({
            "schema_version": 1,
            "request_id": request.request_id,
            "status": "unavailable",
            "draft_id": request.draft_id,
            "revision": 0,
            "execution_hash": "",
            "layout_hash": "",
            "handoff_handle": "",
            "error_code": "builder_authoring_not_wired",
            "truncated": false,
        })
    }

    pub(crate) async fn dispatch_conversational_workflow_composer(
        &self,
        request: generated::ConversationalWorkflowComposerCommand,
    ) -> serde_json::Value {
        if request.idempotency_key.trim().is_empty() {
            return serde_json::json!({"schema_version":1,"request_id":request.request_id,"status":"invalid","draft_id":request.draft_id,"revision":request.expected_revision,"proposal_id":"","execution_hash":"","layout_hash":"","error_code":"missing_idempotency_key","projection_json":[],"truncated":false});
        }
        let command_hash =
            hex_encode(&[request.operation.as_bytes(), request.payload.as_ref()].concat());
        match self
            .journal
            .record_deduplicated(
                "workflow-composer",
                &request.idempotency_key,
                &command_hash,
                &[],
            )
            .await
        {
            Ok(Some(bytes)) => {
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    return value;
                }
            }
            Err(_) => {
                return serde_json::json!({
                    "schema_version": 1,
                    "request_id": request.request_id,
                    "status": "conflict",
                    "draft_id": request.draft_id,
                    "revision": request.expected_revision,
                    "proposal_id": "",
                    "execution_hash": "",
                    "layout_hash": "",
                    "error_code": "idempotency_conflict",
                    "projection_json": [],
                    "truncated": false
                });
            }
            Ok(None) => {}
        }
        let result = self
            .dispatch_conversational_workflow_composer_inner(request.clone())
            .await;
        if let Ok(bytes) = serde_json::to_vec(&result) {
            let _ = self
                .journal
                .record_deduplicated(
                    "workflow-composer",
                    &request.idempotency_key,
                    &command_hash,
                    &bytes,
                )
                .await;
        }
        result
    }

    pub(crate) async fn dispatch_conversational_workflow_composer_inner(
        &self,
        request: generated::ConversationalWorkflowComposerCommand,
    ) -> serde_json::Value {
        use crate::conversational_workflow_composer as composer;
        let base = |status: &str, error: &str| {
            serde_json::json!({
                "schema_version": 1,
                "request_id": request.request_id,
                "status": status,
                "draft_id": request.draft_id,
                "revision": request.expected_revision,
                "proposal_id": "",
                "execution_hash": "",
                "layout_hash": "",
                "error_code": error,
                "projection_json": [],
                "truncated": false
            })
        };
        if request.schema_version != 0 && request.schema_version != 1 {
            return base("invalid", "unsupported_schema_version");
        }
        if request.owner_scope.trim().is_empty() || request.draft_id.trim().is_empty() {
            return base("invalid", "invalid_scope");
        }
        match request.operation.as_str() {
            "generate" => {
                let Ok(request_hash) = composer::request_hash(&request.payload) else {
                    return base("invalid", "request_too_large");
                };
                let Some(config) = self.gateway_config.clone() else {
                    return base("unavailable", "model_unavailable");
                };
                let Ok(gateway) = evohime_model_gateway::ModelGateway::from_config(&config) else {
                    return base("unavailable", "model_unavailable");
                };
                let prompt = String::from_utf8_lossy(&request.payload).into_owned();
                let messages = vec![
                    evohime_model_gateway::providers::ChatMessage::text(
                        evohime_model_gateway::providers::ChatRole::System,
                        "Return only JSON matching composer-proposal/v1 with schema_version, proposal_id, definition, assumptions. Never add tools, permissions, credentials or executable identities.",
                    ),
                    evohime_model_gateway::providers::ChatMessage::text(
                        evohime_model_gateway::providers::ChatRole::User,
                        prompt,
                    ),
                ];
                let routing = evohime_model_gateway::RoutingRequest {
                    required_capabilities: vec!["chat".into()],
                    max_cost_micros_per_1k_tokens: None,
                    max_latency_ms: Some(30_000),
                    required_privacy: evohime_model_gateway::PrivacyClass::Internal,
                    allow_fallback: true,
                    preferred_route: Some(config.default_route.clone()),
                    task_class: Some("workflow_composer".into()),
                    offline: false,
                    allow_cloud: true,
                    estimated_input_tokens: (request.payload.len() / 4) as u32,
                    quality_delta: 0.05,
                };
                let response = tokio::time::timeout(
                    std::time::Duration::from_secs(30),
                    gateway.chat_with_tools_with_policy_and_route(
                        evohime_model_gateway::RoutingMode::Balanced,
                        &routing,
                        self.selected_model.get().as_deref(),
                        &messages,
                        &[],
                    ),
                )
                .await;
                let content = match response {
                    Ok(Ok(result)) => result.result.content,
                    Ok(Err(_)) => return base("unavailable", "model_unavailable"),
                    Err(_) => return base("unavailable", "model_timeout"),
                };
                let Ok(proposal) = composer::parse_proposal(content.as_bytes()) else {
                    return base("invalid", "malformed_proposal");
                };
                let projection = serde_json::json!({
                    "proposal_id": proposal.proposal_id,
                    "assumptions": proposal.assumptions,
                    "definition": proposal.definition,
                    "request_hash": request_hash,
                    "requires_review": true,
                    "risk": "review_required",
                });
                let mut result = base("proposal", "");
                result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                result["execution_hash"] = serde_json::json!(proposal.definition.execution_hash());
                result["layout_hash"] = serde_json::json!(proposal.definition.layout_hash());
                result["projection_json"] =
                    serde_json::to_vec(&projection).unwrap_or_default().into();
                result
            }
            "validate" => {
                let Ok(proposal) = composer::parse_proposal(&request.payload) else {
                    return base("invalid", "malformed_proposal");
                };
                if self
                    .validate_visual_workflow_definition(&proposal.definition)
                    .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let mut result = base("valid", "");
                result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                result["execution_hash"] = serde_json::json!(proposal.definition.execution_hash());
                result["layout_hash"] = serde_json::json!(proposal.definition.layout_hash());
                result["projection_json"] = serde_json::to_vec(&serde_json::json!({"risk":"review_required","assumptions":proposal.assumptions})).unwrap_or_default().into();
                result
            }
            "save" => {
                let Ok(proposal) = composer::parse_proposal(&request.payload) else {
                    return base("invalid", "malformed_proposal");
                };
                if self
                    .validate_visual_workflow_definition(&proposal.definition)
                    .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let definition_json = serde_json::to_vec(&proposal.definition).unwrap_or_default();
                let layout_json =
                    serde_json::to_vec(&proposal.definition.layout).unwrap_or_default();
                let execution_hash = proposal.definition.execution_hash();
                let layout_hash = proposal.definition.layout_hash();
                let database = self.journal.database().lock().await;
                let provenance_json = serde_json::to_vec(&composer::ComposerProvenance {
                    schema_version: composer::PROVENANCE_VERSION.into(),
                    request_hash: composer::request_hash(&request.payload).unwrap_or_default(),
                    proposal_hash: composer::canonical_proposal(&proposal)
                        .ok()
                        .map(|bytes| hex::encode(<sha2::Sha256 as sha2::Digest>::digest(bytes)))
                        .unwrap_or_default(),
                    catalog_hash: "core-workflow-registry-v1".into(),
                    model_route: "core-model-gateway".into(),
                    model_version: "bounded-v1".into(),
                })
                .ok();
                match evohime_local_storage::visual_workflow_builder_store::save_draft(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::SaveDraft {
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        expected_revision: request.expected_revision,
                        definition_json: &definition_json,
                        layout_json: &layout_json,
                        execution_hash: &execution_hash,
                        layout_hash: &layout_hash,
                        composer_provenance_json: provenance_json.as_deref(),
                        updated_at_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(Ok(revision)) => {
                        let mut result = base("saved", "");
                        result["proposal_id"] = serde_json::json!(proposal.proposal_id);
                        result["revision"] = serde_json::json!(revision);
                        result["execution_hash"] = serde_json::json!(execution_hash);
                        result["layout_hash"] = serde_json::json!(layout_hash);
                        result
                    }
                    Ok(Err(code)) => base("conflict", code),
                    Err(_) => base("error", "storage_error"),
                }
            }
            "edit" => {
                let database = self.journal.database().lock().await;
                let Ok(Some((revision, definition_json, _, _))) =
                    evohime_local_storage::visual_workflow_builder_store::read_draft(
                        database.connection(),
                        &request.draft_id,
                        &request.owner_scope,
                    )
                else {
                    return base("error", "unknown_draft");
                };
                if revision != request.expected_revision {
                    return base("conflict", "stale_revision");
                }
                let Ok(mut definition) = serde_json::from_slice::<
                    crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
                >(&definition_json) else {
                    return base("error", "corrupt_draft");
                };
                let Ok(command) = serde_json::from_slice::<
                    crate::visual_workflow_builder::DraftCommand,
                >(&request.payload) else {
                    return base("invalid", "invalid_edit");
                };
                if composer::apply_edit(&mut definition, &command).is_err()
                    || self
                        .validate_visual_workflow_definition(&definition)
                        .is_err()
                {
                    return base("invalid", "binding_rejected");
                }
                let definition_json = serde_json::to_vec(&definition).unwrap_or_default();
                let layout_json = serde_json::to_vec(&definition.layout).unwrap_or_default();
                let execution_hash = definition.execution_hash();
                let layout_hash = definition.layout_hash();
                match evohime_local_storage::visual_workflow_builder_store::save_draft(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::SaveDraft {
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        expected_revision: revision,
                        definition_json: &definition_json,
                        layout_json: &layout_json,
                        execution_hash: &execution_hash,
                        layout_hash: &layout_hash,
                        composer_provenance_json: None,
                        updated_at_ms: crate::task_memory::now_millis() as i64,
                    },
                ) {
                    Ok(Ok(next)) => {
                        let mut result = base("edited", "");
                        result["revision"] = serde_json::json!(next);
                        result["execution_hash"] = serde_json::json!(execution_hash);
                        result["layout_hash"] = serde_json::json!(layout_hash);
                        result
                    }
                    Ok(Err(code)) => base("conflict", code),
                    Err(_) => base("error", "storage_error"),
                }
            }
            "handoff" => {
                let database = self.journal.database().lock().await;
                let Ok(Some((revision, _, execution_hash, layout_hash))) =
                    evohime_local_storage::visual_workflow_builder_store::read_draft(
                        database.connection(),
                        &request.draft_id,
                        &request.owner_scope,
                    )
                else {
                    return base("error", "unknown_draft");
                };
                let handle = format!("composer-handoff:{}:{}", request.draft_id, revision);
                let precondition = format!("{}:{}", revision, execution_hash);
                let result = evohime_local_storage::visual_workflow_builder_store::issue_handoff(
                    database.connection(),
                    evohime_local_storage::visual_workflow_builder_store::Handoff {
                        handle: &handle,
                        draft_id: &request.draft_id,
                        owner_scope: &request.owner_scope,
                        revision,
                        draft_hash: &execution_hash,
                        precondition: &precondition,
                        created_at_ms: crate::task_memory::now_millis() as i64,
                    },
                );
                let mut value = base(
                    if result.is_ok() { "handoff" } else { "error" },
                    if result.is_ok() { "" } else { "storage_error" },
                );
                value["revision"] = serde_json::json!(revision);
                value["execution_hash"] = serde_json::json!(execution_hash);
                value["layout_hash"] = serde_json::json!(layout_hash);
                value["projection_json"] = serde_json::to_vec(
                    &serde_json::json!({"handoff_handle":handle,"save_precondition":precondition}),
                )
                .unwrap_or_default()
                .into();
                value
            }
            "discard" => base("discarded", ""),
            _ => base("unavailable", "composer_operation_unavailable"),
        }
    }

    pub(crate) fn validate_visual_workflow_definition(
        &self,
        definition: &crate::visual_workflow_builder::VisualWorkflowBuilderDefinition,
    ) -> Result<(), crate::visual_workflow_builder::BuilderError> {
        definition.validate()?;
        self.workflow_registry
            .validate_bindings(
                &definition.graph,
                &crate::workflow_registry::ParentCapabilities::default().unrestricted_context(),
            )
            .map_err(|_| crate::visual_workflow_builder::BuilderError::RegistryRejected)
    }

    /// Список ожидающих карточек (этап 04.7).
    ///
    /// Это единственный путь, по которому человекочитаемый текст предложения
    /// пересекает границу IPC: durable journal его не несёт, потому что
    /// `events` — append-only таблица, из которой ambient-содержимое пришлось
    /// бы вычищать. Тот же принцип, по которому `memory.pending` не несёт
    /// `statement`.
    ///
    /// Просроченные карточки снимаются здесь же: список, показывающий
    /// вчерашнее предложение как ждущее ответа, врал бы пользователю.
    pub(crate) async fn dispatch_list_ambient_proposals(
        &self,
        request: generated::ListAmbientProposals,
    ) -> serde_json::Value {
        let limit = if request.limit <= 0 {
            50usize
        } else {
            (request.limit as usize).min(200)
        };
        let now_ms = crate::task_memory::now_millis();
        let _ = self.journal.expire_stale_ambient_proposals(now_ms).await;
        let budget = self.proactivity.budget().await;
        match self.journal.list_open_ambient_proposals(limit).await {
            Ok(records) => serde_json::json!({
                "proposals": records
                    .into_iter()
                    .map(|record| serde_json::json!({
                        "proposal_id": record.proposal_id,
                        "kind": record.kind.as_str(),
                        "subject": record.subject,
                        "title": record.title,
                        "source_episode_id": record.source_episode_id.unwrap_or_default(),
                        "created_at_ms": parse_timestamp_ms(&record.created_at),
                        "expires_at_ms": parse_timestamp_ms(&record.expires_at),
                        "occurrences": record.occurrences,
                        "state": record.state.as_str(),
                    }))
                    .collect::<Vec<_>>(),
                "max_per_hour": budget.max_per_hour,
                "max_per_day": budget.max_per_day,
                "min_interval_ms": budget.min_interval_ms,
                "error_code": "",
            }),
            Err(code) => serde_json::json!({
                "proposals": Vec::<serde_json::Value>::new(),
                "max_per_hour": budget.max_per_hour,
                "max_per_day": budget.max_per_day,
                "min_interval_ms": budget.min_interval_ms,
                "error_code": code.as_str(),
            }),
        }
    }

    /// Решение по ограниченному предложению (этап 04.7).
    ///
    /// Три исхода, а не два: принять, отклонить и «больше не предлагать
    /// такое». Принятие создаёт обычную задачу или неисполняемое напоминание
    /// штатным механизмом Core с сохранением провенанса; ни один другой
    /// эффект здесь недостижим.
    ///
    /// `idempotency_key` обязателен: без него двойной клик по карточке
    /// породил бы две задачи. Повтор с тем же ключом возвращает первое
    /// решение, а не создаёт второе.
    pub(crate) async fn dispatch_resolve_ambient_proposal(
        &self,
        request: generated::ResolveAmbientProposal,
    ) -> serde_json::Value {
        use evohime_listener_contract::AmbientErrorCode as Code;
        use evohime_listener_contract::ProposalState;

        let Ok(proposal_id) =
            evohime_listener_contract::ProposalId::new(request.proposal_id.clone())
        else {
            return resolve_failure(Code::InvalidArgument);
        };
        let idempotency_key = request.idempotency_key.trim().to_owned();
        if idempotency_key.is_empty() || idempotency_key.len() > MAX_PROPOSAL_KEY_BYTES {
            return resolve_failure(Code::InvalidArgument);
        }
        // Повтор того же клика: ответ берётся из уже принятого решения, и
        // вторая задача не создаётся.
        match self
            .journal
            .find_ambient_proposal_by_idempotency(&idempotency_key)
            .await
        {
            Ok(Some(existing)) => {
                return serde_json::json!({
                    "applied": true,
                    "state": existing.state.as_str(),
                    "task_id": existing.accepted_task_id.unwrap_or_default(),
                    "error_code": "",
                })
            }
            Ok(None) => {}
            Err(code) => return resolve_failure(code),
        }

        let record = match self
            .journal
            .get_ambient_proposal(proposal_id.as_str())
            .await
        {
            Ok(Some(record)) => record,
            // Нет такого предложения — это честное «не применено», а не
            // вымышленный успех.
            Ok(None) => return resolve_failure(Code::InvalidArgument),
            Err(code) => return resolve_failure(code),
        };
        if record.state.is_terminal() {
            return serde_json::json!({
                "applied": false,
                "state": record.state.as_str(),
                "task_id": record.accepted_task_id.unwrap_or_default(),
                "error_code": Code::InvalidArgument.as_str(),
            });
        }

        let now_ms = crate::task_memory::now_millis();
        let next_state = if request.mute {
            ProposalState::Muted
        } else if request.accepted {
            ProposalState::Accepted
        } else {
            ProposalState::Declined
        };

        // Задача создаётся только при принятии и только до перевода карточки
        // в терминальное состояние: обратный порядок оставил бы «принято» без
        // задачи, если бы создание не удалось.
        let task_id = if next_state == ProposalState::Accepted {
            match self.create_proposal_effect(&record, &idempotency_key).await {
                Ok(task_id) => Some(task_id),
                Err(code) => return resolve_failure(code),
            }
        } else {
            None
        };

        match self
            .journal
            .resolve_ambient_proposal_row(
                proposal_id.as_str(),
                next_state,
                now_ms,
                task_id.as_deref(),
                Some(&idempotency_key),
            )
            .await
        {
            Ok(true) => {}
            // Кто-то решил карточку между чтением и записью: первый клик
            // выигрывает.
            Ok(false) => return resolve_failure(Code::InvalidArgument),
            Err(code) => return resolve_failure(code),
        }

        if next_state == ProposalState::Muted {
            let _ = self
                .proactivity
                .mute(
                    &self.journal,
                    &record.mute_key,
                    record.kind,
                    &record.subject_key,
                    now_ms,
                )
                .await;
        }

        if let Ok(subject_key) =
            evohime_listener_contract::SubjectKey::new(record.subject_key.clone())
        {
            let _ = self
                .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Proposal {
                    proposal_id,
                    episode_id: record
                        .source_episode_id
                        .as_ref()
                        .and_then(|id| evohime_listener_contract::EpisodeId::new(id.clone()).ok()),
                    kind: record.kind,
                    subject_key,
                    proposal_state: next_state,
                })
                .await;
        }

        serde_json::json!({
            "applied": true,
            "state": next_state.as_str(),
            "task_id": task_id.unwrap_or_default(),
            "error_code": "",
        })
    }

    /// Единственный эффект принятого предложения: строка в списке задач.
    ///
    /// Оба вида — обычная запись `work_items` в статусе `backlog`, то есть
    /// ничего не запускающая сама. Напоминание отличается явным `non_goals`:
    /// «не выполняется автоматически» записано в данных, а не подразумевается.
    /// `source_ref` несёт `episode_id` — тот же провенанс, по которому
    /// удаление эпизода находит своих кандидатов памяти.
    pub(crate) async fn create_proposal_effect(
        &self,
        record: &evohime_local_storage::ambient_store::AmbientProposalRecord,
        idempotency_key: &str,
    ) -> Result<String, evohime_listener_contract::AmbientErrorCode> {
        use evohime_listener_contract::AmbientErrorCode as Code;

        // Проектная строка для услышанного заводится один раз и переиспользуется:
        // `work_items.project_id` — внешний ключ, и задача без проекта не
        // сохранится.
        self.journal
            .create_project(
                AMBIENT_PROPOSAL_PROJECT_ID,
                "Услышанное",
                "",
                Some(AMBIENT_PROPOSAL_PROJECT_ID),
            )
            .await
            .map_err(|_| Code::StorageFailed)?;

        let task_id = uuid::Uuid::new_v4().to_string();
        let non_goals = if record.kind == evohime_listener_contract::ProposalKind::Reminder {
            AMBIENT_REMINDER_NON_GOAL.to_owned()
        } else {
            String::new()
        };
        let item = evohime_local_storage::WorkItemRecord {
            id: task_id.clone(),
            project_id: AMBIENT_PROPOSAL_PROJECT_ID.to_owned(),
            parent_id: None,
            title: record.title.clone(),
            description: String::new(),
            source_ref: record.source_episode_id.clone(),
            acceptance_criteria: String::new(),
            non_goals,
            // `backlog`, а не `ready`: подбор следующей задачи берёт только
            // `ready`, поэтому принятое предложение не начинает выполняться
            // само по себе.
            status: "backlog".to_owned(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        };
        // Тот же dedup-путь, что у `CreateTask`: повторный запрос с этим
        // ключом не создаёт второй записи, а возвращает **ту** задачу, что
        // была создана первым кликом. Свежий идентификатор здесь был бы
        // ссылкой в пустоту.
        if let Some(replay) = self
            .journal
            .record_deduplicated(
                AMBIENT_PROPOSAL_CLIENT_ID,
                idempotency_key,
                &record.proposal_id,
                b"",
            )
            .await
            .map_err(|_| Code::StorageFailed)?
        {
            return String::from_utf8(replay).map_err(|_| Code::StorageFailed);
        }
        self.journal
            .create_work_item(&item)
            .await
            .map_err(|_| Code::StorageFailed)?;
        self.journal
            .record_deduplicated(
                AMBIENT_PROPOSAL_CLIENT_ID,
                idempotency_key,
                &record.proposal_id,
                task_id.as_bytes(),
            )
            .await
            .map_err(|_| Code::StorageFailed)?;
        Ok(task_id)
    }

    /// Очередь услышанных команд. Заголовок приложения приходит только здесь:
    /// событие журнала несёт лишь ключ каталога.
    pub(crate) fn dispatch_list_voice_commands(
        &self,
        request: generated::ListVoiceCommands,
    ) -> serde_json::Value {
        let now_ms = crate::task_memory::now_millis();
        let policy = crate::ambient::load_policy(&self.ambient_data_dir());
        let limit = usize::try_from(request.limit)
            .unwrap_or(crate::voice_command::MAX_PENDING)
            .clamp(1, crate::voice_command::MAX_PENDING);
        let commands = self
            .voice_commands
            .list(now_ms)
            .into_iter()
            .take(limit)
            .map(|command| {
                serde_json::json!({
                    "command_id": command.command_id,
                    "kind": command.kind.as_str(),
                    "app_id": command.app_id,
                    "title": command.title,
                    "created_at_ms": command.created_at_ms,
                    "expires_at_ms": command.expires_at_ms(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "commands": commands,
            "requires_confirmation": !policy.voice_commands_autorun,
        })
    }

    /// Решение по услышанной команде.
    ///
    /// Карточка снимается с очереди до запуска, а не после: иначе двойной клик
    /// открыл бы два окна. Второй клик поэтому находит пустоту и получает
    /// `not_found`, а не второй запуск.
    pub(crate) async fn dispatch_resolve_voice_command(
        &self,
        request: generated::ResolveVoiceCommand,
    ) -> serde_json::Value {
        use evohime_listener_contract::VoiceCommandState;

        let now_ms = crate::task_memory::now_millis();
        let Some(command) = self.voice_commands.take(&request.command_id, now_ms) else {
            return serde_json::json!({
                "launched": false,
                "state": VoiceCommandState::Expired.as_str(),
                "app_id": "",
                "error_code": "not_found",
            });
        };
        if !request.accepted {
            self.publish_voice_command(&command, VoiceCommandState::Declined)
                .await;
            return serde_json::json!({
                "launched": false,
                "state": VoiceCommandState::Declined.as_str(),
                "app_id": command.app_id,
                "error_code": "",
            });
        }
        let registry = self.voice_commands.clone();
        let launch_command = command.clone();
        let launched = match tokio::task::spawn_blocking(move || {
            crate::voice_command::launch(&registry, &launch_command, now_ms)
        })
        .await
        {
            Ok(result) => result,
            Err(error) => {
                tracing::error!(%error, "voice command launch task failed");
                Err("voice command launch task failed".to_owned())
            }
        };
        match launched {
            Ok(_) => {
                self.publish_voice_command(&command, VoiceCommandState::Launched)
                    .await;
                serde_json::json!({
                    "launched": true,
                    "state": VoiceCommandState::Launched.as_str(),
                    "app_id": command.app_id,
                    "error_code": "",
                })
            }
            Err(error) => {
                self.publish_voice_command(&command, VoiceCommandState::Failed)
                    .await;
                // Текст ошибки идёт в трассу, а не в ответ: в нём путь к
                // исполняемому файлу, которому в UI делать нечего.
                crate::write_model_trace(
                    "ambient.voice_command.launch_failed",
                    serde_json::json!({ "app_id": command.app_id, "error": error }),
                );
                serde_json::json!({
                    "launched": false,
                    "state": VoiceCommandState::Failed.as_str(),
                    "app_id": command.app_id,
                    "error_code": "launch_failed",
                })
            }
        }
    }
}
