//! Persist metrics snapshots + Prometheus text exposition (Stage 7.24).

use crate::observability::MetricsSnapshot;
use crate::worker_observability::WorkerMetricsSnapshot;
use serde::Serialize;
use std::time::Duration;

/// Config for periodic PG persistence of `/api/metrics` snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsPersistConfig {
    pub interval: Duration,
    pub history_limit: i64,
}

impl MetricsPersistConfig {
    pub fn from_env() -> Self {
        let interval_secs = parse_u64_env("EVOHIME_METRICS_PERSIST_INTERVAL_SECS", 60);
        let history_limit = parse_i64_env("EVOHIME_METRICS_HISTORY_LIMIT", 2_880);
        Self {
            interval: Duration::from_secs(interval_secs),
            history_limit: history_limit.max(1),
        }
    }

    pub fn enabled(&self) -> bool {
        !self.interval.is_zero()
    }
}

fn parse_u64_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn parse_i64_env(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsPersistStatus {
    pub enabled: bool,
    pub interval_secs: u64,
    pub history_limit: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_persisted_at: Option<String>,
}

/// Render OpenMetrics/Prometheus text for pipeline + worker counters.
pub fn render_prometheus(
    pipeline: &MetricsSnapshot,
    worker: &WorkerMetricsSnapshot,
) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str("# HELP evohime_pipeline_tasks_started Tasks started since process start\n");
    out.push_str("# TYPE evohime_pipeline_tasks_started counter\n");
    push_metric(&mut out, "evohime_pipeline_tasks_started", pipeline.tasks_started);

    out.push_str("# HELP evohime_pipeline_tasks_completed Tasks completed\n");
    out.push_str("# TYPE evohime_pipeline_tasks_completed counter\n");
    push_metric(
        &mut out,
        "evohime_pipeline_tasks_completed",
        pipeline.tasks_completed,
    );

    out.push_str("# HELP evohime_pipeline_tasks_failed Tasks failed\n");
    out.push_str("# TYPE evohime_pipeline_tasks_failed counter\n");
    push_metric(&mut out, "evohime_pipeline_tasks_failed", pipeline.tasks_failed);

    out.push_str("# HELP evohime_pipeline_tools_started Tool invocations started\n");
    out.push_str("# TYPE evohime_pipeline_tools_started counter\n");
    push_metric(&mut out, "evohime_pipeline_tools_started", pipeline.tools_started);

    out.push_str("# HELP evohime_pipeline_tools_completed Tools completed\n");
    out.push_str("# TYPE evohime_pipeline_tools_completed counter\n");
    push_metric(
        &mut out,
        "evohime_pipeline_tools_completed",
        pipeline.tools_completed,
    );

    out.push_str("# HELP evohime_pipeline_tools_failed Tools failed\n");
    out.push_str("# TYPE evohime_pipeline_tools_failed counter\n");
    push_metric(&mut out, "evohime_pipeline_tools_failed", pipeline.tools_failed);

    out.push_str("# HELP evohime_pipeline_approvals_requested Approvals requested\n");
    out.push_str("# TYPE evohime_pipeline_approvals_requested counter\n");
    push_metric(
        &mut out,
        "evohime_pipeline_approvals_requested",
        pipeline.approvals_requested,
    );

    out.push_str("# HELP evohime_pipeline_approvals_granted Approvals granted\n");
    out.push_str("# TYPE evohime_pipeline_approvals_granted counter\n");
    push_metric(
        &mut out,
        "evohime_pipeline_approvals_granted",
        pipeline.approvals_granted,
    );

    out.push_str("# HELP evohime_pipeline_approvals_denied Approvals denied\n");
    out.push_str("# TYPE evohime_pipeline_approvals_denied counter\n");
    push_metric(
        &mut out,
        "evohime_pipeline_approvals_denied",
        pipeline.approvals_denied,
    );

    out.push_str("# HELP evohime_pipeline_task_retries Task retries\n");
    out.push_str("# TYPE evohime_pipeline_task_retries counter\n");
    push_metric(&mut out, "evohime_pipeline_task_retries", pipeline.task_retries);

    out.push_str("# HELP evohime_pipeline_plan_updates Plan updates / replans\n");
    out.push_str("# TYPE evohime_pipeline_plan_updates counter\n");
    push_metric(&mut out, "evohime_pipeline_plan_updates", pipeline.plan_updates);

    out.push_str("# HELP evohime_pipeline_open_tasks Currently open tasks\n");
    out.push_str("# TYPE evohime_pipeline_open_tasks gauge\n");
    push_metric(&mut out, "evohime_pipeline_open_tasks", pipeline.open_tasks);

    out.push_str("# HELP evohime_pipeline_open_approvals Currently open approvals\n");
    out.push_str("# TYPE evohime_pipeline_open_approvals gauge\n");
    push_metric(
        &mut out,
        "evohime_pipeline_open_approvals",
        pipeline.open_approvals,
    );

    out.push_str("# HELP evohime_pipeline_avg_task_duration_ms Average task duration\n");
    out.push_str("# TYPE evohime_pipeline_avg_task_duration_ms gauge\n");
    push_float(
        &mut out,
        "evohime_pipeline_avg_task_duration_ms",
        pipeline.avg_task_duration_ms,
    );

    out.push_str("# HELP evohime_pipeline_avg_tool_duration_ms Average tool duration\n");
    out.push_str("# TYPE evohime_pipeline_avg_tool_duration_ms gauge\n");
    push_float(
        &mut out,
        "evohime_pipeline_avg_tool_duration_ms",
        pipeline.avg_tool_duration_ms,
    );

    out.push_str("# HELP evohime_pipeline_avg_approval_latency_ms Average approval latency\n");
    out.push_str("# TYPE evohime_pipeline_avg_approval_latency_ms gauge\n");
    push_float(
        &mut out,
        "evohime_pipeline_avg_approval_latency_ms",
        pipeline.avg_approval_latency_ms,
    );

    out.push_str("# HELP evohime_pipeline_otel_export_enabled OTLP export configured\n");
    out.push_str("# TYPE evohime_pipeline_otel_export_enabled gauge\n");
    push_metric(
        &mut out,
        "evohime_pipeline_otel_export_enabled",
        u64::from(pipeline.otel_export_enabled),
    );

    out.push_str("# HELP evohime_worker_jobs_submitted Worker jobs submitted\n");
    out.push_str("# TYPE evohime_worker_jobs_submitted counter\n");
    push_metric(
        &mut out,
        "evohime_worker_jobs_submitted",
        worker.jobs_submitted,
    );

    out.push_str("# HELP evohime_worker_jobs_completed Worker jobs completed\n");
    out.push_str("# TYPE evohime_worker_jobs_completed counter\n");
    push_metric(
        &mut out,
        "evohime_worker_jobs_completed",
        worker.jobs_completed,
    );

    out.push_str("# HELP evohime_worker_jobs_failed Worker jobs failed\n");
    out.push_str("# TYPE evohime_worker_jobs_failed counter\n");
    push_metric(&mut out, "evohime_worker_jobs_failed", worker.jobs_failed);

    out.push_str("# HELP evohime_worker_jobs_retried Worker jobs retried\n");
    out.push_str("# TYPE evohime_worker_jobs_retried counter\n");
    push_metric(&mut out, "evohime_worker_jobs_retried", worker.jobs_retried);

    out.push_str("# HELP evohime_worker_jobs_stalled Worker jobs stalled\n");
    out.push_str("# TYPE evohime_worker_jobs_stalled counter\n");
    push_metric(&mut out, "evohime_worker_jobs_stalled", worker.jobs_stalled);

    out.push_str("# HELP evohime_worker_health_checks_ok Successful worker health polls\n");
    out.push_str("# TYPE evohime_worker_health_checks_ok counter\n");
    push_metric(
        &mut out,
        "evohime_worker_health_checks_ok",
        worker.health_checks_ok,
    );

    out.push_str("# HELP evohime_worker_health_checks_failed Failed worker health polls\n");
    out.push_str("# TYPE evohime_worker_health_checks_failed counter\n");
    push_metric(
        &mut out,
        "evohime_worker_health_checks_failed",
        worker.health_checks_failed,
    );

    out.push_str("# HELP evohime_worker_recoveries Worker job recoveries\n");
    out.push_str("# TYPE evohime_worker_recoveries counter\n");
    push_metric(&mut out, "evohime_worker_recoveries", worker.recoveries);

    out.push_str("# HELP evohime_worker_open_jobs Currently open worker jobs\n");
    out.push_str("# TYPE evohime_worker_open_jobs gauge\n");
    push_metric(&mut out, "evohime_worker_open_jobs", worker.open_jobs);

    out.push_str("# HELP evohime_worker_avg_job_duration_ms Average worker job duration\n");
    out.push_str("# TYPE evohime_worker_avg_job_duration_ms gauge\n");
    push_float(
        &mut out,
        "evohime_worker_avg_job_duration_ms",
        worker.avg_job_duration_ms,
    );

    if let Some(health) = &worker.last_health {
        out.push_str("# HELP evohime_worker_healthy Last worker health probe\n");
        out.push_str("# TYPE evohime_worker_healthy gauge\n");
        push_metric(
            &mut out,
            "evohime_worker_healthy",
            u64::from(health.healthy),
        );
        if let Some(queue_depth) = health.queue_depth {
            out.push_str("# HELP evohime_worker_queue_depth Worker queue depth from last health\n");
            out.push_str("# TYPE evohime_worker_queue_depth gauge\n");
            push_metric(&mut out, "evohime_worker_queue_depth", queue_depth as u64);
        }
        if let Some(active_jobs) = health.active_jobs {
            out.push_str("# HELP evohime_worker_active_jobs Active jobs from last health\n");
            out.push_str("# TYPE evohime_worker_active_jobs gauge\n");
            push_metric(&mut out, "evohime_worker_active_jobs", active_jobs as u64);
        }
    }

    out
}

