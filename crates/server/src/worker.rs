use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const MAX_TEXT_LENGTH: usize = 1_000_000;
const SUPPORTED_TASKS: &[&str] = &[
    "echo",
    "text.stats",
    "text.keywords",
    "text.summarize",
    "text.chunk",
    "text.similarity",
    "text.entities",
    "text.diff",
];
const DEFAULT_MAX_SENTENCES: i64 = 3;
const DEFAULT_CHUNK_SIZE: i64 = 500;
const DEFAULT_CHUNK_OVERLAP: i64 = 50;
const DEFAULT_DIFF_CONTEXT: i64 = 3;
const DEFAULT_MAX_DIFF_LINES: i64 = 500;

#[derive(Clone)]
pub struct WorkerClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Serialize)]
pub struct SubmitRequest<'a> {
    pub task: &'a str,
    pub payload: &'a Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkerJob {
    pub id: String,
    pub status: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    #[serde(default)]
    pub heartbeat_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct WorkerHealth {
    pub status: String,
    pub started_at: String,
    pub pid: i64,
    #[serde(default)]
    pub supported_tasks: Vec<String>,
    #[serde(default)]
    pub queue_depth: Option<i64>,
    #[serde(default)]
    pub active_jobs: Option<i64>,
}

pub fn is_terminal_status(status: &str) -> bool {
    matches!(status, "completed" | "failed")
}

pub fn retry_delay(attempts: i32) -> Duration {
    let exponent = attempts.clamp(1, 5) as u32;
    Duration::from_secs((1_u64 << exponent).min(30))
}

pub fn heartbeat_is_stale(
    heartbeat_at: Option<&str>,
    now: DateTime<Utc>,
    stall_after: Duration,
) -> bool {
    let Some(raw) = heartbeat_at else {
        // Missing heartbeat is not a stall yet; the poll timeout path still recovers.
        return false;
    };
    let Ok(parsed) = DateTime::parse_from_rfc3339(raw) else {
        return true;
    };
    let heartbeat = parsed.with_timezone(&Utc);
    now.signed_duration_since(heartbeat)
        .to_std()
        .map(|age| age > stall_after)
        .unwrap_or(true)
}

pub fn validate_task_payload(task: &str, payload: &Value) -> Result<(), String> {
    if task.trim().is_empty() {
        return Err("task must be a non-empty string".into());
    }
    if !payload.is_object() {
        return Err("payload must be an object".into());
    }
    if !SUPPORTED_TASKS.contains(&task) {
        return Err(format!("unsupported task: {task}"));
    }
    if task == "echo" {
        return Ok(());
    }

    match task {
        "text.stats" | "text.keywords" | "text.entities" => {
            require_text(task, payload)?;
            Ok(())
        }
        "text.summarize" => {
            require_text(task, payload)?;
            optional_int(payload, "max_sentences", DEFAULT_MAX_SENTENCES, 1, Some(20))?;
            Ok(())
        }
        "text.chunk" => {
            require_text(task, payload)?;
            let chunk_size =
                optional_int(payload, "chunk_size", DEFAULT_CHUNK_SIZE, 64, Some(8000))?;
            let overlap = optional_int(payload, "overlap", DEFAULT_CHUNK_OVERLAP, 0, None)?;
            if overlap >= chunk_size {
                return Err("payload.overlap must be less than payload.chunk_size".into());
            }
            Ok(())
        }
        "text.similarity" => {
            require_named_text(task, payload, "text_a")?;
            require_named_text(task, payload, "text_b")?;
            Ok(())
        }
        "text.diff" => {
            require_named_text(task, payload, "text_a")?;
            require_named_text(task, payload, "text_b")?;
            optional_int(payload, "context", DEFAULT_DIFF_CONTEXT, 0, Some(20))?;
            optional_int(
                payload,
                "max_diff_lines",
                DEFAULT_MAX_DIFF_LINES,
                1,
                Some(2000),
            )?;
            Ok(())
        }
        _ => Err(format!("unsupported task: {task}")),
    }
}

fn require_text(task: &str, payload: &Value) -> Result<(), String> {
    require_named_text(task, payload, "text")
}

fn require_named_text(task: &str, payload: &Value, key: &str) -> Result<(), String> {
    match payload.get(key) {
        Some(Value::String(text)) if text.len() <= MAX_TEXT_LENGTH => Ok(()),
        Some(Value::String(_)) => {
            Err(format!("payload.{key} exceeds {MAX_TEXT_LENGTH} characters"))
        }
        _ => Err(format!("{task} requires a string payload.{key}")),
    }
}

fn optional_int(
    payload: &Value,
    key: &str,
    default: i64,
    minimum: i64,
    maximum: Option<i64>,
) -> Result<i64, String> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(number)) => {
            let Some(value) = number.as_i64() else {
                return Err(format!("payload.{key} must be an integer"));
            };
            if value < minimum || maximum.is_some_and(|max| value > max) {
                let upper = maximum
                    .map(|max| format!("..{max}"))
                    .unwrap_or_else(|| "+".into());
                return Err(format!("payload.{key} must be in {minimum}{upper}"));
            }
            Ok(value)
        }
        _ => Err(format!("payload.{key} must be an integer")),
    }
}

impl WorkerClient {
    pub fn new(base_url: String) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .context("build worker HTTP client")?,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn health(&self) -> Result<WorkerHealth> {
        let response = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .context("worker health check")?;
        if !response.status().is_success() {
            return Err(anyhow!("worker health returned {}", response.status()));
        }
        response
            .json()
            .await
            .context("decode worker health response")
    }

