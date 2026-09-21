use super::*;

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE research_evidence (
                    id TEXT PRIMARY KEY NOT NULL,
                    source_kind TEXT NOT NULL,
                    source_ref TEXT NOT NULL,
                    redacted_excerpt TEXT NOT NULL,
                    source_hash TEXT NOT NULL,
                    fetched_at TEXT NOT NULL,
                    ttl_seconds INTEGER NOT NULL,
                    provenance_link TEXT
                );",
        )
        .expect("contract fixture creates");
}

fn record(id: &str, provenance_link: Option<&str>) -> ResearchEvidenceRecord {
    ResearchEvidenceRecord {
        id: id.into(),
        source_kind: "url".into(),
        source_ref: "https://example.test/source".into(),
        redacted_excerpt: "redacted result".into(),
        source_hash: "sha256:abc".into(),
        fetched_at: "2026-08-12T10:00:00Z".into(),
        ttl_seconds: 3600,
        provenance_link: provenance_link.map(str::to_owned),
    }
}

#[test]
fn round_trips_record_without_schema_migration() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    let expected = record("evidence-1", Some("run:1"));

    ResearchEvidenceSql::insert(&connection, &expected).expect("evidence inserts");

    assert_eq!(
        ResearchEvidenceSql::get_by_id(&connection, "evidence-1").expect("evidence reads"),
        Some(expected)
    );
}

#[test]
fn lists_by_provenance_in_deterministic_order_and_deletes() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    ResearchEvidenceSql::insert(&connection, &record("b", Some("task:1")))
        .expect("first evidence inserts");
    ResearchEvidenceSql::insert(&connection, &record("a", Some("task:1")))
        .expect("second evidence inserts");
    ResearchEvidenceSql::insert(&connection, &record("c", Some("task:2")))
        .expect("third evidence inserts");

    let records =
        ResearchEvidenceSql::list_by_provenance(&connection, "task:1").expect("evidence lists");
    assert_eq!(
        records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert!(ResearchEvidenceSql::delete_by_id(&connection, "a").expect("evidence deletes"));
    assert!(!ResearchEvidenceSql::delete_by_id(&connection, "missing").expect("missing delete"));
}

#[test]
fn rejects_unbounded_or_empty_contract_fields_before_sql() {
    let mut invalid = record("evidence-1", None);
    invalid.redacted_excerpt = "x".repeat(MAX_EXCERPT_BYTES + 1);
    assert!(matches!(
        invalid.validate(),
        Err(ResearchEvidenceError::Limit {
            field: "redacted_excerpt",
            ..
        })
    ));

    invalid = record("evidence-1", None);
    invalid.source_kind.clear();
    assert_eq!(
        invalid.validate(),
        Err(ResearchEvidenceError::Empty {
            field: "source_kind"
        })
    );

    invalid = record("evidence-1", None);
    invalid.ttl_seconds = MAX_TTL_SECONDS + 1;
    assert!(matches!(
        invalid.validate(),
        Err(ResearchEvidenceError::Limit {
            field: "ttl_seconds",
            ..
        })
    ));
}
