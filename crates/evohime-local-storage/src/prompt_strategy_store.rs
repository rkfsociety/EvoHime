//! Immutable persistence for prompt-strategy metadata and selection snapshots.

use rusqlite::{params, Connection, OptionalExtension, Transaction};

/// Stored profile bytes with their content hash and lifecycle metadata.
pub type ProfileLifecycleRow = (Vec<u8>, String, String, u64);

/// Maximum registry profiles loaded by one resolver pass.
pub const MAX_PROFILES_PER_READ: usize = 1024;
/// Maximum exact bindings loaded by one resolver pass.
pub const MAX_BINDINGS_PER_READ: usize = 4096;
/// Maximum reusable example-set descriptors loaded by one registry pass.
pub const MAX_EXAMPLE_SETS_PER_READ: usize = 1024;
/// Maximum combined encoded registry bytes decoded by one Core resolver pass.
pub const MAX_REGISTRY_BYTES: i64 = 4 * 1024 * 1024;
/// Maximum route fallback snapshots attached to one provenance request.
pub const MAX_SELECTIONS_PER_REQUEST: usize = 32;

/// Rejects registry reads when the combined serialized metadata exceeds the bound.
pub fn validate_registry_size(connection: &Connection) -> rusqlite::Result<()> {
    let total: i64 = connection.query_row(
        "SELECT
            (SELECT COALESCE(SUM(length(profile_json)),0) FROM prompt_strategy_profiles) +
            (SELECT COALESCE(SUM(length(binding_json)),0) FROM prompt_strategy_bindings) +
            (SELECT COALESCE(SUM(length(descriptor_json)),0) FROM prompt_strategy_example_sets) +
            (SELECT COALESCE(SUM(length(contract_json)),0) FROM prompt_strategy_output_contracts)",
        [],
        |row| row.get(0),
    )?;
    if total > MAX_REGISTRY_BYTES {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(())
}

/// Inserts an immutable profile and its initial draft lifecycle atomically.
pub fn put_profile(
    transaction: &Transaction<'_>,
    profile_id: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let inserted = transaction.execute(
        "INSERT OR IGNORE INTO prompt_strategy_profiles(profile_id,profile_revision,content_hash,profile_json,created_at_ms)
         VALUES(?1,?2,?3,?4,?5)",
        params![profile_id, revision, hash, json, now_ms],
    )?;
    if inserted == 1 {
        transaction.execute(
            "INSERT INTO prompt_strategy_lifecycle(profile_id,profile_revision,state,state_revision,updated_at_ms)
             VALUES(?1,?2,'draft',1,?3)",
            params![profile_id, revision, now_ms],
        )?;
    }
    Ok(inserted == 1)
}

/// Installs the prompt-strategy schema for isolated store users.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS prompt_strategy_profiles (
            profile_id TEXT NOT NULL CHECK(length(CAST(profile_id AS BLOB)) <= 128), profile_revision INTEGER NOT NULL CHECK(profile_revision > 0),
            content_hash TEXT NOT NULL, profile_json BLOB NOT NULL CHECK(length(profile_json) <= 65536), created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(profile_id,profile_revision), UNIQUE(profile_id,profile_revision,content_hash));
        CREATE TABLE IF NOT EXISTS prompt_strategy_lifecycle (
            profile_id TEXT NOT NULL, profile_revision INTEGER NOT NULL,
            state TEXT NOT NULL CHECK(state IN ('draft','validated','promoted','superseded','disabled')),
            state_revision INTEGER NOT NULL CHECK(state_revision > 0), updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(profile_id,profile_revision),
            FOREIGN KEY(profile_id,profile_revision) REFERENCES prompt_strategy_profiles(profile_id,profile_revision));
        CREATE TABLE IF NOT EXISTS prompt_strategy_example_sets (
            example_set_id TEXT NOT NULL CHECK(length(CAST(example_set_id AS BLOB)) <= 128), set_revision INTEGER NOT NULL CHECK(set_revision > 0),
            content_hash TEXT NOT NULL, descriptor_json BLOB NOT NULL CHECK(length(descriptor_json) <= 65536), created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(example_set_id,set_revision), UNIQUE(example_set_id,set_revision,content_hash));
        CREATE TABLE IF NOT EXISTS prompt_strategy_output_contracts (
            contract_id TEXT NOT NULL CHECK(length(CAST(contract_id AS BLOB)) <= 128), contract_revision INTEGER NOT NULL CHECK(contract_revision > 0),
            content_hash TEXT NOT NULL, contract_json BLOB NOT NULL CHECK(length(contract_json) <= 65536), created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(contract_id,contract_revision), UNIQUE(contract_id,contract_revision,content_hash));
        CREATE TABLE IF NOT EXISTS prompt_strategy_bindings (
            binding_id TEXT NOT NULL CHECK(length(CAST(binding_id AS BLOB)) <= 128), binding_revision INTEGER NOT NULL CHECK(binding_revision > 0),
            content_hash TEXT NOT NULL, binding_json BLOB NOT NULL CHECK(length(binding_json) <= 65536), created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(binding_id,binding_revision), UNIQUE(binding_id,binding_revision,content_hash));
        CREATE TABLE IF NOT EXISTS prompt_strategy_selections (
            snapshot_id TEXT PRIMARY KEY NOT NULL CHECK(length(CAST(snapshot_id AS BLOB)) <= 128), call_id TEXT NOT NULL CHECK(length(CAST(call_id AS BLOB)) <= 128), run_id TEXT NOT NULL CHECK(length(CAST(run_id AS BLOB)) <= 128), provenance_request_id TEXT NOT NULL CHECK(length(CAST(provenance_request_id AS BLOB)) <= 128),
            content_hash TEXT NOT NULL, snapshot_json BLOB NOT NULL CHECK(length(snapshot_json) <= 65536), created_at_ms INTEGER NOT NULL,
            UNIQUE(call_id,content_hash));
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
            BEFORE DELETE ON prompt_strategy_selections BEGIN SELECT RAISE(ABORT,'immutable prompt strategy snapshot'); END;",
    )
}

