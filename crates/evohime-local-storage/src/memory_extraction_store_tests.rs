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
    install_schema(connection).expect("lifecycle schema");
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