fn push_metric(out: &mut String, name: &str, value: u64) {
    out.push_str(name);
    out.push(' ');
    out.push_str(&value.to_string());
    out.push('\n');
}

fn push_float(out: &mut String, name: &str, value: f64) {
    out.push_str(name);
    out.push(' ');
    if value.is_finite() {
        out.push_str(&format!("{value}"));
    } else {
        out.push('0');
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::MetricsSnapshot;
    use crate::worker_observability::WorkerMetricsSnapshot;

    #[test]
    fn prometheus_text_includes_pipeline_and_worker_counters() {
        let pipeline = MetricsSnapshot {
            tasks_started: 3,
            tasks_completed: 2,
            tasks_failed: 1,
            tools_started: 4,
            tools_completed: 3,
            tools_failed: 0,
            approvals_requested: 1,
            approvals_granted: 1,
            approvals_denied: 0,
            task_retries: 0,
            plan_updates: 1,
            open_tasks: 1,
            open_approvals: 0,
            avg_task_duration_ms: 12.5,
            avg_tool_duration_ms: 3.0,
            avg_approval_latency_ms: 0.0,
            otel_export_enabled: false,
        };
        let worker = WorkerMetricsSnapshot {
            jobs_submitted: 5,
            jobs_completed: 4,
            jobs_failed: 1,
            jobs_retried: 0,
            jobs_stalled: 0,
            health_checks_ok: 2,
            health_checks_failed: 0,
            recoveries: 0,
            open_jobs: 1,
            avg_job_duration_ms: 40.0,
            last_health: None,
        };
        let text = render_prometheus(&pipeline, &worker);
        assert!(text.contains("evohime_pipeline_tasks_started 3"));
        assert!(text.contains("evohime_worker_jobs_submitted 5"));
        assert!(text.contains("# TYPE evohime_pipeline_open_tasks gauge"));
    }

    #[test]
    fn persist_config_zero_interval_disables() {
        let cfg = MetricsPersistConfig {
            interval: Duration::from_secs(0),
            history_limit: 10,
        };
        assert!(!cfg.enabled());
    }
}
