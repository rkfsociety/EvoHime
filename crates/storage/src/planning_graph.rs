use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::StorageError;

#[derive(Debug, Clone, FromRow)]
pub struct TaskExecutionGraph {
    pub id: Uuid,
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub version: i32,
    pub topological_order: Value, // JSON array of step IDs
    pub dependency_map: Value,    // JSON object of dependencies
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TaskExecutionStep {
    pub id: Uuid,
    pub graph_id: Uuid,
    pub step_id: String,
    pub status: String, // "pending" | "running" | "completed" | "failed"
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewTaskExecutionGraph {
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub topological_order: Vec<String>,
    pub dependency_map: std::collections::HashMap<String, Vec<String>>,
}

/// Insert a new task execution graph (increments version if exists)
pub async fn insert_execution_graph(
    pool: &PgPool,
    graph: NewTaskExecutionGraph,
) -> Result<TaskExecutionGraph, StorageError> {
    // Determine version (1 for new, +1 if regenerating after reflection)
    let existing_version = sqlx::query_scalar::<_, Option<i32>>(
        r#"SELECT MAX(version) FROM task_execution_graphs WHERE task_id = $1 AND session_id = $2"#,
    )
    .bind(graph.task_id)
    .bind(graph.session_id)
    .fetch_optional(pool)
    .await?
    .flatten()
    .unwrap_or(0);

    let new_version = existing_version + 1;
    let topo_json = serde_json::to_value(&graph.topological_order)?;
    let deps_json = serde_json::to_value(&graph.dependency_map)?;

    // Start transaction to ensure graph + steps are inserted atomically
    let mut tx = pool.begin().await?;

    // Insert graph
    let row = sqlx::query_as::<_, TaskExecutionGraph>(
        r#"
        INSERT INTO task_execution_graphs (task_id, session_id, version, topological_order, dependency_map)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, task_id, session_id, version, topological_order, dependency_map, created_at
        "#,
    )
    .bind(graph.task_id)
    .bind(graph.session_id)
    .bind(new_version)
    .bind(topo_json)
    .bind(deps_json)
    .fetch_one(&mut *tx)
    .await?;

    // Initialize per-step state (all pending) using batch INSERT
    if !graph.topological_order.is_empty() {
        let mut query_builder = sqlx::query_builder::QueryBuilder::new(
            "INSERT INTO task_execution_steps (graph_id, step_id, status) ",
        );

        query_builder.push_values(graph.topological_order.iter(), |mut b, step_id| {
            b.push_bind(row.id)
                .push_bind(step_id.clone())
                .push_bind("pending");
        });

        query_builder.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;

    Ok(row)
}

/// Get latest execution graph for a task
pub async fn get_execution_graph(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Option<TaskExecutionGraph>, StorageError> {
    let row = sqlx::query_as::<_, TaskExecutionGraph>(
        r#"
        SELECT id, task_id, session_id, version, topological_order, dependency_map, created_at
        FROM task_execution_graphs
        WHERE task_id = $1
        ORDER BY version DESC
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Get all step states for a graph
pub async fn list_steps_for_graph(
    pool: &PgPool,
    graph_id: Uuid,
) -> Result<Vec<TaskExecutionStep>, StorageError> {
    let rows = sqlx::query_as::<_, TaskExecutionStep>(
        r#"
        SELECT id, graph_id, step_id, status, started_at, completed_at, error_message, created_at
        FROM task_execution_steps
        WHERE graph_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(graph_id)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Update single step execution state
pub async fn update_step_status(
    pool: &PgPool,
    graph_id: Uuid,
    step_id: &str,
    status: &str,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    error_message: Option<String>,
) -> Result<(), StorageError> {
    sqlx::query(
        r#"
        UPDATE task_execution_steps
        SET status = $3, started_at = COALESCE($4, started_at),
            completed_at = $5, error_message = $6
        WHERE graph_id = $1 AND step_id = $2
        "#,
    )
    .bind(graph_id)
    .bind(step_id)
    .bind(status)
    .bind(started_at)
    .bind(completed_at)
    .bind(error_message)
    .execute(pool)
    .await?;

    Ok(())
}

/// Query all currently running steps
pub async fn list_running_steps(
    pool: &PgPool,
) -> Result<Vec<TaskExecutionStep>, StorageError> {
    let rows = sqlx::query_as::<_, TaskExecutionStep>(
        r#"
        SELECT id, graph_id, step_id, status, started_at, completed_at, error_message, created_at
        FROM task_execution_steps WHERE status = 'running'
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_insert_and_get_execution_graph() {
        // This test requires DATABASE_URL to be set and database to be up
        // For now, mark as ignored pending database availability
        // Proper test will be added when running integration tests
    }
}
