use rusqlite::Transaction;

/// Adds persistence for user-selected public GitHub repositories.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 180 {
        crate::github_repository_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 180;")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_creates_bounded_github_repository_registry() {
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE unrelated (value TEXT); INSERT INTO unrelated VALUES ('preserved');",
            )
            .expect("unrelated state");
        let transaction = connection.transaction().expect("transaction");
        apply(&transaction, 179).expect("migration");
        transaction.commit().expect("commit");

        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("schema version");
        assert_eq!(version, 180);

        let columns: Vec<String> = connection
            .prepare("SELECT name FROM pragma_table_info('github_saved_repositories') ORDER BY cid")
            .expect("table info")
            .query_map([], |row| row.get(0))
            .expect("columns")
            .collect::<Result<_, _>>()
            .expect("column names");
        assert_eq!(
            columns,
            vec![
                "owner".to_owned(),
                "repo".to_owned(),
                "created_at_ms".to_owned()
            ]
        );
        let preserved: String = connection
            .query_row("SELECT value FROM unrelated", [], |row| row.get(0))
            .expect("unrelated row");
        assert_eq!(preserved, "preserved");
    }
}
