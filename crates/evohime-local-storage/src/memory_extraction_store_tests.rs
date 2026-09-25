use super::*;
use crate::memory_store::{MemoryPrivacy, MemoryRecordInput, MemoryScope};

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE memory_entries (
               id TEXT PRIMARY KEY NOT NULL, scope_kind TEXT NOT NULL,
               scope_id TEXT NOT NULL, title TEXT NOT NULL, content TEXT NOT NULL,
               provenance TEXT NOT NULL, privacy TEXT NOT NULL, created_at TEXT NOT NULL,
               expires_at TEXT, archived INTEGER NOT NULL, forgotten INTEGER NOT NULL,
               confirmations INTEGER NOT NULL DEFAULT 1, lesson_key TEXT,
               kind TEXT NOT NULL DEFAULT 'entity', canonical_subject TEXT,
               confirmation_state TEXT NOT NULL DEFAULT 'confirmed',
               model_confidence REAL NOT NULL DEFAULT 1.0,
               verification_confidence REAL NOT NULL DEFAULT 1.0,
               privacy_class TEXT NOT NULL DEFAULT 'normal', source_trust TEXT NOT NULL DEFAULT 'user',
               supersedes TEXT, superseded_by TEXT, supersession_reason TEXT,
               extractor_version TEXT NOT NULL DEFAULT 'v1_legacy',
               policy_version TEXT NOT NULL DEFAULT 'legacy-v1',
               validation_status TEXT NOT NULL DEFAULT 'not_required', validated_at TEXT,
               provenance_source_id TEXT, record_version INTEGER NOT NULL DEFAULT 1,
               evidence_refs TEXT NOT NULL DEFAULT '[]', execution_event_refs TEXT NOT NULL DEFAULT '[]',
               authority TEXT NOT NULL DEFAULT 'user_asserted', durability TEXT NOT NULL DEFAULT 'durable',
               confidence REAL NOT NULL DEFAULT 1.0
             );",
        )
        .expect("memory schema");
    install_schema(connection).expect("v175 lifecycle schema");
    let transaction = connection
        .unchecked_transaction()
        .expect("migration transaction");
    crate::migrations::v176::apply(&transaction, 175).expect("v176 lifecycle schema");
    transaction.commit().expect("migration commit");
}

fn record(id: &str) -> MemoryRecord {
    MemoryRecord::new(MemoryRecordInput {
        id: id.to_owned(),
        scope: MemoryScope::Project,
        scope_id: "scope".to_owned(),
        title: "subject".to_owned(),
        content: "bounded candidate".to_owned(),
        provenance: "{}".to_owned(),
        privacy: MemoryPrivacy::Private,
        created_at: "1".to_owned(),
        expires_at: None,
    })
    .expect("record")
}

#[test]
fn publication_is_atomic_and_idempotent() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    let first = publish_candidate(
        &connection,
        &record("memory-1"),
        "sha256:basis",
        "sha256:key",
        10,
    )
    .expect("first publish");
    assert_eq!(
        first,
        PublishOutcome::Committed {
            memory_id: "memory-1".to_owned()
        }
    );
    let replay = publish_candidate(
        &connection,
        &record("memory-2"),
        "sha256:basis",
        "sha256:key",
        11,
    )
    .expect("replay");
    assert_eq!(
        replay,
        PublishOutcome::AlreadyCommitted {
            memory_id: "memory-1".to_owned()
        }
    );
    let rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
        .expect("row count");
    assert_eq!(rows, 1);
}

#[test]
fn source_basis_prevents_duplicate_under_new_idempotency_key() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    publish_candidate(
        &connection,
        &record("memory-1"),
        "sha256:basis",
        "sha256:key-1",
        10,
    )
    .expect("first publish");
    let duplicate = publish_candidate(
        &connection,
        &record("memory-2"),
        "sha256:basis",
        "sha256:key-2",
        11,
    )
    .expect("duplicate source basis");
    assert_eq!(
        duplicate,
        PublishOutcome::SourceBasisAlreadyCaptured {
            memory_id: Some("memory-1".to_owned())
        }
    );
}

fn source_input() -> CaptureSourceInput {
    CaptureSourceInput {
        source_id: "source-1".to_owned(),
        source_basis: "sha256:source-basis".to_owned(),
        source_ref_id: "task-1".to_owned(),
        source_revision_hash: "sha256:revision".to_owned(),
        scope_id: "workspace-scope-1".to_owned(),
        origin: MemoryExtractionOrigin::Dialog,
        root_execution_id: "root-1".to_owned(),
        depth: 0,
        source_order: 10,
        primary_request_id: Some("request-1".to_owned()),
        primary_response_id: Some("response-1".to_owned()),
        now_ms: 100,
    }
}

