//! Shared revision-safe file boundary for all mediated filesystem tools.

use crate::{ToolContext, ToolError};
use evohime_permissions::Permission;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;

pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Namespace {
    Uploads,
    Workspace,
    Outputs,
    Scratch,
}

impl Namespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uploads => "uploads",
            Self::Workspace => "workspace",
            Self::Outputs => "outputs",
            Self::Scratch => "scratch",
        }
    }
    fn root(self, workspace: &Path, task_id: uuid::Uuid) -> PathBuf {
        match self {
            Self::Workspace => workspace.to_path_buf(),
            Self::Uploads => workspace.join(".evohime").join("uploads"),
            Self::Outputs => workspace.join(".evohime").join("outputs"),
            Self::Scratch => workspace
                .join(".evohime")
                .join("scratch")
                .join(task_id.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRef {
    pub namespace: Namespace,
    pub path: String,
    pub content_hash: String,
    pub revision: u64,
    pub bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum RevisionError {
    #[error("invalid revision-safe workspace path")]
    InvalidPath,
    #[error("revision-safe workspace path escapes its namespace")]
    Escape,
    #[error("file exceeds the bounded size limit")]
    TooLarge,
    #[error("stale file revision: expected {expected}, observed {observed}")]
    Stale { expected: String, observed: String },
    #[error("file revision precondition is required for an existing file")]
    MissingPrecondition,
    #[error("uploads are immutable")]
    Immutable,
    #[error("file operation failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn parse_logical_path(value: &str) -> Result<(Namespace, String), RevisionError> {
    if value.is_empty() || value.contains('\0') || Path::new(value).is_absolute() {
        return Err(RevisionError::InvalidPath);
    }
    let value = value.replace('\\', "/");
    let mut parts = value.splitn(2, '/');
    let first = parts.next().unwrap_or_default();
    let Some(rest) = parts.next() else {
        return Ok((Namespace::Workspace, value));
    };
    let namespace = match first {
        "uploads" => Namespace::Uploads,
        "workspace" => Namespace::Workspace,
        "outputs" => Namespace::Outputs,
        "scratch" => Namespace::Scratch,
        _ => return Ok((Namespace::Workspace, value)),
    };
    if rest.is_empty() {
        return Err(RevisionError::InvalidPath);
    }
    Ok((namespace, rest.to_owned()))
}

fn resolve(
    ctx: &ToolContext,
    logical: &str,
    write: bool,
) -> Result<(Namespace, String, PathBuf), RevisionError> {
    let (namespace, relative) = parse_logical_path(logical)?;
    let relative_path = Path::new(&relative);
    if relative_path.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err(RevisionError::Escape);
    }
    let root = namespace.root(&ctx.workspace_root, ctx.task_id);
    if write {
        std::fs::create_dir_all(&root).map_err(RevisionError::Io)?;
    }
    let candidate = root.join(relative_path);
    let parent = candidate.parent().ok_or(RevisionError::InvalidPath)?;
    let existing_parent = if write {
        let mut current = parent;
        while !current.exists() {
            current = current.parent().ok_or(RevisionError::Escape)?;
        }
        current
    } else {
        parent
    };
    let canonical_parent = existing_parent.canonicalize()?;
    let canonical_root = root.canonicalize().unwrap_or(root.clone());
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(RevisionError::Escape);
    }
    let resolved = if write {
        let suffix = parent
            .strip_prefix(existing_parent)
            .map_err(|_| RevisionError::Escape)?;
        canonical_parent
            .join(suffix)
            .join(candidate.file_name().ok_or(RevisionError::InvalidPath)?)
    } else {
        candidate.canonicalize()?
    };
    if !resolved.starts_with(&canonical_root) {
        return Err(RevisionError::Escape);
    }
    Ok((namespace, relative.to_owned(), resolved))
}

pub fn resolve_logical(
    ctx: &ToolContext,
    logical: &str,
    write: bool,
) -> Result<(Namespace, String, PathBuf), RevisionError> {
    resolve(ctx, logical, write)
}

fn make_ref(namespace: Namespace, path: String, bytes: &[u8]) -> FileRef {
    let digest = Sha256::digest(bytes);
    let mut revision_bytes = [0_u8; 8];
    revision_bytes.copy_from_slice(&digest[..8]);
    FileRef {
        namespace,
        path,
        content_hash: hex::encode(digest),
        revision: u64::from_be_bytes(revision_bytes),
        bytes: bytes.len(),
    }
}

pub async fn read(ctx: &ToolContext, logical: &str) -> Result<(FileRef, String), RevisionError> {
    let (namespace, path, resolved) = resolve(ctx, logical, false)?;
    let bytes = fs::read(resolved).await?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(RevisionError::TooLarge);
    }
    let text = String::from_utf8(bytes.clone()).map_err(|_| RevisionError::InvalidPath)?;
    Ok((make_ref(namespace, path, &bytes), text))
}

pub async fn write(
    ctx: &ToolContext,
    logical: &str,
    content: &[u8],
    expected_hash: Option<&str>,
) -> Result<FileRef, RevisionError> {
    if content.len() > MAX_FILE_BYTES {
        return Err(RevisionError::TooLarge);
    }
    let (namespace, path, resolved) = resolve(ctx, logical, true)?;
    if fs::try_exists(&resolved).await? {
        if namespace == Namespace::Uploads {
            return Err(RevisionError::Immutable);
        }
        let current = fs::read(&resolved).await?;
        let observed = hex::encode(Sha256::digest(&current));
        let expected = expected_hash.ok_or(RevisionError::MissingPrecondition)?;
        if expected != observed {
            return Err(RevisionError::Stale {
                expected: expected.into(),
                observed,
            });
        }
    }
    if let Some(parent) = resolved.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::write(resolved, content).await?;
    Ok(make_ref(namespace, path, content))
}

pub async fn assert_precondition(
    ctx: &ToolContext,
    logical: &str,
    expected_hash: &str,
) -> Result<(), RevisionError> {
    let (file_ref, _) = read(ctx, logical).await?;
    if file_ref.content_hash != expected_hash {
        return Err(RevisionError::Stale {
            expected: expected_hash.into(),
            observed: file_ref.content_hash,
        });
    }
    Ok(())
}

pub async fn delete(
    ctx: &ToolContext,
    logical: &str,
    expected_hash: Option<&str>,
    recursive: bool,
) -> Result<(Namespace, String, bool), RevisionError> {
    let (namespace, path, resolved) = resolve(ctx, logical, false)?;
    let metadata = fs::metadata(&resolved).await?;
    let is_dir = metadata.is_dir();
    if is_dir {
        if !recursive {
            return Err(RevisionError::InvalidPath);
        }
        fs::remove_dir_all(&resolved).await?;
    } else {
        let current = fs::read(&resolved).await?;
        let observed = hex::encode(Sha256::digest(&current));
        let expected = expected_hash.ok_or(RevisionError::MissingPrecondition)?;
        if expected != observed {
            return Err(RevisionError::Stale {
                expected: expected.into(),
                observed,
            });
        }
        fs::remove_file(&resolved).await?;
    }
    Ok((namespace, path, is_dir))
}

pub async fn move_file(
    ctx: &ToolContext,
    from: &str,
    to: &str,
    expected_hash: Option<&str>,
) -> Result<(Namespace, String, Namespace, String), RevisionError> {
    let (source_namespace, source_path, source) = resolve(ctx, from, false)?;
    let (destination_namespace, destination_path, destination) = resolve(ctx, to, true)?;
    if source.is_file() {
        let current = fs::read(&source).await?;
        let observed = hex::encode(Sha256::digest(&current));
        let expected = expected_hash.ok_or(RevisionError::MissingPrecondition)?;
        if expected != observed {
            return Err(RevisionError::Stale {
                expected: expected.into(),
                observed,
            });
        }
    }
    if fs::try_exists(&destination).await? {
        return Err(RevisionError::InvalidPath);
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::rename(source, destination).await?;
    Ok((
        source_namespace,
        source_path,
        destination_namespace,
        destination_path,
    ))
}

pub fn permission(error: RevisionError, tool: &str, permission: Permission) -> ToolError {
    match error {
        RevisionError::Escape => ToolError::PermissionDenied(permission),
        RevisionError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => ToolError::NotFound {
            tool: tool.into(),
            path: String::new(),
            hint: String::new(),
        },
        other => ToolError::Execution(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn context(root: &Path) -> ToolContext {
        ToolContext {
            workspace_root: root.to_path_buf(),
            task_id: uuid::Uuid::new_v4(),
            session_id: None,
            progress_tx: None,
        }
    }

    #[tokio::test]
    async fn namespaces_refs_and_stale_write_are_enforced() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());
        let created = write(&ctx, "workspace/a.txt", b"one", None).await.unwrap();
        assert_eq!(created.namespace, Namespace::Workspace);
        assert_eq!(created.bytes, 3);
        let error = write(&ctx, "workspace/a.txt", b"two", Some("wrong"))
            .await
            .unwrap_err();
        assert!(matches!(error, RevisionError::Stale { .. }));
        let updated = write(&ctx, "workspace/a.txt", b"two", Some(&created.content_hash))
            .await
            .unwrap();
        assert_ne!(created.content_hash, updated.content_hash);
    }

    #[tokio::test]
    async fn uploads_are_immutable_and_scratch_is_task_scoped() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());
        let first = write(&ctx, "uploads/input.txt", b"input", None)
            .await
            .unwrap();
        assert!(matches!(
            write(
                &ctx,
                "uploads/input.txt",
                b"changed",
                Some(&first.content_hash)
            )
            .await,
            Err(RevisionError::Immutable)
        ));
        let scratch = write(&ctx, "scratch/temp.txt", b"tmp", None).await.unwrap();
        assert_eq!(scratch.namespace, Namespace::Scratch);
        assert!(scratch.path.starts_with("temp.txt"));
        assert!(fs::metadata(dir.path().join(".evohime/scratch")).is_ok());
    }

    #[tokio::test]
    async fn traversal_and_absolute_paths_are_rejected() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());
        assert!(matches!(
            read(&ctx, "workspace/../secret.txt").await,
            Err(RevisionError::Escape)
        ));
        assert!(matches!(
            read(&ctx, &dir.path().join("secret.txt").display().to_string()).await,
            Err(RevisionError::InvalidPath)
        ));
    }
}
