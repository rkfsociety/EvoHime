use rusqlite::Transaction;

pub(crate) fn apply(transaction: &Transaction<'_>, current: u32) -> Result<(), rusqlite::Error> {
    if current >= 169 {
        return Ok(());
    }
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS external_agent_presets (
           id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL,
           protocol TEXT NOT NULL, executable_ref TEXT NOT NULL,
           capabilities_json TEXT NOT NULL, credential_slots_json TEXT NOT NULL,
           control_level TEXT NOT NULL, enabled INTEGER NOT NULL,
           content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL
         );
         ALTER TABLE external_agent_presets ADD COLUMN protocol_kind TEXT NOT NULL DEFAULT 'evohime_v1';
         ALTER TABLE external_agent_presets ADD COLUMN auth_mode TEXT NOT NULL DEFAULT 'declared_credential_slots';
         ALTER TABLE external_agent_presets ADD COLUMN backend_class TEXT NOT NULL DEFAULT 'external_agent_backend';
         ALTER TABLE external_agent_presets ADD COLUMN executable_identity_json TEXT NOT NULL DEFAULT '{}';
         CREATE TABLE IF NOT EXISTS acp_session_projections (
           session_id TEXT PRIMARY KEY NOT NULL,
           preset_id TEXT NOT NULL,
           preset_revision INTEGER NOT NULL,
           protocol_version INTEGER NOT NULL,
           agent_identity TEXT NOT NULL,
           capability_hash TEXT NOT NULL,
           auth_state TEXT NOT NULL,
           control_level TEXT NOT NULL,
           privacy_state TEXT NOT NULL,
           state TEXT NOT NULL,
           provenance_json TEXT NOT NULL,
           expires_at_ms INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_acp_sessions_expiry ON acp_session_projections(expires_at_ms);
         PRAGMA user_version = 169;"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn migration_installs_acp_projection_atomically() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE external_agent_presets (id TEXT PRIMARY KEY, revision INTEGER NOT NULL, protocol TEXT NOT NULL, executable_ref TEXT NOT NULL, capabilities_json TEXT NOT NULL, credential_slots_json TEXT NOT NULL, control_level TEXT NOT NULL, enabled INTEGER NOT NULL, content_hash TEXT NOT NULL, updated_at_ms INTEGER NOT NULL);").unwrap();
        let transaction = connection.unchecked_transaction().unwrap();
        apply(&transaction, 168).unwrap();
        transaction.commit().unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            169
        );
        assert!(connection
            .query_row("SELECT COUNT(*) FROM acp_session_projections", [], |row| {
                row.get::<_, u32>(0)
            })
            .is_ok());
    }
}
