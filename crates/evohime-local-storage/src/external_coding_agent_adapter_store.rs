use crate::StorageError;
use rusqlite::{params, Connection};

pub fn install_schema(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS external_agent_presets (id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL, protocol TEXT NOT NULL, executable_ref TEXT NOT NULL, capabilities_json TEXT NOT NULL, credential_slots_json TEXT NOT NULL, control_level TEXT NOT NULL, enabled INTEGER NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL, protocol_kind TEXT NOT NULL DEFAULT 'evohime_v1', auth_mode TEXT NOT NULL DEFAULT 'declared_credential_slots', backend_class TEXT NOT NULL DEFAULT 'external_agent_backend', executable_identity_json TEXT NOT NULL DEFAULT '{}'); CREATE TABLE IF NOT EXISTS external_agent_preset_revisions (preset_id TEXT NOT NULL, revision INTEGER NOT NULL, snapshot_json TEXT NOT NULL, content_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(preset_id, revision)); CREATE TABLE IF NOT EXISTS external_agent_conversations (id TEXT PRIMARY KEY NOT NULL, preset_id TEXT NOT NULL, preset_revision INTEGER NOT NULL, snapshot_json TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS external_agent_events (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, run_id TEXT NOT NULL, state TEXT NOT NULL, outcome TEXT NOT NULL, correlation_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, created_at_ms INTEGER NOT NULL, UNIQUE(conversation_id, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_external_agent_events_run ON external_agent_events(run_id);")?;
    Ok(())
}

#[derive(Clone, Copy)]
pub struct UpsertPresetInput<'a> {
    pub id: &'a str,
    pub revision: u64,
    pub protocol: &'a str,
    pub executable_ref: &'a str,
    pub capabilities_json: &'a str,
    pub slots_json: &'a str,
    pub control_level: &'a str,
    pub enabled: bool,
    pub content_hash: &'a str,
    pub protocol_kind: &'a str,
    pub auth_mode: &'a str,
    pub backend_class: &'a str,
    pub executable_identity_json: &'a str,
    pub now_ms: i64,
}

pub fn upsert_preset(
    connection: &Connection,
    input: UpsertPresetInput<'_>,
) -> Result<bool, StorageError> {
    Ok(connection.execute("INSERT INTO external_agent_presets(id,revision,protocol,executable_ref,capabilities_json,credential_slots_json,control_level,enabled,content_hash,updated_at_ms,protocol_kind,auth_mode,backend_class,executable_identity_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,protocol=excluded.protocol,executable_ref=excluded.executable_ref,capabilities_json=excluded.capabilities_json,credential_slots_json=excluded.credential_slots_json,control_level=excluded.control_level,enabled=excluded.enabled,content_hash=excluded.content_hash,updated_at_ms=excluded.updated_at_ms,protocol_kind=excluded.protocol_kind,auth_mode=excluded.auth_mode,backend_class=excluded.backend_class,executable_identity_json=excluded.executable_identity_json WHERE excluded.revision > external_agent_presets.revision", params![input.id, input.revision as i64, input.protocol, input.executable_ref, input.capabilities_json, input.slots_json, input.control_level, input.enabled as i64, input.content_hash, input.now_ms, input.protocol_kind, input.auth_mode, input.backend_class, input.executable_identity_json])? == 1)
}

#[derive(Clone, Copy)]
pub struct RecordEventInput<'a> {
    pub conversation_id: &'a str,
    pub run_id: &'a str,
    pub state: &'a str,
    pub outcome: &'a str,
    pub correlation_id: &'a str,
    pub idempotency_key: &'a str,
    pub now_ms: i64,
}

pub fn record_event(
    connection: &Connection,
    input: RecordEventInput<'_>,
) -> Result<bool, StorageError> {
    Ok(connection.execute("INSERT OR IGNORE INTO external_agent_events(conversation_id,run_id,state,outcome,correlation_id,idempotency_key,created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![input.conversation_id, input.run_id, input.state, input.outcome, input.correlation_id, input.idempotency_key, input.now_ms])? == 1)
}

#[derive(Clone, Copy)]
pub struct AcpSessionProjectionInput<'a> {
    pub session_id: &'a str,
    pub preset_id: &'a str,
    pub preset_revision: u64,
    pub protocol_version: u32,
    pub agent_identity: &'a str,
    pub capability_hash: &'a str,
    pub auth_state: &'a str,
    pub control_level: &'a str,
    pub privacy_state: &'a str,
    pub state: &'a str,
    pub provenance_json: &'a str,
    pub expires_at_ms: i64,
    pub updated_at_ms: i64,
}

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
