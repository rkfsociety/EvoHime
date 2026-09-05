// User-triggered Core Doctor export: writes the existing structured Core
// logs (`logs/core.jsonl`, `logs/supervisor.jsonl` when present) plus recent
// `run_tool_metrics` aggregates to a caller-chosen JSONL destination.
//
// Every line goes through the same redaction path as the hook/observability
// contract (`crate::observability::redact_text`) before it is written, so a
// log line that somehow contains a secret-looking token is still redacted on
// export. Windows Event Log export was scoped out: no lightweight Event Log
// crate is already in this workspace's dependency tree, and pulling one in
// for an explicitly optional ("если необходимо") requirement was avoided.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::observability::redact_text;

pub const MAX_METRICS_ROWS: usize = 500;

#[derive(Debug)]
pub enum ExportError {
    Io(io::Error),
    InvalidDestination(&'static str),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Io(error) => write!(f, "export io error: {error}"),
            ExportError::InvalidDestination(reason) => {
                write!(f, "invalid export destination: {reason}")
            }
        }
    }
}

impl From<io::Error> for ExportError {
    fn from(value: io::Error) -> Self {
        ExportError::Io(value)
    }
}

impl From<serde_json::Error> for ExportError {
    fn from(value: serde_json::Error) -> Self {
        ExportError::Io(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExportSummary {
    pub contract_version: u16,
    pub bounded: bool,
    pub destination_label: String,
    pub sources_included: Vec<String>,
    pub lines_exported: u64,
    pub metrics_rows_exported: u64,
}

impl ExportSummary {
    pub fn to_bounded_json(&self) -> String {
        serde_json::to_string(self).expect("ExportSummary is serializable")
    }
}

/// Local data directory (`EVOHIME_DATA_DIR`, else `%LOCALAPPDATA%\EvoHime`).
/// Exposed so memory `forget` can rotate the backup containers that still
/// hold an already-erased statement.
pub fn local_data_dir() -> PathBuf {
    crate::get_data_directory()
}

/// Derives a bounded scheduler liveness probe from the Core heartbeat file
/// (`<data_dir>/core-heartbeat`), the same file `main.rs::spawn_heartbeat`
/// writes to and the supervisor watches for staleness.
pub fn scheduler_probe() -> crate::doctor::SchedulerProbe {
    const STALE_THRESHOLD_MS: u64 = 5 * 60 * 1000; // matches supervisor's own tolerance window
    let path = crate::get_data_directory().join("core-heartbeat");
    let heartbeat_age_ms = fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .map(|elapsed| elapsed.as_millis().min(u128::from(u64::MAX)) as u64);
    crate::doctor::SchedulerProbe {
        heartbeat_label: "core-heartbeat".into(),
        heartbeat_age_ms,
        stale_threshold_ms: STALE_THRESHOLD_MS,
    }
}

/// Redacts every string leaf in a JSON value (recursively), regardless of
/// key name. Object keys and array shape are preserved.
fn redact_value(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(redact_text(&text)),
        Value::Array(items) => Value::Array(items.into_iter().map(redact_value).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, redact_value(value)))
                .collect(),
        ),
        other => other,
    }
}

fn export_jsonl_source(
    source_label: &str,
    path: &Path,
    out: &mut impl Write,
) -> Result<u64, ExportError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let reader = BufReader::new(file);
    let mut exported = 0u64;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let record = match serde_json::from_str::<Value>(&line) {
            Ok(value) => redact_value(value),
            Err(_) => Value::String(redact_text(&line)),
        };
        let envelope = json!({
            "source": source_label,
            "record": record,
        });
        serde_json::to_writer(&mut *out, &envelope)?;
        out.write_all(b"\n")?;
        exported += 1;
    }
    Ok(exported)
}

fn export_tool_metrics(out: &mut impl Write) -> Result<u64, ExportError> {
    let events_path = crate::get_data_directory().join("events.db");
    if !events_path.exists() {
        return Ok(0);
    }
    let database = match evohime_local_storage::LocalDatabase::open(&events_path) {
        Ok(database) => database,
        Err(_) => return Ok(0),
    };
    let rows = match database.read_recent_tool_metrics(MAX_METRICS_ROWS) {
        Ok(rows) => rows,
        Err(_) => return Ok(0),
    };
    let mut exported = 0u64;
    for row in rows {
        let record = json!({
            "task_id": redact_text(&row.task_id),
            "tool_name": redact_text(&row.tool_name),
            "iteration": row.iteration,
            "ok": row.ok,
            "failure_kind": row.failure_kind.as_deref().map(redact_text),
            "recovery_hint": row.recovery_hint,
            "escalated": row.escalated,
            "created_at": row.created_at,
        });
        let envelope = json!({
            "source": "run_tool_metrics",
            "record": record,
        });
        serde_json::to_writer(&mut *out, &envelope)?;
        out.write_all(b"\n")?;
        exported += 1;
    }
    Ok(exported)
}

