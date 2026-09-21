
use super::*;

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE memory_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    provenance TEXT NOT NULL,
                    privacy TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT,
                    archived INTEGER NOT NULL,
                    forgotten INTEGER NOT NULL,
                    confirmations INTEGER NOT NULL DEFAULT 1,
                    lesson_key TEXT,
                    kind TEXT NOT NULL DEFAULT 'entity',
                    canonical_subject TEXT,
                    confirmation_state TEXT NOT NULL DEFAULT 'confirmed',
                    model_confidence REAL NOT NULL DEFAULT 1.0,
                    verification_confidence REAL NOT NULL DEFAULT 1.0,
                    privacy_class TEXT NOT NULL DEFAULT 'normal',
                    source_trust TEXT NOT NULL DEFAULT 'user',
                    supersedes TEXT,
                    superseded_by TEXT,
                    supersession_reason TEXT,
                    extractor_version TEXT NOT NULL DEFAULT 'v1_legacy',
                    policy_version TEXT NOT NULL DEFAULT 'legacy-v1',
                    validation_status TEXT NOT NULL DEFAULT 'not_required',
                    validated_at TEXT,
                    provenance_source_id TEXT,
                     record_version INTEGER NOT NULL DEFAULT 1,
                     evidence_refs TEXT NOT NULL DEFAULT '[]',
                     execution_event_refs TEXT NOT NULL DEFAULT '[]',
                     authority TEXT NOT NULL DEFAULT 'user_asserted',
                     durability TEXT NOT NULL DEFAULT 'durable',
                     confidence REAL NOT NULL DEFAULT 1.0
                );
                CREATE TABLE memory_aliases (
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    alias TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    registered_by TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (scope_kind, scope_id, alias)
                );
                CREATE TABLE memory_tombstones (
                    tombstone_id TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    forgotten_at TEXT NOT NULL,
                    reason_class TEXT NOT NULL,
                    digest TEXT NOT NULL
                );
                CREATE TABLE memory_session_notes (
                    id TEXT PRIMARY KEY NOT NULL,
                    session_id TEXT NOT NULL,
                    scope_kind TEXT NOT NULL,
                    scope_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    statement TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL
                );",
        )
        .expect("memory schema creates");
}

fn pending(id: &str, subject: &str, statement: &str) -> MemoryRecord {
    let mut memory = record(id, statement);
    memory.extraction = MemoryExtractionFields {
        kind: "preference".to_owned(),
        canonical_subject: Some(subject.to_owned()),
        confirmation_state: "pending_confirmation".to_owned(),
        model_confidence: 0.9,
        verification_confidence: 0.0,
        extractor_version: "extractor-v1".to_owned(),
        policy_version: "extraction-policy-v1".to_owned(),
        ..MemoryExtractionFields::default()
    };
    memory
}

fn record(id: &str, content: &str) -> MemoryRecord {
    MemoryRecord::new(MemoryRecordInput {
        id: id.into(),
        scope: MemoryScope::Project,
        scope_id: "project-1".into(),
        title: "Decision".into(),
        content: content.into(),
        provenance: "run:1".into(),
        privacy: MemoryPrivacy::Internal,
        created_at: "2026-08-12T10:00:00Z".into(),
        expires_at: Some("2027-01-01T00:00:00Z".into()),
    })
    .expect("memory record builds")
}

#[test]
fn constructor_redacts_sensitive_content_and_bounds_fields() {
    let memory = record("m-1", "contact roman@example.test with token=secret");
    assert!(!memory.content.contains("roman@example.test"));
    assert!(!memory.content.contains("token=secret"));

    let too_large = MemoryRecord::new(MemoryRecordInput {
        id: "m-1".into(),
        scope: MemoryScope::Project,
        scope_id: "project-1".into(),
        title: "title".into(),
        content: "x".repeat(MAX_CONTENT_BYTES + 1),
        provenance: "run:1".into(),
        privacy: MemoryPrivacy::Internal,
        created_at: "2026-08-12T10:00:00Z".into(),
        expires_at: None,
    });
    assert!(matches!(
        too_large,
        Err(MemoryStoreError::Limit {
            field: "content",
            ..
        })
    ));
}

