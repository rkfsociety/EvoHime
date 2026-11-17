use rusqlite::Transaction;

/// Persists provider-reported cache-hit input tokens alongside actual usage.
///
/// Legacy test fixtures and very old databases can reach this step without the
/// ledger tables, so each optional change is applied only when its table exists.
/// Production databases upgraded through v17 always have both tables.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 182 {
        if table_exists(transaction, "context_ledger_usage")? {
            transaction.execute_batch(
                "ALTER TABLE context_ledger_usage ADD COLUMN cache_read_input_tokens INTEGER;
                 CREATE INDEX IF NOT EXISTS idx_context_ledger_usage_recorded_at
                     ON context_ledger_usage(recorded_at);",
            )?;
        }
        if table_exists(transaction, "context_ledger")? {
            transaction.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_context_ledger_provider_created_at
                     ON context_ledger(provider, created_at);",
            )?;
        }
        transaction.execute_batch("PRAGMA user_version = 182;")?;
    }
    Ok(())
}

fn table_exists(transaction: &Transaction<'_>, name: &str) -> rusqlite::Result<bool> {
    transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [name],
        |row| row.get(0),
    )
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

    #[test]
    fn migration_tolerates_legacy_databases_without_ledger_tables() {
        let mut connection = Connection::open_in_memory().expect("database");
        let transaction = connection.transaction().expect("transaction");
        apply(&transaction, 181).expect("migration");
        transaction.commit().expect("commit");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, 182);
    }
}
