//! Metadata-only persistence for the Core-owned A2A remote-agent bridge.
//!
//! This store is an additive extension of the existing external-agent owner
//! (`external_coding_agent_adapter_store`). It persists bounded profile
//! revisions, untrusted card snapshot hashes and remote task bindings. Full
//! cards, response bodies, headers, cookies, transcripts, objectives and tokens
//! are never stored; only ids, references, digests and typed state are retained.

use crate::StorageError;
use rusqlite::{params, Connection};

const MAX_PROVENANCE_JSON_BYTES: usize = 16 * 1024;

/// Creates the A2A profile, card snapshot and task binding tables and indexes.
pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS a2a_remote_profiles (
           profile_id TEXT NOT NULL,
           revision INTEGER NOT NULL,
           display_name TEXT NOT NULL,
           protocol_kind TEXT NOT NULL,
           endpoint_origin TEXT NOT NULL,
           allow_loopback INTEGER NOT NULL,
           allowed_ports_json TEXT NOT NULL,
           auth_profile_ref TEXT,
           skill_allowlist_json TEXT NOT NULL,
           capability_allowlist_json TEXT NOT NULL,
           privacy_ceiling TEXT NOT NULL,
           concurrency_limit INTEGER NOT NULL,
           enabled INTEGER NOT NULL,
           content_hash TEXT NOT NULL,
           updated_at_ms INTEGER NOT NULL,
           PRIMARY KEY(profile_id, revision)
         );
         CREATE TABLE IF NOT EXISTS a2a_card_snapshots (
           profile_id TEXT NOT NULL,
           profile_revision INTEGER NOT NULL,
           normalized_hash TEXT NOT NULL,
           raw_hash TEXT NOT NULL,
           origin TEXT NOT NULL,
           protocol_version INTEGER NOT NULL,
           fetched_at_ms INTEGER NOT NULL,
           expires_at_ms INTEGER NOT NULL,
           supports_streaming INTEGER NOT NULL,
           skills_json TEXT NOT NULL,
           schema_hashes_json TEXT NOT NULL,
           auth_schemes_json TEXT NOT NULL,
           created_at_ms INTEGER NOT NULL,
           PRIMARY KEY(profile_id, profile_revision, normalized_hash)
         );
         CREATE INDEX IF NOT EXISTS idx_a2a_card_snapshots_expiry
           ON a2a_card_snapshots(expires_at_ms);
         CREATE TABLE IF NOT EXISTS a2a_task_bindings (
           binding_id TEXT PRIMARY KEY NOT NULL,
           delegation_id TEXT NOT NULL,
           remote_task_id TEXT,
           state TEXT NOT NULL,
           reconciliation TEXT NOT NULL,
           attempt INTEGER NOT NULL,
           idempotency_key TEXT NOT NULL,
           request_hash TEXT NOT NULL,
           capsule_hash TEXT NOT NULL,
           policy_hash TEXT NOT NULL,
           revision INTEGER NOT NULL,
           created_at_ms INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL,
           content_hash TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_a2a_task_bindings_delegation
           ON a2a_task_bindings(delegation_id);",
    )?;
    Ok(())
}

/// Fields written for one persisted remote-agent profile revision.
#[derive(Clone, Copy)]
pub struct A2aProfileInput<'a> {
    /// Stable profile identifier.
    pub profile_id: &'a str,
    /// Monotonically increasing profile revision.
    pub revision: u64,
    /// Human-readable profile label.
    pub display_name: &'a str,
    /// Serialized protocol kind.
    pub protocol_kind: &'a str,
    /// Canonical endpoint origin.
    pub endpoint_origin: &'a str,
    /// Whether loopback/private origins are explicitly permitted.
    pub allow_loopback: bool,
    /// Serialized permitted port list.
    pub allowed_ports_json: &'a str,
    /// Reference to an existing auth owner profile, never a secret.
    pub auth_profile_ref: Option<&'a str>,
    /// Serialized skill allowlist.
    pub skill_allowlist_json: &'a str,
    /// Serialized capability allowlist.
    pub capability_allowlist_json: &'a str,
    /// Effective privacy ceiling label.
    pub privacy_ceiling: &'a str,
    /// Maximum concurrent remote tasks.
    pub concurrency_limit: u32,
    /// Whether new delegations may use this revision.
    pub enabled: bool,
    /// Digest of the canonical profile content.
    pub content_hash: &'a str,
    /// Update timestamp in Unix milliseconds.
    pub now_ms: i64,
}

