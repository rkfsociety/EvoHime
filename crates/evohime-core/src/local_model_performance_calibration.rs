//! Core-owned, metadata-only local inference calibration contract.
//!
//! This module deliberately does not launch a process or invent telemetry.
//! Runtime execution is an adapter boundary; absent verified adapter data is
//! represented as `Unavailable` rather than as an estimate.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const CONTRACT_ID: &str = "local-model-performance-calibration-v1";
pub const MAX_SAMPLES: usize = 256;
pub const MAX_CONTEXT_POINTS: usize = 32;
pub const MAX_METADATA_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationAdmission {
    Approved,
    UnavailableAdapter,
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
pub struct CalibrationIdentity {
    pub model_artifact_hash: String,
    pub runtime_id: String,
    pub runtime_version: String,
    pub hardware_profile_hash: String,
    pub driver_fingerprint: Option<String>,
    pub launch_config_hash: String,
    pub context_profile: String,
    pub suite_hash: String,
}

impl CalibrationIdentity {
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
pub enum PromptProfile {
    ShortInteractive,
    MediumGeneration,
    LongContextPrefill,
    StructuredSmallOutput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SampleKind {
    Warmup,
    Measured,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricAvailability {
    Available,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationSample {
    pub sample_id: String,
    pub kind: SampleKind,
    pub prompt_profile: PromptProfile,
    pub context_tokens: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub load_ms: Option<u64>,
    pub ttft_ms: Option<u64>,
    pub prefill_tokens_per_second: Option<f64>,
    pub decode_tokens_per_second: Option<f64>,
    pub end_to_end_ms: Option<u64>,
    pub peak_ram_bytes: Option<u64>,
    pub peak_vram_bytes: Option<u64>,
    pub availability: MetricAvailability,
    pub valid: bool,
    pub termination: String,
    pub health: String,
}

impl CalibrationSample {
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
pub struct AggregateMetrics {
    pub measured_count: u32,
    pub failure_rate: f64,
    pub median_ttft_ms: Option<f64>,
    pub p90_ttft_ms: Option<f64>,
    pub median_decode_tokens_per_second: Option<f64>,
    pub variance_decode_tokens_per_second: Option<f64>,
    pub metric_availability: MetricAvailability,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    MeasuredLocal,
    MeasuredLocalStale,
    EstimatedFromHardware,
    CatalogEstimate,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationState {
    Queued,
    WarmingUp,
    Running,
    Aggregating,
    Completed,
    Cancelled,
    Failed,
    Unavailable,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalModelCalibrationSession {
    pub session_id: String,
    pub identity: CalibrationIdentity,
    pub state: CalibrationState,
    pub samples: Vec<CalibrationSample>,
    pub cancellation_requested: bool,
}

impl LocalModelCalibrationSession {
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
pub struct LocalModelPerformanceProfile {
    pub profile_id: String,
    pub revision: u64,
    pub identity: CalibrationIdentity,
    pub aggregate: AggregateMetrics,
    pub evidence: EvidenceClass,
    pub context_points: Vec<(u32, Option<f64>)>,
    pub comfortable_context: Option<u32>,
    pub interactive_context: Option<u32>,
    pub maximum_measured_context: Option<u32>,
    pub confidence: String,
}

impl LocalModelPerformanceProfile {
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
pub enum CalibrationError {
    EmptyField(&'static str),
    FieldTooLong(&'static str),
    InvalidIdentity,
    InvalidMetric,
    InvalidState,
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
