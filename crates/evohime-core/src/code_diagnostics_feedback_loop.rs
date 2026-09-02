//! Core-owned, revision-bound diagnostics snapshots and deterministic deltas (plan 70).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_REF_BYTES: usize = 512;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;
pub const MAX_DIAGNOSTICS: usize = 2048;
pub const MAX_PROVIDERS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provider {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub trust_class: String,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Binding {
    pub workspace_root_id: String,
    pub workspace_fingerprint: String,
    pub file_ref: String,
    pub file_hash: Option<String>,
    pub file_revision: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub id: String,
    pub binding: Binding,
    pub severity: String,
    pub source: String,
    pub code: Option<String>,
    pub message: String,
    pub provider_id: String,
    pub fingerprint: String,
    pub stale: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub id: String,
    pub workspace_fingerprint: String,
    pub diagnostics: Vec<Diagnostic>,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Delta {
    pub baseline_snapshot_id: String,
    pub current_snapshot_id: String,
    pub introduced: Vec<Diagnostic>,
    pub resolved: Vec<Diagnostic>,
    pub persisting: Vec<Diagnostic>,
    pub stale: Vec<Diagnostic>,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("unsupported diagnostics schema version")]
    UnsupportedVersion,
    #[error("diagnostics input exceeds bounds")]
    TooLarge,
    #[error("invalid diagnostics: {0}")]
    Invalid(String),
    #[error("diagnostics are stale")]
    Stale,
}

fn bounded(value: &str, max: usize, name: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > max || value.chars().any(|c| c.is_control()) {
        return Err(Error::Invalid(name.into()));
    }
    Ok(())
}
pub fn validate_provider(p: &Provider) -> Result<(), Error> {
    bounded(&p.id, MAX_ID_BYTES, "provider_id")?;
    bounded(&p.version, MAX_ID_BYTES, "provider_version")?;
    bounded(&p.kind, MAX_ID_BYTES, "provider_kind")?;
    bounded(&p.trust_class, MAX_ID_BYTES, "trust_class")?;
    bounded(&p.content_hash, MAX_REF_BYTES, "content_hash")
}
pub fn validate_diagnostic(d: &Diagnostic) -> Result<(), Error> {
    bounded(&d.id, MAX_ID_BYTES, "id")?;
    bounded(
        &d.binding.workspace_root_id,
        MAX_ID_BYTES,
        "workspace_root_id",
    )?;
    bounded(
        &d.binding.workspace_fingerprint,
        MAX_REF_BYTES,
        "workspace_fingerprint",
    )?;
    bounded(&d.binding.file_ref, MAX_REF_BYTES, "file_ref")?;
    bounded(&d.severity, MAX_ID_BYTES, "severity")?;
    bounded(&d.source, MAX_ID_BYTES, "source")?;
    bounded(&d.provider_id, MAX_ID_BYTES, "provider_id")?;
    bounded(&d.message, MAX_MESSAGE_BYTES, "message")?;
    if let Some(v) = &d.code {
        bounded(v, MAX_ID_BYTES, "code")?;
    }
    if !d.binding.file_ref.starts_with("/") || d.binding.file_ref.contains("..") {
        return Err(Error::Invalid(
            "file_ref must be canonical workspace-relative".into(),
        ));
    }
    Ok(())
}
pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, Error> {
    let bytes = serde_json::to_vec(value).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}
pub fn validate_snapshot(s: &Snapshot) -> Result<(), Error> {
    bounded(&s.id, MAX_ID_BYTES, "snapshot_id")?;
    bounded(
        &s.workspace_fingerprint,
        MAX_REF_BYTES,
        "workspace_fingerprint",
    )?;
    if s.diagnostics.len() > MAX_DIAGNOSTICS {
        return Err(Error::TooLarge);
    }
    for d in &s.diagnostics {
        validate_diagnostic(d)?;
    }
    let mut copy = s.clone();
    copy.content_hash.clear();
    if s.content_hash != canonical_hash(&copy)? {
        return Err(Error::Invalid("content_hash".into()));
    }
    Ok(())
}
pub fn delta(baseline: &Snapshot, current: &Snapshot) -> Result<Delta, Error> {
    validate_snapshot(baseline)?;
    validate_snapshot(current)?;
    if baseline.workspace_fingerprint != current.workspace_fingerprint {
        return Err(Error::Stale);
    }
    let mut introduced = Vec::new();
    let mut resolved = Vec::new();
    let mut persisting = Vec::new();
    for d in &current.diagnostics {
        if baseline
            .diagnostics
            .iter()
            .any(|x| x.fingerprint == d.fingerprint)
        {
            persisting.push(d.clone())
        } else {
            introduced.push(d.clone())
        }
    }
    for d in &baseline.diagnostics {
        if !current
            .diagnostics
            .iter()
            .any(|x| x.fingerprint == d.fingerprint)
        {
            resolved.push(d.clone())
        }
    }
    Ok(Delta {
        baseline_snapshot_id: baseline.id.clone(),
        current_snapshot_id: current.id.clone(),
        introduced,
        resolved,
        persisting,
        stale: current
            .diagnostics
            .iter()
            .filter(|d| d.stale)
            .cloned()
            .collect(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn d(fp: &str) -> Diagnostic {
        Diagnostic {
            id: fp.into(),
            binding: Binding {
                workspace_root_id: "w".into(),
                workspace_fingerprint: "wf".into(),
                file_ref: "/src/lib.rs".into(),
                file_hash: None,
                file_revision: None,
            },
            severity: "error".into(),
            source: "test".into(),
            code: None,
            message: "bad".into(),
            provider_id: "p".into(),
            fingerprint: fp.into(),
            stale: false,
        }
    }
    fn s(id: &str, ds: Vec<Diagnostic>) -> Snapshot {
        let mut s = Snapshot {
            id: id.into(),
            workspace_fingerprint: "wf".into(),
            diagnostics: ds,
            content_hash: String::new(),
        };
        s.content_hash = canonical_hash(&{
            let mut c = s.clone();
            c.content_hash.clear();
            c
        })
        .unwrap();
        s
    }
    #[test]
    fn deterministic_delta() {
        let x = s("b", vec![d("old")]);
        let y = s("c", vec![d("new")]);
        let z = delta(&x, &y).unwrap();
        assert_eq!(z.introduced.len(), 1);
        assert_eq!(z.resolved.len(), 1)
    }
    #[test]
    fn rejects_traversal() {
        let mut x = d("x");
        x.binding.file_ref = "/../secret".into();
        assert!(validate_diagnostic(&x).is_err())
    }
}
