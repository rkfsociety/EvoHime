use crate::{
    scope::{validate_build_scope, BuildScope, ProposedChange},
    workspace::{build_manifest, content_hash},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildChange {
    pub relative_path: String,
    pub content: Option<String>,
    pub expected_content_hash: Option<String>,
    pub delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub relative_path: String,
    pub existed: bool,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub relative_path: String,
    pub operation: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub bytes_changed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub id: String,
    pub baseline_workspace_hash: String,
    pub files: Vec<SnapshotFile>,
    pub diff: Vec<SnapshotDiff>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedBuild {
    pub intent_hash: String,
    pub effective_permissions_hash: String,
    pub expected_workspace_hash: String,
    pub scope: BuildScope,
    pub changes: Vec<BuildChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildProposal {
    pub scope: BuildScope,
    pub changes: Vec<BuildChange>,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("workspace operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("workspace baseline hash mismatch: expected {expected}, current {current}")]
    WorkspaceConflict { expected: String, current: String },
    #[error("approved intent hash mismatch")]
    IntentHashMismatch,
    #[error("build scope violation: {0}")]
    ScopeViolation(String),
    #[error("expected content hash mismatch for {path}: expected {expected}, current {current}")]
    ContentConflict {
        path: String,
        expected: String,
        current: String,
    },
    #[error("invalid workspace path {0}")]
    InvalidPath(String),
    #[error("bounded build timeout exceeded")]
    TimedOut,
}

pub fn prepare_build(
    root: impl AsRef<Path>,
    proposal: &BuildProposal,
) -> Result<ApprovedBuild, BuildError> {
    let root = root.as_ref().canonicalize()?;
    let manifest = build_manifest(&root, 500, 2 * 1024 * 1024)?;
    let mut changes = proposal.changes.clone();
    for change in &mut changes {
        let path = safe_path(&root, &change.relative_path)?;
        if change.expected_content_hash.is_none() && path.exists() {
            change.expected_content_hash = Some(content_hash(&fs::read(path)?));
        }
    }
    let proposed = changes
        .iter()
        .map(|change| ProposedChange {
            relative_path: change.relative_path.clone(),
            operation: if change.delete {
                "delete".into()
            } else if change.content.is_some() && !root.join(&change.relative_path).exists() {
                "create".into()
            } else {
                "write".into()
            },
            bytes_changed: change.content.as_deref().map(str::len).unwrap_or(0),
            creates: change.content.is_some() && !root.join(&change.relative_path).exists(),
            deletes: change.delete,
        })
        .collect::<Vec<_>>();
    if let Some(violation) = validate_build_scope(&proposal.scope, &proposed).first() {
        return Err(BuildError::ScopeViolation(format!(
            "{}: {}",
            violation.path, violation.reason
        )));
    }
    let mut approved = ApprovedBuild {
        intent_hash: String::new(),
        effective_permissions_hash: calculate_effective_permissions_hash(&proposal.scope),
        expected_workspace_hash: manifest.workspace_hash,
        scope: proposal.scope.clone(),
        changes,
    };
    approved.intent_hash = calculate_intent_hash(&approved.scope, &approved.changes);
    Ok(approved)
}

pub fn calculate_intent_hash(scope: &BuildScope, changes: &[BuildChange]) -> String {
    let effective_permissions_hash = calculate_effective_permissions_hash(scope);
    let canonical = serde_json::to_vec(&(scope, changes, effective_permissions_hash))
        .expect("build intent serializes");
    content_hash(&canonical)
}

pub fn calculate_effective_permissions_hash(scope: &BuildScope) -> String {
    let canonical = serde_json::to_vec(scope).expect("build scope serializes");
    content_hash(&canonical)
}

pub fn apply_approved_build(
    root: impl AsRef<Path>,
    build: &ApprovedBuild,
) -> Result<WorkspaceSnapshot, BuildError> {
    if calculate_effective_permissions_hash(&build.scope) != build.effective_permissions_hash
        || calculate_intent_hash(&build.scope, &build.changes) != build.intent_hash
    {
        return Err(BuildError::IntentHashMismatch);
    }
    let root = root.as_ref().canonicalize()?;
    let manifest = build_manifest(&root, 500, 2 * 1024 * 1024)?;
    if manifest.workspace_hash != build.expected_workspace_hash {
        return Err(BuildError::WorkspaceConflict {
            expected: build.expected_workspace_hash.clone(),
            current: manifest.workspace_hash,
        });
    }
    let proposed = build
        .changes
        .iter()
        .map(|change| ProposedChange {
            relative_path: change.relative_path.clone(),
            operation: if change.delete {
                "delete".into()
            } else if change.content.is_some() && !root.join(&change.relative_path).exists() {
                "create".into()
            } else {
                "write".into()
            },
            bytes_changed: change.content.as_deref().map(str::len).unwrap_or(0),
            creates: change.content.is_some() && !root.join(&change.relative_path).exists(),
            deletes: change.delete,
        })
        .collect::<Vec<_>>();
    let violations = validate_build_scope(&build.scope, &proposed);
    if let Some(violation) = violations.first() {
        return Err(BuildError::ScopeViolation(format!(
            "{}: {}",
            violation.path, violation.reason
        )));
    }

    let mut snapshot = WorkspaceSnapshot {
        id: format!("snapshot-{}", content_hash(build.intent_hash.as_bytes())),
        baseline_workspace_hash: build.expected_workspace_hash.clone(),
        files: Vec::new(),
        diff: Vec::new(),
    };
    for change in &build.changes {
        let path = safe_path(&root, &change.relative_path)?;
        let existing = path.exists().then(|| fs::read(&path)).transpose()?;
        if let Some(expected) = &change.expected_content_hash {
            let current = existing.as_deref().map(content_hash).unwrap_or_default();
            if current != *expected {
                return Err(BuildError::ContentConflict {
                    path: change.relative_path.clone(),
                    expected: expected.clone(),
                    current,
                });
            }
        }
        snapshot.files.push(SnapshotFile {
            relative_path: change.relative_path.clone(),
            existed: existing.is_some(),
            content: existing.unwrap_or_default(),
        });
        let before_hash = snapshot
            .files
            .last()
            .filter(|file| file.existed)
            .map(|file| content_hash(&file.content));
        let operation = if change.delete {
            "delete"
        } else if before_hash.is_none() {
            "create"
        } else {
            "write"
        };
        let after_hash = if change.delete {
            None
        } else {
            change
                .content
                .as_deref()
                .map(|content| content_hash(content.as_bytes()))
        };
        snapshot.diff.push(SnapshotDiff {
            relative_path: change.relative_path.clone(),
            operation: operation.into(),
            before_hash,
            after_hash,
            bytes_changed: change.content.as_deref().map(str::len).unwrap_or(0),
        });
    }

    let deadline = Instant::now() + Duration::from_millis(build.scope.timeout_ms);
    if let Err(error) = apply_changes(&root, build, deadline) {
        let _ = restore_snapshot(&root, &snapshot);
        return Err(error);
    }
    Ok(snapshot)
}

pub fn restore_snapshot(
    root: impl AsRef<Path>,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), BuildError> {
    let root = root.as_ref().canonicalize()?;
    for file in &snapshot.files {
        let path = safe_path(&root, &file.relative_path)?;
        if file.existed {
            fs::write(path, &file.content)?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn apply_changes(root: &Path, build: &ApprovedBuild, deadline: Instant) -> Result<(), BuildError> {
    for change in &build.changes {
        if Instant::now() >= deadline {
            return Err(BuildError::TimedOut);
        }
        let path = safe_path(root, &change.relative_path)?;
        if change.delete {
            if path.exists() {
                fs::remove_file(path)?;
            }
        } else if let Some(content) = &change.content {
            if path.parent().is_none_or(|parent| !parent.exists()) {
                return Err(BuildError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "parent directory does not exist",
                )));
            }
            fs::write(path, content)?;
        }
        if Instant::now() >= deadline {
            return Err(BuildError::TimedOut);
        }
    }
    Ok(())
}

fn safe_path(root: &Path, relative_path: &str) -> Result<PathBuf, BuildError> {
    let normalized = relative_path.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(BuildError::InvalidPath(relative_path.into()));
    }
    Ok(root.join(path))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_approved_build, calculate_effective_permissions_hash, calculate_intent_hash,
        prepare_build, restore_snapshot, ApprovedBuild, BuildChange, BuildProposal,
    };
    use crate::{scope::BuildScope, workspace::build_manifest};
    use std::{fs, path::PathBuf};

    fn root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("evohime-build-{name}-{}", std::process::id()))
    }

    fn scope() -> BuildScope {
        BuildScope {
            allowed_paths: vec!["src".into()],
            allowed_operations: vec!["write".into()],
            expected_outputs: vec!["updated source".into()],
            protected_paths: vec![],
            allowed_file_types: vec!["rs".into()],
            max_files_changed: 1,
            max_bytes_changed: 100,
            allow_create: false,
            allow_delete: false,
            allow_rename: false,
            baseline_snapshot_id: None,
            acceptance_criteria: "tests pass".into(),
            risk_class: "medium".into(),
            timeout_ms: 30_000,
        }
    }

    #[test]
    fn applies_only_approved_expected_baseline_and_restores_snapshot() {
        let root = root("apply");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "old").unwrap();
        let baseline = build_manifest(&root, 500, 2 * 1024 * 1024).unwrap();
        let change = BuildChange {
            relative_path: "src/lib.rs".into(),
            content: Some("new".into()),
            expected_content_hash: Some(crate::workspace::content_hash(b"old")),
            delete: false,
        };
        let mut build = ApprovedBuild {
            intent_hash: String::new(),
            effective_permissions_hash: String::new(),
            expected_workspace_hash: baseline.workspace_hash,
            scope: scope(),
            changes: vec![change],
        };
        build.effective_permissions_hash = calculate_effective_permissions_hash(&build.scope);
        build.intent_hash = calculate_intent_hash(&build.scope, &build.changes);
        let snapshot = apply_approved_build(&root, &build).unwrap();
        assert_eq!(fs::read_to_string(root.join("src/lib.rs")).unwrap(), "new");
        assert_eq!(snapshot.diff.len(), 1);
        assert_eq!(snapshot.diff[0].operation, "write");
        assert_eq!(
            snapshot.diff[0].before_hash.as_deref(),
            Some(crate::workspace::content_hash(b"old").as_str())
        );
        assert_eq!(
            snapshot.diff[0].after_hash.as_deref(),
            Some(crate::workspace::content_hash(b"new").as_str())
        );
        restore_snapshot(&root, &snapshot).unwrap();
        assert_eq!(fs::read_to_string(root.join("src/lib.rs")).unwrap(), "old");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_tampered_intent_and_baseline_conflict() {
        let root = root("conflict");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "old").unwrap();
        let mut build = ApprovedBuild {
            intent_hash: "tampered".into(),
            effective_permissions_hash: String::new(),
            expected_workspace_hash: "wrong".into(),
            scope: scope(),
            changes: vec![],
        };
        assert!(apply_approved_build(&root, &build).is_err());
        build.effective_permissions_hash = calculate_effective_permissions_hash(&build.scope);
        build.intent_hash = calculate_intent_hash(&build.scope, &build.changes);
        assert!(apply_approved_build(&root, &build).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepare_build_does_not_write_and_binds_current_content_hash() {
        let root = root("prepare");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "old").unwrap();
        let proposal = BuildProposal {
            scope: scope(),
            changes: vec![BuildChange {
                relative_path: "src/lib.rs".into(),
                content: Some("new".into()),
                expected_content_hash: None,
                delete: false,
            }],
        };
        let approved = prepare_build(&root, &proposal).unwrap();
        assert_eq!(fs::read_to_string(root.join("src/lib.rs")).unwrap(), "old");
        assert_eq!(
            approved.changes[0].expected_content_hash,
            Some(crate::workspace::content_hash(b"old"))
        );
        fs::remove_dir_all(root).unwrap();
    }
}
