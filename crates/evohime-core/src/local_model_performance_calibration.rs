//! Core-owned, metadata-only local inference calibration contract.
//!
//! This module deliberately does not launch a process or invent telemetry.
//! Runtime execution is an adapter boundary; absent verified adapter data is
//! represented as `Unavailable` rather than as an estimate.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable contract identifier for persisted local calibration data.
pub const CONTRACT_ID: &str = "local-model-performance-calibration-v1";
/// Maximum samples accepted in one calibration session.
pub const MAX_SAMPLES: usize = 256;
/// Maximum context-size points retained in a performance profile.
pub const MAX_CONTEXT_POINTS: usize = 32;
/// Maximum character count for calibration identity metadata.
pub const MAX_METADATA_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Result of checking whether a runtime session may be calibrated.
pub enum CalibrationAdmission {
    /// Runtime identity and a usable stream adapter permit calibration.
    Approved,
    /// Runtime is eligible, but no compatible stream adapter can report measurements.
    UnavailableAdapter,
    /// Runtime session, descriptor, or health state does not match.
    DeniedRuntime,
}

/// Admission is intentionally separate from execution. The current runtime
/// manager can prove identity/health, but only a future versioned stream
/// adapter can provide inference timestamps and token metrics.
pub fn admit_calibration(
    session: &crate::local_model_runtime_manager::LocalModelRuntimeSession,
    descriptor: &crate::local_model_runtime_manager::LocalModelDescriptor,
    runtime: &crate::local_model_runtime_manager::LocalInferenceRuntime,
    stream_adapter_available: bool,
) -> Result<CalibrationAdmission, CalibrationError> {
    session
        .validate()
        .map_err(|_| CalibrationError::InvalidIdentity)?;
    descriptor
        .validate()
        .map_err(|_| CalibrationError::InvalidIdentity)?;
    runtime
        .validate()
        .map_err(|_| CalibrationError::InvalidIdentity)?;
    if session.state != crate::local_model_runtime_manager::RuntimeState::Ready
        || runtime.state != crate::local_model_runtime_manager::RuntimeState::Ready
        || session.model_id != descriptor.model_id
        || session.runtime_id != runtime.runtime_id
        || session.artifact_hash != descriptor.artifact_hash
    {
        return Ok(CalibrationAdmission::DeniedRuntime);
    }
    Ok(if stream_adapter_available {
        CalibrationAdmission::Approved
    } else {
        CalibrationAdmission::UnavailableAdapter
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Hashed model, runtime, hardware, launch, and benchmark-suite identity.
pub struct CalibrationIdentity {
    /// SHA-256 hash of the exact model artifact.
    pub model_artifact_hash: String,
    /// Stable identifier of the inference runtime.
    pub runtime_id: String,
    /// Runtime version used for the measurements.
    pub runtime_version: String,
    /// Hash of the normalized local hardware profile.
    pub hardware_profile_hash: String,
    /// Optional fingerprint of relevant accelerator drivers.
    pub driver_fingerprint: Option<String>,
    /// Hash of the launch settings used for calibration.
    pub launch_config_hash: String,
    /// Context-size and runtime profile selected for the workload.
    pub context_profile: String,
    /// Hash of the benchmark workload suite.
    pub suite_hash: String,
}

impl CalibrationIdentity {
    /// Validates this identity, sample, session, or profile against its invariants.
    pub fn validate(&self) -> Result<(), CalibrationError> {
        for (name, value) in [
            ("model_artifact_hash", &self.model_artifact_hash),
            ("runtime_id", &self.runtime_id),
            ("runtime_version", &self.runtime_version),
            ("hardware_profile_hash", &self.hardware_profile_hash),
            ("launch_config_hash", &self.launch_config_hash),
            ("context_profile", &self.context_profile),
            ("suite_hash", &self.suite_hash),
        ] {
            bounded(name, value)?;
        }
        if !hex_hash(&self.model_artifact_hash)
            || !hex_hash(&self.hardware_profile_hash)
            || !hex_hash(&self.launch_config_hash)
            || !hex_hash(&self.suite_hash)
        {
            return Err(CalibrationError::InvalidIdentity);
        }
        if let Some(driver) = &self.driver_fingerprint {
            bounded("driver_fingerprint", driver)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Representative workload shape used for a calibration sample.
pub enum PromptProfile {
    /// Short prompt and response workload for interactive use.
    ShortInteractive,
    /// Medium-length generation workload.
    MediumGeneration,
    /// Long input workload emphasizing prompt processing.
    LongContextPrefill,
    /// Structured response workload with a small output.
    StructuredSmallOutput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Whether a sample warms the runtime or contributes to measured metrics.
pub enum SampleKind {
    /// Sample used to initialize the runtime and excluded from aggregates.
    Warmup,
    /// Sample included in performance aggregates when valid.
    Measured,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Whether the sample contains usable performance measurements.
pub enum MetricAvailability {
    /// One or more requested performance metrics are present.
    Available,
    /// No trusted evidence is available.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// One validated local inference timing and resource observation.
pub struct CalibrationSample {
    /// Stable identifier for this sample.
    pub sample_id: String,
    /// Warmup or measured sample classification.
    pub kind: SampleKind,
    /// Workload shape represented by this sample.
    pub prompt_profile: PromptProfile,
    /// Context size used for the inference call.
    pub context_tokens: u32,
    /// Number of input tokens processed.
    pub input_tokens: u32,
    /// Number of output tokens generated.
    pub output_tokens: u32,
    /// Optional model load duration in milliseconds.
    pub load_ms: Option<u64>,
    /// Optional time to first generated token in milliseconds.
    pub ttft_ms: Option<u64>,
    /// Optional prompt-processing throughput.
    pub prefill_tokens_per_second: Option<f64>,
    /// Optional generation throughput.
    pub decode_tokens_per_second: Option<f64>,
    /// Optional total inference duration in milliseconds.
    pub end_to_end_ms: Option<u64>,
    /// Optional peak system memory usage.
    pub peak_ram_bytes: Option<u64>,
    /// Optional peak accelerator memory usage.
    pub peak_vram_bytes: Option<u64>,
    /// Whether measured metrics are available for this sample.
    pub availability: MetricAvailability,
    /// Whether this sample passed calibration validation.
    pub valid: bool,
    /// Reason the inference request completed or stopped.
    pub termination: String,
    /// Runtime health state observed for the sample.
    pub health: String,
}

impl CalibrationSample {
    /// Validates this identity, sample, session, or profile against its invariants.
    pub fn validate(&self) -> Result<(), CalibrationError> {
        bounded("sample_id", &self.sample_id)?;
        bounded("termination", &self.termination)?;
        bounded("health", &self.health)?;
        if self.context_tokens == 0 || self.output_tokens == 0 {
            return Err(CalibrationError::InvalidMetric);
        }
        for value in [
            self.prefill_tokens_per_second,
            self.decode_tokens_per_second,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() || value < 0.0 {
                return Err(CalibrationError::InvalidMetric);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Robust summary statistics derived from valid measured samples.
pub struct AggregateMetrics {
    /// Number of valid measured samples used in the aggregate.
    pub measured_count: u32,
    /// Fraction of measured samples that failed validation or execution.
    pub failure_rate: f64,
    /// Median observed time to first token in milliseconds.
    pub median_ttft_ms: Option<f64>,
    /// Ninetieth percentile time to first token in milliseconds.
    pub p90_ttft_ms: Option<f64>,
    /// Median generation throughput.
    pub median_decode_tokens_per_second: Option<f64>,
    /// Variance of generation throughput across samples.
    pub variance_decode_tokens_per_second: Option<f64>,
    /// Whether any aggregate performance metric was available.
    pub metric_availability: MetricAvailability,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Provenance category for calibration evidence.
pub enum EvidenceClass {
    /// Measured on the exact local model and runtime identity.
    MeasuredLocal,
    /// Previously measured locally but runtime or model identity has drifted.
    MeasuredLocalStale,
    /// Estimated from compatible hardware evidence.
    EstimatedFromHardware,
    /// Estimated from a catalog entry.
    CatalogEstimate,
    /// No trusted evidence is available.
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of one calibration session.
pub enum CalibrationState {
    /// Session is waiting for an execution slot.
    Queued,
    /// Runtime is performing warmup requests.
    WarmingUp,
    /// Measured inference requests are in progress.
    Running,
    /// Validated samples are being summarized.
    Aggregating,
    /// Session has valid measured samples and no cancellation request.
    Completed,
    /// Session was cancelled.
    Cancelled,
    /// Session failed before valid aggregation.
    Failed,
    /// Required measurement adapter is unavailable.
    Unavailable,
    /// Session stopped before producing a complete result.
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Validated identity, samples, and cancellation state for a calibration run.
pub struct LocalModelCalibrationSession {
    /// Stable calibration session identifier.
    pub session_id: String,
    /// Exact model, runtime, hardware, and benchmark identity.
    pub identity: CalibrationIdentity,
    /// Current calibration lifecycle state.
    pub state: CalibrationState,
    /// Warmup and measured samples retained by the session.
    pub samples: Vec<CalibrationSample>,
    /// Whether the session has a cancellation request.
    pub cancellation_requested: bool,
}

impl LocalModelCalibrationSession {
    /// Validates this identity, sample, session, or profile against its invariants.
    pub fn validate(&self) -> Result<(), CalibrationError> {
        bounded("session_id", &self.session_id)?;
        self.identity.validate()?;
        if self.samples.len() > MAX_SAMPLES {
            return Err(CalibrationError::LimitExceeded("samples"));
        }
        for sample in &self.samples {
            sample.validate()?;
        }
        if matches!(self.state, CalibrationState::Completed)
            && (!self
                .samples
                .iter()
                .any(|sample| sample.kind == SampleKind::Measured && sample.valid)
                || self.cancellation_requested)
        {
            return Err(CalibrationError::InvalidState);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Versioned aggregate performance data for one exact runtime identity.
pub struct LocalModelPerformanceProfile {
    /// Stable performance profile identifier.
    pub profile_id: String,
    /// Positive profile revision.
    pub revision: u64,
    /// Exact model, runtime, hardware, and benchmark identity.
    pub identity: CalibrationIdentity,
    /// Aggregate metrics calculated from valid samples.
    pub aggregate: AggregateMetrics,
    /// Provenance class for the aggregate evidence.
    pub evidence: EvidenceClass,
    /// Measured throughput observations indexed by context size.
    pub context_points: Vec<(u32, Option<f64>)>,
    /// Largest context size recommended for comfortable use.
    pub comfortable_context: Option<u32>,
    /// Recommended context size for interactive latency.
    pub interactive_context: Option<u32>,
    /// Largest context size with a valid local measurement.
    pub maximum_measured_context: Option<u32>,
    /// Bounded confidence label for the profile.
    pub confidence: String,
}

impl LocalModelPerformanceProfile {
    /// Validates this identity, sample, session, or profile against its invariants.
    pub fn validate(&self) -> Result<(), CalibrationError> {
        bounded("profile_id", &self.profile_id)?;
        bounded("confidence", &self.confidence)?;
        self.identity.validate()?;
        if self.revision == 0 || self.context_points.len() > MAX_CONTEXT_POINTS {
            return Err(CalibrationError::InvalidIdentity);
        }
        if self.evidence == EvidenceClass::MeasuredLocal && self.aggregate.measured_count == 0 {
            return Err(CalibrationError::InvalidState);
        }
        Ok(())
    }
}

/// Computes failure rate and robust latency/throughput summaries from measured samples.
pub fn aggregate_samples(
    samples: &[CalibrationSample],
) -> Result<AggregateMetrics, CalibrationError> {
    if samples.len() > MAX_SAMPLES {
        return Err(CalibrationError::LimitExceeded("samples"));
    }
    let measured: Vec<&CalibrationSample> = samples
        .iter()
        .filter(|sample| sample.kind == SampleKind::Measured)
        .collect();
    let valid: Vec<&CalibrationSample> = measured
        .iter()
        .copied()
        .filter(|sample| sample.valid)
        .collect();
    let failures = measured.len().saturating_sub(valid.len());
    let mut ttft: Vec<f64> = valid
        .iter()
        .filter_map(|sample| sample.ttft_ms.map(|value| value as f64))
        .collect();
    let mut decode: Vec<f64> = valid
        .iter()
        .filter_map(|sample| sample.decode_tokens_per_second)
        .collect();
    ttft.sort_by(f64::total_cmp);
    decode.sort_by(f64::total_cmp);
    Ok(AggregateMetrics {
        measured_count: valid.len() as u32,
        failure_rate: if measured.is_empty() {
            0.0
        } else {
            failures as f64 / measured.len() as f64
        },
        median_ttft_ms: percentile(&ttft, 0.5),
        p90_ttft_ms: percentile(&ttft, 0.9),
        median_decode_tokens_per_second: percentile(&decode, 0.5),
        variance_decode_tokens_per_second: variance(&decode),
        metric_availability: if ttft.is_empty() && decode.is_empty() {
            MetricAvailability::Unknown
        } else {
            MetricAvailability::Available
        },
    })
}

fn percentile(values: &[f64], quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let index = ((values.len() - 1) as f64 * quantile).round() as usize;
    values.get(index).copied()
}

fn variance(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    Some(
        values
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / values.len() as f64,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Invalid identity, metric, session state, or exceeded bound.
pub enum CalibrationError {
    /// A required metadata field is empty.
    EmptyField(&'static str),
    /// A metadata field exceeds its character bound.
    FieldTooLong(&'static str),
    /// Model, runtime, hardware, or suite identity is invalid.
    InvalidIdentity,
    /// A sample metric is negative, non-finite, or missing a required count.
    InvalidMetric,
    /// Session or profile state violates its lifecycle invariant.
    InvalidState,
    /// A sample or context-point bound was exceeded.
    LimitExceeded(&'static str),
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong(field) => write!(f, "{field} exceeds bound"),
            Self::InvalidIdentity => write!(f, "invalid calibration identity"),
            Self::InvalidMetric => write!(f, "invalid calibration metric"),
            Self::InvalidState => write!(f, "invalid calibration state"),
            Self::LimitExceeded(field) => write!(f, "{field} exceeds bound"),
        }
    }
}

impl std::error::Error for CalibrationError {}

fn bounded(field: &'static str, value: &str) -> Result<(), CalibrationError> {
    if value.trim().is_empty() {
        Err(CalibrationError::EmptyField(field))
    } else if value.chars().count() > MAX_METADATA_CHARS {
        Err(CalibrationError::FieldTooLong(field))
    } else {
        Ok(())
    }
}

fn hex_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> CalibrationIdentity {
        CalibrationIdentity {
            model_artifact_hash: "a".repeat(64),
            runtime_id: "runtime".into(),
            runtime_version: "1".into(),
            hardware_profile_hash: "b".repeat(64),
            driver_fingerprint: None,
            launch_config_hash: "c".repeat(64),
            context_profile: "short".into(),
            suite_hash: "d".repeat(64),
        }
    }

    #[test]
    fn warmup_is_excluded_and_unknown_telemetry_is_preserved() {
        let sample = CalibrationSample {
            sample_id: "s".into(),
            kind: SampleKind::Measured,
            prompt_profile: PromptProfile::ShortInteractive,
            context_tokens: 32,
            input_tokens: 4,
            output_tokens: 8,
            load_ms: None,
            ttft_ms: None,
            prefill_tokens_per_second: None,
            decode_tokens_per_second: None,
            end_to_end_ms: None,
            peak_ram_bytes: None,
            peak_vram_bytes: None,
            availability: MetricAvailability::Unknown,
            valid: true,
            termination: "completed".into(),
            health: "healthy".into(),
        };
        let warmup = CalibrationSample {
            kind: SampleKind::Warmup,
            ..sample.clone()
        };
        let aggregate = aggregate_samples(&[warmup, sample]).unwrap();
        assert_eq!(aggregate.measured_count, 1);
        assert_eq!(aggregate.metric_availability, MetricAvailability::Unknown);
    }

    #[test]
    fn profile_rejects_measured_without_valid_sample() {
        let profile = LocalModelPerformanceProfile {
            profile_id: "profile".into(),
            revision: 1,
            identity: identity(),
            aggregate: AggregateMetrics {
                measured_count: 0,
                failure_rate: 1.0,
                median_ttft_ms: None,
                p90_ttft_ms: None,
                median_decode_tokens_per_second: None,
                variance_decode_tokens_per_second: None,
                metric_availability: MetricAvailability::Unknown,
            },
            evidence: EvidenceClass::MeasuredLocal,
            context_points: Vec::new(),
            comfortable_context: None,
            interactive_context: None,
            maximum_measured_context: None,
            confidence: "low".into(),
        };
        assert_eq!(profile.validate(), Err(CalibrationError::InvalidState));
    }
}
