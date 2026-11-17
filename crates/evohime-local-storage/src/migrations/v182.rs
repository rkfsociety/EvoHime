use rusqlite::Transaction;

/// Persists provider-reported cache-hit input tokens alongside actual usage.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 182 {
        transaction.execute_batch(
            "ALTER TABLE context_ledger_usage ADD COLUMN cache_read_input_tokens INTEGER;
             CREATE INDEX IF NOT EXISTS idx_context_ledger_usage_recorded_at
                 ON context_ledger_usage(recorded_at);
             CREATE INDEX IF NOT EXISTS idx_context_ledger_provider_created_at
                 ON context_ledger(provider, created_at);
             PRAGMA user_version = 182;",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_adds_nullable_cache_hit_usage_column() {
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE context_ledger_usage (
                    id INTEGER PRIMARY KEY, ledger_id TEXT NOT NULL,
                    actual_prompt_tokens INTEGER NOT NULL,
                    actual_completion_tokens INTEGER NOT NULL,
                    estimator_drift REAL NOT NULL, recorded_at INTEGER NOT NULL
                );
                CREATE TABLE context_ledger (
                    id TEXT PRIMARY KEY NOT NULL, provider TEXT NOT NULL,
                    model TEXT NOT NULL, created_at INTEGER NOT NULL
                );",
            )
            .expect("legacy usage schema");
        let transaction = connection.transaction().expect("transaction");
        apply(&transaction, 181).expect("migration");
        transaction.commit().expect("commit");

        let column_exists: u32 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('context_ledger_usage')
                 WHERE name = 'cache_read_input_tokens' AND \"notnull\" = 0",
                [],
                |row| row.get(0),
            )
            .expect("column metadata");
        assert_eq!(column_exists, 1);
    }
}
