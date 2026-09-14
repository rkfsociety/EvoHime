use rusqlite::Transaction;
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> Result<(), rusqlite::Error> {
    if current >= 171 {
        return Ok(());
    }
    super::super::language_intelligence_store::install_schema(transaction)?;
    transaction.execute_batch("PRAGMA user_version = 171;")
}
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    #[test]
    fn migration_installs_schema() {
        let c = Connection::open_in_memory().unwrap();
        let t = c.unchecked_transaction().unwrap();
        apply(&t, 170).unwrap();
        t.commit().unwrap();
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            171
        );
    }
}
