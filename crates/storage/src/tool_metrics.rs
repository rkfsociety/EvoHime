use sqlx::PgPool;
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionStat {
    pub id: i64,
    pub tool_name: String,
    pub operation_type: Option<String>,
    pub success: bool,
    pub error_category: Option<String>,
    pub task_id: Uuid,
    pub workspace_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSuccessRate {
    pub tool_name: String,
    pub success_count: i64,
    pub total_count: i64,
    pub smoothed_rate: f32, // (success + 1) / (total + 2)
    pub reliability: ToolMetricsReliability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolMetricsReliability {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "low")]
    Low,
}

const MIN_HISTORY_FOR_RELIABLE: i64 = 5;

pub async fn record_tool_execution(
    pool: &PgPool,
    tool_name: &str,
    operation_type: Option<&str>,
    success: bool,
    error_category: Option<&str>,
    task_id: Uuid,
    workspace_path: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> Result<ToolExecutionStat, sqlx::Error> {
    sqlx::query_as::<_, ToolExecutionStat>(
        r#"
        INSERT INTO tool_execution_stats
        (tool_name, operation_type, success, error_category, task_id, workspace_path, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, tool_name, operation_type, success, error_category, task_id, workspace_path, created_at, metadata
        "#
    )
    .bind(tool_name)
    .bind(operation_type)
    .bind(success)
    .bind(error_category)
    .bind(task_id)
    .bind(workspace_path)
    .bind(metadata.unwrap_or_else(|| serde_json::json!({})))
    .fetch_one(pool)
    .await
}

/// Calculate smoothed success rate for a tool over last N days
/// Using Beta-binomial prior: smoothed = (success + 1) / (total + 2)
pub async fn get_tool_success_rate(
    pool: &PgPool,
    tool_name: &str,
    days_lookback: i64,
) -> Result<ToolSuccessRate, sqlx::Error> {
    let cutoff_date = Utc::now() - Duration::days(days_lookback);

    let row = sqlx::query!(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE success = true) as success_count,
            COUNT(*) as total_count
        FROM tool_execution_stats
        WHERE tool_name = $1 AND created_at > $2
        "#,
        tool_name,
        cutoff_date
    )
    .fetch_one(pool)
    .await?;

    let success_count = row.success_count.unwrap_or(0);
    let total_count = row.total_count.unwrap_or(0);

    let smoothed_rate = ((success_count + 1) as f32) / ((total_count + 2) as f32);
    let reliability = if total_count >= MIN_HISTORY_FOR_RELIABLE {
        ToolMetricsReliability::High
    } else if total_count > 0 {
        ToolMetricsReliability::Medium
    } else {
        ToolMetricsReliability::Low
    };

    Ok(ToolSuccessRate {
        tool_name: tool_name.to_string(),
        success_count,
        total_count,
        smoothed_rate,
        reliability,
    })
}

/// Get success rates for multiple tools in one query (batch optimization)
pub async fn get_tools_success_rates(
    pool: &PgPool,
    tool_names: &[String],
    days_lookback: i64,
) -> Result<Vec<ToolSuccessRate>, sqlx::Error> {
    if tool_names.is_empty() {
        return Ok(Vec::new());
    }

    let cutoff_date = Utc::now() - Duration::days(days_lookback);

    let rows = sqlx::query!(
        r#"
        SELECT
            tool_name,
            COUNT(*) FILTER (WHERE success = true) as success_count,
            COUNT(*) as total_count
        FROM tool_execution_stats
        WHERE tool_name = ANY($1) AND created_at > $2
        GROUP BY tool_name
        "#,
        tool_names,
        cutoff_date
    )
    .fetch_all(pool)
    .await?;

    let mut rates = Vec::new();
    for row in rows {
        let success_count = row.success_count.unwrap_or(0);
        let total_count = row.total_count.unwrap_or(0);

        let smoothed_rate = ((success_count + 1) as f32) / ((total_count + 2) as f32);
        let reliability = if total_count >= MIN_HISTORY_FOR_RELIABLE {
            ToolMetricsReliability::High
        } else if total_count > 0 {
            ToolMetricsReliability::Medium
        } else {
            ToolMetricsReliability::Low
        };

        rates.push(ToolSuccessRate {
            tool_name: row.tool_name,
            success_count,
            total_count,
            smoothed_rate,
            reliability,
        });
    }

    Ok(rates)
}

/// Classify tool as read-only vs destructive
pub fn classify_tool_destructiveness(tool_name: &str, operation_type: Option<&str>) -> bool {
    let read_only = [
        "filesystem.read",
        "filesystem.search",
        "filesystem.list",
        "git.status",
        "git.diff",
        "browser.open",
        "browser.extract",
        "browser.session.read",
        "browser.session.screenshot",
        "memory.search",
        "http.fetch", // GET, safe redirects
    ];

    if read_only.contains(&tool_name) {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoothed_rate_formula() {
        // With 0 history: (0 + 1) / (0 + 2) = 0.5
        let rate = ((0 + 1) as f32) / ((0 + 2) as f32);
        assert!((rate - 0.5).abs() < 0.01);

        // With 3 successes out of 5: (3 + 1) / (5 + 2) = 4/7 ≈ 0.571
        let rate = ((3 + 1) as f32) / ((5 + 2) as f32);
        assert!((rate - 0.571).abs() < 0.01);
    }

    #[test]
    fn test_classify_tool_destructiveness() {
        assert!(!classify_tool_destructiveness("filesystem.read", None));
        assert!(!classify_tool_destructiveness("git.status", None));
        assert!(classify_tool_destructiveness("filesystem.write", None));
        assert!(classify_tool_destructiveness("git.push", None));
        assert!(classify_tool_destructiveness("git.commit", None));
        assert!(classify_tool_destructiveness("shell.execute", Some("rm -rf /")));
    }
}
