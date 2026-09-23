//! Core-owned, privacy-safe hardware fit evidence contract.
//!
//! This module stores portable priors only. Hardware discovery, exact model
//! identity and local measurements remain owned by the local model runtime and
//! calibration contracts.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Schema version accepted by hardware fit evidence records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum character count for portable identity values.
pub const MAX_ID_CHARS: usize = 256;
/// Maximum assumptions retained in one estimate.
pub const MAX_ASSUMPTIONS: usize = 32;
/// Maximum character count for one estimate assumption.
pub const MAX_ASSUMPTION_CHARS: usize = 256;
/// Maximum validated observations considered in aggregation.
pub const MAX_OBSERVATIONS: u32 = 2048;
/// Maximum entries represented by one catalog import.
pub const MAX_CATALOG_ENTRIES: u32 = 4096;

/// Provenance and locality of the evidence supporting a fit recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    /// Measured on the exact local runtime and hardware class.
    MeasuredExactLocal,
    /// Measured on the exact portable hardware class.
    MeasuredPortableExactClass,
    /// Measured on a nearby portable hardware class.
    MeasuredPortableNearClass,
    /// Derived from measurements in a related hardware class.
    DerivedFromMeasuredClass,
    /// Produced by a deterministic local estimator.
    DeterministicEstimate,
    /// Provided by a hardware or model catalog.
    CatalogEstimate,
    /// Fit cannot be determined.
    Unknown,
}

/// Similarity between normalized hardware and model identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchClass {
    /// Normalized hardware and model identities are equal.
    ExactClass,
    /// Hardware classes are close while runtime and model settings match.
    NearClass,
    /// Architecture is comparable but the hardware class differs.
    ArchitectureCompatible,
    /// Only a weak identity comparison is possible.
    WeakComparable,
    /// Identity mismatch prevents a meaningful comparison.
    NoComparableEvidence,
}

/// Conservative admission result for a model fit estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitAdmission {
    /// Evidence supports recommending this fit.
    Recommended,
    /// Fit is based on an estimate and needs cautious treatment.
    EstimateOnly,
    /// Estimated memory exceeds currently available memory.
    BlockedByMemory,
    /// Fit cannot be determined.
    Unknown,
}

/// Lifecycle state of a portable observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// Observation is not available for recommendations.
    Draft,
    /// Observation is eligible for validation and matching.
    Active,
    /// A newer observation replaced this one.
    Superseded,
    /// Observation failed validation.
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

/// Privacy-safe normalized hardware, runtime, and model identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareFitIdentity {
    /// Normalized hardware category; must not contain a raw machine identifier.
    pub hardware_class: String,
    /// Portable memory-capacity category.
    pub memory_class: String,
    /// Optional accelerator architecture category.
    pub accelerator_class: Option<String>,
    /// Optional CPU architecture category.
    pub cpu_class: Option<String>,
    /// Runtime family used to interpret the measurement.
    pub runtime_family: String,
    /// Optional runtime major or compatible version range.
    pub runtime_version_range: Option<String>,
    /// Portable model descriptor reference, not a local path.
    pub model_descriptor_ref: String,
    /// Optional model family category.
    pub model_family: Option<String>,
    /// Model parameter-size category.
    pub parameter_class: String,
    /// Model quantization format.
    pub quantization: String,
    /// Model file or execution format.
    pub format: String,
    /// Context-size category used for the measurement.
    pub context_profile: String,
    /// Normalized launch configuration category.
    pub launch_profile_class: String,
}

/// Validated benchmark observation with normalized identity and integrity hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareFitObservation {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Publication state of this observation or estimate.
    pub lifecycle: Lifecycle,
    /// Normalized hardware and model identity used for matching.
    pub identity: HardwareFitIdentity,
    /// Optional measured time to first token in milliseconds.
    pub ttft_ms: Option<f64>,
    /// Optional measured prompt-processing throughput in tokens per second.
    pub prefill_tps: Option<f64>,
    /// Optional measured generation throughput in tokens per second.
    pub decode_tps: Option<f64>,
    /// Optional peak accelerator memory observed in bytes.
    pub peak_vram_bytes: Option<u64>,
    /// Optional peak system memory observed in bytes.
    pub peak_ram_bytes: Option<u64>,
    /// Optional model load duration in milliseconds.
    pub load_time_ms: Option<u64>,
    /// Whether the benchmark run completed successfully.
    pub success: bool,
    /// Number of observations summarized by this record.
    pub sample_count: u32,
    /// Provenance category of the underlying measurements.
    pub source_class: String,
    /// Bounded confidence label assigned to the evidence.
    pub confidence: String,
    /// Observation timestamp in the contract time format.
    pub observed_at: String,
    /// Integrity hash of the canonical record content.
    pub content_hash: String,
}

