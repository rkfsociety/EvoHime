//! Durable metadata for execution environment profiles.
//!
//! The Core validates every record.  This store intentionally persists only
//! bounded, redacted JSON snapshots and never becomes an owner of model,
//! credential, tool, skill, or policy state.

use rusqlite::{params, Connection, OptionalExtension};

/// Maximum serialized profile, activation, snapshot, or idempotency response size.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

/// Creates tables and indexes for profile revisions, activation state, snapshots, and idempotency.
pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS execution_environment_profiles (
            id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL,
            scope TEXT NOT NULL, state TEXT NOT NULL, content_hash TEXT NOT NULL,
            profile_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_profile_revisions (
            profile_id TEXT NOT NULL, revision INTEGER NOT NULL,
            content_hash TEXT NOT NULL, profile_json BLOB NOT NULL,
            actor TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(profile_id, revision)
        );
        CREATE TABLE IF NOT EXISTS execution_environment_activations (
            id INTEGER PRIMARY KEY AUTOINCREMENT, profile_id TEXT NOT NULL,
            revision INTEGER NOT NULL, scope TEXT NOT NULL, status TEXT NOT NULL,
            snapshot_hash TEXT NOT NULL, activation_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_current (
            scope TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL,
            revision INTEGER NOT NULL, snapshot_json BLOB NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_run_snapshots (
            run_id TEXT PRIMARY KEY NOT NULL, scope TEXT NOT NULL,
            profile_id TEXT NOT NULL, revision INTEGER NOT NULL,
            snapshot_hash TEXT NOT NULL, snapshot_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_idempotency (
            owner_scope TEXT NOT NULL, idempotency_key TEXT NOT NULL,
            command_hash TEXT NOT NULL, response_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_scope, idempotency_key)
        );
        CREATE INDEX IF NOT EXISTS idx_execution_environment_activations_scope
            ON execution_environment_activations(scope, id DESC);",
    )
}

/// Loads the stored command digest and response for an owner-scoped idempotency key.
pub fn load_idempotent_command(
    connection: &Connection,
    owner_scope: &str,
    idempotency_key: &str,
) -> rusqlite::Result<Option<(String, Vec<u8>)>> {
    connection.query_row(
        "SELECT command_hash,response_json FROM execution_environment_idempotency WHERE owner_scope=?1 AND idempotency_key=?2",
        params![owner_scope, idempotency_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()
}

/// Stores a bounded idempotency response without replacing an existing key.
///
/// Returns a SQLite error when the response exceeds [`MAX_RECORD_BYTES`]. Repeated keys are left
/// unchanged so retries observe the original response.
pub fn save_idempotent_command(
    connection: &Connection,
    owner_scope: &str,
    idempotency_key: &str,
    command_hash: &str,
    response: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    if response.len() > MAX_RECORD_BYTES {
        return Err(rusqlite::Error::InvalidQuery);
    }
    connection.execute(
        "INSERT INTO execution_environment_idempotency(owner_scope,idempotency_key,command_hash,response_json,created_at_ms) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(owner_scope,idempotency_key) DO NOTHING",
        params![owner_scope, idempotency_key, command_hash, response, now_ms],
    )?;
    Ok(())
}

/// Values needed to append a profile revision and update its current projection.
pub struct SaveProfileRevisionInput<'a> {
    /// Stable profile identifier.
    pub id: &'a str,
    /// Monotonically increasing revision number.
    pub revision: u64,
    /// Profile scope used to select current state.
    pub scope: &'a str,
    /// Lifecycle state stored with the profile.
    pub state: &'a str,
    /// Digest of the serialized profile.
    pub hash: &'a str,
    /// Serialized profile snapshot, bounded by [`MAX_RECORD_BYTES`].
    pub json: &'a [u8],
    /// Actor credited with creating the revision.
    pub actor: &'a str,
    /// Creation time in Unix milliseconds.
    pub now_ms: i64,
}

/// Appends a profile revision and advances the current profile only to a newer revision.
///
/// Returns `false` when the serialized profile is too large, a stored revision is newer, or the
/// revision already exists.
pub fn save_profile_revision(
    connection: &Connection,
    input: SaveProfileRevisionInput<'_>,
) -> rusqlite::Result<bool> {
    if input.json.len() > MAX_RECORD_BYTES {
        return Ok(false);
    }
    let tx = connection.unchecked_transaction()?;
    let current: Option<u64> = tx
        .query_row(
            "SELECT revision FROM execution_environment_profiles WHERE id=?1",
            [input.id],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some_and(|value| value >= input.revision) {
        return Ok(false);
    }
    let inserted = tx.execute("INSERT INTO execution_environment_profile_revisions(profile_id,revision,content_hash,profile_json,actor,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(profile_id,revision) DO NOTHING", params![input.id, input.revision as i64, input.hash, input.json, input.actor, input.now_ms])?;
    if inserted == 0 {
        return Ok(false);
    }
    tx.execute("INSERT INTO execution_environment_profiles(id,revision,scope,state,content_hash,profile_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,scope=excluded.scope,state=excluded.state,content_hash=excluded.content_hash,profile_json=excluded.profile_json,updated_at_ms=excluded.updated_at_ms", params![input.id, input.revision as i64, input.scope, input.state, input.hash, input.json, input.now_ms])?;
    tx.commit()?;
    Ok(true)
}

/// Loads the current serialized profile for an identifier.
pub fn load_profile(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT profile_json FROM execution_environment_profiles WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
}

/// Lists serialized profiles in identifier order, capped at 256 records.
pub fn load_profiles(connection: &Connection, limit: usize) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection
        .prepare("SELECT profile_json FROM execution_environment_profiles ORDER BY id LIMIT ?1")?;
    let records = statement
        .query_map([limit.min(256) as i64], |row| row.get(0))?
        .collect();
    records
}

/// Values needed to record an activation and update the scope's current snapshot.
pub struct SaveActivationInput<'a> {
    /// Activated profile identifier.
    pub profile_id: &'a str,
    /// Profile revision used for activation.
    pub revision: u64,
    /// Scope whose current activation is updated.
    pub scope: &'a str,
    /// Activation lifecycle state.
    pub status: &'a str,
    /// Digest of the activation snapshot.
    pub snapshot_hash: &'a str,
    /// Serialized activation record, bounded by [`MAX_RECORD_BYTES`].
    pub activation_json: &'a [u8],
    /// Serialized environment snapshot, bounded by [`MAX_RECORD_BYTES`].
    pub snapshot_json: &'a [u8],
    /// Activation time in Unix milliseconds.
    pub now_ms: i64,
}

/// Records a bounded activation and advances the scope's current snapshot when its revision is newer.
///
/// Returns `false` if either serialized value exceeds [`MAX_RECORD_BYTES`].
pub fn save_activation(
    connection: &Connection,
    input: SaveActivationInput<'_>,
) -> rusqlite::Result<bool> {
    if input.activation_json.len() > MAX_RECORD_BYTES
        || input.snapshot_json.len() > MAX_RECORD_BYTES
    {
        return Ok(false);
    }
    let tx = connection.unchecked_transaction()?;
    tx.execute("INSERT INTO execution_environment_activations(profile_id,revision,scope,status,snapshot_hash,activation_json,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![input.profile_id, input.revision as i64, input.scope, input.status, input.snapshot_hash, input.activation_json, input.now_ms])?;
    tx.execute("INSERT INTO execution_environment_current(scope,profile_id,revision,snapshot_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(scope) DO UPDATE SET profile_id=excluded.profile_id,revision=excluded.revision,snapshot_json=excluded.snapshot_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > execution_environment_current.revision", params![input.scope, input.profile_id, input.revision as i64, input.snapshot_json, input.now_ms])?;
    tx.commit()?;
    Ok(true)
}

/// Loads the current serialized environment snapshot for a scope.
pub fn load_current(connection: &Connection, scope: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT snapshot_json FROM execution_environment_current WHERE scope=?1",
            [scope],
            |row| row.get(0),
        )
        .optional()
}

/// Lists activation records for a scope newest first, capped at 256 records.
pub fn load_activations(
    connection: &Connection,
    scope: &str,
    limit: usize,
) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection.prepare("SELECT activation_json FROM execution_environment_activations WHERE scope=?1 ORDER BY id DESC LIMIT ?2")?;
    let records = statement
        .query_map(params![scope, limit.min(256) as i64], |row| row.get(0))?
        .collect();
    records
}

