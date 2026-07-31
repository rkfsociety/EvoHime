use chrono::{DateTime, Utc};
use evohime_protocol::planning::PlanCandidate;
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[cfg(test)]
use evohime_protocol::planning::ScoreBreakdown;
#[cfg(test)]
use serde_json::json;

/// Represents a single entry in the planning history for a task
#[derive(Debug, Clone, FromRow)]
pub struct PlanningHistoryEntry {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub candidates: Value, // JSON array of PlanCandidate objects
    pub chosen_plan_id: Option<String>,
    pub reasoning: String,
    pub created_at: DateTime<Utc>,
}

/// Input for inserting a planning history entry
#[derive(Debug, Clone)]
pub struct NewPlanningHistory {
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub candidates: Vec<PlanCandidate>,
    pub chosen_plan_id: Option<String>,
    pub reasoning: String,
}

impl NewPlanningHistory {
    /// Validates and normalizes the entry (truncates reasoning if needed).
    /// Returns Ok(normalized_entry) or Err if validation fails.
    pub fn validate(mut self) -> Result<Self, StorageError> {
        // Validate confidence for all candidates
        for candidate in &self.candidates {
            if !candidate.is_valid() {
                return Err(StorageError::InvalidPlanningHistory(format!(
                    "invalid confidence: {} (must be in [0.0, 1.0])",
                    candidate.confidence
                )));
            }
        }

        // Truncate reasoning to 512 chars (do not reject)
        if self.reasoning.len() > 512 {
            self.reasoning.truncate(512);
        }

        Ok(self)
    }
}

/// Insert a new planning history entry with validation
pub async fn insert_planning_history(
    pool: &PgPool,
    entry: NewPlanningHistory,
) -> Result<PlanningHistoryEntry, StorageError> {
    let validated = entry.validate()?;

    // Serialize candidates to a single JSON array value (the column is a
    // scalar jsonb, not a postgres array of jsonb).
    let candidates_json: Value = Value::Array(
        validated
            .candidates
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or(Value::Null))
            .collect(),
    );

    let row = sqlx::query_as::<_, PlanningHistoryEntry>(
        r#"
        INSERT INTO planning_history (task_id, session_id, candidates, chosen_plan_id, reasoning)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, task_id, session_id, candidates, chosen_plan_id, reasoning, created_at
        "#,
    )
    .bind(validated.task_id)
    .bind(validated.session_id)
    .bind(&candidates_json)
    .bind(validated.chosen_plan_id)
    .bind(validated.reasoning)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

