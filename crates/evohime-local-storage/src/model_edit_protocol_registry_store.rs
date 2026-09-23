use rusqlite::{params, Connection, OptionalExtension};

/// Fields required to create or compare-and-set a model edit protocol definition.
#[derive(Clone, Copy)]
pub struct DefinitionInput<'a> {
    /// Stable protocol identifier.
    pub protocol_id: &'a str,
    /// Semantic revision of the protocol definition.
    pub revision: u64,
    /// Model profile whose behavior the protocol configures.
    pub model_profile_id: &'a str,
    /// Serialized protocol definition.
    pub definition_json: &'a [u8],
    /// Hash of the serialized definition.
    pub content_hash: &'a str,
    /// Idempotency key for this write attempt.
    pub idempotency_key: &'a str,
    /// Current storage version expected by the caller; zero means create.
    pub expected_version: u64,
    /// Update timestamp in milliseconds.
    pub now_ms: i64,
}
/// Stored model edit protocol definition and its optimistic-lock version.
pub struct DefinitionRecord {
    /// Semantic revision of the definition.
    pub revision: u64,
    /// Model profile identifier.
    pub model_profile_id: String,
    /// Serialized definition bytes.
    pub definition_json: Vec<u8>,
    /// Hash of the serialized definition.
    pub content_hash: String,
    /// Storage version used for compare-and-set updates.
    pub version: u64,
}

/// Creates the model edit protocol registry table.
pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS model_edit_protocol_registry (protocol_id TEXT PRIMARY KEY, revision INTEGER NOT NULL, model_profile_id TEXT NOT NULL, definition_json BLOB NOT NULL, content_hash TEXT NOT NULL, version INTEGER NOT NULL, idempotency_key TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);")
}
/// Inserts a definition or updates it when the stored version matches the input fence.
pub fn save(c: &Connection, input: DefinitionInput<'_>) -> rusqlite::Result<bool> {
    let old: Option<(u64, Vec<u8>, String)> = c.query_row("SELECT version,definition_json,idempotency_key FROM model_edit_protocol_registry WHERE protocol_id=?1", [input.protocol_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    if let Some((version, json, key)) = old {
        if version == input.expected_version
            && key == input.idempotency_key
            && json == input.definition_json
        {
            return Ok(true);
        }
        if version != input.expected_version {
            return Ok(false);
        }
        let changed = c.execute("UPDATE model_edit_protocol_registry SET revision=?1,model_profile_id=?2,definition_json=?3,content_hash=?4,version=version+1,idempotency_key=?5,updated_at_ms=?6 WHERE protocol_id=?7 AND version=?8", params![input.revision as i64,input.model_profile_id,input.definition_json,input.content_hash,input.idempotency_key,input.now_ms,input.protocol_id,input.expected_version as i64])?;
        return Ok(changed == 1);
    }
    if input.expected_version != 0 {
        return Ok(false);
    }
    c.execute("INSERT INTO model_edit_protocol_registry(protocol_id,revision,model_profile_id,definition_json,content_hash,version,idempotency_key,updated_at_ms) VALUES (?1,?2,?3,?4,?5,1,?6,?7)", params![input.protocol_id,input.revision as i64,input.model_profile_id,input.definition_json,input.content_hash,input.idempotency_key,input.now_ms])?;
    Ok(true)
}
/// Loads a stored definition by protocol ID, if present.
pub fn load(c: &Connection, id: &str) -> rusqlite::Result<Option<DefinitionRecord>> {
    c.query_row("SELECT revision,model_profile_id,definition_json,content_hash,version FROM model_edit_protocol_registry WHERE protocol_id=?1", [id], |r| Ok(DefinitionRecord { revision:r.get::<_,i64>(0)? as u64, model_profile_id:r.get(1)?, definition_json:r.get(2)?, content_hash:r.get(3)?, version:r.get::<_,i64>(4)? as u64 })).optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definition_is_fenced_and_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let input = DefinitionInput {
            protocol_id: "p",
            revision: 1,
            model_profile_id: "m",
            definition_json: b"{}",
            content_hash: "h",
            idempotency_key: "k",
            expected_version: 0,
            now_ms: 1,
        };
        assert!(save(&c, input).unwrap());
        assert!(save(
            &c,
            DefinitionInput {
                expected_version: 1,
                ..input
            }
        )
        .unwrap());
        assert!(!save(
            &c,
            DefinitionInput {
                expected_version: 0,
                ..input
            }
        )
        .unwrap());
        assert_eq!(load(&c, "p").unwrap().unwrap().version, 1);
    }
}
