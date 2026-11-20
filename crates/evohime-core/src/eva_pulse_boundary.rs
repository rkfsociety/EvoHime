//! Typed, bounded boundary between EvaPulse and the Hive.
//!
//! EvaPulse may publish an external signal, but it does not receive Hive
//! capabilities or create a Hive task directly. The envelope deliberately
//! contains metadata only; the Hive decides whether a signal becomes work.

use serde::{Deserialize, Serialize};

/// Version of the EvaPulse-to-Hive envelope contract.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum UTF-8 payload carried by one signal.
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024;
/// Maximum accepted lifetime of a signal.
pub const MAX_TTL_MS: i64 = 24 * 60 * 60 * 1_000;
/// Maximum length of an envelope identity field.
pub const MAX_ID_BYTES: usize = 128;

/// Provenance of a Hive task. This is metadata, not an authorization grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiveTaskOrigin {
    /// A task created by the normal Hive UI or internal workflow.
    HiveInternal,
    /// A task accepted by the Hive after an EvaPulse signal.
    EvaPulse,
}

/// Supported external signal kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaPulseSignalKind {
    /// A bounded observation that may deserve Queen review.
    Observation,
    /// A request to review a bounded recovery condition.
    RecoveryReview,
    /// A request to review a bounded evolution candidate.
    EvolutionReview,
}

/// Result of the Hive boundary's decision about a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiveSignalOutcome {
    /// The Hive accepted the signal and owns any resulting task.
    Accepted,
    /// The Hive declined the signal without creating work.
    Rejected,
    /// The Hive retained the signal for a later decision.
    Deferred,
    /// The boundary could not process the signal.
    Failed,
}

/// A bounded message sent from EvaPulse to the Hive boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaPulseSignal {
    /// Contract version.
    pub schema_version: u32,
    /// Globally unique signal identifier used for deduplication.
    pub message_id: String,
    /// Correlates retries and the eventual Hive decision.
    pub correlation_id: String,
    /// Stable source identity; never a capability token.
    pub source: String,
    /// Stable idempotency key for delivery fencing.
    pub idempotency_key: String,
    /// Signal classification.
    pub kind: EvaPulseSignalKind,
    /// Creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Expiration time in Unix milliseconds.
    pub expires_at_ms: i64,
    /// Redacted, bounded metadata payload.
    pub payload: Vec<u8>,
}

/// A typed response from the Hive boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiveSignalResult {
    /// Contract version.
    pub schema_version: u32,
    /// Signal identifier being answered.
    pub message_id: String,
    /// Correlation copied from the signal.
    pub correlation_id: String,
    /// Hive's decision.
    pub outcome: HiveSignalOutcome,
    /// Provenance of a created Hive task, when one exists.
    pub task_origin: Option<HiveTaskOrigin>,
    /// Redacted bounded reason code.
    pub reason: String,
}

/// Validation failures at the EvaPulse/Hive boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryError {
    /// A required identity is missing or too long.
    InvalidIdentity,
    /// The contract version is not supported.
    UnsupportedVersion,
    /// The payload or serialized envelope exceeds its bound.
    PayloadTooLarge,
    /// The expiry is invalid or exceeds the maximum TTL.
    InvalidTtl,
    /// The signal is already expired at the supplied clock time.
    Expired,
}

impl std::fmt::Display for BoundaryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidIdentity => "invalid_identity",
            Self::UnsupportedVersion => "unsupported_version",
            Self::PayloadTooLarge => "payload_too_large",
            Self::InvalidTtl => "invalid_ttl",
            Self::Expired => "expired",
        })
    }
}

impl std::error::Error for BoundaryError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && value.is_ascii()
}

/// Validates an incoming signal against contract, size, and TTL limits.
pub fn validate_signal(signal: &EvaPulseSignal, now_ms: i64) -> Result<(), BoundaryError> {
    if signal.schema_version != CONTRACT_VERSION {
        return Err(BoundaryError::UnsupportedVersion);
    }
    if !valid_id(&signal.message_id)
        || !valid_id(&signal.correlation_id)
        || !valid_id(&signal.source)
        || !valid_id(&signal.idempotency_key)
    {
        return Err(BoundaryError::InvalidIdentity);
    }
    if signal.payload.len() > MAX_PAYLOAD_BYTES {
        return Err(BoundaryError::PayloadTooLarge);
    }
    let ttl = signal.expires_at_ms.saturating_sub(signal.created_at_ms);
    if ttl <= 0 || ttl > MAX_TTL_MS || signal.created_at_ms > now_ms {
        return Err(BoundaryError::InvalidTtl);
    }
    if signal.expires_at_ms <= now_ms {
        return Err(BoundaryError::Expired);
    }
    Ok(())
}

/// Validates that a result cannot grant capabilities through its provenance.
pub fn validate_result(result: &HiveSignalResult) -> Result<(), BoundaryError> {
    if result.schema_version != CONTRACT_VERSION
        || !valid_id(&result.message_id)
        || !valid_id(&result.correlation_id)
        || !valid_id(&result.reason)
    {
        return Err(BoundaryError::InvalidIdentity);
    }
    if result.outcome != HiveSignalOutcome::Accepted && result.task_origin.is_some() {
        return Err(BoundaryError::InvalidIdentity);
    }
    if result.outcome == HiveSignalOutcome::Accepted
        && result.task_origin != Some(HiveTaskOrigin::EvaPulse)
    {
        return Err(BoundaryError::InvalidIdentity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal() -> EvaPulseSignal {
        EvaPulseSignal {
            schema_version: CONTRACT_VERSION,
            message_id: "pulse-1".into(),
            correlation_id: "corr-1".into(),
            source: "eva-pulse".into(),
            idempotency_key: "pulse-1".into(),
            kind: EvaPulseSignalKind::Observation,
            created_at_ms: 1_000,
            expires_at_ms: 10_000,
            payload: b"bounded observation".to_vec(),
        }
    }

    #[test]
    fn validates_round_trip_and_rejects_expiry() {
        let original = signal();
        validate_signal(&original, 2_000).unwrap();
        let encoded = serde_json::to_vec(&original).unwrap();
        let decoded: EvaPulseSignal = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(
            validate_signal(&decoded, 10_000),
            Err(BoundaryError::Expired)
        );
    }

    #[test]
    fn rejects_unknown_version_and_oversized_payload() {
        let mut unknown = signal();
        unknown.schema_version += 1;
        assert_eq!(
            validate_signal(&unknown, 2_000),
            Err(BoundaryError::UnsupportedVersion)
        );
        let mut oversized = signal();
        oversized.payload = vec![0; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(
            validate_signal(&oversized, 2_000),
            Err(BoundaryError::PayloadTooLarge)
        );
    }

    #[test]
    fn accepted_result_requires_eva_pulse_provenance() {
        let result = HiveSignalResult {
            schema_version: CONTRACT_VERSION,
            message_id: "pulse-1".into(),
            correlation_id: "corr-1".into(),
            outcome: HiveSignalOutcome::Accepted,
            task_origin: Some(HiveTaskOrigin::EvaPulse),
            reason: "accepted".into(),
        };
        validate_result(&result).unwrap();
        let mut forged = result;
        forged.task_origin = Some(HiveTaskOrigin::HiveInternal);
        assert_eq!(
            validate_result(&forged),
            Err(BoundaryError::InvalidIdentity)
        );
    }
}
