use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CostLimitRow {
    pub id: i32,
    pub model: String,
    pub daily_cap_tokens: i64,
    pub reset_hour: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostLimitUpdate {
    pub daily_cap_tokens: i64,
    pub reset_hour: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CostTrackingRow {
    pub id: i32,
    pub model: String,
    pub date: chrono::NaiveDate,
    pub tokens_consumed: i64,
}

/// Get or create a cost limit for a model
pub async fn get_or_create_cost_limit(
    pool: &PgPool,
    model: &str,
    default_cap: i64,
) -> Result<CostLimitRow, sqlx::Error> {
    sqlx::query_as::<_, CostLimitRow>(
        r#"
        INSERT INTO cost_limits (model, daily_cap_tokens, reset_hour, enabled)
        VALUES ($1, $2, 0, true)
        ON CONFLICT (model) DO UPDATE
        SET updated_at = NOW()
        RETURNING id, model, daily_cap_tokens, reset_hour, enabled
        "#,
    )
    .bind(model)
    .bind(default_cap)
    .fetch_one(pool)
    .await
}

/// Update cost limit for a model
pub async fn update_cost_limit(
    pool: &PgPool,
    model: &str,
    update: &CostLimitUpdate,
) -> Result<CostLimitRow, sqlx::Error> {
    sqlx::query_as::<_, CostLimitRow>(
        r#"
        UPDATE cost_limits
        SET daily_cap_tokens = $2, reset_hour = $3, enabled = $4, updated_at = NOW()
        WHERE model = $1
        RETURNING id, model, daily_cap_tokens, reset_hour, enabled
        "#,
    )
    .bind(model)
    .bind(update.daily_cap_tokens)
    .bind(update.reset_hour)
    .bind(update.enabled)
    .fetch_one(pool)
    .await
}

/// Get all cost limits
pub async fn list_cost_limits(pool: &PgPool) -> Result<Vec<CostLimitRow>, sqlx::Error> {
    sqlx::query_as::<_, CostLimitRow>(
        r#"
        SELECT id, model, daily_cap_tokens, reset_hour, enabled
        FROM cost_limits
        ORDER BY model
        "#,
    )
    .fetch_all(pool)
    .await
}

/// Get cost limit for a specific model
pub async fn get_cost_limit(
    pool: &PgPool,
    model: &str,
) -> Result<Option<CostLimitRow>, sqlx::Error> {
    sqlx::query_as::<_, CostLimitRow>(
        r#"
        SELECT id, model, daily_cap_tokens, reset_hour, enabled
        FROM cost_limits
        WHERE model = $1
        "#,
    )
    .bind(model)
    .fetch_optional(pool)
    .await
}

/// Add tokens to today's tracking
pub async fn add_tokens_to_tracking(
    pool: &PgPool,
    model: &str,
    tokens: i64,
) -> Result<CostTrackingRow, sqlx::Error> {
    let today = Utc::now().date_naive();
    sqlx::query_as::<_, CostTrackingRow>(
        r#"
        INSERT INTO cost_tracking (model, date, tokens_consumed)
        VALUES ($1, $2, $3)
        ON CONFLICT (model, date) DO UPDATE
        SET tokens_consumed = cost_tracking.tokens_consumed + EXCLUDED.tokens_consumed,
            updated_at = NOW()
        RETURNING id, model, date, tokens_consumed
        "#,
    )
    .bind(model)
    .bind(today)
    .bind(tokens)
    .fetch_one(pool)
    .await
}

/// Get today's token consumption for a model
pub async fn get_today_consumption(pool: &PgPool, model: &str) -> Result<i64, sqlx::Error> {
    let today = Utc::now().date_naive();
    let row = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COALESCE(tokens_consumed, 0) FROM cost_tracking
        WHERE model = $1 AND date = $2
        "#,
    )
    .bind(model)
    .bind(today)
    .fetch_optional(pool)
    .await?;

    Ok(row.unwrap_or(0))
}

/// Check if model has exceeded daily cap
pub async fn check_spending_cap(
    pool: &PgPool,
    model: &str,
    additional_tokens: i64,
) -> Result<bool, sqlx::Error> {
    let limit = get_cost_limit(pool, model).await?;

    if let Some(limit) = limit {
        if !limit.enabled || limit.daily_cap_tokens == 0 {
            // No limit enforced
            return Ok(false);
        }

        let consumed = get_today_consumption(pool, model).await?;
        Ok(consumed + additional_tokens > limit.daily_cap_tokens)
    } else {
        // No limit defined
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_db::connect_integration_pool;

    #[tokio::test]
    async fn test_cost_limit_crud() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping cost_limits integration test: database unavailable");
            return;
        };

        // Create
        let limit = get_or_create_cost_limit(&pool, "test-model", 1_000_000)
            .await
            .unwrap();
        assert_eq!(limit.model, "test-model");
        assert_eq!(limit.daily_cap_tokens, 1_000_000);
        assert!(limit.enabled);

        // Update
        let update = CostLimitUpdate {
            daily_cap_tokens: 500_000,
            reset_hour: 12,
            enabled: false,
        };
        let updated = update_cost_limit(&pool, "test-model", &update)
            .await
            .unwrap();
        assert_eq!(updated.daily_cap_tokens, 500_000);
        assert_eq!(updated.reset_hour, 12);
        assert!(!updated.enabled);

        // Get
        let fetched = get_cost_limit(&pool, "test-model").await.unwrap();
        assert!(fetched.is_some());
    }

    #[tokio::test]
    async fn test_spending_tracking() {
        let Some(pool) = connect_integration_pool().await else {
            eprintln!("skipping cost_limits integration test: database unavailable");
            return;
        };

        // Set limit
        let _limit = get_or_create_cost_limit(&pool, "test-model2", 100_000)
            .await
            .unwrap();

        // Add tokens
        let _tracking = add_tokens_to_tracking(&pool, "test-model2", 50_000)
            .await
            .unwrap();

        // Check consumption
        let consumed = get_today_consumption(&pool, "test-model2").await.unwrap();
        assert_eq!(consumed, 50_000);

        // Check cap not exceeded
        let exceeded = check_spending_cap(&pool, "test-model2", 40_000)
            .await
            .unwrap();
        assert!(!exceeded); // 50k + 40k = 90k < 100k

        // Check cap exceeded
        let exceeded = check_spending_cap(&pool, "test-model2", 60_000)
            .await
            .unwrap();
        assert!(exceeded); // 50k + 60k = 110k > 100k
    }
}