#[test]
fn round_trip_search_is_scoped_bounded_and_deterministic() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &record("b", "Rust decision")).expect("insert b");
    MemoryStoreSql::insert(&connection, &record("a", "Rust decision")).expect("insert a");
    let other = MemoryRecord {
        scope_id: "other-project".into(),
        ..record("c", "Rust decision")
    };
    MemoryStoreSql::insert(&connection, &other).expect("insert other");

    let found = MemoryStoreSql::search(
        &connection,
        MemoryScope::Project,
        "project-1",
        "rust",
        "2026-09-01T00:00:00Z",
        10,
    )
    .expect("search memories");
    assert_eq!(
        found
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        MemoryStoreSql::get_by_id(&connection, "a").unwrap(),
        Some(record("a", "Rust decision"))
    );
}

#[test]
fn search_treats_like_wildcards_as_literal_query_data() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &record("literal", "budget is 100% fixed"))
        .expect("insert literal");
    MemoryStoreSql::insert(&connection, &record("wildcard", "budget is 100X fixed"))
        .expect("insert wildcard candidate");
    MemoryStoreSql::insert(
        &connection,
        &record("windows-path", r#"path C:\temp\agent"#),
    )
    .expect("insert path");

    let found = MemoryStoreSql::search(
        &connection,
        MemoryScope::Project,
        "project-1",
        "100%",
        "2026-09-01T00:00:00Z",
        10,
    )
    .expect("search literal wildcard");
    assert_eq!(
        found
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["literal"]
    );
    let path_found = MemoryStoreSql::search(
        &connection,
        MemoryScope::Project,
        "project-1",
        r#"C:\temp\agent"#,
        "2026-09-01T00:00:00Z",
        10,
    )
    .expect("search literal backslashes");
    assert_eq!(
        path_found
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["windows-path"]
    );
}

#[test]
fn list_is_scoped_and_hides_archived_unless_requested() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &record("b", "Rust decision")).expect("insert b");
    MemoryStoreSql::insert(&connection, &record("a", "Rust decision")).expect("insert a");
    let other = MemoryRecord {
        scope_id: "other-project".into(),
        ..record("c", "Rust decision")
    };
    MemoryStoreSql::insert(&connection, &other).expect("insert other");
    assert!(MemoryStoreSql::archive(&connection, "a").expect("archive a"));

    let active = MemoryStoreSql::list(&connection, MemoryScope::Project, "project-1", false, 10)
        .expect("list active");
    assert_eq!(
        active
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );

    let all = MemoryStoreSql::list(&connection, MemoryScope::Project, "project-1", true, 10)
        .expect("list including archived");
    assert_eq!(all.len(), 2);
}

#[test]
fn archive_and_forget_remove_memory_from_search_without_deleting_row() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &record("m-1", "keep this fact")).expect("insert");
    assert!(MemoryStoreSql::archive(&connection, "m-1").expect("archive"));
    assert!(MemoryStoreSql::search(
        &connection,
        MemoryScope::Project,
        "project-1",
        "fact",
        "2026-09-01T00:00:00Z",
        10
    )
    .unwrap()
    .is_empty());

    assert!(MemoryStoreSql::forget(&connection, "m-1").expect("forget"));
    let forgotten = MemoryStoreSql::get_by_id(&connection, "m-1")
        .unwrap()
        .unwrap();
    assert!(forgotten.forgotten);
    assert!(forgotten.content.is_empty());
    assert!(forgotten.provenance.is_empty());
    assert_eq!(forgotten.extraction.confirmation_state, "forgotten");
}

#[test]
fn pending_records_are_listable_but_never_retrievable() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &record("confirmed-1", "Rust decision"))
        .expect("insert confirmed");
    MemoryStoreSql::insert(
        &connection,
        &pending("pending-1", "язык интерфейса", "Rust decision"),
    )
    .expect("insert pending");

    // Search отдаёт только подтверждённую активную запись.
    let found = MemoryStoreSql::search(
        &connection,
        MemoryScope::Project,
        "project-1",
        "rust",
        "2026-09-01T00:00:00Z",
        10,
    )
    .expect("search");
    assert_eq!(
        found
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["confirmed-1"]
    );

    let queue = MemoryStoreSql::list_by_state(
        &connection,
        MemoryScope::Project,
        "project-1",
        "pending_confirmation",
        10,
    )
    .expect("pending queue");
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].id, "pending-1");

    let counts = MemoryStoreSql::count_by_state(&connection, MemoryScope::Project, "project-1")
        .expect("counts");
    assert_eq!(
        counts,
        vec![
            ("confirmed".to_owned(), 1),
            ("pending_confirmation".to_owned(), 1)
        ]
    );
}

