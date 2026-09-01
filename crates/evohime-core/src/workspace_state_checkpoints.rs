//! Workspace State Checkpoints (plan 58).
//!
//! This contract is deliberately independent from `TaskCheckpointV1`: it
//! describes only file state and never mutates task history or external effects.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_FILES: usize = 4_096;
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub path: String,
    pub hash: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceStateCheckpoint {
    pub version: u32,
    pub checkpoint_id: String,
    pub workspace_id: String,
    pub task_id: Option<String>,
    pub baseline_hash: String,
    pub files: Vec<FileState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub expected: Option<String>,
    pub observed: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("unsupported workspace checkpoint version {0}")]
    UnsupportedVersion(u32),
    #[error("workspace checkpoint path is invalid: {0}")]
    InvalidPath(String),
    #[error("workspace checkpoint contains a symlink or reparse entry: {0}")]
    ReparseEntry(String),
    #[error("workspace checkpoint exceeds a bounded limit: {0}")]
    LimitExceeded(&'static str),
    #[error("workspace checkpoint has a content hash mismatch")]
    HashMismatch,
    #[error("workspace has external changes")]
    Conflicts(Vec<Conflict>),
    #[error("workspace checkpoint I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn capture(
    root: impl AsRef<Path>,
    checkpoint_id: impl Into<String>,
    workspace_id: impl Into<String>,
    task_id: Option<String>,
) -> Result<WorkspaceStateCheckpoint, CheckpointError> {
    let root = root.as_ref().canonicalize()?;
    let mut files = Vec::new();
    let mut total = 0usize;
    collect_files(&root, &root, &mut files, &mut total)?;
    files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let baseline_hash = digest_files(&files);
    Ok(WorkspaceStateCheckpoint {
        version: CONTRACT_VERSION,
        checkpoint_id: checkpoint_id.into(),
        workspace_id: workspace_id.into(),
        task_id,
        baseline_hash,
        files,
    })
}

pub fn compare(
    root: impl AsRef<Path>,
    checkpoint: &WorkspaceStateCheckpoint,
) -> Result<Vec<Conflict>, CheckpointError> {
    validate(checkpoint)?;
    let current = capture(
        root,
        "compare",
        checkpoint.workspace_id.clone(),
        checkpoint.task_id.clone(),
    )?;
    let mut conflicts = Vec::new();
    let expected = checkpoint
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hash.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let observed = current
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.hash.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    for path in expected.keys().chain(observed.keys()) {
        if expected.get(path) != observed.get(path) {
            conflicts.push(Conflict {
                path: (*path).to_owned(),
                expected: expected.get(path).map(|v| (*v).to_owned()),
                observed: observed.get(path).map(|v| (*v).to_owned()),
            });
        }
    }
    conflicts.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    Ok(conflicts)
}

/// Restores only when the caller's preflight observed no change. There is no
/// force mode: a conflict is always surfaced before the first write.
pub fn restore(
    root: impl AsRef<Path>,
    checkpoint: &WorkspaceStateCheckpoint,
) -> Result<(), CheckpointError> {
    let conflicts = compare(&root, checkpoint)?;
    if !conflicts.is_empty() {
        return Err(CheckpointError::Conflicts(conflicts));
    }
    let root = root.as_ref().canonicalize()?;
    let wanted = checkpoint
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<std::collections::HashSet<_>>();
    let current = capture(
        &root,
        "restore",
        checkpoint.workspace_id.clone(),
        checkpoint.task_id.clone(),
    )?;
    for file in current
        .files
        .iter()
        .filter(|f| !wanted.contains(f.path.as_str()))
    {
        fs::remove_file(safe_path(&root, &file.path)?)?;
    }
    for file in &checkpoint.files {
        let path = safe_path(&root, &file.path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &file.bytes)?;
    }
    Ok(())
}

