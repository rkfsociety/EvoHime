//! Optional OpenTelemetry OTLP export for the task pipeline.
//!
//! Enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set and `OTEL_SDK_DISABLED` is not `true`.
//! Without that endpoint, tracing stays fmt-only (same as before).

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::{MetricExporter, SpanExporter};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::resource::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::Arc;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::observability::PipelineMetrics;
use crate::worker_observability::WorkerMetrics;

/// Holds the tracer + meter providers so spans/metrics flush on drop / shutdown.
pub struct OtelGuard {
    provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            if let Err(error) = provider.shutdown() {
                eprintln!("opentelemetry shutdown failed: {error}");
            }
        }
        if let Some(provider) = self.meter_provider.take() {
            if let Err(error) = provider.shutdown() {
                eprintln!("opentelemetry metrics shutdown failed: {error}");
            }
        }
    }
}

/// Initialize fmt tracing, and optionally an OTLP trace pipeline.
pub fn init_tracing() -> Result<OtelGuard> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,evohime_server=info"));

    if !otel_enabled() {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
        return Ok(OtelGuard {
            provider: None,
            meter_provider: None,
        });
    }

    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "evohime-server".to_string());
    let resource = Resource::builder_empty()
        .with_attributes([KeyValue::new("service.name", service_name)])
        .build();

    let exporter = SpanExporter::builder()
        .with_http()
        .build()
        .context("build OTLP span exporter")?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource.clone())
        .build();

    let tracer = provider.tracer("evohime-server");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(otel_layer)
        .init();

    let metric_exporter = MetricExporter::builder()
        .with_http()
        .build()
        .context("build OTLP metric exporter")?;

    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource)
        .build();
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
    tracing::info!(
        endpoint = %crate::log_safety::redact_for_log(&endpoint),
        "opentelemetry OTLP trace + metrics export enabled"
    );

    Ok(OtelGuard {
        provider: Some(provider),
        meter_provider: Some(meter_provider),
    })
}