#[test]
fn state_transitions_are_idempotent_and_terminal_states_stick() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &pending("p-1", "тема", "утверждение"))
        .expect("insert pending");

    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap(),
        "confirmed"
    );
    // Повторный confirm безопасен и возвращает фактическое состояние.
    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap(),
        "confirmed"
    );

    MemoryStoreSql::insert(&connection, &pending("p-2", "тема-2", "утверждение-2"))
        .expect("insert second");
    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "p-2", "rejected").unwrap(),
        "rejected"
    );
    // Отклонённая запись не переоткрывается повторным confirm.
    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "p-2", "confirmed").unwrap(),
        "rejected"
    );

    assert!(matches!(
        MemoryStoreSql::transition_state(&connection, "missing", "confirmed"),
        Err(MemoryStoreError::NotFound)
    ));
}

#[test]
fn supersede_builds_a_chain_and_keeps_the_old_record_inactive() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    for id in ["a", "b", "c"] {
        let mut memory = pending(id, "тема", &format!("вариант {id}"));
        memory.extraction.confirmation_state = "confirmed".to_owned();
        MemoryStoreSql::insert(&connection, &memory).expect("insert");
    }
    MemoryStoreSql::supersede(&connection, "a", "b", "user_choice").expect("a -> b");
    MemoryStoreSql::supersede(&connection, "b", "c", "user_choice").expect("b -> c");

    let chain = MemoryStoreSql::supersession_chain(&connection, "c", 16).expect("chain");
    assert_eq!(chain, ["a", "b", "c"]);

    let old = MemoryStoreSql::get_by_id(&connection, "a")
        .unwrap()
        .unwrap();
    assert_eq!(old.extraction.confirmation_state, "superseded");
    assert_eq!(old.extraction.superseded_by.as_deref(), Some("b"));
    assert_eq!(
        old.extraction.supersession_reason.as_deref(),
        Some("user_choice")
    );

    // Только последняя запись цепочки участвует в retrieval.
    let active = MemoryStoreSql::conflict_candidates(
        &connection,
        MemoryScope::Project,
        "project-1",
        "preference",
        10,
    )
    .expect("active");
    assert_eq!(
        active
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["c"]
    );

    assert!(matches!(
        MemoryStoreSql::supersede(&connection, "a", "c", "user_choice"),
        Err(MemoryStoreError::InvalidTransition { .. })
    ));
    assert!(matches!(
        MemoryStoreSql::supersede(&connection, "c", "missing", "user_choice"),
        Err(MemoryStoreError::NotFound)
    ));
}

#[test]
fn expire_due_removes_records_from_retrieval() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &record("m-1", "Rust decision")).expect("insert");
    assert_eq!(
        MemoryStoreSql::expire_due(&connection, "2026-08-12T10:00:00Z").expect("nothing due"),
        0
    );
    assert_eq!(
        MemoryStoreSql::expire_due(&connection, "2027-06-01T00:00:00Z").expect("expire"),
        1
    );
    assert!(MemoryStoreSql::search(
        &connection,
        MemoryScope::Project,
        "project-1",
        "rust",
        "2026-09-01T00:00:00Z",
        10
    )
    .unwrap()
    .is_empty());
}

