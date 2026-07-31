use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct ReflectionEventRow {
    pub id: i64,
    pub event_id: Uuid,
    pub task_id: Uuid,
    pub tool_call_id: Option<Uuid>,
    pub reflection_type: String,
    pub reflection_action: String,
    pub success_score: sqlx::types::Decimal,
    pub error_patterns: sqlx::types::JsonValue,
    pub confidence: sqlx::types::Decimal,
    pub reasoning: String,
    pub recommendation: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub struct ReflectionEventDAO {
    db: PgPool,
}

impl ReflectionEventDAO {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn insert_reflection_event(
        &self,
        event_id: Uuid,
        task_id: Uuid,
        tool_call_id: Option<Uuid>,
        reflection_type: &str,
        reflection_action: &str,
        success_score: f64,
        error_patterns: &serde_json::Value,
        confidence: f64,
        reasoning: &str,
        recommendation: Option<&str>,
    ) -> Result<ReflectionEventRow, sqlx::Error> {
        sqlx::query_as::<_, ReflectionEventRow>(
            r#"
            INSERT INTO reflection_events (
                event_id, task_id, tool_call_id, reflection_type, reflection_action,
                success_score, error_patterns, confidence, reasoning, recommendation, timestamp
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            RETURNING *
            "#,
        )
        .bind(event_id)
        .bind(task_id)
        .bind(tool_call_id)
        .bind(reflection_type)
        .bind(reflection_action)
        .bind(success_score)
        .bind(error_patterns)
        .bind(confidence)
        .bind(reasoning)
        .bind(recommendation)
        .fetch_one(&self.db)
        .await
    }

    pub async fn get_reflection_events_by_task(
        &self,
        task_id: Uuid,
    ) -> Result<Vec<ReflectionEventRow>, sqlx::Error> {
        sqlx::query_as::<_, ReflectionEventRow>(
            "SELECT * FROM reflection_events WHERE task_id = $1 ORDER BY timestamp ASC"
        )
        .bind(task_id)
        .fetch_all(&self.db)
        .await
    }

    pub async fn get_latest_reflection_before_event(
        &self,
        task_id: Uuid,
        event_id: Uuid,
    ) -> Result<Option<ReflectionEventRow>, sqlx::Error> {
        sqlx::query_as::<_, ReflectionEventRow>(
            r#"
            SELECT * FROM reflection_events
            WHERE task_id = $1 AND event_id < $2
            ORDER BY timestamp DESC LIMIT 1
            "#
        )
        .bind(task_id)
        .bind(event_id)
        .fetch_optional(&self.db)
        .await
    }
}
