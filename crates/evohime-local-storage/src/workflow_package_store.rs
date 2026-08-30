//! Metadata-only durable ownership for Workflow Package imports.
//!
//! Package bytes are deliberately not stored here. The file remains owned by
//! the bounded Core file boundary; SQLite stores only enough information to
//! reconcile an import after a restart and to make duplicate commits safe.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const STORE_SCHEMA_VERSION: u32 = 1;
const MAX_ID_BYTES: usize = 256;
const MAX_HASH_BYTES: usize = 128;
const MAX_METADATA_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Pending,
    Committed,
    Unknown,
}

impl ImportPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Committed => "committed",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageImportRecord {
    pub import_id: String,
    pub package_hash: String,
    pub source_fingerprint: String,
    pub local_workflow_id: String,
    pub local_workflow_version: u64,
    pub phase: ImportPhase,
    pub provenance_json: String,
    pub redaction_summary_json: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowPackageStoreError {
    #[error("invalid workflow package metadata: {0}")]
    InvalidMetadata(&'static str),
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn install_schema(connection: &Connection) -> Result<(), WorkflowPackageStoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS workflow_package_imports (
            import_id TEXT PRIMARY KEY NOT NULL,
            package_hash TEXT NOT NULL,
            source_fingerprint TEXT NOT NULL,
            local_workflow_id TEXT NOT NULL,
            local_workflow_version INTEGER NOT NULL,
            phase TEXT NOT NULL CHECK(phase IN ('pending','committed','unknown')),
            provenance_json TEXT NOT NULL,
            redaction_summary_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            UNIQUE(package_hash, local_workflow_id, local_workflow_version)
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_package_hash
            ON workflow_package_imports(package_hash, phase);
        CREATE INDEX IF NOT EXISTS idx_workflow_package_source
            ON workflow_package_imports(source_fingerprint);
        CREATE TABLE IF NOT EXISTS workflow_package_bindings (
            package_hash TEXT NOT NULL,
            slot_id TEXT NOT NULL,
            local_credential_reference TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL,
            PRIMARY KEY(package_hash, slot_id)
        );",
    )?;
    Ok(())
}

fn bounded(
    name: &'static str,
    value: &str,
    maximum: usize,
    required: bool,
) -> Result<(), WorkflowPackageStoreError> {
    if required && value.trim().is_empty() {
        return Err(WorkflowPackageStoreError::InvalidMetadata(name));
    }
    if value.len() > maximum {
        return Err(WorkflowPackageStoreError::InvalidMetadata(name));
    }
    Ok(())
}

fn validate(record: &PackageImportRecord) -> Result<(), WorkflowPackageStoreError> {
    bounded("import_id", &record.import_id, MAX_ID_BYTES, true)?;
    bounded("package_hash", &record.package_hash, MAX_HASH_BYTES, true)?;
    bounded(
        "source_fingerprint",
        &record.source_fingerprint,
        MAX_HASH_BYTES,
        true,
    )?;
    bounded(
        "local_workflow_id",
        &record.local_workflow_id,
        MAX_ID_BYTES,
        true,
    )?;
    bounded(
        "provenance_json",
        &record.provenance_json,
        MAX_METADATA_BYTES,
        false,
    )?;
    bounded(
        "redaction_summary_json",
        &record.redaction_summary_json,
        MAX_METADATA_BYTES,
        false,
    )?;
    Ok(())
}

pub fn insert_pending(
    connection: &Connection,
    record: &PackageImportRecord,
) -> Result<(), WorkflowPackageStoreError> {
    validate(record)?;
    connection.execute(
        "INSERT INTO workflow_package_imports (
            import_id, package_hash, source_fingerprint, local_workflow_id,
            local_workflow_version, phase, provenance_json, redaction_summary_json,
            updated_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            record.import_id,
            record.package_hash,
            record.source_fingerprint,
            record.local_workflow_id,
            record.local_workflow_version as i64,
            ImportPhase::Pending.as_str(),
            record.provenance_json,
            record.redaction_summary_json,
            record.updated_at_ms
        ],
    )?;
    Ok(())
}

pub fn finish(
    connection: &Connection,
    import_id: &str,
    phase: ImportPhase,
    updated_at_ms: i64,
) -> Result<bool, WorkflowPackageStoreError> {
    bounded("import_id", import_id, MAX_ID_BYTES, true)?;
    let changed = connection.execute(
        "UPDATE workflow_package_imports SET phase = ?2, updated_at_ms = ?3
         WHERE import_id = ?1 AND phase = 'pending'",
        params![import_id, phase.as_str(), updated_at_ms],
    )?;
    Ok(changed == 1)
}

pub fn find_committed_by_hash(
    connection: &Connection,
    package_hash: &str,
) -> Result<Option<PackageImportRecord>, WorkflowPackageStoreError> {
    bounded("package_hash", package_hash, MAX_HASH_BYTES, true)?;
    connection.query_row(
        "SELECT import_id, package_hash, source_fingerprint, local_workflow_id,
                local_workflow_version, phase, provenance_json, redaction_summary_json, updated_at_ms
         FROM workflow_package_imports WHERE package_hash = ?1 AND phase = 'committed'
         ORDER BY updated_at_ms ASC, import_id ASC LIMIT 1",
        params![package_hash],
        |row| Ok(PackageImportRecord {
            import_id: row.get(0)?, package_hash: row.get(1)?, source_fingerprint: row.get(2)?,
            local_workflow_id: row.get(3)?, local_workflow_version: row.get::<_, i64>(4)? as u64,
            phase: ImportPhase::Committed, provenance_json: row.get(6)?, redaction_summary_json: row.get(7)?,
            updated_at_ms: row.get(8)?,
        }),
    ).optional().map_err(WorkflowPackageStoreError::from)
}

pub fn list_pending(
    connection: &Connection,
    limit: u32,
) -> Result<Vec<PackageImportRecord>, WorkflowPackageStoreError> {
    let mut statement = connection.prepare(
        "SELECT import_id, package_hash, source_fingerprint, local_workflow_id,
                local_workflow_version, provenance_json, redaction_summary_json, updated_at_ms
         FROM workflow_package_imports WHERE phase = 'pending'
         ORDER BY updated_at_ms ASC, import_id ASC LIMIT ?1",
    )?;
    let rows = statement.query_map(params![limit.clamp(1, 128)], |row| {
        Ok(PackageImportRecord {
            import_id: row.get(0)?,
            package_hash: row.get(1)?,
            source_fingerprint: row.get(2)?,
            local_workflow_id: row.get(3)?,
            local_workflow_version: row.get::<_, i64>(4)? as u64,
            phase: ImportPhase::Pending,
            provenance_json: row.get(5)?,
            redaction_summary_json: row.get(6)?,
            updated_at_ms: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(WorkflowPackageStoreError::from)
}

pub fn save_binding(
    connection: &Connection,
    package_hash: &str,
    slot_id: &str,
    local_credential_reference: &str,
    updated_at_ms: i64,
) -> Result<(), WorkflowPackageStoreError> {
    bounded("package_hash", package_hash, MAX_HASH_BYTES, true)?;
    bounded("slot_id", slot_id, MAX_ID_BYTES, true)?;
    bounded(
        "local_credential_reference",
        local_credential_reference,
        MAX_ID_BYTES,
        true,
    )?;
    connection.execute(
        "INSERT INTO workflow_package_bindings
            (package_hash, slot_id, local_credential_reference, updated_at_ms)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(package_hash, slot_id) DO UPDATE SET
            local_credential_reference = excluded.local_credential_reference,
            updated_at_ms = excluded.updated_at_ms",
        params![
            package_hash,
            slot_id,
            local_credential_reference,
            updated_at_ms
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_metadata_without_package_bytes() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let record = PackageImportRecord {
            import_id: "import-1".into(),
            package_hash: "a".repeat(64),
            source_fingerprint: "b".repeat(64),
            local_workflow_id: "local-1".into(),
            local_workflow_version: 1,
            phase: ImportPhase::Pending,
            provenance_json: "{}".into(),
            redaction_summary_json: "{}".into(),
            updated_at_ms: 1,
        };
        insert_pending(&connection, &record).unwrap();
        assert!(find_committed_by_hash(&connection, &record.package_hash)
            .unwrap()
            .is_none());
        assert_eq!(list_pending(&connection, 10).unwrap().len(), 1);
        assert!(finish(&connection, "import-1", ImportPhase::Committed, 2).unwrap());
        assert!(find_committed_by_hash(&connection, &record.package_hash)
            .unwrap()
            .is_some());
    }
}