/// Pins the current environment to a newly-created run. `INSERT OR IGNORE`
/// deliberately preserves the original snapshot on retries and recovery.
pub fn bind_current_to_run(
    connection: &Connection,
    run_id: &str,
    scope: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let Some(snapshot_json) = load_current(connection, scope)? else {
        return Ok(false);
    };
    let value: serde_json::Value = serde_json::from_slice(&snapshot_json)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let profile_id = value
        .get("profile_id")
        .and_then(serde_json::Value::as_str)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let revision = value
        .get("profile_revision")
        .and_then(serde_json::Value::as_u64)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let snapshot_hash = value
        .get("snapshot_hash")
        .and_then(serde_json::Value::as_str)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    Ok(connection.execute("INSERT OR IGNORE INTO execution_environment_run_snapshots(run_id,scope,profile_id,revision,snapshot_hash,snapshot_json,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![run_id, scope, profile_id, revision as i64, snapshot_hash, snapshot_json, now_ms])? == 1)
}

/// Loads the immutable environment snapshot pinned to a run, if present.
pub fn load_run_snapshot(
    connection: &Connection,
    run_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT snapshot_json FROM execution_environment_run_snapshots WHERE run_id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
}

#[cfg(test)]
#[path = "execution_environment_profiles_store_tests.rs"]
mod tests;
