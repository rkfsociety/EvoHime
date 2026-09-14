use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> Result<(), rusqlite::Error> {
    if current >= 168 {
        return Ok(());
    }
    super::super::hardware_fit_evidence_store::install_schema(transaction)?;
    transaction.execute_batch("PRAGMA user_version = 168;")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_installs_schema_atomically() {
        let connection = Connection::open_in_memory().expect("sqlite");
        let transaction = connection.unchecked_transaction().expect("transaction");
        apply(&transaction, 167).expect("migration");
        transaction.commit().expect("commit");
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("version"),
            168
        );
        assert!(connection
            .query_row(
                "SELECT COUNT(*) FROM hardware_fit_observations",
                [],
                |row| row.get::<_, u32>(0)
            )
            .is_ok());
    }
}
