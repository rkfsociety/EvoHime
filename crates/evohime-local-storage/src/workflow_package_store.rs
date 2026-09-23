//! Metadata-only durable ownership for Workflow Package imports.
//!
//! Package bytes are deliberately not stored here. The file remains owned by
//! the bounded Core file boundary; SQLite stores only enough information to
//! reconcile an import after a restart and to make duplicate commits safe.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Version of the workflow package import and binding schema.
pub const STORE_SCHEMA_VERSION: u32 = 1;
const MAX_ID_BYTES: usize = 256;
const MAX_HASH_BYTES: usize = 128;
const MAX_METADATA_BYTES: usize = 8 * 1024;

/// Lifecycle phase used to reconcile a package import after interruption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    /// Import has been recorded but has not reached a terminal result.
    Pending,
    /// Imported workflow version was committed successfully.
    Committed,
    /// Import outcome cannot be confirmed and requires reconciliation.
    Unknown,
}

impl ImportPhase {
    /// Returns the stable value stored in SQLite.
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Committed => "committed",
            Self::Unknown => "unknown",
        }
    }
}

/// Metadata required to resume or deduplicate one package import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageImportRecord {
    /// Stable identifier for this import operation.
    pub import_id: String,
    /// Digest of the imported package bytes.
    pub package_hash: String,
    /// Fingerprint of the package's source identity.
    pub source_fingerprint: String,
    /// Identifier assigned to the imported local workflow.
    pub local_workflow_id: String,
    /// Version created in the local workflow store.
    pub local_workflow_version: u64,
    /// Current import lifecycle phase.
    pub phase: ImportPhase,
    /// Bounded serialized import provenance.
    pub provenance_json: String,
    /// Bounded summary of redactions performed during import.
    pub redaction_summary_json: String,
    /// Last update time in Unix milliseconds.
    pub updated_at_ms: i64,
}

/// Validation, SQLite, and JSON errors from workflow package metadata operations.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowPackageStoreError {
    /// Required metadata is empty or exceeds its byte limit.
    #[error("invalid workflow package metadata: {0}")]
    InvalidMetadata(&'static str),
    /// SQLite query or row conversion failed.
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// JSON serialization or deserialization failed.
    #[error("JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
}

/// Creates package import and local credential binding tables and lookup indexes.
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

/// Persists a validated import record in the pending phase.
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

/// Moves a pending import to a terminal or reconciliation phase.
///
/// Returns `false` if the import is absent or is no longer pending.
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

/// Finds the earliest committed import for a package hash.
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

/// Lists pending imports oldest first, capped at 128 rows.
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

/// Saves a package credential-slot binding to a local credential reference.
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
#[path = "workflow_package_store_tests.rs"]
mod tests;
