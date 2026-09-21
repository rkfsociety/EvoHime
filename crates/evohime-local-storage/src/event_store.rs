use crate::{EventRecord, LocalDatabase, StorageError, ToolMetricInput, ToolMetricRecord};

impl LocalDatabase {
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

    pub fn append_event_in_transaction(
        transaction: &rusqlite::Transaction<'_>,
        task_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<i64, StorageError> {
        transaction.execute(
            "INSERT INTO events(task_id, event_type, payload) VALUES (?1, ?2, ?3)",
            rusqlite::params![task_id, event_type, payload],
        )?;
        Ok(transaction.last_insert_rowid())
    }

    /// Appends one journal row with explicit timing boundaries for the Core
    /// journal writer. SQL execution and transaction commit are intentionally
    /// reported separately from the legacy autocommit helper above.
    pub fn append_event_timed(
        &mut self,
        task_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<(i64, f64, f64), StorageError> {
        let transaction = self.connection.transaction()?;
        let sql_started = std::time::Instant::now();
        let sequence =
            Self::append_event_in_transaction(&transaction, task_id, event_type, payload)?;
        let sql_ms = sql_started.elapsed().as_secs_f64() * 1000.0;
        let commit_started = std::time::Instant::now();
        transaction.commit()?;
        let commit_ms = commit_started.elapsed().as_secs_f64() * 1000.0;
        Ok((sequence, sql_ms, commit_ms))
    }

    pub fn record_tool_metric(&self, input: ToolMetricInput<'_>) -> Result<i64, StorageError> {
        self.connection.execute(
            "INSERT INTO run_tool_metrics(task_id, tool_name, iteration, ok, failure_kind, recovery_hint, escalated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                input.task_id,
                input.tool_name,
                input.iteration,
                input.ok as i64,
                input.failure_kind,
                input.recovery_hint as i64,
                input.escalated as i64
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn read_tool_metrics(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<ToolMetricRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, tool_name, iteration, ok, failure_kind, recovery_hint, escalated, created_at
             FROM run_tool_metrics WHERE task_id = ?1 ORDER BY id LIMIT ?2",
        )?;
        let rows = statement.query_map(rusqlite::params![task_id, limit as i64], |row| {
            Ok(ToolMetricRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                tool_name: row.get(2)?,
                iteration: row.get(3)?,
                ok: row.get::<_, i64>(4)? != 0,
                failure_kind: row.get(5)?,
                recovery_hint: row.get::<_, i64>(6)? != 0,
                escalated: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Reads the most recent `run_tool_metrics` rows across all tasks,
    /// newest first, bounded by `limit`. Used by Core Doctor log/metrics
    /// export; carries no secrets (tool names, outcomes, recovery hints).
    pub fn read_recent_tool_metrics(
        &self,
        limit: usize,
    ) -> Result<Vec<ToolMetricRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, tool_name, iteration, ok, failure_kind, recovery_hint, escalated, created_at
             FROM run_tool_metrics ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(rusqlite::params![limit as i64], |row| {
            Ok(ToolMetricRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                tool_name: row.get(2)?,
                iteration: row.get(3)?,
                ok: row.get::<_, i64>(4)? != 0,
                failure_kind: row.get(5)?,
                recovery_hint: row.get::<_, i64>(6)? != 0,
                escalated: row.get::<_, i64>(7)? != 0,
                created_at: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Highest sequence the journal has recorded, or zero when it is empty.
    pub fn latest_event_sequence(&self) -> Result<i64, StorageError> {
        let mut statement = self
            .connection
            .prepare("SELECT COALESCE(MAX(sequence_id), 0) FROM events")?;
        Ok(statement.query_row([], |row| row.get(0))?)
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

    pub fn read_task_events(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT sequence_id, task_id, event_type, payload, created_at
             FROM events WHERE task_id = ?1 ORDER BY sequence_id DESC LIMIT ?2",
        )?;
        let limit = limit.min(i64::MAX as usize) as i64;
        let rows = statement.query_map(rusqlite::params![task_id, limit], |row| {
            Ok(EventRecord {
                sequence_id: row.get(0)?,
                task_id: row.get(1)?,
                event_type: row.get(2)?,
                payload: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut events = rows.collect::<Result<Vec<_>, _>>()?;
        events.reverse();
        Ok(events)
    }

    /// Returns completed review events newest first. Review ids are prefixed
    /// by the Core review contract, so normal agent task history is excluded.
    /// Clearing the history appends a marker rather than deleting rows, so the
    /// query starts after the newest marker and older reviews stay in the
    /// journal for audit and export.
    pub fn read_review_events(&self, limit: usize) -> Result<Vec<EventRecord>, StorageError> {
        let floor: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(sequence_id), 0) FROM events WHERE event_type = 'review.history_cleared'",
            [],
            |row| row.get(0),
        )?;
        let mut statement = self.connection.prepare(
            "SELECT sequence_id, task_id, event_type, payload, created_at
             FROM events WHERE task_id LIKE 'review-%' AND event_type = 'task.completed'
               AND sequence_id > ?2
             ORDER BY sequence_id DESC LIMIT ?1",
        )?;
        let limit = limit.min(i64::MAX as usize) as i64;
        let rows = statement.query_map([limit, floor], |row| {
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
}