/// Stores one immutable revision or returns false when the revision already exists.
pub fn put_immutable(
    connection: &Connection,
    table: ImmutableTable,
    id: &str,
    revision: u64,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let (id_column, revision_column, json_column) = table.columns();
    let sql = format!(
        "INSERT OR IGNORE INTO {}({},{},content_hash,{},created_at_ms) VALUES(?1,?2,?3,?4,?5)",
        table.name(),
        id_column,
        revision_column,
        json_column
    );
    Ok(connection.execute(&sql, params![id, revision, hash, json, now_ms])? == 1)
}

/// Creates the initial lifecycle record for an immutable profile revision.
pub fn create_lifecycle(
    connection: &Connection,
    profile_id: &str,
    profile_revision: u64,
    initial_state: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    connection.execute(
        "INSERT OR IGNORE INTO prompt_strategy_lifecycle(profile_id,profile_revision,state,state_revision,updated_at_ms)
         VALUES(?1,?2,?3,1,?4)",
        params![profile_id, profile_revision, initial_state, now_ms],
    ).map(|changed| changed == 1)
}

/// Reads lifecycle state and optimistic concurrency revision.
pub fn get_lifecycle(
    connection: &Connection,
    profile_id: &str,
    profile_revision: u64,
) -> rusqlite::Result<Option<(String, u64)>> {
    connection.query_row(
        "SELECT state,state_revision FROM prompt_strategy_lifecycle WHERE profile_id=?1 AND profile_revision=?2",
        params![profile_id, profile_revision],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()
}

/// Table family for immutable prompt-strategy data.
#[derive(Debug, Clone, Copy)]
pub enum ImmutableTable {
    /// Profile definition revisions.
    Profiles,
    /// Ordered example reference sets.
    ExampleSets,
    /// Versioned structured-output contracts.
    OutputContracts,
    /// Explicit target bindings.
    Bindings,
}

impl ImmutableTable {
    fn name(self) -> &'static str {
        match self {
            Self::Profiles => "prompt_strategy_profiles",
            Self::ExampleSets => "prompt_strategy_example_sets",
            Self::OutputContracts => "prompt_strategy_output_contracts",
            Self::Bindings => "prompt_strategy_bindings",
        }
    }

    fn columns(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Profiles => ("profile_id", "profile_revision", "profile_json"),
            Self::ExampleSets => ("example_set_id", "set_revision", "descriptor_json"),
            Self::OutputContracts => ("contract_id", "contract_revision", "contract_json"),
            Self::Bindings => ("binding_id", "binding_revision", "binding_json"),
        }
    }
}

/// Loads an immutable revision's serialized body.
pub fn get_immutable(
    connection: &Connection,
    table: ImmutableTable,
    id: &str,
    revision: u64,
) -> rusqlite::Result<Option<Vec<u8>>> {
    let (id_column, revision_column, json_column) = table.columns();
    let sql = format!(
        "SELECT {} FROM {} WHERE {}=?1 AND {}=?2",
        json_column,
        table.name(),
        id_column,
        revision_column
    );
    connection
        .query_row(&sql, params![id, revision], |row| row.get(0))
        .optional()
}

