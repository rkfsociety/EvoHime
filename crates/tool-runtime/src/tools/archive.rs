use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
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
const MAX_EXTRACTED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 4_096;

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

    let archive_for_worker = archive.clone();
    let dest_for_worker = dest.clone();
    tokio::task::spawn_blocking(move || {
        if is_zip {
            extract_zip(&archive_for_worker, &dest_for_worker)
        } else {
            extract_tar(&archive_for_worker, &dest_for_worker)
        }
    })
    .await
    .map_err(|e| ToolError::Execution(format!("archive extraction worker failed: {e}")))?
    .map_err(|e| ToolError::Execution(format!("archive extraction failed: {e}")))?;

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

fn safe_archive_path(raw: &Path) -> io::Result<PathBuf> {
    let mut safe = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "archive entry escapes destination",
                ));
            }
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "archive entry has an empty path",
        ));
    }
    Ok(safe)
}

fn ensure_safe_parent(destination: &Path, target: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(destination)?
        .file_type()
        .is_symlink()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "archive destination is a symbolic link",
        ));
    }
    let parent = target.parent().unwrap_or(destination);
    std::fs::create_dir_all(parent)?;
    let mut current = destination.to_path_buf();
    for component in parent
        .strip_prefix(destination)
        .unwrap_or(parent)
        .components()
    {
        if let Component::Normal(part) = component {
            current.push(part);
            let metadata = std::fs::symlink_metadata(&current)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "archive entry traverses a symbolic link or non-directory",
                ));
            }
        }
    }
    Ok(())
}

fn extract_tar(archive_path: &Path, destination: &Path) -> io::Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let reader: Box<dyn Read> = if archive_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
    {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut archive = tar::Archive::new(reader);
    let mut total_bytes = 0_u64;
    for (index, entry) in archive.entries()?.enumerate() {
        if index >= MAX_ARCHIVE_ENTRIES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "archive contains too many entries",
            ));
        }
        let mut entry = entry?;
        let path = safe_archive_path(&entry.path()?)?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "symbolic and hard links are not allowed in archives",
            ));
        }
        reserve_extraction(&mut total_bytes, entry.header().size()?)?;
        let target = destination.join(path);
        ensure_safe_parent(destination, &target)?;
        if entry_type.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry_type.is_file() {
            if std::fs::symlink_metadata(&target)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "archive entry would overwrite a symbolic link",
                ));
            }
            entry.unpack(&target)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported archive entry type",
            ));
        }
    }
    Ok(())
}

fn extract_zip(archive_path: &Path, destination: &Path) -> io::Result<()> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(io::Error::other)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "archive contains too many entries",
        ));
    }
    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(io::Error::other)?;
        if entry.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "symbolic links are not allowed in archives",
            ));
        }
        reserve_extraction(&mut total_bytes, entry.size())?;
        let raw_name = entry.name().replace('\\', "/");
        let path = safe_archive_path(Path::new(&raw_name))?;
        let target = destination.join(path);
        if entry.is_dir() {
            ensure_safe_parent(destination, &target.join("placeholder"))?;
            std::fs::create_dir_all(&target)?;
        } else {
            ensure_safe_parent(destination, &target)?;
            if std::fs::symlink_metadata(&target)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "archive entry would overwrite a symbolic link",
                ));
            }
            let mut output = std::fs::File::create(&target)?;
            io::copy(&mut entry, &mut output)?;
        }
    }
    Ok(())
}

fn reserve_extraction(total: &mut u64, bytes: u64) -> io::Result<()> {
    let next = total.checked_add(bytes).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "archive extracted size overflow",
        )
    })?;
    if next > MAX_EXTRACTED_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "archive extracted size exceeds 64 MiB",
        ));
    }
    *total = next;
    Ok(())
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

    #[test]
    fn archive_extraction_budget_is_bounded() {
        let mut total = 0;
        assert!(reserve_extraction(&mut total, MAX_EXTRACTED_BYTES).is_ok());
        assert!(reserve_extraction(&mut total, 1).is_err());
    }

    #[tokio::test]
    async fn archive_extract_rejects_zip_traversal() {
        let dir = tempdir().expect("tempdir");
        let archive_path = dir.path().join("unsafe.zip");
        let file = std_fs::File::create(&archive_path).expect("archive");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../escaped.txt", zip::write::SimpleFileOptions::default())
            .expect("entry");
        std::io::Write::write_all(&mut writer, b"must not extract").expect("content");
        writer.finish().expect("finish");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let error = extract(&ctx, json!({"archive": "unsafe.zip", "destination": "out"}))
            .await
            .expect_err("unsafe archive must be rejected");
        assert!(error.to_string().contains("escapes destination"));
        assert!(!dir.path().join("escaped.txt").exists());
    }

    #[tokio::test]
    async fn archive_extract_rejects_tar_traversal() {
        let dir = tempdir().expect("tempdir");
        let archive_path = dir.path().join("unsafe.tar");
        let file = std_fs::File::create(&archive_path).expect("archive");
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        let content = b"must not extract";
        header.set_path("placeholder.txt").expect("path");
        header.as_mut_bytes()[..16].copy_from_slice(b"../escaped.txt\0\0");
        header.set_size(content.len() as u64);
        header.set_cksum();
        builder
            .append(&header, std::io::Cursor::new(content))
            .expect("entry");
        builder.finish().expect("finish");

        let ctx = ToolContext {
            workspace_root: dir.path().to_path_buf(),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let error = extract(&ctx, json!({"archive": "unsafe.tar", "destination": "out"}))
            .await
            .expect_err("unsafe archive must be rejected");
        assert!(error.to_string().contains("escapes destination"));
        assert!(!dir.path().join("escaped.txt").exists());
    }
}
