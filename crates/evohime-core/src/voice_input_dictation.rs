use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA_VERSION: u32 = 1;
const MAX: usize = 256;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Draft,
    Active,
    Superseded,
    Invalid,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictationProfile {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub lifecycle: Lifecycle,
    pub scope: String,
    pub locale: String,
    pub adapter_id: String,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DictationError {
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
pub fn canonical_hash(p: &DictationProfile) -> Result<String, DictationError> {
    let mut n = p.clone();
    n.content_hash.clear();
    let bytes =
        serde_json::to_vec(&n).map_err(|_| DictationError::Invalid("not_serializable".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
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
