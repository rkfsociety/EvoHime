use super::*;

fn checkpoint() -> TaskCheckpointV1 {
    TaskCheckpointV1 {
        id: "checkpoint-1".into(),
        version: TASK_CHECKPOINT_VERSION,
        workspace_id: "workspace-1".into(),
        chat_id: Some("chat-1".into()),
        goal_id: Some("goal-1".into()),
        parent_checkpoint_id: None,
        objective: "Implement the checkpoint contract".into(),
        status: CheckpointStatus::InProgress,
        completed_items: vec![CheckpointItem::core("repository inspected", "event:1")],
        remaining_items: vec![CheckpointItem::model("add runtime integration", "model:1")],
        decisions: vec![CheckpointDecision::core(
            "Core owns checkpoint state",
            "policy:1",
        )],
        blockers: Vec::new(),
        files_read: vec![FileReadRef::core("docs/architecture.md", "file:1")],
        files_changed: Vec::new(),
        tests_passed: vec![TestEvidence::core(
            "contract test",
            TestStatus::Passed,
            "test:1",
        )],
        tests_failed: Vec::new(),
        gates: vec![GateEvidence::core("storage", GateStatus::Passed, "gate:1")],
        pending_approvals: Vec::new(),
        workflow_refs: Vec::new(),
        child_refs: Vec::new(),
        artifact_refs: Vec::new(),
        open_questions: vec![CheckpointItem::model(
            "Should UI expose history?",
            "model:2",
        )],
        next_action: Some(CheckpointItem::model("Implement runtime hook", "model:3")),
        narrative_summary: Some(CheckpointItem::model("Bounded summary", "model:4")),
        source_event_seq: 1,
        created_at: 1_700_000_000_000,
        content_hash: String::new(),
    }
}

#[test]
fn sealing_is_deterministic_and_validation_rejects_tampering() {
    let sealed = checkpoint().seal().expect("checkpoint seals");
    let second = checkpoint().seal().expect("same checkpoint seals");
    assert_eq!(sealed.content_hash, second.content_hash);
    assert_eq!(
        sealed.canonical_json().unwrap(),
        second.canonical_json().unwrap()
    );
    let expected_json = r#"{"id":"checkpoint-1","version":1,"workspace_id":"workspace-1","chat_id":"chat-1","goal_id":"goal-1","parent_checkpoint_id":null,"objective":"Implement the checkpoint contract","status":"in_progress","completed_items":[{"text":"repository inspected","provenance":{"core_derived":{"source":"event:1"}}}],"remaining_items":[{"text":"add runtime integration","provenance":{"model_proposed":{"source":"model:1"}}}],"decisions":[{"text":"Core owns checkpoint state","provenance":{"core_derived":{"source":"policy:1"}}}],"blockers":[],"files_read":[{"path":"docs/architecture.md","evidence_ref":"file:1","provenance":{"core_derived":{"source":"core:file-read"}}}],"files_changed":[],"tests_passed":[{"name":"contract test","status":"passed","evidence_ref":"test:1","provenance":{"core_derived":{"source":"core:test-run"}}}],"tests_failed":[],"gates":[{"id":"storage","status":"passed","evidence_ref":"gate:1","provenance":{"core_derived":{"source":"core:gate"}}}],"pending_approvals":[],"workflow_refs":[],"child_refs":[],"artifact_refs":[],"open_questions":[{"text":"Should UI expose history?","provenance":{"model_proposed":{"source":"model:2"}}}],"next_action":{"text":"Implement runtime hook","provenance":{"model_proposed":{"source":"model:3"}}},"narrative_summary":{"text":"Bounded summary","provenance":{"model_proposed":{"source":"model:4"}}},"source_event_seq":1,"created_at":1700000000000,"content_hash":"e33e378d675e088554628371bee6e6f6a03ce56a2d1658d3be1d38fc5dcf3e3d"}"#;
    assert_eq!(
        String::from_utf8(sealed.canonical_json().unwrap()).unwrap(),
        expected_json
    );
    assert_eq!(
        sealed.content_hash,
        "e33e378d675e088554628371bee6e6f6a03ce56a2d1658d3be1d38fc5dcf3e3d"
    );
    sealed.validate().expect("sealed checkpoint validates");

    let mut tampered = sealed.clone();
    tampered.objective.push_str(" changed");
    assert!(matches!(
        tampered.validate(),
        Err(TaskCheckpointError::ContentHashMismatch { .. })
    ));
}