/// Conservative resource and performance estimate derived from evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HardwareFitEstimate {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Normalized hardware and model identity used for matching.
    pub identity: HardwareFitIdentity,
    /// Estimated memory needed to run the model in bytes.
    pub estimated_memory_bytes: u64,
    /// Optional memory remaining after the estimate.
    pub estimated_headroom_bytes: Option<u64>,
    /// Optional conservative time-to-first-token range.
    pub estimated_ttft_range_ms: Option<(f64, f64)>,
    /// Optional conservative decode-throughput range.
    pub estimated_decode_tps_range: Option<(f64, f64)>,
    /// Provenance and trust class of the supporting evidence.
    pub evidence_class: EvidenceClass,
    /// Identity similarity class used to select supporting observations.
    pub match_class: MatchClass,
    /// Number of validated observations used in the estimate.
    pub matched_observation_count: u32,
    /// Explicit assumptions used to derive the estimate.
    pub assumptions: Vec<String>,
    /// Bounded confidence label assigned to the evidence.
    pub confidence: String,
    /// Estimate generation timestamp in the contract time format.
    pub generated_at: String,
    /// Integrity hash of the canonical record content.
    pub content_hash: String,
}

/// Integrity metadata for an imported hardware/model catalog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogSnapshot {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Revision of the source catalog represented by the snapshot.
    pub catalog_revision: u64,
    /// Snapshot creation timestamp in the contract time format.
    pub created_at: String,
    /// Integrity hash of the catalog source manifest.
    pub source_manifest_hash: String,
    /// Number of entries represented by the catalog snapshot.
    pub entry_count: u32,
    /// Trust category assigned to the catalog source.
    pub trust_class: String,
    /// Whether the catalog source carried a signature.
    pub signature_present: bool,
    /// Integrity hash of the canonical record content.
    pub content_hash: String,
}

/// Validated, privacy-safe aggregate data prepared for contribution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BenchmarkContributionBundle {
    /// Serialized schema version supported by this contract.
    pub schema_version: u32,
    /// Portable normalized hardware category for the bundle.
    pub normalized_hardware_class: String,
    /// Portable model and runtime identity used by the contribution.
    pub model_runtime_identity: String,
    /// Validated aggregate metric names and values.
    pub validated_aggregate_metrics: Vec<(String, f64)>,
    /// Number of observations summarized by this record.
    pub sample_count: u32,
    /// Bounded confidence label assigned to the evidence.
    pub confidence: String,
    /// Version of the benchmark suite that produced the bundle.
    pub benchmark_suite_version: String,
    /// Estimate generation timestamp in the contract time format.
    pub generated_at: String,
    /// Integrity hash of the canonical record content.
    pub content_hash: String,
}

/// Validation, metric, or integrity failure in hardware fit evidence.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum HardwareFitError {
    /// A named identity or evidence field failed validation.
    #[error("invalid hardware fit field: {0}")]
    InvalidField(&'static str),
    /// A named measurement is non-finite or negative.
    #[error("invalid numeric metric: {0}")]
    InvalidMetric(&'static str),
    /// The stored content hash does not match the record.
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

/// Returns the SHA-256 hash after clearing the observation hash field.
pub fn observation_hash(value: &HardwareFitObservation) -> Result<String, HardwareFitError> {
    let mut copy = value.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| HardwareFitError::InvalidField("json"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

/// Validates identity bounds, metrics, sample count, and content hash.
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

/// Returns the stable precedence rank for an evidence class.
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

/// Produces a privacy-safe portable projection of a validated observation.
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

/// Checks whether runtime or model identity has drifted from the observation.
pub fn observation_is_stale(
    observation: &HardwareFitObservation,
    current_runtime_major: Option<&str>,
    current_model_ref: Option<&str>,
) -> bool {
    current_runtime_major.is_some_and(|runtime| {
        observation.identity.runtime_version_range.as_deref() != Some(runtime)
    }) || current_model_ref.is_some_and(|model| observation.identity.model_descriptor_ref != model)
}

/// Returns the median of finite non-negative values.
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
