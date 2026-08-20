//! Migration-neutral persistence contract for the bounded child-handoff and
//! read-only child-delegation records (`evohime_core::child_roles`,
//! `evohime_core::child_runtime`).
//!
//! The module deliberately does not create or migrate tables. A caller owns
//! schema lifecycle and can apply these statements to existing compatible
//! `child_handoffs` / `child_task_requests` / `child_reports` tables,
//! matching `capability_store.rs` / `research_store.rs` / `memory_store.rs`.
//!
//! This store only persists already-validated records: bounds and contract
//! validation (redaction, capability allow-listing, secret-content
//! rejection, nested-child rejection) are entirely owned by
//! `evohime_core::child_roles::HandoffEnvelope` and
//! `evohime_core::child_runtime::{ChildTaskRequest, ChildReport,
//! accept_report}`. This module stores and retrieves the resulting
//! canonical JSON blobs plus a few columns cheap enough to list/filter on
//! without deserializing every row. It does not spawn, execute, or wire up
//! any actual child agent -- see the crate-level scope note in
//! `evohime-core/src/child_runtime.rs` and `child_roles.rs`.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MAX_ID_BYTES: usize = 256;
const MAX_ROLE_BYTES: usize = 128;
const MAX_KIND_BYTES: usize = 64;
const MAX_STATUS_BYTES: usize = 32;
const MAX_ENVELOPE_JSON_BYTES: usize = 64 * 1024;
const MAX_REQUEST_JSON_BYTES: usize = 64 * 1024;
const MAX_REPORT_JSON_BYTES: usize = 64 * 1024;
const MAX_CHECKPOINT_JSON_BYTES: usize = 128 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ChildStoreError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    Limit { field: &'static str, max: usize },
    #[error("SQLite operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

impl PartialEq for ChildStoreError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Empty { field: left }, Self::Empty { field: right }) => left == right,
            (
                Self::Limit {
                    field: left,
                    max: left_max,
                },
                Self::Limit {
                    field: right,
                    max: right_max,
                },
            ) => left == right && left_max == right_max,
            _ => false,
        }
    }
}

impl Eq for ChildStoreError {}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), ChildStoreError> {
    if value.trim().is_empty() {
        return Err(ChildStoreError::Empty { field });
    }
    if value.len() > max_bytes {
        return Err(ChildStoreError::Limit {
            field,
            max: max_bytes,
        });
    }
    Ok(())
}

/// One persisted `HandoffEnvelope`. `envelope_json` is the canonical JSON
/// encoding; the other columns are denormalized copies kept only for cheap
/// listing/filtering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffRecord {
    pub handoff_id: String,
    pub task_id: String,
    pub kind: String,
    pub status: String,
    pub from_role: String,
    pub to_role: String,
    pub sequence: u64,
    pub envelope_json: String,
}

impl HandoffRecord {
    pub fn validate(&self) -> Result<(), ChildStoreError> {
        validate_text("handoff_id", &self.handoff_id, MAX_ID_BYTES)?;
        validate_text("task_id", &self.task_id, MAX_ID_BYTES)?;
        validate_text("kind", &self.kind, MAX_KIND_BYTES)?;
        validate_text("status", &self.status, MAX_STATUS_BYTES)?;
        validate_text("from_role", &self.from_role, MAX_ROLE_BYTES)?;
        validate_text("to_role", &self.to_role, MAX_ROLE_BYTES)?;
        validate_text(
            "envelope_json",
            &self.envelope_json,
            MAX_ENVELOPE_JSON_BYTES,
        )?;
        Ok(())
    }
}

/// One persisted `ChildTaskRequest` (already validated by
/// `child_runtime::ChildTaskRequest::validate`, which rejects nested
/// children, non-read-only capabilities, and oversized context before this
/// store is ever reached).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildTaskRequestRecord {
    pub child_task_id: String,
    pub parent_task_id: String,
    pub role: String,
    pub kind: String,
    pub request_json: String,
}

