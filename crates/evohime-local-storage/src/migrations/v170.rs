use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> Result<(), rusqlite::Error> {
    if current >= 170 {
        return Ok(());
    }
    super::super::multi_reviewer_ensemble_store::install_schema(transaction)?;
    transaction.execute_batch("PRAGMA user_version = 170;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    #[test]
    fn installs_ensemble_schema() {
        let c = Connection::open_in_memory().unwrap();
        let t = c.unchecked_transaction().unwrap();
        apply(&t, 169).unwrap();
        t.commit().unwrap();
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            170
        );
        assert!(c
            .query_row("SELECT COUNT(*) FROM reviewer_ensemble_runs", [], |r| r
                .get::<_, u32>(0))
            .is_ok());
    }
}
