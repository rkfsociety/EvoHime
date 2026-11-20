use rusqlite::Transaction;

/// Attributes every actual-usage row to the configured route that produced it.
///
/// Token quotas are configured per route, and several routes can share one
/// transport kind. Recording the route scope keeps quota accounting from mixing
/// the usage of unrelated accounts. Legacy rows keep an empty scope and are
/// therefore never counted for a configured route quota.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 183 {
        if table_exists(transaction, "context_ledger_usage")? {
            transaction.execute_batch(
                "ALTER TABLE context_ledger_usage ADD COLUMN route_scope TEXT NOT NULL DEFAULT '';
                 CREATE INDEX IF NOT EXISTS idx_context_ledger_usage_route_scope
                     ON context_ledger_usage(route_scope, recorded_at);",
            )?;
        }
        transaction.execute_batch("PRAGMA user_version = 183;")?;
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
    fn migration_adds_route_scope_column_with_default() {
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE context_ledger_usage (
                    id INTEGER PRIMARY KEY, ledger_id TEXT NOT NULL,
                    actual_prompt_tokens INTEGER NOT NULL,
                    actual_completion_tokens INTEGER NOT NULL,
                    estimator_drift REAL NOT NULL, recorded_at INTEGER NOT NULL
                );
                INSERT INTO context_ledger_usage (
                    ledger_id, actual_prompt_tokens, actual_completion_tokens,
                    estimator_drift, recorded_at
                ) VALUES ('legacy', 1, 2, 0.0, 5);",
            )
            .expect("legacy usage schema");
        let transaction = connection.transaction().expect("transaction");
        apply(&transaction, 182).expect("migration");
        transaction.commit().expect("commit");

        let scope: String = connection
            .query_row(
                "SELECT route_scope FROM context_ledger_usage WHERE ledger_id = 'legacy'",
                [],
                |row| row.get(0),
            )
            .expect("legacy row scope");
        assert_eq!(scope, "");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, 183);
    }

    #[test]
    fn migration_tolerates_legacy_databases_without_ledger_tables() {
        let mut connection = Connection::open_in_memory().expect("database");
        let transaction = connection.transaction().expect("transaction");
        apply(&transaction, 182).expect("migration");
        transaction.commit().expect("commit");
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, 183);
    }
}