/// Inserts a profile revision or replaces it only when strictly newer.
///
/// Returns `true` when a row was inserted or updated and `false` when an equal
/// or newer revision already exists.
pub fn upsert_profile(
    connection: &Connection,
    input: A2aProfileInput<'_>,
) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "INSERT INTO a2a_remote_profiles(profile_id,revision,display_name,protocol_kind,endpoint_origin,allow_loopback,allowed_ports_json,auth_profile_ref,skill_allowlist_json,capability_allowlist_json,privacy_ceiling,concurrency_limit,enabled,content_hash,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
         ON CONFLICT(profile_id,revision) DO UPDATE SET
           display_name=excluded.display_name, protocol_kind=excluded.protocol_kind,
           endpoint_origin=excluded.endpoint_origin, allow_loopback=excluded.allow_loopback,
           allowed_ports_json=excluded.allowed_ports_json, auth_profile_ref=excluded.auth_profile_ref,
           skill_allowlist_json=excluded.skill_allowlist_json,
           capability_allowlist_json=excluded.capability_allowlist_json,
           privacy_ceiling=excluded.privacy_ceiling, concurrency_limit=excluded.concurrency_limit,
           enabled=excluded.enabled, content_hash=excluded.content_hash,
           updated_at_ms=excluded.updated_at_ms
         WHERE excluded.updated_at_ms >= a2a_remote_profiles.updated_at_ms",
        params![
            input.profile_id,
            input.revision as i64,
            input.display_name,
            input.protocol_kind,
            input.endpoint_origin,
            input.allow_loopback as i64,
            input.allowed_ports_json,
            input.auth_profile_ref,
            input.skill_allowlist_json,
            input.capability_allowlist_json,
            input.privacy_ceiling,
            input.concurrency_limit as i64,
            input.enabled as i64,
            input.content_hash,
            input.now_ms
        ],
    )? == 1)
}

/// Disables every revision of a profile without deleting task history.
pub fn disable_profile(
    connection: &Connection,
    profile_id: &str,
    now_ms: i64,
) -> Result<usize, StorageError> {
    Ok(connection.execute(
        "UPDATE a2a_remote_profiles SET enabled=0, updated_at_ms=?2 WHERE profile_id=?1",
        params![profile_id, now_ms],
    )?)
}

/// Fields written for one untrusted card snapshot.
#[derive(Clone, Copy)]
pub struct A2aCardSnapshotInput<'a> {
    /// Owning profile identifier.
    pub profile_id: &'a str,
    /// Profile revision the card was fetched for.
    pub profile_revision: u64,
    /// Digest of the normalized, bounded snapshot content.
    pub normalized_hash: &'a str,
    /// Digest of the raw response bytes.
    pub raw_hash: &'a str,
    /// Origin the card was fetched from.
    pub origin: &'a str,
    /// Protocol version advertised by the card.
    pub protocol_version: u32,
    /// Time the card was fetched in Unix milliseconds.
    pub fetched_at_ms: i64,
    /// Time the snapshot becomes stale in Unix milliseconds.
    pub expires_at_ms: i64,
    /// Whether the card claims streaming support.
    pub supports_streaming: bool,
    /// Serialized bounded skill claims.
    pub skills_json: &'a str,
    /// Serialized schema digests.
    pub schema_hashes_json: &'a str,
    /// Serialized auth scheme labels.
    pub auth_schemes_json: &'a str,
    /// Insertion timestamp in Unix milliseconds.
    pub now_ms: i64,
}

/// Inserts an immutable card snapshot for a profile revision.
///
/// A snapshot for an already-recorded normalized hash is left unchanged, so
/// historical snapshots stay readable for replay/reconciliation.
pub fn insert_card_snapshot(
    connection: &Connection,
    input: A2aCardSnapshotInput<'_>,
) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO a2a_card_snapshots(profile_id,profile_revision,normalized_hash,raw_hash,origin,protocol_version,fetched_at_ms,expires_at_ms,supports_streaming,skills_json,schema_hashes_json,auth_schemes_json,created_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            input.profile_id,
            input.profile_revision as i64,
            input.normalized_hash,
            input.raw_hash,
            input.origin,
            input.protocol_version as i64,
            input.fetched_at_ms,
            input.expires_at_ms,
            input.supports_streaming as i64,
            input.skills_json,
            input.schema_hashes_json,
            input.auth_schemes_json,
            input.now_ms
        ],
    )? == 1)
}

