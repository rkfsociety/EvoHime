use super::*;

#[test]
fn schema_has_no_secret_payload_columns() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let names: Vec<String> = c
        .prepare("PRAGMA table_info(event_trigger_events)")
        .unwrap()
        .query_map([], |r| r.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        !names
            .iter()
            .any(|n| n.contains("secret") || n.contains("credential"))
    );
    assert!(record_dedup(&c, "t", "k", "e", 10).unwrap());
    assert!(!record_dedup(&c, "t", "k", "e2", 10).unwrap());
    record_event(
        &c,
        &serde_json::json!({"safe": true}),
        &EventRecordMeta {
            event_id: "e",
            trigger_id: "t",
            outcome: "pending",
            correlation_id: "c",
            accepted_at_ms: 1,
            expires_at_ms: 2,
        },
    )
    .unwrap();
    assert_eq!(
        c.query_row(
            "SELECT outcome FROM event_trigger_events WHERE event_id='e'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending"
    );
    record_event(
        &c,
        &serde_json::json!({"safe": false}),
        &EventRecordMeta {
            event_id: "e",
            trigger_id: "t",
            outcome: "failed",
            correlation_id: "replacement",
            accepted_at_ms: 2,
            expires_at_ms: 3,
        },
    )
    .unwrap();
    assert_eq!(
        c.query_row(
            "SELECT outcome FROM event_trigger_events WHERE event_id='e'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "pending"
    );
}

#[test]
fn definition_payload_is_bounded() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let oversized = "x".repeat(MAX_DEFINITION_BYTES);
    assert!(put_definition(&c, "t", "scope", &oversized, "hash", 1, 1).is_err());
}

#[test]
fn duplicate_definition_version_is_idempotent() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    put_definition(
        &c,
        "t",
        "scope",
        &serde_json::json!({"first": true}),
        "first",
        1,
        1,
    )
    .unwrap();
    put_definition(
        &c,
        "t",
        "scope",
        &serde_json::json!({"first": false}),
        "second",
        1,
        2,
    )
    .unwrap();
    assert_eq!(
        get_definition::<serde_json::Value>(&c, "t", "scope").unwrap(),
        Some(serde_json::json!({"first": true}))
    );
    assert_eq!(
        get_definition::<serde_json::Value>(&c, "t", "other-scope").unwrap(),
        None
    );
    let listed = list_definitions::<serde_json::Value>(&c, "scope").unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].definition, serde_json::json!({"first": true}));
    assert_eq!(listed[0].version, 1);
    assert!(
        list_definitions::<serde_json::Value>(&c, "other-scope")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn event_history_is_scoped_and_returns_metadata_only() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    put_definition(
        &c,
        "trigger",
        "owner-a",
        &serde_json::json!({"trigger_id":"trigger"}),
        "hash",
        1,
        1,
    )
    .unwrap();
    record_event(
        &c,
        &serde_json::json!({"secret_payload":"must not be projected"}),
        &EventRecordMeta {
            event_id: "event-1",
            trigger_id: "trigger",
            outcome: "pending",
            correlation_id: "correlation-1",
            accepted_at_ms: 2,
            expires_at_ms: 100,
        },
    )
    .unwrap();
    let events = list_events(&c, "trigger", "owner-a", 10, 20).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, "event-1");
    assert_eq!(events[0].outcome, "pending");
    assert!(
        list_events(&c, "trigger", "owner-b", 10, 20)
            .unwrap()
            .is_empty()
    );
}
