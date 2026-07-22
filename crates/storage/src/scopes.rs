#![allow(clippy::items_after_test_module)]

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    #[tokio::test]
    async fn sessions_are_isolated_by_operator_id() {
        let Some(pool) = crate::connect_integration_pool().await else {
            eprintln!("skipping operator scope test: database unavailable");
            return;
        };
        let first = crate::create_operator(
            &pool,
            &format!("scope-a-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("first operator");
        let second = crate::create_operator(
            &pool,
            &format!("scope-b-{}", Uuid::new_v4()),
            crate::OperatorRole::Member,
        )
        .await
        .expect("second operator");
        let session_a = crate::create_session_for_operator(&pool, first.0.id)
            .await
            .expect("session a");
        let session_b = crate::create_session_for_operator(&pool, second.0.id)
            .await
            .expect("session b");

        assert!(
            crate::load_session_for_operator(&pool, first.0.id, session_a.id)
                .await
                .expect("load a")
                .is_some()
        );
        assert!(
            crate::load_session_for_operator(&pool, first.0.id, session_b.id)
                .await
                .expect("load b")
                .is_none()
        );

        sqlx::query("DELETE FROM operators WHERE id = ANY($1)")
            .bind(vec![first.0.id, second.0.id])
            .execute(&pool)
            .await
            .expect("cleanup operators");
    }
}
use crate::{EventRow, MessageRow, SessionSummaryRow, StorageError, TaskRow};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn delete_session_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    session_id: Uuid,
) -> Result<bool, StorageError> {
    Ok(
        sqlx::query("DELETE FROM sessions WHERE id=$1 AND operator_id=$2")
            .bind(session_id)
            .bind(operator_id)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

async fn list_sessions_inner(
    pool: &PgPool,
    operator_id: Uuid,
    archived: bool,
    limit: i64,
) -> Result<Vec<SessionSummaryRow>, StorageError> {
    let clause = if archived { "IS NOT NULL" } else { "IS NULL" };
    Ok(sqlx::query_as::<_, SessionSummaryRow>(&format!("SELECT s.id,s.operator_id,s.created_at,s.title,s.workspace_path,last_message.created_at AS last_message_at,last_message.content AS last_message,last_message.role AS last_role FROM sessions s LEFT JOIN LATERAL (SELECT role,content,created_at FROM session_messages WHERE session_id=s.id ORDER BY created_at DESC LIMIT 1) last_message ON TRUE WHERE s.operator_id=$1 AND s.archived_at {clause} ORDER BY s.created_at DESC LIMIT $2")).bind(operator_id).bind(limit).fetch_all(pool).await?)
}
pub async fn list_sessions_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    limit: i64,
) -> Result<Vec<SessionSummaryRow>, StorageError> {
    list_sessions_inner(pool, operator_id, false, limit).await
}
pub async fn list_archived_sessions_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    limit: i64,
) -> Result<Vec<SessionSummaryRow>, StorageError> {
    list_sessions_inner(pool, operator_id, true, limit).await
}

pub async fn archive_session_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    session_id: Uuid,
) -> Result<bool, StorageError> {
    Ok(sqlx::query("UPDATE sessions SET archived_at=now() WHERE id=$1 AND operator_id=$2 AND archived_at IS NULL").bind(session_id).bind(operator_id).execute(pool).await?.rows_affected() > 0)
}
pub async fn unarchive_session_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    session_id: Uuid,
) -> Result<bool, StorageError> {
    Ok(sqlx::query("UPDATE sessions SET archived_at=NULL WHERE id=$1 AND operator_id=$2 AND archived_at IS NOT NULL").bind(session_id).bind(operator_id).execute(pool).await?.rows_affected() > 0)
}
pub async fn list_session_events_after_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    session_id: Uuid,
    after: i64,
) -> Result<Vec<EventRow>, StorageError> {
    Ok(sqlx::query_as::<_, EventRow>("SELECT e.sequence,e.created_at,e.event_json FROM session_events e JOIN sessions s ON s.id=e.session_id WHERE e.session_id=$1 AND s.operator_id=$2 AND e.sequence>$3 ORDER BY e.sequence ASC").bind(session_id).bind(operator_id).bind(after).fetch_all(pool).await?)
}
pub async fn list_tasks_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    session_id: Option<Uuid>,
) -> Result<Vec<TaskRow>, StorageError> {
    if let Some(session_id) = session_id {
        Ok(sqlx::query_as::<_, TaskRow>("SELECT t.id,t.session_id,t.user_message,t.model_route,t.model,t.workspace_path,t.status,t.created_at,t.completed_at FROM tasks t JOIN sessions s ON s.id=t.session_id WHERE t.session_id=$1 AND s.operator_id=$2 ORDER BY t.created_at DESC").bind(session_id).bind(operator_id).fetch_all(pool).await?)
    } else {
        Ok(sqlx::query_as::<_, TaskRow>("SELECT t.id,t.session_id,t.user_message,t.model_route,t.model,t.workspace_path,t.status,t.created_at,t.completed_at FROM tasks t JOIN sessions s ON s.id=t.session_id WHERE s.operator_id=$1 ORDER BY t.created_at DESC").bind(operator_id).fetch_all(pool).await?)
    }
}
pub async fn load_task_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    task_id: Uuid,
) -> Result<Option<TaskRow>, StorageError> {
    Ok(sqlx::query_as::<_, TaskRow>("SELECT t.id,t.session_id,t.user_message,t.model_route,t.model,t.workspace_path,t.status,t.created_at,t.completed_at FROM tasks t JOIN sessions s ON s.id=t.session_id WHERE t.id=$1 AND s.operator_id=$2").bind(task_id).bind(operator_id).fetch_optional(pool).await?)
}
pub async fn list_session_messages_for_operator(
    pool: &PgPool,
    operator_id: Uuid,
    session_id: Uuid,
) -> Result<Vec<MessageRow>, StorageError> {
    Ok(sqlx::query_as::<_, MessageRow>("SELECT m.role,m.content,m.created_at FROM session_messages m JOIN sessions s ON s.id=m.session_id WHERE m.session_id=$1 AND s.operator_id=$2 ORDER BY m.created_at ASC").bind(session_id).bind(operator_id).fetch_all(pool).await?)
}
