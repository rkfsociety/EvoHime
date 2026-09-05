use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

use crate::{observability, sensitive_data_guardrails};

pub struct StructuredLogger {
    file: Mutex<BufWriter<File>>,
}

impl StructuredLogger {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(BufWriter::new(file)),
        })
    }

    pub fn write(&self, level: &str, event: &str, fields: Value) -> io::Result<()> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let record = json!({
            "timestamp_ms": timestamp_ms,
            "level": level,
            "event": event,
            "fields": fields,
        });
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("structured logger lock poisoned"))?;
        serde_json::to_writer(&mut *file, &record)?;
        file.write_all(b"\n")?;
        file.flush()
    }
}

fn audit_log_path() -> PathBuf {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime"));
    data_dir.join("logs").join("audit.jsonl")
}

pub(crate) fn append_audit_line(line: &str) {
    let path = audit_log_path();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub(crate) fn write_model_trace(event: &str, fields: serde_json::Value) {
    let policy = sensitive_data_guardrails::default_policy("local-trace");
    let fields = sensitive_data_guardrails::redact_json(&policy, &fields)
        .map(|(value, _)| value)
        .unwrap_or_else(|_| serde_json::json!({"redaction_status":"blocked"}));
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime"));
    let logs_dir = data_dir.join("logs");
    if fs::create_dir_all(&logs_dir).is_err() {
        return;
    }
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let record = serde_json::json!({
        "timestamp_ms": timestamp_ms,
        "event": event,
        "fields": fields,
    });
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_dir.join("model-trace.jsonl"))
    {
        if serde_json::to_writer(&mut file, &record).is_ok() {
            let _ = file.write_all(b"\n");
        }
    }
}

pub(crate) fn redact_boundary_text(
    destination: &str,
    value: &str,
) -> Result<String, sensitive_data_guardrails::GuardrailError> {
    sensitive_data_guardrails::redact_text(
        &sensitive_data_guardrails::default_policy(destination),
        value,
    )
    .map(|result| result.value)
}

pub(crate) fn write_observability_hook(
    task_id: &str,
    sequence: u64,
    hook: observability::HookName,
    fields: impl IntoIterator<Item = (String, String)>,
) {
    let Ok(payload) = observability::HookPayload::new(fields) else {
        return;
    };
    let Ok(context_order) = observability::ContextOrder::capture(
        ["system", "user", "assistant", "tool"]
            .into_iter()
            .map(String::from),
    ) else {
        return;
    };
    let decision = observability::HookPolicy::default().decide(hook);
    let event_id = format!("{task_id}:{sequence}");
    let Ok(event) = observability::HookEvent::new(
        hook,
        event_id,
        task_id,
        sequence,
        decision,
        context_order,
        payload,
    ) else {
        return;
    };
    let fields =
        serde_json::from_str(&event.to_deterministic_json()).unwrap_or(serde_json::Value::Null);
    write_model_trace("observability.hook", fields);
}

#[cfg(test)]
mod tests {
    use super::StructuredLogger;
    use serde_json::json;

    #[test]
    fn writes_one_valid_json_record_per_line() {
        let path = std::env::temp_dir().join(format!("evohime-log-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let logger = StructuredLogger::open(&path).expect("logger opens");
        logger
            .write("info", "core.started", json!({"protocol_major": 1}))
            .expect("record writes");
        drop(logger);
        let content = std::fs::read_to_string(&path).expect("log reads");
        let record: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSON");
        assert_eq!(record["event"], "core.started");
        let _ = std::fs::remove_file(path);
    }
}
