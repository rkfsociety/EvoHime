//! SQLite persistence for external coding-agent presets, events, and ACP session projections.

use crate::StorageError;
use rusqlite::{params, Connection};

/// Creates the external-agent preset, revision, conversation, and event tables and indexes.
pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS external_agent_presets (id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, protocol TEXT NOT NULL, executable_ref TEXT NOT NULL, capabilities_json TEXT NOT NULL, credential_slots_json TEXT NOT NULL, control_level TEXT NOT NULL, enabled INTEGER NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, protocol_kind TEXT NOT NULL DEFAULT 'evohime_v1', auth_mode TEXT NOT NULL DEFAULT 'declared_credential_slots', backend_class TEXT NOT NULL DEFAULT 'external_agent_backend', executable_identity_json TEXT NOT NULL DEFAULT '{}'); CREATE TABLE IF NOT EXISTS external_agent_preset_revisions (preset_id TEXT NOT NULL, revision INTEGER NOT NULL, snapshot_json TEXT NOT NULL, content_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(preset_id, revision)); CREATE TABLE IF NOT EXISTS external_agent_conversations (id TEXT PRIMARY KEY NOT NULL, preset_id TEXT NOT NULL, preset_revision INTEGER NOT NULL, snapshot_json TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS external_agent_events (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, run_id TEXT NOT NULL, state TEXT NOT NULL, outcome TEXT NOT NULL, correlation_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, UNIQUE(conversation_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_external_agent_events_run ON external_agent_events(run_id);")?;
    Ok(())
}

/// Fields used to insert or revision-update a persisted external-agent preset.
#[derive(Clone, Copy)]
pub struct UpsertPresetInput<'a> {
    /// Stable preset identifier.
    pub id: &'a str,
    /// Monotonically increasing preset revision.
    pub revision: u64,
    /// Protocol label used to communicate with the agent.
    pub protocol: &'a str,
    /// Configured executable reference; interpretation is owned by the runtime.
    pub executable_ref: &'a str,
    /// Serialized capability declarations.
    pub capabilities_json: &'a str,
    /// Serialized credential-slot declarations, not secret values.
    pub slots_json: &'a str,
    /// Requested control level for the external agent.
    pub control_level: &'a str,
    /// Whether the preset may be selected for new sessions.
    pub enabled: bool,
    /// Digest of the canonical preset content.
    pub content_hash: &'a str,
    /// Protocol implementation kind.
    pub protocol_kind: &'a str,
    /// Authentication mode declared for the protocol.
    pub auth_mode: &'a str,
    /// Runtime backend class associated with the preset.
    pub backend_class: &'a str,
    /// Serialized executable identity evidence.
    pub executable_identity_json: &'a str,
    /// Update timestamp in Unix milliseconds.
    pub now_ms: i64,
}

/// Inserts a preset or updates it only when `input.revision` is newer.
///
/// Returns `true` when a row was inserted or updated and `false` when an existing revision was
/// equal or newer.
pub fn upsert_preset(
    connection: &Connection,
    input: UpsertPresetInput<'_>,
) -> Result<bool, StorageError> {
    Ok(connection.execute("INSERT INTO external_agent_presets(id,revision,protocol,executable_ref,capabilities_json,credential_slots_json,control_level,enabled,content_hash,updated_at_ms,protocol_kind,auth_mode,backend_class,executable_identity_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,protocol=excluded.protocol,executable_ref=excluded.executable_ref,capabilities_json=excluded.capabilities_json,credential_slots_json=excluded.credential_slots_json,control_level=excluded.control_level,enabled=excluded.enabled,content_hash=excluded.content_hash,updated_at_ms=excluded.updated_at_ms,protocol_kind=excluded.protocol_kind,auth_mode=excluded.auth_mode,backend_class=excluded.backend_class,executable_identity_json=excluded.executable_identity_json WHERE excluded.revision > external_agent_presets.revision", params![input.id, input.revision as i64, input.protocol, input.executable_ref, input.capabilities_json, input.slots_json, input.control_level, input.enabled as i64, input.content_hash, input.now_ms, input.protocol_kind, input.auth_mode, input.backend_class, input.executable_identity_json])? == 1)
}

/// Fields for an idempotently recorded external-agent conversation event.
#[derive(Clone, Copy)]
pub struct RecordEventInput<'a> {
    /// Conversation associated with the event.
    pub conversation_id: &'a str,
    /// EvoHime run associated with the event.
    pub run_id: &'a str,
    /// Event lifecycle state.
    pub state: &'a str,
    /// Event outcome label.
    pub outcome: &'a str,
    /// Correlation identifier propagated through the operation.
    pub correlation_id: &'a str,
    /// Key that makes repeated event delivery idempotent within a conversation.
    pub idempotency_key: &'a str,
    /// Event creation time in Unix milliseconds.
    pub now_ms: i64,
}

