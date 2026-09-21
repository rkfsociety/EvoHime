use super::*;
use evohime_model_provenance::{
    ContextProjection, ModelMessage, ModelParameters, ProjectionEntry, RequestKind, SourceRef,
    ToolSchema,
};

fn db() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE context_ledger(id TEXT PRIMARY KEY, context_ledger_hash TEXT NOT NULL)",
    )
    .unwrap();
    install_schema(&db).unwrap();
    db
}
fn envelope() -> ModelRequestEnvelopeV1 {
    let mut projection = ContextProjection {
        ledger_id: "l".into(),
        context_ledger_hash: "a".repeat(64),
        entries: vec![ProjectionEntry {
            projection_entry_id: "m".into(),
            operation: "include".into(),
            source_refs: vec![],
            block_ref_id: Some("b".into()),
            drop_reason: None,
        }],
        context_projection_hash: String::new(),
    };
    projection.context_projection_hash = projection.compute_hash().unwrap();
    ModelRequestEnvelopeV1 {
        version: 1,
        request_id: Uuid::now_v7().to_string(),
        logical_request_id: "logical".into(),
        attempt: 1,
        parent_request_id: None,
        ledger_id: "l".into(),
        request_kind: RequestKind::Agent,
        provider: "mock".into(),
        model: "m".into(),
        route_snapshot_hash: "b".repeat(64),
        policy_snapshot_hash: "c".repeat(64),
        route_policy_hash_shared: false,
        system_prompt: "system".into(),
        messages: vec![ModelMessage {
            role: "user".into(),
            content: "hello".into(),
        }],
        tools: vec![ToolSchema {
            name: "tool".into(),
            description: "tool".into(),
            input_schema: serde_json::json!({"type":"object"}),
        }],
        model_parameters: ModelParameters {
            temperature: None,
            top_p: None,
            max_output_tokens: Some(10),
            reasoning_mode: None,
            provider_options: Default::default(),
        },
        context_projection: projection,
        previous_request_hash: None,
    }
}

#[test]
fn commit_is_idempotent_and_deduplicates_blocks() {
    let db = db();
    db.execute(
        "INSERT INTO context_ledger VALUES('l',?1)",
        ["a".repeat(64)],
    )
    .unwrap();
    let repo = ModelProvenanceRepository::new(&db);
    let one = envelope();
    let first = repo
        .commit_envelope(&one, CommitMode::FullForDispatch)
        .unwrap();
    let second = repo
        .commit_envelope(&one, CommitMode::FullForDispatch)
        .unwrap();
    assert_eq!(first, second);
    assert!(!String::from_utf8_lossy(&first.envelope_blob).contains("hello"));
    assert!(!String::from_utf8_lossy(&first.envelope_blob).contains("\\\"content\\\""));
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM model_request_blocks", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
}

#[test]
fn full_commit_accepts_nested_provenance_source_refs() {
    let db = db();
    db.execute(
        "INSERT INTO context_ledger VALUES('l',?1)",
        ["a".repeat(64)],
    )
    .unwrap();
    let mut request = envelope();
    request.context_projection.entries[0]
        .source_refs
        .push(SourceRef {
            source_ref_id: "source-ref".into(),
            source_kind: "workspace_file".into(),
            source_id: "README.md".into(),
            source_version: Some("workspace-v1".into()),
            classification: "internal".into(),
        });
    request.context_projection.context_projection_hash =
        request.context_projection.compute_hash().unwrap();

    ModelProvenanceRepository::new(&db)
        .commit_envelope(&request, CommitMode::FullForDispatch)
        .expect("model provenance depth limit must allow source references");
}

#[test]
fn repeated_source_refs_are_scoped_to_each_request() {
    let db = db();
    db.execute(
        "INSERT INTO context_ledger VALUES('l',?1)",
        ["a".repeat(64)],
    )
    .unwrap();
    let mut first = envelope();
    first.logical_request_id = "logical-first".into();
    first.context_projection.entries[0]
        .source_refs
        .push(SourceRef {
            source_ref_id: "instruction:workspace".into(),
            source_kind: "project_instruction".into(),
            source_id: "workspace".into(),
            source_version: Some("v1".into()),
            classification: "internal".into(),
        });
    first.context_projection.context_projection_hash =
        first.context_projection.compute_hash().unwrap();
    let mut second = first.clone();
    second.request_id = Uuid::now_v7().to_string();
    second.logical_request_id = "logical-second".into();

    let repo = ModelProvenanceRepository::new(&db);
    repo.commit_envelope(&first, CommitMode::FullForDispatch)
        .unwrap();
    repo.commit_envelope(&second, CommitMode::FullForDispatch)
        .expect("the same stable source reference may be reused by another request");

    assert_eq!(
        MODEL_PROVENANCE_SCHEMA_VERSION, 3,
        "schema constant must match the installed migration"
    );
    assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM model_request_sources WHERE source_ref_id='instruction:workspace'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            2
        );
}