#[test]
fn forget_leaves_only_metadata_and_a_digest_tombstone() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    let mut source = record("m-1", "конфиденциальная деталь");
    source.extraction.evidence_refs = vec!["evidence-1".into()];
    source.extraction.execution_event_refs = vec![42];
    MemoryStoreSql::insert(&connection, &source).expect("insert");
    assert!(MemoryStoreSql::forget_with_tombstone(
        &connection,
        "m-1",
        "tomb-random-1",
        "user_request",
        "2026-08-14T00:00:00Z"
    )
    .expect("forget"));

    let forgotten = MemoryStoreSql::get_by_id(&connection, "m-1")
        .unwrap()
        .unwrap();
    assert!(forgotten.content.is_empty());
    assert!(forgotten.title.is_empty());
    assert!(forgotten.provenance.is_empty());
    assert!(forgotten.extraction.canonical_subject.is_none());
    assert!(forgotten.extraction.evidence_refs.is_empty());
    assert!(forgotten.extraction.execution_event_refs.is_empty());
    assert_eq!(forgotten.extraction.confirmation_state, "forgotten");

    let (digest, reason): (String, String) = connection
        .query_row(
            "SELECT digest, reason_class FROM memory_tombstones WHERE tombstone_id = ?1",
            params!["tomb-random-1"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("tombstone exists");
    assert_eq!(digest.len(), 64);
    assert!(!digest.contains("конфиденциальная"));
    assert_eq!(reason, "user_request");

    // Повторный forget не создаёт второй tombstone.
    assert!(!MemoryStoreSql::forget_with_tombstone(
        &connection,
        "m-1",
        "tomb-random-2",
        "user_request",
        "2026-08-14T00:00:00Z"
    )
    .expect("second forget is a no-op"));
}

#[test]
fn reused_tombstone_id_cannot_forget_another_memory_or_rewrite_audit() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &record("m-1", "first detail")).expect("insert first");
    MemoryStoreSql::insert(&connection, &record("m-2", "second detail")).expect("insert second");

    assert!(MemoryStoreSql::forget_with_tombstone(
        &connection,
        "m-1",
        "tomb-reused",
        "user_request",
        "2026-08-14T00:00:00Z"
    )
    .expect("first forget"));
    assert!(!MemoryStoreSql::forget_with_tombstone(
        &connection,
        "m-2",
        "tomb-reused",
        "retention",
        "2026-08-15T00:00:00Z"
    )
    .expect("reused tombstone is rejected"));

    let second = MemoryStoreSql::get_by_id(&connection, "m-2")
        .expect("second memory loads")
        .expect("second memory exists");
    assert_eq!(second.content, "second detail");
    let (scope_id, forgotten_at, reason): (String, String, String) = connection
        .query_row(
            "SELECT scope_id, forgotten_at, reason_class
                 FROM memory_tombstones WHERE tombstone_id = ?1",
            params!["tomb-reused"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("original tombstone remains");
    assert_eq!(scope_id, "project-1");
    assert_eq!(forgotten_at, "2026-08-14T00:00:00Z");
    assert_eq!(reason, "user_request");
}

#[test]
fn aliases_cannot_be_registered_by_model_inference() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::register_alias(
        &connection,
        MemoryScope::Project,
        "project-1",
        "ui язык",
        "entity:ui-language",
        "user",
        "2026-08-14T00:00:00Z",
    )
    .expect("user alias registers");
    assert!(matches!(
        MemoryStoreSql::register_alias(
            &connection,
            MemoryScope::Project,
            "project-1",
            "другой",
            "entity:other",
            "model_inference",
            "2026-08-14T00:00:00Z",
        ),
        Err(MemoryStoreError::Empty {
            field: "registered_by"
        })
    ));
    assert_eq!(
        MemoryStoreSql::list_aliases(&connection, MemoryScope::Project, "project-1").unwrap(),
        vec![("ui язык".to_owned(), "entity:ui-language".to_owned())]
    );
}

#[test]
fn alias_listing_is_bounded() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    for index in 0..(MAX_METADATA_ROWS + 1) {
        MemoryStoreSql::register_alias(
            &connection,
            MemoryScope::Project,
            "project-1",
            &format!("alias-{index:03}"),
            &format!("entity-{index:03}"),
            "user",
            "2026-08-14T00:00:00Z",
        )
        .unwrap();
    }
    assert_eq!(
        MemoryStoreSql::list_aliases(&connection, MemoryScope::Project, "project-1")
            .unwrap()
            .len(),
        MAX_METADATA_ROWS
    );
}

