use super::*;

const MAX_MEMORY_VALIDATION_FILE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct PreparedMemoryExtractionSource {
    pub source_id: String,
    pub source_basis: String,
    pub origin: evohime_local_storage::domains::memory::MemoryExtractionOrigin,
    pub scope_id: String,
    pub primary_request_id: Option<String>,
    pub primary_response_id: Option<String>,
}

struct MemoryExtractionDiagnostic<'a> {
    task_id: &'a str,
    source_id: Option<&'a str>,
    origin: &'a str,
    stage: &'a str,
    status: &'a str,
    reason_code: Option<&'a str>,
    backlog: usize,
    conflict_count: usize,
    suppressed_reentry: bool,
}

struct ProvenancedExtractorContext {
    ambient: bool,
    generation: i64,
}

type MemoryExtractorResult = Result<Option<String>, &'static str>;

async fn emit_memory_extraction_diagnostic(
    events: &crate::EventSink,
    diagnostic: MemoryExtractionDiagnostic<'_>,
) {
    fn bounded_identifier(value: &str) -> Option<String> {
        (!value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-')))
        .then(|| value.to_owned())
    }

    const STAGES: &[&str] = &[
        "source",
        "extractor",
        "candidate",
        "finalization",
        "recovery",
    ];
    const STATUSES: &[&str] = &[
        "attempted",
        "skipped",
        "captured",
        "rejected",
        "duplicate",
        "conflict",
        "superseded",
        "deferred",
        "recovered",
        "committed",
        "stale",
        "failed",
    ];
    const ORIGINS: &[&str] = &["dialog", "ambient", "recovery"];
    let Some(task_id) = bounded_identifier(diagnostic.task_id) else {
        return;
    };
    if !STAGES.contains(&diagnostic.stage)
        || !STATUSES.contains(&diagnostic.status)
        || !ORIGINS.contains(&diagnostic.origin)
    {
        return;
    }
    let reason_code = diagnostic
        .reason_code
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
        .map(str::to_owned);
    let source_id = diagnostic.source_id.and_then(bounded_identifier);
    let event = crate::CoreEvent::MemoryExtractionDiagnostic {
        task_id,
        source_id,
        origin: diagnostic.origin.to_owned(),
        stage: diagnostic.stage.to_owned(),
        status: diagnostic.status.to_owned(),
        reason_code,
        backlog: diagnostic.backlog.min(1_000_000) as u32,
        conflict_count: diagnostic.conflict_count.min(1_000_000) as u32,
        suppressed_reentry_count: if diagnostic.suppressed_reentry { 1 } else { 0 },
    };
    let _ = events.send(event).await;
}

fn extraction_digest(domain: &[u8], parts: &[&str]) -> String {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    digest.update(domain);
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("sha256:{}", hex::encode(digest.finalize()))
}

impl ToolAgent {
    /// Durably references one completed dialog source before Core reports the
    /// primary turn complete. The source body remains in the existing journal.
    pub(super) async fn capture_dialog_memory_source(
        &self,
        task_id: &str,
        workspace_root: &std::path::Path,
        user_prompt: &str,
        assistant_reply: &str,
        primary_model_refs: Option<(&str, &str)>,
        events: &crate::EventSink,
    ) -> Option<PreparedMemoryExtractionSource> {
        use crate::memory_extraction as extraction;

        let journal = self.journal.as_ref()?;
        let mode = memory_extraction_mode();
        let trigger = extraction::detect_explicit_trigger(user_prompt);
        let policy = extraction::ExtractionPolicy::default();
        let now_ms = task_memory::now_millis();
        let eligibility = self.extraction_guard.lock().await.check_can_extract(
            mode,
            trigger.as_ref(),
            now_ms,
            &policy,
        );
        if let Err(error) = eligibility {
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id,
                    source_id: None,
                    origin: "dialog",
                    stage: "source",
                    status: "skipped",
                    reason_code: Some("policy_suppressed"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                },
            )
            .await;
            write_model_trace(
                "memory.extraction.skipped",
                serde_json::json!({
                    "task_id": task_id,
                    "mode": mode.as_str(),
                    "reason": error.to_string(),
                }),
            );
            return None;
        }
        emit_memory_extraction_diagnostic(
            events,
            MemoryExtractionDiagnostic {
                task_id,
                source_id: None,
                origin: "dialog",
                stage: "source",
                status: "attempted",
                reason_code: None,
                backlog: 0,
                conflict_count: 0,
                suppressed_reentry: false,
            },
        )
        .await;
        let revision_hash = extraction_digest(
            b"evohime-memory-source-revision:v1\0",
            &[user_prompt, assistant_reply],
        );
        let request_ref = primary_model_refs.map(|(request_id, _)| request_id.to_owned());
        let response_ref = primary_model_refs.map(|(_, response_id)| response_id.to_owned());
        let source_basis = extraction_digest(
            b"evohime-memory-source-basis:v1\0",
            &[
                "dialog",
                task_id,
                &revision_hash,
                request_ref.as_deref().unwrap_or_default(),
                response_ref.as_deref().unwrap_or_default(),
            ],
        );
        let input = evohime_local_storage::domains::memory::CaptureSourceInput {
            source_id: source_basis.clone(),
            source_basis: source_basis.clone(),
            source_ref_id: task_id.to_owned(),
            source_revision_hash: revision_hash,
            scope_id: task_memory::workspace_scope_id(workspace_root),
            origin: evohime_local_storage::domains::memory::MemoryExtractionOrigin::Dialog,
            root_execution_id: task_id.to_owned(),
            depth: 0,
            source_order: now_ms as i64,
            primary_request_id: request_ref.clone(),
            primary_response_id: response_ref.clone(),
            now_ms: now_ms as i64,
        };
        match journal.capture_memory_extraction_source(&input).await {
            Ok(evohime_local_storage::domains::memory::CaptureSourceOutcome::Captured {
                source_id,
            }) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id,
                    source_id: Some(&source_id),
                    origin: "dialog",
                    stage: "source",
                    status: "captured",
                    reason_code: None,
                    backlog: 1,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                Some(PreparedMemoryExtractionSource {
                    source_id,
                    source_basis,
                    origin: evohime_local_storage::domains::memory::MemoryExtractionOrigin::Dialog,
                    scope_id: input.scope_id.clone(),
                    primary_request_id: request_ref,
                    primary_response_id: response_ref,
                })
            }
            Ok(evohime_local_storage::domains::memory::CaptureSourceOutcome::AlreadyCaptured {
                source_id,
                state,
            }) if state != evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id,
                    source_id: Some(&source_id),
                    origin: "dialog",
                    stage: "source",
                    status: "recovered",
                    reason_code: None,
                    backlog: 1,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                Some(PreparedMemoryExtractionSource {
                    source_id,
                source_basis,
                origin: evohime_local_storage::domains::memory::MemoryExtractionOrigin::Dialog,
                scope_id: input.scope_id.clone(),
                    primary_request_id: request_ref,
                    primary_response_id: response_ref,
                })
            }
            Ok(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id,
                    source_id: None,
                    origin: "dialog",
                    stage: "source",
                    status: "skipped",
                    reason_code: Some("source_already_committed"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                None
            }
            Err(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id,
                    source_id: None,
                    origin: "dialog",
                    stage: "source",
                    status: "failed",
                    reason_code: Some("source_capture_failed"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                write_model_trace(
                    "memory.extraction.suppressed",
                    serde_json::json!({
                        "task_id": task_id,
                        "error_code": "source_capture_failed",
                        "operation": "dialog",
                    }),
                );
                None
            }
        }
    }

    /// Recovers one bounded page of durable extraction sources after startup.
    /// Dialogs are rehydrated from their existing task event records; ambient
    /// work reuses its retained episode source. Unsupported origins fail
    /// closed instead of being replayed as new user evidence.
    pub(super) async fn recover_pending_memory_extractions(&self, events: &crate::EventSink) {
        use crate::memory_extraction as extraction;

        let Some(journal) = &self.journal else {
            return;
        };
        let now_ms = task_memory::now_millis() as i64;
        let sources = match journal
            .list_recoverable_memory_extraction_sources(
                now_ms,
                evohime_local_storage::domains::memory::MAX_RECOVERABLE_SOURCES,
            )
            .await
        {
            Ok(sources) => sources,
            Err(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: "memory-extraction-recovery",
                        source_id: None,
                        origin: "recovery",
                        stage: "recovery",
                        status: "failed",
                        reason_code: Some("recovery_scan_failed"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.extraction.recovery_failed",
                    serde_json::json!({"error_code":"recovery_scan_failed"}),
                );
                return;
            }
        };
        if !sources.is_empty() {
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id: "memory-extraction-recovery",
                    source_id: None,
                    origin: "recovery",
                    stage: "recovery",
                    status: "attempted",
                    reason_code: None,
                    backlog: sources.len(),
                    conflict_count: 0,
                    suppressed_reentry: false,
                },
            )
            .await;
        }
        for source in sources {
            if source.depth != 0 || source.root_execution_id != source.source_ref_id {
                self.finish_recovered_memory_source(
                    &source,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                    "recovery_source_lineage_invalid",
                    events,
                )
                .await;
                continue;
            }
            if source.origin
                == evohime_local_storage::domains::memory::MemoryExtractionOrigin::Ambient
            {
                self.run_ambient_memory_extraction(&source.source_ref_id, events)
                    .await;
                continue;
            }
            if source.origin
                != evohime_local_storage::domains::memory::MemoryExtractionOrigin::Dialog
            {
                self.finish_recovered_memory_source(
                    &source,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                    "recovery_origin_unsupported",
                    events,
                )
                .await;
                continue;
            }
            let Some(_lease) =
                extraction::ExtractionLease::try_acquire(Arc::clone(&self.extraction_lease))
            else {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: &source.source_ref_id,
                        source_id: Some(&source.source_id),
                        origin: "dialog",
                        stage: "recovery",
                        status: "deferred",
                        reason_code: Some("lease_busy"),
                        backlog: 1,
                        conflict_count: 0,
                        suppressed_reentry: true,
                    },
                )
                .await;
                continue;
            };
            let (started_at, prompt) = match journal
                .memory_extraction_dialog_prompt(&source.source_ref_id)
                .await
            {
                Ok(Some(prompt)) => prompt,
                Ok(None) => {
                    self.finish_recovered_memory_source(
                        &source,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        "recovery_source_incomplete",
                        events,
                    )
                    .await;
                    continue;
                }
                Err(error_code) => {
                    self.finish_recovered_memory_source(
                        &source,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        &error_code,
                        events,
                    )
                    .await;
                    continue;
                }
            };
            let mode = memory_extraction_mode();
            let trigger = extraction::detect_explicit_trigger(&prompt);
            let policy = extraction::ExtractionPolicy::default();
            let eligibility = self.extraction_guard.lock().await.check_can_extract(
                mode,
                trigger.as_ref(),
                task_memory::now_millis(),
                &policy,
            );
            if eligibility.is_err() {
                self.finish_recovered_memory_source(
                    &source,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                    "policy_suppressed",
                    events,
                )
                .await;
                continue;
            }
            let (completed_at, reply) = match journal
                .memory_extraction_dialog_reply(&source.source_ref_id)
                .await
            {
                Ok(Some(reply)) => reply,
                Ok(None) => {
                    self.finish_recovered_memory_source(
                        &source,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        "recovery_source_incomplete",
                        events,
                    )
                    .await;
                    continue;
                }
                Err(error_code) => {
                    self.finish_recovered_memory_source(
                        &source,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        &error_code,
                        events,
                    )
                    .await;
                    continue;
                }
            };
            if started_at >= completed_at {
                self.finish_recovered_memory_source(
                    &source,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                    "task_source_mismatch",
                    events,
                )
                .await;
                continue;
            }
            let recovered_revision =
                extraction_digest(b"evohime-memory-source-revision:v1\0", &[&prompt, &reply]);
            if recovered_revision != source.source_revision_hash {
                self.finish_recovered_memory_source(
                    &source,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Stale,
                    "recovery_source_revision_mismatch",
                    events,
                )
                .await;
                continue;
            }
            self.run_memory_extraction_inner(
                &source.source_ref_id,
                None,
                &prompt,
                &reply,
                Some(PreparedMemoryExtractionSource {
                    source_id: source.source_id.clone(),
                    source_basis: source.source_basis.clone(),
                    origin: source.origin,
                    scope_id: source.scope_id.clone(),
                    primary_request_id: source.primary_request_id.clone(),
                    primary_response_id: source.primary_response_id.clone(),
                }),
                events,
            )
            .await;
        }
    }

    async fn finish_recovered_memory_source(
        &self,
        source: &evohime_local_storage::domains::memory::MemoryExtractionSourceRecord,
        target: evohime_local_storage::domains::memory::MemoryExtractionSourceState,
        error_code: &str,
        events: &crate::EventSink,
    ) {
        let Some(journal) = &self.journal else {
            return;
        };
        let now_ms = task_memory::now_millis() as i64;
        let owner = uuid::Uuid::now_v7().to_string();
        let Ok(evohime_local_storage::domains::memory::SourceLeaseOutcome::Acquired { generation }) =
            journal
                .acquire_memory_extraction_source_lease(&source.source_id, &owner, now_ms, 300_000)
                .await
        else {
            return;
        };
        if matches!(
            journal
                .finish_memory_extraction_source(
                    &source.source_id,
                    generation,
                    target,
                    Some(error_code),
                    task_memory::now_millis() as i64,
                )
                .await,
            Ok(true)
        ) {
            let status = match target {
                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed => {
                    "committed"
                }
                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Deferred => {
                    "deferred"
                }
                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Stale => {
                    "stale"
                }
                _ => "failed",
            };
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id: &source.source_ref_id,
                    source_id: Some(&source.source_id),
                    origin: match source.origin {
                        evohime_local_storage::domains::memory::MemoryExtractionOrigin::Dialog => {
                            "dialog"
                        }
                        evohime_local_storage::domains::memory::MemoryExtractionOrigin::Ambient => {
                            "ambient"
                        }
                        _ => "recovery",
                    },
                    stage: "recovery",
                    status,
                    reason_code: Some(error_code),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                },
            )
            .await;
            write_model_trace(
                "memory.extraction.recovery",
                serde_json::json!({
                    "source_id": source.source_id,
                    "status": target.as_str(),
                    "error_code": error_code,
                }),
            );
        }
    }

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
        workspace_root: Option<&std::path::Path>,
        user_prompt: &str,
        assistant_reply: &str,
        source: Option<PreparedMemoryExtractionSource>,
        events: &crate::EventSink,
    ) {
        use crate::memory_extraction as extraction;

        let Some(_lease) =
            extraction::ExtractionLease::try_acquire(Arc::clone(&self.extraction_lease))
        else {
            write_model_trace(
                "memory.extraction.skipped",
                serde_json::json!({
                    "task_id": task_id,
                    "error_code": "memory_extraction_reentrant",
                    "source": "core.memory_extraction",
                    "operation": "dialog",
                    "reason": extraction::ExtractionError::Throttled {
                        reason: extraction::ThrottleReason::Reentrant,
                    }
                    .to_string(),
                }),
            );
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id,
                    source_id: source.as_ref().map(|item| item.source_id.as_str()),
                    origin: source
                        .as_ref()
                        .map_or("dialog", |item| item.origin.as_str()),
                    stage: "extractor",
                    status: "skipped",
                    reason_code: Some("memory_extraction_reentrant"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: true,
                },
            )
            .await;
            return;
        };
        self.run_memory_extraction_inner(
            task_id,
            workspace_root,
            user_prompt,
            assistant_reply,
            source,
            events,
        )
        .await;
    }

    async fn run_memory_extraction_inner(
        &self,
        task_id: &str,
        workspace_root: Option<&std::path::Path>,
        user_prompt: &str,
        assistant_reply: &str,
        source: Option<PreparedMemoryExtractionSource>,
        events: &crate::EventSink,
    ) {
        use crate::memory_extraction as extraction;

        let Some(journal) = &self.journal else {
            return;
        };
        let Some(source) = source else {
            write_model_trace(
                "memory.extraction.suppressed",
                serde_json::json!({
                    "task_id": task_id,
                    "error_code": "source_not_durable",
                    "operation": "dialog",
                }),
            );
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id,
                    source_id: None,
                    origin: "dialog",
                    stage: "source",
                    status: "skipped",
                    reason_code: Some("source_not_durable"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                },
            )
            .await;
            return;
        };
        emit_memory_extraction_diagnostic(
            events,
            MemoryExtractionDiagnostic {
                task_id,
                source_id: Some(&source.source_id),
                origin: source.origin.as_str(),
                stage: "extractor",
                status: "attempted",
                reason_code: None,
                backlog: 0,
                conflict_count: 0,
                suppressed_reentry: false,
            },
        )
        .await;
        let now_ms = task_memory::now_millis();
        let owner = uuid::Uuid::now_v7().to_string();
        let generation = match journal
            .acquire_memory_extraction_source_lease(
                &source.source_id,
                &owner,
                now_ms as i64,
                300_000,
            )
            .await
        {
            Ok(evohime_local_storage::domains::memory::SourceLeaseOutcome::Acquired {
                generation,
            }) => generation,
            Ok(evohime_local_storage::domains::memory::SourceLeaseOutcome::Busy {
                expires_at_ms,
            }) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "finalization",
                        status: "deferred",
                        reason_code: Some("lease_busy"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.extraction.deferred",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "lease_busy",
                        "lease_expires_at_ms": expires_at_ms,
                    }),
                );
                return;
            }
            Ok(evohime_local_storage::domains::memory::SourceLeaseOutcome::Terminal { state }) => {
                let status = match state {
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed => "committed",
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Deferred => "deferred",
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Stale => "stale",
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed => "failed",
                    _ => "skipped",
                };
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "finalization",
                        status,
                        reason_code: Some("source_terminal"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.extraction.skipped",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "state": state.as_str(),
                    }),
                );
                return;
            }
            Err(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "finalization",
                        status: "failed",
                        reason_code: Some("source_lease_failed"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "source_lease_failed",
                    }),
                );
                return;
            }
        };
        let mode = memory_extraction_mode();
        let trigger = extraction::detect_explicit_trigger(user_prompt);
        let policy = extraction::ExtractionPolicy::default();
        let now_ms = task_memory::now_millis();
        {
            let mut guard = self.extraction_guard.lock().await;
            guard.begin_turn();
            if let Err(error) = guard.check_can_extract(mode, trigger.as_ref(), now_ms, &policy) {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "extractor",
                        status: "skipped",
                        reason_code: Some("policy_suppressed"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.extraction.skipped",
                    serde_json::json!({
                        "task_id": task_id,
                        "mode": mode.as_str(),
                        "reason": error.to_string(),
                    }),
                );
                let _ = journal
                    .finish_memory_extraction_source(
                        &source.source_id,
                        generation,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        Some("policy_suppressed"),
                        now_ms as i64,
                    )
                    .await;
                return;
            }
        }

        let scope_id = source.scope_id.clone();
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

        let logical_request_id = format!("memory-extraction:{}", source.source_id);
        let mut retained_output = None;
        let latest_request = match journal
            .latest_model_request_for_logical_id(&logical_request_id)
            .await
        {
            Ok(request) => request,
            Err(_) => {
                let _ = journal
                    .finish_memory_extraction_source(
                        &source.source_id,
                        generation,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        Some("provenance_unavailable"),
                        task_memory::now_millis() as i64,
                    )
                    .await;
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "provenance_unavailable",
                    }),
                );
                return;
            }
        };
        if let Some(request) = latest_request {
            match journal
                .model_response_for_request(&request.request_id)
                .await
            {
                Ok(Some(response)) if response.status == "complete" => {
                    let Some(output) = response.output else {
                        let _ = journal
                            .finish_memory_extraction_source(
                                &source.source_id,
                                generation,
                                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                                Some("provenance_response_unavailable"),
                                task_memory::now_millis() as i64,
                            )
                            .await;
                        return;
                    };
                    if !matches!(
                        journal
                            .link_memory_extractor_response(
                                &source.source_id,
                                generation,
                                &request.request_id,
                                &response.response_id,
                                task_memory::now_millis() as i64,
                            )
                            .await,
                        Ok(true)
                    ) {
                        let _ = journal
                            .finish_memory_extraction_source(
                                &source.source_id,
                                generation,
                                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                                Some("source_link_failed"),
                                task_memory::now_millis() as i64,
                            )
                            .await;
                        return;
                    }
                    retained_output = Some(output);
                }
                Ok(Some(response)) if response.status == "failed" => {}
                Ok(None) if request.dispatch_at.is_none() => {}
                Err(_) => {
                    let _ = journal
                        .finish_memory_extraction_source(
                            &source.source_id,
                            generation,
                            evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                            Some("provenance_unavailable"),
                            task_memory::now_millis() as i64,
                        )
                        .await;
                    return;
                }
                _ => {
                    // A durable dispatch marker without a retained terminal
                    // response has an unknown provider outcome. Never blindly
                    // dispatch the same logical extractor again.
                    let _ = journal
                        .finish_memory_extraction_source(
                            &source.source_id,
                            generation,
                            evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                            Some("provider_outcome_unknown"),
                            task_memory::now_millis() as i64,
                        )
                        .await;
                    write_model_trace(
                        "memory.extraction.failed",
                        serde_json::json!({
                            "task_id": task_id,
                            "source_id": source.source_id,
                            "error_code": "provider_outcome_unknown",
                        }),
                    );
                    return;
                }
            }
        }
        let raw_output = match retained_output {
            Some(output) => Some(output),
            None => match self
                .call_memory_extractor(task_id, user_prompt, assistant_reply, &source, generation)
                .await
            {
                Ok(output) => output,
                Err(reason_code) => {
                    let status = if reason_code == "sensitive_data_blocked" {
                        "skipped"
                    } else {
                        "failed"
                    };
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "extractor",
                            status,
                            reason_code: Some(reason_code),
                            backlog: 0,
                            conflict_count: 0,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    let _ = journal
                        .finish_memory_extraction_source(
                            &source.source_id,
                            generation,
                            evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                            Some(reason_code),
                            task_memory::now_millis() as i64,
                        )
                        .await;
                    return;
                }
            },
        };
        let Some(raw_output) = raw_output else {
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id,
                    source_id: Some(&source.source_id),
                    origin: source.origin.as_str(),
                    stage: "extractor",
                    status: "failed",
                    reason_code: Some("extractor_unavailable"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                },
            )
            .await;
            let _ = journal
                .finish_memory_extraction_source(
                    &source.source_id,
                    generation,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                    Some("extractor_unavailable"),
                    task_memory::now_millis() as i64,
                )
                .await;
            return;
        };
        let candidates = match extraction::parse_extraction(&raw_output, &policy) {
            Ok(candidates) => candidates,
            Err(error) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "candidate",
                        status: "rejected",
                        reason_code: Some("extractor_malformed"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                let _ = journal
                    .finish_memory_extraction_source(
                        &source.source_id,
                        generation,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        Some("extractor_malformed"),
                        task_memory::now_millis() as i64,
                    )
                    .await;
                return;
            }
        };

        let mut conflict_count = 0usize;
        for raw in &candidates {
            let (candidate, subject) = match extraction::validate_candidate(raw, &aliases, &policy)
            {
                Ok(validated) => validated,
                Err(error) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "candidate",
                            status: "rejected",
                            reason_code: Some("candidate_validation_failed"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
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
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "candidate",
                        status: "rejected",
                        reason_code: Some("policy_rejected"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "candidate",
                            status: "duplicate",
                            reason_code: Some("duplicate_memory"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
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
                    conflict_count = conflict_count.saturating_add(1);
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "candidate",
                            status: "conflict",
                            reason_code: Some("active_memory_conflict"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
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
                // The record remains non-retrievable until the durable source
                // and candidate-slot CAS has completed.
                confirmation_state: "candidate".to_owned(),
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
            if crate::memory_governance::MemoryWriteGate::validate(&record).is_err() {
                write_model_trace(
                    "memory.extraction.rejected",
                    serde_json::json!({
                        "task_id": task_id,
                        "error_code": "candidate_governance_rejected",
                    }),
                );
                continue;
            }
            let final_state = decision.state.as_str().to_owned();
            let slot = match evohime_local_storage::domains::memory::candidate_slot_for(&record) {
                Ok(slot) => slot,
                Err(_) => continue,
            };
            let (candidate_basis, idempotency_key) =
                match evohime_local_storage::domains::memory::candidate_basis_for(
                    &source.source_basis,
                    &slot,
                ) {
                    Ok(keys) => keys,
                    Err(_) => continue,
                };
            let capture = journal
                .capture_memory_extraction_candidate(
                    &evohime_local_storage::domains::memory::CaptureCandidateInput {
                        source_id: &source.source_id,
                        record: &record,
                        source_basis: &candidate_basis,
                        idempotency_key: &idempotency_key,
                        candidate_slot: &slot,
                        generation,
                        now_ms: task_memory::now_millis() as i64,
                    },
                )
                .await;
            let should_finalize = match capture {
                Ok(evohime_local_storage::domains::memory::CaptureCandidateOutcome::Captured {
                    ..
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "candidate",
                        status: "captured",
                        reason_code: None,
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                        })

                    .await;
                    true
                }
                Ok(evohime_local_storage::domains::memory::CaptureCandidateOutcome::AlreadyCaptured {
                    memory_id: Some(_),
                    state,
                }) if state == "captured" => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "candidate",
                        status: "recovered",
                        reason_code: Some("candidate_already_captured"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                        })

                    .await;
                    true
                }
                Ok(evohime_local_storage::domains::memory::CaptureCandidateOutcome::AlreadyCaptured {
                    memory_id,
                    state,
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "candidate",
                        status: "skipped",
                        reason_code: Some("candidate_terminal"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                        })

                    .await;
                    write_model_trace(
                        "memory.extraction.skipped",
                        serde_json::json!({
                            "task_id": task_id,
                            "memory_id": memory_id,
                            "state": state,
                        }),
                    );
                    false
                }
                Ok(evohime_local_storage::domains::memory::CaptureCandidateOutcome::SupersededByNewer {
                    memory_id,
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "candidate",
                        status: "superseded",
                        reason_code: Some("newer_source_already_captured"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                        })

                    .await;
                    write_model_trace(
                        "memory.extraction.superseded",
                        serde_json::json!({
                            "task_id": task_id,
                            "memory_id": memory_id,
                            "reason": "newer_source_already_captured",
                        }),
                    );
                    false
                }
                Err(_) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "candidate",
                        status: "failed",
                        reason_code: Some("candidate_capture_failed"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                        })

                    .await;
                    write_model_trace(
                        "memory.extraction.failed",
                        serde_json::json!({
                            "task_id": task_id,
                            "error_code": "candidate_capture_failed",
                        }),
                    );
                    false
                }
            };
            if !should_finalize {
                continue;
            }
            let outcome = journal
                .finalize_memory_extraction_candidate(
                    evohime_local_storage::domains::memory::FinalizeCandidateInput {
                        source_id: &source.source_id,
                        idempotency_key: &idempotency_key,
                        generation,
                        target_confirmation_state: &final_state,
                        now_ms: task_memory::now_millis() as i64,
                    },
                )
                .await;
            match outcome {
                Ok(evohime_local_storage::domains::memory::PublishOutcome::Committed {
                    ..
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "finalization",
                            status: "committed",
                            reason_code: None,
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::AlreadyCommitted {
                    ..
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "finalization",
                            status: "recovered",
                            reason_code: None,
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::SupersededByNewer {
                    memory_id,
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "finalization",
                            status: "superseded",
                            reason_code: Some("newer_source_committed"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.extraction.superseded",
                        serde_json::json!({ "task_id": task_id, "memory_id": memory_id }),
                    );
                    continue;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::RevisionConflict {
                    current_revision,
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "finalization",
                            status: "conflict",
                            reason_code: Some("candidate_revision_conflict"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.extraction.conflict",
                        serde_json::json!({
                            "task_id": task_id,
                            "error_code": "candidate_revision_conflict",
                            "current_revision": current_revision,
                        }),
                    );
                    continue;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::RejectedByPolicy {
                    reason_code,
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "finalization",
                            status: "rejected",
                            reason_code: Some("candidate_rejected_by_policy"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.extraction.rejected",
                        serde_json::json!({ "task_id": task_id, "reason": reason_code }),
                    );
                    continue;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::StaleBasis) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "finalization",
                            status: "stale",
                            reason_code: Some("candidate_source_stale"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.extraction.stale",
                        serde_json::json!({
                            "task_id": task_id,
                            "error_code": "candidate_source_stale",
                        }),
                    );
                    continue;
                }
                Ok(_) | Err(_) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id,
                            source_id: Some(&source.source_id),
                            origin: source.origin.as_str(),
                            stage: "finalization",
                            status: "failed",
                            reason_code: Some("candidate_finalization_failed"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.extraction.failed",
                        serde_json::json!({
                            "task_id": task_id,
                            "error_code": "candidate_finalization_failed",
                        }),
                    );
                    continue;
                }
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
        let source_finished = journal
            .finish_memory_extraction_source(
                &source.source_id,
                generation,
                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed,
                None,
                task_memory::now_millis() as i64,
            )
            .await;
        match source_finished {
            Ok(true) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "finalization",
                        status: "committed",
                        reason_code: None,
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                    },
                )
                .await;
            }
            Ok(false) | Err(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id,
                        source_id: Some(&source.source_id),
                        origin: source.origin.as_str(),
                        stage: "finalization",
                        status: "deferred",
                        reason_code: Some("source_finish_not_confirmed"),
                        backlog: 1,
                        conflict_count,
                        suppressed_reentry: false,
                    },
                )
                .await;
            }
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
    pub(super) async fn run_ambient_memory_extraction(
        &self,
        episode_id: &str,
        events: &crate::EventSink,
    ) {
        use crate::memory_extraction as extraction;

        let Some(_lease) =
            extraction::ExtractionLease::try_acquire(Arc::clone(&self.extraction_lease))
        else {
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id: episode_id,
                    source_id: None,
                    origin: "ambient",
                    stage: "extractor",
                    status: "skipped",
                    reason_code: Some("memory_extraction_reentrant"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: true,
                },
            )
            .await;
            write_model_trace(
                "memory.ambient.skipped",
                serde_json::json!({
                    "episode_id": episode_id,
                    "error_code": "memory_extraction_reentrant",
                    "source": "core.memory_extraction",
                    "operation": "ambient",
                    "reason": extraction::ExtractionError::Throttled {
                        reason: extraction::ThrottleReason::Reentrant,
                    }
                    .to_string(),
                }),
            );
            return;
        };
        self.run_ambient_memory_extraction_inner(episode_id, events)
            .await;
    }

    async fn run_ambient_memory_extraction_inner(
        &self,
        episode_id: &str,
        events: &crate::EventSink,
    ) {
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
        emit_memory_extraction_diagnostic(
            events,
            MemoryExtractionDiagnostic {
                task_id: episode_id,
                source_id: None,
                origin: "ambient",
                stage: "source",
                status: "attempted",
                reason_code: None,
                backlog: 0,
                conflict_count: 0,
                suppressed_reentry: false,
            },
        )
        .await;
        {
            let mut guard = self.extraction_guard.lock().await;
            if let Err(error) = guard.check_can_extract_ambient(ambient_mode, mode, now_ms, &policy)
            {
                drop(guard);
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: None,
                        origin: "ambient",
                        stage: "extractor",
                        status: "skipped",
                        reason_code: Some("policy_suppressed"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                drop(guard);
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: None,
                        origin: "ambient",
                        stage: "extractor",
                        status: "skipped",
                        reason_code: Some("cooling_down"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.ambient.skipped",
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
        }
        let episode = match journal.get_ambient_episode(episode_id).await {
            Ok(Some(episode)) if episode.ended_at.is_some() => episode,
            _ => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: None,
                        origin: "ambient",
                        stage: "source",
                        status: "failed",
                        reason_code: Some("closed_source_unavailable"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.ambient.suppressed",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "error_code": "closed_source_unavailable",
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
        let source_revision_hash = extraction_digest(
            b"evohime-ambient-source-revision:v1\0",
            &[
                episode_id,
                &episode.started_at,
                episode.ended_at.as_deref().unwrap_or_default(),
                &episode.utterance_count.to_string(),
                &episode.speech_ms.to_string(),
                &episode.expires_at,
            ],
        );
        let source_basis = extraction_digest(
            b"evohime-memory-source-basis:v1\0",
            &["ambient", episode_id, &source_revision_hash],
        );
        let source_input = evohime_local_storage::domains::memory::CaptureSourceInput {
            source_id: source_basis.clone(),
            source_basis: source_basis.clone(),
            source_ref_id: episode_id.to_owned(),
            source_revision_hash,
            scope_id: AMBIENT_MEMORY_SCOPE_ID.to_owned(),
            origin: evohime_local_storage::domains::memory::MemoryExtractionOrigin::Ambient,
            root_execution_id: episode_id.to_owned(),
            depth: 0,
            source_order: now_ms as i64,
            primary_request_id: None,
            primary_response_id: None,
            now_ms: now_ms as i64,
        };
        let source_id = match journal
            .capture_memory_extraction_source(&source_input)
            .await
        {
            Ok(evohime_local_storage::domains::memory::CaptureSourceOutcome::Captured {
                source_id,
            }) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id: episode_id,
                    source_id: Some(&source_id),
                    origin: "ambient",
                    stage: "source",
                    status: "captured",
                    reason_code: None,
                    backlog: 1,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                source_id
            }
            Ok(evohime_local_storage::domains::memory::CaptureSourceOutcome::AlreadyCaptured {
                source_id,
                state,
            }) if state != evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id: episode_id,
                    source_id: Some(&source_id),
                    origin: "ambient",
                    stage: "source",
                    status: "recovered",
                    reason_code: Some("source_already_captured"),
                    backlog: 1,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                source_id
            }
            Ok(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id: episode_id,
                    source_id: None,
                    origin: "ambient",
                    stage: "source",
                    status: "committed",
                    reason_code: Some("source_already_committed"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                let _ = journal
                    .set_ambient_extraction_state(
                        episode_id,
                        evohime_listener_contract::ExtractionState::Done,
                    )
                    .await;
                return;
            }
            Err(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                    task_id: episode_id,
                    source_id: None,
                    origin: "ambient",
                    stage: "source",
                    status: "failed",
                    reason_code: Some("source_capture_failed"),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                    })

                .await;
                write_model_trace(
                    "memory.ambient.suppressed",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "error_code": "source_capture_failed",
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
        let _ = journal
            .set_ambient_extraction_state(
                episode_id,
                evohime_listener_contract::ExtractionState::Pending,
            )
            .await;

        let generation = match journal
            .acquire_memory_extraction_source_lease(
                &source_id,
                &uuid::Uuid::now_v7().to_string(),
                now_ms as i64,
                300_000,
            )
            .await
        {
            Ok(evohime_local_storage::domains::memory::SourceLeaseOutcome::Acquired {
                generation,
            }) => generation,
            Ok(evohime_local_storage::domains::memory::SourceLeaseOutcome::Busy {
                expires_at_ms,
            }) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source_id),
                        origin: "ambient",
                        stage: "finalization",
                        status: "deferred",
                        reason_code: Some("lease_busy"),
                        backlog: 1,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                write_model_trace(
                    "memory.ambient.deferred",
                    serde_json::json!({
                        "episode_id": episode_id,
                        "error_code": "lease_busy",
                        "lease_expires_at_ms": expires_at_ms,
                    }),
                );
                return;
            }
            Ok(evohime_local_storage::domains::memory::SourceLeaseOutcome::Terminal { state }) => {
                let status = match state {
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed => "committed",
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Deferred => "deferred",
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Stale => "stale",
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed => "failed",
                    _ => "skipped",
                };
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source_id),
                        origin: "ambient",
                        stage: "finalization",
                        status,
                        reason_code: Some("source_terminal"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                if state == evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed {
                    let _ = journal
                        .set_ambient_extraction_state(
                            episode_id,
                            evohime_listener_contract::ExtractionState::Done,
                        )
                        .await;
                }
                return;
            }
            Err(_) => {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source_id),
                        origin: "ambient",
                        stage: "finalization",
                        status: "failed",
                        reason_code: Some("source_lease_failed"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
                let _ = journal
                    .set_ambient_extraction_state(
                        episode_id,
                        evohime_listener_contract::ExtractionState::Failed,
                    )
                    .await;
                return;
            }
        };

        let Some(context) = self.ambient_episode_context(episode_id).await else {
            // An empty or fully redacted episode has nothing to extract; that
            // is a finished episode, not a failed one.
            let _ = journal
                .set_ambient_extraction_state(
                    episode_id,
                    evohime_listener_contract::ExtractionState::Done,
                )
                .await;
            let finish = journal
                .finish_memory_extraction_source(
                    &source_id,
                    generation,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed,
                    None,
                    task_memory::now_millis() as i64,
                )
                .await;
            let finish_confirmed = matches!(&finish, Ok(true));
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id: episode_id,
                    source_id: Some(&source_id),
                    origin: "ambient",
                    stage: "finalization",
                    status: if finish_confirmed {
                        "committed"
                    } else {
                        "deferred"
                    },
                    reason_code: if finish_confirmed {
                        None
                    } else {
                        Some("source_finish_not_confirmed")
                    },
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                },
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

        let source = PreparedMemoryExtractionSource {
            source_id: source_id.clone(),
            source_basis: source_basis.clone(),
            origin: evohime_local_storage::domains::memory::MemoryExtractionOrigin::Ambient,
            scope_id: AMBIENT_MEMORY_SCOPE_ID.to_owned(),
            primary_request_id: None,
            primary_response_id: None,
        };
        emit_memory_extraction_diagnostic(
            events,
            MemoryExtractionDiagnostic {
                task_id: episode_id,
                source_id: Some(&source.source_id),
                origin: "ambient",
                stage: "extractor",
                status: "attempted",
                reason_code: None,
                backlog: 0,
                conflict_count: 0,
                suppressed_reentry: false,
            },
        )
        .await;
        let extractor_result = self
            .call_extractor(
                episode_id,
                AMBIENT_MEMORY_EXTRACTION_PROMPT,
                context,
                true,
                Some(&source),
                Some(generation),
            )
            .await;
        let (raw_output, failure_reason) = match extractor_result {
            Ok(Some(raw_output)) => (Some(raw_output), None),
            Ok(None) => (None, Some("extractor_unavailable")),
            Err(reason_code) => (None, Some(reason_code)),
        };
        let Some(raw_output) = raw_output else {
            let reason_code = failure_reason.unwrap_or("extractor_unavailable");
            emit_memory_extraction_diagnostic(
                events,
                MemoryExtractionDiagnostic {
                    task_id: episode_id,
                    source_id: Some(&source.source_id),
                    origin: "ambient",
                    stage: "extractor",
                    status: if reason_code == "sensitive_data_blocked" {
                        "skipped"
                    } else {
                        "failed"
                    },
                    reason_code: Some(reason_code),
                    backlog: 0,
                    conflict_count: 0,
                    suppressed_reentry: false,
                },
            )
            .await;
            let _ = journal
                .finish_memory_extraction_source(
                    &source_id,
                    generation,
                    evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                    Some(reason_code),
                    task_memory::now_millis() as i64,
                )
                .await;
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
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source.source_id),
                        origin: "ambient",
                        stage: "candidate",
                        status: "rejected",
                        reason_code: Some("extractor_malformed"),
                        backlog: 0,
                        conflict_count: 0,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                    .finish_memory_extraction_source(
                        &source_id,
                        generation,
                        evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed,
                        Some("extractor_malformed"),
                        task_memory::now_millis() as i64,
                    )
                    .await;
                let _ = journal
                    .set_ambient_extraction_state(
                        episode_id,
                        evohime_listener_contract::ExtractionState::Failed,
                    )
                    .await;
                return;
            }
        };

        let mut conflict_count = 0usize;
        for raw in &candidates {
            let Ok((mut candidate, subject)) =
                extraction::validate_candidate(raw, &aliases, &policy)
            else {
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source.source_id),
                        origin: "ambient",
                        stage: "candidate",
                        status: "rejected",
                        reason_code: Some("candidate_validation_failed"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source.source_id),
                        origin: "ambient",
                        stage: "candidate",
                        status: "rejected",
                        reason_code: Some("ambient_kind_not_allowed"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source.source_id),
                        origin: "ambient",
                        stage: "candidate",
                        status: "rejected",
                        reason_code: Some("policy_rejected"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                emit_memory_extraction_diagnostic(
                    events,
                    MemoryExtractionDiagnostic {
                        task_id: episode_id,
                        source_id: Some(&source.source_id),
                        origin: "ambient",
                        stage: "candidate",
                        status: "duplicate",
                        reason_code: Some("duplicate_memory"),
                        backlog: 0,
                        conflict_count,
                        suppressed_reentry: false,
                    },
                )
                .await;
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
                confirmation_state: "candidate".to_owned(),
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
            let final_state = decision.state.as_str().to_owned();
            match capture_and_finalize_memory_candidate(
                journal,
                &source,
                generation,
                &record,
                &final_state,
                task_memory::now_millis() as i64,
            )
            .await
            {
                Ok(evohime_local_storage::domains::memory::PublishOutcome::Committed {
                    ..
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id: episode_id,
                            source_id: Some(&source.source_id),
                            origin: "ambient",
                            stage: "finalization",
                            status: "committed",
                            reason_code: None,
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::AlreadyCommitted {
                    ..
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id: episode_id,
                            source_id: Some(&source.source_id),
                            origin: "ambient",
                            stage: "finalization",
                            status: "recovered",
                            reason_code: None,
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::SupersededByNewer {
                    memory_id,
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id: episode_id,
                            source_id: Some(&source.source_id),
                            origin: "ambient",
                            stage: "finalization",
                            status: "superseded",
                            reason_code: Some("newer_source_committed"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.ambient.superseded",
                        serde_json::json!({ "episode_id": episode_id, "memory_id": memory_id }),
                    );
                    continue;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::RevisionConflict {
                    current_revision,
                }) => {
                    conflict_count = conflict_count.saturating_add(1);
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id: episode_id,
                            source_id: Some(&source.source_id),
                            origin: "ambient",
                            stage: "finalization",
                            status: "conflict",
                            reason_code: Some("candidate_revision_conflict"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.ambient.conflict",
                        serde_json::json!({
                            "episode_id": episode_id,
                            "error_code": "candidate_revision_conflict",
                            "current_revision": current_revision,
                        }),
                    );
                    continue;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::RejectedByPolicy {
                    reason_code,
                }) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id: episode_id,
                            source_id: Some(&source.source_id),
                            origin: "ambient",
                            stage: "finalization",
                            status: "rejected",
                            reason_code: Some("candidate_rejected_by_policy"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.ambient.rejected",
                        serde_json::json!({ "episode_id": episode_id, "reason": reason_code }),
                    );
                    continue;
                }
                Ok(evohime_local_storage::domains::memory::PublishOutcome::StaleBasis) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id: episode_id,
                            source_id: Some(&source.source_id),
                            origin: "ambient",
                            stage: "finalization",
                            status: "stale",
                            reason_code: Some("candidate_source_stale"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.ambient.stale",
                        serde_json::json!({
                            "episode_id": episode_id,
                            "error_code": "candidate_source_stale",
                        }),
                    );
                    continue;
                }
                Ok(_) | Err(_) => {
                    emit_memory_extraction_diagnostic(
                        events,
                        MemoryExtractionDiagnostic {
                            task_id: episode_id,
                            source_id: Some(&source.source_id),
                            origin: "ambient",
                            stage: "finalization",
                            status: "failed",
                            reason_code: Some("candidate_finalization_failed"),
                            backlog: 0,
                            conflict_count,
                            suppressed_reentry: false,
                        },
                    )
                    .await;
                    write_model_trace(
                        "memory.ambient.failed",
                        serde_json::json!({
                            "episode_id": episode_id,
                            "error_code": "candidate_finalization_failed",
                        }),
                    );
                    continue;
                }
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
        let source_finished = journal
            .finish_memory_extraction_source(
                &source_id,
                generation,
                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Committed,
                None,
                task_memory::now_millis() as i64,
            )
            .await;
        let finish_confirmed = matches!(&source_finished, Ok(true));
        emit_memory_extraction_diagnostic(
            events,
            MemoryExtractionDiagnostic {
                task_id: episode_id,
                source_id: Some(&source_id),
                origin: "ambient",
                stage: "finalization",
                status: if finish_confirmed {
                    "committed"
                } else {
                    "deferred"
                },
                reason_code: if finish_confirmed {
                    None
                } else {
                    Some("source_finish_not_confirmed")
                },
                backlog: if finish_confirmed { 0 } else { 1 },
                conflict_count,
                suppressed_reentry: false,
            },
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
        let mut text = String::new();
        for record in records.iter().filter(|record| !record.redacted) {
            let utterance = record.text.trim();
            if utterance.is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(utterance);
        }
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
        workspace_root: Option<&std::path::Path>,
        candidate: &crate::memory_extraction::Candidate,
    ) -> Option<crate::memory_extraction::VerificationVerdict> {
        use crate::memory_extraction as extraction;

        let target = extraction::validation_target(candidate)?;
        let policy = extraction::ExtractionPolicy::default();
        let expected = candidate.evidence.content_hash.clone();
        let Some(workspace_root) = workspace_root else {
            let outcome =
                extraction::file_evidence_outcome(&expected, None, task_memory::now_millis());
            return Some(extraction::apply_verification(&outcome, &policy));
        };
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
                            read_bounded_validation_file(&path),
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
        source: &PreparedMemoryExtractionSource,
        generation: i64,
    ) -> MemoryExtractorResult {
        use crate::memory_extraction as extraction;

        let budget_chars = extraction::MAX_CONTEXT_TOKENS * 4;
        let context = truncate_chars(
            &format!("Пользователь: {user_prompt}\nАгент: {assistant_reply}"),
            budget_chars,
        );
        self.call_extractor(
            task_id,
            MEMORY_EXTRACTION_PROMPT,
            context,
            false,
            Some(source),
            Some(generation),
        )
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
        source: Option<&PreparedMemoryExtractionSource>,
        generation: Option<i64>,
    ) -> MemoryExtractorResult {
        use crate::memory_extraction as extraction;

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
        if self.journal.is_some() {
            let (Some(source), Some(generation)) = (source, generation) else {
                write_model_trace(
                    "memory.extraction.suppressed",
                    serde_json::json!({
                        "task_id": task_id,
                        "error_code": "source_lease_required",
                        "ambient": ambient,
                    }),
                );
                return Err("source_lease_required");
            };
            return self
                .call_provenanced_extractor(
                    task_id,
                    &messages,
                    &routing_request,
                    model.as_deref(),
                    source,
                    ProvenancedExtractorContext {
                        ambient,
                        generation,
                    },
                )
                .await;
        }
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
                    return Ok(Some(result.content));
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
        Ok(None)
    }

    async fn call_provenanced_extractor(
        &self,
        task_id: &str,
        messages: &[ChatMessage],
        routing_request: &RoutingRequest,
        model: Option<&str>,
        source: &PreparedMemoryExtractionSource,
        context: ProvenancedExtractorContext,
    ) -> MemoryExtractorResult {
        use crate::memory_extraction as extraction;
        use evohime_context_budget::ledger::{
            ContextLedgerEntry, LedgerOutcome, SelectedItemRecord, CONTEXT_LEDGER_SCHEMA_VERSION,
        };

        let ProvenancedExtractorContext {
            ambient,
            generation,
        } = context;
        let Some(journal) = self.journal.as_ref() else {
            return Err("provenance_unavailable");
        };
        let logical_request_id = format!("memory-extraction:{}", source.source_id);
        let request_kind = if ambient {
            evohime_model_provenance::RequestKind::Ambient
        } else {
            evohime_model_provenance::RequestKind::Memory
        };
        let route_snapshot_hash = match self
            .gateway
            .provenance_route_snapshot_hash_with_model(routing_request, model)
        {
            Ok(hash) => hash,
            Err(_) => {
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "route_provenance_unavailable",
                    }),
                );
                return Err("provenance_unavailable");
            }
        };
        let provider_messages = match messages
            .iter()
            .map(|message| {
                let mut message = message.clone();
                message.content = redact_boundary_text("model", &message.content)
                    .map_err(|_| "sensitive_data_blocked")?;
                Ok(message)
            })
            .collect::<Result<Vec<_>, &str>>()
        {
            Ok(messages) => messages,
            Err(_) => {
                write_model_trace(
                    "memory.extraction.suppressed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "sensitive_data_blocked",
                    }),
                );
                return Err("sensitive_data_blocked");
            }
        };
        let estimated_tokens =
            context_token_estimate(&provider_messages).min(u32::MAX as usize) as u32;
        let source_refs = [evohime_model_provenance::SourceRef {
            source_ref_id: "memory-extraction-source".to_owned(),
            source_kind: if ambient {
                "ambient_episode".to_owned()
            } else {
                "conversation_turn".to_owned()
            },
            source_id: source.source_id.clone(),
            source_version: Some(source.source_basis.clone()),
            classification: "user_content".to_owned(),
        }];
        let mut previous_request = match journal
            .latest_model_request_for_logical_id(&logical_request_id)
            .await
        {
            Ok(Some(previous)) => match previous.envelope_hash {
                Some(hash) => Some((previous.request_id, hash, previous.attempt)),
                None => {
                    write_model_trace(
                        "memory.extraction.failed",
                        serde_json::json!({
                            "task_id": task_id,
                            "source_id": source.source_id,
                            "error_code": "provenance_retry_parent_missing",
                        }),
                    );
                    return Err("provenance_unavailable");
                }
            },
            Ok(None) => None,
            Err(_) => {
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "provenance_unavailable",
                    }),
                );
                return Err("provenance_unavailable");
            }
        };
        let attempts_used = previous_request
            .as_ref()
            .map_or(0, |(_, _, attempt)| *attempt as usize);
        let attempt_limit = extraction::RETRY_DELAYS_MS.len() + 1;
        let attempts_remaining = attempt_limit.saturating_sub(attempts_used);
        if attempts_remaining == 0 {
            write_model_trace(
                "memory.extraction.failed",
                serde_json::json!({
                    "task_id": task_id,
                    "source_id": source.source_id,
                    "error_code": "retry_limit_reached",
                }),
            );
            return Err("retry_limit_reached");
        }
        for retry_index in 0..attempts_remaining {
            let attempt_number = attempts_used.saturating_add(retry_index).saturating_add(1);
            if attempt_number > 1 {
                let retry_delay_index = attempt_number.saturating_sub(2);
                if let Some(delay) = extraction::ExtractionGuard::retry_delay_ms(retry_delay_index)
                {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
            let now_ms = task_memory::now_millis() as i64;
            let request_id = uuid::Uuid::now_v7().to_string();
            let (parent_request_id, previous_request_hash) = previous_request
                .as_ref()
                .map(|(id, hash, _)| (Some(id.clone()), Some(hash.clone())))
                .unwrap_or((None, None));
            let mut ledger = ContextLedgerEntry {
                id: request_id.clone(),
                schema_version: CONTEXT_LEDGER_SCHEMA_VERSION,
                task_id: task_id.to_owned(),
                session_id: task_id.to_owned(),
                model_call_id: logical_request_id.clone(),
                created_at: now_ms,
                provider: "policy-router".to_owned(),
                model: model
                    .unwrap_or_else(|| self.gateway.model_name())
                    .to_owned(),
                profile_version: "memory-extraction-v1".to_owned(),
                profile_snapshot: serde_json::json!({
                    "purpose": "memory_extraction",
                    "origin": source.origin,
                })
                .to_string(),
                tokenizer_version: "bounded-character-estimate-v1".to_owned(),
                normalizer_version: "memory-extraction-provenance-v1".to_owned(),
                strategy_version: "restricted-extraction-v1".to_owned(),
                mandatory_tokens: estimated_tokens,
                selected_optional_tokens: 0,
                reserves_tokens: 0,
                estimated_prompt_tokens: estimated_tokens,
                selected_items: vec![SelectedItemRecord {
                    id: format!("memory-source:{}", source.source_id),
                    estimated_tokens,
                }],
                dropped_items: Vec::new(),
                mandatory_parts: Vec::new(),
                ladder_levels_applied: Vec::new(),
                compression: Vec::new(),
                loadout: None,
                fallback_estimator: true,
                replan_of: None,
                outcome: LedgerOutcome::Sent,
                budget_unavailable: None,
                context_ledger_hash: String::new(),
            };
            ledger.finalize_hash();
            if journal.record_context_ledger(&ledger).await.is_err() {
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "provenance_unavailable",
                    }),
                );
                return Err("provenance_unavailable");
            }
            let envelope = match model_request_envelope(ModelRequestEnvelopeInput {
                logical_request_id: &logical_request_id,
                request_id: request_id.clone(),
                attempt: u32::try_from(attempt_number).unwrap_or(u32::MAX),
                parent_request_id,
                previous_request_hash,
                ledger: &ledger,
                messages: &provider_messages,
                specs: &[],
                source_refs: &source_refs,
                route_snapshot_hash: &route_snapshot_hash,
                request_kind,
            }) {
                Ok(envelope) => envelope,
                Err(_) => {
                    write_model_trace(
                        "memory.extraction.failed",
                        serde_json::json!({
                            "task_id": task_id,
                            "source_id": source.source_id,
                            "error_code": "provenance_envelope_invalid",
                        }),
                    );
                    return Err("provenance_unavailable");
                }
            };
            let record = match journal
                .commit_model_request(
                    &envelope,
                    evohime_local_storage::domains::receipts::CommitMode::FullForDispatch,
                )
                .await
            {
                Ok(record) if record.payload_mode == "full" && record.envelope_hash.is_some() => {
                    record
                }
                _ => {
                    write_model_trace(
                        "memory.extraction.failed",
                        serde_json::json!({
                            "task_id": task_id,
                            "source_id": source.source_id,
                            "error_code": "provenance_unavailable",
                        }),
                    );
                    return Err("provenance_unavailable");
                }
            };
            if journal
                .record_context_shadowing(&request_id, &ledger, &source_refs)
                .await
                .is_err()
            {
                return Err("provenance_unavailable");
            }
            let Some(keys) = self.receipt_keys.as_ref() else {
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "provenance_unavailable",
                    }),
                );
                return Err("provenance_unavailable");
            };
            if journal
                .append_model_request_receipt(keys, &record)
                .await
                .is_err()
            {
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "provenance_unavailable",
                    }),
                );
                return Err("provenance_unavailable");
            }
            if !matches!(
                journal
                    .link_memory_extractor_request(
                        &source.source_id,
                        generation,
                        &logical_request_id,
                        &request_id,
                        task_memory::now_millis() as i64,
                    )
                    .await,
                Ok(true)
            ) {
                write_model_trace(
                    "memory.extraction.failed",
                    serde_json::json!({
                        "task_id": task_id,
                        "source_id": source.source_id,
                        "error_code": "source_link_failed",
                    }),
                );
                return Err("provenance_unavailable");
            }
            if journal
                .mark_model_dispatch(&request_id, task_memory::now_millis() as i64)
                .await
                .is_err()
            {
                return Err("provenance_unavailable");
            }
            previous_request = Some((
                request_id.clone(),
                record.envelope_hash.clone().unwrap_or_default(),
                attempt_number as u32,
            ));
            let started_at = task_memory::now_millis() as i64;
            match timeout(
                Duration::from_secs(20),
                self.gateway.chat_with_tools_with_policy(
                    RoutingMode::Balanced,
                    routing_request,
                    model,
                    &provider_messages,
                    &[],
                ),
            )
            .await
            {
                Ok(Ok(result)) => {
                    let response_id = uuid::Uuid::now_v7().to_string();
                    let completed_at = task_memory::now_millis() as i64;
                    let response = evohime_local_storage::domains::receipts::ModelResponseRecord {
                        response_id: response_id.clone(),
                        request_id: request_id.clone(),
                        status: "complete".to_owned(),
                        output: Some(result.content.clone()),
                        output_hash: None,
                        finish_reason: Some("stop".to_owned()),
                        started_at,
                        completed_at: Some(completed_at),
                    };
                    if journal
                        .record_model_response(
                            &response,
                            evohime_model_provenance::RequestStatus::Completed,
                        )
                        .await
                        .is_err()
                        || !matches!(
                            journal
                                .link_memory_extractor_response(
                                    &source.source_id,
                                    generation,
                                    &request_id,
                                    &response_id,
                                    completed_at,
                                )
                                .await,
                            Ok(true)
                        )
                    {
                        write_model_trace(
                            "memory.extraction.failed",
                            serde_json::json!({
                                "task_id": task_id,
                                "source_id": source.source_id,
                                "error_code": "response_provenance_failed",
                            }),
                        );
                        return Err("provenance_unavailable");
                    }
                    let tokens = (estimated_tokens as usize
                        + result.content.chars().count().div_ceil(4))
                        as u64;
                    let mut guard = self.extraction_guard.lock().await;
                    if ambient {
                        guard.register_ambient_tokens(completed_at as u64, tokens);
                    } else {
                        guard.register_tokens(completed_at as u64, tokens);
                    }
                    return Ok(Some(result.content));
                }
                result => {
                    let completed_at = task_memory::now_millis() as i64;
                    let response_id = uuid::Uuid::now_v7().to_string();
                    let response = evohime_local_storage::domains::receipts::ModelResponseRecord {
                        response_id: response_id.clone(),
                        request_id: request_id.clone(),
                        status: "failed".to_owned(),
                        output: None,
                        output_hash: None,
                        finish_reason: Some(match result {
                            Err(_) => "timeout".to_owned(),
                            Ok(Err(_)) => "provider_error".to_owned(),
                            Ok(Ok(_)) => "unexpected_provider_result".to_owned(),
                        }),
                        started_at,
                        completed_at: Some(completed_at),
                    };
                    if journal
                        .record_model_response(
                            &response,
                            evohime_model_provenance::RequestStatus::Failed,
                        )
                        .await
                        .is_err()
                        || !matches!(
                            journal
                                .link_memory_extractor_response(
                                    &source.source_id,
                                    generation,
                                    &request_id,
                                    &response_id,
                                    completed_at,
                                )
                                .await,
                            Ok(true)
                        )
                    {
                        return Err("provenance_unavailable");
                    }
                    write_model_trace(
                        "memory.extraction.provider_error",
                        serde_json::json!({
                            "task_id": task_id,
                            "attempt": attempt_number,
                            "error_code": "provider_error",
                        }),
                    );
                }
            }
        }
        Ok(None)
    }
}

