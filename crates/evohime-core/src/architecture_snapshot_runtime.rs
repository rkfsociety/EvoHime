//! Deterministic, non-executing architecture evidence extraction.
use crate::architecture_snapshot::{
    self, ArchitectureSnapshot, Boundary, Component, Coverage, EvidenceRef, FactState, Freshness,
    Relationship, CONTRACT_VERSION, MAX_ITEMS,
};
use sha2::{Digest, Sha256};
use std::path::Path;

const MAX_FILE_BYTES: u64 = 256 * 1024;
const ALLOWED_FILES: &[(&str, &str, &str)] = &[
    ("Cargo.toml", "manifest", "rust-package"),
    ("package.json", "manifest", "electron-package"),
    (
        "crates/desktop-ipc/proto/evohime.desktop.proto",
        "ipc",
        "desktop-ipc",
    ),
    (
        "docs/architecture.md",
        "architecture",
        "project-architecture",
    ),
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("workspace unavailable: {0}")]
    Workspace(String),
    #[error("snapshot contract: {0}")]
    Contract(#[from] architecture_snapshot::Error),
}

fn hash_file(path: &Path) -> Result<(String, String), Error> {
    let metadata = std::fs::metadata(path).map_err(|e| Error::Workspace(e.to_string()))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(Error::Workspace("file_not_allowed".into()));
    }
    let bytes = std::fs::read(path).map_err(|e| Error::Workspace(e.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok((
        hex::encode(hasher.finalize()),
        format!("bytes:{}", bytes.len()),
    ))
}

fn evidence(root: &Path, relative: &str, kind: &str, revision: &str) -> Option<EvidenceRef> {
    let path = root.join(relative);
    let (hash, _) = hash_file(&path).ok()?;
    Some(EvidenceRef {
        file_ref: relative.to_string(),
        file_revision: revision.to_string(),
        content_hash: hash,
        evidence_kind: kind.to_string(),
        start_line: None,
        end_line: None,
        symbol: None,
    })
}

/// Reads only fixed manifest/config paths. It never invokes a repository file.
pub fn extract(
    root: &Path,
    workspace_identity: &str,
    source_revision: &str,
    snapshot_id: &str,
) -> Result<ArchitectureSnapshot, Error> {
    let root = root
        .canonicalize()
        .map_err(|e| Error::Workspace(e.to_string()))?;
    if !root.is_dir() {
        return Err(Error::Workspace("root_not_directory".into()));
    }
    let mut components = Vec::new();
    let mut covered = Vec::new();
    let mut diagnostics = Vec::new();
    for (relative, kind, component_kind) in ALLOWED_FILES {
        if let Some(ev) = evidence(&root, relative, kind, source_revision) {
            let id = format!("file:{relative}");
            components.push(Component {
                id,
                kind: (*component_kind).into(),
                name: relative.to_string(),
                responsibility: format!("evidence from {kind} manifest"),
                state: FactState::Verified,
                evidence: vec![ev],
            });
            covered.push((*component_kind).into());
        } else {
            diagnostics.push(crate::architecture_snapshot::CoverageDiagnostic {
                code: "unsupported_or_missing".into(),
                detail: (*relative).into(),
                state: FactState::Unsupported,
            });
        }
    }
    components.sort_by(|a, b| a.id.cmp(&b.id));
    components.truncate(MAX_ITEMS);
    let relationships = if components
        .iter()
        .any(|c| c.id == "file:crates/desktop-ipc/proto/evohime.desktop.proto")
        && components
            .iter()
            .any(|c| c.id == "file:crates/evohime-core/src/lib.rs")
    {
        vec![Relationship {
            id: "rel:core->desktop-ipc".into(),
            from: "file:crates/evohime-core/src/lib.rs".into(),
            to: "file:crates/desktop-ipc/proto/evohime.desktop.proto".into(),
            kind: "uses-contract".into(),
            state: FactState::Candidate,
            evidence: vec![],
        }]
    } else {
        Vec::new()
    };
    let snapshot = ArchitectureSnapshot {
        schema_version: CONTRACT_VERSION,
        snapshot_id: snapshot_id.into(),
        workspace_identity: workspace_identity.into(),
        source_revision: source_revision.into(),
        freshness: Freshness::Fresh,
        components,
        relationships,
        boundaries: vec![Boundary {
            id: "boundary:workspace".into(),
            name: "workspace root".into(),
            state: FactState::Reviewed,
            members: Vec::new(),
            evidence: Vec::new(),
        }],
        coverage: Coverage {
            extractor_version: "builtin-architecture-v1".into(),
            covered_kinds: covered,
            diagnostics,
            omissions: vec![
                crate::architecture_snapshot::Omission {
                    id: "dynamic-runtime-reachability".into(),
                    reason: "extractor does not execute runtime code".into(),
                    bound_revision: source_revision.into(),
                    state: FactState::Unsupported,
                },
                crate::architecture_snapshot::Omission {
                    id: "arbitrary-source-symbols".into(),
                    reason: "extractor uses an explicit file allowlist".into(),
                    bound_revision: source_revision.into(),
                    state: FactState::Unsupported,
                },
            ],
        },
    };
    architecture_snapshot::validate(&snapshot)?;
    Ok(snapshot)
}

pub fn source_fingerprint(root: &Path, source_revision: &str) -> String {
    architecture_snapshot::canonical_hash(&(root.to_string_lossy().to_string(), source_revision))
}

pub fn authorize_root(root: &Path, allowed_roots: &[String]) -> Result<(), Error> {
    let canonical = root
        .canonicalize()
        .map_err(|e| Error::Workspace(e.to_string()))?;
    if allowed_roots.is_empty() {
        return Err(Error::Workspace("root_grant_required".into()));
    }
    let allowed = allowed_roots.iter().any(|candidate| {
        std::path::Path::new(candidate)
            .canonicalize()
            .map(|path| path == canonical)
            .unwrap_or(false)
    });
    if allowed {
        Ok(())
    } else {
        Err(Error::Workspace("root_denied".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn allowlist_is_deterministic() {
        let root =
            std::env::temp_dir().join(format!("evohime-architecture-{}", std::process::id()));
        let _ = fs::create_dir_all(root.join("crates/desktop-ipc/proto"));
        fs::write(root.join("Cargo.toml"), "[package]\nname='x'\n").unwrap();
        let a = extract(&root, "w", "r", "s").unwrap();
        let b = extract(&root, "w", "r", "s").unwrap();
        assert_eq!(a, b);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn missing_inputs_are_diagnostic() {
        let root = tempfile::tempdir().unwrap();
        let s = extract(root.path(), "w", "r", "s").unwrap();
        assert!(!s.coverage.diagnostics.is_empty());
    }

    #[test]
    fn root_authorization_is_exact_and_no_substitution() {
        let root = tempfile::tempdir().unwrap();
        assert!(authorize_root(root.path(), &[root.path().display().to_string()]).is_ok());
        assert!(authorize_root(root.path(), &[]).is_err());
        let other = tempfile::tempdir().unwrap();
        assert!(authorize_root(root.path(), &[other.path().display().to_string()]).is_err());
    }
}
