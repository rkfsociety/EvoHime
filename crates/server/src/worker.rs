use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

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
        Ok(response
            .json()
            .await
            .context("decode worker submit response")?)
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
        Ok(response
            .json()
            .await
            .context("decode worker poll response")?)
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
