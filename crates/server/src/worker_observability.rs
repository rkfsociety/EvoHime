//! Worker-subsystem observability: counters, last health snapshot, structured logs.
//!
//! Mutex poison is recovered via `into_inner` (Stage 7.20) so metrics never panic the server.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// In-process metrics for the Python worker bridge.
#[derive(Debug, Default)]
pub struct WorkerMetrics {
    inner: Mutex<WorkerMetricsInner>,
}

#[derive(Debug, Default)]
struct WorkerMetricsInner {
    jobs_submitted: u64,
    jobs_completed: u64,
    jobs_failed: u64,
    jobs_retried: u64,
    jobs_stalled: u64,
    health_checks_ok: u64,
    health_checks_failed: u64,
    recoveries: u64,
    job_duration_ms_total: u64,
    job_duration_samples: u64,
    open_jobs: HashMap<Uuid, Instant>,
    last_health: Option<WorkerHealthView>,
}

/// Last observed worker health (from `/health` poll).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkerHealthView {
    pub healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_depth: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_jobs: Option<i64>,
    pub checked_at_ms: u64,
}

/// Serializable worker metrics snapshot.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkerMetricsSnapshot {
    pub jobs_submitted: u64,
    pub jobs_completed: u64,
    pub jobs_failed: u64,
    pub jobs_retried: u64,
    pub jobs_stalled: u64,
    pub health_checks_ok: u64,
    pub health_checks_failed: u64,
    pub recoveries: u64,
    pub open_jobs: u64,
    pub avg_job_duration_ms: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_health: Option<WorkerHealthView>,
}

impl WorkerMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> WorkerMetricsSnapshot {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        WorkerMetricsSnapshot {
            jobs_submitted: inner.jobs_submitted,
            jobs_completed: inner.jobs_completed,
            jobs_failed: inner.jobs_failed,
            jobs_retried: inner.jobs_retried,
            jobs_stalled: inner.jobs_stalled,
            health_checks_ok: inner.health_checks_ok,
            health_checks_failed: inner.health_checks_failed,
            recoveries: inner.recoveries,
            open_jobs: inner.open_jobs.len() as u64,
            avg_job_duration_ms: avg_ms(inner.job_duration_ms_total, inner.job_duration_samples),
            last_health: inner.last_health.clone(),
        }
    }

    pub fn job_submitted(&self, job_id: Uuid, task: &str) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.jobs_submitted += 1;
        inner.open_jobs.insert(job_id, Instant::now());
        tracing::info!(
            job_id = %job_id,
            task,
            "worker.pipeline.job_submitted"
        );
    }

    pub fn job_finished(&self, job_id: Uuid, task: &str, ok: bool) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed = inner
            .open_jobs
            .remove(&job_id)
            .map(|started| started.elapsed());
        if ok {
            inner.jobs_completed += 1;
        } else {
            inner.jobs_failed += 1;
        }
        if let Some(elapsed) = elapsed {
            let ms = duration_ms(elapsed);
            inner.job_duration_ms_total = inner.job_duration_ms_total.saturating_add(ms);
            inner.job_duration_samples = inner.job_duration_samples.saturating_add(1);
        }
        tracing::info!(
            job_id = %job_id,
            task,
            ok,
            duration_ms = elapsed.map(duration_ms).unwrap_or(0),
            "worker.pipeline.job_finished"
        );
    }

    pub fn job_retried(&self, job_id: Uuid, task: &str, reason: &str) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.jobs_retried += 1;
        tracing::info!(
            job_id = %job_id,
            task,
            reason,
            "worker.pipeline.job_retried"
        );
    }

    pub fn job_stalled(&self, job_id: Uuid, task: &str) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.jobs_stalled += 1;
        tracing::warn!(
            job_id = %job_id,
            task,
            "worker.pipeline.job_stalled"
        );
    }

    pub fn health_ok(
        &self,
        started_at: String,
        pid: i64,
        queue_depth: Option<i64>,
        active_jobs: Option<i64>,
    ) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.health_checks_ok += 1;
        inner.last_health = Some(WorkerHealthView {
            healthy: true,
            started_at: Some(started_at),
            pid: Some(pid),
            queue_depth,
            active_jobs,
            checked_at_ms: now_ms(),
        });
        tracing::debug!(pid, queue_depth, active_jobs, "worker.pipeline.health_ok");
    }

    pub fn health_failed(&self, error: &str) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.health_checks_failed += 1;
        let previous = inner.last_health.clone();
        inner.last_health = Some(WorkerHealthView {
            healthy: false,
            started_at: previous.as_ref().and_then(|view| view.started_at.clone()),
            pid: previous.as_ref().and_then(|view| view.pid),
            queue_depth: None,
            active_jobs: None,
            checked_at_ms: now_ms(),
        });
        tracing::warn!(error, "worker.pipeline.health_failed");
    }

    pub fn recovery(&self, count: usize, reason: &str) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.recoveries += 1;
        tracing::info!(count, reason, "worker.pipeline.recovery");
    }
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn avg_ms(total: u64, samples: u64) -> f64 {
    if samples == 0 {
        0.0
    } else {
        total as f64 / samples as f64
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn records_submit_finish_retry_and_health() {
        let metrics = WorkerMetrics::new();
        let job = Uuid::new_v4();
        metrics.job_submitted(job, "text.stats");
        thread::sleep(Duration::from_millis(5));
        metrics.job_stalled(job, "text.stats");
        metrics.job_retried(job, "text.stats", "heartbeat stalled");
        metrics.job_finished(job, "text.stats", true);
        metrics.health_ok("2026-07-17T00:00:00Z".into(), 42, Some(1), Some(0));
        metrics.health_failed("connection refused");
        metrics.recovery(2, "worker restart");

        let snap = metrics.snapshot();
        assert_eq!(snap.jobs_submitted, 1);
        assert_eq!(snap.jobs_completed, 1);
        assert_eq!(snap.jobs_stalled, 1);
        assert_eq!(snap.jobs_retried, 1);
        assert_eq!(snap.health_checks_ok, 1);
        assert_eq!(snap.health_checks_failed, 1);
        assert_eq!(snap.recoveries, 1);
        assert_eq!(snap.open_jobs, 0);
        assert!(snap.avg_job_duration_ms >= 1.0);
        assert_eq!(snap.last_health.as_ref().map(|h| h.healthy), Some(false));
    }
}
