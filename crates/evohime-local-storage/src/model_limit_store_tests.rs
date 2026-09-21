use super::*;

fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE model_context_limits (
                model TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                context_tokens INTEGER,
                max_output_tokens INTEGER,
                fetched_at TEXT NOT NULL
             );",
        )
        .expect("schema is created");
}

fn record(model: &str, context: Option<u32>) -> ModelLimitRecord {
    ModelLimitRecord {
        model: model.into(),
        provider: "literouter".into(),
        context_tokens: context,
        max_output_tokens: Some(32_768),
    }
}

#[test]
fn stores_and_reads_back_a_window() {
    let connection = Connection::open_in_memory().expect("memory database opens");
    schema(&connection);

    ModelLimitStoreSql::upsert_all(&connection, &[record("a:free", Some(128_000))])
        .expect("limits are stored");

    let stored = ModelLimitStoreSql::get(&connection, "a:free").expect("read succeeds");
    assert_eq!(stored, Some(record("a:free", Some(128_000))));
}

/// Провайдер может не сообщить окно — тогда строка есть, а лимита нет, и
/// это должно читаться как «неизвестно», а не как ноль.
#[test]
fn an_unknown_window_reads_back_as_none() {
    let connection = Connection::open_in_memory().expect("memory database opens");
    schema(&connection);

    ModelLimitStoreSql::upsert_all(&connection, &[record("a", None)]).expect("limits stored");

    let stored = ModelLimitStoreSql::get(&connection, "a").expect("read succeeds");
    assert_eq!(stored.and_then(|record| record.context_tokens), None);
}

#[test]
fn a_later_catalogue_overwrites_the_previous_limits() {
    let connection = Connection::open_in_memory().expect("memory database opens");
    schema(&connection);

    ModelLimitStoreSql::upsert_all(&connection, &[record("a", Some(8_000))]).expect("stored");
    ModelLimitStoreSql::upsert_all(&connection, &[record("a", Some(256_000))]).expect("stored");

    let stored = ModelLimitStoreSql::list(&connection).expect("read succeeds");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].context_tokens, Some(256_000));
}

#[test]
fn model_listing_is_bounded() {
    let connection = Connection::open_in_memory().expect("memory database opens");
    schema(&connection);
    let records: Vec<_> = (0..300)
        .map(|index| record(&format!("model-{index:03}"), Some(128_000)))
        .collect();
    ModelLimitStoreSql::upsert_all(&connection, &records).expect("limits are stored");
    assert_eq!(
        ModelLimitStoreSql::list(&connection).unwrap().len(),
        MAX_MODELS as usize
    );
}

#[test]
fn an_empty_model_identifier_is_refused() {
    let connection = Connection::open_in_memory().expect("memory database opens");
    schema(&connection);

    let outcome = ModelLimitStoreSql::upsert_all(&connection, &[record("  ", Some(1_000))]);
    assert!(matches!(
        outcome,
        Err(ModelLimitStoreError::Empty { field: "model" })
    ));
}
