//! Additive SQLite schema for Core-owned visual workflow drafts.

use rusqlite::{Connection, OptionalExtension};

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS visual_workflow_drafts (
            draft_id TEXT PRIMARY KEY, owner_scope TEXT NOT NULL, revision INTEGER NOT NULL,
            state TEXT NOT NULL, definition_json BLOB NOT NULL, layout_json BLOB NOT NULL,
            execution_hash TEXT NOT NULL, layout_hash TEXT NOT NULL,
            composer_provenance_json BLOB,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS visual_workflow_versions (
            graph_id TEXT NOT NULL, version INTEGER NOT NULL, owner_scope TEXT NOT NULL,
            definition_json BLOB NOT NULL, execution_hash TEXT NOT NULL, composer_provenance_json BLOB, created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(graph_id, version, owner_scope)
        );
        CREATE TABLE IF NOT EXISTS visual_workflow_handoffs (
            handle TEXT PRIMARY KEY, draft_id TEXT NOT NULL REFERENCES visual_workflow_drafts(draft_id),
            owner_scope TEXT NOT NULL, draft_revision INTEGER NOT NULL, draft_hash TEXT NOT NULL,
            save_precondition TEXT NOT NULL, status TEXT NOT NULL, created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_visual_workflow_drafts_scope ON visual_workflow_drafts(owner_scope, updated_at_ms);",
    )?;
    let _ = connection.execute(
        "ALTER TABLE visual_workflow_drafts ADD COLUMN composer_provenance_json BLOB",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE visual_workflow_versions ADD COLUMN composer_provenance_json BLOB",
        [],
    );
    Ok(())
}

pub struct SaveDraft<'a> {
    pub draft_id: &'a str,
    pub owner_scope: &'a str,
    pub expected_revision: u64,
    pub definition_json: &'a [u8],
    pub layout_json: &'a [u8],
    pub execution_hash: &'a str,
    pub layout_hash: &'a str,
    pub composer_provenance_json: Option<&'a [u8]>,
    pub updated_at_ms: i64,
}

pub fn save_draft(
    connection: &Connection,
    input: SaveDraft<'_>,
) -> rusqlite::Result<Result<u64, &'static str>> {
    let tx = connection.unchecked_transaction()?;
    let current: Option<u64> = tx
        .query_row(
            "SELECT revision FROM visual_workflow_drafts WHERE draft_id=?1 AND owner_scope=?2",
            (input.draft_id, input.owner_scope),
            |row| row.get(0),
        )
        .optional()?;
    if current.unwrap_or(0) != input.expected_revision {
        return Ok(Err("stale_revision"));
    }
    let revision = input.expected_revision + 1;
    tx.execute(
        "INSERT INTO visual_workflow_drafts(draft_id,owner_scope,revision,state,definition_json,layout_json,execution_hash,layout_hash,composer_provenance_json,updated_at_ms) VALUES(?1,?2,?3,'valid',?4,?5,?6,?7,?8,?9) ON CONFLICT(draft_id) DO UPDATE SET owner_scope=excluded.owner_scope,revision=excluded.revision,state='valid',definition_json=excluded.definition_json,layout_json=excluded.layout_json,execution_hash=excluded.execution_hash,layout_hash=excluded.layout_hash,composer_provenance_json=COALESCE(excluded.composer_provenance_json, visual_workflow_drafts.composer_provenance_json),updated_at_ms=excluded.updated_at_ms",
        rusqlite::params![input.draft_id, input.owner_scope, revision, input.definition_json, input.layout_json, input.execution_hash, input.layout_hash, input.composer_provenance_json, input.updated_at_ms],
    )?;
    tx.commit()?;
    Ok(Ok(revision))
}

pub struct Handoff<'a> {
    pub handle: &'a str,
    pub draft_id: &'a str,
    pub owner_scope: &'a str,
    pub revision: u64,
    pub draft_hash: &'a str,
    pub precondition: &'a str,
    pub created_at_ms: i64,
}

