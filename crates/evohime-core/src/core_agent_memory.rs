use super::*;

impl ToolAgent {
    pub(super) async fn persist_lesson(&self, task_id: &str, workspace_root: &std::path::Path) {
        let Some(journal) = &self.journal else {
            return;
        };
        let Ok(metrics) = journal.tool_metrics(task_id, 256).await else {
            return;
        };
        let Some(lesson) = task_memory::build_lesson(task_id, workspace_root, &metrics) else {
            return;
        };
        let _ = journal.record_lesson(&lesson).await;
    }

    /// Runs bounded memory extraction for one finished turn.
    ///
    /// Nothing here can make the task fail: every error path writes a trace
    /// and returns. Nothing here can create active memory on its own either —
    /// the state of every produced record comes from
    /// `memory_extraction::evaluate`, and a conflict with existing active
    /// memory always downgrades the result to `pending_confirmation`.
    pub(super) async fn run_memory_extraction(
        &self,
        task_id: &str,
        workspace_root: &std::path::Path,
        user_prompt: &str,
        assistant_reply: &str,
    ) {
        use crate::memory_extraction as extraction;

        let Some(journal) = &self.journal else {
            return;
        };
        let mode = memory_extraction_mode();
        let trigger = extraction::detect_explicit_trigger(user_prompt);
        let policy = extraction::ExtractionPolicy::default();
        let now_ms = task_memory::now_millis();
        {
            let mut guard = self.extraction_guard.lock().await;
            guard.begin_turn();
            if let Err(error) = guard.check_can_extract(mode, trigger.as_ref(), now_ms, &policy) {
                write_model_trace(
                    "memory.extraction.skipped",
                    serde_json::json!({
                        "task_id": task_id,
                        "mode": mode.as_str(),
                        "reason": error.to_string(),
                    }),
                );
                return;
            }
        }

        let scope_id = task_memory::workspace_scope_id(workspace_root);
        let mut aliases = extraction::AliasTable::new();
        if let Ok(registered) = journal
            .list_memory_aliases(
                evohime_local_storage::domains::memory::MemoryScope::Project,
                &scope_id,
            )
            .await
        {
            for (alias, entity_id) in registered {
                let _ = aliases.register(&alias, &entity_id);
            }
        }

        let Some(raw_output) = self
            .call_memory_extractor(task_id, user_prompt, assistant_reply)
            .await
        else {
            return;
        };
        let candidates = match extraction::parse_extraction(&raw_output, &policy) {
            Ok(candidates) => candidates,
            Err(error) => {
                // Only the failure class is logged, never the output itself.
                self.extraction_guard
                    .lock()
                    .await
                    .register_malformed(now_ms);
                write_model_trace(
                    "memory.extraction.rejected",
                    serde_json::json!({
                        "task_id": task_id,
                        "reason": error.to_string(),
                    }),
                );
                return;
            }
        };

        for raw in &candidates {
            let (candidate, subject) = match extraction::validate_candidate(raw, &aliases, &policy)
            {
                Ok(validated) => validated,
                Err(error) => {
                    write_model_trace(
                        "memory.extraction.rejected",
                        serde_json::json!({
                            "task_id": task_id,
                            "reason": error.to_string(),
                        }),
                    );
                    continue;
                }
            };
            if self
                .extraction_guard
                .lock()
                .await
                .register_candidate(now_ms, &policy)
                .is_err()
            {
                break;
            }
            // A model cannot vouch for itself: source trust is only `user`
            // when this turn actually carried an explicit user assertion.
            let context = extraction::TurnContext {
                mode,
                trigger: trigger.clone(),
                user_asserted: trigger.is_some(),
            };
            let mut decision = extraction::evaluate(&candidate, &context, &subject, &policy);
            if decision.outcome == extraction::PolicyOutcome::Reject {
                write_model_trace(
                    "memory.extraction.rejected",
                    serde_json::json!({
                        "task_id": task_id,
                        "kind": candidate.kind.as_str(),
                        "reason": decision.reason.as_str(),
                    }),
                );
                continue;
            }

            let store_scope = match candidate.scope {
                extraction::MemoryScopeLevel::Task => {
                    evohime_local_storage::domains::memory::MemoryScope::Task
                }
                extraction::MemoryScopeLevel::Workspace => {
                    evohime_local_storage::domains::memory::MemoryScope::Workspace
                }
                extraction::MemoryScopeLevel::Session => {
                    evohime_local_storage::domains::memory::MemoryScope::Session
                }
                extraction::MemoryScopeLevel::Project => {
                    evohime_local_storage::domains::memory::MemoryScope::Project
                }
            };

            // Session-only results never create a persistent row.
            if decision.session_only {
                let expires_at = now_ms.saturating_add(extraction::SESSION_SUMMARY_GRACE_MS);
                let _ = journal
                    .save_memory_session_note(SessionMemoryNote {
                        id: &uuid::Uuid::new_v4().to_string(),
                        session_id: task_id,
                        scope: store_scope,
                        scope_id: &scope_id,
                        kind: candidate.kind.as_str(),
                        statement: &candidate.statement,
                        created_at: &now_ms.to_string(),
                        expires_at: &expires_at.to_string(),
                    })
                    .await;
                continue;
            }

            // An unresolved conflict never overwrites the active record: the
            // candidate waits for an explicit user choice instead.
            let active = journal
                .memory_conflict_candidates(store_scope, &scope_id, candidate.kind.as_str(), 100)
                .await
                .unwrap_or_default();
            let summaries = active
                .iter()
                .filter_map(memory_active_summary)
                .collect::<Vec<_>>();
            let conflict = extraction::detect_conflict(&candidate, &summaries);
            match conflict {
                extraction::ConflictVerdict::Duplicate { .. } => {
                    write_model_trace(
                        "memory.extraction.duplicate",
                        serde_json::json!({
                            "task_id": task_id,
                            "subject": candidate.canonical_subject,
                        }),
                    );
                    continue;
                }
                extraction::ConflictVerdict::Conflict { .. } => {
                    decision.outcome = extraction::PolicyOutcome::Pending;
                    decision.state = extraction::ConfirmationState::PendingConfirmation;
                }
                extraction::ConflictVerdict::None => {}
            }

            let Ok(provenance) = candidate.evidence.to_provenance_json() else {
                continue;
            };
            let Ok(mut record) = evohime_local_storage::domains::memory::MemoryRecord::new(
                evohime_local_storage::domains::memory::MemoryRecordInput {
                    id: uuid::Uuid::new_v4().to_string(),
                    scope: store_scope,
                    scope_id: scope_id.clone(),
                    title: candidate.raw_subject.clone(),
                    content: candidate.statement.clone(),
                    provenance,
                    privacy: evohime_local_storage::domains::memory::MemoryPrivacy::Private,
                    created_at: now_ms.to_string(),
                    expires_at: Some(now_ms.saturating_add(decision.ttl_ms).to_string()),
                },
            ) else {
                continue;
            };
            // Verification runs before persistence so the stored record
            // already carries an honest validation status; `invalid` and
            // `unknown` both keep it out of retrieval.
            let verdict = self.verify_candidate(workspace_root, &candidate).await;
            record.extraction = evohime_local_storage::domains::memory::MemoryExtractionFields {
                record_version: 1,
                evidence_refs: memory_provenance_source_id(&candidate.evidence)
                    .into_iter()
                    .collect(),
                execution_event_refs: Vec::new(),
                kind: candidate.kind.as_str().to_owned(),
                canonical_subject: Some(candidate.canonical_subject.clone()),
                confirmation_state: decision.state.as_str().to_owned(),
                model_confidence: candidate.model_confidence,
                // Raised only by the versioned verification policy.
                verification_confidence: verdict
                    .as_ref()
                    .map(|verdict| verdict.verification_confidence)
                    .unwrap_or(0.0),
                privacy_class: candidate.privacy.as_str().to_owned(),
                source_trust: candidate.source_trust.as_str().to_owned(),
                supersedes: None,
                superseded_by: None,
                supersession_reason: None,
                extractor_version: decision.extractor_version.to_owned(),
                policy_version: decision.policy_version.to_owned(),
                validation_status: verdict
                    .as_ref()
                    .map(|verdict| verdict.status.as_str().to_owned())
                    .unwrap_or_else(|| decision.validation_status.as_str().to_owned()),
                validated_at: verdict
                    .as_ref()
                    .map(|verdict| verdict.validated_at_ms.to_string()),
                provenance_source_id: memory_provenance_source_id(&candidate.evidence),
                authority: "model_proposed".to_owned(),
                durability: "durable".to_owned(),
                confidence: verdict
                    .as_ref()
                    .map(|verdict| verdict.verification_confidence)
                    .unwrap_or(0.0),
            };
            if let Err(error) = journal.save_memory(&record).await {
                write_model_trace(
                    "memory.extraction.rejected",
                    serde_json::json!({ "task_id": task_id, "reason": error }),
                );
                continue;
            }
            write_model_trace(
                "memory.extraction.candidate",
                serde_json::json!({
                    "task_id": task_id,
                    "memory_id": record.id,
                    "kind": candidate.kind.as_str(),
                    "state": decision.state.as_str(),
                    "risk": decision.risk.as_str(),
                    "reason": decision.reason.as_str(),
                    "policy_version": decision.policy_version,
                    "extractor_version": decision.extractor_version,
                }),
            );
        }
    }

