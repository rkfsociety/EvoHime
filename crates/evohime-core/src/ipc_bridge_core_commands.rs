use super::*;

const MAX_PROVIDER_CATALOG_RECOVERY_ROUTES: usize = 64;

fn snapshot_matches_profile(
    snapshot: &crate::free_provider_reliability_routing::ProviderCatalogSnapshot,
    profile: &crate::free_provider_reliability_routing::ProviderProfile,
) -> bool {
    snapshot.provider_id == profile.provider_id
        && snapshot.credential_binding == profile.credential_binding
        && snapshot.region == profile.region
        && snapshot.profile_revision == profile.revision
        && snapshot.profile_content_hash == profile.content_hash
}

impl IpcBridge {
    pub fn journal(&self) -> EventJournal {
        self.journal.clone()
    }

    /// Identity this process picked at construction (план 08-2/08-3
    /// `core_instance_id`) — used to publish the `core_start` ledger event
    /// under the exact id this bridge will stamp on every `EventEnvelope`.
    pub fn core_instance_id(&self) -> &str {
        &self.core_instance_id
    }

    /// True when the client's own `CommandEnvelope` names a generation
    /// (`core_instance_id`/`session_epoch`) other than this process's
    /// current one. An empty/zero client field never counts as stale — it
    /// means the client has no known generation yet (first connect).
    pub(crate) fn stale_generation(&self, command: &generated::CommandEnvelope) -> bool {
        (!command.core_instance_id.is_empty() && command.core_instance_id != self.core_instance_id)
            || (command.session_epoch > 0 && command.session_epoch != self.session_epoch)
    }

