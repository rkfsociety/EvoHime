use rusqlite::{Connection, Transaction};

fn has_column(connection: &Connection, column: &str) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare("PRAGMA table_info(memory_extraction_lifecycle)")?;
    let names = statement.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 176 {
        for (name, sql) in [
            (
                "source_id",
                "ALTER TABLE memory_extraction_lifecycle ADD COLUMN source_id TEXT",
            ),
            (
                "candidate_slot",
                "ALTER TABLE memory_extraction_lifecycle ADD COLUMN candidate_slot TEXT",
            ),
            (
                "source_order",
                "ALTER TABLE memory_extraction_lifecycle ADD COLUMN source_order INTEGER NOT NULL DEFAULT 0 CHECK(source_order >= 0)",
            ),
            (
                "expected_head_id",
                "ALTER TABLE memory_extraction_lifecycle ADD COLUMN expected_head_id TEXT",
            ),
            (
                "expected_head_revision",
                "ALTER TABLE memory_extraction_lifecycle ADD COLUMN expected_head_revision INTEGER NOT NULL DEFAULT 0 CHECK(expected_head_revision >= 0)",
            ),
        ] {
            if !has_column(transaction, name)? {
                transaction.execute_batch(sql)?;
            }
        }
        super::super::memory_extraction_store::install_schema_v176(transaction)?;
        transaction.execute_batch("PRAGMA user_version = 176;")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_176_preserves_v175_rows_and_adds_source_recovery_tables() {
        let connection = Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                "CREATE TABLE memory_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    content TEXT NOT NULL
                 );
                 INSERT INTO memory_entries (id,content) VALUES ('memory-1','kept');
                 CREATE TABLE ambient_episodes (
                    episode_id TEXT PRIMARY KEY NOT NULL,
                    extraction_state TEXT NOT NULL
                 );
                 INSERT INTO ambient_episodes (episode_id,extraction_state)
                 VALUES ('ambient-1','done');",
            )
            .expect("pre-existing memory and ambient rows");
        let transaction = connection.unchecked_transaction().expect("transaction");
        super::super::v175::apply(&transaction, 174).expect("v175 migration");
        transaction
            .execute(
                "INSERT INTO memory_extraction_lifecycle
                   (idempotency_key,source_basis,state,created_at_ms,updated_at_ms)
                 VALUES ('sha256:legacy-key','sha256:legacy-basis','committed',10,10)",
                [],
            )
            .expect("legacy lifecycle row");
        transaction.commit().expect("commit v175");

        let transaction = connection.unchecked_transaction().expect("transaction");
        apply(&transaction, 175).expect("v176 migration");
        transaction.commit().expect("commit v176");

        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .expect("schema version"),
            176
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT state,source_id,source_order FROM memory_extraction_lifecycle
                     WHERE idempotency_key='sha256:legacy-key'",
                    [],
                    |row| Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?
                    )),
                )
                .expect("legacy row survives"),
            ("committed".to_owned(), None, 0)
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type='table' AND name IN ('memory_extraction_sources','memory_extraction_heads')",
                    [],
                    |row| row.get::<_, u32>(0),
                )
                .expect("source tables installed"),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT content FROM memory_entries WHERE id='memory-1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("existing memory row survives"),
            "kept"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT extraction_state FROM ambient_episodes WHERE episode_id='ambient-1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("existing ambient row survives"),
            "done"
        );
    }
}