    pub async fn submit(&self, task: &str, payload: &Value) -> Result<WorkerJob> {
        let response = self
            .client
            .post(format!("{}/v1/jobs", self.base_url))
            .json(&SubmitRequest { task, payload })
            .send()
            .await
            .context("submit worker job")?;
        if !response.status().is_success() {
            return Err(anyhow!("worker submit returned {}", response.status()));
        }
        response
            .json()
            .await
            .context("decode worker submit response")
    }

    pub async fn get(&self, id: &str) -> Result<WorkerJob> {
        let response = self
            .client
            .get(format!("{}/v1/jobs/{}", self.base_url, id))
            .send()
            .await
            .context("poll worker job")?;
        if !response.status().is_success() {
            return Err(anyhow!("worker poll returned {}", response.status()));
        }
        response.json().await.context("decode worker poll response")
    }
}

#[cfg(test)]
mod status_tests {
    use super::{
        heartbeat_is_stale, is_terminal_status, retry_delay, validate_task_payload, WorkerHealth,
    };
    use chrono::{Duration as ChronoDuration, Utc};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn only_completed_and_failed_are_terminal() {
        assert!(!is_terminal_status("queued"));
        assert!(!is_terminal_status("running"));
        assert!(is_terminal_status("completed"));
        assert!(is_terminal_status("failed"));
    }

    #[test]
    fn retry_delay_is_bounded_exponential_backoff() {
        assert_eq!(retry_delay(1).as_secs(), 2);
        assert_eq!(retry_delay(2).as_secs(), 4);
        assert_eq!(retry_delay(8).as_secs(), 30);
    }

    #[test]
    fn heartbeat_without_timestamp_is_not_stale_yet() {
        assert!(!heartbeat_is_stale(
            None,
            Utc::now(),
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn recent_heartbeat_is_not_stale() {
        let now = Utc::now();
        let heartbeat = (now - ChronoDuration::seconds(5)).to_rfc3339();
        assert!(!heartbeat_is_stale(
            Some(&heartbeat),
            now,
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn old_heartbeat_is_stale() {
        let now = Utc::now();
        let heartbeat = (now - ChronoDuration::seconds(45)).to_rfc3339();
        assert!(heartbeat_is_stale(
            Some(&heartbeat),
            now,
            Duration::from_secs(30)
        ));
    }

    #[test]
    fn validate_rejects_unknown_task_and_bad_text_payload() {
        assert!(validate_task_payload("missing", &json!({})).is_err());
        assert!(validate_task_payload("text.stats", &json!({"text": 1})).is_err());
        assert!(validate_task_payload("text.stats", &json!({"text": "ok"})).is_ok());
        assert!(validate_task_payload("echo", &json!({"x": 1})).is_ok());
    }

    #[test]
    fn validate_summarize_and_chunk_payloads() {
        assert!(validate_task_payload(
            "text.summarize",
            &json!({"text": "hi", "max_sentences": 3})
        )
        .is_ok());
        assert!(validate_task_payload(
            "text.summarize",
            &json!({"text": "hi", "max_sentences": 0})
        )
        .is_err());
        assert!(validate_task_payload(
            "text.chunk",
            &json!({"text": "hi", "chunk_size": 128, "overlap": 10})
        )
        .is_ok());
        assert!(validate_task_payload(
            "text.chunk",
            &json!({"text": "hi", "chunk_size": 64, "overlap": 64})
        )
        .is_err());
    }

    #[test]
    fn validate_similarity_and_entities_payloads() {
        assert!(validate_task_payload(
            "text.similarity",
            &json!({"text_a": "cats nap", "text_b": "cats sleep"})
        )
        .is_ok());
        assert!(validate_task_payload(
            "text.similarity",
            &json!({"text": "only one field"})
        )
        .is_err());
        assert!(validate_task_payload(
            "text.entities",
            &json!({"text": "see https://example.com and EVOHIME-42"})
        )
        .is_ok());
        assert!(validate_task_payload("text.entities", &json!({"text": 1})).is_err());
        assert!(validate_task_payload(
            "text.diff",
            &json!({"text_a": "a\n", "text_b": "b\n", "context": 2})
        )
        .is_ok());
        assert!(validate_task_payload(
            "text.diff",
            &json!({"text_a": "a", "text_b": "b", "max_diff_lines": 0})
        )
        .is_err());
    }

    #[test]
    fn worker_health_deserializes_liveness_fields() {
        let health: WorkerHealth = serde_json::from_value(json!({
            "status": "ok",
            "worker": "python",
            "started_at": "2026-07-16T10:00:00Z",
            "pid": 4242,
            "queue_depth": 0,
            "active_jobs": 0,
            "supported_tasks": ["echo", "text.stats"]
        }))
        .expect("health payload");
        assert_eq!(health.status, "ok");
        assert_eq!(health.started_at, "2026-07-16T10:00:00Z");
        assert_eq!(health.pid, 4242);
        assert!(health.supported_tasks.contains(&"echo".to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::SubmitRequest;
    use serde_json::json;

    #[test]
    fn submit_request_preserves_task_and_payload_shape() {
        let payload = json!({"text": "hello"});
        let request = serde_json::to_value(SubmitRequest {
            task: "text.keywords",
            payload: &payload,
        })
        .expect("request is serializable");
        assert_eq!(
            request,
            json!({"task": "text.keywords", "payload": {"text": "hello"}})
        );
    }
}
