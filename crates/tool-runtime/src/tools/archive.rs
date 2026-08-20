use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

// ============================================================================
// archive.create: Create tar/zip archive
// ============================================================================

pub const CREATE_NAME: &str = "archive.create";
pub const CREATE_DESCRIPTION: &str = "Create a tar.gz or zip archive";
pub const CREATE_PERMISSIONS: &[Permission] =
    &[Permission::FilesystemRead, Permission::FilesystemWrite];
pub const CREATE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct CreateInput {
    source: String,      // file or directory to archive
    destination: String, // output archive path
    #[serde(default = "default_format")]
    format: String, // "tar" | "gz" | "tar.gz" | "zip"
}

fn default_format() -> String {
    "tar.gz".to_string()
}

pub async fn create(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: CreateInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: CREATE_NAME.to_string(),
        message: e.to_string(),
    })?;

    let source = ctx.sandbox()?.resolve_existing(&opts.source)?;
    let dest = ctx.sandbox()?.resolve_for_write(&opts.destination)?;

    match opts.format.as_str() {
        "tar" | "tar.gz" | "gz" => {
            let dest_str = dest.to_string_lossy().into_owned();
            let mut args = vec![];

            if opts.format == "tar.gz" || opts.format == "gz" {
                args.push("-z");
            }

            args.push("-c");
            args.push("-f");
            args.push(&dest_str);

            // tar получает имя записи одинаково для файла и каталога:
            // рабочий каталог уже переставлен на родителя источника.
            let source_name = source.file_name().unwrap().to_string_lossy().into_owned();
            args.push(&source_name);

            let output = Command::new("tar")
                .args(&args)
                .current_dir(source.parent().unwrap_or_else(|| std::path::Path::new(".")))
                .output()
                .await
                .map_err(|e| ToolError::Execution(format!("tar failed: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ToolError::Execution(format!("tar failed: {}", stderr)));
            }

            Ok(ToolResult {
                output: format!(
                    "Archive created: {}",
                    dest.file_name().unwrap_or_default().to_string_lossy()
                ),
                structured: json!({
                    "action": "create",
                    "format": opts.format,
                    "source": opts.source,
                    "destination": opts.destination,
                    "success": true
                }),
            })
        }
        "zip" => {
            let dest_str = dest.to_string_lossy().into_owned();
            let source_str = source.to_string_lossy().into_owned();
            let args = vec!["-r", &dest_str, &source_str];

            let output = Command::new("zip")
                .args(&args)
                .current_dir(&ctx.workspace_root)
                .output()
                .await
                .map_err(|e| ToolError::Execution(format!("zip failed: {e}")))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(ToolError::Execution(format!("zip failed: {}", stderr)));
            }

            Ok(ToolResult {
                output: format!(
                    "Archive created: {}",
                    dest.file_name().unwrap_or_default().to_string_lossy()
                ),
                structured: json!({
                    "action": "create",
                    "format": "zip",
                    "source": opts.source,
                    "destination": opts.destination,
                    "success": true
                }),
            })
        }
        _ => Err(ToolError::InvalidInput {
            tool: CREATE_NAME.to_string(),
            message: format!(
                "unsupported format '{}', expected: tar|tar.gz|gz|zip",
                opts.format
            ),
        }),
    }
}

// ============================================================================
// archive.extract: Extract archive
// ============================================================================

pub const EXTRACT_NAME: &str = "archive.extract";
pub const EXTRACT_DESCRIPTION: &str = "Extract a tar.gz or zip archive";
pub const EXTRACT_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const EXTRACT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct ExtractInput {
    archive: String,
    #[serde(default)]
    destination: Option<String>,
}

pub async fn extract(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: ExtractInput =
        serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
            tool: EXTRACT_NAME.to_string(),
            message: e.to_string(),
        })?;

    let archive = ctx.sandbox()?.resolve_existing(&opts.archive)?;
    let dest = if let Some(path) = opts.destination {
        ctx.sandbox()?.resolve_for_write(&path)?
    } else {
        ctx.workspace_root.clone()
    };

    // Detect format from file extension
    let is_zip = archive.to_string_lossy().ends_with(".zip");

    if is_zip {
        let output = Command::new("unzip")
            .arg("-q")
            .arg(&archive)
            .arg("-d")
            .arg(&dest)
            .output()
            .await
            .map_err(|e| ToolError::Execution(format!("unzip failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::Execution(format!("unzip failed: {}", stderr)));
        }
    } else {
        let output = Command::new("tar")
            .arg("-x")
            .arg("-f")
            .arg(&archive)
            .arg("-C")
            .arg(&dest)
            .output()
            .await
            .map_err(|e| ToolError::Execution(format!("tar extract failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::Execution(format!(
                "tar extract failed: {}",
                stderr
            )));
        }
    }

    Ok(ToolResult {
        output: format!("Archive extracted to {}", dest.display()),
        structured: json!({
            "action": "extract",
            "archive": opts.archive,
            "destination": dest.to_string_lossy(),
            "success": true
        }),
    })
}

// ============================================================================
// archive.list: List archive contents
// ============================================================================

pub const LIST_NAME: &str = "archive.list";
pub const LIST_DESCRIPTION: &str = "List contents of an archive";
pub const LIST_PERMISSIONS: &[Permission] = &[Permission::FilesystemRead];
pub const LIST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct ListInput {
    archive: String,
}

pub async fn list(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: ListInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: LIST_NAME.to_string(),
        message: e.to_string(),
    })?;

    let archive = ctx.sandbox()?.resolve_existing(&opts.archive)?;
    let is_zip = archive.to_string_lossy().ends_with(".zip");

    let output = if is_zip {
        Command::new("unzip")
            .arg("-l")
            .arg(&archive)
            .output()
            .await
            .map_err(|e| ToolError::Execution(format!("unzip list failed: {e}")))?
    } else {
        Command::new("tar")
            .arg("-t")
            .arg("-f")
            .arg(&archive)
            .output()
            .await
            .map_err(|e| ToolError::Execution(format!("tar list failed: {e}")))?
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "list",
            "archive": opts.archive,
            "entries": stdout.lines().collect::<Vec<_>>()
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
    async fn archive_create_and_extract_works() {
        let dir = tempdir().expect("tempdir");
        let src = dir.path().join("source.txt");
        std_fs::write(&src, "test content").expect("write");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let archive_path = "test.tar.gz";
        let result = create(
            &ctx,
            json!({
                "source": "source.txt",
                "destination": archive_path,
                "format": "tar.gz"
            }),
        )
        .await;

        assert!(result.is_ok(), "archive create failed");
        assert!(dir.path().join(archive_path).exists());
    }
}