    /// Builds a typed `ReplayGap` envelope (план 08-3): honestly filled
    /// bounds instead of the generic JSON `"reason"` field this used to be.
    pub(crate) fn replay_gap_envelope(
        &self,
        requested_after_sequence: u64,
        earliest_available_sequence: Option<u64>,
        latest_available_sequence: u64,
        reason: &str,
    ) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: latest_available_sequence,
            task_id: String::new(),
            event_type: "replay.gap".into(),
            payload: Vec::new(),
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event: Some(generated::event_envelope::Event::ReplayGap(
                generated::ReplayGap {
                    requested_after_sequence,
                    earliest_available_sequence: earliest_available_sequence.unwrap_or(0),
                    latest_available_sequence,
                    reason: reason.to_string(),
                },
            )),
        }
    }

    /// Publishes a typed `ApprovalDecision` ledger event for a resolved
    /// approval, when it is linked to a receipts-tracked action (план 08-4
    /// acceptance: "approval approve/reject/expiry"). Cancellation already
    /// collapses into `granted = false` at the call site — a cancelled
    /// approval and a denied one both land as `Rejected` here, matching the
    /// existing `approval.decision` audit record's own `granted` field.
    /// A no-op when `approval_id` isn't a receipts approval intent (e.g. a
    /// pure workflow-node or routing approval) — those aren't
    /// receipts-tracked actions and get no `ExecutionEventV1` here.
    pub(crate) async fn record_ledger_approval_decision(&self, approval_id: &str, granted: bool) {
        let database = self.journal.database().lock().await;
        let linked: Option<(String, String, String)> = database
            .connection()
            .query_row(
                "SELECT action_id, task_id, run_id FROM receipt_approval_intents
                   WHERE approval_id = ?1",
                [approval_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .unwrap_or(None);
        let Some((action_id, task_id, run_id)) = linked else {
            return;
        };
        let state_after = if granted {
            execution_ledger::ActionState::Running
        } else {
            execution_ledger::ActionState::Denied
        };
        let event = execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: uuid::Uuid::now_v7().to_string(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::Standalone,
            run_id,
            session_id: Some(task_id.clone()),
            task_id,
            created_at_ms: now_ms(),
            state_after: Some(state_after),
            action_id: Some(action_id),
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: None,
            node_id: None,
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: execution_ledger::ExecutionEventBody::ApprovalDecision {
                approval_intent_id: approval_id.to_string(),
                decision: if granted {
                    execution_ledger::ApprovalOutcome::Approved
                } else {
                    execution_ledger::ApprovalOutcome::Rejected
                },
                snapshot_hash: None,
            },
            redaction: execution_ledger::RedactionMeta::default(),
        };
        if let Err(error) = database.append_ledger_event(&event) {
            tracing::warn!(
                event = "ledger.approval_decision_publish_failed",
                approval_id,
                error = %error,
                "typed ledger event failed to publish"
            );
        }
    }

    pub(crate) fn manager_for(journal: &EventJournal) -> Arc<ReceiptKeyManager> {
        let data_dir = journal
            .database_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        Arc::new(ReceiptKeyManager::new(data_dir))
    }
    /// Записывает лимиты каталога в локальную базу. Это подсказка для
    /// планировщика контекста, а не условие работы: провайдер может не сообщить
    /// окно, а база — быть занята другим писателем, и ни то, ни другое не повод
    /// проваливать запрос каталога.
    pub(crate) async fn remember_model_limits(
        &self,
        provider: &str,
        entries: &[evohime_model_gateway::ModelCatalogEntry],
    ) {
        if entries.is_empty() {
            return;
        }
        let records = entries
            .iter()
            .map(
                |entry| evohime_local_storage::model_limit_store::ModelLimitRecord {
                    model: entry.id.clone(),
                    provider: provider.to_string(),
                    context_tokens: entry.context_tokens,
                    max_output_tokens: entry.max_output_tokens,
                },
            )
            .collect::<Vec<_>>();
        let database = self.journal.database().lock().await;
        if let Err(error) = evohime_local_storage::model_limit_store::ModelLimitStoreSql::upsert_all(
            database.connection(),
            &records,
        ) {
            tracing::warn!(target: "model.catalog", %error, "model context limits were not stored");
        }
    }

    /// Publishes one validated empirical free-access snapshot. The storage
    /// layer owns the monotonic revision fence; the process cache is updated
    /// only after that durable publication succeeds.
    pub(crate) async fn remember_free_access_evidence(
        &self,
        evidence: crate::free_provider_reliability_routing::FreeAccessEvidence,
    ) -> Result<bool, &'static str> {
        evidence.validate()?;
        let record = evidence.to_storage_record()?;
        let persisted = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::free_access_evidence_store::put(database.connection(), &record)
                .map_err(|_| "free access evidence storage error")?
        };
        if persisted {
            self.free_access_evidence
                .write()
                .map_err(|_| "free access evidence cache lock failed")?
                .insert(
                    crate::free_provider_reliability_routing::free_access_evidence_scope_key(
                        &evidence.provider_id,
                        &evidence.model_id,
                        &evidence.credential_binding,
                        &evidence.region,
                    ),
                    evidence,
                );
        }
        Ok(persisted)
    }

    /// Hydrates only the configured provider/model scopes. A durable row that
    /// does not round-trip through the Core contract is ignored fail-closed;
    /// no credential binding or raw evidence payload is logged or projected.
    pub async fn hydrate_free_access_evidence(&self) -> usize {
        let routes = self
            .gateway_config
            .as_ref()
            .map(|config| {
                config
                    .routes
                    .values()
                    .take(MAX_PROVIDER_CATALOG_RECOVERY_ROUTES)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut recovered = 0;
        for route in routes {
            let profile =
                match crate::free_provider_reliability_routing::ProviderProfile::from_route_config(
                    &route,
                ) {
                    Ok(profile) => profile,
                    Err(_) => continue,
                };
            let model_id = route.literouter.model.trim();
            if model_id.is_empty() {
                continue;
            }
            let record = {
                let database = self.journal.database().lock().await;
                evohime_local_storage::free_access_evidence_store::get(
                    database.connection(),
                    &profile.provider_id,
                    model_id,
                    &profile.credential_binding,
                    &profile.region,
                )
                .ok()
                .flatten()
            };
            let Some(record) = record else {
                continue;
            };
            let Ok(evidence) =
                crate::free_provider_reliability_routing::FreeAccessEvidence::from_storage_record(
                    &record,
                )
            else {
                tracing::debug!(
                    target: "model.catalog",
                    error_code = "free_access_evidence_recovery_invalid",
                    "free access evidence recovery skipped invalid durable row"
                );
                continue;
            };
            self.free_access_evidence
                .write()
                .expect("free access evidence cache write lock")
                .insert(
                    crate::free_provider_reliability_routing::free_access_evidence_scope_key(
                        &evidence.provider_id,
                        &evidence.model_id,
                        &evidence.credential_binding,
                        &evidence.region,
                    ),
                    evidence,
                );
            recovered += 1;
        }
        tracing::info!(
            target: "model.catalog",
            recovered,
            "free access evidence recovery cache hydrated"
        );
        recovered
    }

    fn free_access_projection(
        &self,
        route: &evohime_model_gateway::ModelRouteConfig,
        selected_model: Option<&str>,
    ) -> serde_json::Value {
        let profile =
            crate::free_provider_reliability_routing::ProviderProfile::from_route_config(route)
                .ok();
        let model_id = selected_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| route.literouter.model.trim());
        let evidence = profile.as_ref().and_then(|profile| {
            self.free_access_evidence.read().ok().and_then(|cache| {
                cache
                    .get(
                        &crate::free_provider_reliability_routing::free_access_evidence_scope_key(
                            &profile.provider_id,
                            model_id,
                            &profile.credential_binding,
                            &profile.region,
                        ),
                    )
                    .cloned()
            })
        });
        let Some(evidence) = evidence else {
            return serde_json::json!({
                "state": "unobserved",
                "model": model_id,
                "strict_eligible": false,
                "redacted": true,
            });
        };
        let now_ms = crate::task_memory::now_millis();
        let freshness = evidence.freshness_at(now_ms);
        serde_json::json!({
            "state": evidence.observed_state,
            "advertised_state": evidence.advertised_state,
            "activation": evidence.activation,
            "allowance": evidence.allowance,
            "freshness": freshness,
            "strict_eligible": evidence.is_strictly_free_at(now_ms),
            "model": evidence.model_id,
            "confidence_bps": evidence.confidence_bps,
            "successful_sample_count": evidence.successful_sample_count,
            "observed_at_ms": evidence.observed_at_ms,
            "expires_at_ms": evidence.expires_at_ms,
            "invalidation": evidence.invalidation,
            "failure_reason": evidence.failure_reason,
            "limits": evidence.limits,
            "redacted": true,
        })
    }

    /// Persists the safe catalog lifecycle snapshot. Provider errors are
    /// reduced to typed codes before reaching storage; no gateway error text
    /// or credential is part of this path.
    pub(crate) async fn remember_provider_catalog_snapshot(
        &self,
        route: &evohime_model_gateway::ModelRouteConfig,
        entries: &[evohime_model_gateway::ModelCatalogEntry],
        failure: Option<crate::free_provider_reliability_routing::CatalogFailureCode>,
    ) -> Option<Vec<evohime_model_gateway::ModelCatalogEntry>> {
        let profile =
            match crate::free_provider_reliability_routing::ProviderProfile::from_route_config(
                route,
            ) {
                Ok(profile) => profile,
                Err(_) => {
                    tracing::warn!(
                        target: "model.catalog",
                        error_code = "provider_profile_invalid",
                        "provider catalog profile was not persisted"
                    );
                    return None;
                }
            };
        let previous_record = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::provider_profile_catalog_store::get(
                database.connection(),
                &profile.provider_id,
                &profile.credential_binding,
                &profile.region,
            )
            .ok()
            .flatten()
        };
        let recovered_snapshot = {
            self.provider_catalog_snapshots
                .read()
                .expect("provider catalog cache read lock")
                .get(
                    &crate::free_provider_reliability_routing::provider_catalog_scope_key(&profile),
                )
                .cloned()
        };
        let previous_snapshot = recovered_snapshot
            .filter(|snapshot| snapshot_matches_profile(snapshot, &profile))
            .or_else(|| {
                previous_record.as_ref().and_then(|record| {
            crate::free_provider_reliability_routing::ProviderCatalogSnapshot::from_storage_record(
                record,
            )
            .ok()
        })
            });
        let next_revision = previous_record
            .as_ref()
            .and_then(|record| u64::try_from(record.revision).ok())
            .and_then(|revision| revision.checked_add(1))
            .unwrap_or(1);
        let now_ms = crate::task_memory::now_millis();
        let catalog_hash = crate::free_provider_reliability_routing::catalog_content_hash(entries)
            .unwrap_or_else(|_| "0".repeat(64));
        let expires_at_ms = now_ms.saturating_add(24 * 60 * 60 * 1_000);
        let snapshot = match failure {
            None => crate::free_provider_reliability_routing::ProviderCatalogSnapshot::fresh_from_catalog(
                &profile,
                entries,
                next_revision,
                catalog_hash,
                now_ms,
                expires_at_ms,
            ),
            Some(code) => {
                if entries.is_empty()
                    && !matches!(
                        code,
                        crate::free_provider_reliability_routing::CatalogFailureCode::CredentialRejected
                            | crate::free_provider_reliability_routing::CatalogFailureCode::DiscoveryUnsupported
                    )
                {
                    if let Some(previous) = previous_snapshot
                        .as_ref()
                        .filter(|snapshot| !snapshot.models.is_empty())
                    {
                        if let Ok(snapshot) = crate::free_provider_reliability_routing::ProviderCatalogSnapshot::stale_after_failure(
                            &profile,
                            previous,
                            next_revision,
                            code,
                        ) {
                            return self
                                .persist_provider_catalog_snapshot(&profile, snapshot)
                                .await;
                        }
                    }
                }
                let state = match code {
                    crate::free_provider_reliability_routing::CatalogFailureCode::CredentialRejected =>
                        crate::free_provider_reliability_routing::ProviderCatalogState::CredentialRejected,
                    crate::free_provider_reliability_routing::CatalogFailureCode::DiscoveryUnsupported =>
                        crate::free_provider_reliability_routing::ProviderCatalogState::DiscoveryUnsupported,
                    _ => crate::free_provider_reliability_routing::ProviderCatalogState::Unavailable,
                };
                crate::free_provider_reliability_routing::ProviderCatalogSnapshot::failure(
                    &profile,
                    next_revision,
                    catalog_hash,
                    state,
                    code,
                    now_ms,
                    expires_at_ms,
                )
            }
        };
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(_) => {
                tracing::warn!(
                    target: "model.catalog",
                    error_code = "provider_catalog_snapshot_invalid",
                    "provider catalog snapshot was not persisted"
                );
                return None;
            }
        };
        self.persist_provider_catalog_snapshot(&profile, snapshot)
            .await
    }

    async fn persist_provider_catalog_snapshot(
        &self,
        profile: &crate::free_provider_reliability_routing::ProviderProfile,
        snapshot: crate::free_provider_reliability_routing::ProviderCatalogSnapshot,
    ) -> Option<Vec<evohime_model_gateway::ModelCatalogEntry>> {
        let record = match snapshot.to_storage_record(profile) {
            Ok(record) => record,
            Err(_) => {
                tracing::warn!(
                    target: "model.catalog",
                    error_code = "provider_catalog_storage_projection_invalid",
                    "provider catalog storage projection was not persisted"
                );
                return None;
            }
        };
        let persisted = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::provider_profile_catalog_store::put(
                database.connection(),
                &record,
            )
        };
        match persisted {
            Ok(true)
                if snapshot.state
                    == crate::free_provider_reliability_routing::ProviderCatalogState::Stale =>
            {
                self.provider_catalog_snapshots
                    .write()
                    .expect("provider catalog cache write lock")
                    .insert(
                        crate::free_provider_reliability_routing::provider_catalog_scope_key(
                            profile,
                        ),
                        snapshot.clone(),
                    );
                snapshot.gateway_entries().ok()
            }
            Ok(true) => {
                self.provider_catalog_snapshots
                    .write()
                    .expect("provider catalog cache write lock")
                    .insert(
                        crate::free_provider_reliability_routing::provider_catalog_scope_key(
                            profile,
                        ),
                        snapshot,
                    );
                None
            }
            Ok(false) => {
                tracing::debug!(
                    target: "model.catalog",
                    error_code = "provider_catalog_revision_conflict",
                    "provider catalog snapshot was not current"
                );
                None
            }
            Err(_) => {
                tracing::warn!(
                    target: "model.catalog",
                    error_code = "provider_catalog_storage_error",
                    "provider catalog snapshot storage failed"
                );
                None
            }
        }
    }

    /// Hydrates only the configured provider scopes from durable storage.
    /// Invalid, mismatched or stale-schema rows are ignored fail-closed; a
    /// later catalog refresh can replace them. No endpoint, prompt or secret
    /// is included in the recovery log.
    pub async fn hydrate_provider_catalog_snapshots(&self) -> usize {
        let routes = self
            .gateway_config
            .as_ref()
            .map(|config| {
                config
                    .routes
                    .values()
                    .take(MAX_PROVIDER_CATALOG_RECOVERY_ROUTES)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut recovered = 0;
        for route in routes {
            let profile =
                match crate::free_provider_reliability_routing::ProviderProfile::from_route_config(
                    &route,
                ) {
                    Ok(profile) => profile,
                    Err(_) => {
                        tracing::debug!(
                            target: "model.catalog",
                            error_code = "provider_profile_invalid",
                            "provider catalog recovery skipped invalid route"
                        );
                        continue;
                    }
                };
            let record = {
                let database = self.journal.database().lock().await;
                evohime_local_storage::provider_profile_catalog_store::get(
                    database.connection(),
                    &profile.provider_id,
                    &profile.credential_binding,
                    &profile.region,
                )
                .ok()
                .flatten()
            };
            let Some(record) = record else {
                continue;
            };
            let Ok(snapshot) = crate::free_provider_reliability_routing::ProviderCatalogSnapshot::from_storage_record(&record) else {
                tracing::debug!(
                    target: "model.catalog",
                    error_code = "provider_catalog_recovery_invalid",
                    "provider catalog recovery skipped invalid durable row"
                );
                continue;
            };
            if !snapshot_matches_profile(&snapshot, &profile) {
                tracing::debug!(
                    target: "model.catalog",
                    error_code = "provider_catalog_recovery_scope_mismatch",
                    "provider catalog recovery skipped a different route scope"
                );
                continue;
            }
            self.provider_catalog_snapshots
                .write()
                .expect("provider catalog cache write lock")
                .insert(
                    crate::free_provider_reliability_routing::provider_catalog_scope_key(&profile),
                    snapshot,
                );
            recovered += 1;
        }
        tracing::info!(
            target: "model.catalog",
            recovered,
            "provider catalog recovery cache hydrated"
        );
        recovered
    }

    pub fn with_provider_catalog_cache(
        mut self,
        cache: crate::free_provider_reliability_routing::ProviderCatalogCache,
    ) -> Self {
        self.provider_catalog_snapshots = cache;
        self
    }

    /// Safe additive projection carried by the existing authenticated
    /// `model.catalog` event. It deliberately omits endpoint, credential
    /// binding, raw provider errors and every request/response body.
    pub(crate) async fn provider_catalog_projection(
        &self,
        route: &evohime_model_gateway::ModelRouteConfig,
        selected_model: Option<&str>,
    ) -> serde_json::Value {
        let profile =
            crate::free_provider_reliability_routing::ProviderProfile::from_route_config(route)
                .ok();
        let snapshot =
            profile.as_ref().and_then(|profile| {
                self.provider_catalog_snapshots
                    .read()
                    .ok()
                    .and_then(|cache| {
                        cache
                        .get(&crate::free_provider_reliability_routing::provider_catalog_scope_key(
                            profile,
                        ))
                        .cloned()
                    })
            });
        let model_id = selected_model
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| route.literouter.model.trim());
        let models = snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .models
                    .iter()
                    .take(256)
                    .map(|model| {
                        serde_json::json!({
                            "id": model.model_id,
                            "limits": {
                                "context_tokens": model.limits.context_tokens,
                                "max_output_tokens": model.limits.max_output_tokens,
                            },
                            "capabilities": model.capabilities,
                            "privacy": model.privacy,
                            "usage": model.usage,
                            "lifecycle": model.lifecycle,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let now_ms = crate::task_memory::now_millis();
        let state = snapshot
            .as_ref()
            .map(|snapshot| {
                if snapshot.state
                    == crate::free_provider_reliability_routing::ProviderCatalogState::Fresh
                    && now_ms >= snapshot.expires_at_ms
                {
                    serde_json::Value::String("expired".into())
                } else {
                    serde_json::to_value(snapshot.state).unwrap_or_default()
                }
            })
            .unwrap_or_else(|| serde_json::Value::String("unobserved".into()));
        let failure_code = snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .failure
                .and_then(|failure| serde_json::to_value(failure).ok())
        });
        let credential_status = if !route.configured() {
            "needs_credential"
        } else if snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.state
                == crate::free_provider_reliability_routing::ProviderCatalogState::CredentialRejected
        }) {
            "rejected"
        } else {
            "configured"
        };
        serde_json::json!({
            "schema_version": 1,
            "provider": profile.as_ref().map(|profile| serde_json::json!({
                "id": profile.provider_id,
                "family": profile.provider_family,
                "transport": profile.transport_kind,
                "region": profile.region,
                "configured": route.configured(),
                "credential_status": credential_status,
            })),
            "catalog": {
                "state": state,
                "revision": snapshot.as_ref().map(|snapshot| snapshot.revision).unwrap_or(0),
                "observed_at_ms": snapshot.as_ref().map(|snapshot| snapshot.observed_at_ms).unwrap_or(0),
                "expires_at_ms": snapshot.as_ref().map(|snapshot| snapshot.expires_at_ms).unwrap_or(0),
                "failure_code": failure_code,
                "model_count": snapshot.as_ref().map(|snapshot| snapshot.models.len()).unwrap_or(0),
                "truncated": snapshot.as_ref().is_some_and(|snapshot| snapshot.models.len() > 256),
                "configured_model": model_id,
                "configured_model_eligible": snapshot.as_ref().map(|snapshot| {
                    snapshot.route_eligible_at(model_id, now_ms)
                }),
            },
            "models": models,
            "free_access": self.free_access_projection(route, selected_model),
            "redacted": true,
        })
    }

    pub fn new(journal: EventJournal) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: None,
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            provider_catalog_snapshots:
                crate::free_provider_reliability_routing::new_provider_catalog_cache(),
            free_access_evidence:
                crate::free_provider_reliability_routing::new_free_access_evidence_cache(),
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
            tool_simulation: Arc::new(tokio::sync::Mutex::new(
                crate::tool_simulation_runtime::ToolSimulationRuntime::default(),
            )),
            external_agents: Arc::new(tokio::sync::Mutex::new(Default::default())),
            role_profiles: Arc::new(tokio::sync::Mutex::new(Default::default())),
            conversation_subscription: Arc::new(tokio::sync::Mutex::new(None)),
            team_sop: Arc::new(tokio::sync::Mutex::new(Default::default())),
            human_work_items: Arc::new(tokio::sync::Mutex::new(Default::default())),
            browser_backends: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            background_tasks: Arc::new(crate::bounded_tasks::BoundedTaskGroup::new(
                crate::bounded_tasks::DEFAULT_CAPACITY,
            )),
        }
    }

    pub fn with_coordinator(journal: EventJournal, coordinator: TaskCoordinator) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            provider_catalog_snapshots:
                crate::free_provider_reliability_routing::new_provider_catalog_cache(),
            free_access_evidence:
                crate::free_provider_reliability_routing::new_free_access_evidence_cache(),
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
            tool_simulation: Arc::new(tokio::sync::Mutex::new(
                crate::tool_simulation_runtime::ToolSimulationRuntime::default(),
            )),
            external_agents: Arc::new(tokio::sync::Mutex::new(Default::default())),
            role_profiles: Arc::new(tokio::sync::Mutex::new(Default::default())),
            conversation_subscription: Arc::new(tokio::sync::Mutex::new(None)),
            team_sop: Arc::new(tokio::sync::Mutex::new(Default::default())),
            human_work_items: Arc::new(tokio::sync::Mutex::new(Default::default())),
            browser_backends: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            background_tasks: Arc::new(crate::bounded_tasks::BoundedTaskGroup::new(
                crate::bounded_tasks::DEFAULT_CAPACITY,
            )),
        }
    }

    pub fn with_coordinator_and_approvals(
        journal: EventJournal,
        coordinator: TaskCoordinator,
        approvals: ApprovalCoordinator,
        tools: Arc<ToolRegistry>,
        model_config: Option<ModelConfigSnapshot>,
        gateway_config: Option<ModelGatewayConfig>,
    ) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: Some(approvals),
            tools: Some(tools),
            model_config,
            gateway_config,
            provider_catalog_snapshots:
                crate::free_provider_reliability_routing::new_provider_catalog_cache(),
            free_access_evidence:
                crate::free_provider_reliability_routing::new_free_access_evidence_cache(),
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            revision_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            analysis_kernels: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            ambient: crate::ambient::AmbientListeningRegistry::default(),
            ambient_data_dir: None,
            proactivity: crate::ambient::AmbientProactivityRegistry::default(),
            workflow_approvals: Arc::new(crate::workflow_runtime::WorkflowApprovalRegistry::new()),
            voice_commands: Arc::new(crate::voice_command::VoiceCommandRegistry::new()),
            workflow_registry: Arc::new(crate::workflow_registry::WorkflowRegistry::bootstrap()),
            tool_simulation: Arc::new(tokio::sync::Mutex::new(
                crate::tool_simulation_runtime::ToolSimulationRuntime::default(),
            )),
            external_agents: Arc::new(tokio::sync::Mutex::new(Default::default())),
            role_profiles: Arc::new(tokio::sync::Mutex::new(Default::default())),
            conversation_subscription: Arc::new(tokio::sync::Mutex::new(None)),
            team_sop: Arc::new(tokio::sync::Mutex::new(Default::default())),
            human_work_items: Arc::new(tokio::sync::Mutex::new(Default::default())),
            browser_backends: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            background_tasks: Arc::new(crate::bounded_tasks::BoundedTaskGroup::new(
                crate::bounded_tasks::DEFAULT_CAPACITY,
            )),
        }
    }

    /// Разделяемый реестр состояния слушания.
    pub fn ambient(&self) -> crate::ambient::AmbientListeningRegistry {
        self.ambient.clone()
    }

    pub fn voice_commands(&self) -> Arc<crate::voice_command::VoiceCommandRegistry> {
        self.voice_commands.clone()
    }

    /// Подключает готовый реестр: `main.rs` создаёт его до моста, чтобы
    /// endpoint листенера и мост говорили об одном и том же состоянии.
    pub fn with_ambient(mut self, ambient: crate::ambient::AmbientListeningRegistry) -> Self {
        self.ambient = ambient;
        self
    }

    /// Каталог политики и намерения слушания.
    pub fn with_ambient_data_dir(mut self, directory: std::path::PathBuf) -> Self {
        self.ambient_data_dir = Some(directory);
        self
    }

    /// Разделяемый реестр проактивности.
    pub fn proactivity(&self) -> crate::ambient::AmbientProactivityRegistry {
        self.proactivity.clone()
    }

    /// Подключает готовый реестр проактивности: `main.rs` создаёт его до
    /// агента и до моста, чтобы обе стороны считали один и тот же потолок.
    pub fn with_proactivity(
        mut self,
        proactivity: crate::ambient::AmbientProactivityRegistry,
    ) -> Self {
        self.proactivity = proactivity;
        self
    }

    pub(crate) fn ambient_data_dir(&self) -> std::path::PathBuf {
        self.ambient_data_dir
            .clone()
            .unwrap_or_else(crate::ambient::data_dir)
    }

    /// Пишет ambient-событие в durable journal и будит push к оболочке.
    ///
    /// Без второго шага запись легла бы в базу, но открытое окно узнало бы о
    /// ней только со следующим событием задачи.
    pub async fn publish_ambient(
        &self,
        event: &evohime_listener_contract::AmbientLogEvent,
    ) -> Result<i64, evohime_listener_contract::AmbientErrorCode> {
        let sequence = self.journal.append_ambient_event(event).await?;
        if let Some(coordinator) = &self.coordinator {
            coordinator.notify_journalled(sequence.max(0) as u64);
        }
        Ok(sequence)
    }

    /// Отдаёт закрытый эпизод в ambient-извлечение (04.6).
    ///
    /// Мост здесь только курьер: решают `EVOHIME_AMBIENT_MEMORY`, общий режим
    /// извлечения и ambient-бюджеты, и все три проверяются в Core, а не тут.
    /// Без координатора вызов молча ничего не делает — извлекателя в этой
    /// сборке просто нет.
    pub async fn request_ambient_extraction(&self, episode_id: &str) {
        let Some(coordinator) = &self.coordinator else {
            return;
        };
        let _ = coordinator
            .dispatch(CoreCommand::ExtractAmbientMemory {
                episode_id: episode_id.to_owned(),
            })
            .await;
    }
}