/// Compatibility adapter for the existing build snapshot store. It keeps the
/// old task/run ownership and audit records, but applies the plan-58 preflight:
/// only files captured by the snapshot are compared, and no changed file is
/// overwritten silently.
pub fn restore_build_snapshot_safe(
    root: impl AsRef<Path>,
    snapshot: &crate::build::WorkspaceSnapshot,
) -> Result<(), CheckpointError> {
    let root = root.as_ref().canonicalize()?;
    let mut conflicts = Vec::new();
    for file in &snapshot.files {
        let path = safe_path(&root, &file.relative_path)?;
        let observed = path.exists().then(|| fs::read(&path)).transpose()?;
        let expected = file.existed.then(|| hash(&file.content));
        let actual = observed.as_deref().map(hash);
        if expected != actual {
            conflicts.push(Conflict {
                path: file.relative_path.clone(),
                expected,
                observed: actual,
            });
        }
    }
    if !conflicts.is_empty() {
        return Err(CheckpointError::Conflicts(conflicts));
    }
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

pub fn validate(checkpoint: &WorkspaceStateCheckpoint) -> Result<(), CheckpointError> {
    if checkpoint.version != CONTRACT_VERSION {
        return Err(CheckpointError::UnsupportedVersion(checkpoint.version));
    }
    if checkpoint.files.len() > MAX_FILES {
        return Err(CheckpointError::LimitExceeded("files"));
    }
    let mut total = 0usize;
    for file in &checkpoint.files {
        validate_relative(&file.path)?;
        if file.bytes.len() > MAX_FILE_BYTES {
            return Err(CheckpointError::LimitExceeded("file_bytes"));
        }
        total = total
            .checked_add(file.bytes.len())
            .ok_or(CheckpointError::LimitExceeded("snapshot_bytes"))?;
        if file.hash != hash(&file.bytes) {
            return Err(CheckpointError::HashMismatch);
        }
    }
    if total > MAX_SNAPSHOT_BYTES {
        return Err(CheckpointError::LimitExceeded("snapshot_bytes"));
    }
    if digest_files(&checkpoint.files) != checkpoint.baseline_hash {
        return Err(CheckpointError::HashMismatch);
    }
    Ok(())
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<FileState>,
    total: &mut usize,
) -> Result<(), CheckpointError> {
    if files.len() > MAX_FILES {
        return Err(CheckpointError::LimitExceeded("files"));
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| CheckpointError::InvalidPath(path.display().to_string()))?;
        if excluded(relative) {
            continue;
        }
        if metadata.file_type().is_symlink() {
            return Err(CheckpointError::ReparseEntry(
                relative.display().to_string(),
            ));
        }
        if metadata.is_dir() {
            collect_files(root, &path, files, total)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        if metadata.len() as usize > MAX_FILE_BYTES {
            return Err(CheckpointError::LimitExceeded("file_bytes"));
        }
        let bytes = fs::read(&path)?;
        *total = total
            .checked_add(bytes.len())
            .ok_or(CheckpointError::LimitExceeded("snapshot_bytes"))?;
        if *total > MAX_SNAPSHOT_BYTES {
            return Err(CheckpointError::LimitExceeded("snapshot_bytes"));
        }
        files.push(FileState {
            path: relative.to_string_lossy().replace('\\', "/"),
            hash: hash(&bytes),
            bytes,
        });
    }
    Ok(())
}

fn excluded(path: &Path) -> bool {
    path.components().any(|c| matches!(c, std::path::Component::Normal(v) if matches!(v.to_string_lossy().as_ref(), ".git"|".hg"|".svn"|"node_modules"|"target"|"bin"|"obj"|"dist"|"build"|".venv"|"vendor")))
}
fn validate_relative(path: &str) -> Result<(), CheckpointError> {
    let p = Path::new(path);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(CheckpointError::InvalidPath(path.into()));
    }
    if path.len() > 512 || p.components().count() > 128 {
        return Err(CheckpointError::LimitExceeded("path"));
    }
    Ok(())
}
fn safe_path(root: &Path, relative: &str) -> Result<PathBuf, CheckpointError> {
    validate_relative(relative)?;
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        if parent.exists() && parent.canonicalize()?.strip_prefix(root).is_err() {
            return Err(CheckpointError::InvalidPath(relative.into()));
        }
    }
    Ok(path)
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn digest_files(files: &[FileState]) -> String {
    let mut h = Sha256::new();
    for f in files {
        h.update(f.path.as_bytes());
        h.update([0]);
        h.update(f.hash.as_bytes());
        h.update([0]);
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("evohime-cp-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn capture_compare_and_restore_are_conflict_safe() {
        let p = temp();
        fs::write(p.join("a.txt"), b"one").unwrap();
        let cp = capture(&p, "cp", "ws", None).unwrap();
        assert!(compare(&p, &cp).unwrap().is_empty());
        fs::write(p.join("a.txt"), b"user").unwrap();
        assert!(matches!(
            restore(&p, &cp),
            Err(CheckpointError::Conflicts(_))
        ));
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn excludes_build_and_vcs_entries() {
        let p = temp();
        fs::create_dir_all(p.join("target")).unwrap();
        fs::write(p.join("target/cache"), b"x").unwrap();
        fs::create_dir_all(p.join(".git")).unwrap();
        fs::write(p.join(".git/config"), b"x").unwrap();
        fs::write(p.join("kept"), b"x").unwrap();
        let cp = capture(&p, "cp", "ws", None).unwrap();
        assert_eq!(cp.files.len(), 1);
        fs::remove_dir_all(p).unwrap();
    }

    #[test]
    fn build_snapshot_adapter_rejects_a_user_edit_before_writing() {
        let p = temp();
        fs::write(p.join("a.txt"), b"before").unwrap();
        let snapshot = crate::build::WorkspaceSnapshot {
            id: "s".into(),
            run_id: "r".into(),
            baseline_workspace_hash: "h".into(),
            files: vec![crate::build::SnapshotFile {
                relative_path: "a.txt".into(),
                existed: true,
                content: b"before".to_vec(),
            }],
            diff: vec![],
            created_at_ms: 1,
            rollback_scope: crate::build::RollbackScope::WorkspaceFilesOnly,
        };
        fs::write(p.join("a.txt"), b"user-edit").unwrap();
        assert!(matches!(
            restore_build_snapshot_safe(&p, &snapshot),
            Err(CheckpointError::Conflicts(_))
        ));
        fs::remove_dir_all(p).unwrap();
    }
}
