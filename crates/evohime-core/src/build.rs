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
    /// Present when this change renames a tracked file: the workspace-relative
    /// source path. When set, `relative_path` is the destination, `content` is
    /// the (optionally edited) content written to the destination, and the
    /// operation is gated by `BuildScope::allow_rename` rather than
    /// `allow_create`/`allow_delete`, even though a rename is implemented as a
    /// delete of `rename_from` plus a create of `relative_path`.
    #[serde(default)]
    pub rename_from: Option<String>,
}

/// Rollback restores only the workspace files captured in a
/// [`WorkspaceSnapshot`]. It never touches SQLite/durable state (runs,
/// checkpoints, effects, audit log) and never undoes external side effects
/// (network calls, spawned processes, anything outside the workspace root)
/// performed while the build ran. Any UI surface presenting rollback MUST
/// render this scope explicitly — never as a bare boolean — so a user cannot
/// conclude that rollback undid "everything the run did".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackScope {
    WorkspaceFilesOnly,
}

impl RollbackScope {
    pub fn label(self) -> &'static str {
        match self {
            RollbackScope::WorkspaceFilesOnly => "workspace_files_only",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            RollbackScope::WorkspaceFilesOnly => {
                "Restores workspace files captured in this snapshot only. \
                 Does not restore SQLite/durable state and does not undo \
                 external effects (network calls, processes, anything \
                 outside the workspace)."
            }
        }
    }
}

