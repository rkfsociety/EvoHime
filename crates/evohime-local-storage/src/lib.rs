use std::{
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported database schema version {0}")]
    UnsupportedSchema(u32),
}

pub struct LocalDatabase {
    path: PathBuf,
    connection: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub sequence_id: i64,
    pub task_id: String,
    pub event_type: String,
    pub payload: Vec<u8>,
    pub created_at: String,
}

impl LocalDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let existed = path.exists();
        let connection = Connection::open(&path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let version = Self::read_schema_version(&connection)?;
        if version > SCHEMA_VERSION {
            return Err(StorageError::UnsupportedSchema(version));
        }
        if version < SCHEMA_VERSION {
            if existed {
                fs::copy(&path, path.with_extension("db.bak"))?;
            }
            Self::migrate(&connection, version)?;
        }
        Ok(Self { path, connection })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(Self::read_schema_version(&self.connection)?)
    }

    pub fn has_events_table(&self) -> Result<bool, StorageError> {
        Ok(self
            .connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'events' LIMIT 1",
                [],
                |row| row.get::<_, i32>(0),
            )
            .optional()?
            .is_some())
    }

    pub fn append_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<i64, StorageError> {
        self.connection.execute(
            "INSERT INTO events(task_id, event_type, payload) VALUES (?1, ?2, ?3)",
            rusqlite::params![task_id, event_type, payload],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn read_events_after(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT sequence_id, task_id, event_type, payload, created_at
             FROM events WHERE sequence_id > ?1 ORDER BY sequence_id LIMIT ?2",
        )?;
        let limit = limit.min(i64::MAX as usize) as i64;
        let rows = statement.query_map(rusqlite::params![after_sequence, limit], |row| {
            Ok(EventRecord {
                sequence_id: row.get(0)?,
                task_id: row.get(1)?,
                event_type: row.get(2)?,
                payload: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn export_events_jsonl(&self, output: impl AsRef<Path>) -> Result<(), StorageError> {
        if let Some(parent) = output.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let file = fs::File::create(output)?;
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

    fn read_schema_version(connection: &Connection) -> Result<u32, rusqlite::Error> {
        connection.query_row("PRAGMA user_version", [], |row| row.get(0))
    }

    fn migrate(connection: &Connection, current: u32) -> Result<(), StorageError> {
        let transaction = connection.unchecked_transaction()?;
        if current < 1 {
            transaction.execute_batch(
                "CREATE TABLE IF NOT EXISTS events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX IF NOT EXISTS idx_events_task_sequence ON events(task_id, sequence_id);
                PRAGMA user_version = 1;",
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalDatabase, SCHEMA_VERSION};
    use std::path::PathBuf;

    fn temp_database_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("evohime-test-{name}-{}.db", std::process::id()))
    }

    #[test]
    fn creates_schema_and_reports_version() {
        let path = temp_database_path("schema");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        assert_eq!(
            database.schema_version().expect("version reads"),
            SCHEMA_VERSION
        );
        assert!(database.has_events_table().expect("table exists"));
        drop(database);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn backs_up_existing_database_before_migration() {
        let path = temp_database_path("backup");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        {
            let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
            connection
                .pragma_update(None, "user_version", 0_u32)
                .expect("legacy version writes");
        }
        let _database = LocalDatabase::open(&path).expect("database migrates");
        assert!(path.with_extension("db.bak").exists());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
    }

    #[test]
    fn appends_and_replays_events_by_sequence() {
        let path = temp_database_path("events");
        let _ = std::fs::remove_file(&path);
        let database = LocalDatabase::open(&path).expect("database opens");
        let first = database
            .append_event("task-1", "task.started", b"one")
            .expect("first event");
        let second = database
            .append_event("task-1", "task.completed", b"two")
            .expect("second event");
        let events = database.read_events_after(first, 10).expect("events read");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].sequence_id, second);
        assert_eq!(events[0].payload, b"two");
        drop(database);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn exports_events_as_jsonl() {
        let path = temp_database_path("export");
        let output = path.with_extension("jsonl");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&output);
        let database = LocalDatabase::open(&path).expect("database opens");
        database
            .append_event("task-export", "task.started", br#"{"ok":true}"#)
            .expect("event writes");
        database
            .export_events_jsonl(&output)
            .expect("export writes");
        let content = std::fs::read_to_string(&output).expect("export reads");
        let record: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSON");
        assert_eq!(record["task_id"], "task-export");
        assert_eq!(record["payload"]["ok"], true);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(output);
    }
}
