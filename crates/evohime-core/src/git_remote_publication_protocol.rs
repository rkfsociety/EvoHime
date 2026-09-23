use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Schema version for Git remote publication protocol metadata.
pub const SCHEMA_VERSION: u32 = 1;
const MAX: usize = 256;
/// Lifecycle of a proposed Git remote publication record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Protocol metadata is being prepared.
    Draft,
    /// Protocol metadata is eligible for inspection.
    Active,
    /// Protocol metadata has been replaced by a newer revision.
    Superseded,
    /// Protocol metadata is invalid and cannot be used.
    Invalid,
}
/// Content-addressed metadata for a proposed publication to a Git remote.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicationProtocol {
    /// Version of the serialized protocol schema.
    pub schema_version: u32,
    /// Stable identifier for this protocol record.
    pub id: String,
    /// Monotonically increasing revision of the record.
    pub revision: u64,
    /// Current lifecycle state.
    pub lifecycle: Lifecycle,
    /// Workspace or project scope that owns this record.
    pub scope: String,
    /// Configured remote reference identifier.
    pub remote_ref: String,
    /// Branch associated with the proposed publication.
    pub branch: String,
    /// Commit hash associated with the proposed publication.
    pub commit_hash: String,
    /// SHA-256 digest of the canonical record with this field cleared.
    pub content_hash: String,
}
/// Validation failures for Git remote publication metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    /// The protocol record failed schema, bounds, revision, or hash validation.
    #[error("invalid git remote publication protocol: {0}")]
    Invalid(String),
}
fn b(v: &str, n: &str) -> Result<(), ProtocolError> {
    if v.trim().is_empty() || v.len() > MAX {
        Err(ProtocolError::Invalid(format!("{n}_out_of_bounds")))
    } else {
        Ok(())
    }
}
/// Computes the canonical SHA-256 digest with `content_hash` cleared.
pub fn canonical_hash(p: &PublicationProtocol) -> Result<String, ProtocolError> {
    let mut n = p.clone();
    n.content_hash.clear();
    let bytes =
        serde_json::to_vec(&n).map_err(|_| ProtocolError::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
/// Validates schema version, bounded identifiers, revision, and content hash.
pub fn validate(p: &PublicationProtocol) -> Result<(), ProtocolError> {
    if p.schema_version != SCHEMA_VERSION {
        return Err(ProtocolError::Invalid("unsupported_schema_version".into()));
    }
    b(&p.id, "id")?;
    b(&p.scope, "scope")?;
    b(&p.remote_ref, "remote_ref")?;
    b(&p.branch, "branch")?;
    b(&p.commit_hash, "commit_hash")?;
    if p.revision == 0 {
        return Err(ProtocolError::Invalid("revision_invalid".into()));
    }
    if canonical_hash(p)? != p.content_hash {
        return Err(ProtocolError::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}
/// Returns a metadata-only publication status after validating the record.
///
/// This function does not contact a remote or publish a commit.
pub fn inspect(p: &PublicationProtocol) -> Result<serde_json::Value, ProtocolError> {
    validate(p)?;
    Ok(
        serde_json::json!({"status":"transport_unavailable","remote_ref":p.remote_ref,"branch":p.branch,"commit_hash_prefix":&p.commit_hash[..8],"effect_owner":"existing_core_git_change_set_owner"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> PublicationProtocol {
        let mut p = PublicationProtocol {
            schema_version: 1,
            id: "pub".into(),
            revision: 1,
            lifecycle: Lifecycle::Active,
            scope: "workspace".into(),
            remote_ref: "https://example.invalid/repo.git".into(),
            branch: "main".into(),
            commit_hash: "abcdef0123456789".into(),
            content_hash: String::new(),
        };
        p.content_hash = canonical_hash(&p).unwrap();
        p
    }
    #[test]
    fn transport_stays_unavailable() {
        assert_eq!(inspect(&p()).unwrap()["status"], "transport_unavailable")
    }
    #[test]
    fn tamper_fails() {
        let mut p = p();
        p.branch = "other".into();
        assert!(validate(&p).is_err())
    }
}