#[test]
fn migrates_legacy_global_source_ref_uniqueness() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
            "PRAGMA foreign_keys=OFF;
             CREATE TABLE model_provenance_meta(id INTEGER PRIMARY KEY CHECK(id=1), schema_version INTEGER NOT NULL);
             INSERT INTO model_provenance_meta VALUES(1,2);
             CREATE TABLE model_requests (
               request_id TEXT PRIMARY KEY, logical_request_id TEXT NOT NULL, attempt INTEGER NOT NULL CHECK(attempt > 0),
               parent_request_id TEXT, previous_request_hash TEXT, request_kind TEXT NOT NULL, ledger_id TEXT NOT NULL,
               provider TEXT NOT NULL, model TEXT NOT NULL, envelope_version INTEGER NOT NULL,
               payload_mode TEXT NOT NULL CHECK(payload_mode IN ('full','hash_only')), envelope_hash TEXT,
               envelope_blob BLOB NOT NULL, context_projection_hash TEXT NOT NULL, route_snapshot_hash TEXT NOT NULL,
               policy_snapshot_hash TEXT NOT NULL, route_policy_hash_shared INTEGER NOT NULL CHECK(route_policy_hash_shared IN (0,1)),
               status TEXT NOT NULL CHECK(status IN ('active','completed','failed','interrupted','unknown_outcome','redacted','retention_pruned')),
               dispatch_at INTEGER, completed_at INTEGER, UNIQUE(logical_request_id, attempt),
               CHECK((payload_mode='full' AND envelope_hash IS NOT NULL) OR (payload_mode='hash_only' AND envelope_hash IS NULL)),
               CHECK((attempt=1 AND parent_request_id IS NULL AND previous_request_hash IS NULL) OR (attempt>1 AND parent_request_id IS NOT NULL AND previous_request_hash IS NOT NULL))
             );
             INSERT INTO model_requests(request_id,logical_request_id,attempt,request_kind,ledger_id,provider,model,envelope_version,payload_mode,envelope_hash,envelope_blob,context_projection_hash,route_snapshot_hash,policy_snapshot_hash,route_policy_hash_shared,status)
               VALUES('legacy-request','legacy-logical',1,'agent','legacy-ledger','mock','m',1,'full','hash',X'00','a','b','c',0,'active');
             CREATE TABLE model_request_sources (
               request_id TEXT NOT NULL REFERENCES model_requests(request_id), ordinal INTEGER NOT NULL,
               source_ref_id TEXT NOT NULL UNIQUE, source_kind TEXT NOT NULL, source_id TEXT NOT NULL,
               source_version TEXT, source_hash TEXT, PRIMARY KEY(request_id, ordinal)
             );
             INSERT INTO model_request_sources VALUES('legacy-request',0,'instruction:workspace','project_instruction','workspace','v1',NULL);",
        )
        .unwrap();

    install_schema(&db).unwrap();

    assert_eq!(
        db.query_row(
            "SELECT schema_version FROM model_provenance_meta WHERE id=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        3
    );
    assert_eq!(
        db.query_row(
            "SELECT source_ref_id FROM model_request_sources WHERE request_id='legacy-request'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "instruction:workspace"
    );
    let source_sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='model_request_sources'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!source_sql
        .to_ascii_lowercase()
        .contains("source_ref_id text not null unique"));
}

#[test]
fn failed_lineage_does_not_leave_rows() {
    let db = db();
    db.execute(
        "INSERT INTO context_ledger VALUES('l',?1)",
        ["a".repeat(64)],
    )
    .unwrap();
    let repo = ModelProvenanceRepository::new(&db);
    let mut one = envelope();
    one.attempt = 2;
    one.parent_request_id = Some("missing".into());
    one.previous_request_hash = Some("d".repeat(64));
    assert!(repo
        .commit_envelope(&one, CommitMode::FullForDispatch)
        .is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM model_requests", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
