use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current schema version for voice input dictation profiles.
pub const SCHEMA_VERSION: u32 = 1;
const MAX: usize = 256;
/// Lifecycle state of a voice dictation profile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Profile is being prepared.
    Draft,
    /// Profile is eligible for selection.
    Active,
    /// Profile has been replaced by a newer revision.
    Superseded,
    /// Profile failed validation or was explicitly invalidated.
    Invalid,
}
/// Bounded configuration and adapter reference for voice input dictation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictationProfile {
    /// Schema version of the profile.
    pub schema_version: u32,
    /// Stable profile identifier.
    pub id: String,
    /// Monotonically increasing profile revision.
    pub revision: u64,
    /// Profile lifecycle state.
    pub lifecycle: Lifecycle,
    /// Scope to which the profile applies.
    pub scope: String,
    /// Locale expected by the adapter.
    pub locale: String,
    /// Listener adapter identifier.
    pub adapter_id: String,
    /// Hash of the canonical profile metadata.
    pub content_hash: String,
}
/// Validation errors for voice dictation profile metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DictationError {
    /// The profile failed schema, bounds, revision, or hash validation.
    #[error("invalid voice input dictation profile: {0}")]
    Invalid(String),
}
fn b(v: &str, n: &str) -> Result<(), DictationError> {
    if v.trim().is_empty() || v.len() > MAX {
        Err(DictationError::Invalid(format!("{n}_out_of_bounds")))
    } else {
        Ok(())
    }
}
/// Computes the canonical SHA-256 hash with `content_hash` cleared.
pub fn canonical_hash(p: &DictationProfile) -> Result<String, DictationError> {
    let mut n = p.clone();
    n.content_hash.clear();
    let bytes =
        serde_json::to_vec(&n).map_err(|_| DictationError::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
/// Validates schema version, bounded identifiers, and the canonical profile hash.
pub fn validate(p: &DictationProfile) -> Result<(), DictationError> {
    if p.schema_version != SCHEMA_VERSION {
        return Err(DictationError::Invalid("unsupported_schema_version".into()));
    }
    b(&p.id, "id")?;
    b(&p.scope, "scope")?;
    b(&p.locale, "locale")?;
    b(&p.adapter_id, "adapter_id")?;
    if p.revision == 0 {
        return Err(DictationError::Invalid("revision_invalid".into()));
    }
    if canonical_hash(p)? != p.content_hash {
        return Err(DictationError::Invalid("content_hash_mismatch".into()));
    }
    Ok(())
}
/// Returns the current availability declaration after validating the profile.
pub fn availability(p: &DictationProfile) -> Result<serde_json::Value, DictationError> {
    validate(p)?;
    Ok(
        serde_json::json!({"status":"unavailable","reason":"listener_runtime_evidence_required","adapter_id":p.adapter_id,"locale":p.locale,"raw_audio_persisted":false,"raw_transcript_projected":false}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> DictationProfile {
        let mut p = DictationProfile {
            schema_version: 1,
            id: "voice".into(),
            revision: 1,
            lifecycle: Lifecycle::Active,
            scope: "workspace".into(),
            locale: "ru-RU".into(),
            adapter_id: "listener".into(),
            content_hash: String::new(),
        };
        p.content_hash = canonical_hash(&p).unwrap();
        p
    }
    #[test]
    fn unavailable_is_typed() {
        assert_eq!(availability(&p()).unwrap()["status"], "unavailable")
    }
    #[test]
    fn tamper_fails() {
        let mut p = p();
        p.locale = "en-US".into();
        assert!(validate(&p).is_err())
    }
}
