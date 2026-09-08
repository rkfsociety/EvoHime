use super::*;

pub(super) async fn handle(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
    match command {
        CoreCommand::KnowledgeSourceRegistryProjectRole {
            operation,
            source_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let _ = idempotency_key;
            let result = async {
                    let journal = state.lock().await.journal.clone().ok_or_else(|| "storage journal is not configured".to_string())?;
                    let database = journal.database().lock().await;
                    use crate::knowledge_source_registry_project_role as knowledge;
                    use evohime_local_storage::knowledge_source_registry_project_role_store as store;
                    let policy = knowledge::default_policy();
                    match operation.as_str() {
                        "collection_register" => {
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_collection".to_string())?;
                            knowledge::validate_collection(&collection, &policy).map_err(|e| e.to_string())?;
                            for source_id in &collection.source_ids {
                                if store::get_source(database.connection(), source_id).map_err(|_| "storage_failed".to_string())?.is_none() {
                                    return Err("knowledge_source_not_found".into());
                                }
                            }
                            let json = serde_json::to_vec(&collection).map_err(|_| "serialization_failed".to_string())?;
                            if !store::put_collection(database.connection(), &collection.id, collection.version, &collection.content_hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())? {
                                return Err("knowledge_collection_stale_version".into());
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"source_count":collection.source_ids.len(),"status":collection.status,"scope":collection.scope,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "collection_get" => {
                            let json = store::get_collection(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_collection_not_found".to_string())?;
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_collection".to_string())?;
                            knowledge::validate_collection(&collection, &policy).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"source_count":collection.source_ids.len(),"status":collection.status,"scope":collection.scope,"content_hash":collection.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "collection_view" => {
                            let request: KnowledgeCollectionViewRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_collection_view".to_string())?;
                            let collection_json = store::get_collection(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_collection_not_found".to_string())?;
                            let collection: knowledge::KnowledgeCollection = serde_json::from_slice(&collection_json).map_err(|_| "corrupt_knowledge_collection".to_string())?;
                            let target_kind = request.target_kind;
                            let target_id = request.target_id.as_str();
                            let sources = collection.source_ids.iter().filter_map(|id| store::get_source(database.connection(), id).ok().flatten()).filter_map(|json| serde_json::from_slice::<knowledge::KnowledgeSource>(&json).ok()).collect::<Vec<_>>();
                            let mut bindings = Vec::new();
                            for id in &collection.source_ids {
                                bindings.extend(store::list_bindings(database.connection(), id, knowledge::MAX_BINDINGS_PER_SOURCE).map_err(|_| "storage_failed".to_string())?.into_iter().filter_map(|json| serde_json::from_slice::<knowledge::KnowledgeBinding>(&json).ok()));
                            }
                            let view = knowledge::build_collection_view(knowledge::BuildCollectionViewInput { collection: &collection, sources: &sources, bindings: &bindings, target_kind, target_id, max_sensitivity: knowledge::Sensitivity::Internal, expires_at_ms: None, policy: &policy }).map_err(|e| e.to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"collection_id":collection.id,"version":collection.version,"view_id":view.id,"source_ids":view.source_ids,"content_hash":view.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "register" => {
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_source".to_string())?;
                            knowledge::validate_source(&source, &policy).map_err(|e| e.to_string())?;
                            let json = serde_json::to_vec(&source).map_err(|_| "serialization_failed".to_string())?;
                            store::put_source(database.connection(), &source.id, source.version, &source.content_hash, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source.id,"version":source.version,"kind":source.kind,"status":source.status,"fingerprint":source.source_fingerprint,"sensitivity":source.sensitivity,"content_hash":source.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "get" => {
                            let json = store::get_source(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_source_not_found".to_string())?;
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_source".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source.id,"version":source.version,"kind":source.kind,"status":source.status,"fingerprint":source.source_fingerprint,"sensitivity":source.sensitivity,"content_hash":source.content_hash,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "bind" => {
                            let binding: knowledge::KnowledgeBinding = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_binding".to_string())?;
                            knowledge::validate_binding(&binding, &policy).map_err(|e| e.to_string())?;
                            if binding.source_id != source_id { return Err("knowledge_source_mismatch".into()); }
                            let binding_id = format!("{}:{}:{}", binding.target_kind as u8, binding.target_id, binding.source_id);
                            let json = serde_json::to_vec(&binding).map_err(|_| "serialization_failed".to_string())?;
                            store::put_binding(database.connection(), &binding_id, &binding.source_id, &json, crate::task_memory::now_millis() as i64).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":binding.source_id,"target_id":binding.target_id,"bound":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "index" => {
                            let chunks: Vec<knowledge::KnowledgeChunk> = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_chunks".to_string())?;
                            if chunks.len() > knowledge::MAX_CHUNKS_PER_SOURCE { return Err("knowledge_chunk_limit".into()); }
                            for chunk in &chunks {
                                if chunk.source_id != source_id || chunk.content_projection.len() > knowledge::MAX_CHUNK_BYTES { return Err("invalid_knowledge_chunk".into()); }
                                let json = serde_json::to_vec(chunk).map_err(|_| "serialization_failed".to_string())?;
                                store::put_chunk(database.connection(), store::PutChunkInput { id: &chunk.id, source_id: &chunk.source_id, revision: chunk.source_revision, ordinal: chunk.ordinal, locator: &chunk.locator, json: &json, now_ms: crate::task_memory::now_millis() as i64 }).map_err(|_| "storage_failed".to_string())?;
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source_id,"indexed_chunks":chunks.len(),"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "retrieve" => {
                            let request: KnowledgeQueryRequest = serde_json::from_slice(&payload).map_err(|_| "invalid_knowledge_query".to_string())?;
                            let query = request.query.as_str();
                            if query.is_empty() || query.len() > knowledge::MAX_ID_BYTES { return Err("invalid_knowledge_query".into()); }
                            let target_kind = request.target_kind;
                            let target_id = request.target_id.as_str();
                            let source_json = store::get_source(database.connection(), &source_id).map_err(|_| "storage_failed".to_string())?.ok_or_else(|| "knowledge_source_not_found".to_string())?;
                            let source: knowledge::KnowledgeSource = serde_json::from_slice(&source_json).map_err(|_| "corrupt_knowledge_source".to_string())?;
                            let bindings = store::list_bindings(database.connection(), &source_id, knowledge::MAX_BINDINGS_PER_SOURCE).map_err(|_| "storage_failed".to_string())?.into_iter().filter_map(|json| serde_json::from_slice(&json).ok()).collect::<Vec<knowledge::KnowledgeBinding>>();
                            let view = knowledge::build_view(knowledge::BuildViewInput { id: format!("view-{source_id}"), run_id: "runtime".into(), sources: std::slice::from_ref(&source), bindings: &bindings, target_kind, target_id, max_sensitivity: knowledge::Sensitivity::Internal, retrieval_profile: "keyword".into(), expires_at_ms: None, policy: &policy }).map_err(|e| e.to_string())?;
                            let mut hits = Vec::new();
                            for json in store::list_chunks(database.connection(), &source_id, knowledge::MAX_CHUNKS_PER_SOURCE).map_err(|_| "storage_failed".to_string())? {
                                let chunk: knowledge::KnowledgeChunk = serde_json::from_slice(&json).map_err(|_| "corrupt_knowledge_chunk".to_string())?;
                                if chunk.content_projection.to_ascii_lowercase().contains(&query.to_ascii_lowercase()) {
                                    let hit = knowledge::KnowledgeHit { source_id: chunk.source_id, source_revision: chunk.source_revision, chunk_id: chunk.id, locator: chunk.locator, excerpt: chunk.content_projection, score: 1, match_reasons: vec!["keyword".into()], freshness: if source.status == knowledge::SourceStatus::Ready { "current".into() } else { "stale".into() }, trust_class: source.trust_class.clone() };
                                    knowledge::validate_hit(&hit, &view, &policy).map_err(|e| e.to_string())?;
                                    hits.push(hit); if hits.len() >= knowledge::MAX_HITS { break; }
                                }
                            }
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"source_id":source_id,"view_id":view.id,"hit_count":hits.len(),"hits":hits,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_revision" => {
                            let record: evohime_local_storage::grounded_research_store::ResearchRevisionRecord =
                                serde_json::from_slice(&payload).map_err(|_| "invalid_research_revision".to_string())?;
                            if record.source_id != source_id {
                                return Err("research_source_mismatch".into());
                            }
                            let inserted = evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_revision(
                                database.connection(), &record).map_err(|_| "invalid_research_revision".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"revision_id":record.revision_id,"inserted":inserted,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_evidence_item" => {
                            let item: crate::research::EvidenceItem = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_research_evidence_item".to_string())?;
                            item.validate().map_err(|_| "invalid_research_evidence_item".to_string())?;
                            let revision = evohime_local_storage::grounded_research_store::GroundedResearchStore::get_revision(
                                database.connection(), &item.revision_id,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "research_revision_not_found".to_string())?;
                            if item.revision_id.is_empty() || revision.source_id != source_id {
                                return Err("research_evidence_source_mismatch".into());
                            }
                            let revision: crate::research::ResearchSourceRevision =
                                serde_json::from_value(serde_json::json!({
                                    "revision_id": revision.revision_id,
                                    "source_id": revision.source_id,
                                    "revision": revision.revision as u64,
                                    "content_hash": revision.content_hash,
                                    "origin_snapshot": revision.origin_snapshot,
                                    "parser_version": revision.parser_version,
                                    "index_profile": revision.index_profile,
                                    "status": revision.status,
                                    "trust": revision.trust,
                                    "locator_root": revision.locator_root,
                                }))
                                .map_err(|_| "corrupt_research_revision".to_string())?;
                            item.validate_against_revision(&revision)
                                .map_err(|_| "stale_research_evidence_item".to_string())?;
                            let locator_json = serde_json::to_vec(&item.locator).map_err(|_| "serialization_failed".to_string())?;
                            let inserted = evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_evidence_item(
                                database.connection(), &item.evidence_id, &item.revision_id, &locator_json,
                                &item.content_hash, &serde_json::to_string(&item.trust).unwrap_or_default(),
                            ).map_err(|_| "invalid_research_evidence_item".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"evidence_id":item.evidence_id,"inserted":inserted,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_session" => {
                            let session: crate::research::ResearchSession = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_research_session".to_string())?;
                            session.validate().map_err(|_| "invalid_research_session".to_string())?;
                            if session.collection_id != source_id {
                                return Err("research_collection_mismatch".into());
                            }
                            let pinned = serde_json::to_vec(&session.pinned_revision_ids)
                                .map_err(|_| "serialization_failed".to_string())?;
                            let budget = serde_json::to_vec(&session.budget)
                                .map_err(|_| "serialization_failed".to_string())?;
                            let inserted = evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_session(
                                database.connection(),
                                &session.session_id,
                                &session.workspace_id,
                                &session.collection_id,
                                1,
                                &serde_json::to_string(&session.mode).unwrap_or_default(),
                                &serde_json::to_string(&session.source_policy).unwrap_or_default(),
                                &pinned,
                                session.tool_policy_snapshot.as_bytes(),
                                session.model_policy_snapshot.as_bytes(),
                                &budget,
                                &serde_json::to_string(&session.state).unwrap_or_default(),
                            ).map_err(|_| "invalid_research_session".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"session_id":session.session_id,"inserted":inserted,"state":session.state,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_artifact" => {
                            let artifact: crate::research::ResearchArtifact = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_research_artifact".to_string())?;
                            artifact.validate().map_err(|_| "invalid_research_artifact".to_string())?;
                            for citation in &artifact.citations {
                                let claim = artifact
                                    .claims
                                    .iter()
                                    .find(|claim| claim.claim_id == citation.claim_id)
                                    .ok_or_else(|| "invalid_research_citation".to_string())?;
                                let (revision_id, locator_json, content_hash, trust) =
                                    evohime_local_storage::grounded_research_store::GroundedResearchStore::get_evidence(
                                        database.connection(), &citation.evidence_id,
                                    )
                                    .map_err(|_| "storage_failed".to_string())?
                                    .ok_or_else(|| "research_evidence_not_found".to_string())?;
                                let evidence = crate::research::EvidenceItem {
                                    evidence_id: citation.evidence_id.clone(),
                                    revision_id: revision_id.clone(),
                                    locator: serde_json::from_slice(&locator_json)
                                        .map_err(|_| "corrupt_research_locator".to_string())?,
                                    content_hash,
                                    trust: serde_json::from_str(&trust)
                                        .map_err(|_| "corrupt_research_trust".to_string())?,
                                };
                                let revision_record = evohime_local_storage::grounded_research_store::GroundedResearchStore::get_revision(
                                    database.connection(), &revision_id,
                                )
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "research_revision_not_found".to_string())?;
                                let revision: crate::research::ResearchSourceRevision =
                                    serde_json::from_value(serde_json::json!({
                                        "revision_id": revision_record.revision_id,
                                        "source_id": revision_record.source_id,
                                        "revision": revision_record.revision as u64,
                                        "content_hash": revision_record.content_hash,
                                        "origin_snapshot": revision_record.origin_snapshot,
                                        "parser_version": revision_record.parser_version,
                                        "index_profile": revision_record.index_profile,
                                        "status": revision_record.status,
                                        "trust": revision_record.trust,
                                        "locator_root": revision_record.locator_root,
                                    }))
                                    .map_err(|_| "corrupt_research_revision".to_string())?;
                                crate::research::validate_research_citation(
                                    citation, claim, &evidence, &revision,
                                )
                                .map_err(|_| "invalid_research_citation".to_string())?;
                            }
                            let claims = serde_json::to_vec(&artifact.claims)
                                .map_err(|_| "serialization_failed".to_string())?;
                            let citations = serde_json::to_vec(&artifact.citations)
                                .map_err(|_| "serialization_failed".to_string())?;
                            let inserted = evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_artifact(
                                database.connection(),
                                &artifact.artifact_id,
                                artifact.revision as i64,
                                &artifact.session_id,
                                &artifact.content_hash,
                                &serde_json::to_string(&artifact.coverage).unwrap_or_default(),
                                &claims,
                                &citations,
                                crate::task_memory::now_millis() as i64,
                            ).map_err(|_| "invalid_research_artifact".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"artifact_id":artifact.artifact_id,"revision":artifact.revision,"inserted":inserted,"immutable":true,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_artifact_get" => {
                            let request: serde_json::Value = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_research_artifact_query".to_string())?;
                            let artifact_id = request.get("artifact_id").and_then(|v| v.as_str())
                                .ok_or_else(|| "invalid_research_artifact_query".to_string())?;
                            let revision = request.get("revision").and_then(|v| v.as_i64())
                                .ok_or_else(|| "invalid_research_artifact_query".to_string())?;
                            let result = evohime_local_storage::grounded_research_store::GroundedResearchStore::get_artifact(
                                database.connection(), artifact_id, revision)
                                .map_err(|_| "storage_failed".to_string())?
                                .ok_or_else(|| "research_artifact_not_found".to_string())?;
                            serde_json::to_vec(&serde_json::json!({
                                "schema_version": 1,
                                "artifact_id": artifact_id,
                                "revision": revision,
                                "content_hash": result.2,
                                "coverage": result.3,
                                "claims": serde_json::from_slice::<serde_json::Value>(&result.0).unwrap_or(serde_json::Value::Null),
                                "citations": serde_json::from_slice::<serde_json::Value>(&result.1).unwrap_or(serde_json::Value::Null),
                                "immutable": true,
                                "redacted": true,
                            })).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_delta" => {
                            let delta: crate::research::ResearchDelta = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_research_delta".to_string())?;
                            let added = serde_json::to_vec(&delta.added_evidence_ids).map_err(|_| "serialization_failed".to_string())?;
                            let stale = serde_json::to_vec(&delta.stale_evidence_ids).map_err(|_| "serialization_failed".to_string())?;
                            let inserted = evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_delta(
                                database.connection(), &delta.delta_id, &delta.previous_artifact_id,
                                &delta.current_artifact_id, &added, &stale)
                                .map_err(|_| "invalid_research_delta".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"delta_id":delta.delta_id,"inserted":inserted,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_artifact_promote" => {
                            let request: serde_json::Value = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_research_promotion".to_string())?;
                            let artifact: crate::research::ResearchArtifact = serde_json::from_value(
                                request.get("artifact").cloned().ok_or_else(|| "invalid_research_promotion".to_string())?,
                            ).map_err(|_| "invalid_research_artifact".to_string())?;
                            artifact.validate().map_err(|_| "invalid_research_artifact".to_string())?;
                            let project_id = request.get("project_id").and_then(|value| value.as_str())
                                .ok_or_else(|| "project_id_required".to_string())?;
                            let locator = request.get("content_locator").and_then(|value| value.as_str())
                                .ok_or_else(|| "content_locator_required".to_string())?;
                            let row = crate::artifact_handoff_registry::ProjectArtifactRevision {
                                schema_version: crate::artifact_handoff_registry::CONTRACT_VERSION,
                                artifact_id: artifact.artifact_id.clone(),
                                project_id: project_id.to_string(),
                                revision: artifact.revision,
                                state: crate::artifact_handoff_registry::ArtifactState::Produced,
                                content_locator: locator.to_string(),
                                content_hash: artifact.content_hash.clone(),
                                producer_identity: "core:grounded-research".to_string(),
                                workspace_fingerprint: None,
                                parent_fingerprints: Vec::new(),
                                metadata: serde_json::json!({
                                    "research_session_id": artifact.session_id,
                                    "coverage": artifact.coverage,
                                    "claims": artifact.claims.len(),
                                    "citations": artifact.citations.len(),
                                }),
                            };
                            crate::artifact_handoff_registry::validate(&row).map_err(|error| error.to_string())?;
                            let metadata_json = serde_json::to_vec(&row.metadata).map_err(|_| "serialization_failed".to_string())?;
                            let inserted = evohime_local_storage::artifact_handoff_registry_store::insert_revision_atomic(
                                database.connection(),
                                &evohime_local_storage::artifact_handoff_registry_store::RegistryRow {
                                    artifact_id: row.artifact_id.clone(), project_id: row.project_id.clone(),
                                    revision: row.revision, state: row.state.as_str().to_string(),
                                    content_locator: row.content_locator, content_hash: row.content_hash,
                                    metadata_json, created_at_ms: crate::task_memory::now_millis() as i64,
                                },
                                &[],
                            ).map(|_| true).map_err(|_| "artifact_promotion_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"artifact_id":artifact.artifact_id,"revision":artifact.revision,"promoted":inserted,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        "research_session_transition" => {
                            let request: serde_json::Value = serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_research_session_transition".to_string())?;
                            let session_id = request.get("session_id").and_then(|value| value.as_str())
                                .ok_or_else(|| "session_id_required".to_string())?;
                            let expected_revision = request.get("expected_revision").and_then(|value| value.as_i64())
                                .ok_or_else(|| "expected_revision_required".to_string())?;
                            let from: crate::research::ResearchSessionState = serde_json::from_value(
                                request.get("from_state").cloned().ok_or_else(|| "from_state_required".to_string())?,
                            ).map_err(|_| "invalid_research_session_transition".to_string())?;
                            let next: crate::research::ResearchSessionState = serde_json::from_value(
                                request.get("next_state").cloned().ok_or_else(|| "next_state_required".to_string())?,
                            ).map_err(|_| "invalid_research_session_transition".to_string())?;
                            crate::research::transition_research_session(from, next)
                                .map_err(|_| "invalid_research_session_transition".to_string())?;
                            let changed = evohime_local_storage::grounded_research_store::GroundedResearchStore::transition_session(
                                database.connection(), session_id, expected_revision,
                                &serde_json::to_string(&from).unwrap_or_default(),
                                &serde_json::to_string(&next).unwrap_or_default(),
                            ).map_err(|_| "storage_failed".to_string())?;
                            serde_json::to_vec(&serde_json::json!({"schema_version":1,"session_id":session_id,"changed":changed,"next_state":next,"redacted":true})).map_err(|_| "serialization_failed".to_string())
                        }
                        _ => Err("unsupported_knowledge_registry_operation".into()),
                    }
                }.await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::KnowledgeSourceRegistryProjectRole {
                source_id,
                operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::DurableRemoteTaskBridge {
            operation,
            remote_task_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let event_task_id = remote_task_id.clone();
            let result = async {
                if idempotency_key.is_empty() {
                    return Err("invalid_remote_task_idempotency_key".to_string());
                }
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                let database = journal.database().lock().await;
                use crate::durable_remote_task_bridge as bridge;
                use evohime_local_storage::durable_remote_task_bridge_store as store;
                let policy = bridge::default_policy();
                match operation.as_str() {
                    "submit" => {
                        let request: RemoteTaskSubmitRequest = serde_json::from_slice(&payload)
                            .map_err(|_| "invalid_remote_task_submit".to_string())?;
                        let request_bytes = serde_json::to_vec(&request.request)
                            .map_err(|_| "invalid_remote_task_request".to_string())?;
                        let record = bridge::build_record(
                            remote_task_id.clone(),
                            &request.toolset,
                            request.operation,
                            &request_bytes,
                            request.provenance_ref,
                            crate::task_memory::now_millis() as i64,
                            &policy,
                        )
                        .map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&record)
                            .map_err(|_| "serialization_failed".to_string())?;
                        if !store::put_record(
                            database.connection(),
                            &record.id,
                            record.version,
                            &format!("{:?}", record.status),
                            &record.content_hash,
                            &json,
                            record.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("remote_task_stale_version".into());
                        }
                        serde_json::to_vec(&bridge::status_projection(&record))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "status" => {
                        let json = store::get_record(database.connection(), &remote_task_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "remote_task_not_found".to_string())?;
                        let record: bridge::RemoteTaskRecord = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_remote_task".to_string())?;
                        serde_json::to_vec(&bridge::status_projection(&record))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "cancel" | "poll" | "result" => {
                        let json = store::get_record(database.connection(), &remote_task_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "remote_task_not_found".to_string())?;
                        let mut record: bridge::RemoteTaskRecord = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_remote_task".to_string())?;
                        if expected_version != 0 && expected_version != record.version {
                            return Err("remote_task_stale_version".into());
                        }
                        let now = crate::task_memory::now_millis() as i64;
                        match operation.as_str() {
                            "cancel" => {
                                let version = record.version;
                                bridge::cancel(&mut record, version, now)
                                    .map_err(|e| e.to_string())?;
                            }
                            "poll" => {
                                let request: RemoteTaskPollRequest =
                                    serde_json::from_slice(&payload)
                                        .map_err(|_| "invalid_remote_task_poll".to_string())?;
                                bridge::lease_for_poll(
                                    &mut record,
                                    &request.lease_owner,
                                    now,
                                    &policy,
                                )
                                .map_err(|e| e.to_string())?;
                            }
                            "result" => {
                                let request: RemoteTaskResultRequest =
                                    serde_json::from_slice(&payload)
                                        .map_err(|_| "invalid_remote_task_result".to_string())?;
                                let status = request.status;
                                if !matches!(
                                    status,
                                    bridge::RemoteTaskStatus::InputRequired
                                        | bridge::RemoteTaskStatus::Completed
                                        | bridge::RemoteTaskStatus::Failed
                                        | bridge::RemoteTaskStatus::Cancelled
                                        | bridge::RemoteTaskStatus::Unknown
                                ) {
                                    return Err("invalid_remote_task_transition".into());
                                }
                                record.status = status;
                                record.transport_status = request.transport_status;
                                record.result_artifact_ref = request.result_artifact_ref;
                                record.version += 1;
                                record.updated_at_ms = now;
                                bridge::validate_record(
                                    &record,
                                    &bridge::RemoteTaskToolset {
                                        schema_version: 1,
                                        id: record.toolset_id.clone(),
                                        version: 1,
                                        provider_kind: bridge::RemoteProviderKind::Mcp,
                                        provider_ref: "trusted-adapter".into(),
                                        operation_names: vec![record.operation.clone()],
                                        content_hash: "trusted".into(),
                                    },
                                    &policy,
                                )
                                .map_err(|e| e.to_string())?;
                            }
                            _ => unreachable!(),
                        }
                        bridge::refresh_content_hash(&mut record).map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&record)
                            .map_err(|_| "serialization_failed".to_string())?;
                        store::put_record(
                            database.connection(),
                            &record.id,
                            record.version,
                            &format!("{:?}", record.status),
                            &record.content_hash,
                            &json,
                            record.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?;
                        serde_json::to_vec(&bridge::status_projection(&record))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    _ => Err("unsupported_remote_task_operation".into()),
                }
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::DurableRemoteTaskBridge {
                remote_task_id: event_task_id,
                operation: event_operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::MessageInterventionPolicies {
            operation,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_operation = operation.clone();
            let result = async {
                if idempotency_key.is_empty() || idempotency_key.len() > 128 {
                    return Err("invalid_intervention_idempotency_key".into());
                }
                if expected_version > 1 {
                    return Err("intervention_stale_version".into());
                }
                let request: InterventionRequest = serde_json::from_slice(&payload)
                    .map_err(|_| "invalid_intervention_payload".to_string())?;
                let verdict = crate::message_intervention_policies::evaluate(
                    &request.policy,
                    &request.context,
                    request.seen,
                )
                .map_err(|e| e.to_string())?;
                serde_json::to_vec(&serde_json::json!({
                    "status": "evaluated",
                    "operation": operation,
                    "version": 1,
                    "verdict": verdict,
                    "redacted": true,
                }))
                .map_err(|_| "serialization_failed".to_string())
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::MessageInterventionPolicies {
                operation: event_operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        CoreCommand::BatchInvocationRuntime {
            operation,
            batch_id,
            payload,
            expected_version,
            idempotency_key,
            reply,
        } => {
            let event_id = batch_id.clone();
            let event_operation = operation.clone();
            let result = async {
                if idempotency_key.is_empty() {
                    return Err("invalid_batch_idempotency_key".into());
                }
                let journal = state
                    .lock()
                    .await
                    .journal
                    .clone()
                    .ok_or_else(|| "storage journal is not configured".to_string())?;
                let database = journal.database().lock().await;
                use crate::batch_invocation_runtime as batch;
                use evohime_local_storage::batch_invocation_runtime_store as store;
                let policy = batch::default_policy();
                match operation.as_str() {
                    "create" => {
                        let request: batch::CreateBatchRequest =
                            serde_json::from_slice(&payload)
                                .map_err(|_| "invalid_batch_payload".to_string())?;
                        let value = batch::new_batch(batch::NewBatchInput {
                            id: batch_id.clone(),
                            definition_ref: request.definition_ref,
                            definition_version: request.definition_version,
                            inputs: request.inputs,
                            max_concurrency: request.max_concurrency,
                            failure_policy: request.failure_policy,
                            now_ms: crate::task_memory::now_millis() as i64,
                            policy: policy.clone(),
                        })
                        .map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&value)
                            .map_err(|_| "serialization_failed".to_string())?;
                        if !store::put(
                            database.connection(),
                            &value.id,
                            value.version,
                            &format!("{:?}", value.status),
                            &value.content_hash,
                            &json,
                            value.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("batch_duplicate".into());
                        }
                        serde_json::to_vec(&batch::projection(&value))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "get" | "resume" | "start" => {
                        let (version, json) = store::get(database.connection(), &batch_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "batch_not_found".to_string())?;
                        let mut value: batch::BatchInvocation = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_batch".to_string())?;
                        if operation == "resume" {
                            batch::resume_pending(
                                &mut value,
                                expected_version.max(version),
                                crate::task_memory::now_millis() as i64,
                                &policy,
                            )
                            .map_err(|e| e.to_string())?;
                        } else if operation == "start" {
                            batch::start_batch(
                                &mut value,
                                expected_version.max(version),
                                crate::task_memory::now_millis() as i64,
                                &policy,
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        if operation != "get" {
                            let json = serde_json::to_vec(&value)
                                .map_err(|_| "serialization_failed".to_string())?;
                            if !store::put(
                                database.connection(),
                                &value.id,
                                value.version,
                                &format!("{:?}", value.status),
                                &value.content_hash,
                                &json,
                                value.updated_at_ms,
                            )
                            .map_err(|_| "storage_failed".to_string())?
                            {
                                return Err("batch_stale_version".into());
                            }
                        }
                        serde_json::to_vec(&batch::projection(&value))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    "result" => {
                        let (version, json) = store::get(database.connection(), &batch_id)
                            .map_err(|_| "storage_failed".to_string())?
                            .ok_or_else(|| "batch_not_found".to_string())?;
                        let mut value: batch::BatchInvocation = serde_json::from_slice(&json)
                            .map_err(|_| "corrupt_batch".to_string())?;
                        let request: batch::RecordResultRequest = serde_json::from_slice(&payload)
                            .map_err(|_| "invalid_batch_result".to_string())?;
                        batch::record_result(batch::RecordResultInput {
                            batch: &mut value,
                            item_id: &request.item_id,
                            expected_version: expected_version.max(version),
                            status: request.status,
                            result_ref: request.result_ref,
                            error_class: request.error_class,
                            now_ms: crate::task_memory::now_millis() as i64,
                            policy: &policy,
                        })
                        .map_err(|e| e.to_string())?;
                        let json = serde_json::to_vec(&value)
                            .map_err(|_| "serialization_failed".to_string())?;
                        if !store::put(
                            database.connection(),
                            &value.id,
                            value.version,
                            &format!("{:?}", value.status),
                            &value.content_hash,
                            &json,
                            value.updated_at_ms,
                        )
                        .map_err(|_| "storage_failed".to_string())?
                        {
                            return Err("batch_stale_version".into());
                        }
                        serde_json::to_vec(&batch::projection(&value))
                            .map_err(|_| "serialization_failed".to_string())
                    }
                    _ => Err("unsupported_batch_operation".into()),
                }
            }
            .await;
            let projection_json = result
                .as_ref()
                .ok()
                .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
                .unwrap_or_else(|| "{}".into());
            let event = CoreEvent::BatchInvocationRuntime {
                batch_id: event_id,
                operation: event_operation,
                version: expected_version,
                projection_json,
            };
            if let Some(journal) = state.lock().await.journal.clone() {
                let _ = journal.record(&event).await;
            }
            TaskCoordinator::emit_state_event(&state, event).await;
            let _ = reply.send(result);
        }
        _ => unreachable!("command routed to the wrong coordinator domain"),
    }
}
