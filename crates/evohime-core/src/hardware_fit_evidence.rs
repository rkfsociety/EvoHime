//! Core-owned, privacy-safe hardware fit evidence contract.
//!
//! This module stores portable priors only. Hardware discovery, exact model
//! identity and local measurements remain owned by the local model runtime and
//! calibration contracts.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_CHARS: usize = 256;
pub const MAX_ASSUMPTIONS: usize = 32;
pub const MAX_ASSUMPTION_CHARS: usize = 256;
pub const MAX_OBSERVATIONS: u32 = 2048;
pub const MAX_CATALOG_ENTRIES: u32 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    MeasuredExactLocal,
    MeasuredPortableExactClass,
    MeasuredPortableNearClass,
    DerivedFromMeasuredClass,
    DeterministicEstimate,
    CatalogEstimate,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchClass {
    ExactClass,
    NearClass,
    ArchitectureCompatible,
    WeakComparable,
    NoComparableEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitAdmission {
    Recommended,
    EstimateOnly,
    BlockedByMemory,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Draft,
    Active,
    Superseded,
    Invalid,
}

/// Portable evidence can inform a recommendation but cannot override the
/// current conservative memory admission owned by the runtime manager.
pub fn admit_fit(
    estimated_memory_bytes: u64,
    available_memory_bytes: Option<u64>,
    evidence: EvidenceClass,
) -> FitAdmission {
    if estimated_memory_bytes == 0 {
        return FitAdmission::Unknown;
    }
    if let Some(available) = available_memory_bytes {
        if estimated_memory_bytes > available {
            return FitAdmission::BlockedByMemory;
        }
    }
    if matches!(
        evidence,
        EvidenceClass::Unknown
            | EvidenceClass::CatalogEstimate
            | EvidenceClass::DeterministicEstimate
    ) {
        FitAdmission::EstimateOnly
    } else {
        FitAdmission::Recommended
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareFitIdentity {
    pub hardware_class: String,
    pub memory_class: String,
    pub accelerator_class: Option<String>,
    pub cpu_class: Option<String>,
    pub runtime_family: String,
    pub runtime_version_range: Option<String>,
    pub model_descriptor_ref: String,
    pub model_family: Option<String>,
    pub parameter_class: String,
    pub quantization: String,
    pub format: String,
    pub context_profile: String,
    pub launch_profile_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareFitObservation {
    pub schema_version: u32,
    pub lifecycle: Lifecycle,
    pub identity: HardwareFitIdentity,
    pub ttft_ms: Option<f64>,
    pub prefill_tps: Option<f64>,
    pub decode_tps: Option<f64>,
    pub peak_vram_bytes: Option<u64>,
    pub peak_ram_bytes: Option<u64>,
    pub load_time_ms: Option<u64>,
    pub success: bool,
    pub sample_count: u32,
    pub source_class: String,
    pub confidence: String,
    pub observed_at: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareFitEstimate {
    pub schema_version: u32,
    pub identity: HardwareFitIdentity,
    pub estimated_memory_bytes: u64,
    pub estimated_headroom_bytes: Option<u64>,
    pub estimated_ttft_range_ms: Option<(f64, f64)>,
    pub estimated_decode_tps_range: Option<(f64, f64)>,
    pub evidence_class: EvidenceClass,
    pub match_class: MatchClass,
    pub matched_observation_count: u32,
    pub assumptions: Vec<String>,
    pub confidence: String,
    pub generated_at: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub schema_version: u32,
    pub catalog_revision: u64,
    pub created_at: String,
    pub source_manifest_hash: String,
    pub entry_count: u32,
    pub trust_class: String,
    pub signature_present: bool,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkContributionBundle {
    pub schema_version: u32,
    pub normalized_hardware_class: String,
    pub model_runtime_identity: String,
    pub validated_aggregate_metrics: Vec<(String, f64)>,
    pub sample_count: u32,
    pub confidence: String,
    pub benchmark_suite_version: String,
    pub generated_at: String,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum HardwareFitError {
    #[error("invalid hardware fit field: {0}")]
    InvalidField(&'static str),
    #[error("invalid numeric metric: {0}")]
    InvalidMetric(&'static str),
    #[error("content hash mismatch")]
    HashMismatch,
}

fn bounded(name: &'static str, value: &str) -> Result<(), HardwareFitError> {
    if value.trim().is_empty() || value.chars().count() > MAX_ID_CHARS {
        return Err(HardwareFitError::InvalidField(name));
    }
    Ok(())
}

fn metric(name: &'static str, value: Option<f64>) -> Result<(), HardwareFitError> {
    if let Some(value) = value {
        if !value.is_finite() || value < 0.0 {
            return Err(HardwareFitError::InvalidMetric(name));
        }
    }
    Ok(())
}

pub fn observation_hash(value: &HardwareFitObservation) -> Result<String, HardwareFitError> {
    let mut copy = value.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| HardwareFitError::InvalidField("json"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn validate_observation(value: &HardwareFitObservation) -> Result<(), HardwareFitError> {
    if value.schema_version != SCHEMA_VERSION
        || value.sample_count == 0
        || matches!(value.lifecycle, Lifecycle::Invalid)
    {
        return Err(HardwareFitError::InvalidField("schema_or_samples"));
    }
    for (name, item) in [
        ("hardware_class", &value.identity.hardware_class),
        ("memory_class", &value.identity.memory_class),
        ("runtime_family", &value.identity.runtime_family),
        ("model_descriptor_ref", &value.identity.model_descriptor_ref),
        ("parameter_class", &value.identity.parameter_class),
        ("quantization", &value.identity.quantization),
        ("format", &value.identity.format),
        ("context_profile", &value.identity.context_profile),
        ("launch_profile_class", &value.identity.launch_profile_class),
    ] {
        bounded(name, item)?;
    }
    metric("ttft_ms", value.ttft_ms)?;
    metric("prefill_tps", value.prefill_tps)?;
    metric("decode_tps", value.decode_tps)?;
    if observation_hash(value)? != value.content_hash {
        return Err(HardwareFitError::HashMismatch);
    }
    Ok(())
}

pub fn evidence_priority(class: EvidenceClass) -> u8 {
    match class {
        EvidenceClass::MeasuredExactLocal => 7,
        EvidenceClass::MeasuredPortableExactClass => 6,
        EvidenceClass::MeasuredPortableNearClass => 5,
        EvidenceClass::DerivedFromMeasuredClass => 4,
        EvidenceClass::DeterministicEstimate => 3,
        EvidenceClass::CatalogEstimate => 2,
        EvidenceClass::Unknown => 0,
    }
}

/// Compares normalized identities without treating a display name as a
/// sufficient hardware or model identity.
pub fn match_class(left: &HardwareFitIdentity, right: &HardwareFitIdentity) -> MatchClass {
    if left.model_descriptor_ref != right.model_descriptor_ref
        || left.quantization != right.quantization
        || left.format != right.format
        || left.context_profile != right.context_profile
        || left.runtime_family != right.runtime_family
    {
        return MatchClass::NoComparableEvidence;
    }
    if left == right {
        return MatchClass::ExactClass;
    }
    if left.accelerator_class == right.accelerator_class
        && left.memory_class == right.memory_class
        && left.cpu_class == right.cpu_class
    {
        MatchClass::NearClass
    } else if left.runtime_family == right.runtime_family {
        MatchClass::ArchitectureCompatible
    } else {
        MatchClass::WeakComparable
    }
}

pub fn projection(
    observation: &HardwareFitObservation,
) -> Result<serde_json::Value, HardwareFitError> {
    validate_observation(observation)?;
    Ok(serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "evidence_class": "measured_portable",
        "hardware_class": observation.identity.hardware_class,
        "runtime_family": observation.identity.runtime_family,
        "model_descriptor_ref": observation.identity.model_descriptor_ref,
        "context_profile": observation.identity.context_profile,
        "sample_count": observation.sample_count,
        "confidence": observation.confidence,
        "content_hash": observation.content_hash,
        "raw_machine_identifiers": false,
        "raw_payload": false,
    }))
}

/// Applies the canonical precedence rule without mutating portable evidence.
pub fn effective_evidence(portable: EvidenceClass, exact_local_available: bool) -> EvidenceClass {
    if exact_local_available {
        EvidenceClass::MeasuredExactLocal
    } else {
        portable
    }
}

/// Produces a conservative range from validated observations. It deliberately
/// returns no estimate when identities are not comparable.
pub fn bounded_decode_range(
    observations: &[HardwareFitObservation],
    requested: &HardwareFitIdentity,
) -> Option<(f64, f64, u32)> {
    let mut values: Vec<f64> = observations
        .iter()
        .filter_map(|observation| {
            if validate_observation(observation).is_ok()
                && match_class(&observation.identity, requested) != MatchClass::NoComparableEvidence
            {
                observation.decode_tps
            } else {
                None
            }
        })
        .collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let low = values[values.len() / 10].max(0.0);
    let high = values[(values.len() * 9 / 10).min(values.len() - 1)];
    Some((low, high.max(low), values.len() as u32))
}

pub fn observation_is_stale(
    observation: &HardwareFitObservation,
    current_runtime_major: Option<&str>,
    current_model_ref: Option<&str>,
) -> bool {
    current_runtime_major.is_some_and(|runtime| {
        observation.identity.runtime_version_range.as_deref() != Some(runtime)
    }) || current_model_ref.is_some_and(|model| observation.identity.model_descriptor_ref != model)
}

pub fn robust_median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty()
        || values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(if values.len().is_multiple_of(2) {
        (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
    } else {
        values[values.len() / 2]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> HardwareFitObservation {
        let mut value = HardwareFitObservation {
            schema_version: SCHEMA_VERSION,
            lifecycle: Lifecycle::Active,
            identity: HardwareFitIdentity {
                hardware_class: "gpu:nvidia:midrange|vram:8gb".into(),
                memory_class: "ram:16gb".into(),
                accelerator_class: Some("cuda".into()),
                cpu_class: Some("x64".into()),
                runtime_family: "llama.cpp".into(),
                runtime_version_range: Some("0.3".into()),
                model_descriptor_ref: "model:example".into(),
                model_family: Some("example".into()),
                parameter_class: "7b".into(),
                quantization: "Q4_K_M".into(),
                format: "GGUF".into(),
                context_profile: "8K".into(),
                launch_profile_class: "interactive".into(),
            },
            ttft_ms: Some(120.0),
            prefill_tps: Some(80.0),
            decode_tps: Some(24.0),
            peak_vram_bytes: Some(8_000_000_000),
            peak_ram_bytes: None,
            load_time_ms: Some(500),
            success: true,
            sample_count: 4,
            source_class: "portable_import".into(),
            confidence: "medium".into(),
            observed_at: "2026-09-14T00:00:00Z".into(),
            content_hash: String::new(),
        };
        value.content_hash = observation_hash(&value).expect("hash");
        value
    }

    #[test]
    fn validates_redacted_observation_and_hash() {
        assert!(validate_observation(&observation()).is_ok());
    }

    #[test]
    fn rejects_tampered_observation() {
        let mut value = observation();
        value.decode_tps = Some(999.0);
        assert_eq!(
            validate_observation(&value),
            Err(HardwareFitError::HashMismatch)
        );
    }

    #[test]
    fn quantization_or_context_mismatch_is_not_comparable() {
        let a = observation();
        let mut b = a.clone();
        b.identity.quantization = "Q8_0".into();
        assert_eq!(
            match_class(&a.identity, &b.identity),
            MatchClass::NoComparableEvidence
        );
    }

    #[test]
    fn exact_local_measurement_has_priority_without_mutating_prior() {
        assert_eq!(
            effective_evidence(EvidenceClass::CatalogEstimate, true),
            EvidenceClass::MeasuredExactLocal
        );
        assert_eq!(
            effective_evidence(EvidenceClass::CatalogEstimate, false),
            EvidenceClass::CatalogEstimate
        );
    }

    #[test]
    fn robust_aggregate_ignores_no_values_and_stale_is_degraded() {
        let mut values = vec![24.0, 25.0, 9999.0];
        assert_eq!(robust_median(&mut values), Some(25.0));
        let value = observation();
        assert!(observation_is_stale(&value, Some("0.4"), None));
    }
}
