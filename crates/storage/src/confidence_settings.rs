use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ConfidenceSettingsRow {
    pub id: i32,
    pub operator_id: Uuid,
    pub setting_key: String,
    pub setting_value: serde_json::Value,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceThresholds {
    pub version: String,
    pub risk_none: ThresholdPair,
    pub risk_low: ThresholdPair,
    pub risk_medium: ThresholdPair,
    pub risk_high: ThresholdPair,
    pub missing_signal_ask_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdPair {
    pub proceed: f32,
    pub ask: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require: Option<f32>,
}

impl Default for ConfidenceThresholds {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            risk_none: ThresholdPair {
                proceed: 0.65,
                ask: 0.40,
                require: None,
            },
            risk_low: ThresholdPair {
                proceed: 0.70,
                ask: 0.45,
                require: None,
            },
            risk_medium: ThresholdPair {
                proceed: 0.75,
                ask: 0.50,
                require: None,
            },
            risk_high: ThresholdPair {
                proceed: 0.85,
                ask: 0.65,
                require: Some(0.30),
            },
            missing_signal_ask_threshold: 0.5,
        }
    }
}

pub async fn get_confidence_thresholds(
    pool: &PgPool,
    operator_id: Uuid,
) -> Result<ConfidenceThresholds, sqlx::Error> {
    let row = sqlx::query_as::<_, ConfidenceSettingsRow>(
        "SELECT * FROM confidence_settings WHERE operator_id = $1 AND setting_key = 'confidence_thresholds'"
    )
    .bind(operator_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            serde_json::from_value(row.setting_value)
                .map_err(|_| sqlx::Error::RowNotFound)
        }
        None => Ok(ConfidenceThresholds::default()),
    }
}

pub async fn set_confidence_thresholds(
    pool: &PgPool,
    operator_id: Uuid,
    thresholds: &ConfidenceThresholds,
) -> Result<ConfidenceSettingsRow, sqlx::Error> {
    let value = serde_json::to_value(thresholds).map_err(|_| sqlx::Error::RowNotFound)?;

    sqlx::query_as::<_, ConfidenceSettingsRow>(
        r#"
        INSERT INTO confidence_settings (operator_id, setting_key, setting_value, version)
        VALUES ($1, 'confidence_thresholds', $2, 1)
        ON CONFLICT (operator_id, setting_key) DO UPDATE
        SET setting_value = $2, updated_at = NOW()
        RETURNING *
        "#
    )
    .bind(operator_id)
    .bind(value)
    .fetch_one(pool)
    .await
}
