//! Core-owned governance gate for the durable memory store.
//!
//! The storage record remains the sole durable representation. This module is
//! deliberately a policy/validation layer: it does not own another record or
//! perform SQL, so every Core write can be checked immediately before effect.

use evohime_local_storage::memory_store::MemoryRecord;
use std::collections::BTreeSet;
use std::fmt;

pub const GOVERNANCE_CONTRACT_VERSION: &str = "memory-governance-v1";
pub const MAX_IDEMPOTENCY_KEY_CHARS: usize = 128;
pub const MIN_CONFIDENCE: f64 = 0.0;
pub const MAX_CONFIDENCE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAuthority {
    UserAsserted,
    SystemDefined,
    ModelProposed,
    Imported,
}

impl MemoryAuthority {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user_asserted" => Some(Self::UserAsserted),
            "system_defined" => Some(Self::SystemDefined),
            "model_proposed" => Some(Self::ModelProposed),
            "imported" => Some(Self::Imported),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryDurability {
    Ephemeral,
    Session,
    Durable,
}

impl MemoryDurability {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ephemeral" => Some(Self::Ephemeral),
            "session" => Some(Self::Session),
            "durable" => Some(Self::Durable),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryGovernanceError {
    UnknownAuthority,
    UnknownDurability,
    InvalidConfidence,
    SecretNotPersistable,
    EphemeralStoreBypass,
    UnverifiedDurableRecord,
    AuthorityRequiresCoreEvidence,
    InvalidIdempotencyKey,
    InsufficientIndependentEvidence,
    DuplicateEvidenceSource,
}

impl fmt::Display for MemoryGovernanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "memory governance: {:?}", self)
    }
}

impl std::error::Error for MemoryGovernanceError {}

pub struct MemoryWriteGate;

impl MemoryWriteGate {
    /// Validates the exact record immediately before the SQL insert/update.
    /// Pending model output may be durable as a review candidate, but it can
    /// never become retrievable without a Core validation/approval transition.
    pub fn validate(record: &MemoryRecord) -> Result<(), MemoryGovernanceError> {
        let authority = MemoryAuthority::parse(&record.extraction.authority)
            .ok_or(MemoryGovernanceError::UnknownAuthority)?;
        let durability = MemoryDurability::parse(&record.extraction.durability)
            .ok_or(MemoryGovernanceError::UnknownDurability)?;
        let confidence = record.extraction.confidence;
        if !confidence.is_finite() || !(MIN_CONFIDENCE..=MAX_CONFIDENCE).contains(&confidence) {
            return Err(MemoryGovernanceError::InvalidConfidence);
        }
        if record.extraction.privacy_class == "secret" {
            return Err(MemoryGovernanceError::SecretNotPersistable);
        }
        if matches!(
            durability,
            MemoryDurability::Ephemeral | MemoryDurability::Session
        ) {
            return Err(MemoryGovernanceError::EphemeralStoreBypass);
        }
        if matches!(
            authority,
            MemoryAuthority::SystemDefined | MemoryAuthority::Imported
        ) && record.extraction.source_trust != "user"
        {
            return Err(MemoryGovernanceError::AuthorityRequiresCoreEvidence);
        }
        if record.extraction.confirmation_state == "confirmed"
            && matches!(
                authority,
                MemoryAuthority::ModelProposed | MemoryAuthority::Imported
            )
            && record.extraction.validation_status != "valid"
        {
            return Err(MemoryGovernanceError::UnverifiedDurableRecord);
        }
        Ok(())
    }

    pub fn validate_idempotency_key(value: &str) -> Result<(), MemoryGovernanceError> {
        if value.trim().is_empty() || value.chars().count() > MAX_IDEMPOTENCY_KEY_CHARS {
            return Err(MemoryGovernanceError::InvalidIdempotencyKey);
        }
        Ok(())
    }

    /// Reinforcement is accepted only when evidence comes from at least two
    /// distinct provenance sources. Replaying one source is not reinforcement.
    pub fn validate_independent_evidence(
        evidence_refs: &[String],
    ) -> Result<(), MemoryGovernanceError> {
        if evidence_refs.len() < 2 {
            return Err(MemoryGovernanceError::InsufficientIndependentEvidence);
        }
        let unique = evidence_refs
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if unique.len() != evidence_refs.len() {
            return Err(MemoryGovernanceError::DuplicateEvidenceSource);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_local_storage::memory_store::{MemoryPrivacy, MemoryScope};

    fn record() -> MemoryRecord {
        MemoryRecord::new(
            "m",
            MemoryScope::Project,
            "p",
            "title",
            "body",
            "source",
            MemoryPrivacy::Private,
            "1",
            None,
        )
        .unwrap()
    }

    #[test]
    fn gate_accepts_legacy_user_record_and_rejects_unknown_authority() {
        let mut value = record();
        MemoryWriteGate::validate(&value).unwrap();
        value.extraction.authority = "admin_magic".into();
        assert_eq!(
            MemoryWriteGate::validate(&value),
            Err(MemoryGovernanceError::UnknownAuthority)
        );
    }

    #[test]
    fn model_confirmed_record_requires_validation() {
        let mut value = record();
        value.extraction.authority = "model_proposed".into();
        value.extraction.confirmation_state = "confirmed".into();
        assert_eq!(
            MemoryWriteGate::validate(&value),
            Err(MemoryGovernanceError::UnverifiedDurableRecord)
        );
        value.extraction.validation_status = "valid".into();
        MemoryWriteGate::validate(&value).unwrap();
    }

    #[test]
    fn reinforcement_requires_distinct_sources() {
        assert_eq!(
            MemoryWriteGate::validate_independent_evidence(&["a".into()]),
            Err(MemoryGovernanceError::InsufficientIndependentEvidence)
        );
        assert_eq!(
            MemoryWriteGate::validate_independent_evidence(&["a".into(), "a".into()]),
            Err(MemoryGovernanceError::DuplicateEvidenceSource)
        );
        MemoryWriteGate::validate_independent_evidence(&["a".into(), "b".into()]).unwrap();
    }
}