/// Exports `logs/core.jsonl`, `logs/supervisor.jsonl` (when present), and
/// recent `run_tool_metrics` rows to `destination`, one bounded, redacted
/// JSON record per line. Missing source files are skipped, not an error;
/// only I/O failures on the destination itself fail the export.
pub fn export_logs(destination: &Path) -> Result<ExportSummary, ExportError> {
    if destination.as_os_str().is_empty() {
        return Err(ExportError::InvalidDestination("empty path"));
    }
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(destination)?;

    let logs_dir = crate::get_data_directory().join("logs");
    let mut sources_included = Vec::new();
    let mut lines_exported = 0u64;

    let core_log = logs_dir.join("core.jsonl");
    let core_lines = export_jsonl_source("core.jsonl", &core_log, &mut out)?;
    if core_lines > 0 {
        sources_included.push("core.jsonl".to_string());
    }
    lines_exported += core_lines;

    let supervisor_log = logs_dir.join("supervisor.jsonl");
    let supervisor_lines = export_jsonl_source("supervisor.jsonl", &supervisor_log, &mut out)?;
    if supervisor_lines > 0 {
        sources_included.push("supervisor.jsonl".to_string());
    }
    lines_exported += supervisor_lines;

    let metrics_rows_exported = export_tool_metrics(&mut out)?;
    if metrics_rows_exported > 0 {
        sources_included.push("run_tool_metrics".to_string());
    }
    out.flush()?;

    Ok(ExportSummary {
        contract_version: 1,
        bounded: true,
        destination_label: destination.display().to_string(),
        sources_included,
        lines_exported,
        metrics_rows_exported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `EVOHIME_DATA_DIR` is process-global; serialize tests that set it so
    // they don't race with each other under the default parallel test runner.
    static ENV_GUARD: Mutex<()> = Mutex::new(());

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("evohime-export-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn redacts_secret_looking_tokens_and_emails_in_log_lines() {
        let dir = temp_dir("redact");
        let logs_dir = dir.join("logs");
        fs::create_dir_all(&logs_dir).unwrap();
        fs::write(
            logs_dir.join("core.jsonl"),
            format!(
                "{}\n",
                json!({
                    "event": "provider.call",
                    "fields": {
                        "authorization": "Bearer sk-verysecrettoken",
                        "contact": "someone@example.com"
                    }
                })
            ),
        )
        .unwrap();

        let _guard = ENV_GUARD.lock().unwrap();
        std::env::set_var("EVOHIME_DATA_DIR", &dir);
        let destination = dir.join("export.jsonl");
        let summary = export_logs(&destination).unwrap();
        std::env::remove_var("EVOHIME_DATA_DIR");

        assert_eq!(summary.lines_exported, 1);
        let content = fs::read_to_string(&destination).unwrap();
        assert!(!content.contains("verysecrettoken"));
        assert!(!content.contains("someone@example.com"));
        assert!(content.contains("[REDACTED]"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_source_files_are_skipped_not_an_error() {
        let dir = temp_dir("missing");
        let _guard = ENV_GUARD.lock().unwrap();
        std::env::set_var("EVOHIME_DATA_DIR", &dir);
        let destination = dir.join("export.jsonl");
        let summary = export_logs(&destination).unwrap();
        std::env::remove_var("EVOHIME_DATA_DIR");

        assert_eq!(summary.lines_exported, 0);
        assert!(summary.sources_included.is_empty());
        assert!(destination.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_empty_destination() {
        let error = export_logs(Path::new("")).unwrap_err();
        assert!(matches!(error, ExportError::InvalidDestination(_)));
    }

    #[test]
    fn scheduler_probe_reports_none_when_heartbeat_file_absent() {
        let dir = temp_dir("heartbeat-absent");
        let _guard = ENV_GUARD.lock().unwrap();
        std::env::set_var("EVOHIME_DATA_DIR", &dir);
        let probe = scheduler_probe();
        std::env::remove_var("EVOHIME_DATA_DIR");
        assert_eq!(probe.heartbeat_age_ms, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scheduler_probe_reports_fresh_age_when_heartbeat_file_present() {
        let dir = temp_dir("heartbeat-present");
        fs::write(dir.join("core-heartbeat"), b"12345\n").unwrap();
        let _guard = ENV_GUARD.lock().unwrap();
        std::env::set_var("EVOHIME_DATA_DIR", &dir);
        let probe = scheduler_probe();
        std::env::remove_var("EVOHIME_DATA_DIR");
        let age = probe.heartbeat_age_ms.expect("heartbeat age present");
        assert!(age < 60_000);
        let _ = fs::remove_dir_all(&dir);
    }
}
