//! Thinking settings and usage tracking (Wave 3B: Extended Reasoning).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThinkingSettings {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub enabled: bool,
    pub budget_tokens: Option<i32>,
    pub max_budget_tokens: Option<i32>,
    pub show_thinking: bool,
    pub thinking_verbosity: Option<String>,
    pub monthly_thinking_cost_limit: Option<rust_decimal::Decimal>,
    pub warning_threshold_percent: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThinkingUsage {
    pub id: Uuid,
    pub session_id: Uuid,
    pub operator_id: Uuid,
    pub task_id: Option<Uuid>,
    pub thinking_tokens: i32,
    pub thinking_duration_ms: Option<i32>,
    pub final_content_tokens: Option<i32>,
    pub estimated_cost_usd: Option<rust_decimal::Decimal>,
    pub created_at: DateTime<Utc>,
}

/// Get or create thinking settings for operator (defaults to disabled, 5000 tokens budget)
pub async fn get_or_create_thinking_settings(
    pool: &PgPool,
    operator_id: Uuid,
) -> sqlx::Result<ThinkingSettings> {
    sqlx::query_as::<_, ThinkingSettings>(
        r#"
        INSERT INTO thinking_settings (operator_id, enabled, budget_tokens)
        VALUES ($1, false, 5000)
        ON CONFLICT (operator_id) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
        RETURNING *
        "#,
    )
    .bind(operator_id)
    .fetch_one(pool)
    .await
}

/// Update thinking settings
pub async fn update_thinking_settings(
    pool: &PgPool,
    operator_id: Uuid,
    enabled: bool,
    budget_tokens: Option<i32>,
    show_thinking: bool,
    thinking_verbosity: Option<String>,
) -> sqlx::Result<ThinkingSettings> {
    sqlx::query_as::<_, ThinkingSettings>(
        r#"
        UPDATE thinking_settings
        SET enabled = $2,
            budget_tokens = COALESCE($3, budget_tokens),
            show_thinking = $4,
            thinking_verbosity = COALESCE($5, thinking_verbosity),
            updated_at = CURRENT_TIMESTAMP
        WHERE operator_id = $1
        RETURNING *
        "#,
    )
    .bind(operator_id)
    .bind(enabled)
    .bind(budget_tokens)
    .bind(show_thinking)
    .bind(thinking_verbosity)
    .fetch_one(pool)
    .await
}

/// Record thinking usage for a session/task
pub async fn record_thinking_usage(
    pool: &PgPool,
    session_id: Uuid,
    operator_id: Uuid,
    task_id: Option<Uuid>,
    thinking_tokens: i32,
    thinking_duration_ms: Option<i32>,
    final_content_tokens: Option<i32>,
) -> sqlx::Result<ThinkingUsage> {
    // Cost estimation: Claude API typically charges ~$0.000003 per thinking token
    let estimated_cost_usd = (thinking_tokens as f64) * 0.000003;

    sqlx::query_as::<_, ThinkingUsage>(
        r#"
        INSERT INTO thinking_usage (
            session_id, operator_id, task_id, thinking_tokens,
            thinking_duration_ms, final_content_tokens, estimated_cost_usd
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING *
        "#,
    )
    .bind(session_id)
    .bind(operator_id)
    .bind(task_id)
    .bind(thinking_tokens)
    .bind(thinking_duration_ms)
    .bind(final_content_tokens)
    .bind(rust_decimal::Decimal::from_f64_retain(estimated_cost_usd))
    .fetch_one(pool)
    .await
}

/// Get monthly thinking usage cost for operator
pub async fn get_monthly_thinking_cost(
    pool: &PgPool,
    operator_id: Uuid,
) -> sqlx::Result<Option<rust_decimal::Decimal>> {
    #[derive(sqlx::FromRow)]
    struct CostRow {
        total_cost: Option<rust_decimal::Decimal>,
    }

    let row = sqlx::query_as::<_, CostRow>(
        r#"
        SELECT COALESCE(SUM(estimated_cost_usd), 0) as total_cost
        FROM thinking_usage
        WHERE operator_id = $1
          AND created_at >= date_trunc('month', CURRENT_TIMESTAMP)
        "#,
    )
    .bind(operator_id)
    .fetch_one(pool)
    .await?;

    Ok(row.total_cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_settings_serde() {
        let settings = ThinkingSettings {
            id: Uuid::new_v4(),
            operator_id: Uuid::new_v4(),
            enabled: true,
            budget_tokens: Some(10000),
            max_budget_tokens: Some(32000),
            show_thinking: true,
            thinking_verbosity: Some("full".into()),
            monthly_thinking_cost_limit: Some(rust_decimal::Decimal::new(50, 2)),
            warning_threshold_percent: Some(80),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let json = serde_json::to_string(&settings).expect("serialize");
        let _deserialized: ThinkingSettings =
            serde_json::from_str(&json).expect("deserialize");
    }
}