/// Fetch planning history for a specific task
pub async fn list_planning_by_task(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Vec<PlanningHistoryEntry>, StorageError> {
    let rows = sqlx::query_as::<_, PlanningHistoryEntry>(
        r#"
        SELECT id, task_id, session_id, candidates, chosen_plan_id, reasoning, created_at
        FROM planning_history
        WHERE task_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Delete planning history entries older than retention_days
/// Returns the count of deleted rows
pub async fn cleanup_old_planning_history(
    pool: &PgPool,
    retention_days: i32,
) -> Result<u64, StorageError> {
    let result = sqlx::query(
        r#"
        DELETE FROM planning_history
        WHERE created_at < now() - interval '1 day' * $1
        "#,
    )
    .bind(retention_days)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::connect_integration_pool;

    #[tokio::test]
    async fn insert_and_list_planning_history() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping planning_history test: database unavailable");
            return;
        };

        // Create a session and task for testing
        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000001'::uuid) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("create task");

        // Create a planning history entry
        let entry = NewPlanningHistory {
            task_id: task,
            session_id: session,
            candidates: vec![PlanCandidate {
                id: "plan-1".to_string(),
                description: "Test plan".to_string(),
                confidence: 0.85,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.9,
                    tool_success_rate: 0.85,
                    complexity_penalty: 0.1,
                    feedback_adjustment: 0.0,
                    final_score: 0.8,
                },
            }],
            chosen_plan_id: Some("plan-1".to_string()),
            reasoning: "Selected based on similarity score".to_string(),
        };

        let inserted = insert_planning_history(&pool, entry)
            .await
            .expect("insert planning history");

        assert_eq!(inserted.task_id, task);
        assert_eq!(inserted.session_id, session);
        assert_eq!(inserted.chosen_plan_id, Some("plan-1".to_string()));
        assert_eq!(inserted.reasoning, "Selected based on similarity score");
        assert_eq!(inserted.candidates.as_array().map(Vec::len), Some(1));

        // List and verify
        let rows = list_planning_by_task(&pool, task)
            .await
            .expect("list planning history");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, inserted.id);
        assert_eq!(rows[0].task_id, task);
    }

    #[tokio::test]
    async fn validate_confidence_in_range() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping confidence validation test: database unavailable");
            return;
        };

        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000001'::uuid) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("create task");

        // Valid confidence
        let valid_entry = NewPlanningHistory {
            task_id: task,
            session_id: session,
            candidates: vec![PlanCandidate {
                id: "plan-1".to_string(),
                description: "Test".to_string(),
                confidence: 0.75,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.8,
                    tool_success_rate: 0.8,
                    complexity_penalty: 0.2,
                    feedback_adjustment: 0.0,
                    final_score: 0.75,
                },
            }],
            chosen_plan_id: None,
            reasoning: "Test".to_string(),
        };

        assert!(valid_entry.validate().is_ok());

        // Invalid confidence (> 1.0)
        let invalid_high = NewPlanningHistory {
            task_id: task,
            session_id: session,
            candidates: vec![PlanCandidate {
                id: "plan-2".to_string(),
                description: "Test".to_string(),
                confidence: 1.5,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.8,
                    tool_success_rate: 0.8,
                    complexity_penalty: 0.2,
                    feedback_adjustment: 0.0,
                    final_score: 0.75,
                },
            }],
            chosen_plan_id: None,
            reasoning: "Test".to_string(),
        };

        assert!(invalid_high.validate().is_err());

        // Invalid confidence (< 0.0)
        let invalid_low = NewPlanningHistory {
            task_id: task,
            session_id: session,
            candidates: vec![PlanCandidate {
                id: "plan-3".to_string(),
                description: "Test".to_string(),
                confidence: -0.1,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.8,
                    tool_success_rate: 0.8,
                    complexity_penalty: 0.2,
                    feedback_adjustment: 0.0,
                    final_score: 0.75,
                },
            }],
            chosen_plan_id: None,
            reasoning: "Test".to_string(),
        };

        assert!(invalid_low.validate().is_err());
    }

    #[tokio::test]
    async fn truncate_long_reasoning() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping reasoning truncation test: database unavailable");
            return;
        };

        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000001'::uuid) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("create task");

        // Create reasoning longer than 512 chars
        let long_reasoning = "a".repeat(600);

        let entry = NewPlanningHistory {
            task_id: task,
            session_id: session,
            candidates: vec![PlanCandidate {
                id: "plan-1".to_string(),
                description: "Test".to_string(),
                confidence: 0.8,
                score_breakdown: ScoreBreakdown {
                    similarity_score: 0.8,
                    tool_success_rate: 0.8,
                    complexity_penalty: 0.2,
                    feedback_adjustment: 0.0,
                    final_score: 0.75,
                },
            }],
            chosen_plan_id: None,
            reasoning: long_reasoning,
        };

        let validated = entry.validate().expect("validation should succeed");
        assert_eq!(validated.reasoning.len(), 512);

        // Should be insertable
        let inserted = insert_planning_history(&pool, validated)
            .await
            .expect("insert should succeed");
        assert_eq!(inserted.reasoning.len(), 512);
    }

    #[tokio::test]
    async fn cleanup_old_planning_history_by_retention() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping cleanup test: database unavailable");
            return;
        };

        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000001'::uuid) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .expect("create task");

        // Insert an old entry directly (bypass insert_planning_history to control created_at)
        sqlx::query(
            r#"
            INSERT INTO planning_history (task_id, session_id, candidates, chosen_plan_id, reasoning, created_at)
            VALUES ($1, $2, $3, $4, $5, now() - interval '100 days')
            "#,
        )
        .bind(task)
        .bind(session)
        .bind(json!([{"id":"plan-1"}]))
        .bind("plan-1")
        .bind("old reasoning")
        .execute(&pool)
        .await
        .expect("insert old entry");

        // Insert a recent entry
        sqlx::query(
            r#"
            INSERT INTO planning_history (task_id, session_id, candidates, chosen_plan_id, reasoning)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(task)
        .bind(session)
        .bind(json!([{"id":"plan-2"}]))
        .bind("plan-2")
        .bind("recent reasoning")
        .execute(&pool)
        .await
        .expect("insert recent entry");

        // Cleanup entries older than 30 days
        let deleted = cleanup_old_planning_history(&pool, 30)
            .await
            .expect("cleanup should succeed");

        assert_eq!(deleted, 1u64); // Only the old entry should be deleted

        // Verify only one entry remains
        let remaining = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM planning_history WHERE task_id = $1",
        )
        .bind(task)
        .fetch_one(&pool)
        .await
        .expect("count");

        assert_eq!(remaining, 1i64);
    }
}
