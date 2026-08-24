//! Durable definition/run boundary for automation contract 16.1.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub const AUTOMATION_STORE_SCHEMA: u32 = 1;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmitRunResult {
    Inserted,
    Existing(AutomationRunRecord),
    IdempotencyConflict { existing_payload_hash: String },
}

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS automation_definitions (definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, definition_json TEXT NOT NULL, definition_hash TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(definition_id, revision, owner_scope)); CREATE TABLE IF NOT EXISTS automation_runs (run_id TEXT PRIMARY KEY, definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, idempotency_key TEXT NOT NULL, payload_hash TEXT NOT NULL, state TEXT NOT NULL, generation INTEGER NOT NULL, permission_snapshot TEXT NOT NULL, approval_snapshot TEXT NOT NULL, created_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL, UNIQUE(owner_scope, definition_id, revision, idempotency_key)); CREATE INDEX IF NOT EXISTS idx_automation_runs_state ON automation_runs(state, updated_at_ms); CREATE TABLE IF NOT EXISTS automation_run_events (run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE, run_sequence INTEGER NOT NULL, event_type TEXT NOT NULL, generation INTEGER NOT NULL, payload_json TEXT NOT NULL, created_at_ms INTEGER NOT NULL, PRIMARY KEY(run_id, run_sequence)); CREATE TABLE IF NOT EXISTS automation_leases (run_id TEXT PRIMARY KEY REFERENCES automation_runs(run_id) ON DELETE CASCADE, owner_id TEXT NOT NULL, generation INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS automation_snapshots (snapshot_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE, definition_revision INTEGER NOT NULL, generation INTEGER NOT NULL, event_sequence INTEGER NOT NULL, snapshot_json TEXT NOT NULL, checksum_sha256 TEXT NOT NULL, created_at_ms INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS automation_schedules (schedule_id TEXT PRIMARY KEY, definition_id TEXT NOT NULL, revision INTEGER NOT NULL, owner_scope TEXT NOT NULL, hour INTEGER NOT NULL, minute INTEGER NOT NULL, timezone_minutes INTEGER NOT NULL, missed_grace_ms INTEGER NOT NULL, enabled INTEGER NOT NULL, last_slot TEXT, updated_at_ms INTEGER NOT NULL, FOREIGN KEY(definition_id, revision, owner_scope) REFERENCES automation_definitions(definition_id, revision, owner_scope));")
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
    run_id: &str,
    from_state: &str,
    to_state: &str,
    generation: u64,
    event_type: &str,
    payload_json: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let tx = connection.transaction()?;
    let changed = tx.execute("UPDATE automation_runs SET state=?1, updated_at_ms=?2 WHERE run_id=?3 AND state=?4 AND generation=?5", params![to_state, now_ms, run_id, from_state, generation as i64])?;
    if changed == 0 {
        tx.rollback()?;
        return Ok(false);
    }
    let sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(run_sequence), -1) + 1 FROM automation_run_events WHERE run_id=?1",
        [run_id],
        |row| row.get(0),
    )?;
    tx.execute("INSERT INTO automation_run_events (run_id, run_sequence, event_type, generation, payload_json, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6)", params![run_id, sequence, event_type, generation as i64, payload_json, now_ms])?;
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
    snapshot_id: &str,
    run_id: &str,
    definition_revision: u64,
    generation: u64,
    event_sequence: u64,
    snapshot_json: &str,
    checksum_sha256: &str,
    now_ms: i64,
) -> rusqlite::Result<()> {
    connection.execute("INSERT INTO automation_snapshots (snapshot_id, run_id, definition_revision, generation, event_sequence, snapshot_json, checksum_sha256, created_at_ms) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![snapshot_id, run_id, definition_revision as i64, generation as i64, event_sequence as i64, snapshot_json, checksum_sha256, now_ms])?;
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
        assert!(transition_run(&mut c, "run", "admitted", "queued", 1, "queued", "{}", 2).unwrap());
        assert!(
            !transition_run(&mut c, "run", "queued", "running", 0, "running", "{}", 3).unwrap()
        );
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
}
