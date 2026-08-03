use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConfidenceAuditLog {
    pub id: i64,
    pub event_id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub confidence_score: f32,
    pub risk_level: String, // "none" | "low" | "medium" | "high"
    pub confidence_version: String,
    pub breakdown: serde_json::Value,
    pub reliability_scores: serde_json::Value,
    pub missing_signals: Vec<String>,
    pub decision: String, // "proceed" | "ask" | "require_approval"
    pub force_approved: Option<bool>,
    pub force_approval_reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewConfidenceAuditLog {
    pub event_id: Uuid,
    pub task_id: Uuid,
    pub session_id: Option<Uuid>,
    pub confidence_score: f32,
    pub risk_level: String,
    pub breakdown: serde_json::Value,
    pub reliability_scores: serde_json::Value,
    pub missing_signals: Vec<String>,
    pub decision: String,
    pub force_approved: bool,
    pub force_approval_reason: Option<String>,
}

pub async fn insert_confidence_audit(
    pool: &PgPool,
    record: &NewConfidenceAuditLog,
) -> Result<ConfidenceAuditLog, sqlx::Error> {
    sqlx::query_as::<_, ConfidenceAuditLog>(
        r#"
        INSERT INTO confidence_audit_log
        (event_id, task_id, session_id, confidence_score, risk_level, confidence_version,
         breakdown, reliability_scores, missing_signals, decision, force_approved, force_approval_reason)
        VALUES ($1, $2, $3, $4, $5, '1', $6, $7, $8, $9, $10, $11)
        RETURNING id, event_id, task_id, session_id, confidence_score, risk_level, confidence_version,
                  breakdown, reliability_scores, missing_signals, decision, force_approved,
                  force_approval_reason, timestamp
        "#
    )
    .bind(&record.event_id)
    .bind(&record.task_id)
    .bind(&record.session_id)
    .bind(record.confidence_score)
    .bind(&record.risk_level)
    .bind(&record.breakdown)
    .bind(&record.reliability_scores)
    .bind(&record.missing_signals)
    .bind(&record.decision)
    .bind(record.force_approved)
    .bind(&record.force_approval_reason)
    .fetch_one(pool)
    .await
}

pub async fn list_confidence_audit_for_task(
    pool: &PgPool,
    task_id: Uuid,
) -> Result<Vec<ConfidenceAuditLog>, sqlx::Error> {
    sqlx::query_as::<_, ConfidenceAuditLog>(
        r#"
        SELECT id, event_id, task_id, session_id, confidence_score, risk_level, confidence_version,
               breakdown, reliability_scores, missing_signals, decision, force_approved,
               force_approval_reason, timestamp
        FROM confidence_audit_log
        WHERE task_id = $1
        ORDER BY timestamp DESC
        "#
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
}

pub async fn list_confidence_audit_for_session(
    pool: &PgPool,
    session_id: Uuid,
) -> Result<Vec<ConfidenceAuditLog>, sqlx::Error> {
    sqlx::query_as::<_, ConfidenceAuditLog>(
        r#"
        SELECT id, event_id, task_id, session_id, confidence_score, risk_level, confidence_version,
               breakdown, reliability_scores, missing_signals, decision, force_approved,
               force_approval_reason, timestamp
        FROM confidence_audit_log
        WHERE session_id = $1
        ORDER BY timestamp DESC
        LIMIT 1000
        "#
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
}