#[test]
fn session_notes_expire_and_never_touch_persistent_memory() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert_session_note(
        &connection,
        InsertSessionNoteInput {
            id: "note-1",
            session_id: "session-1",
            scope: MemoryScope::Session,
            scope_id: "project-1",
            kind: "preference",
            statement: "только на эту сессию: краткие ответы",
            created_at: "2026-08-14T00:00:00Z",
            expires_at: "2026-08-15T00:00:00Z",
        },
    )
    .expect("session note");

    assert_eq!(
        MemoryStoreSql::list_session_notes(&connection, "session-1", "2026-08-14T12:00:00Z")
            .unwrap()
            .len(),
        1
    );
    assert!(
        MemoryStoreSql::list_session_notes(&connection, "session-1", "2026-08-16T00:00:00Z")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        MemoryStoreSql::purge_expired_session_notes(&connection, "2026-08-16T00:00:00Z").unwrap(),
        1
    );
    // Persistent память при этом не создавалась.
    let persistent: i64 = connection
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
        .unwrap();
    assert_eq!(persistent, 0);
}

#[test]
fn session_note_listing_is_bounded() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    for index in 0..(MAX_METADATA_ROWS + 1) {
        MemoryStoreSql::insert_session_note(
            &connection,
            InsertSessionNoteInput {
                id: &format!("note-{index:03}"),
                session_id: "session-1",
                scope: MemoryScope::Session,
                scope_id: "project-1",
                kind: "context",
                statement: "temporary note",
                created_at: "2026-08-14T00:00:00Z",
                expires_at: "2026-09-15T00:00:00Z",
            },
        )
        .unwrap();
    }
    assert_eq!(
        MemoryStoreSql::list_session_notes(&connection, "session-1", "2026-08-14T12:00:00Z")
            .unwrap()
            .len(),
        MAX_METADATA_ROWS
    );
}

#[test]
fn revising_a_pending_statement_makes_it_a_user_assertion_without_confirming_it() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    let mut record = pending("p-1", "тема", "модель предложила так");
    record.extraction.source_trust = "model_inference".to_owned();
    record.extraction.validation_status = "valid".to_owned();
    record.extraction.verification_confidence = 0.9;
    MemoryStoreSql::insert(&connection, &record).expect("insert pending");

    MemoryStoreSql::revise_pending_statement(&connection, "p-1", "пользователь написал так")
        .expect("revision applies");
    let revised = MemoryStoreSql::get_by_id(&connection, "p-1")
        .unwrap()
        .unwrap();
    assert_eq!(revised.content, "пользователь написал так");
    assert_eq!(revised.extraction.source_trust, "user");
    assert_eq!(revised.extraction.extractor_version, "user_edited");
    // Прошлая проверка относилась к прежней формулировке.
    assert_eq!(revised.extraction.verification_confidence, 0.0);
    assert_eq!(revised.extraction.validation_status, "not_required");
    // Правка не подтверждает запись.
    assert_eq!(
        revised.extraction.confirmation_state,
        "pending_confirmation"
    );

    // Секреты не проникают в память через поле правки.
    MemoryStoreSql::revise_pending_statement(&connection, "p-1", "ключ sk-live-42")
        .expect("revision applies");
    let redacted = MemoryStoreSql::get_by_id(&connection, "p-1")
        .unwrap()
        .unwrap();
    assert!(!redacted.content.contains("sk-live-42"));

    // Уже решённую запись править нельзя.
    MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap();
    assert!(matches!(
        MemoryStoreSql::revise_pending_statement(&connection, "p-1", "поздно"),
        Err(MemoryStoreError::InvalidTransition { .. })
    ));
}

#[test]
fn candidate_state_never_reaches_retrieval_after_a_crash() {
    // A crash between the model call and the confirmation leaves a row in
    // `candidate` state. It must behave like nothing was ever learned:
    // invisible to search, still resolvable by the user afterwards.
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    let mut record = pending("c-1", "тема", "Rust decision");
    record.extraction.confirmation_state = "candidate".to_owned();
    MemoryStoreSql::insert(&connection, &record).expect("insert candidate");

    assert!(MemoryStoreSql::search(
        &connection,
        MemoryScope::Project,
        "project-1",
        "rust",
        "2026-09-01T00:00:00Z",
        10
    )
    .unwrap()
    .is_empty());
    assert!(MemoryStoreSql::conflict_candidates(
        &connection,
        MemoryScope::Project,
        "project-1",
        "preference",
        10
    )
    .unwrap()
    .is_empty());

    // Recovery can still route it through the normal approval path.
    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "c-1", "pending_confirmation").unwrap(),
        "pending_confirmation"
    );
}