/// Registers observable OTLP counters/gauges mirroring `/metrics` (Prometheus) and
/// `/api/metrics` (JSON). Call once after `AppState`'s metrics registries exist.
/// No-op when OTLP export is disabled — callbacks are never registered, so there is
/// no polling overhead on the hot path.
pub fn register_pipeline_metrics(pipeline: Arc<PipelineMetrics>, worker: Arc<WorkerMetrics>) {
    if !otel_enabled() {
        return;
    }
    let meter = opentelemetry::global::meter("evohime-server");

    macro_rules! counter {
        ($name:literal, $help:literal, $field:ident) => {
            let p = pipeline.clone();
            let _ = meter
                .u64_observable_counter($name)
                .with_description($help)
                .with_callback(move |observer| observer.observe(p.snapshot().$field, &[]))
                .build();
        };
    }
    macro_rules! gauge_u64 {
        ($name:literal, $help:literal, $field:ident) => {
            let p = pipeline.clone();
            let _ = meter
                .u64_observable_gauge($name)
                .with_description($help)
                .with_callback(move |observer| observer.observe(p.snapshot().$field, &[]))
                .build();
        };
    }
    macro_rules! gauge_f64 {
        ($name:literal, $help:literal, $field:ident) => {
            let p = pipeline.clone();
            let _ = meter
                .f64_observable_gauge($name)
                .with_description($help)
                .with_callback(move |observer| observer.observe(p.snapshot().$field, &[]))
                .build();
        };
    }

    counter!(
        "evohime.pipeline.tasks_started",
        "Tasks started since process start",
        tasks_started
    );
    counter!(
        "evohime.pipeline.tasks_completed",
        "Tasks completed",
        tasks_completed
    );
    counter!("evohime.pipeline.tasks_failed", "Tasks failed", tasks_failed);
    counter!(
        "evohime.pipeline.tools_started",
        "Tool invocations started",
        tools_started
    );
    counter!(
        "evohime.pipeline.tools_completed",
        "Tools completed",
        tools_completed
    );
    counter!("evohime.pipeline.tools_failed", "Tools failed", tools_failed);
    counter!(
        "evohime.pipeline.approvals_requested",
        "Approvals requested",
        approvals_requested
    );
    counter!(
        "evohime.pipeline.approvals_granted",
        "Approvals granted",
        approvals_granted
    );
    counter!(
        "evohime.pipeline.approvals_denied",
        "Approvals denied",
        approvals_denied
    );
    counter!(
        "evohime.pipeline.task_retries",
        "Task retries",
        task_retries
    );
    counter!(
        "evohime.pipeline.plan_updates",
        "Plan updates / replans",
        plan_updates
    );
    counter!("evohime.pipeline.llm_calls", "LLM chat completions", llm_calls);
    counter!(
        "evohime.pipeline.llm_calls_failed",
        "Failed LLM chat completions",
        llm_calls_failed
    );
    counter!(
        "evohime.pipeline.llm_prompt_tokens",
        "Prompt tokens reported by provider",
        llm_prompt_tokens
    );
    counter!(
        "evohime.pipeline.llm_completion_tokens",
        "Completion tokens reported by provider",
        llm_completion_tokens
    );
    gauge_u64!(
        "evohime.pipeline.open_tasks",
        "Currently open tasks",
        open_tasks
    );
    gauge_u64!(
        "evohime.pipeline.open_approvals",
        "Currently open approvals",
        open_approvals
    );
    gauge_f64!(
        "evohime.pipeline.avg_task_duration_ms",
        "Average task duration",
        avg_task_duration_ms
    );
    gauge_f64!(
        "evohime.pipeline.avg_tool_duration_ms",
        "Average tool duration",
        avg_tool_duration_ms
    );
    gauge_f64!(
        "evohime.pipeline.avg_approval_latency_ms",
        "Average approval latency",
        avg_approval_latency_ms
    );
    gauge_f64!(
        "evohime.pipeline.avg_llm_duration_ms",
        "Average LLM call duration",
        avg_llm_duration_ms
    );

    macro_rules! worker_counter {
        ($name:literal, $help:literal, $field:ident) => {
            let w = worker.clone();
            let _ = meter
                .u64_observable_counter($name)
                .with_description($help)
                .with_callback(move |observer| observer.observe(w.snapshot().$field, &[]))
                .build();
        };
    }
    worker_counter!(
        "evohime.worker.jobs_submitted",
        "Worker jobs submitted",
        jobs_submitted
    );
    worker_counter!(
        "evohime.worker.jobs_completed",
        "Worker jobs completed",
        jobs_completed
    );
    worker_counter!("evohime.worker.jobs_failed", "Worker jobs failed", jobs_failed);
    worker_counter!(
        "evohime.worker.jobs_retried",
        "Worker jobs retried",
        jobs_retried
    );
    worker_counter!(
        "evohime.worker.jobs_stalled",
        "Worker jobs stalled",
        jobs_stalled
    );
    worker_counter!(
        "evohime.worker.health_checks_ok",
        "Successful worker health polls",
        health_checks_ok
    );
    worker_counter!(
        "evohime.worker.health_checks_failed",
        "Failed worker health polls",
        health_checks_failed
    );
    worker_counter!(
        "evohime.worker.recoveries",
        "Worker job recoveries",
        recoveries
    );

    let w = worker.clone();
    let _ = meter
        .u64_observable_gauge("evohime.worker.open_jobs")
        .with_description("Currently open worker jobs")
        .with_callback(move |observer| observer.observe(w.snapshot().open_jobs, &[]))
        .build();
    let w = worker.clone();
    let _ = meter
        .f64_observable_gauge("evohime.worker.avg_job_duration_ms")
        .with_description("Average worker job duration")
        .with_callback(move |observer| observer.observe(w.snapshot().avg_job_duration_ms, &[]))
        .build();

    tracing::info!("opentelemetry pipeline/worker metric instruments registered");
}

/// Whether OTLP export is currently configured (does not require init).
pub fn export_enabled() -> bool {
    otel_enabled()
}

fn otel_enabled() -> bool {
    if !crate::features::enabled("EVOHIME_FEATURE_OTLP", true) {
        return false;
    }
    if std::env::var("OTEL_SDK_DISABLED")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return false;
    }
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::otel_enabled;

    #[test]
    fn otel_disabled_without_endpoint() {
        // SAFETY: test process; env is isolated per test binary run.
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        std::env::remove_var("OTEL_SDK_DISABLED");
        assert!(!otel_enabled());
    }

    #[test]
    fn otel_respects_sdk_disabled() {
        std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:4318");
        std::env::set_var("OTEL_SDK_DISABLED", "true");
        assert!(!otel_enabled());
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        std::env::remove_var("OTEL_SDK_DISABLED");
    }
}
