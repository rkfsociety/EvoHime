//! Additive SQLite schema for Core-owned visual workflow drafts.

use rusqlite::{Connection, OptionalExtension};

/// Creates tables for visual workflow drafts, published versions, and one-time handoffs.
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

/// Inputs for a revision-checked visual workflow draft save.
pub struct SaveDraft<'a> {
    /// Stable draft identifier.
    pub draft_id: &'a str,
    /// Owner scope that isolates the draft.
    pub owner_scope: &'a str,
    /// Current revision expected by the caller; zero creates a new draft.
    pub expected_revision: u64,
    /// Serialized workflow definition.
    pub definition_json: &'a [u8],
    /// Serialized visual layout metadata.
    pub layout_json: &'a [u8],
    /// Digest of execution-relevant workflow content.
    pub execution_hash: &'a str,
    /// Digest of layout metadata.
    pub layout_hash: &'a str,
    /// Optional serialized provenance for the composer that produced the draft.
    pub composer_provenance_json: Option<&'a [u8]>,
    /// Save timestamp in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Saves a draft only when its current revision matches the supplied precondition.
///
/// Returns a nested `Err("stale_revision")` for a revision conflict; SQLite errors remain the
/// outer error result.
pub fn save_draft(
    connection: &Connection,
    input: SaveDraft<'_>,
) -> rusqlite::Result<Result<u64, &'static str>> {
    let tx = connection.unchecked_transaction()?;
    let current: Option<(String, u64)> = tx
        .query_row(
            "SELECT owner_scope, revision FROM visual_workflow_drafts WHERE draft_id=?1",
            [input.draft_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if current
        .as_ref()
        .is_some_and(|(owner_scope, _)| owner_scope != input.owner_scope)
    {
        return Ok(Err("owner_conflict"));
    }
    if current.map(|(_, revision)| revision).unwrap_or(0) != input.expected_revision {
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

/// Single-use authorization to publish one exact revision and hash of a draft.
pub struct Handoff<'a> {
    /// Opaque handoff handle.
    pub handle: &'a str,
    /// Draft authorized for publication.
    pub draft_id: &'a str,
    /// Owner scope that owns both draft and handoff.
    pub owner_scope: &'a str,
    /// Draft revision captured by this handoff.
    pub revision: u64,
    /// Execution hash captured by this handoff.
    pub draft_hash: &'a str,
    /// Save precondition captured when issuing the handoff.
    pub precondition: &'a str,
    /// Handoff creation time in Unix milliseconds.
    pub created_at_ms: i64,
}

/// Issues or reactivates a handle bound to one draft revision and execution hash.
pub fn issue_handoff(connection: &Connection, input: Handoff<'_>) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO visual_workflow_handoffs(handle,draft_id,owner_scope,draft_revision,draft_hash,save_precondition,status,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'active',?7) ON CONFLICT(handle) DO UPDATE SET draft_id=excluded.draft_id,owner_scope=excluded.owner_scope,draft_revision=excluded.draft_revision,draft_hash=excluded.draft_hash,save_precondition=excluded.save_precondition,status='active',created_at_ms=excluded.created_at_ms", rusqlite::params![input.handle,input.draft_id,input.owner_scope,input.revision,input.draft_hash,input.precondition,input.created_at_ms])?;
    Ok(())
}

/// Consumes an active handle for its owner scope, returning whether it was active.
pub fn consume_handoff(
    connection: &Connection,
    handle: &str,
    owner_scope: &str,
) -> rusqlite::Result<bool> {
    let changed = connection.execute("UPDATE visual_workflow_handoffs SET status='consumed' WHERE handle=?1 AND owner_scope=?2 AND status='active'", (handle, owner_scope))?;
    Ok(changed == 1)
}

/// Inserts an immutable published workflow version if the scoped version is not already present.
pub fn publish_version(
    connection: &Connection,
    graph_id: &str,
    owner_scope: &str,
    version: u64,
    definition_json: &[u8],
    execution_hash: &str,
    created_at_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO visual_workflow_versions(graph_id,version,owner_scope,definition_json,execution_hash,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(graph_id,version,owner_scope) DO NOTHING", rusqlite::params![graph_id,version,owner_scope,definition_json,execution_hash,created_at_ms])?;
    Ok(())
}

/// Publishes a draft only when its active handoff still matches the draft revision and hash.
///
/// Returns `invalid_handoff` for a missing or already consumed handle and `stale_handoff` when the
/// draft changed after the handle was issued.
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
    tx.execute("INSERT INTO visual_workflow_versions(graph_id,version,owner_scope,definition_json,execution_hash,composer_provenance_json,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(graph_id,version,owner_scope) DO NOTHING", rusqlite::params![graph_id,version,owner_scope,row.1,row.2,provenance,created_at_ms])?;
    tx.commit()?;
    Ok(Ok(row))
}

/// Selected draft fields: revision, definition JSON, execution hash, and layout hash.
pub type DraftRow = (u64, Vec<u8>, String, String);
/// Nested publication outcome returned by [`publish_from_handoff`].
pub type PublishResult = Result<DraftRow, &'static str>;

/// Reads the current draft fields for an owner-scoped identifier.
pub fn read_draft(
    connection: &Connection,
    draft_id: &str,
    owner_scope: &str,
) -> rusqlite::Result<Option<DraftRow>> {
    connection.query_row("SELECT revision, definition_json, execution_hash, layout_hash FROM visual_workflow_drafts WHERE draft_id=?1 AND owner_scope=?2", (draft_id, owner_scope), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).optional()
}

/// Reads optional provenance for one owner-scoped draft identifier.
pub fn read_draft_provenance(
    connection: &Connection,
    draft_id: &str,
    owner_scope: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT composer_provenance_json FROM visual_workflow_drafts WHERE draft_id=?1 AND owner_scope=?2",
            (draft_id, owner_scope),
            |row| row.get(0),
        )
        .optional()
        .map(Option::flatten)
}

#[cfg(test)]
#[path = "visual_workflow_builder_store_tests.rs"]
mod tests;
