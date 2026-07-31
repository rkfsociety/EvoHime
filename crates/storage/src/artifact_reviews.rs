use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewArtifactKind {
    Spec,
    Plan,
}

impl ReviewArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Spec => "spec",
            Self::Plan => "plan",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerCommentEntry {
    pub route_name: String,
    pub comments: String,
    pub failed: bool,
}

/// One persisted review round for a task's spec or plan.
#[derive(Debug, Clone, FromRow)]
pub struct ArtifactReviewRow {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub artifact_kind: String,
    pub round_number: i32,
    pub original_content: String,
    pub reviewer_comments: Value, // JSON array of ReviewerCommentEntry
    pub synthesized_feedback: String,
    pub revised_content: String,
    pub self_check_iterations: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewArtifactReview {
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub artifact_kind: ReviewArtifactKind,
    pub round_number: i32,
    pub original_content: String,
    pub reviewer_comments: Vec<ReviewerCommentEntry>,
    pub synthesized_feedback: String,
    pub revised_content: String,
    pub self_check_iterations: i32,
}

pub async fn insert_artifact_review(
    pool: &PgPool,
    entry: NewArtifactReview,
) -> Result<ArtifactReviewRow, StorageError> {
    if entry.round_number < 1 {
        return Err(StorageError::InvalidArtifactReview(format!(
            "round_number must be >= 1, got {}",
            entry.round_number
        )));
    }

    // The column is a scalar jsonb holding a JSON array, not a Postgres
    // array of jsonb — bind one Value::Array, same fix as planning_history.
    let comments_json: Value = Value::Array(
        entry
            .reviewer_comments
            .iter()
            .map(|comment| serde_json::to_value(comment).unwrap_or(Value::Null))
            .collect(),
    );

    let row = sqlx::query_as::<_, ArtifactReviewRow>(
        r#"
        INSERT INTO artifact_reviews
            (task_id, session_id, artifact_kind, round_number, original_content,
             reviewer_comments, synthesized_feedback, revised_content, self_check_iterations)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, task_id, session_id, artifact_kind, round_number, original_content,
                  reviewer_comments, synthesized_feedback, revised_content, self_check_iterations, created_at
        "#,
    )
    .bind(entry.task_id)
    .bind(entry.session_id)
    .bind(entry.artifact_kind.as_str())
    .bind(entry.round_number)
    .bind(entry.original_content)
    .bind(&comments_json)
    .bind(entry.synthesized_feedback)
    .bind(entry.revised_content)
    .bind(entry.self_check_iterations)
    .fetch_one(pool)
    .await?;

    Ok(row)
}

pub async fn list_artifact_reviews_by_task(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Vec<ArtifactReviewRow>, StorageError> {
    let rows = sqlx::query_as::<_, ArtifactReviewRow>(
        r#"
        SELECT id, task_id, session_id, artifact_kind, round_number, original_content,
               reviewer_comments, synthesized_feedback, revised_content, self_check_iterations, created_at
        FROM artifact_reviews
        WHERE task_id = $1
        ORDER BY round_number ASC
        "#,
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::connect_integration_pool;

    async fn seed_session_and_task(pool: &PgPool) -> (Uuid, Uuid) {
        let session = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO sessions (operator_id) VALUES ('00000000-0000-0000-0000-000000000001'::uuid) RETURNING id",
        )
        .fetch_one(pool)
        .await
        .expect("create session");

        let task = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO tasks (session_id, user_message, status) VALUES ($1, 'test', 'running') RETURNING id",
        )
        .bind(session)
        .fetch_one(pool)
        .await
        .expect("create task");

        (session, task)
    }

    #[tokio::test]
    async fn insert_and_list_artifact_reviews() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping artifact_reviews test: database unavailable");
            return;
        };
        let (session, task) = seed_session_and_task(&pool).await;

        let entry = NewArtifactReview {
            task_id: task,
            session_id: session,
            artifact_kind: ReviewArtifactKind::Plan,
            round_number: 1,
            original_content: "step 1\nstep 2".to_string(),
            reviewer_comments: vec![
                ReviewerCommentEntry {
                    route_name: "reviewer_0".to_string(),
                    comments: "missing error handling".to_string(),
                    failed: false,
                },
                ReviewerCommentEntry {
                    route_name: "reviewer_1".to_string(),
                    comments: String::new(),
                    failed: true,
                },
            ],
            synthesized_feedback: "Add error handling to step 2.".to_string(),
            revised_content: "step 1\nstep 2 (with error handling)".to_string(),
            self_check_iterations: 1,
        };

        let inserted = insert_artifact_review(&pool, entry)
            .await
            .expect("insert artifact review");

        assert_eq!(inserted.task_id, task);
        assert_eq!(inserted.artifact_kind, "plan");
        assert_eq!(inserted.round_number, 1);
        assert_eq!(inserted.reviewer_comments.as_array().map(Vec::len), Some(2));

        let rows = list_artifact_reviews_by_task(&pool, task)
            .await
            .expect("list artifact reviews");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, inserted.id);
    }

    #[tokio::test]
    async fn rejects_round_number_below_one() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping round_number validation test: database unavailable");
            return;
        };
        let (session, task) = seed_session_and_task(&pool).await;

        let entry = NewArtifactReview {
            task_id: task,
            session_id: session,
            artifact_kind: ReviewArtifactKind::Spec,
            round_number: 0,
            original_content: "spec".to_string(),
            reviewer_comments: vec![],
            synthesized_feedback: String::new(),
            revised_content: "spec".to_string(),
            self_check_iterations: 0,
        };

        let error = insert_artifact_review(&pool, entry)
            .await
            .expect_err("round_number 0 should be rejected");
        assert!(matches!(error, StorageError::InvalidArtifactReview(_)));
    }
}
