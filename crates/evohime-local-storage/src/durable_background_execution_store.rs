//! Supplemental durable storage for plan 132.
//!
//! `automation_runs` and its event history remain the source of truth for run
//! lifecycle.  This module only adds bounded background snapshots, queues,
//! waits, wakeups and immutable dispatch attempts.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_JSON_BYTES: usize = 64 * 1024;
pub const MAX_QUEUE_ROWS: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRecord {
    pub queue_id: String,
    pub owner_scope: String,
    pub revision: u64,
    pub max_active: u32,
    pub max_queued: u32,
    pub priority: String,
    pub overflow_policy: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitRecord {
    pub run_id: String,
    pub revision: u64,
    pub condition_json: Vec<u8>,
    pub wake_at_ms: Option<i64>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptRecord {
    pub attempt_id: String,
    pub run_id: String,
    pub generation: u64,
    pub dispatcher_id: String,
    pub state: String,
    pub outcome_code: String,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundScheduleRecord {
    pub schedule_id: String,
    pub definition_id: String,
    pub revision: u64,
    pub owner_scope: String,
    pub spec_json: String,
    pub missed_fire_policy: String,
    pub overlap_policy: String,
    pub enabled: bool,
    pub last_slot: Option<String>,
    pub hour: u8,
    pub minute: u8,
    pub timezone_minutes: i32,
}

pub fn install_schema(c: &Connection) -> rusqlite::Result<()> {
    for (name, definition) in [
        (
            "background_kind",
            "TEXT NOT NULL DEFAULT 'registered_background_task'",
        ),
        ("background_snapshot_json", "BLOB NOT NULL DEFAULT '{}'"),
        ("background_next_wakeup_at_ms", "INTEGER"),
        (
            "background_queue_ref",
            "TEXT NOT NULL DEFAULT 'workflow-default'",
        ),
        ("background_concurrency_key", "TEXT"),
        (
            "background_priority",
            "TEXT NOT NULL DEFAULT 'background_workflow'",
        ),
        ("background_content_hash", "TEXT NOT NULL DEFAULT ''"),
    ] {
        let exists: bool = c.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('automation_runs') WHERE name=?1",
            [name],
            |row| row.get(0),
        )?;
        if !exists {
            c.execute(
                &format!("ALTER TABLE automation_runs ADD COLUMN {name} {definition}"),
                [],
            )?;
        }
    }
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS automation_queues (
            queue_id TEXT PRIMARY KEY NOT NULL, owner_scope TEXT NOT NULL,
            revision INTEGER NOT NULL, max_active INTEGER NOT NULL,
            max_queued INTEGER NOT NULL, priority TEXT NOT NULL,
            overflow_policy TEXT NOT NULL, content_hash TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS automation_waits (
            run_id TEXT PRIMARY KEY NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE,
            revision INTEGER NOT NULL, condition_json BLOB NOT NULL,
            wake_at_ms INTEGER, state TEXT NOT NULL, updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS automation_wakeups (
            wake_key TEXT PRIMARY KEY NOT NULL, run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE,
            wake_at_ms INTEGER NOT NULL, kind TEXT NOT NULL, active INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_automation_wakeups_due ON automation_wakeups(active, wake_at_ms);
        CREATE TABLE IF NOT EXISTS automation_attempts (
            attempt_id TEXT PRIMARY KEY NOT NULL, run_id TEXT NOT NULL REFERENCES automation_runs(run_id) ON DELETE CASCADE,
            generation INTEGER NOT NULL, dispatcher_id TEXT NOT NULL, state TEXT NOT NULL,
            outcome_code TEXT NOT NULL, started_at_ms INTEGER, ended_at_ms INTEGER,
            created_at_ms INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_automation_attempts_run ON automation_attempts(run_id, created_at_ms);
        CREATE INDEX IF NOT EXISTS idx_automation_attempts_active ON automation_attempts(state, created_at_ms);",
    )?;
    Ok(())
}

fn bounded(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= MAX_JSON_BYTES
}

#[allow(clippy::too_many_arguments)]
pub fn save_run_metadata(
    c: &Connection,
    run_id: &str,
    kind: &str,
    snapshot: &[u8],
    queue_ref: &str,
    priority: &str,
    concurrency_key: Option<&str>,
    content_hash: &str,
    next_wakeup_at_ms: Option<i64>,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    if !bounded(snapshot) || run_id.is_empty() || queue_ref.is_empty() || priority.is_empty() {
        return Ok(false);
    }
    Ok(c.execute(
        "UPDATE automation_runs SET background_kind=?1, background_snapshot_json=?2, background_queue_ref=?3, background_priority=?4, background_concurrency_key=?5, background_content_hash=?6, background_next_wakeup_at_ms=?7, updated_at_ms=?8 WHERE run_id=?9",
        params![kind, snapshot, queue_ref, priority, concurrency_key, content_hash, next_wakeup_at_ms, now_ms, run_id],
    )? == 1)
}

pub fn get_queue(
    c: &Connection,
    queue_id: &str,
    owner_scope: &str,
) -> rusqlite::Result<Option<QueueRecord>> {
    c.query_row(
        "SELECT queue_id,owner_scope,revision,max_active,max_queued,priority,overflow_policy,content_hash FROM automation_queues WHERE queue_id=?1 AND owner_scope=?2",
        params![queue_id, owner_scope],
        |row| Ok(QueueRecord { queue_id: row.get(0)?, owner_scope: row.get(1)?, revision: row.get::<_, i64>(2)? as u64, max_active: row.get::<_, i64>(3)? as u32, max_queued: row.get::<_, i64>(4)? as u32, priority: row.get(5)?, overflow_policy: row.get(6)?, content_hash: row.get(7)? }),
    ).optional()
}

pub fn queue_load(
    c: &Connection,
    queue_id: &str,
    owner_scope: &str,
) -> rusqlite::Result<(u32, u32)> {
    c.query_row(
        "SELECT COALESCE(SUM(CASE WHEN state='queued' THEN 1 ELSE 0 END),0), COALESCE(SUM(CASE WHEN state IN ('dispatching','running','waiting','retrying') THEN 1 ELSE 0 END),0) FROM automation_runs WHERE owner_scope=?1 AND background_queue_ref=?2",
        params![owner_scope, queue_id],
        |row| Ok((row.get::<_, i64>(0)? as u32, row.get::<_, i64>(1)? as u32)),
    )
}

pub fn drop_oldest_queued(
    c: &mut Connection,
    queue_id: &str,
    owner_scope: &str,
    now_ms: i64,
) -> rusqlite::Result<Option<String>> {
    let run_id: Option<String> = c.query_row(
        "SELECT run_id FROM automation_runs WHERE owner_scope=?1 AND background_queue_ref=?2 AND state='queued' ORDER BY updated_at_ms,run_id LIMIT 1",
        params![owner_scope, queue_id],
        |row| row.get(0),
    ).optional()?;
    let Some(run_id) = run_id else {
        return Ok(None);
    };
    let generation = crate::automation_store::get_run(c, &run_id)?
        .map(|run| run.generation)
        .unwrap_or(0);
    let changed = crate::automation_store::transition_run(
        c,
        crate::automation_store::RunTransition {
            run_id: &run_id,
            from_state: "queued",
            to_state: "dead_lettered",
            generation,
            event_type: "background.queue_overflow",
            payload_json: "{}",
            now_ms,
        },
    )?;
    Ok(changed.then_some(run_id))
}

pub fn find_queued_by_concurrency_key(
    c: &Connection,
    queue_id: &str,
    owner_scope: &str,
    concurrency_key: &str,
) -> rusqlite::Result<Option<String>> {
    c.query_row(
        "SELECT run_id FROM automation_runs WHERE owner_scope=?1 AND background_queue_ref=?2 AND background_concurrency_key=?3 AND state='queued' ORDER BY updated_at_ms,run_id LIMIT 1",
        params![owner_scope, queue_id, concurrency_key],
        |row| row.get(0),
    ).optional()
}

pub fn find_active_by_concurrency_key(
    c: &Connection,
    queue_id: &str,
    owner_scope: &str,
    concurrency_key: &str,
) -> rusqlite::Result<Option<String>> {
    c.query_row(
        "SELECT run_id FROM automation_runs WHERE owner_scope=?1 AND background_queue_ref=?2 AND background_concurrency_key=?3 AND state IN ('admitted','queued','dispatching','running','waiting','retrying') ORDER BY updated_at_ms,run_id LIMIT 1",
        params![owner_scope, queue_id, concurrency_key],
        |row| row.get(0),
    ).optional()
}

pub fn save_queue(c: &Connection, record: &QueueRecord, now_ms: i64) -> rusqlite::Result<bool> {
    if record.queue_id.is_empty()
        || record.owner_scope.is_empty()
        || record.revision == 0
        || record.max_active == 0
        || record.max_queued > MAX_QUEUE_ROWS
        || record.content_hash.is_empty()
    {
        return Ok(false);
    }
    Ok(c.execute(
        "INSERT INTO automation_queues(queue_id,owner_scope,revision,max_active,max_queued,priority,overflow_policy,content_hash,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(queue_id) DO UPDATE SET owner_scope=excluded.owner_scope,revision=excluded.revision,max_active=excluded.max_active,max_queued=excluded.max_queued,priority=excluded.priority,overflow_policy=excluded.overflow_policy,content_hash=excluded.content_hash,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > automation_queues.revision",
        params![record.queue_id, record.owner_scope, record.revision as i64, record.max_active as i64, record.max_queued as i64, record.priority, record.overflow_policy, record.content_hash, now_ms],
    )? == 1)
}

pub fn list_queues(
    c: &Connection,
    owner_scope: &str,
    limit: u32,
) -> rusqlite::Result<Vec<QueueRecord>> {
    let mut s = c.prepare("SELECT queue_id,owner_scope,revision,max_active,max_queued,priority,overflow_policy,content_hash FROM automation_queues WHERE (?1='' OR owner_scope=?1) ORDER BY queue_id LIMIT ?2")?;
    let rows = s.query_map(params![owner_scope, limit.clamp(1, 256)], |row| {
        Ok(QueueRecord {
            queue_id: row.get(0)?,
            owner_scope: row.get(1)?,
            revision: row.get::<_, i64>(2)? as u64,
            max_active: row.get::<_, i64>(3)? as u32,
            max_queued: row.get::<_, i64>(4)? as u32,
            priority: row.get(5)?,
            overflow_policy: row.get(6)?,
            content_hash: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn put_wait(c: &mut Connection, record: &WaitRecord, now_ms: i64) -> rusqlite::Result<bool> {
    if !bounded(&record.condition_json) || record.run_id.is_empty() || record.revision == 0 {
        return Ok(false);
    }
    let tx = c.transaction()?;
    let changed = tx.execute("INSERT INTO automation_waits(run_id,revision,condition_json,wake_at_ms,state,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(run_id) DO UPDATE SET revision=excluded.revision,condition_json=excluded.condition_json,wake_at_ms=excluded.wake_at_ms,state=excluded.state,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision >= automation_waits.revision", params![record.run_id, record.revision as i64, &record.condition_json, record.wake_at_ms, record.state, now_ms])?;
    if changed == 1 {
        if let Some(wake) = record.wake_at_ms {
            tx.execute("INSERT INTO automation_wakeups(wake_key,run_id,wake_at_ms,kind,active,created_at_ms) VALUES(?1,?2,?3,'wait',1,?4) ON CONFLICT(wake_key) DO UPDATE SET run_id=excluded.run_id,wake_at_ms=excluded.wake_at_ms,kind=excluded.kind,active=excluded.active,created_at_ms=excluded.created_at_ms", params![format!("wait:{}:{}", record.run_id, record.revision), record.run_id, wake, now_ms])?;
        }
    }
    tx.commit()?;
    Ok(changed == 1)
}

pub fn due_wakeups(
    c: &Connection,
    now_ms: i64,
    limit: u32,
) -> rusqlite::Result<Vec<(String, String)>> {
    let mut s = c.prepare("SELECT wake_key,run_id FROM automation_wakeups WHERE active=1 AND wake_at_ms<=?1 ORDER BY wake_at_ms,wake_key LIMIT ?2")?;
    let rows = s.query_map(params![now_ms, limit.clamp(1, 256)], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;
    rows.collect()
}

pub fn list_waits(c: &Connection, limit: u32) -> rusqlite::Result<Vec<WaitRecord>> {
    let mut s = c.prepare("SELECT run_id,revision,condition_json,wake_at_ms,state FROM automation_waits WHERE state IN ('waiting','scheduled') ORDER BY COALESCE(wake_at_ms,9223372036854775807),run_id LIMIT ?1")?;
    let rows = s.query_map([limit.clamp(1, 256)], |row| {
        Ok(WaitRecord {
            run_id: row.get(0)?,
            revision: row.get::<_, i64>(1)? as u64,
            condition_json: row.get(2)?,
            wake_at_ms: row.get(3)?,
            state: row.get(4)?,
        })
    })?;
    rows.collect()
}

pub fn complete_wait(
    c: &Connection,
    run_id: &str,
    revision: u64,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let changed = c.execute("UPDATE automation_waits SET state='satisfied',updated_at_ms=?1 WHERE run_id=?2 AND revision=?3 AND state IN ('waiting','scheduled')", params![now_ms, run_id, revision as i64])?;
    c.execute(
        "UPDATE automation_wakeups SET active=0,created_at_ms=?1 WHERE run_id=?2 AND active=1",
        params![now_ms, run_id],
    )?;
    Ok(changed == 1)
}

pub fn mark_wakeup_consumed(c: &Connection, wake_key: &str, now_ms: i64) -> rusqlite::Result<bool> {
    Ok(c.execute(
        "UPDATE automation_wakeups SET active=0,created_at_ms=?1 WHERE wake_key=?2 AND active=1",
        params![now_ms, wake_key],
    )? == 1)
}

pub fn insert_attempt(
    c: &Connection,
    attempt: &AttemptRecord,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    if attempt.attempt_id.is_empty()
        || attempt.run_id.is_empty()
        || attempt.dispatcher_id.is_empty()
        || attempt.generation == 0
    {
        return Ok(false);
    }
    Ok(c.execute("INSERT OR IGNORE INTO automation_attempts(attempt_id,run_id,generation,dispatcher_id,state,outcome_code,started_at_ms,ended_at_ms,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![attempt.attempt_id,attempt.run_id,attempt.generation as i64,attempt.dispatcher_id,attempt.state,attempt.outcome_code,attempt.started_at_ms,attempt.ended_at_ms,now_ms])? == 1)
}

pub fn list_attempts(
    c: &Connection,
    run_id: &str,
    limit: u32,
) -> rusqlite::Result<Vec<AttemptRecord>> {
    let mut s = c.prepare("SELECT attempt_id,run_id,generation,dispatcher_id,state,outcome_code,started_at_ms,ended_at_ms FROM automation_attempts WHERE run_id=?1 ORDER BY created_at_ms DESC LIMIT ?2")?;
    let rows = s.query_map(params![run_id, limit.clamp(1, 256)], |row| {
        Ok(AttemptRecord {
            attempt_id: row.get(0)?,
            run_id: row.get(1)?,
            generation: row.get::<_, i64>(2)? as u64,
            dispatcher_id: row.get(3)?,
            state: row.get(4)?,
            outcome_code: row.get(5)?,
            started_at_ms: row.get(6)?,
            ended_at_ms: row.get(7)?,
        })
    })?;
    rows.collect()
}

pub fn reconcile_after_restart(c: &Connection, now_ms: i64) -> rusqlite::Result<u32> {
    Ok(c.execute("UPDATE automation_attempts SET state='reconcile_required', outcome_code='unknown_after_restart', ended_at_ms=?1 WHERE state IN ('dispatching','running')", [now_ms])? as u32)
}

pub fn background_snapshot(c: &Connection, run_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    c.query_row(
        "SELECT background_snapshot_json FROM automation_runs WHERE run_id=?1",
        [run_id],
        |row| row.get(0),
    )
    .optional()
}

pub fn save_background_schedule(
    c: &Connection,
    record: &crate::automation_store::AutomationScheduleRecord,
    spec_json: &str,
    missed_fire_policy: &str,
    overlap_policy: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    if spec_json.is_empty()
        || spec_json.len() > MAX_JSON_BYTES
        || missed_fire_policy.is_empty()
        || overlap_policy.is_empty()
    {
        return Ok(false);
    }
    crate::automation_store::upsert_schedule(c, record, now_ms)?;
    Ok(c.execute(
        "UPDATE automation_schedules SET background_spec_json=?1, background_missed_fire_policy=?2, background_overlap_policy=?3 WHERE schedule_id=?4 AND owner_scope=?5 AND revision=?6",
        params![spec_json, missed_fire_policy, overlap_policy, record.schedule_id, record.owner_scope, record.revision as i64],
    )? == 1)
}

pub fn list_background_schedules(
    c: &Connection,
    owner_scope: &str,
    limit: u32,
) -> rusqlite::Result<Vec<BackgroundScheduleRecord>> {
    let mut statement = c.prepare(
        "SELECT schedule_id,definition_id,revision,owner_scope,background_spec_json,background_missed_fire_policy,background_overlap_policy,enabled,last_slot,hour,minute,timezone_minutes FROM automation_schedules WHERE (?1='' OR owner_scope=?1) AND background_spec_json IS NOT NULL ORDER BY schedule_id LIMIT ?2",
    )?;
    let rows = statement.query_map(params![owner_scope, limit.clamp(1, 256)], |row| {
        Ok(BackgroundScheduleRecord {
            schedule_id: row.get(0)?,
            definition_id: row.get(1)?,
            revision: row.get::<_, i64>(2)? as u64,
            owner_scope: row.get(3)?,
            spec_json: row.get(4)?,
            missed_fire_policy: row.get(5)?,
            overlap_policy: row.get(6)?,
            enabled: row.get::<_, i64>(7)? != 0,
            last_slot: row.get(8)?,
            hour: row.get::<_, i64>(9)? as u8,
            minute: row.get::<_, i64>(10)? as u8,
            timezone_minutes: row.get(11)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation_store::{self, AutomationDefinitionRecord};

    #[test]
    fn metadata_and_waits_are_bounded_and_idempotent() {
        let c = Connection::open_in_memory().unwrap();
        automation_store::install_schema(&c).unwrap();
        install_schema(&c).unwrap();
        automation_store::insert_definition(
            &c,
            &AutomationDefinitionRecord {
                definition_id: "d".into(),
                revision: 1,
                owner_scope: "o".into(),
                definition_json: "{}".into(),
                definition_hash: "h".into(),
            },
            1,
        )
        .unwrap();
        let run = automation_store::AutomationRunRecord {
            run_id: "r".into(),
            definition_id: "d".into(),
            revision: 1,
            owner_scope: "o".into(),
            idempotency_key: "k".into(),
            payload_hash: "p".into(),
            state: "accepted".into(),
            generation: 1,
            permission_snapshot: "p".into(),
            approval_snapshot: "a".into(),
        };
        automation_store::insert_run(&c, &run, 1).unwrap();
        assert!(save_run_metadata(
            &c,
            "r",
            "workflow_run",
            br#"{}"#,
            "q",
            "background_workflow",
            None,
            "hash",
            Some(10),
            1
        )
        .unwrap());
        let mut c = c;
        assert!(put_wait(
            &mut c,
            &WaitRecord {
                run_id: "r".into(),
                revision: 1,
                condition_json: br#"{}"#.to_vec(),
                wake_at_ms: Some(10),
                state: "waiting".into()
            },
            1
        )
        .unwrap());
        assert_eq!(due_wakeups(&c, 10, 10).unwrap().len(), 1);
        assert!(!put_wait(
            &mut c,
            &WaitRecord {
                run_id: "r".into(),
                revision: 0,
                condition_json: br#"{"stale":true}"#.to_vec(),
                wake_at_ms: Some(1),
                state: "waiting".into()
            },
            2
        )
        .unwrap());
        assert_eq!(due_wakeups(&c, 10, 10).unwrap().len(), 1);
        assert!(save_queue(
            &c,
            &QueueRecord {
                queue_id: "q".into(),
                owner_scope: "o".into(),
                revision: 1,
                max_active: 2,
                max_queued: 4,
                priority: "background_workflow".into(),
                overflow_policy: "reject_new".into(),
                content_hash: "hash".into()
            },
            1
        )
        .unwrap());
        assert_eq!(list_queues(&c, "o", 10).unwrap().len(), 1);
        let attempt = AttemptRecord {
            attempt_id: "a".into(),
            run_id: "r".into(),
            generation: 1,
            dispatcher_id: "test".into(),
            state: "blocked".into(),
            outcome_code: "runtime_adapter_unavailable".into(),
            started_at_ms: Some(1),
            ended_at_ms: Some(1),
        };
        assert!(insert_attempt(&c, &attempt, 1).unwrap());
        assert!(!insert_attempt(&c, &attempt, 2).unwrap());
        assert_eq!(reconcile_after_restart(&c, 3).unwrap(), 0);
    }
}