fn candidate_record(id: &str, title: &str, content: &str) -> MemoryRecord {
    let mut candidate = record(id);
    candidate.title = title.to_owned();
    candidate.content = content.to_owned();
    candidate.extraction.kind = "preference".to_owned();
    candidate.extraction.canonical_subject = Some("язык интерфейса".to_owned());
    candidate.extraction.confirmation_state = "candidate".to_owned();
    candidate.extraction.authority = "model_proposed".to_owned();
    candidate
}

fn capture_candidate_for_source(
    connection: &Connection,
    source: &CaptureSourceInput,
    candidate: &MemoryRecord,
    owner: &str,
) -> (i64, String) {
    capture_source(connection, source).expect("capture source");
    let generation =
        match acquire_source_lease(connection, &source.source_id, owner, source.now_ms, 60_000)
            .expect("acquire source lease")
        {
            SourceLeaseOutcome::Acquired { generation } => generation,
            other => panic!("unexpected lease outcome: {other:?}"),
        };
    let slot = candidate_slot_for(candidate).expect("candidate slot");
    let (basis, key) = candidate_basis_for(&source.source_basis, &slot).expect("candidate basis");
    capture_candidate(
        connection,
        &CaptureCandidateInput {
            source_id: &source.source_id,
            record: candidate,
            source_basis: &basis,
            idempotency_key: &key,
            candidate_slot: &slot,
            generation,
            now_ms: source.now_ms + 1,
        },
    )
    .expect("capture candidate");
    (generation, key)
}

#[test]
fn source_capture_is_metadata_only_and_idempotent() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    let input = source_input();
    assert_eq!(
        capture_source(&connection, &input).expect("capture"),
        CaptureSourceOutcome::Captured {
            source_id: "source-1".to_owned()
        }
    );
    assert_eq!(
        capture_source(&connection, &input).expect("replay"),
        CaptureSourceOutcome::AlreadyCaptured {
            source_id: "source-1".to_owned(),
            state: MemoryExtractionSourceState::Captured
        }
    );
    let stored: (String, String, String) = connection
        .query_row(
            "SELECT origin,source_ref_id,source_revision_hash
             FROM memory_extraction_sources WHERE source_id='source-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("stored metadata");
    assert_eq!(
        stored,
        (
            "dialog".to_owned(),
            "task-1".to_owned(),
            "sha256:revision".to_owned()
        )
    );
    let columns: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('memory_extraction_sources')
             WHERE name IN ('prompt','transcript','content','statement')",
            [],
            |row| row.get(0),
        )
        .expect("metadata-only shape");
    assert_eq!(columns, 0);
}

#[test]
fn source_lease_expiry_increments_fence_and_rejects_old_generation() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    capture_source(&connection, &source_input()).expect("capture");
    assert_eq!(
        acquire_source_lease(&connection, "source-1", "worker-a", 100, 10).expect("acquire"),
        SourceLeaseOutcome::Acquired { generation: 1 }
    );
    assert_eq!(
        acquire_source_lease(&connection, "source-1", "worker-b", 105, 10).expect("busy"),
        SourceLeaseOutcome::Busy { expires_at_ms: 110 }
    );
    assert_eq!(
        acquire_source_lease(&connection, "source-1", "worker-b", 110, 10).expect("reacquire"),
        SourceLeaseOutcome::Acquired { generation: 2 }
    );
    assert!(!finish_source(
        &connection,
        "source-1",
        1,
        MemoryExtractionSourceState::Committed,
        None,
        111,
    )
    .expect("old generation is fenced"));
    assert!(finish_source(
        &connection,
        "source-1",
        2,
        MemoryExtractionSourceState::Committed,
        None,
        111,
    )
    .expect("current generation commits"));
    assert_eq!(
        acquire_source_lease(&connection, "source-1", "worker-c", 112, 10).expect("terminal"),
        SourceLeaseOutcome::Terminal {
            state: MemoryExtractionSourceState::Committed
        }
    );
}

#[test]
fn source_capture_rejects_unbounded_depth_and_basis_reuse() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    let mut invalid = source_input();
    invalid.depth = MAX_EXTRACTION_DEPTH + 1;
    assert!(capture_source(&connection, &invalid).is_err());
    capture_source(&connection, &source_input()).expect("capture");
    let mut conflicting = source_input();
    conflicting.source_ref_id = "another-task".to_owned();
    assert!(matches!(
        capture_source(&connection, &conflicting),
        Err(MemoryStoreError::ExtractionIdempotencyConflict)
    ));
}

#[test]
fn extraction_origin_serde_and_persisted_parser_reject_unknown_values() {
    assert_eq!(MemoryExtractionOrigin::Dialog.as_str(), "dialog");
    assert_eq!(
        MemoryExtractionOrigin::parse("ambient").expect("known origin"),
        MemoryExtractionOrigin::Ambient
    );
    assert!(MemoryExtractionOrigin::parse("future_origin").is_err());
    assert!(serde_json::from_str::<MemoryExtractionOrigin>("\"future_origin\"").is_err());
}

