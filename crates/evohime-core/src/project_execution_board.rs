//! Versioned metadata contract for the project execution board.
//!
//! This projection describes a scoped board capability only; it does not
//! schedule work, mutate tasks, or execute project actions.
//!
//! ```
//! use evohime_core::project_execution_board::{canonical_hash, projection, validate, Lifecycle, ProjectExecutionBoardRecord, SCHEMA_VERSION};
//! let mut record = ProjectExecutionBoardRecord {
//!     schema_version: SCHEMA_VERSION, id: "board-1".into(), revision: 1,
//!     lifecycle: Lifecycle::Active, scope: "project-opaque-id".into(),
//!     content_hash: String::new(),
//! };
//! record.content_hash = canonical_hash(&record)?;
//! validate(&record)?;
//! assert_eq!(projection(&record)?["external_effect"], false);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized project-board contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// Availability state for a project execution board record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Defined but not available to project consumers.
    Draft,
    /// Valid and available within its declared project scope.
    Active,
    /// Replaced by a newer board capability revision.
    Superseded,
    /// Invalid and excluded from use.
    Invalid,
}

/// Versioned metadata identifying a project execution board capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectExecutionBoardRecord {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable board record identifier.
    pub id: String,
    /// Positive revision used to order board metadata updates.
    pub revision: u64,
    /// Whether this board capability can be selected.
    pub lifecycle: Lifecycle,
    /// Opaque project or workspace scope identifier.
    pub scope: String,
    /// Digest of the canonical record with this field cleared.
    pub content_hash: String,
}

/// Validation failure for project execution board metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// The record failed schema, identity, bounds, lifecycle, or digest checks.
    #[error("invalid project_execution_board: {0}")]
    Invalid(String),
}

/// Computes the SHA-256 digest of canonical board metadata, excluding its digest field.
pub fn canonical_hash(v: &ProjectExecutionBoardRecord) -> Result<String, Error> {
    let mut c = v.clone();
    c.content_hash.clear();
    let b = serde_json::to_vec(&c).map_err(|_| Error::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(b)))
}
/// Validates the schema version, board identity, revision, lifecycle, and digest.
pub fn validate(v: &ProjectExecutionBoardRecord) -> Result<(), Error> {
    if v.schema_version != SCHEMA_VERSION
        || v.id.trim().is_empty()
        || v.id.len() > 256
        || v.scope.len() > 256
        || v.revision == 0
        || matches!(v.lifecycle, Lifecycle::Invalid)
    {
        return Err(Error::Invalid("bounds_or_lifecycle".into()));
    }
    if canonical_hash(v)? != v.content_hash {
        return Err(Error::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}
/// Produces a validated metadata-only board projection with no task side effects.
pub fn projection(v: &ProjectExecutionBoardRecord) -> Result<serde_json::Value, Error> {
    validate(v)?;
    Ok(
        serde_json::json!({"status":"metadata_only","capability":"project_execution_board","id":v.id,"revision":v.revision,"scope":v.scope,"external_effect":false}),
    )
}
