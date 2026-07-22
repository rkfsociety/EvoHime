//! Optional OpenTelemetry OTLP export for the task pipeline.
//!
//! Enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set and `OTEL_SDK_DISABLED` is not `true`.
//! Without that endpoint, tracing stays fmt-only (same as before).

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::resource::Resource;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Holds the tracer provider so spans flush on drop / shutdown.
pub struct OtelGuard {
    provider: Option<SdkTracerProvider>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            if let Err(error) = provider.shutdown() {
                eprintln!("opentelemetry shutdown failed: {error}");
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
        return Ok(OtelGuard { provider: None });
    }

    let exporter = SpanExporter::builder()
        .with_http()
        .build()
        .context("build OTLP span exporter")?;

    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "evohime-server".to_string());

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder_empty()
                .with_attributes([KeyValue::new("service.name", service_name)])
                .build(),
        )
        .build();

    let tracer = provider.tracer("evohime-server");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(otel_layer)
        .init();

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
    tracing::info!(
        endpoint = %endpoint,
        "opentelemetry OTLP export enabled"
    );

    Ok(OtelGuard {
        provider: Some(provider),
    })
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