#[test]
fn source_order_is_strictly_monotonic_and_capture_replay_is_stable() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    let mut first = source_input();
    first.source_order = 100;
    capture_source(&connection, &first).expect("first source");

    let mut replay = first.clone();
    replay.source_order = 500;
    assert!(matches!(
        capture_source(&connection, &replay),
        Ok(CaptureSourceOutcome::AlreadyCaptured { .. })
    ));
    assert_eq!(
        get_source(&connection, &first.source_id)
            .expect("source read")
            .expect("source exists")
            .source_order,
        100
    );

    let mut second = source_input();
    second.source_id = "source-2".to_owned();
    second.source_basis = "sha256:source-2".to_owned();
    second.source_ref_id = "task-2".to_owned();
    second.source_order = 100;
    capture_source(&connection, &second).expect("second source");
    assert_eq!(
        get_source(&connection, &second.source_id)
            .expect("source read")
            .expect("source exists")
            .source_order,
        101
    );
}

#[test]
fn candidate_slot_and_capture_identity_ignore_generated_wording() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    capture_source(&connection, &source_input()).expect("capture source");
    let generation =
        match acquire_source_lease(&connection, "source-1", "worker-a", 100, 100).expect("lease") {
            SourceLeaseOutcome::Acquired { generation } => generation,
            other => panic!("unexpected lease outcome: {other:?}"),
        };

    let first = candidate_record("memory-1", "Предпочтительный язык", "Использует русский.");
    let rewritten = candidate_record("memory-2", "Язык для интерфейса", "Русский язык.");
    let slot = candidate_slot_for(&first).expect("slot");
    assert_eq!(slot, candidate_slot_for(&rewritten).expect("same slot"));
    let (source_basis, idempotency_key) =
        candidate_basis_for("sha256:source-basis", &slot).expect("candidate identity");

    assert_eq!(
        capture_candidate(
            &connection,
            &CaptureCandidateInput {
                source_id: "source-1",
                record: &first,
                source_basis: &source_basis,
                idempotency_key: &idempotency_key,
                candidate_slot: &slot,
                generation,
                now_ms: 101,
            },
        )
        .expect("candidate capture"),
        CaptureCandidateOutcome::Captured {
            memory_id: "memory-1".to_owned()
        }
    );
    assert_eq!(
        capture_candidate(
            &connection,
            &CaptureCandidateInput {
                source_id: "source-1",
                record: &rewritten,
                source_basis: &source_basis,
                idempotency_key: &idempotency_key,
                candidate_slot: &slot,
                generation,
                now_ms: 102,
            },
        )
        .expect("wording replay"),
        CaptureCandidateOutcome::AlreadyCaptured {
            memory_id: Some("memory-1".to_owned()),
            state: "captured".to_owned(),
        }
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row
                .get::<_, i64>(0))
            .expect("single candidate body"),
        1
    );
}

#[test]
fn candidate_capture_rejects_forged_slot_or_idempotency_basis() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    capture_source(&connection, &source_input()).expect("capture source");
    let generation =
        match acquire_source_lease(&connection, "source-1", "worker-a", 100, 100).expect("lease") {
            SourceLeaseOutcome::Acquired { generation } => generation,
            other => panic!("unexpected lease outcome: {other:?}"),
        };
    let candidate = candidate_record("memory-1", "Язык", "Русский");
    let slot = candidate_slot_for(&candidate).expect("slot");
    let (source_basis, idempotency_key) =
        candidate_basis_for("sha256:source-basis", &slot).expect("candidate identity");
    let forged = CaptureCandidateInput {
        source_id: "source-1",
        record: &candidate,
        source_basis: "sha256:forged-basis",
        idempotency_key: &idempotency_key,
        candidate_slot: &slot,
        generation,
        now_ms: 101,
    };
    assert!(matches!(
        capture_candidate(&connection, &forged),
        Err(MemoryStoreError::InvalidField("candidate_source_basis"))
    ));
    let forged_slot = CaptureCandidateInput {
        source_id: "source-1",
        record: &candidate,
        source_basis: &source_basis,
        idempotency_key: &idempotency_key,
        candidate_slot: "sha256:forged-slot",
        generation,
        now_ms: 101,
    };
    assert!(matches!(
        capture_candidate(&connection, &forged_slot),
        Err(MemoryStoreError::InvalidField("candidate_slot"))
    ));
}