    /// Runs bounded memory extraction for one closed ambient episode (04.6).
    ///
    /// This is a separate entry point on purpose. `run_memory_extraction`
    /// takes the pair (user prompt, assistant reply) of one finished turn, and
    /// passing heard speech as the user's half would quietly turn
    /// `user_asserted` into a lie. The policy gate below is the same one; only
    /// the way into it is different, and it is strictly stricter: an ambient
    /// candidate can never auto-confirm.
    pub(super) async fn run_ambient_memory_extraction(&self, episode_id: &str) {
        use crate::memory_extraction as extraction;

        let Some(journal) = &self.journal else {
            return;
        };
        if episode_id.trim().is_empty() {
            return;
        }
        // The general switch outranks the specific one: with extraction off
        // entirely, ambient does not run at all, whatever
        // `EVOHIME_AMBIENT_MEMORY` says. This is checked here, before
        // `evaluate`, because the ambient gate inside `evaluate` stands above
        // the `ExtractionDisabled` branch and would otherwise let it through.
        let mode = memory_extraction_mode();
        let ambient_mode = ambient_memory_mode();
        let policy = extraction::ExtractionPolicy::default();
        let now_ms = task_memory::now_millis();
        {
            let mut guard = self.extraction_guard.lock().await;
            if let Err(error) = guard.check_can_extract_ambient(ambient_mode, mode, now_ms, &policy)
            {
                write_model_trace(
                    "memory.ambient.skipped",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "mode": mode.as_str(),
                        "ambient_mode": ambient_mode.as_str(),
                        "reason": error.to_string(),
                    }),
                );
                return;
            }
            if let Err(error) = guard.register_ambient_episode(now_ms, &policy) {
                write_model_trace(
                    "memory.ambient.skipped",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "reason": error.to_string(),
                    }),
                );
                drop(guard);
                let _ = journal
                    .set_ambient_extraction_state(
                        episode_id,
                        evohime_listener_contract::ExtractionState::Failed,
                    )
                    .await;
                return;
            }
        }
        let _ = journal
            .set_ambient_extraction_state(
                episode_id,
                evohime_listener_contract::ExtractionState::Pending,
            )
            .await;

        let Some(context) = self.ambient_episode_context(episode_id).await else {
            // An empty or fully redacted episode has nothing to extract; that
            // is a finished episode, not a failed one.
            let _ = journal
                .set_ambient_extraction_state(
                    episode_id,
                    evohime_listener_contract::ExtractionState::Done,
                )
                .await;
            return;
        };

        let mut aliases = extraction::AliasTable::new();
        if let Ok(registered) = journal
            .list_memory_aliases(
                evohime_local_storage::domains::memory::MemoryScope::Workspace,
                AMBIENT_MEMORY_SCOPE_ID,
            )
            .await
        {
            for (alias, entity_id) in registered {
                let _ = aliases.register(&alias, &entity_id);
            }
        }

        let Some(raw_output) = self
            .call_extractor(episode_id, AMBIENT_MEMORY_EXTRACTION_PROMPT, context, true)
            .await
        else {
            let _ = journal
                .set_ambient_extraction_state(
                    episode_id,
                    evohime_listener_contract::ExtractionState::Failed,
                )
                .await;
            return;
        };
        let candidates = match extraction::parse_extraction(&raw_output, &policy) {
            Ok(candidates) => candidates,
            Err(error) => {
                // The breaker is shared with the dialog path: a malformed
                // extractor is equally broken whichever text it was given.
                self.extraction_guard
                    .lock()
                    .await
                    .register_malformed(now_ms);
                write_model_trace(
                    "memory.ambient.rejected",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "reason": error.to_string(),
                    }),
                );
                let _ = journal
                    .set_ambient_extraction_state(
                        episode_id,
                        evohime_listener_contract::ExtractionState::Failed,
                    )
                    .await;
                return;
            }
        };

        for raw in &candidates {
            let Ok((mut candidate, subject)) =
                extraction::validate_candidate(raw, &aliases, &policy)
            else {
                continue;
            };
            // Trust is decided by where the text came from, not by what the
            // model claims about itself.
            candidate.source_trust = extraction::SourceTrust::Ambient;
            // The locator is rebuilt rather than trusted: the episode is the
            // only provenance heard speech has, and `content_hash` stays empty
            // because the hash of a short phrase is the phrase (04.1).
            candidate.evidence = extraction::RawEvidenceLocator {
                episode_id: episode_id.to_owned(),
                ..extraction::RawEvidenceLocator::default()
            };
            // Speech at the desk belongs to no repository, so claiming a
            // project or task scope for it would be an invention.
            candidate.scope = extraction::MemoryScopeLevel::Workspace;
            if !extraction::ambient_kind_allowed(candidate.kind) {
                write_model_trace(
                    "memory.ambient.rejected",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "kind": candidate.kind.as_str(),
                        "reason": "kind_not_allowed_from_ambient",
                    }),
                );
                // 04.6 отбрасывает `constraint` и `decision` до persistence
                // именно потому, что они влияют на действия. 04.7 не
                // воскрешает их как память: они становятся ограниченным
                // предложением, которое само по себе ничего не делает и ждёт
                // клика. Потолок, mute и закрытый список эффектов проверяются
                // внутри.
                self.propose_from_ambient(episode_id, &candidate).await;
                continue;
            }
            let raised = extraction::apply_ambient_privacy_floor(&mut candidate);
            if self
                .extraction_guard
                .lock()
                .await
                .register_ambient_candidate(now_ms, &policy)
                .is_err()
            {
                break;
            }
            let context = extraction::TurnContext::ambient(mode);
            let mut decision = extraction::evaluate(&candidate, &context, &subject, &policy);
            if decision.outcome == extraction::PolicyOutcome::Reject {
                write_model_trace(
                    "memory.ambient.rejected",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "kind": candidate.kind.as_str(),
                        "reason": decision.reason.as_str(),
                    }),
                );
                continue;
            }
            // Belt and braces: `evaluate` cannot return `AutoConfirm` for an
            // ambient candidate, and if it ever did, persistence would still
            // not be the place to find out.
            if decision.outcome == extraction::PolicyOutcome::AutoConfirm {
                decision.outcome = extraction::PolicyOutcome::Pending;
                decision.state = extraction::ConfirmationState::PendingConfirmation;
                decision.reason = extraction::PolicyReason::AmbientNeverAutoConfirms;
            }

            let store_scope = evohime_local_storage::domains::memory::MemoryScope::Workspace;
            let active = journal
                .memory_conflict_candidates(
                    store_scope,
                    AMBIENT_MEMORY_SCOPE_ID,
                    candidate.kind.as_str(),
                    100,
                )
                .await
                .unwrap_or_default();
            let summaries = active
                .iter()
                .filter_map(memory_active_summary)
                .collect::<Vec<_>>();
            if let extraction::ConflictVerdict::Duplicate { .. } =
                extraction::detect_conflict(&candidate, &summaries)
            {
                write_model_trace(
                    "memory.ambient.duplicate",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "subject": candidate.canonical_subject,
                    }),
                );
                continue;
            }

            let Ok(provenance) = candidate.evidence.to_provenance_json() else {
                continue;
            };
            let Ok(mut record) = evohime_local_storage::domains::memory::MemoryRecord::new(
                evohime_local_storage::domains::memory::MemoryRecordInput {
                    id: uuid::Uuid::new_v4().to_string(),
                    scope: store_scope,
                    scope_id: AMBIENT_MEMORY_SCOPE_ID.to_owned(),
                    title: candidate.raw_subject.clone(),
                    content: candidate.statement.clone(),
                    provenance,
                    privacy: evohime_local_storage::domains::memory::MemoryPrivacy::Private,
                    created_at: now_ms.to_string(),
                    expires_at: Some(now_ms.saturating_add(decision.ttl_ms).to_string()),
                },
            ) else {
                continue;
            };
            record.extraction = evohime_local_storage::domains::memory::MemoryExtractionFields {
                record_version: 1,
                evidence_refs: memory_provenance_source_id(&candidate.evidence)
                    .into_iter()
                    .collect(),
                execution_event_refs: Vec::new(),
                kind: candidate.kind.as_str().to_owned(),
                canonical_subject: Some(candidate.canonical_subject.clone()),
                confirmation_state: decision.state.as_str().to_owned(),
                model_confidence: candidate.model_confidence,
                verification_confidence: 0.0,
                privacy_class: candidate.privacy.as_str().to_owned(),
                source_trust: candidate.source_trust.as_str().to_owned(),
                supersedes: None,
                superseded_by: None,
                supersession_reason: None,
                extractor_version: decision.extractor_version.to_owned(),
                policy_version: decision.policy_version.to_owned(),
                // Heard speech has no validator: no file to re-read, no tool
                // call to replay, and no verified speaker. `unknown` is the
                // honest answer, and it keeps the record out of retrieval.
                validation_status: extraction::ValidationStatus::Unknown.as_str().to_owned(),
                validated_at: None,
                provenance_source_id: memory_provenance_source_id(&candidate.evidence),
                authority: "model_proposed".to_owned(),
                durability: "durable".to_owned(),
                confidence: 0.0,
            };
            if let Err(error) = journal.save_memory(&record).await {
                write_model_trace(
                    "memory.ambient.rejected",
                    serde_json::json!({ "episode_id": episode_id, "reason": error }),
                );
                continue;
            }
            write_model_trace(
                "memory.ambient.candidate",
                serde_json::json!({
                    "episode_id": episode_id,
                    "memory_id": record.id,
                    "kind": candidate.kind.as_str(),
                    "state": decision.state.as_str(),
                    "risk": decision.risk.as_str(),
                    "reason": decision.reason.as_str(),
                    "privacy_raised": raised,
                    "policy_version": decision.policy_version,
                    "extractor_version": decision.extractor_version,
                }),
            );
        }
        let _ = journal
            .set_ambient_extraction_state(
                episode_id,
                evohime_listener_contract::ExtractionState::Done,
            )
            .await;
    }

    /// Превращает услышанное действие в ограниченное предложение (04.7).
    ///
    /// Всё, что здесь может произойти, — появление карточки в очереди и
    /// строка `ambient.proposal` в журнале. Ни задачи, ни инструмента, ни
    /// файла, ни сети: закрытый список эффектов проверяется до любого
    /// эффекта, и запрещённому эффекту просто нечего вернуть.
    ///
    /// Превышение потолка **отбрасывает** предложение со счётчиком в трассе,
    /// а не ставит его в очередь: иначе после часа тишины пользователь
    /// получил бы десять карточек разом.
    pub(super) async fn propose_from_ambient(
        &self,
        episode_id: &str,
        candidate: &crate::memory_extraction::Candidate,
    ) {
        use crate::ambient_proactivity as proactivity;
        use evohime_local_storage::ambient_store::ProposalInsert;

        let (Some(journal), Some(registry)) = (self.journal.as_ref(), self.proactivity.as_ref())
        else {
            return;
        };
        let Some(kind) = ambient_proposal_kind(candidate.kind) else {
            return;
        };
        if candidate.statement.trim().is_empty() {
            return;
        }
        let now_ms = task_memory::now_millis();
        let subject_key = proactivity::subject_key(&candidate.canonical_subject);
        let mute_key = proactivity::mute_key(kind, &subject_key);
        let proposal_key = proactivity::proposal_key(kind, &subject_key, now_ms);

        let authorized = match registry.decide(journal, kind, &mute_key, now_ms).await {
            Ok(authorized) => authorized,
            Err(rejection) => {
                write_model_trace(
                    "ambient.proposal.dropped",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "kind": kind.as_str(),
                        "reason": rejection.as_str(),
                    }),
                );
                return;
            }
        };
        debug_assert!(
            authorized.effect().is_proactively_allowed(),
            "авторизованным может быть только эффект из закрытого списка"
        );

        let proposal_id = uuid::Uuid::new_v4().to_string();
        let record = crate::ambient::proposal_record(crate::ambient::ProposalRecordInput {
            proposal_id: &proposal_id,
            proposal_key: &proposal_key,
            mute_key: &mute_key,
            kind,
            subject_key: &subject_key,
            subject: &candidate.canonical_subject,
            title: &candidate.statement,
            source_episode_id: Some(episode_id),
            now_ms,
        });
        match journal.record_ambient_proposal(&record).await {
            Ok(ProposalInsert::Created) => {
                // Счётчик поднимается только после появления карточки:
                // отброшенное хранилищем предложение не должно съедать час.
                registry.commit(journal, now_ms).await;
                let Ok(typed_id) = evohime_listener_contract::ProposalId::new(proposal_id.clone())
                else {
                    return;
                };
                let _ = registry
                    .publish(
                        journal,
                        &evohime_listener_contract::AmbientLogEvent::Proposal {
                            proposal_id: typed_id,
                            episode_id: evohime_listener_contract::EpisodeId::new(
                                episode_id.to_owned(),
                            )
                            .ok(),
                            kind,
                            subject_key: subject_key.clone(),
                            proposal_state: evohime_listener_contract::ProposalState::Proposed,
                        },
                    )
                    .await;
                write_model_trace(
                    "ambient.proposal.created",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "proposal_id": proposal_id,
                        "kind": kind.as_str(),
                        "subject_key": subject_key.as_str(),
                    }),
                );
            }
            Ok(ProposalInsert::Duplicate {
                proposal_id,
                occurrences,
            }) => {
                // Бюджет не тратится: второй карточки не появилось.
                write_model_trace(
                    "ambient.proposal.duplicate",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "proposal_id": proposal_id,
                        "occurrences": occurrences,
                    }),
                );
            }
            Ok(ProposalInsert::Muted) => {
                write_model_trace(
                    "ambient.proposal.dropped",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "kind": kind.as_str(),
                        "reason": "muted",
                    }),
                );
            }
            Err(code) => {
                write_model_trace(
                    "ambient.proposal.dropped",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "kind": kind.as_str(),
                        "reason": code.as_str(),
                    }),
                );
            }
        }
    }

    /// Builds the bounded extractor context of one episode.
    ///
    /// Redacted utterances are skipped rather than sent as holes: a record
    /// that the policy already withheld must not reach the extractor through
    /// the back door. `None` means there is nothing to extract from.
    pub(super) async fn ambient_episode_context(&self, episode_id: &str) -> Option<String> {
        use crate::memory_extraction as extraction;

        let journal = self.journal.as_ref()?;
        let records = journal
            .list_ambient_utterances(episode_id, 500)
            .await
            .ok()?;
        let text = records
            .iter()
            .filter(|record| !record.redacted)
            .map(|record| record.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if text.trim().is_empty() {
            return None;
        }
        let budget_chars = extraction::MAX_CONTEXT_TOKENS * 4;
        Some(truncate_chars(
            &format!("Эпизод {episode_id}. Услышанная речь:\n{text}"),
            budget_chars,
        ))
    }

    /// Runs the verification hook for one candidate and returns the verdict
    /// the versioned verification policy produced. A timeout, an unreadable
    /// file or a missing validator yields `unknown`, which keeps the record
    /// pending rather than confirming or rejecting it. One retry, as the plan
    /// specifies; a failing validator never fails the task.
    pub(super) async fn verify_candidate(
        &self,
        workspace_root: &std::path::Path,
        candidate: &crate::memory_extraction::Candidate,
    ) -> Option<crate::memory_extraction::VerificationVerdict> {
        use crate::memory_extraction as extraction;

        let target = extraction::validation_target(candidate)?;
        let policy = extraction::ExtractionPolicy::default();
        let expected = candidate.evidence.content_hash.clone();
        let mut outcome = None;
        for _ in 0..2 {
            let actual = match target {
                extraction::ValidationTarget::Filesystem => {
                    if candidate.source_trust == extraction::SourceTrust::Document {
                        match (&self.journal, expected.trim()) {
                            (Some(journal), chunk_hash) if !chunk_hash.is_empty() => timeout(
                                Duration::from_millis(target.timeout_ms()),
                                journal.verify_workspace_document_provenance(
                                    workspace_root,
                                    &candidate.evidence.file_path,
                                    chunk_hash,
                                ),
                            )
                            .await
                            .ok()
                            .and_then(Result::ok)
                            .filter(|valid| *valid)
                            .map(|_| chunk_hash.to_string()),
                            _ => None,
                        }
                    } else {
                        let path = workspace_root.join(&candidate.evidence.file_path);
                        match timeout(
                            Duration::from_millis(target.timeout_ms()),
                            tokio::fs::read(path),
                        )
                        .await
                        {
                            Ok(Ok(bytes)) => Some(crate::research::sha256_hex(&bytes)),
                            _ => None,
                        }
                    }
                }
                // Tool/API validation still has no authoritative replayable
                // source in Local Agentic RAG v1, so it remains unknown.
                extraction::ValidationTarget::Tool => None,
            };
            let candidate_outcome = extraction::file_evidence_outcome(
                &expected,
                actual.as_deref(),
                task_memory::now_millis(),
            );
            let resolved = candidate_outcome.valid.is_some();
            outcome = Some(candidate_outcome);
            if resolved {
                break;
            }
        }
        outcome.map(|outcome| extraction::apply_verification(&outcome, &policy))
    }

    /// One bounded extraction call: no tools, no provider secrets, context
    /// limited to the current exchange, and at most two retries. Returns
    /// `None` when the model is unavailable — the task continues without
    /// memory.
    pub(super) async fn call_memory_extractor(
        &self,
        task_id: &str,
        user_prompt: &str,
        assistant_reply: &str,
    ) -> Option<String> {
        use crate::memory_extraction as extraction;

        let budget_chars = extraction::MAX_CONTEXT_TOKENS * 4;
        let context = truncate_chars(
            &format!("Пользователь: {user_prompt}\nАгент: {assistant_reply}"),
            budget_chars,
        );
        self.call_extractor(task_id, MEMORY_EXTRACTION_PROMPT, context, false)
            .await
    }

    /// The shared half of both extractor calls. `ambient` selects which
    /// hourly token budget the spent tokens are charged to: ambient has its
    /// own, so a talkative room cannot eat the dialog budget.
    pub(super) async fn call_extractor(
        &self,
        task_id: &str,
        system_prompt: &str,
        context: String,
        ambient: bool,
    ) -> Option<String> {
        use crate::memory_extraction as extraction;

        // Auxiliary extraction is a model request too. Until it has a
        // ledger-backed checkpoint (the dialog path below has one), a
        // storage-backed Core refuses the dispatch instead of leaking an
        // unrecorded prompt. In-memory/unit-test agents retain their legacy
        // behavior because they have no durable provenance owner.
        if self.journal.is_some() {
            write_model_trace(
                "memory.extraction.provenance_required",
                serde_json::json!({ "task_id": task_id, "ambient": ambient }),
            );
            return None;
        }

        let messages = vec![
            ChatMessage::text(ChatRole::System, system_prompt.to_string()),
            ChatMessage::text(ChatRole::User, context),
        ];
        let model = std::env::var("EVOHIME_MEMORY_EXTRACTION_MODEL")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let routing_request = RoutingRequest {
            required_capabilities: vec!["chat".into()],
            max_cost_micros_per_1k_tokens: None,
            max_latency_ms: None,
            required_privacy: PrivacyClass::Internal,
            allow_fallback: true,
            preferred_route: None,
            task_class: None,
            offline: false,
            allow_cloud: true,
            estimated_input_tokens: 0,
            quality_delta: 0.05,
        };
        for attempt in 0..=extraction::RETRY_DELAYS_MS.len() {
            if attempt > 0 {
                if let Some(delay) = extraction::ExtractionGuard::retry_delay_ms(attempt - 1) {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
            let call = self.gateway.chat_with_tools_with_policy(
                RoutingMode::Balanced,
                &routing_request,
                model.as_deref(),
                &messages,
                &[],
            );
            match timeout(Duration::from_secs(20), call).await {
                Ok(Ok(result)) => {
                    let tokens = (context_token_estimate(&messages)
                        + result.content.chars().count().div_ceil(4))
                        as u64;
                    let now_ms = task_memory::now_millis();
                    let mut guard = self.extraction_guard.lock().await;
                    // Ambient tokens are charged to their own hourly
                    // budget: a talkative room must not spend the budget
                    // the dialog path lives on.
                    if ambient {
                        guard.register_ambient_tokens(now_ms, tokens);
                    } else {
                        guard.register_tokens(now_ms, tokens);
                    }
                    drop(guard);
                    return Some(result.content);
                }
                Ok(Err(error)) => {
                    write_model_trace(
                        "memory.extraction.provider_error",
                        serde_json::json!({
                            "task_id": task_id,
                            "attempt": attempt + 1,
                            "error": error.to_string(),
                        }),
                    );
                }
                Err(_) => {
                    write_model_trace(
                        "memory.extraction.provider_error",
                        serde_json::json!({
                            "task_id": task_id,
                            "attempt": attempt + 1,
                            "error": "timeout",
                        }),
                    );
                }
            }
        }
        None
    }
}
