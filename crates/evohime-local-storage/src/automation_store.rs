//! Durable definition/run boundary for automation contract 16.1.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::Digest;

pub const AUTOMATION_STORE_SCHEMA: u32 = 1;
pub const MAX_ARCHIVE_RUNS: u32 = 10_000;
pub const MAX_ARCHIVE_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
pub const MAX_ARCHIVE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ARCHIVE_EVENTS: usize = 256;
pub const MAX_ARCHIVE_SNAPSHOTS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationScheduleRecord {
    pub schedule_id: String,
    pub definition_id: String,
    pub revision: u64,
    pub owner_scope: String,
    pub hour: u8,
    pub minute: u8,
    pub timezone_minutes: i32,
    pub missed_grace_ms: i64,
    pub enabled: bool,
    pub last_slot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationDefinitionRecord {
    pub definition_id: String,
    pub revision: u64,
    pub owner_scope: String,
    pub definition_json: String,
    pub definition_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationRunRecord {
    pub run_id: String,
    pub definition_id: String,
    pub revision: u64,
    pub owner_scope: String,
    pub idempotency_key: String,
    pub payload_hash: String,
    pub state: String,
    pub generation: u64,
    pub permission_snapshot: String,
    pub approval_snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationRunEventRecord {
    pub run_sequence: u64,
    pub event_type: String,
    pub generation: u64,
    pub payload_json: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationArchiveRecord {
    pub archive_id: String,
    pub run_id: String,
    pub archive_json: String,
    pub checksum_sha256: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AutomationSnapshotRecord {
    snapshot_id: String,
    run_id: String,
    definition_revision: u64,
    generation: u64,
    event_sequence: u64,
    snapshot_json: String,
    checksum_sha256: String,
    created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitRunResult {
    Inserted,
    Existing(AutomationRunRecord),
    IdempotencyConflict { existing_payload_hash: String },
}

pub struct RunTransition<'a> {
    pub run_id: &'a str,
    pub from_state: &'a str,
    pub to_state: &'a str,
    pub generation: u64,
    pub event_type: &'a str,
    pub payload_json: &'a str,
    pub now_ms: i64,
}

pub struct SnapshotInsert<'a> {
    pub snapshot_id: &'a str,
    pub run_id: &'a str,
    pub definition_revision: u64,
    pub generation: u64,
    pub event_sequence: u64,
    pub snapshot_json: &'a str,
    pub checksum_sha256: &'a str,
    pub now_ms: i64,
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS automation_definitions (definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, definition_json TEXT NOT NULL, definition_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(definition_id, revision, owner_scope)); CREATE TABLE IF NOT EXISTS automation_runs (run_id TEXT PRIMARY KEY, definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, idempotency_key TEXT NOT NULL, payload_hash TEXT NOT NULL, state TEXT NOT NULL, generation INTEGER NOT NULL, permission_snapshot TEXT NOT NULL, approval_snapshot TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, UNIQUE(owner_scope, definition_id, revision, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_automation_runs_state ON automation_runs(state, updated_at_ms); CREATE TABLE IF NOT EXISTS automation_run_events (run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE, run_sequence INTEGER NOT NULL, event_type TEXT NOT NULL, generation INTEGER NOT NULL, payload_json TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(run_id, run_sequence)); CREATE TABLE IF NOT EXISTS automation_leases (run_id TEXT PRIMARY KEY REFERENCES automation_runs(run_id) ON DELETE CASCADE, owner_id TEXT NOT NULL, generation INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS automation_snapshots (snapshot_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE, definition_revision INTEGER NOT NULL, generation INTEGER NOT NULL, event_sequence INTEGER NOT NULL, snapshot_json TEXT NOT NULL, checksum_sha256 TEXT NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS automation_schedules (schedule_id TEXT PRIMARY KEY, definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, hour INTEGER NOT NULL, minute INTEGER NOT NULL, timezone_minutes INTEGER NOT NULL, missed_grace_ms INTEGER NOT NULL, enabled INTEGER NOT NULL, last_slot TEXT, updated_at_ms INTEGER NOT NULL, FOREIGN KEY(definition_id, revision, owner_scope) REFERENCES automation_definitions(definition_id, revision, owner_scope)); CREATE TABLE IF NOT EXISTS automation_archives (archive_id TEXT PRIMARY KEY, run_id TEXT NOT NULL UNIQUE, archive_json TEXT NOT NULL, checksum_sha256 TEXT NOT NULL, created_at_ms INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL);")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArchivePayload {
    run: AutomationRunRecord,
    events: Vec<AutomationRunEventRecord>,
    snapshots: Vec<AutomationSnapshotRecord>,
}

pub fn archive_run(
    connection: &mut Connection,
    archive_id: &str,
    run_id: &str,
    now_ms: i64,
    expires_at_ms: i64,
) -> rusqlite::Result<bool> {
    let tx = connection.transaction()?;
    let archive_count: i64 =
        tx.query_row("SELECT COUNT(*) FROM automation_archives", [], |row| {
            row.get(0)
        })?;
    if archive_count >= i64::from(MAX_ARCHIVE_RUNS) {
        tx.rollback()?;
        return Ok(false);
    }
    let run = tx
        .query_row(
            "SELECT run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot FROM automation_runs WHERE run_id=?1",
            [run_id],
            map_run,
        )
        .optional()?;
    let Some(run) = run else {
        tx.rollback()?;
        return Ok(false);
    };
    let mut event_statement = tx.prepare(
        "SELECT run_sequence, event_type, generation, payload_json, created_at_ms FROM automation_run_events WHERE run_id=?1 ORDER BY run_sequence",
    )?;
    let events = event_statement
        .query_map([run_id], |row| {
            Ok(AutomationRunEventRecord {
                run_sequence: row.get::<_, i64>(0)? as u64,
                event_type: row.get(1)?,
                generation: row.get::<_, i64>(2)? as u64,
                payload_json: row.get(3)?,
                created_at_ms: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(event_statement);
    let mut snapshot_statement = tx.prepare(
        "SELECT snapshot_id, run_id, definition_revision, generation, event_sequence, snapshot_json, checksum_sha256, created_at_ms FROM automation_snapshots WHERE run_id=?1 ORDER BY snapshot_id",
    )?;
    let snapshots = snapshot_statement
        .query_map([run_id], |row| {
            Ok(AutomationSnapshotRecord {
                snapshot_id: row.get(0)?,
                run_id: row.get(1)?,
                definition_revision: row.get::<_, i64>(2)? as u64,
                generation: row.get::<_, i64>(3)? as u64,
                event_sequence: row.get::<_, i64>(4)? as u64,
                snapshot_json: row.get(5)?,
                checksum_sha256: row.get(6)?,
                created_at_ms: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(snapshot_statement);
    if expires_at_ms < now_ms
        || expires_at_ms > now_ms.saturating_add(MAX_ARCHIVE_RETENTION_MS)
        || events.len() > MAX_ARCHIVE_EVENTS
        || snapshots.len() > MAX_ARCHIVE_SNAPSHOTS
    {
        tx.rollback()?;
        return Ok(false);
    }
    let payload = serde_json::to_string(&ArchivePayload {
        run,
        events,
        snapshots,
    })
    .map_err(|_| rusqlite::Error::InvalidQuery)?;
    let checksum = hex::encode(sha2::Sha256::digest(payload.as_bytes()));
    if payload.len() > MAX_ARCHIVE_BYTES {
        tx.rollback()?;
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO automation_archives (archive_id, run_id, archive_json, checksum_sha256, created_at_ms, expires_at_ms) VALUES (?1,?2,?3,?4,?5,?6)",
        params![archive_id, run_id, payload, checksum, now_ms, expires_at_ms],
    )?;
    tx.execute("DELETE FROM automation_runs WHERE run_id=?1", [run_id])?;
    tx.commit()?;
    Ok(true)
}

pub fn restore_archive(
    connection: &mut Connection,
    archive_id: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let tx = connection.transaction()?;
    let (run_id, archive_json, checksum, expires_at_ms): (String, String, String, i64) = tx.query_row(
        "SELECT run_id, archive_json, checksum_sha256, expires_at_ms FROM automation_archives WHERE archive_id=?1",
        [archive_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let observed = hex::encode(sha2::Sha256::digest(archive_json.as_bytes()));
    if observed != checksum
        || now_ms >= expires_at_ms
        || archive_json.len() > MAX_ARCHIVE_BYTES
        || tx.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM automation_runs WHERE run_id=?1",
            [&run_id],
            |row| row.get(0),
        )? != 0
    {
        tx.rollback()?;
        return Ok(false);
    }
    let payload: ArchivePayload =
        serde_json::from_str(&archive_json).map_err(|_| rusqlite::Error::InvalidQuery)?;
    if payload.events.len() > MAX_ARCHIVE_EVENTS || payload.snapshots.len() > MAX_ARCHIVE_SNAPSHOTS
    {
        tx.rollback()?;
        return Ok(false);
    }
    let run = payload.run;
    tx.execute(
        "INSERT INTO automation_runs (run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot, created_at_ms, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
        params![run.run_id, run.definition_id, run.revision as i64, run.owner_scope, run.idempotency_key, run.payload_hash, run.state, run.generation as i64, run.permission_snapshot, run.approval_snapshot, now_ms],
    )?;
    for event in payload.events {
        tx.execute(
            "INSERT INTO automation_run_events (run_id, run_sequence, event_type, generation, payload_json, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6)",
            params![run_id, event.run_sequence as i64, event.event_type, event.generation as i64, event.payload_json, event.created_at_ms],
        )?;
    }
    for snapshot in payload.snapshots {
        tx.execute(
            "INSERT INTO automation_snapshots (snapshot_id, run_id, definition_revision, generation, event_sequence, snapshot_json, checksum_sha256, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![snapshot.snapshot_id, snapshot.run_id, snapshot.definition_revision as i64, snapshot.generation as i64, snapshot.event_sequence as i64, snapshot.snapshot_json, snapshot.checksum_sha256, snapshot.created_at_ms],
        )?;
    }
    tx.commit()?;
    Ok(true)
}

pub fn sweep_expired_archives(connection: &Connection, now_ms: i64) -> rusqlite::Result<u32> {
    Ok(connection.execute(
        "DELETE FROM automation_archives WHERE expires_at_ms <= ?1",
        [now_ms],
    )? as u32)
}

pub fn upsert_schedule(
    connection: &Connection,
    record: &AutomationScheduleRecord,
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO automation_schedules (schedule_id, definition_id, revision, owner_scope, hour, minute, timezone_minutes, missed_grace_ms, enabled, last_slot, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(schedule_id) DO UPDATE SET definition_id=excluded.definition_id, revision=excluded.revision, owner_scope=excluded.owner_scope, hour=excluded.hour, minute=excluded.minute, timezone_minutes=excluded.timezone_minutes, missed_grace_ms=excluded.missed_grace_ms, enabled=excluded.enabled, updated_at_ms=excluded.updated_at_ms",
        params![
            record.schedule_id,
            record.definition_id,
            record.revision as i64,
            record.owner_scope,
            record.hour as i64,
            record.minute as i64,
            record.timezone_minutes,
            record.missed_grace_ms,
            record.enabled as i64,
            record.last_slot,
            now_ms,
        ],
    )?;
    Ok(())
}

pub fn get_schedule(
    connection: &Connection,
    schedule_id: &str,
) -> rusqlite::Result<Option<AutomationScheduleRecord>> {
    connection
        .query_row(
            "SELECT schedule_id, definition_id, revision, owner_scope, hour, minute, timezone_minutes, missed_grace_ms, enabled, last_slot FROM automation_schedules WHERE schedule_id=?1",
            [schedule_id],
            |row| {
                Ok(AutomationScheduleRecord {
                    schedule_id: row.get(0)?,
                    definition_id: row.get(1)?,
                    revision: row.get::<_, i64>(2)? as u64,
                    owner_scope: row.get(3)?,
                    hour: row.get::<_, i64>(4)? as u8,
                    minute: row.get::<_, i64>(5)? as u8,
                    timezone_minutes: row.get(6)?,
                    missed_grace_ms: row.get(7)?,
                    enabled: row.get::<_, i64>(8)? != 0,
                    last_slot: row.get(9)?,
                })
            },
        )
        .optional()
}

pub fn list_schedules(
    connection: &Connection,
    owner_scope: &str,
    limit: u32,
) -> rusqlite::Result<Vec<AutomationScheduleRecord>> {
    let mut statement = connection.prepare(
        "SELECT schedule_id, definition_id, revision, owner_scope, hour, minute, timezone_minutes, missed_grace_ms, enabled, last_slot FROM automation_schedules WHERE owner_scope=?1 ORDER BY schedule_id LIMIT ?2",
    )?;
    let rows = statement.query_map(params![owner_scope, limit.clamp(1, 256)], |row| {
        Ok(AutomationScheduleRecord {
            schedule_id: row.get(0)?,
            definition_id: row.get(1)?,
            revision: row.get::<_, i64>(2)? as u64,
            owner_scope: row.get(3)?,
            hour: row.get::<_, i64>(4)? as u8,
            minute: row.get::<_, i64>(5)? as u8,
            timezone_minutes: row.get(6)?,
            missed_grace_ms: row.get(7)?,
            enabled: row.get::<_, i64>(8)? != 0,
            last_slot: row.get(9)?,
        })
    })?;
    rows.collect()
}

pub fn list_enabled_schedules(
    connection: &Connection,
) -> rusqlite::Result<Vec<AutomationScheduleRecord>> {
    let mut statement = connection.prepare(
        "SELECT schedule_id, definition_id, revision, owner_scope, hour, minute, timezone_minutes, missed_grace_ms, enabled, last_slot FROM automation_schedules WHERE enabled=1 ORDER BY schedule_id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(AutomationScheduleRecord {
            schedule_id: row.get(0)?,
            definition_id: row.get(1)?,
            revision: row.get::<_, i64>(2)? as u64,
            owner_scope: row.get(3)?,
            hour: row.get::<_, i64>(4)? as u8,
            minute: row.get::<_, i64>(5)? as u8,
            timezone_minutes: row.get(6)?,
            missed_grace_ms: row.get(7)?,
            enabled: row.get::<_, i64>(8)? != 0,
            last_slot: row.get(9)?,
        })
    })?;
    rows.collect()
}

/// Advances a schedule cursor only if it still points at `expected_last_slot`.
/// The compare-and-swap makes scheduler polling safe across Core generations.
pub fn advance_schedule_slot(
    connection: &Connection,
    schedule_id: &str,
    expected_last_slot: Option<&str>,
    next_slot: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = match expected_last_slot {
        Some(expected) => connection.execute(
            "UPDATE automation_schedules SET last_slot=?1, updated_at_ms=?2 WHERE schedule_id=?3 AND last_slot=?4",
            params![next_slot, now_ms, schedule_id, expected],
        )?,
        None => connection.execute(
            "UPDATE automation_schedules SET last_slot=?1, updated_at_ms=?2 WHERE schedule_id=?3 AND last_slot IS NULL",
            params![next_slot, now_ms, schedule_id],
        )?,
    };
    Ok(changed == 1)
}

pub fn insert_definition(
    connection: &Connection,
    record: &AutomationDefinitionRecord,
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO automation_definitions (definition_id, revision, owner_scope, definition_json, definition_hash, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![record.definition_id, record.revision as i64, record.owner_scope, record.definition_json, record.definition_hash, now_ms])?;
    Ok(())
}

pub fn get_definition(
    connection: &Connection,
    definition_id: &str,
    revision: u64,
    owner_scope: &str,
) -> rusqlite::Result<Option<AutomationDefinitionRecord>> {
    connection.query_row("SELECT definition_id, revision, owner_scope, definition_json, definition_hash FROM automation_definitions WHERE definition_id=?1 AND revision=?2 AND owner_scope=?3", params![definition_id, revision as i64, owner_scope], |row| Ok(AutomationDefinitionRecord { definition_id: row.get(0)?, revision: row.get::<_, i64>(1)? as u64, owner_scope: row.get(2)?, definition_json: row.get(3)?, definition_hash: row.get(4)? })).optional()
}

pub fn find_run_by_idempotency(
    connection: &Connection,
    owner_scope: &str,
    definition_id: &str,
    revision: u64,
    idempotency_key: &str,
) -> rusqlite::Result<Option<AutomationRunRecord>> {
    connection.query_row("SELECT run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot FROM automation_runs WHERE owner_scope=?1 AND definition_id=?2 AND revision=?3 AND idempotency_key=?4", params![owner_scope, definition_id, revision as i64, idempotency_key], map_run).optional()
}

pub fn get_run(
    connection: &Connection,
    run_id: &str,
) -> rusqlite::Result<Option<AutomationRunRecord>> {
    connection
        .query_row(
            "SELECT run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot FROM automation_runs WHERE run_id=?1",
            [run_id],
            map_run,
        )
        .optional()
}

pub fn list_runs(
    connection: &Connection,
    owner_scope: &str,
    definition_id: &str,
    limit: u32,
) -> rusqlite::Result<Vec<AutomationRunRecord>> {
    let mut statement = connection.prepare(
        "SELECT run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot FROM automation_runs WHERE owner_scope=?1 AND (?2='' OR definition_id=?2) ORDER BY updated_at_ms DESC, run_id DESC LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![owner_scope, definition_id, limit.clamp(1, 256)],
        map_run,
    )?;
    rows.collect()
}

pub fn list_run_events(
    connection: &Connection,
    run_id: &str,
    after_sequence: i64,
    limit: u32,
) -> rusqlite::Result<Vec<AutomationRunEventRecord>> {
    let mut statement = connection.prepare(
        "SELECT run_sequence, event_type, generation, payload_json, created_at_ms FROM automation_run_events WHERE run_id=?1 AND run_sequence>?2 ORDER BY run_sequence LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![run_id, after_sequence.max(-1), limit.clamp(1, 256)],
        |row| {
            Ok(AutomationRunEventRecord {
                run_sequence: row.get::<_, i64>(0)? as u64,
                event_type: row.get(1)?,
                generation: row.get::<_, i64>(2)? as u64,
                payload_json: row.get(3)?,
                created_at_ms: row.get(4)?,
            })
        },
    )?;
    rows.collect()
}

pub fn cancel_run(
    connection: &mut Connection,
    run_id: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let tx = connection.transaction()?;
    let changed = tx.execute(
        "UPDATE automation_runs SET state='cancelled', updated_at_ms=?1 WHERE run_id=?2 AND state IN ('admitted','queued','starting','running','waiting_approval','retrying','paused','cancelling')",
        params![now_ms, run_id],
    )?;
    if changed == 0 {
        tx.rollback()?;
        return Ok(false);
    }
    let sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(run_sequence), -1) + 1 FROM automation_run_events WHERE run_id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO automation_run_events (run_id, run_sequence, event_type, generation, payload_json, created_at_ms) SELECT run_id, ?1, 'cancelled', generation, '{}', ?2 FROM automation_runs WHERE run_id=?3",
        params![sequence, now_ms, run_id],
    )?;
    tx.commit()?;
    Ok(true)
}

pub fn set_schedule_enabled(
    connection: &Connection,
    schedule_id: &str,
    enabled: bool,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    Ok(connection.execute(
        "UPDATE automation_schedules SET enabled=?1, updated_at_ms=?2 WHERE schedule_id=?3",
        params![enabled as i64, now_ms, schedule_id],
    )? == 1)
}

pub fn insert_run(
    connection: &Connection,
    record: &AutomationRunRecord,
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO automation_runs (run_id, definition_id, revision, owner_scope, idempotency_key, payload_hash, state, generation, permission_snapshot, approval_snapshot, created_at_ms, updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)", params![record.run_id, record.definition_id, record.revision as i64, record.owner_scope, record.idempotency_key, record.payload_hash, record.state, record.generation as i64, record.permission_snapshot, record.approval_snapshot, now_ms])?;
    Ok(())
}

/// Atomically applies trigger idempotency.  Callers must compare the payload
/// hash before treating a repeated key as the same request.
pub fn admit_run(
    connection: &Connection,
    record: &AutomationRunRecord,
    now_ms: i64,
) -> rusqlite::Result<AdmitRunResult> {
    if let Some(existing) = find_run_by_idempotency(
        connection,
        &record.owner_scope,
        &record.definition_id,
        record.revision,
        &record.idempotency_key,
    )? {
        return if existing.payload_hash == record.payload_hash {
            Ok(AdmitRunResult::Existing(existing))
        } else {
            Ok(AdmitRunResult::IdempotencyConflict {
                existing_payload_hash: existing.payload_hash,
            })
        };
    }
    insert_run(connection, record, now_ms)?;
    Ok(AdmitRunResult::Inserted)
}

/// Fenced durable transition. The guarded UPDATE and event insert share one
/// SQLite transaction, so stale runners cannot publish a transition.
pub fn transition_run(
    connection: &mut Connection,
    transition: RunTransition<'_>,
) -> rusqlite::Result<bool> {
    let tx = connection.transaction()?;
    let changed = tx.execute("UPDATE automation_runs SET state=?1, updated_at_ms=?2 WHERE run_id=?3 AND state=?4 AND generation=?5", params![transition.to_state, transition.now_ms, transition.run_id, transition.from_state, transition.generation as i64])?;
    if changed == 0 {
        tx.rollback()?;
        return Ok(false);
    }
    let sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(run_sequence), -1) + 1 FROM automation_run_events WHERE run_id=?1",
        [transition.run_id],
        |row| row.get(0),
    )?;
    tx.execute("INSERT INTO automation_run_events (run_id, run_sequence, event_type, generation, payload_json, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6)", params![transition.run_id, sequence, transition.event_type, transition.generation as i64, transition.payload_json, transition.now_ms])?;
    tx.commit()?;
    Ok(true)
}

pub fn acquire_lease(
    connection: &Connection,
    run_id: &str,
    owner_id: &str,
    generation: u64,
    now_ms: i64,
    ttl_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = connection.execute("INSERT INTO automation_leases (run_id, owner_id, generation, expires_at_ms) VALUES (?1,?2,?3,?4) ON CONFLICT(run_id) DO UPDATE SET owner_id=excluded.owner_id, generation=excluded.generation, expires_at_ms=excluded.expires_at_ms WHERE automation_leases.expires_at_ms <= ?5", params![run_id, owner_id, generation as i64, now_ms + ttl_ms, now_ms])?;
    Ok(changed == 1)
}

pub fn save_snapshot(
    connection: &Connection,
    snapshot: SnapshotInsert<'_>,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO automation_snapshots (snapshot_id, run_id, definition_revision, generation, event_sequence, snapshot_json, checksum_sha256, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![snapshot.snapshot_id, snapshot.run_id, snapshot.definition_revision as i64, snapshot.generation as i64, snapshot.event_sequence as i64, snapshot.snapshot_json, snapshot.checksum_sha256, snapshot.now_ms])?;
    Ok(())
}

fn map_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRunRecord> {
    Ok(AutomationRunRecord {
        run_id: row.get(0)?,
        definition_id: row.get(1)?,
        revision: row.get::<_, i64>(2)? as u64,
        owner_scope: row.get(3)?,
        idempotency_key: row.get(4)?,
        payload_hash: row.get(5)?,
        state: row.get(6)?,
        generation: row.get::<_, i64>(7)? as u64,
        permission_snapshot: row.get(8)?,
        approval_snapshot: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idempotency_is_durable_and_scoped() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let d = AutomationDefinitionRecord {
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            definition_json: "{}".into(),
            definition_hash: "h".into(),
        };
        insert_definition(&c, &d, 1).unwrap();
        let r = AutomationRunRecord {
            run_id: "run".into(),
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            idempotency_key: "k".into(),
            payload_hash: "p".into(),
            state: "admitted".into(),
            generation: 1,
            permission_snapshot: "ps".into(),
            approval_snapshot: "as".into(),
        };
        insert_run(&c, &r, 1).unwrap();
        assert_eq!(
            find_run_by_idempotency(&c, "o", "d", 1, "k")
                .unwrap()
                .unwrap(),
            r
        );
        assert!(find_run_by_idempotency(&c, "other", "d", 1, "k")
            .unwrap()
            .is_none());
        assert!(matches!(
            admit_run(&c, &r, 2).unwrap(),
            AdmitRunResult::Existing(_)
        ));
        let mut conflict = r.clone();
        conflict.payload_hash = "different".into();
        assert!(matches!(
            admit_run(&c, &conflict, 2).unwrap(),
            AdmitRunResult::IdempotencyConflict { .. }
        ));
        let mut c = c;
        assert!(transition_run(
            &mut c,
            RunTransition {
                run_id: "run",
                from_state: "admitted",
                to_state: "queued",
                generation: 1,
                event_type: "queued",
                payload_json: "{}",
                now_ms: 2,
            },
        )
        .unwrap());
        assert!(!transition_run(
            &mut c,
            RunTransition {
                run_id: "run",
                from_state: "queued",
                to_state: "running",
                generation: 0,
                event_type: "running",
                payload_json: "{}",
                now_ms: 3,
            },
        )
        .unwrap());
        assert!(acquire_lease(&c, "run", "core-a", 1, 10, 30).unwrap());
        assert!(!acquire_lease(&c, "run", "core-b", 2, 20, 30).unwrap());
        assert!(acquire_lease(&c, "run", "core-b", 2, 40, 30).unwrap());
    }

    #[test]
    fn schedule_cursor_is_durable_and_compare_and_swap_fenced() {
        let c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let definition = AutomationDefinitionRecord {
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            definition_json: "{}".into(),
            definition_hash: "h".into(),
        };
        insert_definition(&c, &definition, 1).unwrap();
        let schedule = AutomationScheduleRecord {
            schedule_id: "s".into(),
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            hour: 12,
            minute: 0,
            timezone_minutes: 0,
            missed_grace_ms: 60_000,
            enabled: true,
            last_slot: None,
        };
        upsert_schedule(&c, &schedule, 1).unwrap();
        assert!(advance_schedule_slot(&c, "s", None, "slot-1", 2).unwrap());
        assert!(!advance_schedule_slot(&c, "s", None, "slot-2", 3).unwrap());
        assert!(!advance_schedule_slot(&c, "s", Some("old"), "slot-2", 3).unwrap());
        assert!(advance_schedule_slot(&c, "s", Some("slot-1"), "slot-2", 4).unwrap());
        assert_eq!(
            get_schedule(&c, "s").unwrap().unwrap().last_slot.as_deref(),
            Some("slot-2")
        );
    }

    #[test]
    fn archive_restore_is_atomic_checksum_verified_and_retention_bounded() {
        let mut c = Connection::open_in_memory().unwrap();
        install_schema(&c).unwrap();
        let record = AutomationRunRecord {
            run_id: "run-archive".into(),
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            idempotency_key: "k".into(),
            payload_hash: "p".into(),
            state: "completed".into(),
            generation: 1,
            permission_snapshot: "ps".into(),
            approval_snapshot: "as".into(),
        };
        insert_run(&c, &record, 10).unwrap();
        save_snapshot(
            &c,
            SnapshotInsert {
                snapshot_id: "snapshot-1",
                run_id: "run-archive",
                definition_revision: 1,
                generation: 1,
                event_sequence: 0,
                snapshot_json: "{}",
                checksum_sha256: "checksum",
                now_ms: 11,
            },
        )
        .unwrap();
        assert!(archive_run(&mut c, "archive-1", "run-archive", 20, 100).unwrap());
        assert!(get_run(&c, "run-archive").unwrap().is_none());
        assert!(restore_archive(&mut c, "archive-1", 30).unwrap());
        assert_eq!(get_run(&c, "run-archive").unwrap().unwrap(), record);
        assert_eq!(
            c.query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM automation_snapshots WHERE run_id='run-archive'",
                [],
                |row| row.get(0)
            )
            .unwrap(),
            1
        );
        assert!(!restore_archive(&mut c, "archive-1", 31).unwrap());
        assert_eq!(sweep_expired_archives(&c, 100).unwrap(), 1);
    }
}