async fn capture_and_finalize_memory_candidate(
    journal: &EventJournal,
    source: &PreparedMemoryExtractionSource,
    generation: i64,
    record: &evohime_local_storage::domains::memory::MemoryRecord,
    target_confirmation_state: &str,
    now_ms: i64,
) -> Result<evohime_local_storage::domains::memory::PublishOutcome, String> {
    use evohime_local_storage::domains::memory;

    crate::memory_governance::MemoryWriteGate::validate(record)
        .map_err(|_| "candidate_governance_rejected".to_owned())?;
    let candidate_slot =
        memory::candidate_slot_for(record).map_err(|_| "candidate_slot_invalid")?;
    let (source_basis, idempotency_key) =
        memory::candidate_basis_for(&source.source_basis, &candidate_slot)
            .map_err(|_| "candidate_basis_invalid")?;
    let captured = journal
        .capture_memory_extraction_candidate(&memory::CaptureCandidateInput {
            source_id: &source.source_id,
            record,
            source_basis: &source_basis,
            idempotency_key: &idempotency_key,
            candidate_slot: &candidate_slot,
            generation,
            now_ms,
        })
        .await?;
    match captured {
        memory::CaptureCandidateOutcome::Captured { .. } => {}
        memory::CaptureCandidateOutcome::AlreadyCaptured {
            memory_id: Some(memory_id),
            state,
        } if state == "captured" => {
            let _ = memory_id;
        }
        memory::CaptureCandidateOutcome::AlreadyCaptured {
            memory_id: Some(memory_id),
            state,
        } if state == "committed" => {
            return Ok(memory::PublishOutcome::AlreadyCommitted { memory_id });
        }
        memory::CaptureCandidateOutcome::AlreadyCaptured { .. } => {
            return Ok(memory::PublishOutcome::StaleBasis);
        }
        memory::CaptureCandidateOutcome::SupersededByNewer { memory_id } => {
            return Ok(memory::PublishOutcome::SupersededByNewer { memory_id });
        }
    }
    journal
        .finalize_memory_extraction_candidate(memory::FinalizeCandidateInput {
            source_id: &source.source_id,
            idempotency_key: &idempotency_key,
            generation,
            target_confirmation_state,
            now_ms,
        })
        .await
}

