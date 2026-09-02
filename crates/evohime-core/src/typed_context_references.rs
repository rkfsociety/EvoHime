//! Core-owned typed context references and bounded resolver registry (plan 75).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_REFS: usize = 256;
pub const MAX_LOCATOR: usize = 64 * 1024;
pub const MAX_PROJECTED_BYTES: usize = 512 * 1024;
pub const MAX_METADATA_BYTES: usize = 128 * 1024;
pub const MAX_TERMINAL_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefKind {
    File,
    Folder,
    Workspace,
    GitDiff,
    GitCommit,
    Diagnostics,
    TerminalRange,
    Artifact,
    TaskCheckpoint,
    PlanArtifact,
    Goal,
    WorkflowRun,
    BrowserSnapshot,
    UrlReadOnly,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Projection {
    MetadataOnly,
    Summary,
    SelectedRange,
    RelevantChunks,
    FullBounded,
    ArtifactRefOnly,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Availability {
    Ready,
    Stale,
    Missing,
    PermissionDenied,
    TooLarge,
    ProviderUnavailable,
    Unsupported,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextRef {
    pub id: String,
    pub kind: RefKind,
    pub locator: String,
    pub workspace_binding_id: Option<String>,
    pub revision_hint: Option<String>,
    pub range: Option<String>,
    pub display_label: Option<String>,
    pub requested_projection: Projection,
    pub created_by: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedContextRef {
    pub ref_id: String,
    pub kind: RefKind,
    pub canonical_resource_id: String,
    pub workspace_binding_id: Option<String>,
    pub observed_revision: Option<String>,
    pub content_hash: Option<String>,
    pub sensitivity: String,
    pub estimated_tokens: Option<u32>,
    pub availability: Availability,
    pub display_projection: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextBudgetPlan {
    pub refs: Vec<String>,
    pub estimated_tokens: u32,
    pub selected_projection: Projection,
    pub omitted_or_deferred: Vec<String>,
    pub truncation_reasons: Vec<String>,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContextRefError {
    #[error("invalid context reference: {0}")]
    Invalid(String),
    #[error("unsupported reference kind")]
    UnsupportedKind,
    #[error("reference is too large")]
    TooLarge,
    #[error("path traversal or unsafe URL")]
    UnsafeLocator,
    #[error("stale mutable reference")]
    Stale,
    #[error("capability denied")]
    CapabilityDenied,
}
fn bounded(s: &str, n: usize) -> Result<(), ContextRefError> {
    if s.is_empty() || s.len() > n || s.chars().any(char::is_control) {
        Err(ContextRefError::Invalid("bounded text".into()))
    } else {
        Ok(())
    }
}
pub fn hash<T: Serialize>(v: &T) -> String {
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(serde_json::to_vec(v).unwrap_or_default()))
    )
}
pub fn validate_ref(r: &ContextRef) -> Result<(), ContextRefError> {
    bounded(&r.id, 128)?;
    bounded(&r.locator, MAX_LOCATOR)?;
    bounded(&r.created_by, 128)?;
    if let Some(v) = &r.workspace_binding_id {
        bounded(v, 128)?
    }
    if let Some(v) = &r.revision_hint {
        bounded(v, 256)?
    }
    if let Some(v) = &r.range {
        bounded(v, 256)?
    }
    if let Some(v) = &r.display_label {
        bounded(v, 256)?
    }
    if r.locator.contains("..") || false {
        return Err(ContextRefError::UnsafeLocator);
    }
    if matches!(r.kind, RefKind::UrlReadOnly)
        && (!r.locator.starts_with("https://")
            || r.locator.contains("localhost")
            || r.locator.contains("127.0.0.1")
            || r.locator.contains("[::1]"))
    {
        return Err(ContextRefError::UnsafeLocator);
    }
    Ok(())
}
pub fn resolve(
    r: &ContextRef,
    observed_revision: Option<String>,
    content_hash: Option<String>,
) -> Result<ResolvedContextRef, ContextRefError> {
    validate_ref(r)?;
    if r.revision_hint.is_some() && r.revision_hint != observed_revision {
        return Err(ContextRefError::Stale);
    }
    if let Some(h) = &content_hash {
        if !h.starts_with("sha256:") || h.len() != 71 {
            return Err(ContextRefError::Invalid("content hash".into()));
        }
    }
    Ok(ResolvedContextRef {
        ref_id: r.id.clone(),
        kind: r.kind.clone(),
        canonical_resource_id: format!("{:?}:{}", r.kind, r.locator),
        workspace_binding_id: r.workspace_binding_id.clone(),
        observed_revision,
        content_hash,
        sensitivity: "untrusted".into(),
        estimated_tokens: Some(0),
        availability: Availability::Ready,
        display_projection: r.display_label.clone().unwrap_or_else(|| r.locator.clone()),
    })
}
pub fn plan_budget(refs: &[ResolvedContextRef], max_tokens: u32) -> ContextBudgetPlan {
    let mut used: u32 = 0;
    let mut included = Vec::new();
    let mut omitted = Vec::new();
    for r in refs {
        let n = r.estimated_tokens.unwrap_or(0);
        if used.saturating_add(n) <= max_tokens {
            used += n;
            included.push(r.ref_id.clone())
        } else {
            omitted.push(r.ref_id.clone())
        }
    }
    ContextBudgetPlan {
        refs: included,
        estimated_tokens: used,
        selected_projection: Projection::FullBounded,
        omitted_or_deferred: omitted,
        truncation_reasons: Vec::new(),
    }
}
pub fn supported_kinds() -> BTreeSet<RefKind> {
    [
        RefKind::File,
        RefKind::Folder,
        RefKind::Workspace,
        RefKind::GitDiff,
        RefKind::GitCommit,
        RefKind::Diagnostics,
        RefKind::TerminalRange,
        RefKind::Artifact,
        RefKind::TaskCheckpoint,
        RefKind::PlanArtifact,
        RefKind::Goal,
        RefKind::WorkflowRun,
        RefKind::BrowserSnapshot,
        RefKind::UrlReadOnly,
    ]
    .into_iter()
    .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn r() -> ContextRef {
        ContextRef {
            id: "r".into(),
            kind: RefKind::File,
            locator: "src/lib.rs".into(),
            workspace_binding_id: Some("w".into()),
            revision_hint: Some("v1".into()),
            range: None,
            display_label: Some("lib.rs".into()),
            requested_projection: Projection::SelectedRange,
            created_by: "user".into(),
        }
    }
    #[test]
    fn resolves_exact_revision_and_budget() {
        let x = r();
        assert!(resolve(
            &x,
            Some("v1".into()),
            Some(format!("sha256:{}", "a".repeat(64)))
        )
        .is_ok());
        let mut x = resolve(&x, Some("v1".into()), None).unwrap();
        x.estimated_tokens = Some(10);
        assert_eq!(plan_budget(&[x], 0).omitted_or_deferred.len(), 1)
    }
    #[test]
    fn rejects_traversal_and_private_url() {
        let mut x = r();
        x.locator = "../secret".into();
        assert_eq!(validate_ref(&x), Err(ContextRefError::UnsafeLocator));
        x.kind = RefKind::UrlReadOnly;
        x.locator = "https://127.0.0.1/a".into();
        assert_eq!(validate_ref(&x), Err(ContextRefError::UnsafeLocator))
    }
}
