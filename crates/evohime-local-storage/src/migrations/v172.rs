use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 172 {
        super::super::free_access_evidence_store::install_schema(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 172;")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_installs_scoped_evidence_schema() {
        let connection = Connection::open_in_memory().expect("sqlite");
        let transaction = connection.unchecked_transaction().expect("transaction");
        apply(&transaction, 171).expect("migration");
        transaction.commit().expect("commit");
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("schema version"),
            172
        );
        assert!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='free_access_evidence'",
                    [],
                    |row| row.get::<_, u32>(0)
                )
                .expect("table exists")
                == 1
        );
    }
}
