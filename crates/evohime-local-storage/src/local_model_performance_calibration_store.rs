//! Durable metadata boundary for local model performance calibration.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_JSON_BYTES: usize = 64 * 1024;

pub type SessionRow = (i64, Vec<u8>, String, Vec<u8>, bool);

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS local_model_calibration_sessions (
           session_id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL,
           identity_json BLOB NOT NULL, state TEXT NOT NULL, samples_json BLOB NOT NULL,
           cancellation_requested INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS local_model_performance_profiles (
           profile_id TEXT NOT NULL, revision INTEGER NOT NULL,
           identity_json BLOB NOT NULL, aggregate_json BLOB NOT NULL,
           evidence_class TEXT NOT NULL, context_points_json BLOB NOT NULL,
           confidence TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
           PRIMARY KEY(profile_id, revision)
         );
         CREATE INDEX IF NOT EXISTS idx_local_model_calibration_identity
           ON local_model_performance_profiles(profile_id, revision);",
    )
}

#[allow(clippy::too_many_arguments)]
pub fn put_session(
    connection: &Connection,
    session_id: &str,
    revision: i64,
    identity_json: &[u8],
    state: &str,
    samples_json: &[u8],
    cancellation_requested: bool,
    updated_at_ms: i64,
) -> Result<bool, &'static str> {
    if session_id.trim().is_empty()
        || revision <= 0
        || state.trim().is_empty()
        || identity_json.is_empty()
        || samples_json.is_empty()
        || identity_json.len() > MAX_JSON_BYTES
        || samples_json.len() > MAX_JSON_BYTES
        || updated_at_ms <= 0
    {
        return Err("invalid calibration session");
    }
    let changed = connection.execute(
        "INSERT INTO local_model_calibration_sessions
         (session_id,revision,identity_json,state,samples_json,cancellation_requested,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(session_id) DO UPDATE SET revision=excluded.revision,
           identity_json=excluded.identity_json,state=excluded.state,
           samples_json=excluded.samples_json,cancellation_requested=excluded.cancellation_requested,
           updated_at_ms=excluded.updated_at_ms
         WHERE excluded.revision = local_model_calibration_sessions.revision + 1",
        params![session_id, revision, identity_json, state, samples_json, cancellation_requested, updated_at_ms],
    ).map_err(|_| "sqlite")?;
    Ok(changed == 1)
}

pub fn get_session(
    connection: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<SessionRow>> {
    connection
        .query_row(
            "SELECT revision,identity_json,state,samples_json,cancellation_requested
         FROM local_model_calibration_sessions WHERE session_id=?1",
            params![session_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
}

#[allow(clippy::too_many_arguments)]
pub fn put_profile(
    connection: &Connection,
    profile_id: &str,
    revision: i64,
    identity_json: &[u8],
    aggregate_json: &[u8],
    evidence_class: &str,
    context_points_json: &[u8],
    confidence: &str,
    created_at_ms: i64,
) -> Result<bool, &'static str> {
    if profile_id.trim().is_empty()
        || revision <= 0
        || evidence_class.trim().is_empty()
        || confidence.trim().is_empty()
        || created_at_ms <= 0
        || identity_json.is_empty()
        || aggregate_json.is_empty()
        || context_points_json.is_empty()
        || identity_json.len() > MAX_JSON_BYTES
        || aggregate_json.len() > MAX_JSON_BYTES
        || context_points_json.len() > MAX_JSON_BYTES
    {
        return Err("invalid calibration profile");
    }
    connection.execute(
        "INSERT OR IGNORE INTO local_model_performance_profiles
         (profile_id,revision,identity_json,aggregate_json,evidence_class,context_points_json,confidence,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![profile_id, revision, identity_json, aggregate_json, evidence_class, context_points_json, confidence, created_at_ms],
    ).map(|count| count == 1).map_err(|_| "sqlite")
}
