use evohime_storage::{collect_backup, connect_pool, PoolConfig};
use std::{
    env,
    ffi::OsString,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[tokio::main]
async fn main() -> Result<(), String> {
    let output = parse_args(env::args_os())?;
    validate_output_path(&output)?;

    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required to create a backup".to_string())?;
    let pool = connect_pool(&database_url, &PoolConfig::from_env())
        .await
        .map_err(|error| format!("database connection failed: {error}"))?;
    let dump = collect_backup(&pool, evohime_storage::BOOTSTRAP_OWNER_ID)
        .await
        .map_err(|error| format!("backup collection failed: {error}"))?;
    let session_count = dump.sessions.len();
    let memory_count = dump.memory_items.len();
    let json = serde_json::to_vec_pretty(&dump)
        .map_err(|error| format!("backup serialization failed: {error}"))?;

    write_backup(&output, &json).await?;
    println!(
        "Backup written to {} ({} sessions, {} memory items)",
        output.display(),
        session_count,
        memory_count
    );
    Ok(())
}

fn parse_args<I, S>(args: I) -> Result<PathBuf, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into).skip(1);
    let mut output = None;
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--output") | Some("-o") => {
                if output.is_some() {
                    return Err("--output may be provided only once".into());
                }
                let value = args
                    .next()
                    .ok_or_else(|| "--output requires a path".to_string())?;
                if value.is_empty() {
                    return Err("--output requires a non-empty path".into());
                }
                output = Some(PathBuf::from(value));
            }
            Some(flag) => return Err(format!("unknown argument: {flag}")),
            None => return Err("arguments must be valid UTF-8".into()),
        }
    }
    output.ok_or_else(|| "required argument missing: --output <path>".into())
}

fn validate_output_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err("output must name a file".into());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(format!(
            "output parent does not exist: {}",
            parent.display()
        ));
    }
    Ok(())
}

async fn write_backup(output: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = temporary_path(output);
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .await
            .map_err(|error| format!("cannot create temporary backup: {error}"))?;
        use tokio::io::AsyncWriteExt;
        file.write_all(bytes)
            .await
            .map_err(|error| format!("cannot write temporary backup: {error}"))?;
        file.sync_all()
            .await
            .map_err(|error| format!("cannot flush temporary backup: {error}"))?;
        drop(file);

        match tokio::fs::rename(&temp, output).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                tokio::fs::remove_file(output)
                    .await
                    .map_err(|remove_error| {
                        format!("cannot replace existing backup: {remove_error}")
                    })?;
                tokio::fs::rename(&temp, output)
                    .await
                    .map_err(|rename_error| format!("cannot finalize backup: {rename_error}"))
            }
            Err(error) => Err(format!("cannot finalize backup: {error}")),
        }
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result
}

fn temporary_path(output: &Path) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("backup.json");
    output.with_file_name(format!(".{name}.{}.{}.tmp", std::process::id(), suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parses_output_long_option() {
        let args = vec!["evohime-export", "--output", "backup.json"];
        assert_eq!(parse_args(args).unwrap(), PathBuf::from("backup.json"));
    }

    #[test]
    fn parses_output_short_option() {
        let args = vec!["evohime-export", "-o", "backup.json"];
        assert_eq!(parse_args(args).unwrap(), PathBuf::from("backup.json"));
    }

    #[test]
    fn rejects_missing_output_and_unknown_arguments() {
        assert!(parse_args(vec!["evohime-export"]).is_err());
        assert!(parse_args(vec!["evohime-export", "--other", "x"]).is_err());
    }

    #[test]
    fn rejects_missing_output_parent() {
        let path = std::env::temp_dir()
            .join(format!("evohime-export-test-{}", std::process::id()))
            .join("backup.json");
        assert!(validate_output_path(&path).is_err());
    }

    #[tokio::test]
    async fn writes_json_bytes_without_leaving_temporary_file() {
        let output = std::env::temp_dir().join(format!(
            "evohime-export-output-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_backup(&output, br#"{"format":"evohime-backup"}"#)
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read(&output).await.unwrap(),
            br#"{"format":"evohime-backup"}"#
        );
        let temporary = temporary_path(&output);
        assert!(!temporary.exists());
        tokio::fs::remove_file(output).await.unwrap();
    }
}
