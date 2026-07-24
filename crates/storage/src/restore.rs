//! Idempotent restore of a `BackupDump` into PostgreSQL (Stage 7.99, wave 2).
//!
//! Sessions and memory items are skipped when their `id` already exists;
//! a skipped session skips all of its messages/tasks/steps/events, so the
//! restore stays idempotent even though `session_messages` has no portable id.
//! Everything runs inside one transaction — a failure restores nothing.

use crate::backup::{BackupDump, BackupSession};
use crate::StorageError;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

const SUPPORTED_FORMAT: &str = "evohime-backup";
const SUPPORTED_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct RestoreReport {
    pub sessions_inserted: u64,
    pub sessions_skipped: u64,
    pub messages_inserted: u64,
    pub tasks_inserted: u64,
    pub steps_inserted: u64,
    pub events_inserted: u64,
    pub memory_inserted: u64,
    pub memory_skipped: u64,
}

pub fn validate_backup_header(dump: &BackupDump) -> Result<(), StorageError> {
    if dump.format != SUPPORTED_FORMAT {
        return Err(StorageError::InvalidSync(format!(
            "unsupported backup format: {}",
            dump.format
        )));
    }
    if dump.version != SUPPORTED_VERSION {
        return Err(StorageError::InvalidSync(format!(
            "unsupported backup version: {}",
            dump.version
        )));
    }
    Ok(())
}

pub async fn restore_backup(
    pool: &PgPool,
    operator_id: Uuid,
    dump: &BackupDump,
) -> Result<RestoreReport, StorageError> {
    validate_backup_header(dump)?;
    let mut tx = pool.begin().await?;
    let mut report = RestoreReport::default();

    for session in &dump.sessions {
        restore_session(&mut tx, operator_id, session, &mut report).await?;
    }
    restore_memory_items(&mut tx, operator_id, dump, &mut report).await?;

    tx.commit().await?;
    Ok(report)
}