/// Lists immutable revisions and their lifecycle states in stable identity order.
pub fn list_profiles(
    connection: &Connection,
) -> rusqlite::Result<Vec<ProfileLifecycleRow>> {
    let mut statement = connection.prepare(
        "SELECT profile_json,content_hash,state,state_revision FROM prompt_strategy_profiles
         JOIN prompt_strategy_lifecycle USING(profile_id,profile_revision)
         ORDER BY profile_id,profile_revision LIMIT ?1",
    )?;
    let rows = statement.query_map([MAX_PROFILES_PER_READ as i64 + 1], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?;
    let profiles: Vec<_> = rows.collect::<rusqlite::Result<_>>()?;
    if profiles.len() > MAX_PROFILES_PER_READ {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(profiles)
}

/// Lists immutable binding revisions in stable identity order.
pub fn list_bindings(connection: &Connection) -> rusqlite::Result<Vec<(Vec<u8>, String)>> {
    let mut statement = connection.prepare(
        "SELECT binding_json,content_hash FROM prompt_strategy_bindings ORDER BY binding_id,binding_revision LIMIT ?1",
    )?;
    let rows = statement.query_map([MAX_BINDINGS_PER_READ as i64 + 1], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    let bindings: Vec<_> = rows.collect::<rusqlite::Result<_>>()?;
    if bindings.len() > MAX_BINDINGS_PER_READ {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(bindings)
}

/// Lists immutable example-set revisions in stable identity order.
pub fn list_example_sets(connection: &Connection) -> rusqlite::Result<Vec<(Vec<u8>, String)>> {
    let mut statement = connection.prepare(
        "SELECT descriptor_json,content_hash FROM prompt_strategy_example_sets ORDER BY example_set_id,set_revision LIMIT ?1",
    )?;
    let rows = statement.query_map([MAX_EXAMPLE_SETS_PER_READ as i64 + 1], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    let sets: Vec<_> = rows.collect::<rusqlite::Result<_>>()?;
    if sets.len() > MAX_EXAMPLE_SETS_PER_READ {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(sets)
}

/// Changes lifecycle state only when both caller state and revision match.
pub fn transition_lifecycle(
    transaction: &Transaction<'_>,
    profile_id: &str,
    profile_revision: u64,
    expected_state: &str,
    expected_state_revision: u64,
    next_state: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    transaction.execute(
        "UPDATE prompt_strategy_lifecycle SET state=?1,state_revision=state_revision+1,updated_at_ms=?2
         WHERE profile_id=?3 AND profile_revision=?4 AND state=?5 AND state_revision=?6
         AND ((state='draft' AND ?1 IN ('validated','disabled'))
           OR (state='validated' AND ?1 IN ('promoted','disabled'))
           OR (state='promoted' AND ?1 IN ('superseded','disabled'))) ",
        params![next_state, now_ms, profile_id, profile_revision, expected_state, expected_state_revision],
    ).map(|changed| changed == 1)
}

/// Inserts an exact per-call selection snapshot idempotently.
#[allow(clippy::too_many_arguments)]
pub fn put_selection(
    connection: &Connection,
    snapshot_id: &str,
    call_id: &str,
    run_id: &str,
    provenance_request_id: &str,
    hash: &str,
    json: &[u8],
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO prompt_strategy_selections(snapshot_id,call_id,run_id,provenance_request_id,content_hash,snapshot_json,created_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![snapshot_id, call_id, run_id, provenance_request_id, hash, json, now_ms],
    )? == 1)
}

/// Loads a frozen selection snapshot by its stable snapshot identity.
pub fn get_selection(
    connection: &Connection,
    snapshot_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT snapshot_json FROM prompt_strategy_selections WHERE snapshot_id=?1",
            [snapshot_id],
            |row| row.get(0),
        )
        .optional()
}

/// Loads the first immutable selection snapshot recorded for one run.
pub fn first_selection_for_run(
    connection: &Connection,
    run_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT snapshot_json FROM prompt_strategy_selections
             WHERE run_id=?1 ORDER BY rowid LIMIT 1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
}

/// Loads bounded selection snapshots linked to one provenance request.
pub fn list_selections_for_provenance(
    connection: &Connection,
    request_id: &str,
) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection.prepare(
        "SELECT snapshot_json FROM prompt_strategy_selections
         WHERE provenance_request_id=?1 ORDER BY created_at_ms,snapshot_id LIMIT ?2",
    )?;
    let rows = statement.query_map(
        params![request_id, MAX_SELECTIONS_PER_REQUEST as i64 + 1],
        |row| row.get(0),
    )?;
    let snapshots: Vec<_> = rows.collect::<rusqlite::Result<_>>()?;
    if snapshots.len() > MAX_SELECTIONS_PER_REQUEST {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn immutable_revisions_and_lifecycle_compare_and_swap_are_enforced() {
        let mut connection = Connection::open_in_memory().expect("database");
        install_schema(&connection).expect("schema");
        let transaction = connection.transaction().expect("transaction");
        assert!(put_profile(&transaction, "p", 1, "hash", b"{}", 1).expect("profile"));
        transaction.commit().expect("commit");

        let transaction = connection.transaction().expect("transaction");
        assert!(
            transition_lifecycle(&transaction, "p", 1, "draft", 1, "validated", 2)
                .expect("transition")
        );
        assert!(
            !transition_lifecycle(&transaction, "p", 1, "draft", 1, "promoted", 3)
                .expect("stale transition")
        );
        transaction.commit().expect("commit");

        let transaction = connection.unchecked_transaction().expect("transaction");
        assert!(
            !put_profile(&transaction, "p", 1, "other", b"other", 4).expect("duplicate revision")
        );
        transaction.commit().expect("commit");
        assert!(connection.execute(
            "UPDATE prompt_strategy_profiles SET content_hash='changed' WHERE profile_id='p' AND profile_revision=1",
            [],
        ).is_err());
    }

    #[test]
    fn structured_output_contract_revisions_are_immutable() {
        let connection = Connection::open_in_memory().expect("database");
        install_schema(&connection).expect("schema");
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
