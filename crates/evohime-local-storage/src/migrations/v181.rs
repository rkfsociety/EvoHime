use rusqlite::Transaction;

/// Adds metadata-only persistence for the Core-owned A2A remote-agent bridge.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 181 {
        crate::external_agent_a2a_store::install_schema(transaction)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        transaction.execute_batch("PRAGMA user_version = 181;")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_creates_metadata_only_a2a_tables() {
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE unrelated (value TEXT); INSERT INTO unrelated VALUES ('kept');",
            )
            .expect("unrelated state");
        let transaction = connection.transaction().expect("transaction");
        apply(&transaction, 180).expect("migration");
        transaction.commit().expect("commit");

        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 181);

        for table in [
            "a2a_remote_profiles",
            "a2a_card_snapshots",
            "a2a_task_bindings",
        ] {
            let count: u32 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("table existence");
            assert_eq!(count, 1, "{table} must exist after migration");
        }

        let preserved: String = connection
            .query_row("SELECT value FROM unrelated", [], |row| row.get(0))
            .expect("unrelated row");
        assert_eq!(preserved, "kept");
    }

    #[test]
    fn migration_is_idempotent_when_tables_already_exist() {
        let connection = Connection::open_in_memory().expect("database");
        let transaction = connection.unchecked_transaction().expect("transaction");
        crate::external_agent_a2a_store::install_schema(&transaction).expect("first schema");
        apply(&transaction, 180).expect("migration");
        transaction.commit().expect("commit");
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .expect("schema version"),
            181
        );
    }
}
