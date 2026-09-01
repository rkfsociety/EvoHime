//! Core-owned semantic artifact registry. It references ArtifactStore bytes;
//! it never becomes a second content store or an authority-bearing renderer API.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_METADATA_BYTES: usize = 64 * 1024;
pub const MAX_ID_BYTES: usize = 256;
pub const MAX_PARENTS: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactState {
    Draft,
    Produced,
    UnderReview,
    Accepted,
    NeedsRevision,
    Superseded,
    Stale,
    Rejected,
}

impl ArtifactState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Produced => "produced",
            Self::UnderReview => "under_review",
            Self::Accepted => "accepted",
            Self::NeedsRevision => "needs_revision",
            Self::Superseded => "superseded",
            Self::Stale => "stale",
            Self::Rejected => "rejected",
        }
    }
    pub fn can_transition(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Produced | Self::Rejected)
                | (
                    Self::Produced,
                    Self::UnderReview | Self::NeedsRevision | Self::Stale | Self::Superseded
                )
                | (
                    Self::UnderReview,
                    Self::Accepted | Self::NeedsRevision | Self::Rejected | Self::Stale
                )
                | (Self::NeedsRevision, Self::Produced | Self::Rejected)
                | (Self::Accepted, Self::Superseded | Self::Stale)
                | (Self::Stale, Self::UnderReview | Self::Superseded)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectArtifactRevision {
    pub schema_version: u32,
    pub artifact_id: String,
    pub project_id: String,
    pub revision: u64,
    pub state: ArtifactState,
    pub content_locator: String,
    pub content_hash: String,
    pub producer_identity: String,
    pub workspace_fingerprint: Option<String>,
    pub parent_fingerprints: Vec<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    Invalid(String),
    UnsupportedVersion,
    Limit(String),
    InvalidTransition,
    StaleRevision,
    UntrustedReference,
    SecretField,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Invalid(v) => v,
                Self::UnsupportedVersion => "unsupported_version",
                Self::Limit(v) => v,
                Self::InvalidTransition => "invalid_transition",
                Self::StaleRevision => "stale_revision",
                Self::UntrustedReference => "untrusted_reference",
                Self::SecretField => "secret_field",
            }
        )
    }
}
impl std::error::Error for RegistryError {}

pub fn validate(revision: &ProjectArtifactRevision) -> Result<(), RegistryError> {
    if revision.schema_version != CONTRACT_VERSION {
        return Err(RegistryError::UnsupportedVersion);
    }
    for value in [
        &revision.artifact_id,
        &revision.project_id,
        &revision.content_locator,
        &revision.content_hash,
        &revision.producer_identity,
    ] {
        if value.is_empty() || value.len() > MAX_ID_BYTES {
            return Err(RegistryError::Limit("id_or_ref_limit".into()));
        }
    }
    if revision.parent_fingerprints.len() > MAX_PARENTS {
        return Err(RegistryError::Limit("parent_limit".into()));
    }
    let metadata = serde_json::to_vec(&revision.metadata)
        .map_err(|_| RegistryError::Invalid("metadata".into()))?;
    if metadata.len() > MAX_METADATA_BYTES {
        return Err(RegistryError::Limit("metadata_limit".into()));
    }
    if contains_secret_key(&revision.metadata) {
        return Err(RegistryError::SecretField);
    }
    if !revision.content_locator.starts_with("artifact://") {
        return Err(RegistryError::UntrustedReference);
    }
    Ok(())
}

fn contains_secret_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            [
                "secret",
                "token",
                "credential",
                "password",
                "prompt",
                "output",
            ]
            .iter()
            .any(|part| key.contains(part))
                || contains_secret_key(value)
        }),
        Value::Array(values) => values.iter().any(contains_secret_key),
        _ => false,
    }
}

pub fn canonical_hash(revision: &ProjectArtifactRevision) -> Result<String, RegistryError> {
    validate(revision)?;
    let bytes =
        serde_json::to_vec(revision).map_err(|_| RegistryError::Invalid("serialization".into()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> ProjectArtifactRevision {
        ProjectArtifactRevision {
            schema_version: 1,
            artifact_id: "a".into(),
            project_id: "p".into(),
            revision: 1,
            state: ArtifactState::Produced,
            content_locator: "artifact://task/hash".into(),
            content_hash: "hash".into(),
            producer_identity: "role:builder".into(),
            workspace_fingerprint: Some("w1".into()),
            parent_fingerprints: vec![],
            metadata: serde_json::json!({"title":"report"}),
        }
    }
    #[test]
    fn transitions_are_fail_closed() {
        assert!(ArtifactState::Produced.can_transition(ArtifactState::UnderReview));
        assert!(!ArtifactState::Accepted.can_transition(ArtifactState::Produced));
    }
    #[test]
    fn hash_is_deterministic_and_secret_is_rejected() {
        let a = sample();
        assert_eq!(canonical_hash(&a), canonical_hash(&a));
        let mut b = a;
        b.metadata = serde_json::json!({"api_token":"x"});
        assert_eq!(validate(&b), Err(RegistryError::SecretField));
    }
}