impl Default for RollbackScope {
    fn default() -> Self {
        RollbackScope::WorkspaceFilesOnly
    }
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
    /// The run this snapshot belongs to. A snapshot is only ever
    /// meaningful in the context of the run that produced it.
    #[serde(default)]
    pub run_id: String,
    pub baseline_workspace_hash: String,
    pub files: Vec<SnapshotFile>,
    pub diff: Vec<SnapshotDiff>,
    #[serde(default)]
    pub created_at_ms: u64,
    /// Never a bare boolean — see [`RollbackScope`]. Rollback of this
    /// snapshot restores workspace files only; it does not touch SQLite
    /// state and does not undo external effects.
    #[serde(default)]
    pub rollback_scope: RollbackScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovedBuild {
    pub intent_hash: String,
    pub effective_permissions_hash: String,
    pub expected_workspace_hash: String,
    pub scope: BuildScope,
    pub changes: Vec<BuildChange>,
    /// Read-only diff preview computed at `prepare_build` time from the
    /// current on-disk content, so an approval surface can render diff +
    /// files + budget/risk/timeout + `intent_hash` before anything is
    /// written. This field is NOT part of `intent_hash`/
    /// `effective_permissions_hash` (those bind `scope` + `changes` only),
    /// so it is purely informational: the authoritative diff is the one
    /// `apply_approved_build` recomputes against the workspace at apply
    /// time, re-checked against `expected_content_hash` per change.
    #[serde(default)]
    pub preview_diff: Vec<SnapshotDiff>,
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
        .map(|change| to_proposed_change(&root, change))
        .collect::<Vec<_>>();
    if let Some(violation) = validate_build_scope(&proposal.scope, &proposed).first() {
        return Err(BuildError::ScopeViolation(format!(
            "{}: {}",
            violation.path, violation.reason
        )));
    }
    let preview_diff = preview_diff(&root, &changes)?;
    let mut approved = ApprovedBuild {
        intent_hash: String::new(),
        effective_permissions_hash: calculate_effective_permissions_hash(&proposal.scope),
        expected_workspace_hash: manifest.workspace_hash,
        scope: proposal.scope.clone(),
        changes,
        preview_diff,
    };
    approved.intent_hash = calculate_intent_hash(&approved.scope, &approved.changes);
    Ok(approved)
}

/// Classifies a [`BuildChange`] into the operation vocabulary enforced by
/// [`validate_build_scope`]. Renames (`rename_from` set) are reported as
/// `"rename"` so they are gated by `allow_rename`, even though they are
/// physically applied as a delete of the source plus a create/write of the
/// destination.
fn to_proposed_change(root: &Path, change: &BuildChange) -> ProposedChange {
    let destination_exists = root.join(&change.relative_path).exists();
    let operation = if change.rename_from.is_some() {
        "rename"
    } else if change.delete {
        "delete"
    } else if change.content.is_some() && !destination_exists {
        "create"
    } else {
        "write"
    };
    ProposedChange {
        relative_path: change.relative_path.clone(),
        operation: operation.into(),
        bytes_changed: change.content.as_deref().map(str::len).unwrap_or(0),
        creates: operation == "create",
        deletes: change.delete,
    }
}

/// Computes a read-only diff preview against the current on-disk content,
/// without writing anything. Used by `prepare_build` so an approval surface
/// can show diff + files before the user approves. This does NOT enforce
/// `expected_content_hash` — that conflict check only happens, authoritatively,
/// in `apply_approved_build` at apply time.
fn preview_diff(root: &Path, changes: &[BuildChange]) -> Result<Vec<SnapshotDiff>, BuildError> {
    let mut diff = Vec::with_capacity(changes.len());
    for change in changes {
        let before_hash = if let Some(rename_from) = &change.rename_from {
            let source_path = safe_path(root, rename_from)?;
            source_path
                .exists()
                .then(|| fs::read(&source_path))
                .transpose()?
                .map(|content| content_hash(&content))
        } else {
            let path = safe_path(root, &change.relative_path)?;
            path.exists()
                .then(|| fs::read(&path))
                .transpose()?
                .map(|content| content_hash(&content))
        };
        let operation = if change.rename_from.is_some() {
            "rename"
        } else if change.delete {
            "delete"
        } else if before_hash.is_none() {
            "create"
        } else {
            "write"
        };
        let after_hash = if change.delete {
            None
        } else if let Some(content) = &change.content {
            Some(content_hash(content.as_bytes()))
        } else {
            before_hash.clone()
        };
        diff.push(SnapshotDiff {
            relative_path: change.relative_path.clone(),
            operation: operation.into(),
            before_hash,
            after_hash,
            bytes_changed: change.content.as_deref().map(str::len).unwrap_or(0),
        });
    }
    Ok(diff)
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
    run_id: &str,
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
        .map(|change| to_proposed_change(&root, change))
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
        run_id: run_id.to_string(),
        baseline_workspace_hash: build.expected_workspace_hash.clone(),
        files: Vec::new(),
        diff: Vec::new(),
        created_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or_default(),
        rollback_scope: RollbackScope::WorkspaceFilesOnly,
    };
    for change in &build.changes {
        // For a rename, capture the *source* file's pre-state too so
        // restore_snapshot can recreate it — the source is never touched by
        // the `relative_path`/`expected_content_hash` handling below, which
        // only concerns the destination.
        if let Some(rename_from) = &change.rename_from {
            let source_path = safe_path(&root, rename_from)?;
            let source_existing = source_path
                .exists()
                .then(|| fs::read(&source_path))
                .transpose()?;
            snapshot.files.push(SnapshotFile {
                relative_path: rename_from.clone(),
                existed: source_existing.is_some(),
                content: source_existing.unwrap_or_default(),
            });
        }
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
        let operation = if change.rename_from.is_some() {
            "rename"
        } else if change.delete {
            "delete"
        } else if before_hash.is_none() {
            "create"
        } else {
            "write"
        };
        let after_hash = if change.delete {
            None
        } else if let Some(content) = &change.content {
            Some(content_hash(content.as_bytes()))
        } else if change.rename_from.is_some() {
            // Rename without an edit: destination content equals whatever
            // the source held.
            snapshot
                .files
                .iter()
                .rev()
                .find(|file| Some(&file.relative_path) == change.rename_from.as_ref())
                .map(|file| content_hash(&file.content))
        } else {
            None
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
        if let Some(rename_from) = &change.rename_from {
            let source_path = safe_path(root, rename_from)?;
            if path.parent().is_none_or(|parent| !parent.exists()) {
                return Err(BuildError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "parent directory does not exist",
                )));
            }
            let content = match &change.content {
                Some(content) => content.clone(),
                None => String::from_utf8(fs::read(&source_path)?).map_err(|error| {
                    BuildError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        error.utf8_error(),
                    ))
                })?,
            };
            fs::write(&path, content)?;
            if source_path.exists() {
                fs::remove_file(source_path)?;
            }
        } else if change.delete {
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

/// Resolves a workspace-relative path and rejects anything that could write
/// or read outside the workspace: `..`/absolute components, and existing
/// symlinks or Windows reparse points at any point on the path. Writes are
/// never routed through a symlink, even one that happens to resolve back
/// inside the workspace root — the manifest never claims to know what such a
/// link ultimately points at, so it is rejected rather than silently
/// followed.
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
    let resolved = root.join(path);
    let mut probe = root.to_path_buf();
    for component in path.components() {
        probe.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&probe) {
            if metadata.file_type().is_symlink() {
                return Err(BuildError::InvalidPath(relative_path.into()));
            }
        }
    }
    Ok(resolved)
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
            rename_from: None,
        };
        let mut build = ApprovedBuild {
            intent_hash: String::new(),
            effective_permissions_hash: String::new(),
            expected_workspace_hash: baseline.workspace_hash,
            scope: scope(),
            changes: vec![change],
            preview_diff: Vec::new(),
        };
        build.effective_permissions_hash = calculate_effective_permissions_hash(&build.scope);
        build.intent_hash = calculate_intent_hash(&build.scope, &build.changes);
        let snapshot = apply_approved_build(&root, "run-apply", &build).unwrap();
        assert_eq!(snapshot.run_id, "run-apply");
        assert_eq!(snapshot.rollback_scope, super::RollbackScope::WorkspaceFilesOnly);
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
            preview_diff: Vec::new(),
        };
        assert!(apply_approved_build(&root, "run-conflict", &build).is_err());
        build.effective_permissions_hash = calculate_effective_permissions_hash(&build.scope);
        build.intent_hash = calculate_intent_hash(&build.scope, &build.changes);
        assert!(apply_approved_build(&root, "run-conflict", &build).is_err());
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
                rename_from: None,
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

    #[test]
    fn rename_requires_allow_rename_and_restores_source_on_rollback() {
        let root = root("rename");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/old_name.rs"), "content").unwrap();
        let baseline = build_manifest(&root, 500, 2 * 1024 * 1024).unwrap();
        let change = BuildChange {
            relative_path: "src/new_name.rs".into(),
            content: None,
            expected_content_hash: None,
            delete: false,
            rename_from: Some("src/old_name.rs".into()),
        };
        let mut denied_scope = scope();
        denied_scope.allowed_operations = vec!["write".into(), "rename".into()];
        let mut build = ApprovedBuild {
            intent_hash: String::new(),
            effective_permissions_hash: String::new(),
            expected_workspace_hash: baseline.workspace_hash.clone(),
            scope: denied_scope,
            changes: vec![change.clone()],
            preview_diff: Vec::new(),
        };
        build.effective_permissions_hash = calculate_effective_permissions_hash(&build.scope);
        build.intent_hash = calculate_intent_hash(&build.scope, &build.changes);
        assert!(matches!(
            apply_approved_build(&root, "run-rename", &build),
            Err(super::BuildError::ScopeViolation(reason)) if reason.contains("rename")
        ));

        let mut allowed_scope = scope();
        allowed_scope.allowed_operations = vec!["write".into(), "rename".into()];
        allowed_scope.allow_rename = true;
        build.scope = allowed_scope;
        build.effective_permissions_hash = calculate_effective_permissions_hash(&build.scope);
        build.intent_hash = calculate_intent_hash(&build.scope, &build.changes);
        let snapshot = apply_approved_build(&root, "run-rename", &build).unwrap();
        assert!(!root.join("src/old_name.rs").exists());
        assert_eq!(
            fs::read_to_string(root.join("src/new_name.rs")).unwrap(),
            "content"
        );
        restore_snapshot(&root, &snapshot).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("src/old_name.rs")).unwrap(),
            "content"
        );
        assert!(!root.join("src/new_name.rs").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
