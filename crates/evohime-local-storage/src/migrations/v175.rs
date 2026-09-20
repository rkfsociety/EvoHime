use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 175 {
        super::super::memory_extraction_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 175;")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_installs_memory_extraction_lifecycle_schema() {
        let connection = Connection::open_in_memory().expect("sqlite");
        let transaction = connection.unchecked_transaction().expect("transaction");
        apply(&transaction, 174).expect("migration");
        transaction.commit().expect("commit");
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("schema version"),
            175
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type='table' AND name='memory_extraction_lifecycle'",
                    [],
                    |row| row.get::<_, u32>(0),
                )
                .expect("table exists"),
            1
        );
    }
}