#[test]
fn model_proposed_evidence_cannot_confirm_effects_or_tests() {
    let mut invalid = checkpoint();
    invalid.tests_passed[0].provenance = Provenance::model("model:forged");
    let error = invalid.seal().expect_err("model evidence must be rejected");
    assert!(matches!(
        error,
        TaskCheckpointError::AuthorityViolation { .. }
    ));
}

#[test]
fn secrets_and_workspace_escape_are_rejected_before_persistence() {
    let mut secret = checkpoint();
    secret.objective = "api_key=super-secret-value".into();
    assert!(matches!(
        secret.seal(),
        Err(TaskCheckpointError::SensitiveText { .. })
    ));

    let mut traversal = checkpoint();
    traversal.files_read[0].path = "../outside.txt".into();
    assert!(matches!(
        traversal.seal(),
        Err(TaskCheckpointError::InvalidPath(_))
    ));
}

#[test]
fn store_is_immutable_idempotent_and_skips_corrupt_latest() {
    let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema installs");
    let store = TaskCheckpointStore::new(&connection);
    let mut first = checkpoint();
    first.status = CheckpointStatus::Paused;
    let first = first.seal().expect("checkpoint seals");
    assert_eq!(store.insert(&first).unwrap(), InsertOutcome::Inserted);
    assert_eq!(store.insert(&first).unwrap(), InsertOutcome::AlreadyPresent);
    assert_eq!(store.get(&first.id).unwrap(), Some(first.clone()));
    assert_eq!(store.get("missing").unwrap(), None);
    assert_eq!(
        store
            .latest_valid_for_chat("workspace-1", "chat-1")
            .unwrap(),
        Some(first.clone())
    );

    let mut conflicting = first.clone();
    conflicting.objective = "different".into();
    conflicting.content_hash = String::new();
    conflicting = conflicting.seal().expect("conflicting checkpoint seals");
    assert!(matches!(
        store.insert(&conflicting),
        Err(StorageError::TaskCheckpoint(
            TaskCheckpointError::ImmutableConflict { .. }
        ))
    ));

    let mut child = checkpoint();
    child.id = "checkpoint-2".into();
    child.parent_checkpoint_id = Some(first.id.clone());
    child.source_event_seq = 2;
    child = child.seal().expect("child seals");
    assert_eq!(store.insert(&child).unwrap(), InsertOutcome::Inserted);
    let mut grandchild = checkpoint();
    grandchild.id = "checkpoint-3".into();
    grandchild.parent_checkpoint_id = Some(child.id.clone());
    grandchild.source_event_seq = 3;
    grandchild = grandchild.seal().expect("grandchild seals");
    assert_eq!(store.insert(&grandchild).unwrap(), InsertOutcome::Inserted);
    assert_eq!(store.list("workspace-1", 10).unwrap().len(), 3);
    assert_eq!(
        store.latest_valid("workspace-1").unwrap().unwrap().id,
        grandchild.id
    );

    connection
        .execute(
            "UPDATE task_checkpoints SET canonical_json = ?1 WHERE id = ?2",
            rusqlite::params![b"{\"not\":\"a checkpoint\"}", child.id],
        )
        .expect("test corruption writes");
    let latest = store
        .latest_valid("workspace-1")
        .unwrap()
        .expect("fallback");
    assert_eq!(latest.id, first.id);

    connection
        .execute(
            "UPDATE task_checkpoints SET content_hash = ?1 WHERE id = ?2",
            rusqlite::params!["0".repeat(64), first.id],
        )
        .expect("test metadata corruption writes");
    assert!(matches!(
        store.get(&first.id),
        Err(StorageError::TaskCheckpoint(
            TaskCheckpointError::InvalidStoredMetadata {
                field: "metadata",
                ..
            }
        ))
    ));
}

#[test]
fn parent_must_be_same_workspace_and_older_event_sequence() {
    let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema installs");
    let store = TaskCheckpointStore::new(&connection);
    let first = checkpoint().seal().expect("checkpoint seals");
    store.insert(&first).expect("parent inserts");

    let mut child = checkpoint();
    child.id = "checkpoint-2".into();
    child.parent_checkpoint_id = Some(first.id.clone());
    child.source_event_seq = first.source_event_seq;
    let child = child.seal().expect("child seals");
    assert!(matches!(
        store.insert(&child),
        Err(StorageError::TaskCheckpoint(
            TaskCheckpointError::ParentSequenceNotNewer
        ))
    ));
}