#[test]
fn candidate_finalization_commits_once_and_replays_read_only() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    let source = source_input();
    let candidate = candidate_record("memory-1", "Язык", "Русский");
    let (generation, idempotency_key) =
        capture_candidate_for_source(&connection, &source, &candidate, "worker-a");
    let input = FinalizeCandidateInput {
        source_id: &source.source_id,
        idempotency_key: &idempotency_key,
        generation,
        now_ms: 102,
        target_confirmation_state: "pending_confirmation",
    };
    assert_eq!(
        finalize_candidate(&connection, input).expect("finalize"),
        PublishOutcome::Committed {
            memory_id: "memory-1".to_owned()
        }
    );
    assert_eq!(
        finalize_candidate(&connection, input).expect("replay"),
        PublishOutcome::AlreadyCommitted {
            memory_id: "memory-1".to_owned()
        }
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT memory_id,revision,source_order FROM memory_extraction_heads",
                [],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?
                )),
            )
            .expect("committed head"),
        ("memory-1".to_owned(), 1, 10)
    );
}

#[test]
fn candidate_finalizers_preserve_newest_source_order() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    let mut older_source = source_input();
    older_source.source_id = "source-old".to_owned();
    older_source.source_basis = "sha256:source-old".to_owned();
    older_source.source_order = 20;
    older_source.now_ms = 200;
    let mut newer_source = source_input();
    newer_source.source_id = "source-new".to_owned();
    newer_source.source_basis = "sha256:source-new".to_owned();
    newer_source.source_order = 21;
    newer_source.now_ms = 210;
    let older = candidate_record("memory-old", "Old wording", "Old statement");
    let newer = candidate_record("memory-new", "New wording", "New statement");
    let (older_generation, older_key) =
        capture_candidate_for_source(&connection, &older_source, &older, "worker-old");
    let (newer_generation, newer_key) =
        capture_candidate_for_source(&connection, &newer_source, &newer, "worker-new");

    assert_eq!(
        finalize_candidate(
            &connection,
            FinalizeCandidateInput {
                source_id: "source-new",
                idempotency_key: &newer_key,
                generation: newer_generation,
                now_ms: 212,
                target_confirmation_state: "pending_confirmation",
            },
        )
        .expect("newer finalizes first"),
        PublishOutcome::Committed {
            memory_id: "memory-new".to_owned()
        }
    );
    assert_eq!(
        finalize_candidate(
            &connection,
            FinalizeCandidateInput {
                source_id: "source-old",
                idempotency_key: &older_key,
                generation: older_generation,
                now_ms: 213,
                target_confirmation_state: "pending_confirmation",
            },
        )
        .expect("older is fenced by newer head"),
        PublishOutcome::SupersededByNewer {
            memory_id: "memory-new".to_owned()
        }
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT memory_id,source_order FROM memory_extraction_heads",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .expect("newest head remains"),
        ("memory-new".to_owned(), 21)
    );
}

#[test]
fn newer_finalizer_refreshes_an_older_committed_precondition() {
    let connection = Connection::open_in_memory().expect("sqlite");
    schema(&connection);
    let mut older_source = source_input();
    older_source.source_id = "source-old".to_owned();
    older_source.source_basis = "sha256:source-old".to_owned();
    older_source.source_order = 20;
    older_source.now_ms = 200;
    let mut newer_source = source_input();
    newer_source.source_id = "source-new".to_owned();
    newer_source.source_basis = "sha256:source-new".to_owned();
    newer_source.source_order = 21;
    newer_source.now_ms = 210;
    let older = candidate_record("memory-old", "Old wording", "Old statement");
    let newer = candidate_record("memory-new", "New wording", "New statement");
    let (older_generation, older_key) =
        capture_candidate_for_source(&connection, &older_source, &older, "worker-old");
    let (newer_generation, newer_key) =
        capture_candidate_for_source(&connection, &newer_source, &newer, "worker-new");

    assert_eq!(
        finalize_candidate(
            &connection,
            FinalizeCandidateInput {
                source_id: "source-old",
                idempotency_key: &older_key,
                generation: older_generation,
                now_ms: 202,
                target_confirmation_state: "pending_confirmation",
            },
        )
        .expect("older commits first"),
        PublishOutcome::Committed {
            memory_id: "memory-old".to_owned()
        }
    );
    assert_eq!(
        finalize_candidate(
            &connection,
            FinalizeCandidateInput {
                source_id: "source-new",
                idempotency_key: &newer_key,
                generation: newer_generation,
                now_ms: 212,
                target_confirmation_state: "pending_confirmation",
            },
        )
        .expect("newer refreshes precondition"),
        PublishOutcome::Committed {
            memory_id: "memory-new".to_owned()
        }
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT memory_id,revision,source_order FROM memory_extraction_heads",
                [],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?
                )),
            )
            .expect("new head"),
        ("memory-new".to_owned(), 2, 21)
    );
}
