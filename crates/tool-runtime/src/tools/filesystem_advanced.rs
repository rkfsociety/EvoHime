use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::fs;

// ============================================================================
// delete: Delete files or directories
// ============================================================================

pub const DELETE_NAME: &str = "filesystem.delete";
pub const DELETE_DESCRIPTION: &str = "Delete a file or directory (with confirmation required)";
pub const DELETE_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const DELETE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct DeleteInput {
    path: String,
    #[serde(default)]
    recursive: bool,
}

pub async fn delete(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: DeleteInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: DELETE_NAME.to_string(),
        message: e.to_string(),
    })?;

    let target = ctx.sandbox()?.resolve_existing(&opts.path)?;

    if target.is_dir() {
        if opts.recursive {
            fs::remove_dir_all(&target)
                .await
                .map_err(|e| ToolError::Execution(format!("directory delete failed: {e}")))?;
            Ok(ToolResult {
                output: format!("Directory '{}' deleted recursively", opts.path),
                structured: json!({
                    "path": opts.path,
                    "type": "directory",
                    "recursive": true
                }),
            })
        } else {
            Err(ToolError::InvalidInput {
                tool: DELETE_NAME.to_string(),
                message: "use recursive=true to delete directories".to_string(),
            })
        }
    } else {
        fs::remove_file(&target)
            .await
            .map_err(|e| ToolError::Execution(format!("file delete failed: {e}")))?;
        Ok(ToolResult {
            output: format!("File '{}' deleted", opts.path),
            structured: json!({
                "path": opts.path,
                "type": "file"
            }),
        })
    }
}

// ============================================================================
// move: Rename or move files
// ============================================================================

pub const MOVE_NAME: &str = "filesystem.move";
pub const MOVE_DESCRIPTION: &str = "Move or rename a file or directory";
pub const MOVE_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const MOVE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct MoveInput {
    from: String,
    to: String,
}

pub async fn move_file(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: MoveInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: MOVE_NAME.to_string(),
        message: e.to_string(),
    })?;

    let source = ctx.sandbox()?.resolve_existing(&opts.from)?;
    let dest = ctx.sandbox()?.resolve_for_write(&opts.to)?;

    fs::rename(&source, &dest)
        .await
        .map_err(|e| ToolError::Execution(format!("move failed: {e}")))?;

    Ok(ToolResult {
        output: format!("Moved '{}' to '{}'", opts.from, opts.to),
        structured: json!({
            "from": opts.from,
            "to": opts.to
        }),
    })
}

// ============================================================================
// copy: Copy files or directories
// ============================================================================

pub const COPY_NAME: &str = "filesystem.copy";
pub const COPY_DESCRIPTION: &str = "Copy a file or directory";
pub const COPY_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const COPY_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct CopyInput {
    from: String,
    to: String,
    #[serde(default)]
    recursive: bool,
}

pub async fn copy(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: CopyInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: COPY_NAME.to_string(),
        message: e.to_string(),
    })?;

    let source = ctx.sandbox()?.resolve_existing(&opts.from)?;
    let dest = ctx.sandbox()?.resolve_for_write(&opts.to)?;

    if source.is_dir() {
        if !opts.recursive {
            return Err(ToolError::InvalidInput {
                tool: COPY_NAME.to_string(),
                message: "use recursive=true to copy directories".to_string(),
            });
        }
        copy_dir_recursive(&source, &dest)
            .await
            .map_err(|e| ToolError::Execution(format!("recursive copy failed: {e}")))?;
    } else {
        fs::copy(&source, &dest)
            .await
            .map_err(|e| ToolError::Execution(format!("file copy failed: {e}")))?;
    }

    Ok(ToolResult {
        output: format!("Copied '{}' to '{}'", opts.from, opts.to),
        structured: json!({
            "from": opts.from,
            "to": opts.to,
            "recursive": opts.recursive
        }),
    })
}

async fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    let mut pending = vec![(src.to_path_buf(), dst.to_path_buf())];

    while let Some((source_dir, destination_dir)) = pending.pop() {
        fs::create_dir_all(&destination_dir).await?;
        let mut entries = fs::read_dir(&source_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let dest_path = destination_dir.join(entry.file_name());

            if path.is_dir() {
                pending.push((path, dest_path));
            } else {
                fs::copy(path, dest_path).await?;
            }
        }
    }

    Ok(())
}

// ============================================================================
// stat: Get file metadata
// ============================================================================

pub const STAT_NAME: &str = "filesystem.stat";
pub const STAT_DESCRIPTION: &str = "Get file or directory metadata (size, modified, permissions)";
pub const STAT_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const STAT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
struct StatInput {
    path: String,
}

pub async fn stat(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: StatInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: STAT_NAME.to_string(),
        message: e.to_string(),
    })?;

    let target = ctx.sandbox()?.resolve_existing(&opts.path)?;
    let metadata = fs::metadata(&target)
        .await
        .map_err(|e| ToolError::Execution(format!("stat failed: {e}")))?;

    let is_dir = metadata.is_dir();
    let is_file = metadata.is_file();
    let size = metadata.len();
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        Some(format!("{:o}", metadata.permissions().mode()))
    };

    #[cfg(not(unix))]
    let mode: Option<String> = None;

    Ok(ToolResult {
        output: format!(
            "Path: {}\nType: {}\nSize: {} bytes\nModified: {:?}\nPermissions: {:?}",
            opts.path,
            if is_dir { "directory" } else { "file" },
            size,
            modified,
            mode
        ),
        structured: json!({
            "path": opts.path,
            "is_directory": is_dir,
            "is_file": is_file,
            "size_bytes": size,
            "modified_timestamp": modified,
            "permissions": mode
        }),
    })
}

// ============================================================================
// mkdir: Create a directory
// ============================================================================

pub const MKDIR_NAME: &str = "filesystem.mkdir";
pub const MKDIR_DESCRIPTION: &str = "Create a directory (with parent directories if needed)";
pub const MKDIR_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const MKDIR_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
struct MkdirInput {
    path: String,
}

pub async fn mkdir(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: MkdirInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: MKDIR_NAME.to_string(),
        message: e.to_string(),
    })?;

    let target = ctx.sandbox()?.resolve_for_write(&opts.path)?;

    fs::create_dir_all(&target)
        .await
        .map_err(|e| ToolError::Execution(format!("mkdir failed: {e}")))?;

    Ok(ToolResult {
        output: format!("Directory '{}' created", opts.path),
        structured: json!({
            "path": opts.path
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as std_fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[tokio::test]
    async fn delete_file_works() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("test.txt");
        std_fs::write(&file, "content").expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = delete(&ctx, json!({"path": "test.txt", "recursive": false}))
            .await
            .expect("delete");

        assert!(!file.exists());
        assert!(result.output.contains("deleted"));
    }

    #[tokio::test]
    async fn move_file_works() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("src.txt");
        let dst = dir.path().join("dst.txt");
        std_fs::write(&src, "content").expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        move_file(&ctx, json!({"from": "src.txt", "to": "dst.txt"}))
            .await
            .expect("move");

        assert!(!src.exists());
        assert!(dst.exists());
    }

    #[tokio::test]
    async fn stat_works() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("test.txt");
        std_fs::write(&file, "content").expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = stat(&ctx, json!({"path": "test.txt"})).await.expect("stat");

        assert!(result.output.contains("test.txt"));
        assert!(result.output.contains("file"));
    }
}
