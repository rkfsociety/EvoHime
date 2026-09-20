use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 174 {
        if !has_column(transaction, "state")? {
            transaction.execute_batch(
                "ALTER TABLE provider_profile_catalog_snapshots
                 ADD COLUMN state TEXT NOT NULL DEFAULT 'fresh';",
            )?;
        }
        if !has_column(transaction, "observed_at_ms")? {
            transaction.execute_batch(
                "ALTER TABLE provider_profile_catalog_snapshots
                 ADD COLUMN observed_at_ms INTEGER NOT NULL DEFAULT 1;",
            )?;
        }
        if !has_column(transaction, "expires_at_ms")? {
            transaction.execute_batch(
                "ALTER TABLE provider_profile_catalog_snapshots
                 ADD COLUMN expires_at_ms INTEGER NOT NULL DEFAULT 2;",
            )?;
        }
        if !has_column(transaction, "failure_code")? {
            transaction.execute_batch(
                "ALTER TABLE provider_profile_catalog_snapshots
                 ADD COLUMN failure_code TEXT;",
            )?;
        }
        transaction.execute_batch(
            "UPDATE provider_profile_catalog_snapshots
             SET state='fresh',
                 observed_at_ms=CASE
                   WHEN updated_at_ms > 0
                    AND updated_at_ms <= (9223372036854775807 - 604800000)
                   THEN updated_at_ms ELSE 1 END,
                 expires_at_ms=CASE
                   WHEN updated_at_ms > 0
                    AND updated_at_ms <= (9223372036854775807 - 604800000)
                   THEN updated_at_ms + 604800000 ELSE 2 END,
                 failure_code=NULL
             WHERE state='fresh' AND observed_at_ms=1 AND expires_at_ms=2;",
        )?;
        transaction.execute_batch("PRAGMA user_version = 174;")?;
    }
    Ok(())
}

fn has_column(transaction: &Transaction<'_>, expected: &str) -> rusqlite::Result<bool> {
    let mut statement = transaction
        .prepare("SELECT name FROM pragma_table_info('provider_profile_catalog_snapshots')")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(0))?;
    for column in columns {
        if column? == expected {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    #[test]
    fn migration_adds_recovery_columns_to_legacy_catalog_table() {
        let connection = Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                "CREATE TABLE provider_profile_catalog_snapshots (
                   provider_id TEXT NOT NULL,
                   credential_binding TEXT NOT NULL,
                   region TEXT NOT NULL,
                   revision INTEGER NOT NULL,
                   profile_content_hash TEXT NOT NULL,
                   profile_json BLOB NOT NULL,
                   catalog_content_hash TEXT NOT NULL,
                   catalog_json BLOB NOT NULL,
                   updated_at_ms INTEGER NOT NULL,
                   PRIMARY KEY(provider_id, credential_binding, region)
                 );",
            )
            .expect("legacy schema");
        connection
            .execute(
                "INSERT INTO provider_profile_catalog_snapshots
                 (provider_id,credential_binding,region,revision,profile_content_hash,
                  profile_json,catalog_content_hash,catalog_json,updated_at_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    "openrouter",
                    "credential:openrouter",
                    "global",
                    1_i64,
                    "a".repeat(64),
                    br#"{}"#.to_vec(),
                    "b".repeat(64),
                    br#"[]"#.to_vec(),
                    1_000_i64,
                ],
            )
            .expect("legacy row");
        let transaction = connection.unchecked_transaction().expect("transaction");
        apply(&transaction, 173).expect("migration");
        transaction.commit().expect("commit");

        for column in ["state", "observed_at_ms", "expires_at_ms", "failure_code"] {
            let present = connection
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('provider_profile_catalog_snapshots')
                     WHERE name=?1",
                    [column],
                    |row| row.get::<_, u32>(0),
                )
                .expect("column");
            assert_eq!(present, 1);
        }
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("schema version"),
            174
        );
        let lifecycle = connection
            .query_row(
                "SELECT state,observed_at_ms,expires_at_ms,failure_code
                 FROM provider_profile_catalog_snapshots",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .expect("lifecycle");
        assert_eq!(lifecycle, ("fresh".to_string(), 1_000, 604_801_000, None));
    }
}