impl ChildTaskRequestRecord {
    pub fn validate(&self) -> Result<(), ChildStoreError> {
        validate_text("child_task_id", &self.child_task_id, MAX_ID_BYTES)?;
        validate_text("parent_task_id", &self.parent_task_id, MAX_ID_BYTES)?;
        validate_text("role", &self.role, MAX_ROLE_BYTES)?;
        validate_text("kind", &self.kind, MAX_KIND_BYTES)?;
        validate_text("request_json", &self.request_json, MAX_REQUEST_JSON_BYTES)?;
        Ok(())
    }
}

/// One persisted, accepted `ChildReport` (already validated by
/// `child_runtime::accept_report` against its matching request).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildReportRecord {
    pub child_task_id: String,
    pub parent_task_id: String,
    pub status: String,
    pub confidence_percent: u8,
    pub report_json: String,
}

/// Durable coordinator state for one child revision. The JSON columns contain
/// only typed/redacted projections; raw transcripts are never persisted here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorCheckpointRecord {
    pub schema_version: i64,
    pub child_task_id: String,
    pub parent_task_id: String,
    pub revision: i64,
    pub state: String,
    pub failure_reason: Option<String>,
    pub dead_letter: bool,
    pub report_json: Option<String>,
    pub evidence_locators_json: Option<String>,
    pub provenance_hashes_json: Option<String>,
    pub parent_sequence: i64,
    pub lease_deadline_monotonic_ms: Option<i64>,
    pub lease_created_monotonic_ms: Option<i64>,
    pub lease_clock_boot_id: Option<String>,
    pub lease_holder_process_id: Option<String>,
    pub last_transition_event: String,
    pub last_transition_at_ms: i64,
    pub created_at_ms: i64,
}

impl CoordinatorCheckpointRecord {
    pub fn validate(&self) -> Result<(), ChildStoreError> {
        validate_text("child_task_id", &self.child_task_id, MAX_ID_BYTES)?;
        validate_text("parent_task_id", &self.parent_task_id, MAX_ID_BYTES)?;
        validate_text("state", &self.state, MAX_STATUS_BYTES)?;
        validate_text("last_transition_event", &self.last_transition_event, MAX_KIND_BYTES)?;
        for (field, value) in [
            ("failure_reason", self.failure_reason.as_deref()),
            ("report_json", self.report_json.as_deref()),
            ("evidence_locators_json", self.evidence_locators_json.as_deref()),
            ("provenance_hashes_json", self.provenance_hashes_json.as_deref()),
        ] {
            if let Some(value) = value { validate_text(field, value, MAX_CHECKPOINT_JSON_BYTES)?; }
        }
        if self.revision < 0 || self.parent_sequence < 0 { return Err(ChildStoreError::Limit { field: "checkpoint_counter", max: i64::MAX as usize }); }
        Ok(())
    }
}

impl ChildReportRecord {
    pub fn validate(&self) -> Result<(), ChildStoreError> {
        validate_text("child_task_id", &self.child_task_id, MAX_ID_BYTES)?;
        validate_text("parent_task_id", &self.parent_task_id, MAX_ID_BYTES)?;
        validate_text("status", &self.status, MAX_STATUS_BYTES)?;
        validate_text("report_json", &self.report_json, MAX_REPORT_JSON_BYTES)?;
        Ok(())
    }
}

/// SQL contract only; schema creation and migrations remain outside this API.
pub struct ChildStoreSql;

