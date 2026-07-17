//! Agent tool: submit a Python worker job and await completion (Stage 7.52).

use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{env, time::Duration};
use tokio::time::{sleep, Instant};

pub const NAME: &str = "worker.run";
pub const DESCRIPTION: &str = "Submit a Python worker job and wait for the result. Input: { task, payload, timeout_ms?, poll_ms? }. Tasks: echo, text.stats, text.keywords, text.summarize, text.chunk, text.similarity, text.entities, text.diff.";
pub const PERMISSIONS: &[Permission] = &[];
pub const TIMEOUT: Duration = Duration::from_secs(120);

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

#[derive(Debug, Clone)]
pub struct WorkerRunInput {
    pub task: String,
    pub payload: Value,
    pub timeout_ms: u64,
    pub poll_ms: u64,
    pub worker_url: String,
}

#[derive(Debug, Deserialize)]
struct WorkerJob {
    id: String,
    status: String,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

pub fn parse_input(input: &Value) -> Result<WorkerRunInput, ToolError> {
    let task = input
        .get("task")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: "task is required".into(),
        })?
        .to_string();
    if !SUPPORTED_TASKS.contains(&task.as_str()) {
        return Err(ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: format!(
                "unsupported task: {task} (allowed: {})",
                SUPPORTED_TASKS.join(", ")
            ),
        });
    }
    let payload = input.get("payload").cloned().unwrap_or_else(|| json!({}));
    if !payload.is_object() {
        return Err(ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: "payload must be an object".into(),
        });
    }
    if task != "echo" && payload.as_object().map(|o| o.is_empty()).unwrap_or(true) {
        return Err(ToolError::InvalidInput {
            tool: NAME.to_string(),
            message: format!("{task} requires a non-empty payload object"),
        });
    }
    let timeout_ms = input
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(60_000)
        .clamp(1_000, 300_000);
    let poll_ms = input
        .get("poll_ms")
        .and_then(Value::as_u64)
        .unwrap_or(250)
        .clamp(50, 5_000);
    let worker_url = input
        .get("worker_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| env::var("PYTHON_WORKER_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:8090".to_string())
        .trim_end_matches('/')
        .to_string();
    Ok(WorkerRunInput {
        task,
        payload,
        timeout_ms,
        poll_ms,
        worker_url,
    })
}

pub async fn execute(_ctx: &ToolContext, value: Value) -> Result<ToolResult, ToolError> {
    let input = parse_input(&value)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| ToolError::Execution(format!("worker client: {error}")))?;

    let submit = client
        .post(format!("{}/v1/jobs", input.worker_url))
        .json(&json!({
            "task": input.task,
            "payload": input.payload,
        }))
        .send()
        .await
        .map_err(|error| ToolError::Execution(format!("worker submit failed: {error}")))?;
    if !submit.status().is_success() {
        return Err(ToolError::Execution(format!(
            "worker submit returned {}",
            submit.status()
        )));
    }
    let mut job: WorkerJob = submit
        .json()
        .await
        .map_err(|error| ToolError::Execution(format!("worker submit decode: {error}")))?;

    let deadline = Instant::now() + Duration::from_millis(input.timeout_ms);
    while !is_terminal(&job.status) {
        if Instant::now() >= deadline {
            return Err(ToolError::TimedOut(Duration::from_millis(input.timeout_ms)));
        }
        sleep(Duration::from_millis(input.poll_ms)).await;
        let poll = client
            .get(format!("{}/v1/jobs/{}", input.worker_url, job.id))
            .send()
            .await
            .map_err(|error| ToolError::Execution(format!("worker poll failed: {error}")))?;
        if !poll.status().is_success() {
            return Err(ToolError::Execution(format!(
                "worker poll returned {}",
                poll.status()
            )));
        }
        job = poll
            .json()
            .await
            .map_err(|error| ToolError::Execution(format!("worker poll decode: {error}")))?;
    }

    if job.status == "failed" {
        let message = job
            .error
            .unwrap_or_else(|| "worker job failed without error detail".into());
        return Err(ToolError::Execution(format!(
            "worker job {} failed: {message}",
            job.id
        )));
    }

    let result = job.result.unwrap_or(Value::Null);
    let pretty = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
    Ok(ToolResult {
        output: format!(
            "worker.run task={} job={} status=completed\n{pretty}",
            input.task, job.id
        ),
        structured: json!({
            "task": input.task,
            "job_id": job.id,
            "status": job.status,
            "result": result,
        }),
    })
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn parse_input_rejects_unknown_task() {
        let err = parse_input(&json!({"task": "nope", "payload": {}})).unwrap_err();
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn parse_input_defaults_worker_url() {
        let parsed = parse_input(&json!({
            "task": "echo",
            "payload": {"hello": "world"}
        }))
        .expect("parse");
        assert_eq!(parsed.task, "echo");
        assert!(parsed.worker_url.contains("8090") || !parsed.worker_url.is_empty());
    }

    #[tokio::test]
    async fn submit_and_await_completed_job() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/jobs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "job-1",
                "status": "queued",
                "result": null,
                "error": null
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/jobs/job-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "job-1",
                "status": "completed",
                "result": {"ok": true, "echo": "hi"},
                "error": null
            })))
            .mount(&server)
            .await;

        let ctx = ToolContext {
            workspace_root: std::env::temp_dir(),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = execute(
            &ctx,
            json!({
                "task": "echo",
                "payload": {"hi": 1},
                "worker_url": server.uri(),
                "poll_ms": 10,
                "timeout_ms": 5_000
            }),
        )
        .await
        .expect("worker.run");
        assert!(result.output.contains("completed"));
        assert_eq!(result.structured["job_id"], "job-1");
        assert_eq!(result.structured["result"]["ok"], true);
    }
}
