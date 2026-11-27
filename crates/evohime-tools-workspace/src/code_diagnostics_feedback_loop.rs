//! Core-owned, revision-bound diagnostics snapshots and deterministic deltas (plan 70).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// Current schema version for persisted diagnostics snapshots.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum encoded length of identifiers and short labels.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum encoded length of resource references and fingerprints.
pub const MAX_REF_BYTES: usize = 512;
/// Maximum encoded length of one diagnostic message.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;
/// Maximum number of diagnostics accepted in one snapshot.
pub const MAX_DIAGNOSTICS: usize = 2048;
/// Maximum number of providers accepted by a diagnostics producer.
pub const MAX_PROVIDERS: usize = 64;

/// Identity and trust metadata for a diagnostics-producing provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provider {
    /// Stable provider identifier.
    pub id: String,
    /// Provider release or schema version.
    pub version: String,
    /// Provider kind, such as compiler or linter.
    pub kind: String,
    /// Trust category assigned to the provider.
    pub trust_class: String,
    /// Digest binding the provider metadata to its registered definition.
    pub content_hash: String,
}
/// Workspace and file revision to which a diagnostic is bound.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Binding {
    /// Stable identifier for the workspace root.
    pub workspace_root_id: String,
    /// Fingerprint of the workspace state.
    pub workspace_fingerprint: String,
    /// Canonical workspace-relative file reference.
    pub file_ref: String,
    /// Optional digest of the file contents observed by the provider.
    pub file_hash: Option<String>,
    /// Optional source-control or editor revision for the file.
    pub file_revision: Option<String>,
}
/// One provider-reported issue tied to a workspace revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    /// Provider-local identifier for this diagnostic.
    pub id: String,
    /// File and workspace revision associated with the result.
    pub binding: Binding,
    /// Severity label, for example `error` or `warning`.
    pub severity: String,
    /// Name of the diagnostic source.
    pub source: String,
    /// Optional source-specific diagnostic code.
    pub code: Option<String>,
    /// Human-readable explanation of the issue.
    pub message: String,
    /// Provider that produced the diagnostic.
    pub provider_id: String,
    /// Stable fingerprint used to compare snapshots.
    pub fingerprint: String,
    /// Whether the diagnostic is known to refer to an older file revision.
    pub stale: bool,
}
/// Integrity-bound set of diagnostics for one workspace revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    /// Stable snapshot identifier.
    pub id: String,
    /// Workspace fingerprint shared by all diagnostics in this snapshot.
    pub workspace_fingerprint: String,
    /// Diagnostics captured for the workspace.
    pub diagnostics: Vec<Diagnostic>,
    /// Canonical digest of the snapshot with this field cleared.
    pub content_hash: String,
}
/// Classification of diagnostics between two workspace snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Delta {
    /// Snapshot used as the comparison baseline.
    pub baseline_snapshot_id: String,
    /// Snapshot being compared with the baseline.
    pub current_snapshot_id: String,
    /// Diagnostics present only in the current snapshot.
    pub introduced: Vec<Diagnostic>,
    /// Diagnostics present only in the baseline snapshot.
    pub resolved: Vec<Diagnostic>,
    /// Diagnostics with matching fingerprints in both snapshots.
    pub persisting: Vec<Diagnostic>,
    /// Current diagnostics explicitly marked as stale.
    pub stale: Vec<Diagnostic>,
}
/// Invalid, oversized, or workspace-stale diagnostics data.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Input uses a schema version this implementation cannot read.
    #[error("unsupported diagnostics schema version")]
    UnsupportedVersion,
    /// Input exceeds a documented size or item-count bound.
    #[error("diagnostics input exceeds bounds")]
    TooLarge,
    /// A required field, path, or integrity digest is invalid.
    #[error("invalid diagnostics: {0}")]
    Invalid(String),
    /// Snapshots belong to different workspace revisions.
    #[error("diagnostics are stale")]
    Stale,
}

fn bounded(value: &str, max: usize, name: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > max || value.chars().any(|c| c.is_control()) {
        return Err(Error::Invalid(name.into()));
    }
    Ok(())
}
/// Checks provider identity, version, trust metadata, and digest fields.
pub fn validate_provider(p: &Provider) -> Result<(), Error> {
    bounded(&p.id, MAX_ID_BYTES, "provider_id")?;
    bounded(&p.version, MAX_ID_BYTES, "provider_version")?;
    bounded(&p.kind, MAX_ID_BYTES, "provider_kind")?;
    bounded(&p.trust_class, MAX_ID_BYTES, "trust_class")?;
    bounded(&p.content_hash, MAX_REF_BYTES, "content_hash")
}
/// Checks diagnostic bounds and ensures its file reference stays workspace-relative.
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
/// Serializes a value deterministically and returns its SHA-256 digest.
pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String, Error> {
    let bytes = serde_json::to_vec(value).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}
/// Validates all snapshot entries and verifies the snapshot content digest.
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
/// Classifies diagnostics by fingerprint across snapshots of the same workspace.
pub fn delta(baseline: &Snapshot, current: &Snapshot) -> Result<Delta, Error> {
    validate_snapshot(baseline)?;
    validate_snapshot(current)?;
    if baseline.workspace_fingerprint != current.workspace_fingerprint {
        return Err(Error::Stale);
    }
    let baseline_fingerprints: HashSet<&str> = baseline
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.fingerprint.as_str())
        .collect();
    let current_fingerprints: HashSet<&str> = current
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.fingerprint.as_str())
        .collect();
    let mut introduced = Vec::new();
    let mut resolved = Vec::new();
    let mut persisting = Vec::new();
    for d in &current.diagnostics {
        if baseline_fingerprints.contains(d.fingerprint.as_str()) {
            persisting.push(d.clone())
        } else {
            introduced.push(d.clone())
        }
    }
    for d in &baseline.diagnostics {
        if !current_fingerprints.contains(d.fingerprint.as_str()) {
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