impl ChildStoreSql {
    /// Atomically reserves the next sequence for one parent task. Gaps are
    /// allowed, but committed children can never share a sequence.
    pub fn next_parent_sequence(connection: &Connection, parent_task_id: &str) -> Result<i64, ChildStoreError> {
        validate_text("parent_task_id", parent_task_id, MAX_ID_BYTES)?;
        connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            connection.execute(
                "INSERT INTO child_parent_sequences(parent_task_id, next_sequence) VALUES (?1, 0) ON CONFLICT(parent_task_id) DO NOTHING",
                [parent_task_id],
            )?;
            connection.execute("UPDATE child_parent_sequences SET next_sequence = next_sequence + 1 WHERE parent_task_id = ?1", [parent_task_id])?;
            connection.query_row("SELECT next_sequence FROM child_parent_sequences WHERE parent_task_id = ?1", [parent_task_id], |row| row.get(0))
        })();
        match result {
            Ok(value) => { connection.execute_batch("COMMIT")?; Ok(value) }
            Err(error) => { let _ = connection.execute_batch("ROLLBACK"); Err(error.into()) }
        }
    }

    pub const INSERT_HANDOFF: &'static str = r#"
        INSERT INTO child_handoffs
            (handoff_id, task_id, kind, status, from_role, to_role, sequence, envelope_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
    "#;

    pub const SELECT_HANDOFFS_BY_TASK: &'static str = r#"
        SELECT handoff_id, task_id, kind, status, from_role, to_role, sequence, envelope_json
        FROM child_handoffs
        WHERE task_id = ?1
        ORDER BY sequence ASC, created_at ASC
        LIMIT ?2
    "#;

    pub const INSERT_CHILD_TASK_REQUEST: &'static str = r#"
        INSERT INTO child_task_requests
            (child_task_id, parent_task_id, role, kind, request_json)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(child_task_id) DO UPDATE SET
            parent_task_id = excluded.parent_task_id,
            role = excluded.role,
            kind = excluded.kind,
            request_json = excluded.request_json
    "#;

    pub const SELECT_CHILD_TASK_REQUEST_BY_ID: &'static str = r#"
        SELECT child_task_id, parent_task_id, role, kind, request_json
        FROM child_task_requests
        WHERE child_task_id = ?1
    "#;

    pub const INSERT_CHILD_REPORT: &'static str = r#"
        INSERT INTO child_reports
            (child_task_id, parent_task_id, status, confidence_percent, report_json)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(child_task_id) DO UPDATE SET
            parent_task_id = excluded.parent_task_id,
            status = excluded.status,
            confidence_percent = excluded.confidence_percent,
            report_json = excluded.report_json
    "#;

    pub const SELECT_CHILD_REPORT_BY_ID: &'static str = r#"
        SELECT child_task_id, parent_task_id, status, confidence_percent, report_json
        FROM child_reports
        WHERE child_task_id = ?1
    "#;

    pub const UPSERT_COORDINATOR_CHECKPOINT: &'static str = r#"
        INSERT INTO coordinator_child_checkpoint
            (schema_version, child_task_id, parent_task_id, revision, state,
             failure_reason, dead_letter, report_json, evidence_locators_json,
             provenance_hashes_json, parent_sequence, lease_deadline_monotonic_ms,
             lease_created_monotonic_ms, lease_clock_boot_id, lease_holder_process_id,
             last_transition_event, last_transition_at_ms, created_at_ms)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
        ON CONFLICT(child_task_id, revision) DO UPDATE SET
            schema_version=excluded.schema_version, parent_task_id=excluded.parent_task_id,
            state=excluded.state, failure_reason=excluded.failure_reason,
            dead_letter=excluded.dead_letter, report_json=excluded.report_json,
            evidence_locators_json=excluded.evidence_locators_json,
            provenance_hashes_json=excluded.provenance_hashes_json,
            parent_sequence=excluded.parent_sequence,
            lease_deadline_monotonic_ms=excluded.lease_deadline_monotonic_ms,
            lease_created_monotonic_ms=excluded.lease_created_monotonic_ms,
            lease_clock_boot_id=excluded.lease_clock_boot_id,
            lease_holder_process_id=excluded.lease_holder_process_id,
            last_transition_event=excluded.last_transition_event,
            last_transition_at_ms=excluded.last_transition_at_ms,
            created_at_ms=excluded.created_at_ms
    "#;

    pub const SELECT_LATEST_COORDINATOR_CHECKPOINT: &'static str = r#"
        SELECT schema_version, child_task_id, parent_task_id, revision, state,
               failure_reason, dead_letter, report_json, evidence_locators_json,
               provenance_hashes_json, parent_sequence, lease_deadline_monotonic_ms,
               lease_created_monotonic_ms, lease_clock_boot_id, lease_holder_process_id,
               last_transition_event, last_transition_at_ms, created_at_ms
        FROM coordinator_child_checkpoint WHERE child_task_id = ?1
        ORDER BY revision DESC, last_transition_at_ms DESC LIMIT 1
    "#;

    pub fn insert_handoff(
        connection: &Connection,
        record: &HandoffRecord,
    ) -> Result<(), ChildStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT_HANDOFF,
            params![
                record.handoff_id,
                record.task_id,
                record.kind,
                record.status,
                record.from_role,
                record.to_role,
                record.sequence as i64,
                record.envelope_json,
            ],
        )?;
        Ok(())
    }

    /// Lists handoffs for a task in sequence order, bounded to at most 500
    /// rows per call so a caller can always page a task's full handoff
    /// history in a small number of calls.
    pub fn list_handoffs_by_task(
        connection: &Connection,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<HandoffRecord>, ChildStoreError> {
        let mut statement = connection.prepare(Self::SELECT_HANDOFFS_BY_TASK)?;
        let records = statement
            .query_map(
                params![task_id, i64::from(limit.clamp(1, 500))],
                map_handoff,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Inserts a new child task request, or replaces the row of the same
    /// `child_task_id` (a caller may resubmit an updated, still-valid
    /// request under the same id before a report is accepted).
    pub fn insert_child_task_request(
        connection: &Connection,
        record: &ChildTaskRequestRecord,
    ) -> Result<(), ChildStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT_CHILD_TASK_REQUEST,
            params![
                record.child_task_id,
                record.parent_task_id,
                record.role,
                record.kind,
                record.request_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_child_task_request(
        connection: &Connection,
        child_task_id: &str,
    ) -> Result<Option<ChildTaskRequestRecord>, ChildStoreError> {
        let record = connection
            .query_row(
                Self::SELECT_CHILD_TASK_REQUEST_BY_ID,
                params![child_task_id],
                map_request,
            )
            .optional()?;
        Ok(record)
    }

    pub fn insert_child_report(
        connection: &Connection,
        record: &ChildReportRecord,
    ) -> Result<(), ChildStoreError> {
        record.validate()?;
        connection.execute(
            Self::INSERT_CHILD_REPORT,
            params![
                record.child_task_id,
                record.parent_task_id,
                record.status,
                i64::from(record.confidence_percent),
                record.report_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_child_report(
        connection: &Connection,
        child_task_id: &str,
    ) -> Result<Option<ChildReportRecord>, ChildStoreError> {
        let record = connection
            .query_row(
                Self::SELECT_CHILD_REPORT_BY_ID,
                params![child_task_id],
                map_report,
            )
            .optional()?;
        Ok(record)
    }

    pub fn upsert_coordinator_checkpoint(
        connection: &Connection,
        record: &CoordinatorCheckpointRecord,
    ) -> Result<(), ChildStoreError> {
        record.validate()?;
        connection.execute(Self::UPSERT_COORDINATOR_CHECKPOINT, params![
            record.schema_version, record.child_task_id, record.parent_task_id,
            record.revision, record.state, record.failure_reason, record.dead_letter as i64,
            record.report_json, record.evidence_locators_json, record.provenance_hashes_json,
            record.parent_sequence, record.lease_deadline_monotonic_ms,
            record.lease_created_monotonic_ms, record.lease_clock_boot_id,
            record.lease_holder_process_id, record.last_transition_event,
            record.last_transition_at_ms, record.created_at_ms,
        ])?;
        Ok(())
    }

    pub fn latest_coordinator_checkpoint(
        connection: &Connection,
        child_task_id: &str,
    ) -> Result<Option<CoordinatorCheckpointRecord>, ChildStoreError> {
        connection.query_row(Self::SELECT_LATEST_COORDINATOR_CHECKPOINT, [child_task_id], map_checkpoint).optional().map_err(Into::into)
    }

    pub fn list_dead_letter_checkpoints(
        connection: &Connection,
        parent_task_id: &str,
        now_ms: i64,
        limit: u32,
    ) -> Result<Vec<CoordinatorCheckpointRecord>, ChildStoreError> {
        let mut statement = connection.prepare(
            "SELECT schema_version, child_task_id, parent_task_id, revision, state,
             failure_reason, dead_letter, report_json, evidence_locators_json,
             provenance_hashes_json, parent_sequence, lease_deadline_monotonic_ms,
             lease_created_monotonic_ms, lease_clock_boot_id, lease_holder_process_id,
             last_transition_event, last_transition_at_ms, created_at_ms
             FROM coordinator_child_checkpoint
             WHERE parent_task_id = ?1 AND dead_letter = 1 AND created_at_ms >= ?2
             ORDER BY last_transition_at_ms DESC LIMIT ?3")?;
        let records = statement
            .query_map(
                params![
                    parent_task_id,
                    now_ms.saturating_sub(30 * 24 * 60 * 60 * 1000),
                    i64::from(limit.clamp(1, 500))
                ],
                map_checkpoint,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    /// Removes only expired dead-letter rows. Terminal checkpoints remain
    /// durable until this explicit retention sweep, and the operation is
    /// idempotent so it is safe to run during every Core boot.
    pub fn prune_dead_letter_checkpoints(
        connection: &Connection,
        now_ms: i64,
    ) -> Result<usize, ChildStoreError> {
        let cutoff = now_ms.saturating_sub(30 * 24 * 60 * 60 * 1000);
        Ok(connection.execute(
            "DELETE FROM coordinator_child_checkpoint
             WHERE dead_letter = 1 AND created_at_ms < ?1",
            [cutoff],
        )?)
    }

    /// Clears lease ownership for terminal checkpoints without changing the
    /// outcome. This makes recovery/cleanup safe to repeat after a crash.
    pub fn cleanup_terminal_leases(connection: &Connection) -> Result<usize, ChildStoreError> {
        Ok(connection.execute(
            "UPDATE coordinator_child_checkpoint
             SET lease_deadline_monotonic_ms = NULL,
                 lease_created_monotonic_ms = NULL,
                 lease_clock_boot_id = NULL,
                 lease_holder_process_id = NULL
             WHERE state IN ('accepted','rejected','failed','cancelled','timed_out','aborted','revise_plan')
               AND (lease_deadline_monotonic_ms IS NOT NULL
                    OR lease_created_monotonic_ms IS NOT NULL
                    OR lease_clock_boot_id IS NOT NULL
                    OR lease_holder_process_id IS NOT NULL)",
            [],
        )?)
    }
}

fn map_handoff(row: &rusqlite::Row<'_>) -> rusqlite::Result<HandoffRecord> {
    Ok(HandoffRecord {
        handoff_id: row.get(0)?,
        task_id: row.get(1)?,
        kind: row.get(2)?,
        status: row.get(3)?,
        from_role: row.get(4)?,
        to_role: row.get(5)?,
        sequence: row.get::<_, i64>(6)? as u64,
        envelope_json: row.get(7)?,
    })
}

fn map_request(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChildTaskRequestRecord> {
    Ok(ChildTaskRequestRecord {
        child_task_id: row.get(0)?,
        parent_task_id: row.get(1)?,
        role: row.get(2)?,
        kind: row.get(3)?,
        request_json: row.get(4)?,
    })
}

fn map_report(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChildReportRecord> {
    Ok(ChildReportRecord {
        child_task_id: row.get(0)?,
        parent_task_id: row.get(1)?,
        status: row.get(2)?,
        confidence_percent: row.get::<_, i64>(3)? as u8,
        report_json: row.get(4)?,
    })
}

fn map_checkpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<CoordinatorCheckpointRecord> {
    Ok(CoordinatorCheckpointRecord {
        schema_version: row.get(0)?, child_task_id: row.get(1)?, parent_task_id: row.get(2)?,
        revision: row.get(3)?, state: row.get(4)?, failure_reason: row.get(5)?,
        dead_letter: row.get::<_, i64>(6)? != 0, report_json: row.get(7)?,
        evidence_locators_json: row.get(8)?, provenance_hashes_json: row.get(9)?,
        parent_sequence: row.get(10)?, lease_deadline_monotonic_ms: row.get(11)?,
        lease_created_monotonic_ms: row.get(12)?, lease_clock_boot_id: row.get(13)?,
        lease_holder_process_id: row.get(14)?, last_transition_event: row.get(15)?,
        last_transition_at_ms: row.get(16)?, created_at_ms: row.get(17)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE child_handoffs (
                    handoff_id TEXT PRIMARY KEY NOT NULL,
                    task_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    status TEXT NOT NULL,
                    from_role TEXT NOT NULL,
                    to_role TEXT NOT NULL,
                    sequence INTEGER NOT NULL,
                    envelope_json BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE child_task_requests (
                    child_task_id TEXT PRIMARY KEY NOT NULL,
                    parent_task_id TEXT NOT NULL,
                    role TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    request_json BLOB NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                 CREATE TABLE child_reports (
                    child_task_id TEXT PRIMARY KEY NOT NULL,
                    parent_task_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    confidence_percent INTEGER NOT NULL,
                    report_json BLOB NOT NULL,
                     accepted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                 );
                 CREATE TABLE coordinator_child_checkpoint (
                     schema_version INTEGER NOT NULL DEFAULT 1,
                     child_task_id TEXT NOT NULL,
                     parent_task_id TEXT NOT NULL,
                     revision INTEGER NOT NULL,
                     state TEXT NOT NULL CHECK(state IN ('created','queued','running','validating','waiting_parent_acceptance','accepted','rejected','failed','cancelled','timed_out','aborted','revise_plan')),
                     failure_reason TEXT,
                     dead_letter INTEGER NOT NULL DEFAULT 0,
                     report_json BLOB,
                     evidence_locators_json BLOB,
                     provenance_hashes_json BLOB,
                     parent_sequence INTEGER NOT NULL,
                     lease_deadline_monotonic_ms INTEGER,
                     lease_created_monotonic_ms INTEGER,
                     lease_clock_boot_id TEXT,
                     lease_holder_process_id TEXT,
                     last_transition_event TEXT NOT NULL,
                     last_transition_at_ms INTEGER NOT NULL,
                     created_at_ms INTEGER NOT NULL,
                     PRIMARY KEY(child_task_id, revision)
                 );
                 CREATE TABLE child_parent_sequences (
                     parent_task_id TEXT PRIMARY KEY NOT NULL,
                     next_sequence INTEGER NOT NULL DEFAULT 0
                 );",
            )
            .expect("contract fixture creates");
    }

    fn handoff(id: &str, sequence: u64) -> HandoffRecord {
        HandoffRecord {
            handoff_id: id.into(),
            task_id: "task-1".into(),
            kind: "delegate".into(),
            status: "pending".into(),
            from_role: "coordinator".into(),
            to_role: "researcher".into(),
            sequence,
            envelope_json: r#"{"handoff_id":"h"}"#.into(),
        }
    }

    fn request(id: &str) -> ChildTaskRequestRecord {
        ChildTaskRequestRecord {
            child_task_id: id.into(),
            parent_task_id: "task-1".into(),
            role: "researcher".into(),
            kind: "code_search".into(),
            request_json: r#"{"child_task_id":"child-1"}"#.into(),
        }
    }

    fn report(id: &str) -> ChildReportRecord {
        ChildReportRecord {
            child_task_id: id.into(),
            parent_task_id: "task-1".into(),
            status: "complete".into(),
            confidence_percent: 90,
            report_json: r#"{"child_task_id":"child-1"}"#.into(),
        }
    }

    #[test]
    fn round_trips_handoff_without_schema_migration() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        let expected = handoff("h-1", 1);

        ChildStoreSql::insert_handoff(&connection, &expected).expect("handoff inserts");

        let listed =
            ChildStoreSql::list_handoffs_by_task(&connection, "task-1", 10).expect("list reads");
        assert_eq!(listed, vec![expected]);
    }

    #[test]
    fn lists_handoffs_in_sequence_order() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        ChildStoreSql::insert_handoff(&connection, &handoff("h-2", 2)).expect("insert 2");
        ChildStoreSql::insert_handoff(&connection, &handoff("h-1", 1)).expect("insert 1");

        let listed =
            ChildStoreSql::list_handoffs_by_task(&connection, "task-1", 10).expect("list reads");
        assert_eq!(
            listed
                .iter()
                .map(|item| item.handoff_id.as_str())
                .collect::<Vec<_>>(),
            ["h-1", "h-2"]
        );
    }

    #[test]
    fn round_trips_request_and_report_and_upserts_by_id() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        ChildStoreSql::insert_child_task_request(&connection, &request("child-1"))
            .expect("request inserts");
        assert_eq!(
            ChildStoreSql::get_child_task_request(&connection, "child-1").expect("request reads"),
            Some(request("child-1"))
        );

        let mut updated = request("child-1");
        updated.role = "planner".into();
        ChildStoreSql::insert_child_task_request(&connection, &updated).expect("request upserts");
        assert_eq!(
            ChildStoreSql::get_child_task_request(&connection, "child-1")
                .expect("request reads")
                .map(|record| record.role),
            Some("planner".to_string())
        );

        ChildStoreSql::insert_child_report(&connection, &report("child-1"))
            .expect("report inserts");
        assert_eq!(
            ChildStoreSql::get_child_report(&connection, "child-1").expect("report reads"),
            Some(report("child-1"))
        );
    }

    #[test]
    fn rejects_unbounded_or_empty_contract_fields_before_sql() {
        let mut invalid = handoff("h-1", 1);
        invalid.envelope_json = "x".repeat(MAX_ENVELOPE_JSON_BYTES + 1);
        assert!(matches!(
            invalid.validate(),
            Err(ChildStoreError::Limit {
                field: "envelope_json",
                ..
            })
        ));

        let mut invalid_request = request("child-1");
        invalid_request.child_task_id.clear();
        assert_eq!(
            invalid_request.validate(),
            Err(ChildStoreError::Empty {
                field: "child_task_id"
            })
        );

        let mut invalid_report = report("child-1");
        invalid_report.status.clear();
        assert_eq!(
            invalid_report.validate(),
            Err(ChildStoreError::Empty { field: "status" })
        );
    }

    #[test]
    fn sequences_are_atomic_and_checkpoint_round_trips() {
        let connection = Connection::open_in_memory().expect("sqlite opens");
        schema(&connection);
        assert_eq!(ChildStoreSql::next_parent_sequence(&connection, "parent").unwrap(), 1);
        assert_eq!(ChildStoreSql::next_parent_sequence(&connection, "parent").unwrap(), 2);
        let record = CoordinatorCheckpointRecord {
            schema_version: 1,
            child_task_id: "child".into(),
            parent_task_id: "parent".into(),
            revision: 0,
            state: "created".into(),
            failure_reason: None,
            dead_letter: false,
            report_json: None,
            evidence_locators_json: None,
            provenance_hashes_json: None,
            parent_sequence: 2,
            lease_deadline_monotonic_ms: Some(100),
            lease_created_monotonic_ms: Some(1),
            lease_clock_boot_id: Some("boot".into()),
            lease_holder_process_id: Some("pid".into()),
            last_transition_event: "created".into(),
            last_transition_at_ms: 2,
            created_at_ms: 1,
        };
        ChildStoreSql::upsert_coordinator_checkpoint(&connection, &record).unwrap();
        assert_eq!(ChildStoreSql::latest_coordinator_checkpoint(&connection, "child").unwrap(), Some(record));
    }
}
