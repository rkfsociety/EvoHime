//! CLI restore of an EvoHime JSON backup (Stage 7.99, wave 2).
//!
//! Mirrors `evohime-export`: reads a dump produced by the exporter or
//! fetched from the cloud sync receiver and restores it idempotently
//! under the given operator.

use evohime_storage::{
    connect_pool, find_operator_by_name, restore_backup, validate_backup_header, BackupDump,
    PoolConfig,
};
use std::{env, ffi::OsString, path::PathBuf};

const DEFAULT_OPERATOR_NAME: &str = "local-owner";

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = parse_args(env::args_os())?;

    let raw = tokio::fs::read(&args.input)
        .await
        .map_err(|error| format!("backup read failed ({}): {error}", args.input.display()))?;
    let dump: BackupDump =
        serde_json::from_slice(&raw).map_err(|error| format!("backup parse failed: {error}"))?;
    validate_backup_header(&dump).map_err(|error| error.to_string())?;

    let database_url = env::var("DATABASE_URL")
        .map_err(|_| "DATABASE_URL is required to restore a backup".to_string())?;
    let pool = connect_pool(&database_url, &PoolConfig::from_env())
        .await
        .map_err(|error| format!("database connection failed: {error}"))?;

    let operator = find_operator_by_name(&pool, &args.operator_name)
        .await
        .map_err(|error| format!("operator lookup failed: {error}"))?
        .ok_or_else(|| format!("operator not found: {}", args.operator_name))?;
    if !operator.active {
        return Err(format!("operator is inactive: {}", args.operator_name));
    }

    let report = restore_backup(&pool, operator.id, &dump)
        .await
        .map_err(|error| format!("restore failed, nothing was written: {error}"))?;

    println!(
        "Restore into operator '{}' complete: sessions {} inserted / {} skipped, \
         {} messages, {} tasks, {} steps, {} events, memory {} inserted / {} skipped",
        operator.name,
        report.sessions_inserted,
        report.sessions_skipped,
        report.messages_inserted,
        report.tasks_inserted,
        report.steps_inserted,
        report.events_inserted,
        report.memory_inserted,
        report.memory_skipped,
    );
    Ok(())
}

struct ImportArgs {
    input: PathBuf,
    operator_name: String,
}

fn parse_args<I, S>(args: I) -> Result<ImportArgs, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into).skip(1);
    let mut input = None;
    let mut operator_name = None;
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--input") | Some("-i") => {
                if input.is_some() {
                    return Err("--input may be provided only once".into());
                }
                let value = args
                    .next()
                    .ok_or_else(|| "--input requires a path".to_string())?;
                if value.is_empty() {
                    return Err("--input requires a non-empty path".into());
                }
                input = Some(PathBuf::from(value));
            }
            Some("--operator-name") => {
                if operator_name.is_some() {
                    return Err("--operator-name may be provided only once".into());
                }
                let value = args
                    .next()
                    .ok_or_else(|| "--operator-name requires a value".to_string())?;
                let value = value
                    .to_str()
                    .ok_or_else(|| "--operator-name must be valid UTF-8".to_string())?
                    .trim()
                    .to_string();
                if value.is_empty() {
                    return Err("--operator-name requires a non-empty value".into());
                }
                operator_name = Some(value);
            }
            Some(flag) => return Err(format!("unknown argument: {flag}")),
            None => return Err("arguments must be valid UTF-8".into()),
        }
    }
    Ok(ImportArgs {
        input: input.ok_or_else(|| "--input <path> is required".to_string())?,
        operator_name: operator_name.unwrap_or_else(|| DEFAULT_OPERATOR_NAME.into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<ImportArgs, String> {
        parse_args(std::iter::once("evohime-import").chain(args.iter().copied()))
    }

    #[test]
    fn input_is_required() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["--input"]).is_err());
        let args = parse(&["--input", "backup.json"]).expect("parse");
        assert_eq!(args.input, PathBuf::from("backup.json"));
        assert_eq!(args.operator_name, DEFAULT_OPERATOR_NAME);
    }

    #[test]
    fn operator_name_is_optional_and_trimmed() {
        let args = parse(&["--input", "b.json", "--operator-name", " alice "]).expect("parse");
        assert_eq!(args.operator_name, "alice");
        assert!(parse(&["--input", "b.json", "--operator-name", " "]).is_err());
    }

    #[test]
    fn duplicate_and_unknown_flags_are_rejected() {
        assert!(parse(&["--input", "a", "--input", "b"]).is_err());
        assert!(parse(&["--wat"]).is_err());
    }
}