#[test]
fn concurrent_decisions_on_one_record_converge_to_a_single_state() {
    // Two reviewers acting at once must not produce two transitions: the
    // second call reports the state the store actually holds.
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    MemoryStoreSql::insert(&connection, &pending("p-1", "тема", "утверждение"))
        .expect("insert pending");

    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "p-1", "confirmed").unwrap(),
        "confirmed"
    );
    // The losing decision is refused outright rather than silently
    // overwriting the winner.
    assert!(matches!(
        MemoryStoreSql::transition_state(&connection, "p-1", "rejected"),
        Err(MemoryStoreError::InvalidTransition {
            ref from,
            ref to
        }) if from == "confirmed" && to == "rejected"
    ));
    let stored = MemoryStoreSql::get_by_id(&connection, "p-1")
        .unwrap()
        .unwrap();
    assert_eq!(stored.extraction.confirmation_state, "confirmed");

    // The mirror case: once rejected, a later confirm is a safe no-op
    // that reports the real state instead of reopening the record.
    MemoryStoreSql::insert(&connection, &pending("p-2", "тема-2", "утверждение-2"))
        .expect("insert second");
    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "p-2", "rejected").unwrap(),
        "rejected"
    );
    assert_eq!(
        MemoryStoreSql::transition_state(&connection, "p-2", "confirmed").unwrap(),
        "rejected"
    );
}

#[test]
fn a_large_pending_queue_stays_bounded_per_read() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    for index in 0..250 {
        MemoryStoreSql::insert(
            &connection,
            &pending(
                &format!("p-{index:03}"),
                "тема",
                &format!("вариант {index}"),
            ),
        )
        .expect("insert");
    }
    let page = MemoryStoreSql::list_by_state(
        &connection,
        MemoryScope::Project,
        "project-1",
        "pending_confirmation",
        10_000,
    )
    .expect("bounded read");
    assert_eq!(
        page.len(),
        100,
        "read must stay bounded regardless of limit"
    );
    let counts = MemoryStoreSql::count_by_state(&connection, MemoryScope::Project, "project-1")
        .expect("counts");
    assert_eq!(counts, vec![("pending_confirmation".to_owned(), 250)]);
}

#[test]
fn secret_privacy_class_is_never_persisted() {
    let mut memory = record("m-1", "содержимое");
    memory.extraction.privacy_class = "secret".to_owned();
    assert!(matches!(
        memory.validate(),
        Err(MemoryStoreError::SecretNotStorable)
    ));
    memory.extraction.privacy_class = "normal".to_owned();
    memory.extraction.model_confidence = 1.5;
    assert!(matches!(
        memory.validate(),
        Err(MemoryStoreError::InvalidConfidence)
    ));
}

#[test]
fn v31_columns_install_idempotently_and_round_trip_refs() {
    let connection = Connection::open_in_memory().expect("connection");
    schema(&connection);
    install_schema(&connection).expect("first v31 install");
    install_schema(&connection).expect("second v31 install");
    let mut memory = record("refs", "with evidence");
    memory.extraction.evidence_refs = vec!["evidence-1".into()];
    memory.extraction.execution_event_refs = vec![42];
    memory.extraction.authority = "model_proposed".into();
    memory.extraction.durability = "durable".into();
    memory.extraction.confidence = 0.75;
    MemoryStoreSql::insert(&connection, &memory).expect("insert");
    let loaded = MemoryStoreSql::get_by_id(&connection, "refs")
        .expect("read")
        .expect("row");
    assert_eq!(loaded.extraction.record_version, 1);
    assert_eq!(loaded.extraction.evidence_refs, vec!["evidence-1"]);
    assert_eq!(loaded.extraction.execution_event_refs, vec![42]);
    assert_eq!(loaded.extraction.authority, "model_proposed");
    assert_eq!(loaded.extraction.durability, "durable");
    assert_eq!(loaded.extraction.confidence, 0.75);
}
