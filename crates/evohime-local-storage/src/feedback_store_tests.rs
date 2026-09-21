use super::*;

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE feedback_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    run_id TEXT NOT NULL,
                    task_id TEXT,
                    subject_ref TEXT,
                    signal TEXT NOT NULL,
                    correction TEXT,
                    rejection_reason TEXT,
                    outcome TEXT,
                    provenance TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );",
        )
        .expect("feedback schema creates");
}

fn record(id: &str, signal: FeedbackSignal, correction: Option<&str>) -> FeedbackRecord {
    FeedbackRecord::new(FeedbackRecordInput {
        id: id.to_owned(),
        run_id: "run-1".to_owned(),
        task_id: Some("task-1".to_owned()),
        subject_ref: Some("effect-1".to_owned()),
        signal,
        correction: correction.map(str::to_owned),
        rejection_reason: None,
        outcome: Some("tool_succeeded".to_owned()),
        provenance: "run:1".to_owned(),
        created_at: "2026-08-12T10:00:00Z".to_owned(),
    })
    .expect("feedback record builds")
}

#[test]
fn insert_and_list_round_trip_against_real_storage() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    FeedbackStoreSql::insert(&connection, &record("f-1", FeedbackSignal::Useful, None))
        .expect("insert f-1");
    FeedbackStoreSql::insert(
        &connection,
        &record(
            "f-2",
            FeedbackSignal::NotUseful,
            Some("should retry differently"),
        ),
    )
    .expect("insert f-2");

    let listed = FeedbackStoreSql::list_by_run(&connection, "run-1", 10).expect("list");
    assert_eq!(listed.len(), 2);
    assert_eq!(
        FeedbackStoreSql::get_by_id(&connection, "f-1")
            .unwrap()
            .unwrap()
            .signal,
        FeedbackSignal::Useful
    );
}

#[test]
fn aggregate_counts_signals_and_rejection_reasons() {
    let connection = Connection::open_in_memory().expect("sqlite opens");
    schema(&connection);
    FeedbackStoreSql::insert(&connection, &record("f-1", FeedbackSignal::Useful, None)).unwrap();
    FeedbackStoreSql::insert(&connection, &record("f-2", FeedbackSignal::Useful, None)).unwrap();
    let mut rejected = record("f-3", FeedbackSignal::NotUseful, None);
    rejected.rejection_reason = Some("wrong tool chosen".to_owned());
    FeedbackStoreSql::insert(&connection, &rejected).unwrap();
    let mut rejected2 = record("f-4", FeedbackSignal::NotUseful, None);
    rejected2.rejection_reason = Some("wrong tool chosen".to_owned());
    FeedbackStoreSql::insert(&connection, &rejected2).unwrap();

    let aggregate = FeedbackStoreSql::aggregate(&connection, 10, 10).expect("aggregate");
    assert_eq!(aggregate.useful_count, 2);
    assert_eq!(aggregate.not_useful_count, 2);
    assert_eq!(aggregate.neutral_count, 0);
    assert_eq!(
        aggregate.rejection_reasons,
        vec![("wrong tool chosen".to_owned(), 2)]
    );
    assert_eq!(aggregate.outcomes, vec![("tool_succeeded".to_owned(), 4)]);
}

#[test]
fn constructor_redacts_secret_shaped_correction_and_rejection_reason() {
    let record = FeedbackRecord::new(FeedbackRecordInput {
        id: "f-1".to_owned(),
        run_id: "run-1".to_owned(),
        task_id: None,
        subject_ref: None,
        signal: FeedbackSignal::NotUseful,
        correction: Some("use token=abc123 next time".to_owned()),
        rejection_reason: Some("leaked sk-abc123secret in the logs".to_owned()),
        outcome: None,
        provenance: "run:1".to_owned(),
        created_at: "2026-08-12T10:00:00Z".to_owned(),
    })
    .expect("builds");
    assert!(!record.correction.unwrap().contains("token=abc123"));
    assert!(!record.rejection_reason.unwrap().contains("sk-abc123secret"));
}

#[test]
fn field_bounds_are_enforced() {
    let too_long = FeedbackRecord::new(FeedbackRecordInput {
        id: "f-1".to_owned(),
        run_id: "run-1".to_owned(),
        task_id: None,
        subject_ref: None,
        signal: FeedbackSignal::Neutral,
        correction: Some("x".repeat(MAX_CORRECTION_BYTES + 1)),
        rejection_reason: None,
        outcome: None,
        provenance: "run:1".to_owned(),
        created_at: "2026-08-12T10:00:00Z".to_owned(),
    });
    assert!(matches!(
        too_long,
        Err(FeedbackStoreError::Limit {
            field: "correction",
            ..
        })
    ));
}

#[test]
fn external_telemetry_gate_defaults_closed_and_opens_only_when_explicit() {
    assert!(!external_telemetry_allowed(false));
    assert!(external_telemetry_allowed(true));
}
