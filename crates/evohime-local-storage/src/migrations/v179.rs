use rusqlite::Transaction;

/// Installs immutable prompt-strategy metadata and replay snapshot tables.
pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current < 179 {
        ensure_column(
            transaction,
            "benchmark_baselines",
            "suite_hash",
            "ALTER TABLE benchmark_baselines ADD COLUMN suite_hash TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            transaction,
            "benchmark_baselines",
            "policy_hash",
            "ALTER TABLE benchmark_baselines ADD COLUMN policy_hash TEXT NOT NULL DEFAULT ''",
        )?;
        ensure_column(
            transaction,
            "task_artifacts",
            "content_kind",
            "ALTER TABLE task_artifacts ADD COLUMN content_kind TEXT",
        )?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS prompt_strategy_profiles (
                profile_id TEXT NOT NULL CHECK(length(CAST(profile_id AS BLOB)) <= 128),
                profile_revision INTEGER NOT NULL CHECK(profile_revision > 0),
                content_hash TEXT NOT NULL,
                profile_json BLOB NOT NULL CHECK(length(profile_json) <= 65536),
                created_at_ms INTEGER NOT NULL,
                PRIMARY KEY(profile_id,profile_revision),
                UNIQUE(profile_id,profile_revision,content_hash)
            );
            CREATE TABLE IF NOT EXISTS prompt_strategy_lifecycle (
                profile_id TEXT NOT NULL,
                profile_revision INTEGER NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('draft','validated','promoted','superseded','disabled')),
                state_revision INTEGER NOT NULL CHECK(state_revision > 0),
                updated_at_ms INTEGER NOT NULL,
                PRIMARY KEY(profile_id,profile_revision),
                FOREIGN KEY(profile_id,profile_revision)
                    REFERENCES prompt_strategy_profiles(profile_id,profile_revision)
            );
            CREATE TABLE IF NOT EXISTS prompt_strategy_example_sets (
                example_set_id TEXT NOT NULL CHECK(length(CAST(example_set_id AS BLOB)) <= 128),
                set_revision INTEGER NOT NULL CHECK(set_revision > 0),
                content_hash TEXT NOT NULL,
                descriptor_json BLOB NOT NULL CHECK(length(descriptor_json) <= 65536),
                created_at_ms INTEGER NOT NULL,
                PRIMARY KEY(example_set_id,set_revision),
                UNIQUE(example_set_id,set_revision,content_hash)
            );
            CREATE TABLE IF NOT EXISTS prompt_strategy_output_contracts (
                contract_id TEXT NOT NULL CHECK(length(CAST(contract_id AS BLOB)) <= 128),
                contract_revision INTEGER NOT NULL CHECK(contract_revision > 0),
                content_hash TEXT NOT NULL,
                contract_json BLOB NOT NULL CHECK(length(contract_json) <= 65536),
                created_at_ms INTEGER NOT NULL,
                PRIMARY KEY(contract_id,contract_revision),
                UNIQUE(contract_id,contract_revision,content_hash)
            );
            CREATE TABLE IF NOT EXISTS prompt_strategy_bindings (
                binding_id TEXT NOT NULL CHECK(length(CAST(binding_id AS BLOB)) <= 128),
                binding_revision INTEGER NOT NULL CHECK(binding_revision > 0),
                content_hash TEXT NOT NULL,
                binding_json BLOB NOT NULL CHECK(length(binding_json) <= 65536),
                created_at_ms INTEGER NOT NULL,
                PRIMARY KEY(binding_id,binding_revision),
                UNIQUE(binding_id,binding_revision,content_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_prompt_strategy_bindings_target
                ON prompt_strategy_bindings(binding_id,binding_revision);
            CREATE TABLE IF NOT EXISTS prompt_strategy_selections (
                snapshot_id TEXT PRIMARY KEY NOT NULL CHECK(length(CAST(snapshot_id AS BLOB)) <= 128),
                call_id TEXT NOT NULL CHECK(length(CAST(call_id AS BLOB)) <= 128),
                run_id TEXT NOT NULL CHECK(length(CAST(run_id AS BLOB)) <= 128),
                provenance_request_id TEXT NOT NULL CHECK(length(CAST(provenance_request_id AS BLOB)) <= 128),
                content_hash TEXT NOT NULL,
                snapshot_json BLOB NOT NULL CHECK(length(snapshot_json) <= 65536),
                created_at_ms INTEGER NOT NULL,
                UNIQUE(call_id,content_hash)
            );
            CREATE INDEX IF NOT EXISTS idx_prompt_strategy_selections_run
                ON prompt_strategy_selections(run_id,created_at_ms);
            CREATE INDEX IF NOT EXISTS idx_prompt_strategy_selections_provenance
                ON prompt_strategy_selections(provenance_request_id,created_at_ms);
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_profiles_immutable_update
                BEFORE UPDATE ON prompt_strategy_profiles BEGIN SELECT RAISE(ABORT,'immutable prompt strategy'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_profiles_immutable_delete
                BEFORE DELETE ON prompt_strategy_profiles BEGIN SELECT RAISE(ABORT,'immutable prompt strategy'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_example_sets_immutable_update
                BEFORE UPDATE ON prompt_strategy_example_sets BEGIN SELECT RAISE(ABORT,'immutable prompt strategy'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_example_sets_immutable_delete
                BEFORE DELETE ON prompt_strategy_example_sets BEGIN SELECT RAISE(ABORT,'immutable prompt strategy'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_output_contracts_immutable_update
                BEFORE UPDATE ON prompt_strategy_output_contracts BEGIN SELECT RAISE(ABORT,'immutable prompt strategy output contract'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_output_contracts_immutable_delete
                BEFORE DELETE ON prompt_strategy_output_contracts BEGIN SELECT RAISE(ABORT,'immutable prompt strategy output contract'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_bindings_immutable_update
                BEFORE UPDATE ON prompt_strategy_bindings BEGIN SELECT RAISE(ABORT,'immutable prompt strategy'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_bindings_immutable_delete
                BEFORE DELETE ON prompt_strategy_bindings BEGIN SELECT RAISE(ABORT,'immutable prompt strategy'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_selections_immutable_update
                BEFORE UPDATE ON prompt_strategy_selections BEGIN SELECT RAISE(ABORT,'immutable prompt strategy snapshot'); END;
            CREATE TRIGGER IF NOT EXISTS prompt_strategy_selections_immutable_delete
                BEFORE DELETE ON prompt_strategy_selections BEGIN SELECT RAISE(ABORT,'immutable prompt strategy snapshot'); END;
            PRAGMA user_version = 179;",
        )?;
    }
    Ok(())
}

fn ensure_column(
    transaction: &Transaction<'_>,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> rusqlite::Result<()> {
    let exists = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
        rusqlite::params![table, column],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        transaction.execute_batch(alter_sql)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_records_artifact_kind_without_trusting_legacy_rows() {
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE benchmark_baselines (
                    baseline_id TEXT PRIMARY KEY NOT NULL, suite_version TEXT NOT NULL,
                    challenge_id TEXT NOT NULL, model_profile_hash TEXT NOT NULL,
                    agent_profile_hash TEXT NOT NULL, metrics_json TEXT NOT NULL,
                    source_commit TEXT NOT NULL, revision INTEGER NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    UNIQUE(suite_version,challenge_id,model_profile_hash,agent_profile_hash,revision)
                 );
                 CREATE TABLE task_artifacts (
                    content_hash TEXT PRIMARY KEY, bytes INTEGER NOT NULL, content BLOB NOT NULL,
                    created_at INTEGER NOT NULL, last_access_at INTEGER NOT NULL
                 );
                 INSERT INTO task_artifacts VALUES ('legacy-hash',12,x'6578616d706c652074657874',1,1);",
            )
            .expect("legacy schema");
        let transaction = connection.transaction().expect("migration transaction");
        apply(&transaction, 178).expect("migration");
        transaction.commit().expect("commit");

        let kind: Option<String> = connection
            .query_row(
                "SELECT content_kind FROM task_artifacts WHERE content_hash='legacy-hash'",
                [],
                |row| row.get(0),
            )
            .expect("legacy kind");
        assert_eq!(kind, None);
        let column_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('task_artifacts') WHERE name='content_kind')",
                [],
                |row| row.get(0),
            )
            .expect("kind column");
        assert!(column_exists);
    }

    #[test]
    fn migration_protects_output_contract_revisions_from_mutation() {
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE benchmark_baselines (
                baseline_id TEXT PRIMARY KEY NOT NULL, suite_version TEXT NOT NULL,
                challenge_id TEXT NOT NULL, model_profile_hash TEXT NOT NULL,
                agent_profile_hash TEXT NOT NULL, metrics_json TEXT NOT NULL,
                source_commit TEXT NOT NULL, revision INTEGER NOT NULL,
                created_at_ms INTEGER NOT NULL,
                UNIQUE(suite_version,challenge_id,model_profile_hash,agent_profile_hash,revision)
             );
             CREATE TABLE task_artifacts (
                content_hash TEXT PRIMARY KEY, bytes INTEGER NOT NULL, content BLOB NOT NULL,
                created_at INTEGER NOT NULL, last_access_at INTEGER NOT NULL
             );",
            )
            .expect("legacy schema");
        let transaction = connection.transaction().expect("migration transaction");
        apply(&transaction, 178).expect("migration");
        transaction.commit().expect("commit");
        connection.execute(
            "INSERT INTO prompt_strategy_output_contracts(contract_id,contract_revision,content_hash,contract_json,created_at_ms) VALUES('result/v1',1,'hash',x'7b7d',1)",
            [],
        ).expect("insert contract");
        assert!(connection.execute(
            "UPDATE prompt_strategy_output_contracts SET content_hash='changed' WHERE contract_id='result/v1' AND contract_revision=1",
            [],
        ).is_err());
        assert!(connection.execute(
            "DELETE FROM prompt_strategy_output_contracts WHERE contract_id='result/v1' AND contract_revision=1",
            [],
        ).is_err());
    }
}