pub fn issue_handoff(connection: &Connection, input: Handoff<'_>) -> rusqlite::Result<()> {
    connection.execute("INSERT OR REPLACE INTO visual_workflow_handoffs(handle,draft_id,owner_scope,draft_revision,draft_hash,save_precondition,status,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'active',?7)", rusqlite::params![input.handle,input.draft_id,input.owner_scope,input.revision,input.draft_hash,input.precondition,input.created_at_ms])?;
    Ok(())
}

pub fn consume_handoff(
    connection: &Connection,
    handle: &str,
    owner_scope: &str,
) -> rusqlite::Result<bool> {
    let changed = connection.execute("UPDATE visual_workflow_handoffs SET status='consumed' WHERE handle=?1 AND owner_scope=?2 AND status='active'", (handle, owner_scope))?;
    Ok(changed == 1)
}

pub fn publish_version(
    connection: &Connection,
    graph_id: &str,
    owner_scope: &str,
    version: u64,
    definition_json: &[u8],
    execution_hash: &str,
    created_at_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO visual_workflow_versions(graph_id,version,owner_scope,definition_json,execution_hash,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6)", rusqlite::params![graph_id,version,owner_scope,definition_json,execution_hash,created_at_ms])?;
    Ok(())
}

pub fn publish_from_handoff(
    connection: &Connection,
    handle: &str,
    draft_id: &str,
    owner_scope: &str,
    created_at_ms: i64,
) -> rusqlite::Result<PublishResult> {
    let tx = connection.unchecked_transaction()?;
    let handoff: Option<(u64, String)> = tx.query_row(
        "SELECT draft_revision, draft_hash FROM visual_workflow_handoffs WHERE handle=?1 AND draft_id=?2 AND owner_scope=?3 AND status='active'",
        (handle, draft_id, owner_scope),
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    let Some((handoff_revision, handoff_hash)) = handoff else {
        return Ok(Err("invalid_handoff"));
    };
    let row: DraftRow = tx.query_row("SELECT revision, definition_json, execution_hash, layout_hash FROM visual_workflow_drafts WHERE draft_id=?1 AND owner_scope=?2", (draft_id, owner_scope), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?;
    if row.0 != handoff_revision || row.2 != handoff_hash {
        return Ok(Err("stale_handoff"));
    }
    let changed = tx.execute("UPDATE visual_workflow_handoffs SET status='consumed' WHERE handle=?1 AND draft_id=?2 AND owner_scope=?3 AND status='active'", (handle, draft_id, owner_scope))?;
    if changed != 1 {
        return Ok(Err("invalid_handoff"));
    }
    let definition: serde_json::Value =
        serde_json::from_slice(&row.1).map_err(|_| rusqlite::Error::InvalidQuery)?;
    let graph_id = definition
        .get("graph")
        .and_then(|graph| graph.get("graph_id"))
        .and_then(serde_json::Value::as_str)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let version = definition
        .get("graph")
        .and_then(|graph| graph.get("version"))
        .and_then(serde_json::Value::as_u64)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let provenance: Option<Vec<u8>> = tx.query_row("SELECT composer_provenance_json FROM visual_workflow_drafts WHERE draft_id=?1 AND owner_scope=?2", (draft_id, owner_scope), |value| value.get(0))?;
    tx.execute("INSERT INTO visual_workflow_versions(graph_id,version,owner_scope,definition_json,execution_hash,composer_provenance_json,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", rusqlite::params![graph_id,version,owner_scope,row.1,row.2,provenance,created_at_ms])?;
    tx.commit()?;
    Ok(Ok(row))
}

pub type DraftRow = (u64, Vec<u8>, String, String);
pub type PublishResult = Result<DraftRow, &'static str>;

pub fn read_draft(
    connection: &Connection,
    draft_id: &str,
    owner_scope: &str,
) -> rusqlite::Result<Option<DraftRow>> {
    connection.query_row("SELECT revision, definition_json, execution_hash, layout_hash FROM visual_workflow_drafts WHERE draft_id=?1 AND owner_scope=?2", (draft_id, owner_scope), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_is_idempotent_and_separates_layout_hash() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        install_schema(&connection).unwrap();
        let tables: i64 = connection.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE 'visual_workflow_%'", [], |row| row.get(0)).unwrap();
        assert_eq!(tables, 3);
    }

    #[test]
    fn draft_revision_and_handoff_publish_are_atomic() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let definition = br#"{"contract_version":"visual-workflow-builder/v1","graph":{"graph_id":"g","version":1},"layout":{}}"#;
        let first = save_draft(
            &connection,
            SaveDraft {
                draft_id: "d",
                owner_scope: "w",
                expected_revision: 0,
                definition_json: definition,
                layout_json: b"{}",
                execution_hash: "e",
                layout_hash: "l",
                composer_provenance_json: None,
                updated_at_ms: 1,
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(first, 1);
        save_draft(
            &connection,
            SaveDraft {
                draft_id: "d",
                owner_scope: "w",
                expected_revision: 1,
                definition_json: definition,
                layout_json: b"{}",
                execution_hash: "e",
                layout_hash: "l",
                composer_provenance_json: Some(b"{\"request_hash\":\"r\"}"),
                updated_at_ms: 2,
            },
        )
        .unwrap()
        .unwrap();
        save_draft(
            &connection,
            SaveDraft {
                draft_id: "d",
                owner_scope: "w",
                expected_revision: 2,
                definition_json: definition,
                layout_json: b"{}",
                execution_hash: "e",
                layout_hash: "l",
                composer_provenance_json: None,
                updated_at_ms: 2,
            },
        )
        .unwrap()
        .unwrap();
        let provenance: Option<Vec<u8>> = connection
            .query_row(
                "SELECT composer_provenance_json FROM visual_workflow_drafts WHERE draft_id='d'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            provenance.as_deref(),
            Some(b"{\"request_hash\":\"r\"}".as_slice())
        );
        assert_eq!(
            save_draft(
                &connection,
                SaveDraft {
                    draft_id: "d",
                    owner_scope: "w",
                    expected_revision: 0,
                    definition_json: definition,
                    layout_json: b"{}",
                    execution_hash: "e",
                    layout_hash: "l",
                    composer_provenance_json: None,
                    updated_at_ms: 2
                }
            )
            .unwrap(),
            Err("stale_revision")
        );
        issue_handoff(
            &connection,
            Handoff {
                handle: "h",
                draft_id: "d",
                owner_scope: "w",
                revision: 3,
                draft_hash: "e",
                precondition: "3:e",
                created_at_ms: 3,
            },
        )
        .unwrap();
        assert!(publish_from_handoff(&connection, "h", "d", "w", 4)
            .unwrap()
            .is_ok());
        assert!(publish_from_handoff(&connection, "h", "d", "w", 5)
            .unwrap()
            .is_err());
        let versions: i64 = connection
            .query_row("SELECT count(*) FROM visual_workflow_versions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(versions, 1);
    }

    #[test]
    fn stale_handoff_cannot_publish_new_draft_revision() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let definition = br#"{"contract_version":"visual-workflow-builder/v1","graph":{"graph_id":"g","version":1},"layout":{}}"#;
        save_draft(
            &connection,
            SaveDraft {
                draft_id: "d",
                owner_scope: "w",
                expected_revision: 0,
                definition_json: definition,
                layout_json: b"{}",
                execution_hash: "e",
                layout_hash: "l",
                composer_provenance_json: None,
                updated_at_ms: 1,
            },
        )
        .unwrap()
        .unwrap();
        issue_handoff(
            &connection,
            Handoff {
                handle: "h",
                draft_id: "d",
                owner_scope: "w",
                revision: 1,
                draft_hash: "e",
                precondition: "1:e",
                created_at_ms: 2,
            },
        )
        .unwrap();
        save_draft(
            &connection,
            SaveDraft {
                draft_id: "d",
                owner_scope: "w",
                expected_revision: 1,
                definition_json: definition,
                layout_json: b"{}",
                execution_hash: "new",
                layout_hash: "l",
                composer_provenance_json: None,
                updated_at_ms: 3,
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            publish_from_handoff(&connection, "h", "d", "w", 4).unwrap(),
            Err("stale_handoff")
        );
        let status: String = connection
            .query_row(
                "SELECT status FROM visual_workflow_handoffs WHERE handle='h'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "active");
    }
}