async fn read_bounded_validation_file(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;

    let metadata = tokio::fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() > MAX_MEMORY_VALIDATION_FILE_BYTES as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "memory validation file exceeds the read limit",
        ));
    }
    let file = tokio::fs::File::open(path).await?;
    let mut bytes = Vec::with_capacity(MAX_MEMORY_VALIDATION_FILE_BYTES.min(16 * 1024));
    file.take((MAX_MEMORY_VALIDATION_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > MAX_MEMORY_VALIDATION_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "memory validation file exceeds the read limit",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{read_bounded_validation_file, ToolAgent, MAX_MEMORY_VALIDATION_FILE_BYTES};
    use crate::{CoreEvent, EventJournal, EventSink};
    use evohime_model_gateway::providers::{
        ChatFuture, ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream,
    };
    use evohime_model_gateway::{ChatResult, ChatStreamItem, ModelGateway};
    use evohime_tool_runtime::ToolRegistry;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct CountingProvider {
        calls: Arc<AtomicUsize>,
    }

    impl ModelProvider for CountingProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Mock
        }

        fn model_name(&self) -> &str {
            "memory-extraction-test"
        }

        fn base_url(&self) -> &str {
            "mock://memory-extraction-test"
        }

        fn stream_chat(&self, _messages: &[ChatMessage]) -> TokenStream {
            Box::pin(futures_util::stream::empty::<
                Result<ChatStreamItem, ProviderError>,
            >())
        }

        fn chat_with_tools(
            &self,
            _model: Option<&str>,
            _messages: &[ChatMessage],
            _tools: &[evohime_model_gateway::ToolSpec],
        ) -> ChatFuture {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Ok(ChatResult::default()) })
        }
    }

    #[tokio::test]
    async fn validation_read_rejects_an_oversized_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large.txt");
        std::fs::write(&path, vec![b'x'; MAX_MEMORY_VALIDATION_FILE_BYTES + 1]).unwrap();

        let error = read_bounded_validation_file(&path).await.unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn missing_request_receipt_key_fails_before_model_dispatch() {
        let directory = tempfile::tempdir().expect("temporary journal directory");
        let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
        let calls = Arc::new(AtomicUsize::new(0));
        let gateway = Arc::new(ModelGateway::from_provider(Arc::new(CountingProvider {
            calls: Arc::clone(&calls),
        })));
        let agent = ToolAgent::new(gateway, Arc::new(ToolRegistry::bootstrap()))
            .with_journal(journal.clone());
        let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(16);
        let events = EventSink::new(event_sender);
        let task_id = "memory-provenance-gate";
        let prompt = "запомни: язык интерфейса русский";
        let reply = "Язык интерфейса русский.";
        let source = agent
            .capture_dialog_memory_source(task_id, directory.path(), prompt, reply, None, &events)
            .await
            .expect("durable source is captured");

        agent
            .run_memory_extraction(
                task_id,
                Some(directory.path()),
                prompt,
                reply,
                Some(source.clone()),
                &events,
            )
            .await;

        assert_eq!(calls.load(Ordering::Relaxed), 0);
        let stored = journal
            .get_memory_extraction_source(&source.source_id)
            .await
            .expect("source lookup succeeds")
            .expect("source remains inspectable");
        assert_eq!(
            stored.state,
            evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed
        );
        assert_eq!(stored.error_code.as_deref(), Some("provenance_unavailable"));
        let mut provenance_failure_visible = false;
        while let Ok(event) = event_receiver.try_recv() {
            if matches!(
                event,
                CoreEvent::MemoryExtractionDiagnostic {
                    status,
                    reason_code: Some(reason_code),
                    ..
                } if status == "failed" && reason_code == "provenance_unavailable"
            ) {
                provenance_failure_visible = true;
            }
        }
        assert!(provenance_failure_visible);
    }

    #[tokio::test]
    async fn recovery_rejects_invalid_lineage_for_dialog_and_ambient_sources() {
        let directory = tempfile::tempdir().expect("temporary journal directory");
        let journal = EventJournal::open(directory.path().join("core.db")).expect("journal opens");
        let calls = Arc::new(AtomicUsize::new(0));
        let gateway = Arc::new(ModelGateway::from_provider(Arc::new(CountingProvider {
            calls: Arc::clone(&calls),
        })));
        let agent = ToolAgent::new(gateway, Arc::new(ToolRegistry::bootstrap()))
            .with_journal(journal.clone());
        let (event_sender, mut event_receiver) = tokio::sync::mpsc::channel(16);
        let events = EventSink::new(event_sender);
        let source_ids = ["invalid-ambient-lineage", "invalid-dialog-lineage"];
        let origins = [
            evohime_local_storage::domains::memory::MemoryExtractionOrigin::Ambient,
            evohime_local_storage::domains::memory::MemoryExtractionOrigin::Dialog,
        ];

        for (index, (source_id, origin)) in source_ids.into_iter().zip(origins).enumerate() {
            let input = evohime_local_storage::domains::memory::CaptureSourceInput {
                source_id: source_id.to_owned(),
                source_basis: format!("sha256:{source_id}"),
                source_ref_id: format!("source-ref-{index}"),
                source_revision_hash: format!("sha256:revision-{index}"),
                scope_id: "workspace-scope-test".to_owned(),
                origin,
                root_execution_id: "wrong-root".to_owned(),
                depth: 0,
                source_order: index as i64 + 1,
                primary_request_id: None,
                primary_response_id: None,
                now_ms: 100,
            };
            journal
                .capture_memory_extraction_source(&input)
                .await
                .expect("source metadata is captured");
        }

        agent.recover_pending_memory_extractions(&events).await;

        for source_id in source_ids {
            let source = journal
                .get_memory_extraction_source(source_id)
                .await
                .expect("source lookup succeeds")
                .expect("source remains inspectable");
            assert_eq!(
                source.state,
                evohime_local_storage::domains::memory::MemoryExtractionSourceState::Failed
            );
            assert_eq!(
                source.error_code.as_deref(),
                Some("recovery_source_lineage_invalid")
            );
        }
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        let mut lineage_failure_visible = false;
        while let Ok(event) = event_receiver.try_recv() {
            if matches!(
                event,
                CoreEvent::MemoryExtractionDiagnostic {
                    status,
                    reason_code: Some(reason_code),
                    ..
                } if status == "failed" && reason_code == "recovery_source_lineage_invalid"
            ) {
                lineage_failure_visible = true;
            }
        }
        assert!(lineage_failure_visible);
    }
}
