use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

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