/// Fields written for one remote task binding.
#[derive(Clone, Copy)]
pub struct A2aTaskBindingInput<'a> {
    /// Stable binding identifier.
    pub binding_id: &'a str,
    /// Delegation this binding serves.
    pub delegation_id: &'a str,
    /// Remote task id once known.
    pub remote_task_id: Option<&'a str>,
    /// Current lifecycle state label.
    pub state: &'a str,
    /// Recovery disposition label.
    pub reconciliation: &'a str,
    /// Number of submit attempts.
    pub attempt: u32,
    /// Idempotency identity used for the submit.
    pub idempotency_key: &'a str,
    /// Digest of the delegation request.
    pub request_hash: &'a str,
    /// Digest of the context capsule.
    pub capsule_hash: &'a str,
    /// Digest of the applied policy.
    pub policy_hash: &'a str,
    /// Optimistic-concurrency revision.
    pub revision: u64,
    /// Creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Last update time in Unix milliseconds.
    pub updated_at_ms: i64,
    /// Digest of the canonical binding content.
    pub content_hash: &'a str,
}

/// Inserts or advances a task binding only when its revision is strictly newer.
///
/// Returns `true` when a row was inserted or updated and `false` when the
/// stored revision is equal or newer.
pub fn upsert_task_binding(
    connection: &Connection,
    input: A2aTaskBindingInput<'_>,
) -> Result<bool, StorageError> {
    Ok(connection.execute(
        "INSERT INTO a2a_task_bindings(binding_id,delegation_id,remote_task_id,state,reconciliation,attempt,idempotency_key,request_hash,capsule_hash,policy_hash,revision,created_at_ms,updated_at_ms,content_hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
         ON CONFLICT(binding_id) DO UPDATE SET
           remote_task_id=excluded.remote_task_id, state=excluded.state,
           reconciliation=excluded.reconciliation, attempt=excluded.attempt,
           idempotency_key=excluded.idempotency_key, request_hash=excluded.request_hash,
           capsule_hash=excluded.capsule_hash, policy_hash=excluded.policy_hash,
           revision=excluded.revision, updated_at_ms=excluded.updated_at_ms,
           content_hash=excluded.content_hash
         WHERE excluded.revision > a2a_task_bindings.revision",
        params![
            input.binding_id,
            input.delegation_id,
            input.remote_task_id,
            input.state,
            input.reconciliation,
            input.attempt as i64,
            input.idempotency_key,
            input.request_hash,
            input.capsule_hash,
            input.policy_hash,
            input.revision as i64,
            input.created_at_ms,
            input.updated_at_ms,
            input.content_hash
        ],
    )? == 1)
}

/// Redacted projection of a stored task binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredA2aTaskBinding {
    /// Stable binding identifier.
    pub binding_id: String,
    /// Delegation this binding serves.
    pub delegation_id: String,
    /// Remote task id once known.
    pub remote_task_id: Option<String>,
    /// Current lifecycle state label.
    pub state: String,
    /// Optimistic-concurrency revision.
    pub revision: u64,
}

/// Lists bindings that have not reached a terminal state, oldest first.
pub fn list_nonterminal_bindings(
    connection: &Connection,
    limit: u32,
) -> Result<Vec<StoredA2aTaskBinding>, StorageError> {
    let mut statement = connection.prepare(
        "SELECT binding_id,delegation_id,remote_task_id,state,revision
         FROM a2a_task_bindings
         WHERE state NOT IN ('completed','rejected','failed','cancelled','unavailable')
         ORDER BY created_at_ms ASC LIMIT ?1",
    )?;
    let rows = statement.query_map([limit as i64], |row| {
        Ok(StoredA2aTaskBinding {
            binding_id: row.get(0)?,
            delegation_id: row.get(1)?,
            remote_task_id: row.get(2)?,
            state: row.get(3)?,
            revision: row.get::<_, i64>(4)? as u64,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Loads one stored projection payload for replay by binding id.
///
/// Returns the stored `content_hash` and revision without materializing any
/// objective, context or credential data.
pub fn binding_revision(
    connection: &Connection,
    binding_id: &str,
) -> Result<Option<(u64, String)>, StorageError> {
    use rusqlite::OptionalExtension;
    Ok(connection
        .query_row(
            "SELECT revision,content_hash FROM a2a_task_bindings WHERE binding_id=?1",
            [binding_id],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?)),
        )
        .optional()?)
}

/// Returns the largest provenance payload this store will accept.
pub const MAX_STORED_PROVENANCE_BYTES: usize = MAX_PROVENANCE_JSON_BYTES;

#[cfg(test)]
#[path = "external_agent_a2a_store_tests.rs"]
mod tests;