/// Records an external-agent event once per conversation and idempotency key.
///
/// Returns `true` if a row was inserted and `false` if the unique key already existed.
pub fn record_event(
    connection: &Connection,
    input: RecordEventInput<'_>,
) -> Result<bool, StorageError> {
    Ok(connection.execute("INSERT OR IGNORE INTO external_agent_events(conversation_id,run_id,state,outcome,correlation_id,idempotency_key,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![input.conversation_id, input.run_id, input.state, input.outcome, input.correlation_id, input.idempotency_key, input.now_ms])? == 1)
}

/// Fields persisted as the current projection of an ACP session.
#[derive(Clone, Copy)]
pub struct AcpSessionProjectionInput<'a> {
    /// Stable ACP session identifier.
    pub session_id: &'a str,
    /// External-agent preset that established the session.
    pub preset_id: &'a str,
    /// Preset revision captured for the session.
    pub preset_revision: u64,
    /// ACP protocol version negotiated by the parties.
    pub protocol_version: u32,
    /// Identity string reported by the agent.
    pub agent_identity: &'a str,
    /// Digest of the effective capability set.
    pub capability_hash: &'a str,
    /// Authentication state of the session.
    pub auth_state: &'a str,
    /// Effective control level for the session.
    pub control_level: &'a str,
    /// Privacy state associated with the session.
    pub privacy_state: &'a str,
    /// Current session state.
    pub state: &'a str,
    /// Serialized provenance details, limited to 16 KiB.
    pub provenance_json: &'a str,
    /// Expiration time in Unix milliseconds.
    pub expires_at_ms: i64,
    /// Last update time in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Inserts or refreshes a bounded ACP session projection.
///
/// Empty or oversized session IDs, or provenance larger than 16 KiB, return `false` without
/// changing the database. Older preset revisions cannot overwrite newer projections.
pub fn upsert_acp_session_projection(
    connection: &Connection,
    input: AcpSessionProjectionInput<'_>,
) -> Result<bool, StorageError> {
    if input.session_id.is_empty()
        || input.session_id.len() > 96
        || input.provenance_json.len() > 16 * 1024
    {
        return Ok(false);
    }
    Ok(connection.execute(
        "INSERT INTO acp_session_projections(session_id,preset_id,preset_revision,protocol_version,agent_identity,capability_hash,auth_state,control_level,privacy_state,state,provenance_json,expires_at_ms,updated_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(session_id) DO UPDATE SET
           preset_revision=excluded.preset_revision, protocol_version=excluded.protocol_version,
           agent_identity=excluded.agent_identity, capability_hash=excluded.capability_hash,
           auth_state=excluded.auth_state, control_level=excluded.control_level,
           privacy_state=excluded.privacy_state, state=excluded.state,
           provenance_json=excluded.provenance_json, expires_at_ms=excluded.expires_at_ms,
           updated_at_ms=excluded.updated_at_ms
         WHERE excluded.preset_revision >= acp_session_projections.preset_revision",
        params![input.session_id,input.preset_id,input.preset_revision as i64,input.protocol_version as i64,input.agent_identity,input.capability_hash,input.auth_state,input.control_level,input.privacy_state,input.state,input.provenance_json,input.expires_at_ms,input.updated_at_ms],
    )? == 1)
}

#[cfg(test)]
#[path = "external_coding_agent_adapter_store_tests.rs"]
mod tests;
