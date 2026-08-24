//! Bounded automation snapshots and side-effect-free simulation primitives.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationSnapshotV1 {
    pub schema_version: u32,
    pub run_id: String,
    pub definition_id: String,
    pub definition_revision: u64,
    pub fencing_generation: u64,
    pub last_event_sequence: u64,
    pub state_json: String,
    pub policy_snapshot: String,
    pub approval_snapshot: String,
    pub provenance: String,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    Oversized,
    InvalidChecksum,
    IncompatibleSchema,
    StaleDefinition,
    InvalidGeneration,
    InvalidProvenance,
}
impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SnapshotError {}

impl AutomationSnapshotV1 {
    pub fn new(
        run_id: &str,
        definition_id: &str,
        definition_revision: u64,
        generation: u64,
        sequence: u64,
        state_json: &str,
        policy_snapshot: &str,
        approval_snapshot: &str,
        provenance: &str,
    ) -> Self {
        let mut snapshot = Self {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            run_id: run_id.into(),
            definition_id: definition_id.into(),
            definition_revision,
            fencing_generation: generation,
            last_event_sequence: sequence,
            state_json: state_json.into(),
            policy_snapshot: policy_snapshot.into(),
            approval_snapshot: approval_snapshot.into(),
            provenance: provenance.into(),
            checksum_sha256: String::new(),
        };
        snapshot.checksum_sha256 = snapshot.checksum();
        snapshot
    }
    fn checksum(&self) -> String {
        let mut unsigned = self.clone();
        unsigned.checksum_sha256.clear();
        hex::encode(Sha256::digest(
            serde_json::to_vec(&unsigned).expect("snapshot is serializable"),
        ))
    }
    pub fn validate(
        &self,
        expected_definition_revision: u64,
        previous_sequence: Option<u64>,
    ) -> Result<(), SnapshotError> {
        if serde_json::to_vec(self)
            .map_err(|_| SnapshotError::Oversized)?
            .len()
            > MAX_SNAPSHOT_BYTES
        {
            return Err(SnapshotError::Oversized);
        }
        if self.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(SnapshotError::IncompatibleSchema);
        }
        if self.checksum_sha256 != self.checksum() {
            return Err(SnapshotError::InvalidChecksum);
        }
        if self.definition_revision != expected_definition_revision {
            return Err(SnapshotError::StaleDefinition);
        }
        if self.fencing_generation == 0
            || previous_sequence.is_some_and(|previous| self.last_event_sequence < previous)
        {
            return Err(SnapshotError::InvalidGeneration);
        }
        if self.provenance.is_empty() {
            return Err(SnapshotError::InvalidProvenance);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayInputV1 {
    pub schema_version: u32,
    pub definition_revision: u64,
    pub ordered_events: Vec<String>,
    pub normalized_inputs: String,
    pub frozen_clock_ms: i64,
    pub rng_seed: u64,
    pub provider_fixture_ids: Vec<String>,
    pub capability_snapshot: String,
    pub policy_snapshot: String,
}
pub fn replay_hash(input: &ReplayInputV1) -> Result<String, SnapshotError> {
    if input.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(SnapshotError::IncompatibleSchema);
    }
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(input).map_err(|_| SnapshotError::InvalidProvenance)?,
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationEffect {
    Filesystem,
    Network,
    Process,
    Shell,
    Registry,
    Clipboard,
    ProductionIpc,
    FakeProvider,
}
pub fn allow_simulation_effect(effect: SimulationEffect) -> bool {
    matches!(effect, SimulationEffect::FakeProvider)
}
pub fn redact_export(value: &str) -> String {
    value
        .replace("Bearer ", "Bearer [REDACTED]")
        .replace("C:\\", "[ABSOLUTE_PATH]")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_checksum_and_revision_are_required() {
        let snapshot = AutomationSnapshotV1::new("r", "d", 1, 1, 2, "{}", "p", "a", "prov");
        assert!(snapshot.validate(1, Some(1)).is_ok());
        assert_eq!(
            snapshot.validate(2, None),
            Err(SnapshotError::StaleDefinition)
        );
    }
    #[test]
    fn replay_is_deterministic_and_simulation_denies_host_effects() {
        let input = ReplayInputV1 {
            schema_version: 1,
            definition_revision: 1,
            ordered_events: vec!["a".into()],
            normalized_inputs: "{}".into(),
            frozen_clock_ms: 10,
            rng_seed: 2,
            provider_fixture_ids: vec!["fixture".into()],
            capability_snapshot: "c".into(),
            policy_snapshot: "p".into(),
        };
        assert_eq!(replay_hash(&input).unwrap(), replay_hash(&input).unwrap());
        assert!(!allow_simulation_effect(SimulationEffect::Network));
        assert!(allow_simulation_effect(SimulationEffect::FakeProvider));
        assert!(redact_export("Bearer secret C:\\temp\\a").contains("REDACTED"));
    }
}