#[test]
fn version_unknown_fields_secret_refs_and_bounds_fail_closed() {
    let mut unknown_version = checkpoint();
    unknown_version.version = 2;
    assert!(matches!(
        unknown_version.seal(),
        Err(TaskCheckpointError::UnsupportedVersion(2))
    ));

    let mut oversized = checkpoint();
    oversized.narrative_summary = Some(CheckpointItem::model(
        "x".repeat(TASK_CHECKPOINT_MAX_SUMMARY_CHARS + 1),
        "model:oversized",
    ));
    assert!(matches!(
        oversized.seal(),
        Err(TaskCheckpointError::InvalidField {
            field: "narrative_summary.text",
            ..
        })
    ));

    let mut secret_ref = checkpoint();
    secret_ref.artifact_refs.push(CheckpointRef {
        id: "artifact-1".into(),
        kind: "artifact".into(),
        content_hash: None,
        sensitivity: CheckpointSensitivity::Secret,
        provenance: Provenance::core("core:artifact"),
    });
    assert!(matches!(
        secret_ref.seal(),
        Err(TaskCheckpointError::InvalidField {
            field: "artifact_refs",
            ..
        })
    ));

    let mut many = checkpoint();
    many.completed_items =
        vec![CheckpointItem::core("done", "event:1"); TASK_CHECKPOINT_MAX_ITEMS + 1];
    assert!(matches!(
        many.seal(),
        Err(TaskCheckpointError::InvalidField {
            field: "completed_items",
            ..
        })
    ));

    let json = serde_json::to_value(checkpoint()).unwrap();
    let mut object = json.as_object().unwrap().clone();
    object.insert("unexpected_authority".into(), serde_json::json!(true));
    let error = serde_json::from_value::<TaskCheckpointWire>(serde_json::Value::Object(object))
        .expect_err("unknown fields must not be accepted");
    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn invalid_status_transition_is_rejected() {
    let connection = rusqlite::Connection::open_in_memory().expect("sqlite");
    install_schema(&connection).expect("schema installs");
    let store = TaskCheckpointStore::new(&connection);
    let mut first = checkpoint();
    first.status = CheckpointStatus::Paused;
    let first = first.seal().expect("checkpoint seals");
    store.insert(&first).expect("parent inserts");

    let mut child = checkpoint();
    child.id = "checkpoint-2".into();
    child.parent_checkpoint_id = Some(first.id.clone());
    child.source_event_seq = 2;
    child.status = CheckpointStatus::Completed;
    child = child.seal().expect("child seals");
    assert!(matches!(
        store.insert(&child),
        Err(StorageError::TaskCheckpoint(
            TaskCheckpointError::InvalidStateTransition {
                from: CheckpointStatus::Paused,
                to: CheckpointStatus::Completed,
            }
        ))
    ));
}

#[test]
fn secret_shaped_identifiers_and_refs_are_rejected() {
    let mut invalid = checkpoint();
    invalid.id = "token=not-for-storage".into();
    assert!(matches!(
        invalid.seal(),
        Err(TaskCheckpointError::SensitiveText { field: "id" })
    ));

    let mut invalid = checkpoint();
    invalid.files_read[0].evidence_ref = Some("secret-ref".into());
    assert!(matches!(
        invalid.seal(),
        Err(TaskCheckpointError::SensitiveText {
            field: "files_read.evidence_ref"
        })
    ));
}

#[test]
fn hash_uses_normalized_fields() {
    let sealed = checkpoint().seal().expect("checkpoint seals");
    let mut padded = checkpoint();
    padded.objective = "  Implement the checkpoint contract  ".into();
    padded.files_read[0].path = "docs\\architecture.md".into();
    let padded = padded.seal().expect("padded checkpoint seals");
    assert_eq!(sealed.content_hash, padded.content_hash);
    assert_eq!(
        sealed.canonical_json().unwrap(),
        padded.canonical_json().unwrap()
    );
}

#[test]
fn existing_schema_31_gets_checkpoint_table_with_backup() {
    let path = std::env::temp_dir().join(format!(
        "evohime-task-checkpoint-migration-{}.db",
        std::process::id()
    ));
    let backup = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup);
    {
        let connection = rusqlite::Connection::open(&path).expect("sqlite");
        connection
            .execute_batch("CREATE TABLE legacy_marker(value TEXT); PRAGMA user_version = 31;")
            .expect("legacy schema seeds");
    }

    let database = crate::LocalDatabase::open(&path).expect("schema migrates");
    assert_eq!(database.schema_version().unwrap(), crate::SCHEMA_VERSION);
    let table_exists: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'task_checkpoints'",
            [],
            |row| row.get(0),
        )
        .expect("table lookup");
    assert_eq!(table_exists, 1);
    assert!(backup.exists());
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup);
}
