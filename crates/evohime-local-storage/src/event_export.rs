use std::io::{BufWriter, Write};
use std::path::Path;

use crate::{LocalDatabase, StorageError};

impl LocalDatabase {
    /// Exports all stored events as newline-delimited JSON records.
    ///
    /// The output file is replaced, and its parent directories are created if
    /// needed. Event payloads that are not valid JSON are preserved as a
    /// `raw_bytes` value so export does not silently discard stored data.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if reading events, creating the output path,
    /// serializing records, writing, or flushing fails.
    pub fn export_events_jsonl(&self, output: impl AsRef<Path>) -> Result<(), StorageError> {
        if let Some(parent) = output.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(output)?;
        let mut writer = BufWriter::new(file);
        for event in self.read_events_after(0, usize::MAX)? {
            let payload = serde_json::from_slice::<serde_json::Value>(&event.payload)
                .unwrap_or_else(|_| serde_json::json!({"raw_bytes": event.payload}));
            serde_json::to_writer(
                &mut writer,
                &serde_json::json!({
                    "sequence_id": event.sequence_id,
                    "task_id": event.task_id,
                    "event_type": event.event_type,
                    "payload": payload,
                    "created_at": event.created_at,
                }),
            )?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
        Ok(())
    }
}