async fn restore_session(
    conn: &mut PgConnection,
    operator_id: Uuid,
    session: &BackupSession,
    report: &mut RestoreReport,
) -> Result<(), StorageError> {
    let inserted = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO sessions (id, created_at, archived_at, title, workspace_path, operator_id)
         VALUES ($1, $2, CASE WHEN $3 THEN $2 ELSE NULL END, $4, $5, $6)
         ON CONFLICT (id) DO NOTHING
         RETURNING id",
    )
    .bind(session.id)
    .bind(session.created_at)
    .bind(session.archived)
    .bind(&session.title)
    .bind(&session.workspace_path)
    .bind(operator_id)
    .fetch_optional(&mut *conn)
    .await?;
    if inserted.is_none() {
        report.sessions_skipped += 1;
        return Ok(());
    }
    report.sessions_inserted += 1;

    for task in &session.tasks {
        sqlx::query(
            "INSERT INTO tasks (id, session_id, user_message, model_route, model, workspace_path, status, created_at, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(task.task.id)
        .bind(session.id)
        .bind(&task.task.user_message)
        .bind(&task.task.model_route)
        .bind(&task.task.model)
        .bind(&task.task.workspace_path)
        .bind(&task.task.status)
        .bind(task.task.created_at)
        .bind(task.task.completed_at)
        .execute(&mut *conn)
        .await?;
        report.tasks_inserted += 1;

        for step in &task.steps {
            sqlx::query(
                "INSERT INTO task_steps (id, task_id, step_index, tool_name, input_json, depends_on, status, output, error)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(step.id)
            .bind(task.task.id)
            .bind(step.step_index)
            .bind(&step.tool_name)
            .bind(&step.input_json)
            .bind(&step.depends_on)
            .bind(&step.status)
            .bind(&step.output)
            .bind(&step.error)
            .execute(&mut *conn)
            .await?;
            report.steps_inserted += 1;
        }
    }

    for message in &session.messages {
        sqlx::query(
            "INSERT INTO session_messages (session_id, role, content, created_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(session.id)
        .bind(&message.role)
        .bind(&message.content)
        .bind(message.created_at)
        .execute(&mut *conn)
        .await?;
        report.messages_inserted += 1;
    }

    for event in &session.events {
        sqlx::query(
            "INSERT INTO session_events (session_id, sequence, event_json, created_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (session_id, sequence) DO NOTHING",
        )
        .bind(session.id)
        .bind(event.sequence)
        .bind(&event.event_json)
        .bind(event.created_at)
        .execute(&mut *conn)
        .await?;
        report.events_inserted += 1;
    }

    Ok(())
}

async fn restore_memory_items(
    conn: &mut PgConnection,
    operator_id: Uuid,
    dump: &BackupDump,
    report: &mut RestoreReport,
) -> Result<(), StorageError> {
    for item in &dump.memory_items {
        // Source refs survive only when the target row exists; the scalar
        // subquery yields NULL otherwise, matching ON DELETE SET NULL semantics.
        let inserted = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO memory_items (
                id, operator_id, scope, scope_key, kind, status, content, content_json,
                confidence, importance, pinned,
                source_session_id, source_task_id, source_label,
                valid_until, validity_hint, last_used_at, use_count,
                helpful_count, harmful_count, embedding, embedding_version,
                created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                (SELECT id FROM sessions WHERE id = $12),
                (SELECT id FROM tasks WHERE id = $13),
                $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)
             ON CONFLICT (id) DO NOTHING
             RETURNING id",
        )
        .bind(item.id)
        .bind(operator_id)
        .bind(&item.scope)
        .bind(&item.scope_key)
        .bind(&item.kind)
        .bind(&item.status)
        .bind(&item.content)
        .bind(&item.content_json)
        .bind(item.confidence)
        .bind(item.importance)
        .bind(item.pinned)
        .bind(item.source_session_id)
        .bind(item.source_task_id)
        .bind(&item.source_label)
        .bind(item.valid_until)
        .bind(&item.validity_hint)
        .bind(item.last_used_at)
        .bind(item.use_count)
        .bind(item.helpful_count)
        .bind(item.harmful_count)
        .bind(&item.embedding)
        .bind(item.embedding_version)
        .bind(item.created_at)
        .bind(item.updated_at)
        .fetch_optional(&mut *conn)
        .await?;
        if inserted.is_some() {
            report.memory_inserted += 1;
        } else {
            report.memory_skipped += 1;
        }
    }

    // Second pass: supersedes chains may point at items inserted later
    // in the same dump, so they are linked only after every insert.
    for item in &dump.memory_items {
        if let Some(supersedes) = item.supersedes {
            sqlx::query(
                "UPDATE memory_items
                 SET supersedes = (SELECT id FROM memory_items WHERE id = $2)
                 WHERE id = $1 AND supersedes IS NULL",
            )
            .bind(item.id)
            .bind(supersedes)
            .execute(&mut *conn)
            .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_validation_rejects_unknown_format_and_version() {
        let dump = BackupDump::empty();
        assert!(validate_backup_header(&dump).is_ok());

        let mut wrong_format = BackupDump::empty();
        wrong_format.format = "other".into();
        assert!(validate_backup_header(&wrong_format).is_err());

        let mut wrong_version = BackupDump::empty();
        wrong_version.version = 99;
        assert!(validate_backup_header(&wrong_version).is_err());
    }

    #[tokio::test]
    async fn empty_dump_restores_to_zero_report() {
        let Some(pool) = crate::connect_integration_pool().await else {
            eprintln!("skipping empty restore test: database unavailable");
            return;
        };
        let report = restore_backup(&pool, crate::BOOTSTRAP_OWNER_ID, &BackupDump::empty())
            .await
            .expect("restore empty");
        assert_eq!(report, RestoreReport::default());
    }

    #[tokio::test]
    async fn round_trip_restore_is_idempotent_and_rescopes_operator() {
        let Some(pool) = crate::connect_integration_pool().await else {
            eprintln!("skipping round-trip restore test: database unavailable");
            return;
        };
        let (source, _) = crate::create_operator(
            &pool,
            &format!("restore-src-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("source operator");
        let (target, _) = crate::create_operator(
            &pool,
            &format!("restore-dst-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("target operator");

        let session = crate::create_session_for_operator(&pool, source.id)
            .await
            .expect("session");
        crate::insert_message(&pool, session.id, None, "user", "hello backup")
            .await
            .expect("message");
        let item = crate::insert_memory_item(
            &pool,
            &crate::NewMemoryItem {
                operator_id: source.id,
                scope: crate::MemoryScope::Global,
                scope_key: format!("restore-test-{}", Uuid::new_v4()),
                kind: crate::MemoryKind::Fact,
                status: crate::MemoryStatus::Active,
                content: "restored fact".into(),
                content_json: None,
                confidence: 0.9,
                importance: 0.5,
                pinned: false,
                source_session_id: Some(session.id),
                source_task_id: None,
                source_label: None,
                supersedes: None,
                valid_until: None,
                validity_hint: None,
                embedding: None,
                embedding_version: 0,
            },
        )
        .await
        .expect("memory item");

        let dump = crate::collect_backup(&pool, source.id).await.expect("dump");
        assert_eq!(dump.sessions.len(), 1);
        assert_eq!(dump.memory_items.len(), 1);

        // First restore into the same database: ids collide, everything skipped.
        let skipped = restore_backup(&pool, target.id, &dump)
            .await
            .expect("skip restore");
        assert_eq!(skipped.sessions_inserted, 0);
        assert_eq!(skipped.sessions_skipped, 1);
        assert_eq!(skipped.messages_inserted, 0);
        assert_eq!(skipped.memory_inserted, 0);
        assert_eq!(skipped.memory_skipped, 1);

        // Simulate the other machine: drop the source rows, then restore.
        crate::delete_session_for_operator(&pool, source.id, session.id)
            .await
            .expect("delete session");
        crate::delete_memory_item_for_operator(&pool, source.id, item.id)
            .await
            .expect("delete memory");

        let restored = restore_backup(&pool, target.id, &dump)
            .await
            .expect("restore");
        assert_eq!(restored.sessions_inserted, 1);
        assert_eq!(restored.messages_inserted, 1);
        assert_eq!(restored.memory_inserted, 1);

        let sessions = crate::list_sessions_for_operator(&pool, target.id, 200)
            .await
            .expect("target sessions");
        assert!(sessions.iter().any(|row| row.id == session.id));
        let restored_item = crate::get_memory_item_for_operator(&pool, target.id, item.id)
            .await
            .expect("target memory")
            .expect("memory row");
        assert_eq!(restored_item.operator_id, target.id);
        assert_eq!(restored_item.source_session_id, Some(session.id));

        // Second run over restored data: fully idempotent.
        let again = restore_backup(&pool, target.id, &dump)
            .await
            .expect("restore again");
        assert_eq!(again.sessions_inserted, 0);
        assert_eq!(again.sessions_skipped, 1);
        assert_eq!(again.memory_inserted, 0);
        assert_eq!(again.memory_skipped, 1);
    }

    #[tokio::test]
    async fn missing_memory_source_refs_become_null() {
        let Some(pool) = crate::connect_integration_pool().await else {
            eprintln!("skipping ref-null restore test: database unavailable");
            return;
        };
        let (target, _) = crate::create_operator(
            &pool,
            &format!("restore-null-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("target operator");

        let mut dump = BackupDump::empty();
        let orphan_id = Uuid::new_v4();
        dump.memory_items.push(crate::MemoryItemRow {
            id: orphan_id,
            operator_id: Uuid::new_v4(),
            scope: "global".into(),
            scope_key: format!("orphan-{orphan_id}"),
            kind: "fact".into(),
            status: "active".into(),
            content: "orphan fact".into(),
            content_json: None,
            confidence: 0.5,
            importance: 0.5,
            pinned: false,
            source_session_id: Some(Uuid::new_v4()),
            source_task_id: Some(Uuid::new_v4()),
            source_label: None,
            supersedes: Some(Uuid::new_v4()),
            valid_until: None,
            validity_hint: None,
            last_used_at: None,
            use_count: 0,
            helpful_count: 0,
            harmful_count: 0,
            embedding: None,
            embedding_version: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });

        let report = restore_backup(&pool, target.id, &dump)
            .await
            .expect("restore orphan");
        assert_eq!(report.memory_inserted, 1);
        let row = crate::get_memory_item_for_operator(&pool, target.id, orphan_id)
            .await
            .expect("orphan lookup")
            .expect("orphan row");
        assert_eq!(row.source_session_id, None);
        assert_eq!(row.source_task_id, None);
        assert_eq!(row.supersedes, None);
    }
}
